//! Lifecycle tests: scripted / held fake executor, in-memory ledger. No winget, no network.

use std::sync::Arc;
use std::time::Duration;

use super::*;
use crate::db::repo::{installs, operation_log};
use crate::domain::{InstallRecord, InstallState, OperationStage, SourceKind};
use crate::executor::InstallRequest;
use crate::ipc::events::testing::RecordingSink;
use crate::ipc::events::EVENT_OPERATION_FINISHED;
use crate::ipc::state::AppState;
use crate::ipc::test_support::*;

async fn setup(exec: Arc<FakeExec>) -> (Arc<Core>, DbPool, Arc<RecordingSink>) {
    let (state, pool): (AppState, DbPool) =
        state_with(false, false, shared_exec_factory(exec)).await;
    let core = state.core().await.unwrap();
    (core, pool, Arc::new(RecordingSink::default()))
}

/// Waits until `n` finish events have arrived and returns the last one.
async fn finished(sink: &RecordingSink, n: usize) -> serde_json::Value {
    for _ in 0..300 {
        let evs = sink.named(EVENT_OPERATION_FINISHED);
        if evs.len() >= n {
            return evs[n - 1].clone();
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("finish event {n} never arrived");
}

async fn ledger(pool: &DbPool) -> InstallRecord {
    installs::get(pool, &id()).await.unwrap().unwrap()
}

async fn seed(pool: &DbPool, state: InstallState, version: Option<&str>, pinned: bool) {
    let mut rec = InstallRecord::new(id(), KIND);
    rec.state = state;
    rec.installed_version = version.map(str::to_string);
    rec.pinned = pinned;
    installs::upsert(pool, &rec).await.unwrap();
}

fn sink_of(s: &Arc<RecordingSink>) -> SharedSink {
    s.clone()
}

#[tokio::test]
async fn status_of_an_unknown_package_is_not_installed() {
    let (core, _, _) = setup(Arc::new(FakeExec::default())).await;
    let st = status(&core, &id()).await.unwrap();
    assert_eq!(st.state, InstallState::NotInstalled);
    assert_eq!(st.package_id, id());
    assert!(st.active.is_none() && st.installed_version.is_none());
}

#[tokio::test]
async fn running_install_is_reported_then_cancel_cleans_up() {
    let exec = Arc::new(FakeExec::holding());
    let (core, pool, sink) = setup(exec.clone()).await;
    let started = start_install(&core, sink_of(&sink), id(), InstallRequest::default())
        .await
        .unwrap();

    let st = status(&core, &id()).await.unwrap();
    assert_eq!(st.state, InstallState::Installing);
    assert_eq!(st.target_version.as_deref(), Some("1.2.3"));
    assert_eq!(
        st.active,
        Some(ActiveOperation {
            operation_id: started.operation_id.clone(),
            operation: Operation::Install,
        })
    );

    cancel(&core, &started.operation_id).await.unwrap();
    let fin = finished(&sink, 1).await;
    assert_eq!(fin["error"]["code"], "cancelled");

    let rec = ledger(&pool).await;
    assert_eq!(rec.state, InstallState::NotInstalled);
    assert_eq!(rec.target_version, None);
    assert_eq!(rec.installed_version, None);
    assert!(status(&core, &id()).await.unwrap().active.is_none());
}

#[tokio::test]
async fn second_operation_is_refused_while_one_runs() {
    let exec = Arc::new(FakeExec::holding());
    let (core, pool, sink) = setup(exec.clone()).await;
    let started = start_install(&core, sink_of(&sink), id(), InstallRequest::default())
        .await
        .unwrap();
    let err = start_install(&core, sink_of(&sink), id(), InstallRequest::default())
        .await
        .unwrap_err();
    assert_eq!(err.code(), "unsupported");
    let err = start_uninstall(&core, sink_of(&sink), id())
        .await
        .unwrap_err();
    assert_eq!(err.code(), "unsupported");
    // Only one executor operation was ever started.
    assert_eq!(exec.next.load(std::sync::atomic::Ordering::SeqCst), 1);
    cancel(&core, &started.operation_id).await.unwrap();
    finished(&sink, 1).await;
    assert_eq!(ledger(&pool).await.state, InstallState::NotInstalled);
}

#[tokio::test]
async fn install_uninstall_cancel_loop_repeats() {
    use OperationStage::*;
    let exec = Arc::new(FakeExec::default());
    let (core, pool, sink) = setup(exec.clone()).await;
    let mut n = 0;
    for _ in 0..3 {
        // Cancelled install.
        exec.set_hold(true);
        let op = start_install(&core, sink_of(&sink), id(), InstallRequest::default())
            .await
            .unwrap();
        cancel(&core, &op.operation_id).await.unwrap();
        n += 1;
        assert_eq!(finished(&sink, n).await["error"]["code"], "cancelled");
        assert_eq!(ledger(&pool).await.state, InstallState::NotInstalled);

        // Install.
        exec.set_hold(false);
        exec.push((vec![Downloading, Installing, Done], Ok(())));
        start_install(&core, sink_of(&sink), id(), InstallRequest::default())
            .await
            .unwrap();
        n += 1;
        assert_eq!(finished(&sink, n).await["ok"], true);
        let rec = ledger(&pool).await;
        assert_eq!(rec.state, InstallState::Installed);
        assert_eq!(rec.installed_version.as_deref(), Some("1.2.3"));

        // Cancelled uninstall keeps the app.
        exec.set_hold(true);
        let op = start_uninstall(&core, sink_of(&sink), id()).await.unwrap();
        assert_eq!(
            status(&core, &id()).await.unwrap().state,
            InstallState::Uninstalling
        );
        cancel(&core, &op.operation_id).await.unwrap();
        n += 1;
        assert_eq!(finished(&sink, n).await["error"]["code"], "cancelled");
        let rec = ledger(&pool).await;
        assert_eq!(rec.state, InstallState::Installed);
        assert_eq!(rec.installed_version.as_deref(), Some("1.2.3"));

        // Uninstall.
        exec.set_hold(false);
        exec.push((vec![Installing, Done], Ok(())));
        start_uninstall(&core, sink_of(&sink), id()).await.unwrap();
        n += 1;
        assert_eq!(finished(&sink, n).await["ok"], true);
        let rec = ledger(&pool).await;
        assert_eq!(rec.state, InstallState::NotInstalled);
        assert_eq!(rec.installed_version, None);
        assert_eq!(rec.target_version, None);
        assert!(rec.last_updated.is_some());
        assert!(status(&core, &id()).await.unwrap().active.is_none());
    }
}

#[tokio::test]
async fn install_with_a_chosen_version_records_it() {
    let exec = Arc::new(FakeExec::scripted(vec![(
        vec![OperationStage::Done],
        Ok(()),
    )]));
    let (core, pool, sink) = setup(exec).await;
    let req = InstallRequest {
        version: Some("1.0.0".into()),
        scope: None,
    };
    start_install(&core, sink_of(&sink), id(), req)
        .await
        .unwrap();
    finished(&sink, 1).await;
    assert_eq!(
        ledger(&pool).await.installed_version.as_deref(),
        Some("1.0.0")
    );
}

#[tokio::test]
async fn uninstall_needs_a_ledger_row() {
    // Empty script: any executor call would panic.
    let (core, pool, sink) = setup(Arc::new(FakeExec::default())).await;
    let err = start_uninstall(&core, sink_of(&sink), id())
        .await
        .unwrap_err();
    assert_eq!(err.code(), "unsupported");
    assert!(installs::get(&pool, &id()).await.unwrap().is_none());

    // A row that cannot move to Uninstalling is refused the same way.
    seed(&pool, InstallState::NotInstalled, None, false).await;
    let err = start_uninstall(&core, sink_of(&sink), id())
        .await
        .unwrap_err();
    assert_eq!(err.code(), "unsupported");
    assert_eq!(ledger(&pool).await.state, InstallState::NotInstalled);
}

#[tokio::test]
async fn failed_uninstall_keeps_the_version_and_can_be_retried() {
    use OperationStage::*;
    let exec = Arc::new(FakeExec::scripted(vec![
        (
            vec![Failed],
            Err(AppError::WingetFailed {
                code: 5,
                stderr: "in use".into(),
            }),
        ),
        (vec![Done], Ok(())),
    ]));
    let (core, pool, sink) = setup(exec).await;
    seed(&pool, InstallState::Installed, Some("2.0"), false).await;

    start_uninstall(&core, sink_of(&sink), id()).await.unwrap();
    let fin = finished(&sink, 1).await;
    assert_eq!(fin["error"]["code"], "winget_failed");
    let rec = ledger(&pool).await;
    assert_eq!(rec.state, InstallState::Failed);
    assert_eq!(rec.installed_version.as_deref(), Some("2.0"));

    start_uninstall(&core, sink_of(&sink), id()).await.unwrap();
    assert_eq!(finished(&sink, 2).await["ok"], true);
    assert_eq!(ledger(&pool).await.state, InstallState::NotInstalled);
}

#[tokio::test]
async fn cancelled_uninstall_of_a_pinned_app_stays_pinned() {
    let exec = Arc::new(FakeExec::holding());
    let (core, pool, sink) = setup(exec).await;
    seed(&pool, InstallState::Pinned, Some("2.0"), true).await;
    let op = start_uninstall(&core, sink_of(&sink), id()).await.unwrap();
    cancel(&core, &op.operation_id).await.unwrap();
    finished(&sink, 1).await;
    let rec = ledger(&pool).await;
    assert_eq!(rec.state, InstallState::Pinned);
    assert!(rec.pinned);
}

#[tokio::test]
async fn successful_uninstall_clears_the_pin() {
    let exec = Arc::new(FakeExec::scripted(vec![(
        vec![OperationStage::Done],
        Ok(()),
    )]));
    let (core, pool, sink) = setup(exec).await;
    seed(&pool, InstallState::Pinned, Some("2.0"), true).await;
    start_uninstall(&core, sink_of(&sink), id()).await.unwrap();
    finished(&sink, 1).await;
    let rec = ledger(&pool).await;
    assert_eq!(rec.state, InstallState::NotInstalled);
    assert!(!rec.pinned);
}

#[tokio::test]
async fn failed_install_from_failed_can_be_retried() {
    use OperationStage::*;
    let exec = Arc::new(FakeExec::scripted(vec![
        (vec![Failed], Err(AppError::DeploymentFailed("x".into()))),
        (vec![Done], Ok(())),
    ]));
    let (core, pool, sink) = setup(exec).await;
    start_install(&core, sink_of(&sink), id(), InstallRequest::default())
        .await
        .unwrap();
    finished(&sink, 1).await;
    let rec = ledger(&pool).await;
    assert_eq!(rec.state, InstallState::Failed);
    assert_eq!(rec.target_version, None);
    start_install(&core, sink_of(&sink), id(), InstallRequest::default())
        .await
        .unwrap();
    finished(&sink, 2).await;
    assert_eq!(ledger(&pool).await.state, InstallState::Installed);
}

#[tokio::test]
async fn recover_interrupted_settles_busy_rows_once() {
    let pool = crate::db::repo::test_pool().await;
    let mk = |pkg: &str, state, version: Option<&str>| {
        let mut r = InstallRecord::new(PackageId::new(KIND, pkg), KIND);
        r.state = state;
        r.installed_version = version.map(str::to_string);
        r.target_version = Some("9.9".into());
        r
    };
    let rows = [
        mk("Fresh.Install", InstallState::Installing, None),
        mk("Re.Install", InstallState::Installing, Some("1.0")),
        mk("Half.Removed", InstallState::Uninstalling, Some("1.0")),
        mk("Calm.App", InstallState::Installed, Some("1.0")),
    ];
    for r in &rows {
        installs::upsert(&pool, r).await.unwrap();
    }
    assert_eq!(recover_interrupted(&pool).await.unwrap(), 3);
    let get = |pkg: &'static str| {
        let pool = pool.clone();
        async move {
            installs::get(&pool, &PackageId::new(KIND, pkg))
                .await
                .unwrap()
                .unwrap()
        }
    };
    let fresh = get("Fresh.Install").await;
    assert_eq!(fresh.state, InstallState::Failed);
    assert_eq!(fresh.target_version, None);
    let re = get("Re.Install").await;
    assert_eq!(re.state, InstallState::Installed);
    assert_eq!(re.installed_version.as_deref(), Some("1.0"));
    assert_eq!(get("Half.Removed").await.state, InstallState::Failed);
    let calm = get("Calm.App").await;
    assert_eq!(calm.state, InstallState::Installed);
    assert_eq!(calm.target_version.as_deref(), Some("9.9"));

    let logs = operation_log::recent(&pool, 10).await.unwrap();
    assert_eq!(logs.len(), 3);
    assert!(logs.iter().all(|l| l.level == LogLevel::Warn));
    assert_eq!(recover_interrupted(&pool).await.unwrap(), 0);
}

#[tokio::test]
async fn log_tail_is_oldest_first_and_bounded() {
    let pool = crate::db::repo::test_pool().await;
    let other = PackageId::new(SourceKind::WingetCommunity, "Other.App");
    for (i, pkg) in [id(), other.clone(), id(), id()].iter().enumerate() {
        let entry = OperationLogEntry {
            id: None,
            ts: 100 + i as i64,
            level: LogLevel::Info,
            operation: Some(Operation::Install),
            package_id: Some(pkg.clone()),
            message: format!("m{i}"),
            detail: None,
        };
        operation_log::append(&pool, &entry).await.unwrap();
    }
    let tail = log_tail(&pool, &id(), 2).await.unwrap();
    let msgs: Vec<_> = tail.iter().map(|e| e.message.as_str()).collect();
    assert_eq!(msgs, ["m2", "m3"]);
    // Zero is raised to one; huge limits are capped.
    assert_eq!(log_tail(&pool, &id(), 0).await.unwrap().len(), 1);
    assert_eq!(log_tail(&pool, &id(), u32::MAX).await.unwrap().len(), 3);
}

// ---------------------------------------------------------------------------------------------
// SPEC 6.7 "Start" and "End of an operation" rows, and the agreement stop (SPEC 6.6 case c)
// ---------------------------------------------------------------------------------------------

fn agreements(source: bool, version: Option<&str>) -> AppError {
    AppError::AgreementsRequired {
        source_agreements: source,
        package_agreements: !source,
        winget_source: Some("winget".into()),
        version: version.map(str::to_string),
        agreement_text: None,
        agreement_url: None,
    }
}

fn agreements_script() -> Vec<Script> {
    vec![(vec![OperationStage::Failed], Err(agreements(false, None)))]
}

#[tokio::test]
async fn agreements_stop_of_an_install_leaves_the_rest_state() {
    // (start row, version, expected state)
    let cases = [
        (None, None, InstallState::NotInstalled),
        (
            Some(InstallState::Installed),
            Some("2.0"),
            InstallState::Installed,
        ),
        (Some(InstallState::Failed), None, InstallState::NotInstalled),
    ];
    for (start, version, state) in cases {
        let exec = Arc::new(FakeExec::scripted(agreements_script()));
        let (core, pool, sink) = setup(exec).await;
        if let Some(s) = start {
            seed(&pool, s, version, false).await;
        }
        start_install(&core, sink_of(&sink), id(), InstallRequest::default())
            .await
            .unwrap();
        let fin = finished(&sink, 1).await;
        assert_eq!(fin["ok"], false);
        assert_eq!(fin["error"]["code"], "agreements_required");
        let rec = ledger(&pool).await;
        assert_eq!(rec.state, state, "{start:?}");
        assert_eq!(rec.installed_version.as_deref(), version);
        assert_eq!(rec.target_version, None);
    }
}

#[tokio::test]
async fn agreements_stop_of_an_uninstall_leaves_the_rest_state() {
    let cases = [
        (
            InstallState::Installed,
            Some("2.0"),
            false,
            InstallState::Installed,
        ),
        (
            InstallState::Pinned,
            Some("2.0"),
            true,
            InstallState::Pinned,
        ),
        (
            InstallState::Failed,
            None,
            false,
            InstallState::NotInstalled,
        ),
    ];
    for (start, version, pinned, state) in cases {
        let exec = Arc::new(FakeExec::scripted(agreements_script()));
        let (core, pool, sink) = setup(exec).await;
        seed(&pool, start, version, pinned).await;
        start_uninstall(&core, sink_of(&sink), id()).await.unwrap();
        let fin = finished(&sink, 1).await;
        assert_eq!(fin["error"]["code"], "agreements_required");
        let rec = ledger(&pool).await;
        assert_eq!(rec.state, state, "{start}");
        assert_eq!(rec.installed_version.as_deref(), version);
        assert_eq!(rec.pinned, pinned);
        assert_eq!(rec.target_version, None);
    }
}

#[tokio::test]
async fn agreements_error_gets_the_target_as_version_when_it_has_none() {
    // Install without a version: the latest version of the detail.
    let exec = Arc::new(FakeExec::scripted(agreements_script()));
    let (core, _pool, sink) = setup(exec).await;
    start_install(&core, sink_of(&sink), id(), InstallRequest::default())
        .await
        .unwrap();
    assert_eq!(finished(&sink, 1).await["error"]["version"], "1.2.3");

    // Install with a chosen version.
    let exec = Arc::new(FakeExec::scripted(agreements_script()));
    let (core, _pool, sink) = setup(exec).await;
    let req = InstallRequest {
        version: Some("1.0.0".into()),
        scope: None,
    };
    start_install(&core, sink_of(&sink), id(), req)
        .await
        .unwrap();
    assert_eq!(finished(&sink, 1).await["error"]["version"], "1.0.0");

    // A version that the executor already set stays; an uninstall has no target.
    let exec = Arc::new(FakeExec::scripted(vec![
        (
            vec![OperationStage::Failed],
            Err(agreements(true, Some("0.9"))),
        ),
        (vec![OperationStage::Failed], Err(agreements(true, None))),
    ]));
    let (core, pool, sink) = setup(exec).await;
    start_install(&core, sink_of(&sink), id(), InstallRequest::default())
        .await
        .unwrap();
    assert_eq!(finished(&sink, 1).await["error"]["version"], "0.9");
    seed(&pool, InstallState::Installed, Some("1.0"), false).await;
    start_uninstall(&core, sink_of(&sink), id()).await.unwrap();
    let fin = finished(&sink, 2).await;
    assert_eq!(fin["error"]["code"], "agreements_required");
    assert!(fin["error"]["version"].is_null());
}

#[tokio::test]
async fn cancelled_uninstall_goes_to_the_rest_state() {
    for (start, version, state) in [
        (InstallState::Failed, None, InstallState::NotInstalled),
        (
            InstallState::Installed,
            Some("2.0"),
            InstallState::Installed,
        ),
    ] {
        let (core, pool, sink) = setup(Arc::new(FakeExec::holding())).await;
        seed(&pool, start, version, false).await;
        let op = start_uninstall(&core, sink_of(&sink), id()).await.unwrap();
        cancel(&core, &op.operation_id).await.unwrap();
        assert_eq!(finished(&sink, 1).await["error"]["code"], "cancelled");
        let rec = ledger(&pool).await;
        assert_eq!(rec.state, state, "{start}");
        assert_eq!(rec.installed_version.as_deref(), version);
        assert_eq!(rec.target_version, None);
    }
}

// Start table, row 3: `Uninstalling` and `Pinned` refuse `start_install`.
#[tokio::test]
async fn start_install_is_refused_from_uninstalling_and_pinned() {
    for (start, pinned) in [
        (InstallState::Uninstalling, false),
        (InstallState::Pinned, true),
    ] {
        let (core, pool, sink) = setup(Arc::new(FakeExec::default())).await;
        seed(&pool, start, Some("2.0"), pinned).await;
        let err = start_install(&core, sink_of(&sink), id(), InstallRequest::default())
            .await
            .unwrap_err();
        assert_eq!(err.code(), "unsupported", "{start}");
        assert_eq!(ledger(&pool).await.state, start);
    }
}

// Start table, row 4: a leftover `Installing` row with no active operation is accepted.
#[tokio::test]
async fn start_install_accepts_a_leftover_installing_row() {
    let exec = Arc::new(FakeExec::holding());
    let (core, pool, sink) = setup(exec).await;
    seed(&pool, InstallState::Installing, None, false).await;
    let op = start_install(&core, sink_of(&sink), id(), InstallRequest::default())
        .await
        .unwrap();
    let rec = ledger(&pool).await;
    assert_eq!(rec.state, InstallState::Installing);
    assert_eq!(rec.target_version.as_deref(), Some("1.2.3"));
    cancel(&core, &op.operation_id).await.unwrap();
    finished(&sink, 1).await;
}

// Start table, row 5: each listed state moves to `Installing` and sets `target_version`.
#[tokio::test]
async fn start_install_from_each_listed_state_sets_the_target() {
    for start in [
        InstallState::NotInstalled,
        InstallState::Installed,
        InstallState::UpdateAvailable,
        InstallState::Failed,
    ] {
        let (core, pool, sink) = setup(Arc::new(FakeExec::holding())).await;
        seed(&pool, start, Some("1.0"), false).await;
        let op = start_install(&core, sink_of(&sink), id(), InstallRequest::default())
            .await
            .unwrap();
        let rec = ledger(&pool).await;
        assert_eq!(rec.state, InstallState::Installing, "{start}");
        assert_eq!(rec.target_version.as_deref(), Some("1.2.3"));
        cancel(&core, &op.operation_id).await.unwrap();
        finished(&sink, 1).await;
    }
}

// Start table, rows 6 and 7: `start_uninstall`.
#[tokio::test]
async fn start_uninstall_by_ledger_state() {
    for start in [InstallState::Installing, InstallState::Uninstalling] {
        let (core, pool, sink) = setup(Arc::new(FakeExec::default())).await;
        seed(&pool, start, Some("1.0"), false).await;
        let err = start_uninstall(&core, sink_of(&sink), id())
            .await
            .unwrap_err();
        assert_eq!(err.code(), "unsupported", "{start}");
        assert_eq!(ledger(&pool).await.state, start);
    }
    for (start, pinned) in [
        (InstallState::Installed, false),
        (InstallState::UpdateAvailable, false),
        (InstallState::Failed, false),
        (InstallState::Pinned, true),
    ] {
        let (core, pool, sink) = setup(Arc::new(FakeExec::holding())).await;
        seed(&pool, start, Some("1.0"), pinned).await;
        let op = start_uninstall(&core, sink_of(&sink), id()).await.unwrap();
        assert_eq!(ledger(&pool).await.state, InstallState::Uninstalling);
        cancel(&core, &op.operation_id).await.unwrap();
        finished(&sink, 1).await;
    }
}

// Start table, row 8: winget absent refuses either command and changes nothing.
#[tokio::test]
async fn winget_absent_refuses_install_and_uninstall() {
    let factory: crate::ipc::state::ExecutorFactory =
        Arc::new(|| Box::pin(async { Err(AppError::WingetNotFound) }));
    let (state, pool): (AppState, DbPool) = state_with(false, false, factory).await;
    let core = state.core().await.unwrap();
    let sink = Arc::new(RecordingSink::default());
    seed(&pool, InstallState::Installed, Some("1.0"), false).await;
    let e = start_uninstall(&core, sink_of(&sink), id())
        .await
        .unwrap_err();
    assert_eq!(e.code(), "winget_not_found");
    assert_eq!(ledger(&pool).await.state, InstallState::Installed);
    let e = start_install(&core, sink_of(&sink), id(), InstallRequest::default())
        .await
        .unwrap_err();
    assert_eq!(e.code(), "winget_not_found");
    assert_eq!(ledger(&pool).await.state, InstallState::Installed);
}

// End table, row 2: an install that fails while a version is recorded keeps it.
#[tokio::test]
async fn failed_install_with_a_version_keeps_installed() {
    let exec = Arc::new(FakeExec::scripted(vec![(
        vec![OperationStage::Failed],
        Err(AppError::DeploymentFailed("x".into())),
    )]));
    let (core, pool, sink) = setup(exec).await;
    seed(&pool, InstallState::Installed, Some("1.0"), false).await;
    start_install(&core, sink_of(&sink), id(), InstallRequest::default())
        .await
        .unwrap();
    assert_eq!(
        finished(&sink, 1).await["error"]["code"],
        "deployment_failed"
    );
    let rec = ledger(&pool).await;
    assert_eq!(rec.state, InstallState::Installed);
    assert_eq!(rec.installed_version.as_deref(), Some("1.0"));
    assert_eq!(rec.target_version, None);
}
