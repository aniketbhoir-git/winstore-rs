//! Parsers read the real winget 1.29.380 output recorded on a GitHub-hosted runner (see
//! `tests/fixtures/winget/1.29/README.md`). The fake winget replays the files; real winget
//! never starts here.

mod common;
use common::fake_exe;

use store_lib::domain::{PackageId, SourceKind};
use store_lib::executor::winget_cli::{WingetCli, WingetOptions};
use store_lib::executor::PackageExecutor;
use store_lib::services::import::parse_import;

fn pkg(id: &str) -> PackageId {
    PackageId::new(SourceKind::WingetCommunity, id)
}

fn fixture(name: &str) -> String {
    format!(
        "{}/tests/fixtures/winget/1.29/{name}",
        env!("CARGO_MANIFEST_DIR")
    )
}

#[tokio::test]
async fn list_updates_reads_the_real_upgrade_table() {
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("script.txt");
    std::fs::write(&script, format!("file {}\n", fixture("upgrade-real.txt"))).unwrap();
    let env = vec![(
        "FAKE_WINGET_SCRIPT".to_string(),
        script.display().to_string(),
    )];
    let cli = WingetCli::new(
        fake_exe(),
        WingetOptions {
            log_pool: None,
            env,
        },
    );
    let ups = cli.list_updates().await.unwrap();
    assert_eq!(ups.len(), 4);
    assert_eq!(ups[0].package_id, pkg("Amazon.AWSCLI"));
    assert_eq!(ups[0].installed_version, "2.40.3.0");
    assert_eq!(ups[0].latest_version, "2.42.2");
    assert_eq!(ups[3].package_id, pkg("Microsoft.WindowsTerminal"));
}

#[test]
fn import_reads_the_real_export_file() {
    let bytes = std::fs::read(fixture("export-real.json")).unwrap();
    let p = parse_import(&bytes).unwrap();
    assert_eq!(p.packages.len(), 3);
    assert_eq!(p.packages[0].source, "winget");
}
