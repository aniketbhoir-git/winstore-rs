//! Toast on finish (SPEC §6.9). When an operation ends while the main window has no focus, the
//! app shows one toast. A failed toast never fails the operation: one Warn row in the trace log
//! and the code goes on.

use std::sync::Arc;

use crate::db::repo::{operation_log, settings};
use crate::db::DbPool;
use crate::domain::{now_unix, LogLevel, Operation, OperationLogEntry, PackageId};
use crate::error::{AppError, AppResult};
use crate::ipc::state::Core;
use crate::platform::windows::{Notifier, Toast};

/// Settings key that turns toasts off (`"false"`); the default is on.
pub const SETTING_NOTIFICATIONS: &str = "notifications_enabled";

/// True unless the user turned toasts off. A failed read counts as on.
pub async fn enabled(core: &Core) -> bool {
    match settings::get(&core.pool, SETTING_NOTIFICATIONS).await {
        Ok(Some(v)) => v.trim() != "false",
        Ok(None) => true,
        Err(e) => {
            tracing::warn!("could not read {SETTING_NOTIFICATIONS}: {e}");
            true
        }
    }
}

/// The toast for an ended operation, or `None` when the user ended it (cancel).
pub fn finished_toast(id: &PackageId, op: Operation, result: &AppResult<()>) -> Option<Toast> {
    let name = PackageId::parse(id.as_str())
        .map(|(_, n)| n)
        .unwrap_or_else(|_| id.to_string());
    let (done, verb) = match op {
        Operation::Install => ("installed", "Install"),
        Operation::Update => ("updated", "Update"),
        Operation::Remove => ("removed", "Uninstall"),
    };
    match result {
        Ok(()) => Some(Toast {
            title: format!("{verb} finished"),
            body: format!("{name} was {done}."),
        }),
        Err(AppError::Cancelled) => None,
        Err(AppError::AgreementsRequired { .. }) => Some(Toast {
            title: format!("{verb} needs you"),
            body: "Open Store to review an agreement.".into(),
        }),
        Err(_) => Some(Toast {
            title: format!("{verb} failed"),
            body: format!("{name} did not finish. Open Store for details."),
        }),
    }
}

/// Shows the toast of an ended operation when the window has no focus and toasts are on. The
/// checks run here; the call into Windows runs in the background, so the caller never waits
/// for it (`update_all` shows one toast per package).
pub async fn operation_finished(
    core: &Core,
    id: &PackageId,
    op: Operation,
    result: &AppResult<()>,
) {
    let Some(toast) = finished_toast(id, op, result) else {
        return;
    };
    if core.toasts.focus.is_focused() || !enabled(core).await {
        return;
    }
    let notifier = Arc::clone(&core.toasts.notifier);
    let pool = core.pool.clone();
    let id = id.clone();
    tokio::spawn(async move {
        show(notifier, &pool, &toast, Some(op), Some(&id)).await;
    });
}

/// Calls the notifier on a blocking thread. A shown toast writes an Info line to the
/// application log; a failure writes one Warn row to the operation log and nothing else.
/// Returns whether the toast was shown.
pub async fn show(
    notifier: Arc<dyn Notifier>,
    pool: &DbPool,
    toast: &Toast,
    op: Option<Operation>,
    id: Option<&PackageId>,
) -> bool {
    let t = toast.clone();
    let outcome = tokio::task::spawn_blocking(move || notifier.show(&t))
        .await
        .unwrap_or_else(|e| Err(format!("toast thread failed: {e}")));
    match outcome {
        Ok(()) => {
            tracing::info!("toast shown");
            true
        }
        Err(e) => {
            let entry = OperationLogEntry {
                id: None,
                ts: now_unix(),
                level: LogLevel::Warn,
                operation: op,
                package_id: id.cloned(),
                message: format!("toast failed: {e}"),
                detail: None,
            };
            if let Err(e) = operation_log::append(pool, &entry).await {
                tracing::warn!("operation_log write failed: {e}");
            }
            false
        }
    }
}
