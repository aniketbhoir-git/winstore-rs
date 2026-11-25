//! In-app update scan (SPEC §6.10). While the app runs, a timer checks every 15 minutes whether
//! a scan is due. A scan only lists updates (`check_updates`); it never installs, updates or
//! removes anything and never passes an agreement flag.

use std::sync::Arc;
use std::time::Duration;

use crate::db::repo::{operation_log, settings};
use crate::domain::{now_unix, LogLevel, OperationLogEntry};
use crate::error::{AppError, AppResult};
use crate::ipc::state::Core;
use crate::platform::windows::Toast;
use crate::services::notify;

/// `off` (default), `daily` or `weekly`.
pub const SETTING_UPDATE_SCAN: &str = "update_scan";
/// Unix seconds of the last scan.
pub const SETTING_UPDATE_SCAN_LAST: &str = "update_scan_last";
/// Sorted package ids of the last scan toast, joined with `,`.
pub const SETTING_UPDATE_SCAN_LAST_IDS: &str = "update_scan_last_ids";

/// How often the timer checks whether a scan is due.
pub const TICK: Duration = Duration::from_secs(15 * 60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanMode {
    Off,
    Daily,
    Weekly,
}

impl ScanMode {
    pub fn parse(s: &str) -> Option<ScanMode> {
        match s {
            "off" => Some(ScanMode::Off),
            "daily" => Some(ScanMode::Daily),
            "weekly" => Some(ScanMode::Weekly),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            ScanMode::Off => "off",
            ScanMode::Daily => "daily",
            ScanMode::Weekly => "weekly",
        }
    }

    fn interval_secs(self) -> Option<i64> {
        match self {
            ScanMode::Off => None,
            ScanMode::Daily => Some(24 * 3600),
            ScanMode::Weekly => Some(7 * 24 * 3600),
        }
    }
}

/// What one tick did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanOutcome {
    /// The setting is `off`.
    Off,
    /// The last scan is younger than the interval.
    NotDue,
    /// An operation runs; the scan waits for the next tick.
    Busy,
    /// The scan ran and listed `found` updates; `toast` tells whether one toast was shown.
    Scanned { found: usize, toast: bool },
    /// The scan ended with an error (one Warn row), or with `agreements_required` (a toast).
    Failed { toast: bool },
}

/// The saved mode. A missing, unknown or unreadable value counts as `off`.
pub async fn mode(core: &Core) -> ScanMode {
    match settings::get(&core.pool, SETTING_UPDATE_SCAN).await {
        Ok(Some(v)) => ScanMode::parse(v.trim()).unwrap_or(ScanMode::Off),
        Ok(None) => ScanMode::Off,
        Err(e) => {
            tracing::warn!("could not read {SETTING_UPDATE_SCAN}: {e}");
            ScanMode::Off
        }
    }
}

/// Saves the mode; any text other than `off`, `daily` or `weekly` is `invalid_config`.
pub async fn set_mode(core: &Core, value: &str) -> AppResult<()> {
    let mode = ScanMode::parse(value).ok_or_else(|| {
        AppError::InvalidConfig(format!(
            "update_scan must be off, daily or weekly, not {value:?}"
        ))
    })?;
    settings::set(&core.pool, SETTING_UPDATE_SCAN, mode.as_str()).await
}

async fn warn_row(core: &Core, message: String) {
    let entry = OperationLogEntry {
        id: None,
        ts: now_unix(),
        level: LogLevel::Warn,
        operation: None,
        package_id: None,
        message,
        detail: None,
    };
    if let Err(e) = operation_log::append(&core.pool, &entry).await {
        tracing::warn!("operation_log write failed: {e}");
    }
}

fn updates_toast(count: usize) -> Toast {
    let body = if count == 1 {
        "1 update is available.".to_string()
    } else {
        format!("{count} updates are available.")
    };
    Toast {
        title: "Updates available".into(),
        body,
    }
}

async fn show_toast(core: &Core, toast: &Toast) -> bool {
    notify::show(
        Arc::clone(&core.toasts.notifier),
        &core.pool,
        toast,
        None,
        None,
    )
    .await
}

async fn save(core: &Core, key: &str, value: &str) {
    if let Err(e) = settings::set(&core.pool, key, value).await {
        tracing::warn!("could not save {key}: {e}");
    }
}

/// One tick: scans when the mode is on, the last scan is old enough and no operation runs.
/// `now` is Unix seconds (a parameter, so tests control the clock).
pub async fn scan_if_due(core: &Core, now: i64) -> ScanOutcome {
    let Some(interval) = mode(core).await.interval_secs() else {
        return ScanOutcome::Off;
    };
    let last = settings::get(&core.pool, SETTING_UPDATE_SCAN_LAST)
        .await
        .ok()
        .flatten()
        .and_then(|v| v.trim().parse::<i64>().ok());
    // A last scan in the future (the clock moved back) counts as due.
    if last.is_some_and(|l| l <= now && now - l < interval) {
        return ScanOutcome::NotDue;
    }
    if !core.active.is_empty() {
        return ScanOutcome::Busy;
    }
    let result = match core.executor().await {
        Ok(executor) => executor.list_updates().await,
        Err(e) => Err(e),
    };
    // The attempt counts: an error waits for the next interval.
    save(core, SETTING_UPDATE_SCAN_LAST, &now.to_string()).await;
    match result {
        Ok(updates) => {
            let mut ids: Vec<String> = updates.iter().map(|u| u.package_id.to_string()).collect();
            ids.sort();
            let joined = ids.join(",");
            let last_ids = settings::get(&core.pool, SETTING_UPDATE_SCAN_LAST_IDS)
                .await
                .ok()
                .flatten()
                .unwrap_or_default();
            let mut toast = false;
            if ids.is_empty() {
                // A later return of the same set is news again.
                if !last_ids.is_empty() {
                    save(core, SETTING_UPDATE_SCAN_LAST_IDS, "").await;
                }
            } else if joined != last_ids && notify::enabled(core).await {
                toast = show_toast(core, &updates_toast(ids.len())).await;
                if toast {
                    save(core, SETTING_UPDATE_SCAN_LAST_IDS, &joined).await;
                }
            }
            ScanOutcome::Scanned {
                found: ids.len(),
                toast,
            }
        }
        Err(AppError::AgreementsRequired { .. }) => {
            let review = Toast {
                title: "Update check needs you".into(),
                body: "Open Store to review an agreement.".into(),
            };
            let toast = notify::enabled(core).await && show_toast(core, &review).await;
            ScanOutcome::Failed { toast }
        }
        Err(e) => {
            warn_row(core, format!("update scan failed: {e}")).await;
            ScanOutcome::Failed { toast: false }
        }
    }
}

/// The timer: the first tick comes at once, then every `TICK`. Runs until the task is dropped.
/// `get_core` runs on each tick; a tick where it fails (the core is not built yet) is skipped, so
/// one timer is enough however late the core comes up.
pub async fn run<F, Fut>(get_core: F, clock: impl Fn() -> i64)
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = AppResult<Arc<Core>>>,
{
    run_every(get_core, clock, TICK).await
}

/// `run` with its own tick length (tests use a short one).
pub(crate) async fn run_every<F, Fut>(get_core: F, clock: impl Fn() -> i64, tick: Duration)
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = AppResult<Arc<Core>>>,
{
    let mut interval = tokio::time::interval(tick);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        interval.tick().await;
        match get_core().await {
            Ok(core) => {
                let outcome = scan_if_due(&core, clock()).await;
                tracing::debug!(?outcome, "update scan tick");
            }
            Err(e) => tracing::warn!("update scan tick skipped, no core yet: {e}"),
        }
    }
}

#[cfg(test)]
mod tests;
