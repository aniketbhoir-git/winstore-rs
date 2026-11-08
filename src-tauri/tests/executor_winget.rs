//! Executor tests driven against the fake winget (`tests/fixtures/fake_winget.rs`).
//! Real winget is never started here.

use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Duration;

use store_lib::domain::{
    LogLevel, Operation, OperationStage, PackageId, ProgressEvent, SourceKind,
};
use store_lib::error::{AppError, AppResult};
use store_lib::executor::winget_cli::{WingetCli, WingetOptions};
use store_lib::executor::{InstallRequest, InstallScope, OperationHandle, PackageExecutor};

/// The fake is a dev-only cargo example (never part of the app bundle). Build it
/// once, then locate it next to this test binary (`target/<profile>/deps/..`).
fn fake_exe() -> PathBuf {
    static EXE: OnceLock<PathBuf> = OnceLock::new();
    EXE.get_or_init(|| {
        let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
        let manifest = concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml");
        let mut cmd = std::process::Command::new(cargo);
        cmd.args([
            "build",
            "--example",
            "fake_winget",
            "--manifest-path",
            manifest,
        ]);
        let test_exe = std::env::current_exe().expect("current_exe");
        let profile_dir = test_exe
            .parent()
            .and_then(|d| d.parent())
            .expect("profile dir");
        if profile_dir.file_name().is_some_and(|n| n == "release") {
            cmd.arg("--release");
        }
        let status = cmd.status().expect("run cargo build --example");
        assert!(status.success(), "building fake_winget example failed");
        let exe = profile_dir
            .join("examples")
            .join(format!("fake_winget{}", std::env::consts::EXE_SUFFIX));
        assert!(exe.exists(), "missing {}", exe.display());
        exe
    })
    .clone()
}

struct Fx {
    dir: tempfile::TempDir,
    cli: WingetCli,
}

impl Fx {
    fn new(script: &str) -> Fx {
        Self::build(script, false, &[], None)
    }

    fn accepting(script: &str) -> Fx {
        Self::build(script, true, &[], None)
    }

    fn build(
        script: &str,
        accept: bool,
        extra_env: &[(&str, &str)],
        log_pool: Option<store_lib::db::DbPool>,
    ) -> Fx {
        let dir = tempfile::tempdir().unwrap();
        let script_path = dir.path().join("script.txt");
        std::fs::write(&script_path, script).unwrap();
        let mut env = vec![
            (
                "FAKE_WINGET_SCRIPT".to_string(),
                script_path.display().to_string(),
            ),
            (
                "FAKE_WINGET_ARGV".to_string(),
                dir.path().join("argv.txt").display().to_string(),
            ),
            (
                "FAKE_WINGET_PID".to_string(),
                dir.path().join("pid.txt").display().to_string(),
            ),
        ];
        env.extend(
            extra_env
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string())),
        );
        let cli = WingetCli::new(
            fake_exe(),
            WingetOptions {
                accept_agreements: accept,
                log_pool,
                env,
            },
        );
        Fx { dir, cli }
    }

    fn invocations(&self) -> Vec<Vec<String>> {
        let text = std::fs::read_to_string(self.dir.path().join("argv.txt")).unwrap_or_default();
        text.lines()
            .map(|l| l.split(' ').map(str::to_string).collect())
            .collect()
    }

    fn last_argv(&self) -> Vec<String> {
        self.invocations().pop().expect("fake was never run")
    }
}

fn pkg(id: &str) -> PackageId {
    PackageId::new(SourceKind::WingetCommunity, id)
}

async fn collect(mut h: OperationHandle) -> (Vec<ProgressEvent>, AppResult<()>) {
    let mut events = Vec::new();
    let all = async {
        while let Some(e) = h.events.recv().await {
            events.push(e);
        }
    };
    tokio::time::timeout(Duration::from_secs(30), all)
        .await
        .expect("event stream did not close");
    let outcome = h.outcome.await.expect("outcome dropped");
    (events, outcome)
}

fn assert_one_terminal_last(events: &[ProgressEvent]) {
    let terminals: Vec<_> = events.iter().filter(|e| e.stage.is_terminal()).collect();
    assert_eq!(terminals.len(), 1, "{events:#?}");
    assert!(events.last().unwrap().stage.is_terminal());
}

fn assert_monotonic(events: &[ProgressEvent]) {
    for w in events.windows(2) {
        assert!(w[1].fraction >= w[0].fraction, "{:#?}", w);
    }
}

const FULL_INSTALL: &str = "\
out Found Some App [Vendor.SomeApp] Version 1.2.3
out This application is licensed to you by its owner.
out Microsoft is not responsible for, nor does it grant any licenses to, third-party packages.
out Downloading https://example.invalid/installer.exe
out   ██████▒▒▒▒▒▒▒▒  2.00 MB / 8.00 MB
out   ██████████████  8.00 MB / 8.00 MB
out Successfully verified installer hash
out Starting package install...
out Successfully installed
exit 0
";

#[tokio::test]
async fn install_streams_every_stage_and_finishes_once() {
    let fx = Fx::new(FULL_INSTALL);
    let h = fx
        .cli
        .install(&pkg("Vendor.SomeApp"), &InstallRequest::default())
        .await
        .unwrap();
    assert!(h.operation_id.starts_with("winget-"));
    let (events, outcome) = collect(h).await;
    outcome.unwrap();

    let stages: Vec<_> = events.iter().map(|e| e.stage).collect();
    let mut dedup = stages.clone();
    dedup.dedup();
    use OperationStage::*;
    assert_eq!(
        dedup,
        [Queued, Resolving, Downloading, Installing, Finalizing, Done]
    );
    assert_one_terminal_last(&events);
    assert_monotonic(&events);
    assert_eq!(events.last().unwrap().fraction, 1.0);
    assert!(events.iter().all(|e| e.operation == Operation::Install));
    assert!(events.iter().all(|e| e.package_id == pkg("Vendor.SomeApp")));
    // bar: 2/8 -> 0.10 + 0.5 * 0.25
    assert!(events
        .iter()
        .any(|e| e.stage == Downloading && (e.fraction - 0.225).abs() < 1e-3));

    assert_eq!(
        fx.last_argv(),
        [
            "install",
            "--id",
            "Vendor.SomeApp",
            "--exact",
            "--source",
            "winget",
            "--silent",
            "--disable-interactivity",
            "--verbose-logs"
        ]
    );
}

#[tokio::test]
async fn agreements_flags_follow_config() {
    for accept in [false, true] {
        let fx = Fx::build("exit 0\n", accept, &[], None);
        let req = InstallRequest {
            version: Some("2.0".into()),
            scope: Some(InstallScope::Machine),
        };
        let (_, outcome) = collect(fx.cli.install(&pkg("A.B"), &req).await.unwrap()).await;
        outcome.unwrap();
        let argv = fx.last_argv();
        assert_eq!(
            argv.contains(&"--accept-source-agreements".to_string()),
            accept
        );
        assert_eq!(
            argv.contains(&"--accept-package-agreements".to_string()),
            accept
        );
        assert!(argv.windows(2).any(|w| w == ["--scope", "machine"]));
        assert!(argv.windows(2).any(|w| w == ["--version", "2.0"]));
    }
}

#[tokio::test]
async fn forbidden_flags_never_reach_any_command() {
    let fx = Fx::accepting("exit 0\n");
    let id = pkg("A.B");
    let (_, o) = collect(
        fx.cli
            .install(&id, &InstallRequest::default())
            .await
            .unwrap(),
    )
    .await;
    o.unwrap();
    let (_, o) = collect(fx.cli.update(&id).await.unwrap()).await;
    o.unwrap();
    let (_, o) = collect(fx.cli.remove(&id).await.unwrap()).await;
    o.unwrap();
    fx.cli.pin(&id).await.unwrap();
    fx.cli.unpin(&id).await.unwrap();
    fx.cli.list_installed().await.unwrap();
    fx.cli.list_updates().await.unwrap();
    let all = fx.invocations();
    assert_eq!(all.len(), 7);
    for argv in all {
        assert!(!argv
            .iter()
            .any(|a| a == "--force" || a == "--ignore-security-hash"));
    }
}

#[tokio::test]
async fn update_and_remove_argv_and_operation_kind() {
    let fx = Fx::new("out Found X [A.B]\nout Starting package uninstall...\nout Successfully uninstalled\nexit 0\n");
    let id = pkg("A.B");

    let (events, o) = collect(fx.cli.remove(&id).await.unwrap()).await;
    o.unwrap();
    assert!(events.iter().all(|e| e.operation == Operation::Remove));
    assert!(events.iter().any(|e| e.stage == OperationStage::Finalizing));
    assert_eq!(
        fx.last_argv(),
        [
            "uninstall",
            "--id",
            "A.B",
            "--exact",
            "--source",
            "winget",
            "--silent",
            "--disable-interactivity"
        ]
    );

    let (events, o) = collect(fx.cli.update(&id).await.unwrap()).await;
    o.unwrap();
    assert!(events.iter().all(|e| e.operation == Operation::Update));
    assert_eq!(
        fx.last_argv(),
        [
            "upgrade",
            "--id",
            "A.B",
            "--exact",
            "--source",
            "winget",
            "--silent",
            "--disable-interactivity",
            "--verbose-logs"
        ]
    );
}

#[tokio::test]
async fn spinner_and_cr_noise_does_not_disturb_stages() {
    let script = "\
raw - \\r\\ \\r| \\r/ \\r
out Found Some App [Vendor.SomeApp] Version 1.2.3
raw   - \\r  \\ \\r  | \\r
out Downloading https://example.invalid/x.exe
raw ██▒▒  1.00 MB / 4.00 MB\\r██▒▒  1.00 MB / 4.00 MB\\r████  4.00 MB / 4.00 MB\\r
out Starting package install...
raw Successfully installed
exit 0
";
    let fx = Fx::new(script);
    let (events, o) = collect(
        fx.cli
            .install(&pkg("A.B"), &InstallRequest::default())
            .await
            .unwrap(),
    )
    .await;
    o.unwrap();
    assert_one_terminal_last(&events);
    assert_monotonic(&events);
    assert!(events
        .iter()
        .any(|e| e.stage == OperationStage::Downloading && e.fraction > 0.5));
    // the unterminated final line is still parsed
    assert!(events.iter().any(|e| e.stage == OperationStage::Finalizing));
    // no spinner glyph ever leaks into a message
    assert!(events
        .iter()
        .all(|e| !matches!(e.message.trim(), "-" | "\\" | "|" | "/")));
}

#[tokio::test]
async fn invalid_utf8_output_is_tolerated() {
    let script = "\
out Found Some App [Vendor.SomeApp] Version 1.2.3
hex 66 6F FF FE 80 0D 0A
out Starting package install...
exit 0
";
    let fx = Fx::new(script);
    let (events, o) = collect(
        fx.cli
            .install(&pkg("A.B"), &InstallRequest::default())
            .await
            .unwrap(),
    )
    .await;
    o.unwrap();
    assert!(events.iter().any(|e| e.stage == OperationStage::Installing));
    assert_one_terminal_last(&events);
}

#[tokio::test]
async fn exit_code_rows_map_to_errors_and_terminal_events() {
    use OperationStage::{Cancelled, Done, Failed};
    type Check = fn(&AppResult<()>) -> bool;
    let rows: Vec<(&str, OperationStage, Check)> = vec![
        ("0", Done, |r| r.is_ok()),
        ("0x8A15002B", Done, |r| r.is_ok()),
        (
            "0x8A150014",
            Failed,
            |r| matches!(r, Err(AppError::PackageNotFound(id)) if id == "Vendor.App"),
        ),
        ("0x8A150010", Failed, |r| {
            matches!(r, Err(AppError::Unsupported(_)))
        }),
        (
            "0x8A150011",
            Failed,
            |r| matches!(r, Err(AppError::WingetFailed { stderr, .. }) if stderr.contains("hash")),
        ),
        ("0x8A150019", Failed, |r| {
            matches!(r, Err(AppError::ElevationRequired))
        }),
        ("740", Failed, |r| {
            matches!(r, Err(AppError::ElevationRequired))
        }),
        ("0x8A150041", Failed, |r| {
            matches!(
                r,
                Err(AppError::AgreementsRequired {
                    source_agreements: false,
                    package_agreements: true
                })
            )
        }),
        ("0x8A150046", Failed, |r| {
            matches!(
                r,
                Err(AppError::AgreementsRequired {
                    source_agreements: true,
                    package_agreements: false
                })
            )
        }),
        ("0x8A150005", Cancelled, |r| {
            matches!(r, Err(AppError::Cancelled))
        }),
        ("0x8A15006A", Cancelled, |r| {
            matches!(r, Err(AppError::Cancelled))
        }),
        ("0x8A15010C", Cancelled, |r| {
            matches!(r, Err(AppError::Cancelled))
        }),
        (
            "0x8A150115",
            Failed,
            |r| matches!(r, Err(AppError::WingetFailed { stderr, .. }) if stderr.contains("1603")),
        ),
        ("1", Failed, |r| {
            matches!(r, Err(AppError::WingetFailed { code: 1, .. }))
        }),
    ];
    for (code, terminal, check) in rows {
        let script = format!(
            "out Installer failed with exit code: 1603\nerr something went wrong\nexit {code}\n"
        );
        let fx = Fx::new(&script);
        let (events, outcome) = collect(
            fx.cli
                .install(&pkg("Vendor.App"), &InstallRequest::default())
                .await
                .unwrap(),
        )
        .await;
        assert!(check(&outcome), "exit {code}: {outcome:?}");
        assert_eq!(events.last().unwrap().stage, terminal, "exit {code}");
        assert_one_terminal_last(&events);
    }
}

#[tokio::test]
async fn failure_carries_stderr_detail() {
    let fx = Fx::new("err disk exploded\nexit 7\n");
    let (events, o) = collect(
        fx.cli
            .install(&pkg("A.B"), &InstallRequest::default())
            .await
            .unwrap(),
    )
    .await;
    match o {
        Err(AppError::WingetFailed { code: 7, stderr }) => {
            assert!(stderr.contains("disk exploded"))
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(events.last().unwrap().stage, OperationStage::Failed);
}

#[tokio::test]
async fn cancel_mid_download_kills_the_process() {
    let script = "\
out Found Some App [Vendor.SomeApp] Version 1.2.3
out Downloading https://example.invalid/big.exe
out ██▒▒▒▒▒▒  1.00 MB / 8.00 MB
sleep 60000
exit 0
";
    let fx = Fx::new(script);
    let id = pkg("Vendor.SomeApp");
    let mut h = fx
        .cli
        .install(&id, &InstallRequest::default())
        .await
        .unwrap();
    let op = h.operation_id.clone();
    let mut events = Vec::new();
    let started = std::time::Instant::now();
    loop {
        let e = tokio::time::timeout(Duration::from_secs(20), h.events.recv())
            .await
            .expect("no events")
            .expect("stream closed early");
        let downloading = e.stage == OperationStage::Downloading && e.fraction > 0.1;
        events.push(e);
        if downloading {
            break;
        }
    }
    // guard is held while the op runs
    assert!(fx.cli.op_guard().is_busy(&id));
    fx.cli.cancel(&op).await.unwrap();
    let rest = tokio::time::timeout(Duration::from_secs(20), async {
        while let Some(e) = h.events.recv().await {
            events.push(e);
        }
    })
    .await;
    rest.expect("stream did not close after cancel");
    assert!(started.elapsed() < Duration::from_secs(30));
    let outcome = h.outcome.await.unwrap();
    assert!(matches!(outcome, Err(AppError::Cancelled)), "{outcome:?}");
    assert_eq!(events.last().unwrap().stage, OperationStage::Cancelled);
    assert_one_terminal_last(&events);

    // guard released, process gone
    assert!(!fx.cli.op_guard().is_busy(&id));
    let pid = std::fs::read_to_string(fx.dir.path().join("pid.txt")).unwrap();
    #[cfg(windows)]
    {
        let out = std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {}", pid.trim()), "/NH"])
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&out.stdout).to_lowercase();
        assert!(!text.contains("fake_winget"), "fake still running: {text}");
    }
    #[cfg(not(windows))]
    let _ = pid;
}

#[tokio::test]
async fn cancel_after_finish_and_unknown_id_are_harmless() {
    let fx = Fx::new("exit 0\n");
    let h = fx
        .cli
        .install(&pkg("A.B"), &InstallRequest::default())
        .await
        .unwrap();
    let op = h.operation_id.clone();
    let (_, o) = collect(h).await;
    o.unwrap();
    fx.cli.cancel(&op).await.unwrap();
    fx.cli.cancel("nope").await.unwrap();
}

#[tokio::test]
async fn one_mutating_operation_per_package() {
    let fx = Fx::new("sleep 800\nexit 0\n");
    let id = pkg("A.B");
    let first = fx
        .cli
        .install(&id, &InstallRequest::default())
        .await
        .unwrap();
    let err = fx.cli.update(&id).await.unwrap_err();
    assert_eq!(err.code(), "unsupported");
    // another package is unaffected
    let other = fx
        .cli
        .install(&pkg("C.D"), &InstallRequest::default())
        .await
        .unwrap();
    collect(first).await.1.unwrap();
    collect(other).await.1.unwrap();
    // released afterwards
    let again = fx
        .cli
        .install(&id, &InstallRequest::default())
        .await
        .unwrap();
    collect(again).await.1.unwrap();
}

#[tokio::test]
async fn winget_missing_is_winget_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let cli = WingetCli::new(dir.path().join("absent.exe"), WingetOptions::default());
    let err = cli
        .install(&pkg("A.B"), &InstallRequest::default())
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::WingetNotFound), "{err:?}");
    assert!(!cli.is_available().await);
    assert!(!cli.op_guard().is_busy(&pkg("A.B")));
    let err = WingetCli::connect_at(dir.path().join("absent.exe"), WingetOptions::default())
        .await
        .map(|_| ())
        .unwrap_err();
    assert!(matches!(err, AppError::WingetNotFound));
}

#[tokio::test]
async fn version_probe_enforces_floor() {
    let env = |v: &'static str| vec![("FAKE_WINGET_VERSION", v)];
    let ok = Fx::build("", false, &env("v1.29.380"), None);
    let v = ok.cli.probe().await.unwrap();
    assert_eq!((v.major, v.minor), (1, 29));
    assert!(ok.cli.is_available().await);

    let old = Fx::build("", false, &env("v1.5.1881"), None);
    let err = old.cli.probe().await.unwrap_err();
    assert_eq!(err.code(), "unsupported");
    assert!(err.to_string().contains("1.6"));
    assert!(!old.cli.is_available().await);

    let junk = Fx::build("", false, &env("hello"), None);
    assert_eq!(junk.cli.probe().await.unwrap_err().code(), "unsupported");
}

const LIST_TABLE: &str = "\
out Name                       Id                                    Version     Available Source
out ---------------------------------------------------------------------------------------------
out 7-Zip 26.05 (x64)          7zip.7zip                             26.05       26.06     winget
out Android Studio             Google.AndroidStudio                  2026.2.2.9            winget
out Armoury Crate Service      ARP\\Machine\\X64\\Armoury Crate Service 6.7.6.0
exit 0
";

#[tokio::test]
async fn list_installed_parses_table() {
    let fx = Fx::new(LIST_TABLE);
    let rows = fx.cli.list_installed().await.unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].id, "7zip.7zip");
    assert_eq!(rows[0].source, Some(SourceKind::WingetCommunity));
    assert_eq!(rows[0].available.as_deref(), Some("26.06"));
    assert_eq!(rows[2].name, "Armoury Crate Service");
    assert_eq!(rows[2].source, None);
    assert_eq!(fx.last_argv(), ["list", "--disable-interactivity"]);
}

#[tokio::test]
async fn list_installed_with_no_match_is_empty() {
    let fx = Fx::new("out No installed package found matching input criteria.\nexit 0x8A150014\n");
    assert!(fx.cli.list_installed().await.unwrap().is_empty());
}

#[tokio::test]
async fn list_updates_parses_table_and_skips_unmatched_rows() {
    let script = "\
out Name                  Id                         Version      Available    Source
out ----------------------------------------------------------------------------
out 7-Zip 26.05 (x64)     7zip.7zip                  26.05        26.06        winget
out Windows Terminal      Microsoft.WindowsTerminal  1.24.12741.0 1.25.2733.0  winget
out Some Local App        ARP\\Machine\\X64\\Local    1.0
out 2 upgrades available.
exit 0
";
    let fx = Fx::accepting(script);
    let ups = fx.cli.list_updates().await.unwrap();
    assert_eq!(ups.len(), 2);
    assert_eq!(ups[0].package_id, pkg("7zip.7zip"));
    assert_eq!(ups[0].installed_version, "26.05");
    assert_eq!(ups[0].latest_version, "26.06");
    assert_eq!(ups[0].source, SourceKind::WingetCommunity);
    assert_eq!(ups[1].installed_version, "1.24.12741.0");
    assert_eq!(ups[1].latest_version, "1.25.2733.0");
    assert!(!ups[0].is_auto);
    assert_eq!(
        fx.last_argv(),
        [
            "upgrade",
            "--include-unknown",
            "--disable-interactivity",
            "--accept-source-agreements"
        ]
    );
}

#[tokio::test]
async fn list_surfaces_agreement_prompt() {
    let fx = Fx::new("exit 0x8A150046\n");
    let err = fx.cli.list_installed().await.unwrap_err();
    assert!(matches!(
        err,
        AppError::AgreementsRequired {
            source_agreements: true,
            package_agreements: false
        }
    ));
    assert_eq!(err.code(), "agreements_required");
    assert!(!err.retryable());
}

#[tokio::test]
async fn pin_and_unpin_argv() {
    let fx = Fx::new("exit 0\n");
    fx.cli.pin(&pkg("A.B")).await.unwrap();
    assert_eq!(fx.last_argv(), ["pin", "add", "--id", "A.B", "--exact"]);
    fx.cli.unpin(&pkg("A.B")).await.unwrap();
    assert_eq!(fx.last_argv(), ["pin", "remove", "--id", "A.B", "--exact"]);
    let fx = Fx::new("exit 0x8A150014\n");
    assert_eq!(
        fx.cli.pin(&pkg("A.B")).await.unwrap_err().code(),
        "package_not_found"
    );
}

#[tokio::test]
async fn unsupported_source_is_rejected_before_spawning() {
    let fx = Fx::new("exit 0\n");
    let id = PackageId::new(SourceKind::Scoop, "x");
    let err = fx
        .cli
        .install(&id, &InstallRequest::default())
        .await
        .unwrap_err();
    assert_eq!(err.code(), "unsupported");
    assert!(fx.invocations().is_empty());
}

#[tokio::test]
async fn works_through_a_trait_object() {
    let fx = Fx::new(FULL_INSTALL);
    let exec: Box<dyn PackageExecutor> = Box::new(fx.cli.clone());
    let id = pkg("Vendor.SomeApp");
    let h = exec.install(&id, &InstallRequest::default()).await.unwrap();
    collect(h).await.1.unwrap();
}

#[tokio::test]
async fn operations_are_written_to_operation_log() {
    let pool = store_lib::db::init_memory_pool().await.unwrap();
    store_lib::db::run_migrations(&pool).await.unwrap();
    let fx = Fx::build(
        "out Installer failed with exit code: 1603\nerr boom\nexit 0x8A150115\n",
        false,
        &[],
        Some(pool.clone()),
    );
    let id = pkg("Vendor.App");
    let (_, o) = collect(
        fx.cli
            .install(&id, &InstallRequest::default())
            .await
            .unwrap(),
    )
    .await;
    assert!(o.is_err());
    let rows = store_lib::db::repo::operation_log::for_package(&pool, &id, 50)
        .await
        .unwrap();
    assert!(rows.iter().any(|r| r.message.contains("started")));
    let failed = rows
        .iter()
        .find(|r| r.level == LogLevel::Error && r.message.contains("failed"))
        .expect("final error row");
    assert!(failed.detail.as_deref().unwrap_or("").contains("boom"));
}

#[tokio::test]
async fn terminal_event_survives_a_consumer_that_never_drains() {
    // 600 distinct bar updates overflow the 256-slot channel before anyone reads.
    let mut script = String::from("out Found Some App [Vendor.SomeApp] Version 1.2.3\n");
    for i in 1..=600 {
        script.push_str(&format!("out ██▒▒  {i}.00 MB / 1000 MB\n"));
    }
    script.push_str("exit 0\n");
    let fx = Fx::new(&script);
    let mut h = fx
        .cli
        .install(&pkg("A.B"), &InstallRequest::default())
        .await
        .unwrap();
    // wait for the operation to finish without reading a single event
    let outcome = tokio::time::timeout(Duration::from_secs(30), &mut h.outcome)
        .await
        .expect("operation stalled on a full channel")
        .unwrap();
    outcome.unwrap();
    let mut events = Vec::new();
    while let Some(e) = h.events.recv().await {
        events.push(e);
    }
    assert!(events.len() <= 256, "{}", events.len());
    assert_one_terminal_last(&events);
    assert_eq!(events.last().unwrap().stage, OperationStage::Done);
    assert_monotonic(&events);
}

#[tokio::test]
async fn cancel_racing_a_clean_exit_never_contradicts_itself() {
    for _ in 0..10 {
        let fx = Fx::new("exit 0\n");
        let h = fx
            .cli
            .install(&pkg("A.B"), &InstallRequest::default())
            .await
            .unwrap();
        fx.cli.cancel(&h.operation_id).await.unwrap();
        let (events, outcome) = collect(h).await;
        assert_one_terminal_last(&events);
        match outcome {
            Ok(()) => assert_eq!(events.last().unwrap().stage, OperationStage::Done),
            Err(AppError::Cancelled) => {
                assert_eq!(events.last().unwrap().stage, OperationStage::Cancelled)
            }
            Err(other) => panic!("{other:?}"),
        }
    }
}

#[tokio::test]
async fn agreement_flags_on_update_remove_list_and_pin() {
    let src = "--accept-source-agreements".to_string();
    let pkg_flag = "--accept-package-agreements".to_string();
    for accept in [false, true] {
        let fx = Fx::build("exit 0\n", accept, &[], None);
        let id = pkg("A.B");
        collect(fx.cli.update(&id).await.unwrap()).await.1.unwrap();
        let up = fx.last_argv();
        collect(fx.cli.remove(&id).await.unwrap()).await.1.unwrap();
        let rm = fx.last_argv();
        fx.cli.list_installed().await.unwrap();
        let li = fx.last_argv();
        fx.cli.list_updates().await.unwrap();
        let lu = fx.last_argv();
        fx.cli.pin(&id).await.unwrap();
        let pin = fx.last_argv();
        fx.cli.unpin(&id).await.unwrap();
        let unpin = fx.last_argv();

        for argv in [&up, &rm, &li, &lu, &pin, &unpin] {
            assert_eq!(argv.contains(&src), accept, "{argv:?}");
        }
        // the package-agreements flag exists only on update (and install)
        assert_eq!(up.contains(&pkg_flag), accept);
        for argv in [&rm, &li, &lu, &pin, &unpin] {
            assert!(!argv.contains(&pkg_flag), "{argv:?}");
        }
    }
}
