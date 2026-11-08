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

/// Options for `install`. `Default` means "latest version, winget's default scope".
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InstallRequest {
    pub version: Option<String>,
    pub scope: Option<InstallScope>,
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
    fn remove<'a>(&'a self, package: &'a PackageId) -> ExecFuture<'a, OperationHandle>;
    fn list_installed<'a>(&'a self) -> ExecFuture<'a, Vec<InstalledPackage>>;
    fn list_updates<'a>(&'a self) -> ExecFuture<'a, Vec<UpdateAvailable>>;
    fn update<'a>(&'a self, package: &'a PackageId) -> ExecFuture<'a, OperationHandle>;
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
