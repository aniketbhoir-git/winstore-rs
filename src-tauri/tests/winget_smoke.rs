//! Smoke test against the REAL winget. It installs and removes software.
//!
//! Rule (docs/designs/unattended-testing.md): this test runs only on a GitHub-hosted
//! runner. A guard refuses every other machine before any winget process starts. There is
//! no local switch. `#[ignore]` stays, so `cargo test` and `scripts/test.ps1` never run it.
//!
//! Env: `STORE_SMOKE_PACKAGE` (default `jqlang.jq`, winget source).
//! The app never passes an agreement flag (SPEC §6.6 case (c)). A run that stops with
//! `agreements_required` is a valid result to record.

use store_lib::domain::{PackageId, SourceKind};
use store_lib::executor::winget_cli::{WingetCli, WingetOptions};
use store_lib::executor::{InstallRequest, OperationHandle, PackageExecutor, RemoveRequest};

/// Message of a refused run. It names no variable.
const REFUSAL: &str = "This test installs software. It runs only on a GitHub-hosted runner.";

/// Pure guard: `get` looks up an environment value. Allowed only on a GitHub-hosted runner.
fn guard(get: impl Fn(&str) -> Option<String>) -> Result<(), &'static str> {
    let hosted = get("GITHUB_ACTIONS").as_deref() == Some("true")
        && get("RUNNER_ENVIRONMENT").as_deref() == Some("github-hosted");
    if hosted {
        Ok(())
    } else {
        Err(REFUSAL)
    }
}

async fn run(label: &str, h: OperationHandle) -> store_lib::error::AppResult<()> {
    let mut h = h;
    while let Some(e) = h.events.recv().await {
        println!(
            "[{label}] {:>11} {:>5.1}%  {}",
            e.stage.to_string(),
            e.fraction * 100.0,
            e.message
        );
    }
    h.outcome.await.expect("outcome dropped")
}

#[tokio::test]
#[ignore = "runs real winget install/uninstall; real winget: runs only on a GitHub-hosted runner (see guard)"]
async fn install_then_remove_one_small_package() {
    // The guard runs first: a refused run starts no winget process.
    if let Err(msg) = guard(|k| std::env::var(k).ok()) {
        panic!("{msg}");
    }
    let winget_id = std::env::var("STORE_SMOKE_PACKAGE").unwrap_or_else(|_| "jqlang.jq".into());
    let id = PackageId::new(SourceKind::WingetCommunity, &winget_id);

    let cli = WingetCli::connect(WingetOptions::default())
        .await
        .expect("winget not found or too old");
    println!(
        "winget at {} ({})",
        cli.path().display(),
        cli.probe().await.unwrap()
    );

    let installed = run(
        "install",
        cli.install(&id, &InstallRequest::default()).await.unwrap(),
    )
    .await;
    println!("install outcome: {installed:?}");
    let removed = run(
        "remove",
        cli.remove(&id, &RemoveRequest::default()).await.unwrap(),
    )
    .await;
    println!("remove outcome: {removed:?}");

    if !matches!(
        installed,
        Err(store_lib::error::AppError::AgreementsRequired { .. })
    ) {
        installed.expect("install failed");
        removed.expect("remove failed");
    }
}

#[cfg(test)]
mod guard_tests {
    use super::*;

    fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |k| {
            pairs
                .iter()
                .find(|(n, _)| *n == k)
                .map(|(_, v)| (*v).to_string())
        }
    }

    #[test]
    fn allows_only_github_hosted_runner() {
        let ok = [
            ("GITHUB_ACTIONS", "true"),
            ("RUNNER_ENVIRONMENT", "github-hosted"),
        ];
        assert_eq!(guard(env(&ok)), Ok(()));
    }

    #[test]
    fn refuses_with_the_documented_message() {
        let cases: [&[(&str, &str)]; 4] = [
            &[],
            &[("GITHUB_ACTIONS", "true")],
            &[
                ("GITHUB_ACTIONS", "true"),
                ("RUNNER_ENVIRONMENT", "self-hosted"),
            ],
            &[("RUNNER_ENVIRONMENT", "github-hosted")],
        ];
        for c in cases {
            assert_eq!(
                guard(env(c)),
                Err("This test installs software. It runs only on a GitHub-hosted runner.")
            );
        }
        assert!(!REFUSAL.contains("GITHUB_ACTIONS") && !REFUSAL.contains("RUNNER_ENVIRONMENT"));
    }
}
