//! Shared test doubles for handler and service tests: a mock catalog source and a scripted
//! executor (no winget, no network).

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::{mpsc, oneshot};

use super::events::testing::RecordingSink;
use super::state::{AppState, Core, ExecutorFactory};
use crate::catalog::{BoxFuture, Catalog, CatalogSource, SearchQuery};
use crate::config::AppConfig;
use crate::db::{self, DbPool};
use crate::domain::package::fixtures;
use crate::domain::{
    Operation, OperationStage, PackageDetail, PackageId, PackageSummary, ProgressEvent,
    SourceCapabilities, SourceKind, UpdateAvailable,
};
use crate::error::{AppError, AppResult};
use crate::executor::{
    ExecFuture, InstallRequest, InstalledPackage, OperationHandle, PackageExecutor,
};

/// A held operation: its event sender and outcome sender, released by `cancel`.
pub(crate) type Held = (
    String,
    PackageId,
    Operation,
    mpsc::Sender<ProgressEvent>,
    oneshot::Sender<AppResult<()>>,
);

pub(crate) const KIND: SourceKind = SourceKind::WingetCommunity;
pub(crate) const PKG: &str = "winget_community/Foo.Bar";

pub(crate) struct FakeSource {
    pub paid: bool,
    pub down: bool,
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

pub(crate) type Script = (Vec<OperationStage>, AppResult<()>);

/// Executor that replays scripted operations; no processes involved.
#[derive(Default)]
pub(crate) struct FakeExec {
    pub script: Mutex<VecDeque<Script>>,
    pub updates: Vec<UpdateAvailable>,
    pub cancelled: Mutex<Vec<String>>,
    /// When set, operations stay running until `cancel` ends them as cancelled.
    pub hold: AtomicBool,
    pub held: Mutex<Vec<Held>>,
    pub next: AtomicU32,
}

impl FakeExec {
    pub fn scripted(items: Vec<Script>) -> Self {
        FakeExec {
            script: Mutex::new(items.into()),
            ..FakeExec::default()
        }
    }

    /// Operations that stay running until cancelled.
    pub fn holding() -> Self {
        let e = FakeExec::default();
        e.set_hold(true);
        e
    }

    pub fn set_hold(&self, hold: bool) {
        self.hold.store(hold, Ordering::SeqCst);
    }

    /// Queues one scripted (immediately finishing) operation.
    pub fn push(&self, item: Script) {
        self.script.lock().unwrap().push_back(item);
    }

    fn handle(&self, id: &PackageId, op: Operation) -> OperationHandle {
        let op_id = format!("op-{}", self.next.fetch_add(1, Ordering::SeqCst) + 1);
        let (tx, events) = mpsc::channel(16);
        let (otx, outcome) = oneshot::channel();
        if self.hold.load(Ordering::SeqCst) {
            tx.try_send(ProgressEvent::new(
                op_id.as_str(),
                id.clone(),
                op,
                OperationStage::Downloading,
                0.3,
                "downloading",
            ))
            .unwrap();
            self.held
                .lock()
                .unwrap()
                .push((op_id.clone(), id.clone(), op, tx, otx));
        } else {
            let (stages, result) = self.script.lock().unwrap().pop_front().expect("script");
            for (i, st) in stages.iter().enumerate() {
                let f = (i + 1) as f32 / stages.len() as f32;
                tx.try_send(ProgressEvent::new(
                    op_id.as_str(),
                    id.clone(),
                    op,
                    *st,
                    f,
                    "m",
                ))
                .unwrap();
            }
            drop(tx);
            let _ = otx.send(result);
        }
        OperationHandle {
            operation_id: op_id,
            events,
            outcome,
        }
    }

    /// Ends a held operation as cancelled.
    fn release(&self, op_id: &str) {
        let mut held = self.held.lock().unwrap();
        if let Some(pos) = held.iter().position(|h| h.0 == op_id) {
            let (op_id, id, op, tx, otx) = held.remove(pos);
            let _ = tx.try_send(ProgressEvent::new(
                op_id,
                id,
                op,
                OperationStage::Cancelled,
                0.3,
                "cancelled",
            ));
            drop(tx);
            let _ = otx.send(Err(AppError::Cancelled));
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
        self.release(op);
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

pub(crate) async fn state_with(
    paid: bool,
    down: bool,
    factory: ExecutorFactory,
) -> (AppState, DbPool) {
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

pub(crate) fn exec_factory(exec: FakeExec) -> ExecutorFactory {
    shared_exec_factory(Arc::new(exec))
}

/// Keeps a handle on the fake so a test can inspect what the executor received.
pub(crate) fn shared_exec_factory(exec: Arc<FakeExec>) -> ExecutorFactory {
    let exec: Arc<dyn PackageExecutor> = exec;
    Arc::new(move || {
        let e = Arc::clone(&exec);
        Box::pin(async move { Ok(e) })
    })
}

pub(crate) fn id() -> PackageId {
    PackageId::new(KIND, "Foo.Bar")
}

pub(crate) async fn wait_for(sink: &RecordingSink, event: &str) -> serde_json::Value {
    for _ in 0..300 {
        if let Some(v) = sink.named(event).into_iter().next() {
            return v;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("event {event} never arrived");
}
