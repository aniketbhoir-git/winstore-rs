//! Toast on finish (SPEC §6.9). When an operation ends while the main window has no focus, the
//! app shows one toast. A failed toast never fails the operation: one Warn row in the trace log
//! and the code goes on.

use crate::db::repo::settings;
use crate::domain::{Operation, PackageId};
use crate::error::{AppError, AppResult};
use crate::ipc::state::Core;
use crate::platform::windows::Toast;

/// Settings key that turns toasts off (`"false"`); the default is on.
pub const SETTING_NOTIFICATIONS: &str = "notifications_enabled";

/// True unless the user turned toasts off. A failed read counts as on.
pub async fn enabled(core: &Core) -> bool {
    match settings::get(&core.pool, SETTING_NOTIFICATIONS).await {
        Ok(Some(v)) => !matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "false" | "0" | "off"
        ),
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

/// Shows the toast of an ended operation when the window has no focus and toasts are on.
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
    match core.toasts.notifier.show(&toast) {
        Ok(()) => tracing::info!(package = %id, "toast shown"),
        Err(e) => tracing::warn!(package = %id, "toast failed: {e}"),
    }
}
