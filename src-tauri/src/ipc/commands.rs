//! Every `#[tauri::command]` handler (SPEC §3 `ipc/commands.rs`, §6).
//!
//! Each command is a thin wrapper around a plain async function that takes `&AppState` and an
//! `EventSink`, so the behaviour is testable without a Tauri runtime.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use super::events::{
    self, emit, error_json, EventSink, OperationFinishedEvent, SearchFailedEvent,
    SearchResultsEvent, SharedSink, UpdateOutcome, UpdatesSummaryEvent, EVENT_OPERATION_FINISHED,
    EVENT_SEARCH_FAILED, EVENT_SEARCH_RESULTS, EVENT_UPDATES_SUMMARY,
};
use super::state::{AppState, Core};
use crate::catalog::SearchQuery;
use crate::db::repo::installs;
use crate::domain::{
    InstallRecord, InstallState, Operation, PackageDetail, PackageId, PackageSummary,
    UpdateAvailable,
};
use crate::error::{AppError, AppResult};
use crate::executor::{InstallRequest, InstallScope, OperationHandle};

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

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct OperationStarted {
    pub operation_id: String,
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

/// SPEC §6.2: starts an install from an explicit user click. Paid items have no in-app
/// install path (SPEC §0 rule 1) and are refused here, not only hidden in the UI.
pub async fn run_start_install(
    state: &AppState,
    sink: SharedSink,
    package_id: &str,
    options: InstallOptions,
) -> AppResult<OperationStarted> {
    let core = state.core().await?;
    let (kind, local) = PackageId::parse(package_id)?;
    let id = PackageId::new(kind, &local);
    let request = options.into_request()?;

    let detail = core.catalog.get_detail(&id).await?;
    if detail.summary.is_paid {
        return Err(AppError::Unsupported(
            "paid items cannot be installed here; use \"Buy in Microsoft Store\"".into(),
        ));
    }
    let target = request
        .version
        .clone()
        .or_else(|| Some(detail.summary.latest_version.clone()).filter(|v| !v.is_empty()));

    let executor = core.executor().await?;
    // The executor's per-package guard rejects a second operation before anything is written.
    let handle = executor.install(&id, &request).await?;
    if let Err(e) = begin(&core, &id, target.clone(), None).await {
        // The process is already running: cancel it and drain it in the background so
        // nothing keeps running unseen. The ledger was never touched, so it is left alone.
        let _ = executor.cancel(&handle.operation_id).await;
        let err = error_json(&e);
        tokio::spawn(async move { abandon(&sink, handle, &id, err).await });
        return Err(e);
    }
    Ok(spawn_operation(
        core,
        sink,
        handle,
        id,
        Operation::Install,
        target,
    ))
}

pub async fn run_cancel(state: &AppState, operation_id: &str) -> AppResult<()> {
    let core = state.core().await?;
    core.executor().await?.cancel(operation_id).await
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
                match begin(
                    &core,
                    &u.package_id,
                    target.clone(),
                    Some(&u.installed_version),
                )
                .await
                {
                    Ok(()) => {
                        drive(
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
                        abandon(&sink, handle, &u.package_id, error_json(&e)).await;
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
// Operation lifecycle (ledger + progress relay)
// ---------------------------------------------------------------------------------------------

/// Records `Installing` in the ledger. A leftover `Installing` row from an earlier run is
/// accepted: the executor guard already ensured nothing is running for this package.
/// `installed_version` seeds a row for an app that was installed outside this app, so a
/// failed or cancelled update keeps it `Installed`.
async fn begin(
    core: &Core,
    id: &PackageId,
    target: Option<String>,
    installed_version: Option<&str>,
) -> AppResult<()> {
    let mut rec = match installs::get(&core.pool, id).await? {
        Some(r) => r,
        None => {
            let (kind, _) = PackageId::parse(id.as_str())?;
            let mut rec = InstallRecord::new(id.clone(), kind);
            if let Some(v) = installed_version.filter(|v| !v.is_empty()) {
                rec.installed_version = Some(v.to_string());
                rec.state = InstallState::Installed;
            }
            rec
        }
    };
    if rec.state != InstallState::Installing {
        rec.state = rec.state.transition(InstallState::Installing)?;
    }
    rec.target_version = target;
    installs::upsert(&core.pool, &rec).await
}

/// Relays progress events, waits for the outcome and settles the ledger.
async fn drive(
    core: &Core,
    sink: &SharedSink,
    mut handle: OperationHandle,
    id: &PackageId,
    op: Operation,
    target: Option<String>,
) -> AppResult<()> {
    while let Some(ev) = handle.events.recv().await {
        events::emit_progress(sink.as_ref(), &ev);
    }
    let result = match handle.outcome.await {
        Ok(r) => r,
        Err(_) => Err(AppError::DeploymentFailed(
            "the operation ended without a result".into(),
        )),
    };
    if let Err(e) = settle(core, id, op, target, &result).await {
        tracing::error!(package = %id, "could not update install ledger: {e}");
    }
    emit(
        sink.as_ref(),
        EVENT_OPERATION_FINISHED,
        &OperationFinishedEvent {
            operation_id: handle.operation_id.clone(),
            package_id: id.clone(),
            ok: result.is_ok(),
            error: result.as_ref().err().map(error_json),
        },
    );
    result
}

/// Drains a started operation that the ledger refused: progress is still relayed, the outcome
/// is awaited, and the finish event carries the refusal. The ledger is not written.
async fn abandon(
    sink: &SharedSink,
    mut handle: OperationHandle,
    id: &PackageId,
    error: serde_json::Value,
) {
    while let Some(ev) = handle.events.recv().await {
        events::emit_progress(sink.as_ref(), &ev);
    }
    let _ = handle.outcome.await;
    emit(
        sink.as_ref(),
        EVENT_OPERATION_FINISHED,
        &OperationFinishedEvent {
            operation_id: handle.operation_id.clone(),
            package_id: id.clone(),
            ok: false,
            error: Some(error),
        },
    );
}

fn spawn_operation(
    core: Arc<Core>,
    sink: SharedSink,
    handle: OperationHandle,
    id: PackageId,
    op: Operation,
    target: Option<String>,
) -> OperationStarted {
    let started = OperationStarted {
        operation_id: handle.operation_id.clone(),
    };
    tokio::spawn(async move {
        let _ = drive(&core, &sink, handle, &id, op, target).await;
    });
    started
}

async fn settle(
    core: &Core,
    id: &PackageId,
    op: Operation,
    target: Option<String>,
    result: &AppResult<()>,
) -> AppResult<()> {
    let Some(mut rec) = installs::get(&core.pool, id).await? else {
        return Ok(());
    };
    let had_version = rec.installed_version.is_some();
    let next = match result {
        Ok(()) => {
            rec.installed_version = target.or(rec.installed_version.take());
            rec.target_version = None;
            rec.last_updated = Some(crate::domain::now_unix());
            InstallState::Installed
        }
        // A failed or cancelled operation leaves an already installed version in place,
        // whether it was an update or a reinstall.
        Err(_) if op != Operation::Remove && had_version => InstallState::Installed,
        Err(AppError::Cancelled) => InstallState::NotInstalled,
        Err(_) => InstallState::Failed,
    };
    rec.state = rec.state.transition(next)?;
    installs::upsert(&core.pool, &rec).await
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
