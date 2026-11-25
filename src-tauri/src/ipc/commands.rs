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
use crate::executor::winget_parse::safe_https_url;
use crate::executor::{InstallRequest, InstallScope};
use crate::services::consent::Consent;
use crate::services::export::{self, ExportResult};
use crate::services::import::{self, ImportReview};
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
    /// The agreements that the user accepted in the consent dialog (SPEC §6.6). It is checked
    /// by the service, never passed on as it is.
    pub consent: Option<Consent>,
}

/// The `options` argument of `start_uninstall`: a consent of the kind `"source"` only.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct UninstallOptions {
    pub consent: Option<Consent>,
}

impl InstallOptions {
    /// The request for the executor (it carries no consent) and the consent for the check.
    pub(crate) fn into_parts(self) -> AppResult<(InstallRequest, Option<Consent>)> {
        let consent = self.consent.clone();
        Ok((self.into_request()?, consent))
    }

    pub(crate) fn into_request(self) -> AppResult<InstallRequest> {
        let scope = match self.scope.as_deref() {
            None | Some("") => None,
            Some("user") => Some(InstallScope::User),
            Some("machine") => Some(InstallScope::Machine),
            Some(other) => return Err(AppError::InvalidConfig(format!("unknown scope: {other}"))),
        };
        let version = match self.version.as_deref().map(str::trim) {
            None | Some("") => None,
            Some(v) => {
                validate_version(v)?;
                Some(v.to_string())
            }
        };
        Ok(InstallRequest {
            version,
            scope,
            consent: None,
        })
    }
}

/// Longest version string accepted from the UI.
const VERSION_MAX_LEN: usize = 64;

/// A version goes to winget as an argument value, so it may hold only `[0-9A-Za-z._+-]`, may
/// not start with `-` (it would read as a flag) and must be short.
pub(crate) fn validate_version(v: &str) -> AppResult<()> {
    let ok = !v.is_empty()
        && v.len() <= VERSION_MAX_LEN
        && !v.starts_with('-')
        && v.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '+' | '-'));
    if ok {
        Ok(())
    } else {
        Err(AppError::InvalidConfig(format!("invalid version: {v:?}")))
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
    let (request, consent) = options.into_parts()?;
    install::start_install_with_consent(&core, sink, id, request, consent.as_ref()).await
}

/// Uninstalls a package recorded as installed by this app.
pub async fn run_start_uninstall(
    state: &AppState,
    sink: SharedSink,
    package_id: &str,
    options: UninstallOptions,
) -> AppResult<OperationStarted> {
    let core = state.core().await?;
    install::start_uninstall_with_consent(
        &core,
        sink,
        parse_id(package_id)?,
        options.consent.as_ref(),
    )
    .await
}

/// Opens a link of the consent dialog in the system browser. Only a link that winget printed
/// for this package in a live agreement challenge opens (SPEC §6.6 trust boundary).
pub async fn run_open_agreement_link(
    state: &AppState,
    package_id: &str,
    url: &str,
) -> AppResult<()> {
    let core = state.core().await?;
    let id = parse_id(package_id)?;
    if !core.consent.link_allowed(&id, url) {
        return Err(AppError::InvalidConfig(
            "this link does not belong to an agreement of this package".into(),
        ));
    }
    // Defence in depth: only an https link goes to the browser, whatever the challenge holds.
    let url = safe_https_url(url)
        .ok_or_else(|| AppError::InvalidConfig("only an https link opens in the browser".into()))?;
    open_in_browser(&url)
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
        // Ask the ledger first (as `start_install` does): a refused package (pinned, busy)
        // never reaches the executor, so no winget process starts for it.
        let result = match install::check_ledger(&core, &u.package_id, Operation::Update).await {
            Err(e) => Err(e),
            Ok(()) => match executor.update(&u.package_id).await {
                Ok(handle) => {
                    // The ledger can still refuse `begin`; settle the started update then.
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
            },
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
    options: Option<UninstallOptions>,
) -> Result<OperationStarted, AppError> {
    run_start_uninstall(&state, sink(&app), &package_id, options.unwrap_or_default()).await
}

#[tauri::command]
pub async fn open_agreement_link(
    state: State<'_, AppState>,
    package_id: String,
    url: String,
) -> Result<(), AppError> {
    run_open_agreement_link(&state, &package_id, &url).await
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

/// Hardening design §4: exports the installed packages to winget's JSON format in the exports
/// folder. Read-only on the system; never an agreement flag.
pub async fn run_export_packages(state: &AppState) -> AppResult<ExportResult> {
    let core = state.core().await?;
    let executor = core.executor().await?;
    export::export_installed(&*executor, &export::exports_dir()?, &export::today()).await
}

/// Opens the exports folder (created when missing) in the file manager.
#[cfg(windows)]
pub fn run_open_exports_folder() -> AppResult<()> {
    let dir = export::exports_dir()?;
    std::fs::create_dir_all(&dir)?;
    // Argument vector, no shell; the path comes from the core, never from the webview.
    // Full path, so a copy of `explorer.exe` in a search path cannot run instead.
    let windir = std::env::var_os("WINDIR")
        .or_else(|| std::env::var_os("SystemRoot"))
        .unwrap_or_else(|| r"C:\Windows".into());
    std::process::Command::new(std::path::Path::new(&windir).join("explorer.exe"))
        .arg(&dir)
        .spawn()
        .map(|_| ())
        .map_err(AppError::Io)
}

#[cfg(not(windows))]
pub fn run_open_exports_folder() -> AppResult<()> {
    Err(AppError::Unsupported(
        "opening a folder is only supported on Windows".into(),
    ))
}

/// Hardening design §4, "Import: the file": reads a winget export file and returns the review
/// list. The path must be one the core recorded from a drop, or a `.json` file in the exports
/// folder. Installs nothing; the only process it may start is `winget list`.
pub async fn run_read_import_file(state: &AppState, path: &str) -> AppResult<ImportReview> {
    let file = state.import_paths.resolve(path, &export::exports_dir()?)?;
    let parsed = import::parse_import(&import::read_limited(&file)?)?;
    let core = state.core().await?;
    let mut installed = std::collections::HashSet::new();
    for rec in crate::db::repo::installs::list(&core.pool).await? {
        if rec.state == crate::domain::InstallState::Installed {
            if let Ok((_, local)) = PackageId::parse(rec.package_id.as_str()) {
                installed.insert(local.to_lowercase());
            }
        }
    }
    // A failed `winget list` is not fatal; the review then says it could not check.
    let listed = match core.executor().await {
        Ok(e) => e.list_installed().await.ok(),
        Err(_) => None,
    };
    let installed_known = listed.is_some();
    for p in listed.unwrap_or_default() {
        installed.insert(p.id.to_lowercase());
    }
    Ok(import::build_review(
        &import::display_name(&file),
        &parsed,
        &installed,
        installed_known,
    ))
}

#[tauri::command]
pub async fn read_import_file(
    state: State<'_, AppState>,
    path: String,
) -> Result<ImportReview, AppError> {
    run_read_import_file(&state, &path).await
}

#[tauri::command]
pub async fn export_packages(state: State<'_, AppState>) -> Result<ExportResult, AppError> {
    run_export_packages(&state).await
}

#[tauri::command]
pub fn open_exports_folder() -> Result<(), AppError> {
    run_open_exports_folder()
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
