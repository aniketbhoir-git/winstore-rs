//! Every `#[tauri::command]` handler (SPEC §3 `ipc/commands.rs`, §6).
//!
//! Each command is a thin wrapper around a plain async function that takes `&AppState` and an
//! `EventSink`, so the behaviour is testable without a Tauri runtime.

use std::sync::Arc;

use serde::Deserialize;
use tauri::{AppHandle, Emitter, State};

use super::events::{
    emit, error_json, EventSink, SearchFailedEvent, SearchResultsEvent, SharedSink, UpdateOutcome,
    UpdatesSummaryEvent, EVENT_SEARCH_FAILED, EVENT_SEARCH_RESULTS, EVENT_UPDATES_SUMMARY,
};
use super::state::AppState;
use crate::catalog::SearchQuery;
use crate::domain::{
    Operation, OperationLogEntry, PackageDetail, PackageId, PackageSummary, UpdateAvailable,
};
use crate::error::{AppError, AppResult};
use crate::executor::{InstallRequest, InstallScope};
pub use crate::services::install::OperationStarted;
use crate::services::install::{self, InstallStatus};

// ---------------------------------------------------------------------------------------------
// Payload types
// ---------------------------------------------------------------------------------------------

/// What the user may choose when installing. Everything else is winget's default.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct InstallOptions {
    pub version: Option<String>,
    /// `"user"` or `"machine"`.
    pub scope: Option<String>,
}

impl InstallOptions {
    pub(crate) fn into_request(self) -> AppResult<InstallRequest> {
        let scope = match self.scope.as_deref() {
            None | Some("") => None,
            Some("user") => Some(InstallScope::User),
            Some("machine") => Some(InstallScope::Machine),
            Some(other) => return Err(AppError::InvalidConfig(format!("unknown scope: {other}"))),
        };
        Ok(InstallRequest {
            version: self.version.filter(|v| !v.trim().is_empty()),
            scope,
        })
    }
}

// ---------------------------------------------------------------------------------------------
// Handlers (no Tauri types)
// ---------------------------------------------------------------------------------------------

/// SPEC §6.1: search the enabled sources through the coordinator. The result is returned and
/// also pushed as `store:search-results` / `store:search-failed`, tagged with `search_id` so the
/// UI can ignore answers to a query the user has already replaced.
pub async fn run_search(
    state: &AppState,
    sink: &dyn EventSink,
    search_id: String,
    query: SearchQuery,
) -> AppResult<Vec<PackageSummary>> {
    let outcome = async {
        let core = state.core().await?;
        core.catalog.search(&query).await
    }
    .await;
    match &outcome {
        Ok(results) => emit(
            sink,
            EVENT_SEARCH_RESULTS,
            &SearchResultsEvent {
                search_id,
                results: results.clone(),
            },
        ),
        Err(e) => {
            tracing::warn!("search failed: {e}");
            emit(
                sink,
                EVENT_SEARCH_FAILED,
                &SearchFailedEvent {
                    search_id,
                    error: error_json(e),
                },
            );
        }
    }
    outcome
}

pub async fn run_get_detail(state: &AppState, package_id: &str) -> AppResult<PackageDetail> {
    let core = state.core().await?;
    let (kind, local) = PackageId::parse(package_id)?;
    core.catalog.get_detail(&PackageId::new(kind, &local)).await
}

/// SPEC §6.2: starts an install (see `services::install::start_install`).
pub async fn run_start_install(
    state: &AppState,
    sink: SharedSink,
    package_id: &str,
    options: InstallOptions,
) -> AppResult<OperationStarted> {
    let core = state.core().await?;
    let id = parse_id(package_id)?;
    install::start_install(&core, sink, id, options.into_request()?).await
}

/// Uninstalls a package recorded as installed by this app.
pub async fn run_start_uninstall(
    state: &AppState,
    sink: SharedSink,
    package_id: &str,
) -> AppResult<OperationStarted> {
    let core = state.core().await?;
    install::start_uninstall(&core, sink, parse_id(package_id)?).await
}

pub async fn run_cancel(state: &AppState, operation_id: &str) -> AppResult<()> {
    install::cancel(&*state.core().await?, operation_id).await
}

pub async fn run_install_status(state: &AppState, package_id: &str) -> AppResult<InstallStatus> {
    install::status(&*state.core().await?, &parse_id(package_id)?).await
}

/// Log tail for the detail page: the newest `limit` entries for a package, oldest first.
pub async fn run_operation_log(
    state: &AppState,
    package_id: &str,
    limit: Option<u32>,
) -> AppResult<Vec<OperationLogEntry>> {
    let core = state.core().await?;
    install::log_tail(&core.pool, &parse_id(package_id)?, limit.unwrap_or(100)).await
}

fn parse_id(package_id: &str) -> AppResult<PackageId> {
    let (kind, local) = PackageId::parse(package_id)?;
    Ok(PackageId::new(kind, &local))
}

/// SPEC §6.3 step 1.
pub async fn run_check_updates(state: &AppState) -> AppResult<Vec<UpdateAvailable>> {
    let core = state.core().await?;
    core.executor().await?.list_updates().await
}

/// SPEC §6.3 step 3: updates one package at a time, emitting progress for each, then a summary.
pub async fn run_update_all(state: &AppState, sink: SharedSink) -> AppResult<UpdatesSummaryEvent> {
    let core = state.core().await?;
    let executor = core.executor().await?;
    let updates = executor.list_updates().await?;
    let mut outcomes = Vec::with_capacity(updates.len());
    for u in updates {
        let target = Some(u.latest_version.clone());
        let result = match executor.update(&u.package_id).await {
            Ok(handle) => {
                // A ledger refusal (for example a pinned row) is this package's failure, not
                // the loop's: cancel the started update, settle it, and continue.
                match install::begin(
                    &core,
                    &u.package_id,
                    Operation::Update,
                    target.clone(),
                    Some(&u.installed_version),
                )
                .await
                {
                    Ok(()) => {
                        install::track(&core, &u.package_id, &handle, Operation::Update);
                        install::drive(
                            &core,
                            &sink,
                            handle,
                            &u.package_id,
                            Operation::Update,
                            target,
                        )
                        .await
                    }
                    Err(e) => {
                        let _ = executor.cancel(&handle.operation_id).await;
                        install::abandon(&sink, handle, &u.package_id, error_json(&e)).await;
                        Err(e)
                    }
                }
            }
            Err(e) => Err(e),
        };
        outcomes.push(UpdateOutcome {
            package_id: u.package_id.clone(),
            ok: result.is_ok(),
            message: result.err().map(|e| e.to_string()),
        });
    }
    let succeeded = outcomes.iter().filter(|o| o.ok).count();
    let summary = UpdatesSummaryEvent {
        total: outcomes.len(),
        succeeded,
        failed: outcomes.len() - succeeded,
        outcomes,
    };
    emit(sink.as_ref(), EVENT_UPDATES_SUMMARY, &summary);
    Ok(summary)
}

/// Official product page for a Store product id (SPEC §0 rule 1, §6.4).
pub fn store_page_url(product_id: &str) -> AppResult<String> {
    let ok = !product_id.is_empty()
        && product_id.len() <= 32
        && product_id.chars().all(|c| c.is_ascii_alphanumeric());
    if !ok {
        return Err(AppError::InvalidConfig(format!(
            "invalid product id: {product_id}"
        )));
    }
    Ok(format!("https://apps.microsoft.com/detail/{product_id}"))
}

/// Opens the official page in the default browser. Nothing else happens in-app.
#[cfg(windows)]
fn open_in_browser(url: &str) -> AppResult<()> {
    // Argument vector, no shell: the validated URL cannot be interpreted as a command.
    std::process::Command::new("rundll32")
        .args(["url.dll,FileProtocolHandler", url])
        .spawn()
        .map(|_| ())
        .map_err(AppError::Io)
}

#[cfg(not(windows))]
fn open_in_browser(_url: &str) -> AppResult<()> {
    Err(AppError::Unsupported(
        "opening the browser is only supported on Windows".into(),
    ))
}

pub fn run_open_store_page(product_id: &str) -> AppResult<()> {
    open_in_browser(&store_page_url(product_id)?)
}

// ---------------------------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------------------------

/// Sends events to every window through the app handle.
pub struct TauriSink(pub AppHandle);

impl EventSink for TauriSink {
    fn emit_json(&self, event: &str, payload: serde_json::Value) {
        if let Err(e) = self.0.emit(event, payload) {
            tracing::warn!(event, "could not emit event: {e}");
        }
    }
}

fn sink(app: &AppHandle) -> SharedSink {
    Arc::new(TauriSink(app.clone()))
}

#[tauri::command]
pub fn app_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[tauri::command]
pub async fn search_sources(
    app: AppHandle,
    state: State<'_, AppState>,
    search_id: String,
    query: SearchQuery,
) -> Result<Vec<PackageSummary>, AppError> {
    run_search(&state, &TauriSink(app), search_id, query).await
}

#[tauri::command]
pub async fn get_app_detail(
    state: State<'_, AppState>,
    package_id: String,
) -> Result<PackageDetail, AppError> {
    run_get_detail(&state, &package_id).await
}

#[tauri::command]
pub async fn start_install(
    app: AppHandle,
    state: State<'_, AppState>,
    package_id: String,
    options: Option<InstallOptions>,
) -> Result<OperationStarted, AppError> {
    run_start_install(&state, sink(&app), &package_id, options.unwrap_or_default()).await
}

#[tauri::command]
pub async fn cancel_install(
    state: State<'_, AppState>,
    operation_id: String,
) -> Result<(), AppError> {
    run_cancel(&state, &operation_id).await
}

#[tauri::command]
pub async fn start_uninstall(
    app: AppHandle,
    state: State<'_, AppState>,
    package_id: String,
) -> Result<OperationStarted, AppError> {
    run_start_uninstall(&state, sink(&app), &package_id).await
}

#[tauri::command]
pub async fn get_install_status(
    state: State<'_, AppState>,
    package_id: String,
) -> Result<InstallStatus, AppError> {
    run_install_status(&state, &package_id).await
}

#[tauri::command]
pub async fn get_operation_log(
    state: State<'_, AppState>,
    package_id: String,
    limit: Option<u32>,
) -> Result<Vec<OperationLogEntry>, AppError> {
    run_operation_log(&state, &package_id, limit).await
}

#[tauri::command]
pub async fn check_updates(state: State<'_, AppState>) -> Result<Vec<UpdateAvailable>, AppError> {
    run_check_updates(&state).await
}

#[tauri::command]
pub async fn update_all(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<UpdatesSummaryEvent, AppError> {
    run_update_all(&state, sink(&app)).await
}

#[tauri::command]
pub fn open_store_page(product_id: String) -> Result<(), AppError> {
    run_open_store_page(&product_id)
}

/// Phase 3 (SPEC §6.5): the optional headless server is not part of this build.
#[tauri::command]
pub fn start_server(_port: u16) -> Result<(), AppError> {
    Err(AppError::Unsupported(
        "the headless server is not available in this build".into(),
    ))
}
