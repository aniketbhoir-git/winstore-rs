//! Manual smoke test against the REAL winget. Never runs in CI (`#[ignore]`).
//!
//! Installs, then removes, one small package. Requires the owner's approval to run:
//!
//! ```text
//! cd src-tauri
//! cargo test --test winget_smoke -- --ignored --nocapture
//! ```
//!
//! Env: `STORE_SMOKE_PACKAGE` (default `jqlang.jq`, winget source),
//! `STORE_SMOKE_ACCEPT=1` to accept source/package agreements (otherwise the run is expected
//! to stop with `agreements_required`, which is itself a valid result to record).
//! Paste the printed stdout lines into `references/winget-cli.md` as OBSERVED.

use store_lib::domain::{PackageId, SourceKind};
use store_lib::executor::winget_cli::{WingetCli, WingetOptions};
use store_lib::executor::{InstallRequest, OperationHandle, PackageExecutor};

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
#[ignore = "runs real winget install/uninstall; needs owner approval"]
async fn install_then_remove_one_small_package() {
    let accept = std::env::var("STORE_SMOKE_ACCEPT").is_ok_and(|v| v == "1");
    let winget_id = std::env::var("STORE_SMOKE_PACKAGE").unwrap_or_else(|_| "jqlang.jq".into());
    let id = PackageId::new(SourceKind::WingetCommunity, &winget_id);

    let cli = WingetCli::connect(WingetOptions {
        accept_agreements: accept,
        ..Default::default()
    })
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
    let removed = run("remove", cli.remove(&id).await.unwrap()).await;
    println!("remove outcome: {removed:?}");

    if accept {
        installed.expect("install failed");
        removed.expect("remove failed");
    }
}
