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
//! | uninstall        | `NotInstalled` | `Installed` (or `Pinned`)     | `Failed` (version kept)       |
//!
//! `target_version` is cleared whenever an operation ends, so a cancelled install leaves no
//! trace of the attempt in the ledger (the operation log keeps the history).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

use serde::Serialize;

use crate::db::repo::{installs, operation_log};
use crate::db::DbPool;
use crate::domain::{
    now_unix, InstallRecord, InstallState, LogLevel, Operation, OperationLogEntry, PackageId,
};
use crate::error::{AppError, AppResult};
use crate::executor::{InstallRequest, OperationHandle};
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
            },
        );
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
    let detail = core.catalog.get_detail(&id).await?;
    if detail.summary.is_paid {
        return Err(AppError::Unsupported(
            "paid items cannot be installed here; use \"Buy in Microsoft Store\"".into(),
        ));
    }
    check_ledger(core, &id, Operation::Install).await?;
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
    check_ledger(core, &id, Operation::Remove).await?;
    let executor = core.executor().await?;
    let handle = executor.remove(&id).await?;
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
async fn check_ledger(core: &Core, id: &PackageId, op: Operation) -> AppResult<()> {
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
        events::emit_progress(sink.as_ref(), &ev);
    }
    let result = match handle.outcome.await {
        Ok(r) => r,
        Err(_) => Err(AppError::DeploymentFailed(
            "the operation ended without a result".into(),
        )),
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
    result
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
        (Operation::Remove, Err(AppError::Cancelled)) => InstallState::Installed,
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
