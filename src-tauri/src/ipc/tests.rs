//! Handler tests: mock catalog source, scripted executor (no winget, no network).

use std::sync::Arc;

use super::commands::*;
use super::events::testing::RecordingSink;
use super::events::{
    EVENT_OPERATION_FINISHED, EVENT_PROGRESS, EVENT_SEARCH_FAILED, EVENT_SEARCH_RESULTS,
    EVENT_UPDATES_SUMMARY,
};
use super::state::ExecutorFactory;
use super::test_support::*;
use crate::catalog::SearchQuery;
use crate::db::repo::installs;
use crate::domain::{InstallState, OperationStage, PackageId, UpdateAvailable};
use crate::error::AppError;
use crate::executor::InstallScope;
use std::sync::atomic::{AtomicU32, Ordering};

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
    assert!(sink
        .named(EVENT_PROGRESS)
        .iter()
        .all(|e| e["cancel_failed"] == false));

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
    let mut exec = FakeExec::scripted(vec![(vec![OperationStage::Done], Ok(()))]);
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
    // The pinned package never reached the executor, its row is untouched, B.B still went through.
    assert_eq!(exec.next.load(Ordering::SeqCst), 1);
    assert!(exec.cancelled.lock().unwrap().is_empty());
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

// The ledger refuses `begin` after the executor already started the update (here the id has
// no known source, so no row can be created). The started process is cancelled and drained,
// the finish event carries the refusal, and the ledger is untouched.
#[tokio::test]
async fn update_all_cancels_and_abandons_when_begin_is_refused() {
    let bogus: PackageId = serde_json::from_str("\"nosuchsource/A.A\"").unwrap();
    let mut exec = FakeExec::holding();
    exec.updates = vec![UpdateAvailable {
        package_id: bogus.clone(),
        installed_version: "1.0.0".into(),
        latest_version: "2.0.0".into(),
        source: KIND,
        is_auto: false,
    }];
    let exec = Arc::new(exec);
    let (state, pool) = state_with(false, false, shared_exec_factory(exec.clone())).await;
    let sink = Arc::new(RecordingSink::default());
    let summary = run_update_all(&state, sink.clone()).await.unwrap();
    assert_eq!(
        (summary.total, summary.succeeded, summary.failed),
        (1, 0, 1)
    );
    // The started operation was cancelled and nothing is left running.
    assert_eq!(exec.cancelled.lock().unwrap().as_slice(), ["op-1"]);
    assert!(exec.held.lock().unwrap().is_empty());
    // The finish event carries the refusal; the ledger has no row.
    let fin = sink.named(EVENT_OPERATION_FINISHED);
    assert_eq!(fin.len(), 1);
    assert_eq!(fin[0]["ok"], false);
    assert_eq!(fin[0]["error"]["code"], "invalid_config");
    assert!(installs::get(&pool, &bogus).await.unwrap().is_none());
    let core = state.core().await.unwrap();
    assert!(core.active.get(&bogus).is_none());
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

// End table, row 4 for an update: a cancelled update keeps the installed version.
#[tokio::test]
async fn cancelled_update_keeps_the_installed_version() {
    let mut exec = FakeExec::scripted(vec![(
        vec![OperationStage::Cancelled],
        Err(AppError::Cancelled),
    )]);
    exec.updates = vec![UpdateAvailable {
        package_id: id(),
        installed_version: "1.0.0".into(),
        latest_version: "2.0.0".into(),
        source: KIND,
        is_auto: false,
    }];
    let (state, pool) = state_with(false, false, exec_factory(exec)).await;
    run_update_all(&state, Arc::new(RecordingSink::default()))
        .await
        .unwrap();
    let rec = installs::get(&pool, &id()).await.unwrap().unwrap();
    assert_eq!(rec.state, InstallState::Installed);
    assert_eq!(rec.installed_version.as_deref(), Some("1.0.0"));
    assert_eq!(rec.target_version, None);
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
        consent: None,
    }
    .into_request()
    .unwrap();
    assert_eq!(r.scope, Some(InstallScope::Machine));
    assert_eq!(r.version, None);
    let e = InstallOptions {
        version: None,
        scope: Some("root".into()),
        consent: None,
    }
    .into_request()
    .unwrap_err();
    assert_eq!(e.code(), "invalid_config");
}

// ---- Shared IPC fixtures (tests/fixtures/ipc); the UI tests (L2) import the same files ----

/// Compares `actual` with the fixture file. `STORE_WRITE_FIXTURES=1` rewrites the file.
fn check_fixture(name: &str, actual: serde_json::Value) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/ipc")
        .join(name);
    if std::env::var("STORE_WRITE_FIXTURES").is_ok_and(|v| v == "1") {
        assert!(
            std::env::var_os("CI").is_none(),
            "STORE_WRITE_FIXTURES=1 is refused when CI is set: CI must compare fixtures, never rewrite them"
        );
        let text = serde_json::to_string_pretty(&actual).unwrap() + "\n";
        std::fs::write(&path, text).unwrap();
    }
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{name}: {e}"));
    let expected: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        actual, expected,
        "fixture {name} differs from the Rust payload"
    );
}

#[tokio::test]
async fn update_all_never_starts_the_executor_for_a_pinned_row() {
    let mut exec = FakeExec::scripted(vec![]);
    exec.updates = vec![UpdateAvailable {
        package_id: id(),
        installed_version: "1.0.0".into(),
        latest_version: "2.0.0".into(),
        source: KIND,
        is_auto: false,
    }];
    let exec = Arc::new(exec);
    let (state, pool) = state_with(false, false, shared_exec_factory(exec.clone())).await;
    let mut rec = crate::domain::InstallRecord::new(id(), KIND);
    rec.state = InstallState::Pinned;
    rec.pinned = true;
    rec.installed_version = Some("1.0.0".into());
    installs::upsert(&pool, &rec).await.unwrap();

    let summary = run_update_all(&state, Arc::new(RecordingSink::default()))
        .await
        .unwrap();
    assert_eq!((summary.total, summary.failed), (1, 1));
    // No operation was ever created (the empty script would have panicked), nothing cancelled.
    assert_eq!(exec.next.load(Ordering::SeqCst), 0);
    assert!(exec.cancelled.lock().unwrap().is_empty());
    assert_eq!(
        installs::get(&pool, &id()).await.unwrap().unwrap().state,
        InstallState::Pinned
    );
}

#[test]
fn fixture_error_agreements_required() {
    let err = AppError::AgreementsRequired {
        source_agreements: false,
        package_agreements: true,
        winget_source: Some("winget".into()),
        version: Some("1.2.3".into()),
        agreement_text: None,
        agreement_url: Some("https://example.org/terms".into()),
        agreements: vec![crate::error::AgreementEntry {
            label: "Terms of use".into(),
            text: None,
            url: Some("https://example.org/terms".into()),
        }],
        consent_token: Some("tok-0123456789abcdef".into()),
    };
    check_fixture(
        "error_agreements_required.json",
        super::events::error_json(&err),
    );
}

#[test]
fn fixture_event_progress() {
    let ev = crate::domain::ProgressEvent {
        operation_id: "op-1".into(),
        package_id: PackageId::new(KIND, "Foo.Bar"),
        operation: crate::domain::Operation::Install,
        stage: OperationStage::Downloading,
        fraction: 0.5,
        message: "Downloading".into(),
        timestamp: 1_700_000_000,
        cancel_failed: false,
        indeterminate: false,
    };
    check_fixture("event_progress.json", serde_json::to_value(&ev).unwrap());
}

#[test]
fn fixture_event_operation_finished() {
    let ev = super::events::OperationFinishedEvent {
        operation_id: "op-1".into(),
        package_id: PackageId::new(KIND, "Foo.Bar"),
        ok: true,
        error: None,
    };
    check_fixture(
        "event_operation_finished.json",
        serde_json::to_value(&ev).unwrap(),
    );
}

#[test]
fn fixture_event_search_results() {
    let ev = super::events::SearchResultsEvent {
        search_id: "s1".into(),
        results: vec![crate::domain::package::fixtures::summary(
            KIND, "Foo.Bar", "Foo Bar",
        )],
    };
    check_fixture(
        "event_search_results.json",
        serde_json::to_value(&ev).unwrap(),
    );
}

#[test]
fn version_strings_are_validated() {
    let opts = |v: &str| InstallOptions {
        version: Some(v.into()),
        scope: None,
        consent: None,
    };
    for ok in ["1.2.3", "2024.1_beta+build-5", "v1"] {
        assert_eq!(
            opts(ok).into_request().unwrap().version.as_deref(),
            Some(ok)
        );
    }
    // Blank means "latest".
    assert_eq!(opts("  ").into_request().unwrap().version, None);
    let long = "1".repeat(65);
    for bad in [
        "--force",
        "-1",
        "1 2",
        "1;2",
        "1
2",
        "1=2",
        "ü",
        long.as_str(),
    ] {
        assert_eq!(
            opts(bad).into_request().unwrap_err().code(),
            "invalid_config",
            "{bad}"
        );
    }
}

// ---- Consent option and agreement link (SPEC §6.6 case (b)) ----

#[test]
fn consent_option_parses_from_the_json_the_ui_sends() {
    let o: InstallOptions = serde_json::from_value(serde_json::json!({
        "version": "1.2.3",
        "consent": { "kinds": ["source", "package"], "tokens": ["a", "b"] }
    }))
    .unwrap();
    let (req, consent) = o.into_parts().unwrap();
    // The request for the executor never carries the UI's consent.
    assert_eq!(req.consent, None);
    let c = consent.unwrap();
    assert_eq!(c.kinds, ["source", "package"]);
    assert_eq!(c.tokens, ["a", "b"]);
    // No consent key, an empty object: no consent.
    let o: InstallOptions = serde_json::from_value(serde_json::json!({})).unwrap();
    assert!(o.into_parts().unwrap().1.is_none());
}

#[tokio::test]
async fn a_link_that_no_challenge_showed_does_not_open() {
    let (state, _) = state_with(false, false, exec_factory(FakeExec::default())).await;
    for url in [
        "https://evil.example/",
        "file:///c:/windows/system32/calc.exe",
    ] {
        let err = run_open_agreement_link(&state, "winget_community/Foo.Bar", url)
            .await
            .unwrap_err();
        assert_eq!(err.code(), "invalid_config");
    }
}

#[tokio::test]
async fn a_stored_non_https_link_is_refused_before_the_browser() {
    let (state, _) = state_with(false, false, exec_factory(FakeExec::default())).await;
    let core = state.core().await.unwrap();
    let id = PackageId::parse("winget_community/Foo.Bar")
        .map(|(k, l)| PackageId::new(k, &l))
        .unwrap();
    for url in ["http://example.org/t", "file:///c:/x", "https://e.org/<b>"] {
        core.consent.issue(
            &id,
            crate::domain::Operation::Install,
            false,
            None,
            "winget",
            vec![url.to_string()],
        );
        // The link is in a live challenge, yet it is not a safe https link.
        assert!(core.consent.link_allowed(&id, url));
        let err = run_open_agreement_link(&state, "winget_community/Foo.Bar", url)
            .await
            .unwrap_err();
        assert_eq!(err.code(), "invalid_config", "{url}");
    }
}

// ---- Toast on finish (SPEC §6.9) ----

use crate::platform::windows::{FakeNotifier, FakeWindowFocus, ToastDeps};

/// Runs one scripted install to its end with the given focus value and setting; returns the
/// toasts that the fake notifier recorded.
async fn toasts_after_install(
    focused: bool,
    setting: Option<&str>,
    result: crate::error::AppResult<()>,
    notifier_fails: bool,
) -> Vec<crate::platform::windows::Toast> {
    use OperationStage::*;
    let exec = FakeExec::scripted(vec![(vec![Resolving, Done], result)]);
    let notifier = Arc::new(FakeNotifier::default());
    notifier.fail.store(notifier_fails, Ordering::SeqCst);
    let toasts = ToastDeps {
        notifier: notifier.clone(),
        focus: Arc::new(FakeWindowFocus(focused)),
    };
    let (state, pool) = state_with_toasts(exec_factory(exec), toasts).await;
    if let Some(v) = setting {
        crate::db::repo::settings::set(&pool, "notifications_enabled", v)
            .await
            .unwrap();
    }
    let sink = Arc::new(RecordingSink::default());
    run_start_install(&state, sink.clone(), PKG, InstallOptions::default())
        .await
        .unwrap();
    wait_for(&sink, EVENT_OPERATION_FINISHED).await;
    // The toast follows the event; let the task end.
    for _ in 0..50 {
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    notifier.toasts()
}

#[tokio::test]
async fn unfocused_window_gets_one_toast_when_an_operation_ends() {
    let t = toasts_after_install(false, None, Ok(()), false).await;
    assert_eq!(t.len(), 1);
    assert_eq!(t[0].title, "Install finished");
    assert_eq!(t[0].body, "Foo.Bar was installed.");
}

#[tokio::test]
async fn focused_window_gets_no_toast() {
    assert!(toasts_after_install(true, None, Ok(()), false)
        .await
        .is_empty());
}

#[tokio::test]
async fn setting_off_gets_no_toast() {
    assert!(toasts_after_install(false, Some("false"), Ok(()), false)
        .await
        .is_empty());
    assert_eq!(
        toasts_after_install(false, Some("true"), Ok(()), false)
            .await
            .len(),
        1
    );
}

#[tokio::test]
async fn failed_operation_toasts_and_cancel_does_not() {
    let t = toasts_after_install(
        false,
        None,
        Err(AppError::DeploymentFailed("x".into())),
        false,
    )
    .await;
    assert_eq!(t.len(), 1);
    assert_eq!(t[0].title, "Install failed");
    assert!(
        toasts_after_install(false, None, Err(AppError::Cancelled), false)
            .await
            .is_empty()
    );
}

#[tokio::test]
async fn a_failing_notifier_does_not_fail_the_operation() {
    let t = toasts_after_install(false, None, Ok(()), true).await;
    assert!(t.is_empty());
}

#[tokio::test]
async fn read_import_file_reads_a_dropped_file_and_refuses_others() {
    use crate::executor::InstalledPackage;
    use crate::services::import::RowStatus;
    let exec = FakeExec::default();
    exec.installed.lock().unwrap().push(InstalledPackage {
        id: "Git.Git".into(),
        name: "Git".into(),
        version: "2".into(),
        available: None,
        source: None,
    });
    let (state, _) = state_with(false, false, exec_factory(exec)).await;
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("mine.json");
    std::fs::write(
        &file,
        r#"{"Sources":[{"SourceDetails":{"Name":"winget"},"Packages":[
            {"PackageIdentifier":"VideoLAN.VLC"},{"PackageIdentifier":"Git.Git"}]}]}"#,
    )
    .unwrap();
    let path = file.display().to_string();
    // The webview names a path the core never saw: refused, file not opened.
    let err = run_read_import_file(&state, &path).await.unwrap_err();
    assert!(matches!(err, AppError::InvalidConfig(_)), "{err:?}");
    // After a drop event the core recorded it.
    let canon = state.import_paths.record_drop(&file).unwrap();
    let review = run_read_import_file(&state, &canon.display().to_string())
        .await
        .unwrap();
    assert_eq!(review.file_name, "mine.json");
    assert!(review.installed_known);
    let st: Vec<_> = review.rows.iter().map(|r| r.status).collect();
    assert_eq!(st, vec![RowStatus::Ready, RowStatus::Installed]);
}
