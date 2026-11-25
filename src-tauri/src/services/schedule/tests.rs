use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use super::*;
use crate::domain::{PackageId, UpdateAvailable};
use crate::ipc::test_support::*;
use crate::platform::windows::{FakeNotifier, FakeWindowFocus, ToastDeps};

const DAY: i64 = 24 * 3600;

fn update(name: &str) -> UpdateAvailable {
    UpdateAvailable {
        package_id: PackageId::new(KIND, name),
        installed_version: "1.0.0".into(),
        latest_version: "2.0.0".into(),
        source: KIND,
        is_auto: false,
    }
}

struct Rig {
    core: Arc<Core>,
    exec: Arc<FakeExec>,
    notifier: Arc<FakeNotifier>,
}

async fn rig(mode: Option<&str>, updates: Vec<&str>) -> Rig {
    let exec = Arc::new(FakeExec {
        updates: updates.into_iter().map(update).collect(),
        ..FakeExec::default()
    });
    let notifier = Arc::new(FakeNotifier::default());
    let toasts = ToastDeps {
        notifier: notifier.clone(),
        focus: Arc::new(FakeWindowFocus(true)),
    };
    let factory = shared_exec_factory(exec.clone());
    let (state, pool) = state_with_toasts(factory, toasts).await;
    if let Some(m) = mode {
        settings::set(&pool, SETTING_UPDATE_SCAN, m).await.unwrap();
    }
    Rig {
        core: state.core().await.unwrap(),
        exec,
        notifier,
    }
}

fn calls(r: &Rig) -> u32 {
    r.exec.update_calls.load(Ordering::SeqCst)
}

async fn warns(r: &Rig) -> Vec<String> {
    operation_log::recent(&r.core.pool, 50)
        .await
        .unwrap()
        .into_iter()
        .filter(|e| e.level == LogLevel::Warn)
        .map(|e| e.message)
        .collect()
}

#[tokio::test]
async fn setting_off_or_missing_never_scans() {
    for mode in [None, Some("off"), Some("nonsense")] {
        let r = rig(mode, vec!["A.A"]).await;
        assert_eq!(scan_if_due(&r.core, 1_000_000).await, ScanOutcome::Off);
        assert_eq!(calls(&r), 0);
    }
}

#[tokio::test]
async fn due_scans_once_and_not_due_does_not() {
    let r = rig(Some("daily"), vec![]).await;
    let t = 1_000_000;
    assert!(matches!(
        scan_if_due(&r.core, t).await,
        ScanOutcome::Scanned { found: 0, .. }
    ));
    assert_eq!(calls(&r), 1);
    assert_eq!(scan_if_due(&r.core, t + DAY - 1).await, ScanOutcome::NotDue);
    assert_eq!(calls(&r), 1);
    assert!(matches!(
        scan_if_due(&r.core, t + DAY).await,
        ScanOutcome::Scanned { .. }
    ));
    assert_eq!(calls(&r), 2);
}

#[tokio::test]
async fn weekly_waits_a_week() {
    let r = rig(Some("weekly"), vec![]).await;
    scan_if_due(&r.core, 1_000_000).await;
    assert_eq!(
        scan_if_due(&r.core, 1_000_000 + 6 * DAY).await,
        ScanOutcome::NotDue
    );
    assert!(matches!(
        scan_if_due(&r.core, 1_000_000 + 7 * DAY).await,
        ScanOutcome::Scanned { .. }
    ));
}

#[tokio::test]
async fn an_active_operation_delays_the_scan() {
    let r = rig(Some("daily"), vec![]).await;
    let id = PackageId::new(KIND, "Busy.App");
    r.core
        .active
        .insert(&id, "op-1", crate::domain::Operation::Install);
    assert_eq!(scan_if_due(&r.core, 1_000_000).await, ScanOutcome::Busy);
    assert_eq!(calls(&r), 0);
    r.core.active.finish(&id, "op-1");
    assert!(matches!(
        scan_if_due(&r.core, 1_000_000).await,
        ScanOutcome::Scanned { .. }
    ));
    assert_eq!(calls(&r), 1);
}

#[tokio::test]
async fn new_set_toasts_once_and_the_same_set_does_not() {
    let r = rig(Some("daily"), vec!["B.B", "A.A"]).await;
    let t = 1_000_000;
    assert_eq!(
        scan_if_due(&r.core, t).await,
        ScanOutcome::Scanned {
            found: 2,
            toast: true
        }
    );
    let shown = r.notifier.toasts();
    assert_eq!(shown.len(), 1);
    assert_eq!(shown[0].body, "2 updates are available.");
    assert_eq!(
        scan_if_due(&r.core, t + DAY).await,
        ScanOutcome::Scanned {
            found: 2,
            toast: false
        }
    );
    assert_eq!(r.notifier.toasts().len(), 1);
}

#[tokio::test]
async fn a_changed_set_toasts_again() {
    let r = rig(Some("daily"), vec!["A.A", "C.C"]).await;
    // The last toast named a different set.
    settings::set(
        &r.core.pool,
        SETTING_UPDATE_SCAN_LAST_IDS,
        "winget_community/A.A",
    )
    .await
    .unwrap();
    scan_if_due(&r.core, 1_000_000).await;
    assert_eq!(r.notifier.toasts().len(), 1);
}

#[tokio::test]
async fn no_updates_gives_no_toast() {
    let r = rig(Some("daily"), vec![]).await;
    assert_eq!(
        scan_if_due(&r.core, 1_000_000).await,
        ScanOutcome::Scanned {
            found: 0,
            toast: false
        }
    );
    assert!(r.notifier.toasts().is_empty());
}

#[tokio::test]
async fn toasts_off_gives_no_scan_toast() {
    let r = rig(Some("daily"), vec!["A.A"]).await;
    settings::set(&r.core.pool, "notifications_enabled", "false")
        .await
        .unwrap();
    assert_eq!(
        scan_if_due(&r.core, 1_000_000).await,
        ScanOutcome::Scanned {
            found: 1,
            toast: false
        }
    );
    assert!(r.notifier.toasts().is_empty());
}

#[tokio::test]
async fn agreements_required_gives_the_review_toast() {
    let r = rig(Some("daily"), vec![]).await;
    *r.exec.updates_error.lock().unwrap() = Some(AppError::AgreementsRequired {
        source_agreements: true,
        package_agreements: false,
        winget_source: Some("winget".into()),
        version: None,
        agreement_text: None,
        agreement_url: None,
        agreements: Vec::new(),
        consent_token: None,
    });
    assert_eq!(
        scan_if_due(&r.core, 1_000_000).await,
        ScanOutcome::Failed { toast: true }
    );
    let shown = r.notifier.toasts();
    assert_eq!(shown.len(), 1);
    assert_eq!(shown[0].body, "Open Store to review an agreement.");
    assert!(warns(&r).await.is_empty());
}

#[tokio::test]
async fn another_error_gives_one_warn_row_and_no_toast() {
    let r = rig(Some("daily"), vec![]).await;
    *r.exec.updates_error.lock().unwrap() = Some(AppError::SourceUnavailable("down".into()));
    assert_eq!(
        scan_if_due(&r.core, 1_000_000).await,
        ScanOutcome::Failed { toast: false }
    );
    assert!(r.notifier.toasts().is_empty());
    let w = warns(&r).await;
    assert_eq!(w.len(), 1, "{w:?}");
    assert!(w[0].contains("update scan failed"));
    // The next scan comes at the next interval, not at the next tick.
    assert_eq!(
        scan_if_due(&r.core, 1_000_000 + 60).await,
        ScanOutcome::NotDue
    );
}

#[tokio::test]
async fn a_scan_never_installs_updates_or_removes() {
    let r = rig(Some("daily"), vec!["A.A", "B.B"]).await;
    scan_if_due(&r.core, 1_000_000).await;
    assert_eq!(calls(&r), 1);
    assert!(r.exec.install_requests.lock().unwrap().is_empty());
    assert!(r.exec.remove_requests.lock().unwrap().is_empty());
    assert_eq!(
        r.exec.next.load(Ordering::SeqCst),
        0,
        "no operation started"
    );
}

#[tokio::test]
async fn set_mode_accepts_three_words_only() {
    let r = rig(None, vec![]).await;
    for ok in ["off", "daily", "weekly"] {
        set_mode(&r.core, ok).await.unwrap();
        assert_eq!(mode(&r.core).await.as_str(), ok);
    }
    for bad in ["", "hourly", "Daily", "on"] {
        assert_eq!(
            set_mode(&r.core, bad).await.unwrap_err().code(),
            "invalid_config"
        );
    }
    assert_eq!(mode(&r.core).await, ScanMode::Weekly);
}

#[tokio::test]
async fn the_timer_ticks_at_start_then_on_every_tick() {
    let r = rig(Some("daily"), vec![]).await;
    // A short tick and a clock that the test moves; the real tick is `TICK`.
    let now = Arc::new(std::sync::atomic::AtomicI64::new(1_000_000));
    let clock = {
        let now = now.clone();
        move || now.load(Ordering::SeqCst)
    };
    let core = r.core.clone();
    let task = tokio::spawn(run_every(
        move || {
            let core = core.clone();
            async move { Ok(core) }
        },
        clock,
        Duration::from_millis(20),
    ));
    // First tick at once: one scan. Later ticks find the scan not due.
    wait_until("the first scan", || async { calls(&r) == 1 }).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(calls(&r), 1);
    // A day later the next tick finds it due.
    now.store(1_000_000 + DAY, Ordering::SeqCst);
    wait_until("the second scan", || async { calls(&r) == 2 }).await;
    task.abort();
}

#[tokio::test]
async fn the_timer_skips_ticks_until_the_core_exists() {
    let r = rig(Some("daily"), vec![]).await;
    let asked = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let (core, n) = (r.core.clone(), asked.clone());
    let task = tokio::spawn(run_every(
        move || {
            let core = core.clone();
            let n = n.clone();
            async move {
                // The first two asks fail, as when start-up has not built the core yet.
                if n.fetch_add(1, Ordering::SeqCst) < 2 {
                    Err(AppError::Unsupported("not yet".into()))
                } else {
                    Ok(core)
                }
            }
        },
        || 1_000_000,
        Duration::from_millis(20),
    ));
    wait_until("the scan after the core came up", || async {
        calls(&r) == 1
    })
    .await;
    assert!(asked.load(Ordering::SeqCst) >= 3);
    task.abort();
}
