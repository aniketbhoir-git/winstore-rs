use serde::{Deserialize, Serialize};

use super::package::{PackageId, SourceKind};
use crate::error::{AppError, AppResult};

string_enum!(
    /// Local install ledger state.
    InstallState, "install state", {
        NotInstalled => "not_installed",
        Installing => "installing",
        Installed => "installed",
        UpdateAvailable => "update_available",
        Uninstalling => "uninstalling",
        Failed => "failed",
        Pinned => "pinned",
    }
);

impl InstallState {
    pub fn can_transition_to(self, next: InstallState) -> bool {
        use InstallState::*;
        matches!(
            (self, next),
            (NotInstalled, Installing)
                | (Installing, Installed | Failed | NotInstalled)
                | (
                    Installed,
                    Installing | Uninstalling | UpdateAvailable | Pinned
                )
                | (
                    UpdateAvailable,
                    Installing | Uninstalling | Pinned | Installed
                )
                | (Uninstalling, NotInstalled | Failed)
                | (Failed, Installing | Uninstalling | NotInstalled)
                | (Pinned, Installed | Uninstalling | UpdateAvailable)
        )
    }

    pub fn transition(self, next: InstallState) -> AppResult<InstallState> {
        if self.can_transition_to(next) {
            Ok(next)
        } else {
            Err(AppError::Unsupported(format!(
                "invalid install state transition {self} -> {next}"
            )))
        }
    }

    /// True while an install or uninstall is running.
    pub fn is_busy(self) -> bool {
        matches!(self, InstallState::Installing | InstallState::Uninstalling)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PackageVersion {
    pub version: String,
    pub release_date: Option<String>,
    pub installer_url: Option<String>,
    pub sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InstallRecord {
    pub package_id: PackageId,
    pub source: SourceKind,
    pub state: InstallState,
    pub installed_version: Option<String>,
    pub target_version: Option<String>,
    pub pinned: bool,
    pub auto_update: bool,
    pub last_update_check: Option<i64>,
    pub last_updated: Option<i64>,
}

impl InstallRecord {
    pub fn new(package_id: PackageId, source: SourceKind) -> Self {
        InstallRecord {
            package_id,
            source,
            state: InstallState::NotInstalled,
            installed_version: None,
            target_version: None,
            pinned: false,
            auto_update: true,
            last_update_check: None,
            last_updated: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UpdateAvailable {
    pub package_id: PackageId,
    pub installed_version: String,
    pub latest_version: String,
    pub source: SourceKind,
    pub is_auto: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use InstallState::*;

    fn allowed(from: InstallState) -> &'static [InstallState] {
        match from {
            NotInstalled => &[Installing],
            Installing => &[Installed, Failed, NotInstalled],
            Installed => &[Installing, Uninstalling, UpdateAvailable, Pinned],
            UpdateAvailable => &[Installing, Uninstalling, Pinned, Installed],
            Uninstalling => &[NotInstalled, Failed],
            Failed => &[Installing, Uninstalling, NotInstalled],
            Pinned => &[Installed, Uninstalling, UpdateAvailable],
        }
    }

    #[test]
    fn enum_round_trip() {
        assert_eq!(InstallState::VARIANTS.len(), 7);
        for s in InstallState::VARIANTS {
            assert_eq!(s.to_string().parse::<InstallState>().unwrap(), *s);
            assert_eq!(serde_json::to_value(s).unwrap(), s.to_string());
        }
        assert_eq!(UpdateAvailable.to_string(), "update_available");
        assert!("bogus".parse::<InstallState>().is_err());
    }

    #[test]
    fn full_transition_matrix() {
        for from in InstallState::VARIANTS {
            for to in InstallState::VARIANTS {
                let expect = allowed(*from).contains(to);
                assert_eq!(from.can_transition_to(*to), expect, "{from} -> {to}");
                match from.transition(*to) {
                    Ok(s) => {
                        assert!(expect);
                        assert_eq!(s, *to);
                    }
                    Err(e) => {
                        assert!(!expect);
                        assert_eq!(e.code(), "unsupported");
                        assert!(e.to_string().contains(&format!("{from} -> {to}")));
                    }
                }
            }
        }
    }

    #[test]
    fn no_self_transitions() {
        for s in InstallState::VARIANTS {
            assert!(!s.can_transition_to(*s), "{s}");
        }
    }

    #[test]
    fn busy_states() {
        for s in InstallState::VARIANTS {
            assert_eq!(s.is_busy(), matches!(s, Installing | Uninstalling), "{s}");
        }
    }

    #[test]
    fn new_record_defaults() {
        let id = PackageId::new(SourceKind::Scoop, "x");
        let r = InstallRecord::new(id.clone(), SourceKind::Scoop);
        assert_eq!(r.package_id, id);
        assert_eq!(r.state, NotInstalled);
        assert!(!r.pinned);
        assert!(r.auto_update);
        assert!(r.installed_version.is_none() && r.last_updated.is_none());
    }
}
