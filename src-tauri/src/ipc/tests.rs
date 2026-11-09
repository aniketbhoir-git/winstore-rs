//! Handler tests: mock catalog source, scripted executor (no winget, no network).

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::{mpsc, oneshot};

use super::commands::*;
use super::events::testing::RecordingSink;
use super::events::{
    EVENT_OPERATION_FINISHED, EVENT_PROGRESS, EVENT_SEARCH_FAILED, EVENT_SEARCH_RESULTS,
    EVENT_UPDATES_SUMMARY,
};
use super::state::{AppState, Core, ExecutorFactory};
use crate::catalog::{BoxFuture, Catalog, CatalogSource, SearchQuery};
use crate::config::AppConfig;
use crate::db::{self, repo::installs, DbPool};
use crate::domain::package::fixtures;
use crate::domain::{
    InstallState, Operation, OperationStage, PackageDetail, PackageId, PackageSummary,
    ProgressEvent, SourceCapabilities, SourceKind, UpdateAvailable,
};
use crate::error::{AppError, AppResult};
use crate::executor::{
    ExecFuture, InstallRequest, InstallScope, InstalledPackage, OperationHandle, PackageExecutor,
};

const KIND: SourceKind = SourceKind::WingetCommunity;
const PKG: &str = "winget_community/Foo.Bar";

struct FakeSource {
    paid: bool,
    down: bool,
}

impl CatalogSource for FakeSource {
    fn kind(&self) -> SourceKind {
        KIND
    }
    fn capabilities(&self) -> SourceCapabilities {
        SourceCapabilities {
            search: true,
            detail: true,
            ..SourceCapabilities::default()
        }
    }
    fn search<'a>(&'a self, _q: &'a SearchQuery) -> BoxFuture<'a, AppResult<Vec<PackageSummary>>> {
        let r = if self.down {
            Err(AppError::SourceUnavailable("down".into()))
        } else {
            Ok(vec![fixtures::summary(KIND, "Foo.Bar", "Foo Bar")])
        };
        Box::pin(async move { r })
    }
    fn get_detail<'a>(&'a self, id: &'a str) -> BoxFuture<'a, AppResult<PackageDetail>> {
        let mut d = fixtures::detail(KIND, id, "Foo Bar");
        d.summary.is_paid = self.paid;
        Box::pin(async move { Ok(d) })
    }
}

type Script = (Vec<OperationStage>, AppResult<()>);

/// Executor that replays scripted operations; no processes involved.
#[derive(Default)]
struct FakeExec {
    script: Mutex<VecDeque<Script>>,
    updates: Vec<UpdateAvailable>,
    cancelled: Mutex<Vec<String>>,
}

impl FakeExec {
    fn scripted(items: Vec<Script>) -> Self {
        FakeExec {
            script: Mutex::new(items.into()),
            ..FakeExec::default()
        }
    }

    fn handle(&self, id: &PackageId, op: Operation) -> OperationHandle {
        let (stages, result) = self.script.lock().unwrap().pop_front().expect("script");
        let (tx, events) = mpsc::channel(16);
        let (otx, outcome) = oneshot::channel();
        for (i, st) in stages.iter().enumerate() {
            let f = (i + 1) as f32 / stages.len() as f32;
            tx.try_send(ProgressEvent::new("op-1", id.clone(), op, *st, f, "m"))
                .unwrap();
        }
        drop(tx);
        let _ = otx.send(result);
        OperationHandle {
            operation_id: "op-1".into(),
            events,
            outcome,
        }
    }
}

impl PackageExecutor for FakeExec {
    fn install<'a>(
        &'a self,
        p: &'a PackageId,
        _r: &'a InstallRequest,
    ) -> ExecFuture<'a, OperationHandle> {
        Box::pin(async move { Ok(self.handle(p, Operation::Install)) })
    }
    fn cancel<'a>(&'a self, op: &'a str) -> ExecFuture<'a, ()> {
        self.cancelled.lock().unwrap().push(op.to_string());
        Box::pin(async { Ok(()) })
    }
    fn remove<'a>(&'a self, p: &'a PackageId) -> ExecFuture<'a, OperationHandle> {
        Box::pin(async move { Ok(self.handle(p, Operation::Remove)) })
    }
    fn list_installed<'a>(&'a self) -> ExecFuture<'a, Vec<InstalledPackage>> {
        Box::pin(async { Ok(vec![]) })
    }
    fn list_updates<'a>(&'a self) -> ExecFuture<'a, Vec<UpdateAvailable>> {
        let u = self.updates.clone();
        Box::pin(async move { Ok(u) })
    }
    fn update<'a>(&'a self, p: &'a PackageId) -> ExecFuture<'a, OperationHandle> {
        Box::pin(async move { Ok(self.handle(p, Operation::Update)) })
    }
    fn pin<'a>(&'a self, _p: &'a PackageId) -> ExecFuture<'a, ()> {
        Box::pin(async { Ok(()) })
    }
    fn unpin<'a>(&'a self, _p: &'a PackageId) -> ExecFuture<'a, ()> {
        Box::pin(async { Ok(()) })
    }
    fn is_available<'a>(
        &'a self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = bool> + Send + 'a>> {
        Box::pin(async { true })
    }
}

async fn state_with(paid: bool, down: bool, factory: ExecutorFactory) -> (AppState, DbPool) {
    let pool = db::init_memory_pool().await.unwrap();
    db::run_migrations(&pool).await.unwrap();
    let catalog = Catalog::new(
        vec![Arc::new(FakeSource { paid, down })],
        Some(pool.clone()),
        3600,
    );
    let core = Core::new(pool.clone(), catalog, AppConfig::default(), factory);
    (AppState::with_core(core), pool)
}

fn exec_factory(exec: FakeExec) -> ExecutorFactory {
    shared_exec_factory(Arc::new(exec))
}

/// Keeps a handle on the fake so a test can inspect what the executor received.
fn shared_exec_factory(exec: Arc<FakeExec>) -> ExecutorFactory {
    let exec: Arc<dyn PackageExecutor> = exec;
    Arc::new(move || {
        let e = Arc::clone(&exec);
        Box::pin(async move { Ok(e) })
    })
}

fn id() -> PackageId {
    PackageId::new(KIND, "Foo.Bar")
}

async fn wait_for(sink: &RecordingSink, event: &str) -> serde_json::Value {
    for _ in 0..300 {
        if let Some(v) = sink.named(event).into_iter().next() {
            return v;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("event {event} never arrived");
}

#[tokio::test]
async fn search_returns_and_streams_results() {
    let (state, _) = state_with(false, false, exec_factory(FakeExec::default())).await;
    let sink = RecordingSink::default();
    let res = run_search(&state, &sink, "s1".into(), SearchQuery::new("foo"))
        .await
        .unwrap();
    assert_eq!(res.len(), 1);
    let ev = sink.named(EVENT_SEARCH_RESULTS);
    assert_eq!(ev.len(), 1);
    assert_eq!(ev[0]["search_id"], "s1");
    assert_eq!(ev[0]["results"][0]["name"], "Foo Bar");
}

#[tokio::test]
async fn search_failure_is_returned_and_streamed_with_code() {
    let (state, _) = state_with(false, true, exec_factory(FakeExec::default())).await;
    let sink = RecordingSink::default();
    let err = run_search(&state, &sink, "s2".into(), SearchQuery::new("foo"))
        .await
        .unwrap_err();
    assert_eq!(err.code(), "source_unavailable");
    let ev = sink.named(EVENT_SEARCH_FAILED);
    assert_eq!(ev[0]["search_id"], "s2");
    assert_eq!(ev[0]["error"]["code"], "source_unavailable");
    assert_eq!(ev[0]["error"]["retryable"], true);
    assert!(sink.named(EVENT_SEARCH_RESULTS).is_empty());
}

#[tokio::test]
async fn install_streams_progress_and_marks_installed() {
    use OperationStage::*;
    let exec = FakeExec::scripted(vec![(
        vec![Resolving, Downloading, Installing, Done],
        Ok(()),
    )]);
    let (state, pool) = state_with(false, false, exec_factory(exec)).await;
    let sink = Arc::new(RecordingSink::default());
    let started = run_start_install(&state, sink.clone(), PKG, InstallOptions::default())
        .await
        .unwrap();
    assert_eq!(started.operation_id, "op-1");
    let fin = wait_for(&sink, EVENT_OPERATION_FINISHED).await;
    assert_eq!(fin["ok"], true);
    assert_eq!(fin["package_id"], PKG);

    let stages: Vec<_> = sink
        .named(EVENT_PROGRESS)
        .iter()
        .map(|e| e["stage"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(stages, ["resolving", "downloading", "installing", "done"]);

    let rec = installs::get(&pool, &id()).await.unwrap().unwrap();
    assert_eq!(rec.state, InstallState::Installed);
    assert_eq!(rec.installed_version.as_deref(), Some("1.2.3"));
    assert_eq!(rec.target_version, None);
}

#[tokio::test]
async fn failed_install_is_recorded_with_typed_error() {
    let exec = FakeExec::scripted(vec![(
        vec![OperationStage::Failed],
        Err(AppError::WingetFailed {
            code: 1,
            stderr: "boom".into(),
        }),
    )]);
    let (state, pool) = state_with(false, false, exec_factory(exec)).await;
    let sink = Arc::new(RecordingSink::default());
    run_start_install(&state, sink.clone(), PKG, InstallOptions::default())
        .await
        .unwrap();
    let fin = wait_for(&sink, EVENT_OPERATION_FINISHED).await;
    assert_eq!(fin["ok"], false);
    assert_eq!(fin["error"]["code"], "winget_failed");
    assert_eq!(fin["error"]["detail"], "boom");
    let rec = installs::get(&pool, &id()).await.unwrap().unwrap();
    assert_eq!(rec.state, InstallState::Failed);
}

#[tokio::test]
async fn cancelled_install_returns_to_not_installed() {
    let exec = FakeExec::scripted(vec![(
        vec![OperationStage::Cancelled],
        Err(AppError::Cancelled),
    )]);
    let (state, pool) = state_with(false, false, exec_factory(exec)).await;
    let sink = Arc::new(RecordingSink::default());
    run_start_install(&state, sink.clone(), PKG, InstallOptions::default())
        .await
        .unwrap();
    let fin = wait_for(&sink, EVENT_OPERATION_FINISHED).await;
    assert_eq!(fin["error"]["code"], "cancelled");
    let rec = installs::get(&pool, &id()).await.unwrap().unwrap();
    assert_eq!(rec.state, InstallState::NotInstalled);
}

#[tokio::test]
async fn paid_items_have_no_install_path() {
    // Empty script: any install attempt would panic.
    let (state, pool) = state_with(true, false, exec_factory(FakeExec::default())).await;
    let err = run_start_install(
        &state,
        Arc::new(RecordingSink::default()),
        PKG,
        InstallOptions::default(),
    )
    .await
    .unwrap_err();
    assert_eq!(err.code(), "unsupported");
    assert!(installs::get(&pool, &id()).await.unwrap().is_none());
}

#[tokio::test]
async fn missing_winget_is_reported_and_not_cached() {
    let calls = Arc::new(AtomicU32::new(0));
    let c = Arc::clone(&calls);
    let factory: ExecutorFactory = Arc::new(move || {
        c.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Err(AppError::WingetNotFound) })
    });
    let (state, pool) = state_with(false, false, factory).await;
    for _ in 0..2 {
        let err = run_start_install(
            &state,
            Arc::new(RecordingSink::default()),
            PKG,
            InstallOptions::default(),
        )
        .await
        .unwrap_err();
        assert_eq!(err.code(), "winget_not_found");
    }
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert!(installs::get(&pool, &id()).await.unwrap().is_none());
}

#[tokio::test]
async fn update_all_runs_each_update_and_summarises() {
    let mk = |n: &str| UpdateAvailable {
        package_id: PackageId::new(KIND, n),
        installed_version: "1.0.0".into(),
        latest_version: "2.0.0".into(),
        source: KIND,
        is_auto: false,
    };
    let mut exec = FakeExec::scripted(vec![
        (vec![OperationStage::Done], Ok(())),
        (
            vec![OperationStage::Failed],
            Err(AppError::DeploymentFailed("x".into())),
        ),
    ]);
    exec.updates = vec![mk("A.A"), mk("B.B")];
    let (state, pool) = state_with(false, false, exec_factory(exec)).await;
    let sink = Arc::new(RecordingSink::default());
    let summary = run_update_all(&state, sink.clone()).await.unwrap();
    assert_eq!(
        (summary.total, summary.succeeded, summary.failed),
        (2, 1, 1)
    );
    assert_eq!(sink.named(EVENT_UPDATES_SUMMARY).len(), 1);
    let a = installs::get(&pool, &PackageId::new(KIND, "A.A"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(a.installed_version.as_deref(), Some("2.0.0"));
    assert_eq!(a.state, InstallState::Installed);
    // A failed update keeps the version that was already installed.
    let b = installs::get(&pool, &PackageId::new(KIND, "B.B"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(b.state, InstallState::Installed);
    assert_eq!(b.installed_version.as_deref(), Some("1.0.0"));
}

#[tokio::test]
async fn update_all_continues_when_the_ledger_refuses_one_package() {
    let mk = |n: &str| UpdateAvailable {
        package_id: PackageId::new(KIND, n),
        installed_version: "1.0.0".into(),
        latest_version: "2.0.0".into(),
        source: KIND,
        is_auto: false,
    };
    let mut exec = FakeExec::scripted(vec![
        (vec![OperationStage::Cancelled], Err(AppError::Cancelled)),
        (vec![OperationStage::Done], Ok(())),
    ]);
    exec.updates = vec![mk("A.A"), mk("B.B")];
    let exec = Arc::new(exec);
    let (state, pool) = state_with(false, false, shared_exec_factory(exec.clone())).await;
    // A pinned row may not move to Installing (SPEC §4.1 state machine).
    let pinned = PackageId::new(KIND, "A.A");
    let mut rec = crate::domain::InstallRecord::new(pinned.clone(), KIND);
    rec.state = InstallState::Pinned;
    rec.pinned = true;
    rec.installed_version = Some("1.0.0".into());
    installs::upsert(&pool, &rec).await.unwrap();

    let sink = Arc::new(RecordingSink::default());
    let summary = run_update_all(&state, sink.clone()).await.unwrap();
    assert_eq!(
        (summary.total, summary.succeeded, summary.failed),
        (2, 1, 1)
    );
    // The refused update was cancelled, the pinned row is untouched, B.B still went through.
    assert_eq!(exec.cancelled.lock().unwrap().as_slice(), ["op-1"]);
    let a = installs::get(&pool, &pinned).await.unwrap().unwrap();
    assert_eq!(a.state, InstallState::Pinned);
    assert_eq!(a.installed_version.as_deref(), Some("1.0.0"));
    let b = installs::get(&pool, &PackageId::new(KIND, "B.B"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(b.state, InstallState::Installed);
    assert_eq!(b.installed_version.as_deref(), Some("2.0.0"));
}

#[tokio::test]
async fn cancelled_reinstall_keeps_the_installed_version() {
    let exec = FakeExec::scripted(vec![(
        vec![OperationStage::Cancelled],
        Err(AppError::Cancelled),
    )]);
    let (state, pool) = state_with(false, false, exec_factory(exec)).await;
    let mut rec = crate::domain::InstallRecord::new(id(), KIND);
    rec.state = InstallState::Installed;
    rec.installed_version = Some("1.0.0".into());
    installs::upsert(&pool, &rec).await.unwrap();

    let sink = Arc::new(RecordingSink::default());
    run_start_install(&state, sink.clone(), PKG, InstallOptions::default())
        .await
        .unwrap();
    let fin = wait_for(&sink, EVENT_OPERATION_FINISHED).await;
    assert_eq!(fin["error"]["code"], "cancelled");
    let rec = installs::get(&pool, &id()).await.unwrap().unwrap();
    assert_eq!(rec.state, InstallState::Installed);
    assert_eq!(rec.installed_version.as_deref(), Some("1.0.0"));
}

#[tokio::test]
async fn check_updates_and_cancel_go_through_the_executor() {
    let exec = Arc::new(FakeExec {
        updates: vec![UpdateAvailable {
            package_id: id(),
            installed_version: "1".into(),
            latest_version: "2".into(),
            source: KIND,
            is_auto: false,
        }],
        ..FakeExec::default()
    });
    let (state, _) = state_with(false, false, shared_exec_factory(exec.clone())).await;
    assert_eq!(run_check_updates(&state).await.unwrap().len(), 1);
    run_cancel(&state, "op-9").await.unwrap();
    assert_eq!(exec.cancelled.lock().unwrap().as_slice(), ["op-9"]);
}

#[test]
fn store_page_url_accepts_product_ids_only() {
    assert_eq!(
        store_page_url("9NBLGGH4NNS1").unwrap(),
        "https://apps.microsoft.com/detail/9NBLGGH4NNS1"
    );
    let long = "A".repeat(33);
    for bad in ["", "a b", "x/../y", "a&b", "a?b=c", "https://evil", &long] {
        assert_eq!(store_page_url(bad).unwrap_err().code(), "invalid_config");
    }
}

#[test]
fn start_server_is_unsupported() {
    assert_eq!(start_server(8080).unwrap_err().code(), "unsupported");
}

#[test]
fn install_options_parse_scope_and_version() {
    let r = InstallOptions {
        version: Some(" ".into()),
        scope: Some("machine".into()),
    }
    .into_request()
    .unwrap();
    assert_eq!(r.scope, Some(InstallScope::Machine));
    assert_eq!(r.version, None);
    let e = InstallOptions {
        version: None,
        scope: Some("root".into()),
    }
    .into_request()
    .unwrap_err();
    assert_eq!(e.code(), "invalid_config");
}
