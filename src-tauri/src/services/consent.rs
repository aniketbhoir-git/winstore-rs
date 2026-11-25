//! Binds an agreement consent to the exact operation the dialog showed (SPEC §6.6, "How
//! consent is bound").
//!
//! The core cannot see a click. What it can do is refuse a consent that no `agreements_required`
//! asked for. When an install or uninstall ends with `agreements_required`, the service issues a
//! *challenge*: a random token bound to the package id, the version, the winget source and the
//! kind of agreement, held in memory only. A later `consent` carries `{ kinds, tokens }`. The
//! check passes a kind only when one of the tokens names a live challenge of that kind for this
//! very package, with the same version and winget source, on this very command. Nothing is
//! stored on disk and nothing outlives the process.
//!
//! A challenge lives until the attempt ends: any operation of the package that ends in
//! something other than `agreements_required`, any start with no consent (a new attempt), or
//! `CHALLENGE_TTL`.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use serde::Deserialize;

use crate::domain::{Operation, PackageId};
use crate::error::{AppError, AppResult};
use crate::executor::VerifiedConsent;

/// How long a dialog may stay open before its challenge expires.
pub const CHALLENGE_TTL: Duration = Duration::from_secs(15 * 60);

/// The `options.consent` argument of `start_install` and `start_uninstall` (SPEC §4.2).
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Consent {
    /// `"source"`, `"package"`, or both. An empty list is the same as no consent.
    pub kinds: Vec<String>,
    /// The `consent_token` of each `agreements_required` error that the user accepted.
    pub tokens: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Source,
    Package,
}

#[derive(Debug, Clone)]
struct Challenge {
    token: String,
    /// The command that raised the challenge: a token of an uninstall cannot start an install.
    op: Operation,
    kind: Kind,
    version: Option<String>,
    winget_source: String,
    /// The links that the dialog showed; only these may be opened for this package.
    urls: Vec<String>,
    issued: Instant,
}

/// Open challenges by package. Part of `Core`.
#[derive(Debug, Default)]
pub struct ConsentLedger {
    open: Mutex<HashMap<PackageId, Vec<Challenge>>>,
}

impl ConsentLedger {
    fn lock(&self) -> MutexGuard<'_, HashMap<PackageId, Vec<Challenge>>> {
        self.open.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// 128 random bits from the operating system's CSPRNG, as 32 hex characters.
    fn token(&self) -> String {
        let mut bytes = [0u8; 16];
        getrandom::fill(&mut bytes).expect("the operating system has no random source");
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// Records that winget asked for an agreement and returns the token for the dialog. A new
    /// challenge of the same kind replaces the old one.
    pub fn issue(
        &self,
        id: &PackageId,
        op: Operation,
        source_kind: bool,
        version: Option<String>,
        winget_source: &str,
        urls: Vec<String>,
    ) -> String {
        let kind = if source_kind {
            Kind::Source
        } else {
            Kind::Package
        };
        let token = self.token();
        let mut map = self.lock();
        let list = map.entry(id.clone()).or_default();
        list.retain(|c| c.kind != kind && c.issued.elapsed() < CHALLENGE_TTL);
        list.push(Challenge {
            token: token.clone(),
            op,
            kind,
            version,
            winget_source: winget_source.to_string(),
            urls,
            issued: Instant::now(),
        });
        token
    }

    /// Ends the attempt of a package: drops all its challenges.
    pub fn clear(&self, id: &PackageId) {
        self.lock().remove(id);
    }

    /// Whether `url` is a link of a live challenge for this package. Only such a link may be
    /// opened in the browser (the webview cannot ask for an arbitrary address).
    pub fn link_allowed(&self, id: &PackageId, url: &str) -> bool {
        self.lock().get(id).is_some_and(|list| {
            list.iter()
                .any(|c| c.issued.elapsed() < CHALLENGE_TTL && c.urls.iter().any(|u| u == url))
        })
    }

    /// Checks a consent for one command. `Ok(None)` means no consent (and a new attempt: the
    /// package's challenges are dropped). An error means the consent does not fit the
    /// operation; nothing starts.
    pub fn verify(
        &self,
        id: &PackageId,
        op: Operation,
        version: Option<&str>,
        winget_source: &str,
        consent: Option<&Consent>,
    ) -> AppResult<Option<VerifiedConsent>> {
        let Some(consent) = consent.filter(|c| !c.kinds.is_empty()) else {
            self.clear(id);
            return Ok(None);
        };
        let (mut source, mut package) = (false, false);
        for k in &consent.kinds {
            match k.as_str() {
                "source" => source = true,
                "package" => package = true,
                other => {
                    return Err(AppError::InvalidConfig(format!(
                        "unknown agreement kind: {other}"
                    )))
                }
            }
        }
        match op {
            Operation::Install if package && version.is_none() => {
                return Err(AppError::InvalidConfig(
                    "a package agreement needs the version that the dialog showed".into(),
                ))
            }
            Operation::Remove if package => {
                return Err(AppError::InvalidConfig(
                    "winget uninstall accepts the source agreement only".into(),
                ))
            }
            Operation::Update => {
                return Err(AppError::InvalidConfig("an update takes no consent".into()))
            }
            _ => {}
        }
        let map = self.lock();
        let live: Vec<&Challenge> = map
            .get(id)
            .map(|l| {
                l.iter()
                    .filter(|c| c.issued.elapsed() < CHALLENGE_TTL)
                    .collect()
            })
            .unwrap_or_default();
        let fits = |kind: Kind| {
            live.iter().any(|c| {
                c.kind == kind
                    && c.op == op
                    && c.version.as_deref() == version
                    && c.winget_source == winget_source
                    && consent.tokens.contains(&c.token)
            })
        };
        if (source && !fits(Kind::Source)) || (package && !fits(Kind::Package)) {
            return Err(AppError::InvalidConfig(
                "the consent does not match an agreement that winget asked for this package, \
                 version and source"
                    .into(),
            ));
        }
        Ok(Some(VerifiedConsent::new(source, package)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::SourceKind;

    fn pid(s: &str) -> PackageId {
        PackageId::new(SourceKind::WingetCommunity, s)
    }
    fn consent(kinds: &[&str], tokens: &[&str]) -> Consent {
        Consent {
            kinds: kinds.iter().map(|s| s.to_string()).collect(),
            tokens: tokens.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn consent_is_bound_to_package_version_source_and_kind() {
        let l = ConsentLedger::default();
        let a = pid("A.A");
        let t = l.issue(
            &a,
            Operation::Install,
            false,
            Some("1.0".into()),
            "winget",
            vec![],
        );
        let c = consent(&["package"], &[&t]);
        let ok = l
            .verify(&a, Operation::Install, Some("1.0"), "winget", Some(&c))
            .unwrap();
        assert_eq!(ok, Some(VerifiedConsent::new(false, true)));
        // another package, version, source or kind: refused
        for (id, v, src, kinds) in [
            (pid("B.B"), Some("1.0"), "winget", &["package"][..]),
            (a.clone(), Some("2.0"), "winget", &["package"][..]),
            (a.clone(), Some("1.0"), "msstore", &["package"][..]),
            (a.clone(), Some("1.0"), "winget", &["source"][..]),
            (a.clone(), Some("1.0"), "winget", &["source", "package"][..]),
        ] {
            let c = consent(kinds, &[&t]);
            let err = l
                .verify(&id, Operation::Install, v, src, Some(&c))
                .unwrap_err();
            assert_eq!(err.code(), "invalid_config", "{id} {v:?} {src} {kinds:?}");
        }
    }

    #[test]
    fn no_token_or_a_made_up_token_is_refused() {
        let l = ConsentLedger::default();
        let a = pid("A.A");
        l.issue(&a, Operation::Install, true, None, "winget", vec![]);
        for tokens in [&[][..], &["deadbeef"][..]] {
            let c = consent(&["source"], tokens);
            assert!(l
                .verify(&a, Operation::Install, None, "winget", Some(&c))
                .is_err());
        }
    }

    #[test]
    fn start_with_no_consent_ends_the_attempt() {
        let l = ConsentLedger::default();
        let a = pid("A.A");
        let t = l.issue(&a, Operation::Install, true, None, "winget", vec![]);
        assert_eq!(
            l.verify(&a, Operation::Install, None, "winget", None)
                .unwrap(),
            None
        );
        let c = consent(&["source"], &[&t]);
        assert!(l
            .verify(&a, Operation::Install, None, "winget", Some(&c))
            .is_err());
    }

    #[test]
    fn two_kinds_travel_together_inside_one_attempt() {
        let l = ConsentLedger::default();
        let a = pid("A.A");
        let ts = l.issue(
            &a,
            Operation::Install,
            true,
            Some("1.0".into()),
            "msstore",
            vec![],
        );
        let tp = l.issue(
            &a,
            Operation::Install,
            false,
            Some("1.0".into()),
            "msstore",
            vec![],
        );
        let c = consent(&["source", "package"], &[&ts, &tp]);
        let v = l
            .verify(&a, Operation::Install, Some("1.0"), "msstore", Some(&c))
            .unwrap();
        assert_eq!(v, Some(VerifiedConsent::new(true, true)));
    }

    #[test]
    fn uninstall_takes_the_source_kind_only_and_update_none() {
        let l = ConsentLedger::default();
        let a = pid("A.A");
        let ts = l.issue(&a, Operation::Remove, true, None, "winget", vec![]);
        let tp = l.issue(&a, Operation::Remove, false, None, "winget", vec![]);
        let c = consent(&["package"], &[&tp]);
        assert!(l
            .verify(&a, Operation::Remove, None, "winget", Some(&c))
            .is_err());
        let c = consent(&["source"], &[&ts]);
        assert!(l
            .verify(&a, Operation::Remove, None, "winget", Some(&c))
            .is_ok());
        assert!(l
            .verify(&a, Operation::Update, None, "winget", Some(&c))
            .is_err());
    }

    #[test]
    fn package_kind_needs_a_version_and_unknown_kinds_fail() {
        let l = ConsentLedger::default();
        let a = pid("A.A");
        let t = l.issue(&a, Operation::Install, false, None, "winget", vec![]);
        let c = consent(&["package"], &[&t]);
        assert!(l
            .verify(&a, Operation::Install, None, "winget", Some(&c))
            .is_err());
        let c = consent(&["all"], &[&t]);
        assert!(l
            .verify(&a, Operation::Install, None, "winget", Some(&c))
            .is_err());
    }

    #[test]
    fn only_shown_links_of_this_package_may_open() {
        let l = ConsentLedger::default();
        let a = pid("A.A");
        l.issue(
            &a,
            Operation::Install,
            false,
            None,
            "winget",
            vec!["https://x.example/t".into()],
        );
        assert!(l.link_allowed(&a, "https://x.example/t"));
        assert!(!l.link_allowed(&a, "https://evil.example/"));
        assert!(!l.link_allowed(&pid("B.B"), "https://x.example/t"));
        l.clear(&a);
        assert!(!l.link_allowed(&a, "https://x.example/t"));
    }

    #[test]
    fn tokens_differ_and_are_32_hex_chars() {
        let l = ConsentLedger::default();
        let a = l.issue(
            &pid("A.A"),
            Operation::Install,
            true,
            None,
            "winget",
            vec![],
        );
        let b = l.issue(
            &pid("B.B"),
            Operation::Install,
            true,
            None,
            "winget",
            vec![],
        );
        assert_ne!(a, b);
        for t in [&a, &b] {
            assert_eq!(t.len(), 32);
            assert!(t.chars().all(|c| c.is_ascii_hexdigit()));
        }
    }

    #[test]
    fn an_uninstall_token_cannot_start_an_install() {
        let l = ConsentLedger::default();
        let a = pid("A.A");
        let t = l.issue(&a, Operation::Remove, true, None, "winget", vec![]);
        let c = consent(&["source"], &[&t]);
        assert!(l
            .verify(&a, Operation::Install, None, "winget", Some(&c))
            .is_err());
        assert!(l
            .verify(&a, Operation::Remove, None, "winget", Some(&c))
            .is_ok());
        // and the other way round
        let t = l.issue(&a, Operation::Install, true, None, "winget", vec![]);
        let c = consent(&["source"], &[&t]);
        assert!(l
            .verify(&a, Operation::Remove, None, "winget", Some(&c))
            .is_err());
    }
}
