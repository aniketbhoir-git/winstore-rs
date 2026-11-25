//! Package executors (SPEC §5.2): one implementation per packaging back end, registered per
//! source. The trait is object-safe (`Box<dyn PackageExecutor>`), so async methods return
//! manually boxed futures instead of using `async fn` in traits.

pub mod winget_cli;
pub mod winget_parse;

use std::collections::HashSet;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use tokio::sync::{mpsc, oneshot};

use crate::domain::{PackageId, ProgressEvent, SourceKind, UpdateAvailable};
use crate::error::{AppError, AppResult};

/// Boxed future returned by every async executor method.
pub type ExecFuture<'a, T> = Pin<Box<dyn Future<Output = AppResult<T>> + Send + 'a>>;

/// A running install / remove / update.
///
/// `events` ends with exactly one terminal event (`Done`, `Failed` or `Cancelled`) and then
/// closes. `outcome` carries the final result (including the typed `AppError`) once the
/// terminal event has been sent.
#[derive(Debug)]
pub struct OperationHandle {
    pub operation_id: String,
    pub events: mpsc::Receiver<ProgressEvent>,
    pub outcome: oneshot::Receiver<AppResult<()>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallScope {
    User,
    Machine,
}

impl InstallScope {
    pub fn as_str(self) -> &'static str {
        match self {
            InstallScope::User => "user",
            InstallScope::Machine => "machine",
        }
    }
}

/// The agreement kinds that the user accepted for ONE operation, already checked against the
/// challenge that winget raised (`services::consent`, SPEC §6.6). Only that check should
/// build one. It is the one value that lets an agreement flag reach winget (see
/// `winget_cli::guard_args_consented`); no config, setting or stored value can make one.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VerifiedConsent {
    source: bool,
    package: bool,
}

impl VerifiedConsent {
    pub fn new(source: bool, package: bool) -> Self {
        VerifiedConsent { source, package }
    }
    pub fn source(&self) -> bool {
        self.source
    }
    pub fn package(&self) -> bool {
        self.package
    }
    pub fn is_empty(&self) -> bool {
        !self.source && !self.package
    }
}

/// Options for `install`. `Default` means "latest version, winget's default scope, no
/// agreement flag".
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InstallRequest {
    pub version: Option<String>,
    pub scope: Option<InstallScope>,
    /// Set only by `services/install.rs` after the consent check; never by the IPC layer.
    pub consent: Option<VerifiedConsent>,
}

/// Options for `remove`. `winget uninstall` accepts the source agreement flag only (spike
/// question 3), so `consent.package()` is never passed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RemoveRequest {
    pub consent: Option<VerifiedConsent>,
}

/// One row of `winget list`. `source` is `None` for apps winget cannot match to a catalog
/// (for example `ARP\...` entries).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledPackage {
    pub id: String,
    pub name: String,
    pub version: String,
    pub available: Option<String>,
    pub source: Option<SourceKind>,
}

pub trait PackageExecutor: Send + Sync {
    fn install<'a>(
        &'a self,
        package: &'a PackageId,
        request: &'a InstallRequest,
    ) -> ExecFuture<'a, OperationHandle>;
    fn cancel<'a>(&'a self, operation_id: &'a str) -> ExecFuture<'a, ()>;
    fn remove<'a>(
        &'a self,
        package: &'a PackageId,
        request: &'a RemoveRequest,
    ) -> ExecFuture<'a, OperationHandle>;
    fn list_installed<'a>(&'a self) -> ExecFuture<'a, Vec<InstalledPackage>>;
    /// `winget list` narrowed to one native package id (30 s limit); the full list is for the Library.
    fn list_installed_one<'a>(&'a self, id: &'a str) -> ExecFuture<'a, Vec<InstalledPackage>>;
    fn list_updates<'a>(&'a self) -> ExecFuture<'a, Vec<UpdateAvailable>>;
    fn update<'a>(&'a self, package: &'a PackageId) -> ExecFuture<'a, OperationHandle>;
    /// `winget export` into `path` (hardening design §4). The default refuses, so a test
    /// double that does not export needs no code.
    fn export_installed<'a>(&'a self, _path: &'a std::path::Path) -> ExecFuture<'a, ()> {
        Box::pin(async { Err(AppError::Unsupported("export is not available".into())) })
    }
    fn pin<'a>(&'a self, package: &'a PackageId) -> ExecFuture<'a, ()>;
    fn unpin<'a>(&'a self, package: &'a PackageId) -> ExecFuture<'a, ()>;
    fn is_available<'a>(&'a self) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>>;
}

/// At most one mutating operation per package id (SPEC §1.2). A second attempt fails fast
/// instead of queueing, so a double click cannot start two installs.
#[derive(Debug, Clone, Default)]
pub struct OpGuard {
    busy: Arc<Mutex<HashSet<PackageId>>>,
}

/// Held for the lifetime of an operation; releases the package on drop.
#[derive(Debug)]
pub struct OpPermit {
    busy: Arc<Mutex<HashSet<PackageId>>>,
    id: PackageId,
}

impl OpGuard {
    pub fn try_acquire(&self, id: &PackageId) -> AppResult<OpPermit> {
        let mut set = self.busy.lock().unwrap_or_else(|e| e.into_inner());
        if !set.insert(id.clone()) {
            return Err(AppError::Unsupported(format!(
                "another operation is already running for {id}"
            )));
        }
        Ok(OpPermit {
            busy: Arc::clone(&self.busy),
            id: id.clone(),
        })
    }

    pub fn is_busy(&self, id: &PackageId) -> bool {
        self.busy
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains(id)
    }
}

impl Drop for OpPermit {
    fn drop(&mut self) {
        self.busy
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pid(s: &str) -> PackageId {
        PackageId::new(SourceKind::WingetCommunity, s)
    }

    #[test]
    fn guard_is_exclusive_per_package_and_releases_on_drop() {
        let g = OpGuard::default();
        let a = g.try_acquire(&pid("A.A")).unwrap();
        assert!(g.is_busy(&pid("A.A")));
        let err = g.try_acquire(&pid("A.A")).unwrap_err();
        assert_eq!(err.code(), "unsupported");
        // a different package is independent
        let _b = g.try_acquire(&pid("B.B")).unwrap();
        drop(a);
        assert!(!g.is_busy(&pid("A.A")));
        g.try_acquire(&pid("A.A")).unwrap();
    }

    #[test]
    fn trait_is_object_safe() {
        fn _takes(_: &dyn PackageExecutor) {}
        fn _boxed(_: Box<dyn PackageExecutor>) {}
    }
}
