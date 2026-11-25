//! Export tests against the fake winget (hardening design §4). Real winget is never started.

mod common;
use common::fake_exe;

use store_lib::error::AppError;
use store_lib::executor::winget_cli::{WingetCli, WingetOptions};
use store_lib::services::export::{export_installed, export_with_temp};

const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/winget/1.29/export-small.json"
);

struct Fx {
    dir: tempfile::TempDir,
    cli: WingetCli,
}

impl Fx {
    fn new(script: &str) -> Fx {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("script.txt"), script).unwrap();
        let env = vec![
            (
                "FAKE_WINGET_SCRIPT".to_string(),
                dir.path().join("script.txt").display().to_string(),
            ),
            (
                "FAKE_WINGET_ARGV".to_string(),
                dir.path().join("argv.txt").display().to_string(),
            ),
        ];
        let cli = WingetCli::new(
            fake_exe(),
            WingetOptions {
                log_pool: None,
                env,
            },
        );
        Fx { dir, cli }
    }

    fn argv(&self) -> String {
        std::fs::read_to_string(self.dir.path().join("argv.txt")).unwrap_or_default()
    }

    fn exports(&self) -> std::path::PathBuf {
        self.dir.path().join("exports")
    }
}

fn names(dir: &std::path::Path) -> Vec<String> {
    std::fs::read_dir(dir)
        .map(|r| {
            r.map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default()
}

#[tokio::test]
async fn export_copies_the_file_to_the_exports_folder() {
    let fx = Fx::new(&format!("outfile {FIXTURE}\nexit 0\n"));
    let r = export_installed(&fx.cli, &fx.exports(), "2026-11-25")
        .await
        .unwrap();
    assert_eq!(r.file_name, "store-export-2026-11-25.json");
    assert_eq!(r.package_count, 3);
    assert_eq!(
        std::fs::read(&r.path).unwrap(),
        std::fs::read(FIXTURE).unwrap()
    );
    let argv = fx.argv();
    assert!(argv.starts_with("export -o "), "{argv}");
    assert!(argv.contains("--source winget --disable-interactivity"));
    assert!(!argv.contains("accept"), "no agreement flag: {argv}");
}

#[tokio::test]
async fn export_bad_file_is_winget_failed_and_keeps_no_file() {
    let dir = tempfile::tempdir().unwrap();
    let bad = dir.path().join("bad.json");
    std::fs::write(&bad, "this is not json").unwrap();
    let fx = Fx::new(&format!("outfile {}\nexit 0\n", bad.display()));
    let err = export_installed(&fx.cli, &fx.exports(), "2026-11-25")
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::WingetFailed { .. }), "{err:?}");
    assert!(names(&fx.exports()).is_empty());
}

#[tokio::test]
async fn export_no_output_file_is_winget_failed() {
    let fx = Fx::new("exit 0\n");
    let err = export_installed(&fx.cli, &fx.exports(), "2026-11-25")
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::WingetFailed { .. }), "{err:?}");
    assert!(names(&fx.exports()).is_empty());
}

#[tokio::test]
async fn export_source_agreement_stops_it_and_writes_nothing() {
    let fx = Fx::new("exit 0x8A150046\n");
    let err = export_installed(&fx.cli, &fx.exports(), "2026-11-25")
        .await
        .unwrap_err();
    assert!(
        matches!(err, AppError::AgreementsRequired { .. }),
        "{err:?}"
    );
    assert!(names(&fx.exports()).is_empty());
    assert!(!fx.argv().contains("accept"));
}

/// Runs the export with a temp file inside `fx.dir`; returns the result and that temp path.
async fn run_with_temp(
    fx: &Fx,
) -> (
    store_lib::error::AppResult<store_lib::services::export::ExportResult>,
    std::path::PathBuf,
) {
    let temp = fx.dir.path().join("tmp-export.json");
    let r = export_with_temp(&fx.cli, &temp, &fx.exports(), "2026-11-25").await;
    (r, temp)
}

#[tokio::test]
async fn temp_file_is_removed_on_success() {
    let fx = Fx::new(&format!(
        "outfile {FIXTURE}
exit 0
"
    ));
    let (r, temp) = run_with_temp(&fx).await;
    r.unwrap();
    assert!(!temp.exists());
    // Only the final file is in the folder: the atomic temp file is gone.
    assert_eq!(names(&fx.exports()), vec!["store-export-2026-11-25.json"]);
}

#[tokio::test]
async fn a_second_export_replaces_the_file_atomically() {
    let fx = Fx::new(&format!(
        "outfile {FIXTURE}
exit 0
"
    ));
    std::fs::create_dir_all(fx.exports()).unwrap();
    std::fs::write(fx.exports().join("store-export-2026-11-25.json"), "old").unwrap();
    let (r, _) = run_with_temp(&fx).await;
    let r = r.unwrap();
    assert_eq!(
        std::fs::read(&r.path).unwrap(),
        std::fs::read(FIXTURE).unwrap()
    );
    assert_eq!(names(&fx.exports()).len(), 1);
}

#[tokio::test]
async fn temp_file_is_removed_when_the_file_does_not_parse() {
    let dir = tempfile::tempdir().unwrap();
    let bad = dir.path().join("bad.json");
    std::fs::write(&bad, "this is not json").unwrap();
    let fx = Fx::new(&format!(
        "outfile {}
exit 0
",
        bad.display()
    ));
    let (r, temp) = run_with_temp(&fx).await;
    assert!(matches!(r, Err(AppError::WingetFailed { .. })));
    assert!(!temp.exists());
    assert!(names(&fx.exports()).is_empty());
}

#[tokio::test]
async fn winget_writes_a_file_then_exits_non_zero_keeps_nothing() {
    let fx = Fx::new(&format!(
        "outfile {FIXTURE}
exit 1
"
    ));
    let (r, temp) = run_with_temp(&fx).await;
    assert!(r.is_err());
    assert!(!temp.exists(), "the temp file must be removed");
    assert!(names(&fx.exports()).is_empty());
}

#[tokio::test]
async fn an_oversized_temp_file_is_refused_and_removed() {
    let dir = tempfile::tempdir().unwrap();
    let big = dir.path().join("big.json");
    std::fs::write(&big, vec![b' '; 1024 * 1024 + 1]).unwrap();
    let fx = Fx::new(&format!(
        "outfile {}
exit 0
",
        big.display()
    ));
    let (r, temp) = run_with_temp(&fx).await;
    assert!(matches!(r, Err(AppError::WingetFailed { .. })), "{r:?}");
    assert!(!temp.exists());
    assert!(names(&fx.exports()).is_empty());
}

#[tokio::test]
async fn temp_file_is_removed_on_the_source_agreement_path() {
    let fx = Fx::new(
        "exit 0x8A150046
",
    );
    let (r, temp) = run_with_temp(&fx).await;
    assert!(matches!(r, Err(AppError::AgreementsRequired { .. })));
    assert!(!temp.exists());
}
