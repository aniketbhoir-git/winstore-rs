//! Install / uninstall / cancel lifecycle (SPEC §6.2, WP5).
//!
//! Every operation follows one path: check the ledger, start the executor, record the busy
//! state, relay progress, then settle the ledger from the outcome. The `installs` table is the
//! persisted `InstallState`; `ActiveOperations` says which operation runs for a package now.
//!
//! Settled states:
//!
//! | operation        | success        | cancelled                     | failed                        |
//! |------------------|----------------|-------------------------------|-------------------------------|
//! | install / update | `Installed`    | prior version ? `Installed` : `NotInstalled` | prior version ? `Installed` : `Failed` |
//! | uninstall        | `NotInstalled` | rest state                    | `Failed` (version kept)       |
//!
//! The rest state is `Installed` (`Pinned` with the pin flag) when a version is recorded and
//! `NotInstalled` when not. An `agreements_required` ending of any operation also goes to the
//! rest state (SPEC §6.6 case c: the app never passes an agreement flag), not to `Failed`.
//!
//! `target_version` is cleared whenever an operation ends, so a cancelled install leaves no
//! trace of the attempt in the ledger (the operation log keeps the history).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

use serde::Serialize;

use super::consent::Consent;
use crate::db::repo::{installs, operation_log};
use crate::db::DbPool;
use crate::domain::{
    now_unix, InstallRecord, InstallState, LogLevel, Operation, OperationLogEntry, PackageId,
};
use crate::error::{AgreementEntry, AppError, AppResult};
use crate::executor::winget_cli::winget_target;
use crate::executor::{InstallRequest, OperationHandle, RemoveRequest};
use crate::ipc::events::{
    self, emit, error_json, OperationFinishedEvent, SharedSink, EVENT_OPERATION_FINISHED,
};
use crate::ipc::state::{Core, SharedExecutor};

/// Upper bound for one log tail request.
pub const LOG_TAIL_MAX: u32 = 500;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct OperationStarted {
    pub operation_id: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ActiveOperation {
    pub operation_id: String,
    pub operation: Operation,
    /// The executor reported `cancel_failed`: the installer tree cannot be stopped. A reloaded
    /// view reads this flag, because the event is sent once.
    pub cancel_failed: bool,
}

/// Operations running in this process, by package. Lets a reloaded view find (and cancel)
/// an operation it did not start.
#[derive(Debug, Default)]
pub struct ActiveOperations(Mutex<HashMap<PackageId, ActiveOperation>>);

impl ActiveOperations {
    fn lock(&self) -> MutexGuard<'_, HashMap<PackageId, ActiveOperation>> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn insert(&self, id: &PackageId, operation_id: &str, operation: Operation) {
        self.lock().insert(
            id.clone(),
            ActiveOperation {
                operation_id: operation_id.to_string(),
                operation,
                cancel_failed: false,
            },
        );
    }

    /// Remembers that cancel failed, only while the entry still belongs to `operation_id`.
    fn mark_cancel_failed(&self, id: &PackageId, operation_id: &str) {
        if let Some(a) = self.lock().get_mut(id) {
            if a.operation_id == operation_id {
                a.cancel_failed = true;
            }
        }
    }

    /// Removes the entry only while it still belongs to `operation_id`.
    fn finish(&self, id: &PackageId, operation_id: &str) {
        let mut map = self.lock();
        if map.get(id).is_some_and(|a| a.operation_id == operation_id) {
            map.remove(id);
        }
    }

    pub fn get(&self, id: &PackageId) -> Option<ActiveOperation> {
        self.lock().get(id).cloned()
    }
}

/// What the detail page needs to choose its actions.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct InstallStatus {
    pub package_id: PackageId,
    pub state: InstallState,
    pub installed_version: Option<String>,
    pub target_version: Option<String>,
    pub pinned: bool,
    pub last_updated: Option<i64>,
    pub active: Option<ActiveOperation>,
}

// ---------------------------------------------------------------------------------------------
// Public entry points
// ---------------------------------------------------------------------------------------------

/// SPEC §6.2: starts an install from an explicit user click. Paid items have no in-app
/// install path (SPEC §0 rule 1) and are refused here, not only hidden in the UI.
pub async fn start_install(
    core: &Arc<Core>,
    sink: SharedSink,
    id: PackageId,
    request: InstallRequest,
) -> AppResult<OperationStarted> {
    start_install_with_consent(core, sink, id, request, None).await
}

/// `start_install` with the `options.consent` of the UI (SPEC §6.6). The consent is checked
/// against the open challenges of this package, version and winget source; whatever
/// `request.consent` held on entry is replaced by the result of that check.
pub async fn start_install_with_consent(
    core: &Arc<Core>,
    sink: SharedSink,
    id: PackageId,
    mut request: InstallRequest,
    consent: Option<&Consent>,
) -> AppResult<OperationStarted> {
    let detail = core.catalog.get_detail(&id).await?;
    if detail.summary.is_paid {
        return Err(AppError::Unsupported(
            "paid items cannot be installed here; use \"Buy in Microsoft Store\"".into(),
        ));
    }
    check_ledger(core, &id, Operation::Install).await?;
    request.consent = core.consent.verify(
        &id,
        Operation::Install,
        request.version.as_deref(),
        winget_target(&id)?.1,
        consent,
    )?;
    let target = request
        .version
        .clone()
        .or_else(|| Some(detail.summary.latest_version.clone()).filter(|v| !v.is_empty()));

    let executor = core.executor().await?;
    // The executor's per-package guard rejects a second operation before anything is written.
    let handle = executor.install(&id, &request).await?;
    launch(
        core,
        sink,
        &executor,
        handle,
        id,
        Operation::Install,
        target,
    )
    .await
}

/// Uninstalls a package this app has recorded as installed.
pub async fn start_uninstall(
    core: &Arc<Core>,
    sink: SharedSink,
    id: PackageId,
) -> AppResult<OperationStarted> {
    start_uninstall_with_consent(core, sink, id, None).await
}

/// `start_uninstall` with a consent of the kind `"source"` only (spike question 3).
pub async fn start_uninstall_with_consent(
    core: &Arc<Core>,
    sink: SharedSink,
    id: PackageId,
    consent: Option<&Consent>,
) -> AppResult<OperationStarted> {
    check_ledger(core, &id, Operation::Remove).await?;
    let request = RemoveRequest {
        consent: core.consent.verify(
            &id,
            Operation::Remove,
            None,
            winget_target(&id)?.1,
            consent,
        )?,
    };
    let executor = core.executor().await?;
    let handle = executor.remove(&id, &request).await?;
    launch(core, sink, &executor, handle, id, Operation::Remove, None).await
}

/// Cancels a running install / uninstall / update. Unknown or finished ids are a no-op; the
/// `store:operation-finished` event reports the real outcome.
pub async fn cancel(core: &Core, operation_id: &str) -> AppResult<()> {
    core.executor().await?.cancel(operation_id).await
}

/// Ledger state plus the running operation, if any. A package with no ledger row is
/// `NotInstalled`.
pub async fn status(core: &Core, id: &PackageId) -> AppResult<InstallStatus> {
    let rec = match installs::get(&core.pool, id).await? {
        Some(r) => r,
        None => {
            let (kind, _) = PackageId::parse(id.as_str())?;
            InstallRecord::new(id.clone(), kind)
        }
    };
    Ok(InstallStatus {
        active: core.active.get(id),
        package_id: rec.package_id,
        state: rec.state,
        installed_version: rec.installed_version,
        target_version: rec.target_version,
        pinned: rec.pinned,
        last_updated: rec.last_updated,
    })
}

/// The newest `limit` operation log entries for a package, oldest first (tail order).
pub async fn log_tail(
    pool: &DbPool,
    id: &PackageId,
    limit: u32,
) -> AppResult<Vec<OperationLogEntry>> {
    let mut rows = operation_log::for_package(pool, id, limit.clamp(1, LOG_TAIL_MAX)).await?;
    rows.reverse();
    Ok(rows)
}

/// Settles ledger rows left busy by a previous run (crash or forced exit). Nothing runs at
/// start-up, so a busy row is stale. Its real outcome is unknown, so it becomes `Failed`
/// (the user can retry), except an interrupted update or reinstall, which keeps the version
/// that was already there. Returns the number of rows fixed.
pub async fn recover_interrupted(pool: &DbPool) -> AppResult<usize> {
    let mut fixed = 0;
    for busy in [InstallState::Installing, InstallState::Uninstalling] {
        for mut rec in installs::list_by_state(pool, busy).await? {
            let next = if busy == InstallState::Installing && rec.installed_version.is_some() {
                InstallState::Installed
            } else {
                InstallState::Failed
            };
            rec.state = rec.state.transition(next)?;
            rec.target_version = None;
            installs::upsert(pool, &rec).await?;
            let op = if busy == InstallState::Installing {
                Operation::Install
            } else {
                Operation::Remove
            };
            append_log(
                pool,
                LogLevel::Warn,
                op,
                &rec.package_id,
                &format!("{op} was interrupted; marked {next}"),
            )
            .await;
            fixed += 1;
        }
    }
    Ok(fixed)
}

// ---------------------------------------------------------------------------------------------
// Lifecycle building blocks (also used by `update_all`)
// ---------------------------------------------------------------------------------------------

fn busy_state(op: Operation) -> InstallState {
    match op {
        Operation::Remove => InstallState::Uninstalling,
        Operation::Install | Operation::Update => InstallState::Installing,
    }
}

/// Refuses an operation the ledger cannot take, before any process starts.
pub(crate) async fn check_ledger(core: &Core, id: &PackageId, op: Operation) -> AppResult<()> {
    if let Some(a) = core.active.get(id) {
        return Err(AppError::Unsupported(format!(
            "another operation ({}) is already running for {id}",
            a.operation
        )));
    }
    match installs::get(&core.pool, id).await? {
        None if op == Operation::Remove => Err(AppError::Unsupported(format!(
            "{id} is not recorded as installed by this app"
        ))),
        None => Ok(()),
        Some(rec) => {
            let busy = busy_state(op);
            // A leftover `Installing` row is accepted (see `begin`).
            if rec.state == busy && busy == InstallState::Installing {
                return Ok(());
            }
            rec.state.transition(busy).map(|_| ())
        }
    }
}

/// Records the busy state in the ledger. A leftover `Installing` row is accepted: the executor
/// guard already ensured nothing is running for this package. `installed_version` seeds a row
/// for an app that was installed outside this app, so a failed or cancelled update keeps it
/// `Installed`.
pub(crate) async fn begin(
    core: &Core,
    id: &PackageId,
    op: Operation,
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
    let busy = busy_state(op);
    if !(rec.state == busy && busy == InstallState::Installing) {
        rec.state = rec.state.transition(busy)?;
    }
    rec.target_version = target;
    installs::upsert(&core.pool, &rec).await
}

/// Registers a started operation so `status` can report it.
pub(crate) fn track(core: &Core, id: &PackageId, handle: &OperationHandle, op: Operation) {
    core.active.insert(id, &handle.operation_id, op);
}

/// Writes the busy state, then relays the operation in the background. If the ledger refuses,
/// the already running process is cancelled and drained so nothing keeps running unseen.
async fn launch(
    core: &Arc<Core>,
    sink: SharedSink,
    executor: &SharedExecutor,
    handle: OperationHandle,
    id: PackageId,
    op: Operation,
    target: Option<String>,
) -> AppResult<OperationStarted> {
    if let Err(e) = begin(core, &id, op, target.clone(), None).await {
        let _ = executor.cancel(&handle.operation_id).await;
        let err = error_json(&e);
        tokio::spawn(async move { abandon(&sink, handle, &id, err).await });
        return Err(e);
    }
    track(core, &id, &handle, op);
    let started = OperationStarted {
        operation_id: handle.operation_id.clone(),
    };
    let core = Arc::clone(core);
    tokio::spawn(async move {
        let _ = drive(&core, &sink, handle, &id, op, target).await;
    });
    Ok(started)
}

/// Relays progress events, waits for the outcome, settles the ledger and emits
/// `store:operation-finished`.
pub(crate) async fn drive(
    core: &Core,
    sink: &SharedSink,
    mut handle: OperationHandle,
    id: &PackageId,
    op: Operation,
    target: Option<String>,
) -> AppResult<()> {
    while let Some(ev) = handle.events.recv().await {
        if ev.cancel_failed {
            core.active.mark_cancel_failed(id, &handle.operation_id);
        }
        events::emit_progress(sink.as_ref(), &ev);
    }
    let mut result = match handle.outcome.await {
        Ok(r) => r,
        Err(_) => Err(AppError::DeploymentFailed(
            "the operation ended without a result".into(),
        )),
    };
    // SPEC §1.3 `version`: when the request gave winget no version, the target stands in.
    match &mut result {
        Err(AppError::AgreementsRequired {
            source_agreements,
            version,
            winget_source,
            agreement_url,
            agreements,
            consent_token,
            ..
        }) => {
            if version.is_none() {
                version.clone_from(&target);
            }
            // An update takes no consent (SPEC §6.6), so it gets no token.
            if op != Operation::Update {
                // A package agreement with no link of its own falls back to the license link
                // of the cached detail (SPEC §1.3).
                if agreement_url.is_none() && !*source_agreements {
                    let license = core.catalog.get_detail(id).await.ok();
                    if let Some(url) = license.and_then(|d| d.license_url) {
                        agreements.push(AgreementEntry {
                            label: "License".into(),
                            text: None,
                            url: Some(url.clone()),
                        });
                        *agreement_url = Some(url);
                    }
                }
                let urls = agreements.iter().filter_map(|a| a.url.clone()).collect();
                *consent_token = Some(core.consent.issue(
                    id,
                    op,
                    *source_agreements,
                    version.clone(),
                    winget_source.as_deref().unwrap_or_default(),
                    urls,
                ));
            }
        }
        // Any other end closes the attempt: no consent outlives it.
        _ => core.consent.clear(id),
    }
    let target = if result.is_ok() && op != Operation::Remove {
        installed_version_or(core, id, op, target).await
    } else {
        target
    };
    if let Err(e) = settle(&core.pool, id, op, target, &result).await {
        tracing::error!(package = %id, "could not update install ledger: {e}");
    }
    core.active.finish(id, &handle.operation_id);
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
    super::notify::operation_finished(core, id, op, &result).await;
    result
}

/// SPEC §6.7 "Known gaps": after a successful install or update, the version that `winget list`
/// reports is the one to record. The catalog target stands in (and one Warn row says why) when
/// the list call fails or has no row for the package. A difference between the two is logged
/// as a Warn row too.
async fn installed_version_or(
    core: &Core,
    id: &PackageId,
    op: Operation,
    target: Option<String>,
) -> Option<String> {
    let native = PackageId::parse(id.as_str()).map(|(_, n)| n).ok();
    let listed = match (core.executor().await, &native) {
        (Ok(executor), Some(n)) => executor.list_installed_one(n).await,
        (Ok(_), None) => Ok(Vec::new()),
        (Err(e), _) => Err(e),
    };
    let found = match (&listed, &native) {
        (Ok(rows), Some(n)) => rows
            .iter()
            .find(|r| r.id.eq_ignore_ascii_case(n) && usable_version(&r.version))
            .map(|r| r.version.trim().to_string()),
        _ => None,
    };
    match found {
        Some(v) => {
            if target.as_deref().is_some_and(|t| t != v) {
                let msg = format!(
                    "installed version {v} differs from the catalog version {}; recorded {v}",
                    target.as_deref().unwrap_or_default()
                );
                append_log(&core.pool, LogLevel::Warn, op, id, &msg).await;
            }
            Some(v)
        }
        None => {
            let why = match &listed {
                Err(e) => format!("winget list failed ({e})"),
                Ok(_) => "winget list has no row for the package".to_string(),
            };
            let msg = format!(
                "{why}; recorded the catalog version {}",
                target.as_deref().unwrap_or("(none)")
            );
            append_log(&core.pool, LogLevel::Warn, op, id, &msg).await;
            target
        }
    }
}

/// `winget list` prints `Unknown` (any case) or a range such as `< 1.0` when it cannot tell the
/// version. Those are not versions: the row counts as missing.
fn usable_version(v: &str) -> bool {
    let v = v.trim();
    !v.is_empty() && !v.eq_ignore_ascii_case("unknown") && !v.starts_with(['<', '>'])
}

/// Drains a started operation that the ledger refused: progress is still relayed, the outcome
/// is awaited, and the finish event carries the refusal. The ledger is not written.
pub(crate) async fn abandon(
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

/// Moves the ledger row out of its busy state (see the table in the module docs).
pub(crate) async fn settle(
    pool: &DbPool,
    id: &PackageId,
    op: Operation,
    target: Option<String>,
    result: &AppResult<()>,
) -> AppResult<()> {
    let Some(mut rec) = installs::get(pool, id).await? else {
        return Ok(());
    };
    let had_version = rec.installed_version.is_some();
    rec.target_version = None;
    let next = match (op, result) {
        (Operation::Remove, Ok(())) => {
            rec.installed_version = None;
            rec.pinned = false;
            rec.last_updated = Some(now_unix());
            InstallState::NotInstalled
        }
        // The rest state: an agreement stop (any operation) and a cancelled uninstall.
        (_, Err(AppError::AgreementsRequired { .. }))
        | (Operation::Remove, Err(AppError::Cancelled)) => {
            if had_version {
                InstallState::Installed
            } else {
                InstallState::NotInstalled
            }
        }
        (Operation::Remove, Err(_)) => InstallState::Failed,
        (_, Ok(())) => {
            rec.installed_version = target.or(rec.installed_version.take());
            rec.last_updated = Some(now_unix());
            InstallState::Installed
        }
        // A failed or cancelled install/update leaves an already installed version in place.
        (_, Err(_)) if had_version => InstallState::Installed,
        (_, Err(AppError::Cancelled)) => InstallState::NotInstalled,
        (_, Err(_)) => InstallState::Failed,
    };
    rec.state = rec.state.transition(next)?;
    // A pin outlives a cancelled uninstall.
    if rec.state == InstallState::Installed && rec.pinned {
        rec.state = rec.state.transition(InstallState::Pinned)?;
    }
    installs::upsert(pool, &rec).await
}

async fn append_log(pool: &DbPool, level: LogLevel, op: Operation, id: &PackageId, msg: &str) {
    let entry = OperationLogEntry {
        id: None,
        ts: now_unix(),
        level,
        operation: Some(op),
        package_id: Some(id.clone()),
        message: msg.to_string(),
        detail: None,
    };
    if let Err(e) = operation_log::append(pool, &entry).await {
        tracing::warn!("operation_log write failed: {e}");
    }
}

#[cfg(test)]
mod tests;
