//! Executor tests driven against the fake winget (`tests/fixtures/fake_winget.rs`).
//! Real winget is never started here.

use std::time::Duration;

mod common;
use common::fake_exe;

use store_lib::domain::{
    LogLevel, Operation, OperationStage, PackageId, ProgressEvent, SourceKind,
};
use store_lib::error::{AppError, AppResult};
use store_lib::executor::winget_cli::{ProcessJob, StopFuture, WingetCli, WingetOptions};
use store_lib::executor::{InstallRequest, InstallScope, OperationHandle, PackageExecutor};

struct Fx {
    dir: tempfile::TempDir,
    cli: WingetCli,
}

impl Fx {
    fn new(script: &str) -> Fx {
        Self::build(script, &[], None)
    }

    fn build(
        script: &str,
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
        let cli = WingetCli::new(fake_exe(), WingetOptions { log_pool, env });
        Fx { dir, cli }
    }

    /// Replaces the catalog time limits; the executor must not be shared yet.
    fn with_limits(self, search: Duration, show: Duration) -> Fx {
        Fx {
            dir: self.dir,
            cli: self.cli.with_read_limits(search, show),
        }
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

/// winget 1.29.240+ piped layout: stage lines only, no spinner and no bar.
#[tokio::test]
async fn install_with_no_bar_streams_every_stage_and_reports_indeterminate() {
    let script = format!(
        "file {}
exit 0
",
        fixture("install-piped.txt")
    );
    let fx = Fx::new(&script);
    let h = fx
        .cli
        .install(&pkg("Vendor.SomeApp"), &InstallRequest::default())
        .await
        .unwrap();
    let (events, outcome) = collect(h).await;
    outcome.unwrap();

    let mut stages: Vec<_> = events.iter().map(|e| e.stage).collect();
    stages.dedup();
    use OperationStage::*;
    assert_eq!(
        stages,
        [Queued, Resolving, Downloading, Installing, Finalizing, Done]
    );
    assert_one_terminal_last(&events);
    assert_monotonic(&events);
    // Every stage line is relayed as its message.
    for needle in [
        "Found Some App",
        "Downloading https://",
        "Successfully verified installer hash",
        "Starting package install",
        "Successfully installed",
    ] {
        assert!(
            events.iter().any(|e| e.message.starts_with(needle)),
            "no event for {needle}: {events:#?}"
        );
    }
    let (last, running) = events.split_last().unwrap();
    assert!(running.iter().all(|e| e.indeterminate), "{running:#?}");
    assert_eq!(last.stage, Done);
    assert!(!last.indeterminate);
    assert_eq!(last.fraction, 1.0);
}

#[tokio::test]
async fn failure_after_a_piped_install_keeps_the_terminal_event_indeterminate() {
    let script = format!(
        "file {}
exit 1
",
        fixture("install-piped.txt")
    );
    let fx = Fx::new(&script);
    let h = fx
        .cli
        .install(&pkg("Vendor.SomeApp"), &InstallRequest::default())
        .await
        .unwrap();
    let (events, outcome) = collect(h).await;
    assert!(outcome.is_err());
    assert_one_terminal_last(&events);
    let last = events.last().unwrap();
    assert_eq!(last.stage, OperationStage::Failed);
    assert!(last.indeterminate, "{last:#?}");
    assert!(last.fraction < 1.0);
}

#[tokio::test]
async fn install_with_a_bar_is_not_indeterminate_once_the_bar_shows() {
    let fx = Fx::new(FULL_INSTALL);
    let h = fx
        .cli
        .install(&pkg("Vendor.SomeApp"), &InstallRequest::default())
        .await
        .unwrap();
    let (events, outcome) = collect(h).await;
    outcome.unwrap();
    let first_bar = events
        .iter()
        .position(|e| e.stage == OperationStage::Downloading && e.fraction > 0.11)
        .expect("a bar event");
    assert!(events[..first_bar].iter().all(|e| e.indeterminate));
    assert!(events[first_bar..].iter().all(|e| !e.indeterminate));
}

#[tokio::test]
async fn install_passes_no_agreement_flag_in_any_case() {
    let fx = Fx::new("exit 0\n");
    let req = InstallRequest {
        version: Some("2.0".into()),
        scope: Some(InstallScope::Machine),
    };
    let (_, outcome) = collect(fx.cli.install(&pkg("A.B"), &req).await.unwrap()).await;
    outcome.unwrap();
    let argv = fx.last_argv();
    assert!(!argv.contains(&"--accept-source-agreements".to_string()));
    assert!(!argv.contains(&"--accept-package-agreements".to_string()));
    assert!(argv.windows(2).any(|w| w == ["--scope", "machine"]));
    assert!(argv.windows(2).any(|w| w == ["--version", "2.0"]));
}

#[tokio::test]
async fn forbidden_flags_never_reach_any_command() {
    let fx = Fx::new("exit 0\n");
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
                    package_agreements: true,
                    ..
                })
            )
        }),
        ("0x8A150046", Failed, |r| {
            matches!(
                r,
                Err(AppError::AgreementsRequired {
                    source_agreements: true,
                    package_agreements: false,
                    ..
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
    let ok = Fx::build("", &env("v1.29.380"), None);
    let v = ok.cli.probe().await.unwrap();
    assert_eq!((v.major, v.minor), (1, 29));
    assert!(ok.cli.is_available().await);

    let old = Fx::build("", &env("v1.28.240"), None);
    let err = old.cli.probe().await.unwrap_err();
    assert_eq!(err.code(), "unsupported");
    assert!(err.to_string().contains("1.29"));
    assert!(!old.cli.is_available().await);

    let junk = Fx::build("", &env("hello"), None);
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
    let fx = Fx::new(script);
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
        ["upgrade", "--include-unknown", "--disable-interactivity"]
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
            package_agreements: false,
            ..
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
async fn no_command_passes_an_agreement_flag() {
    let fx = Fx::new("exit 0\n");
    let id = pkg("A.B");
    collect(
        fx.cli
            .install(&id, &InstallRequest::default())
            .await
            .unwrap(),
    )
    .await
    .1
    .unwrap();
    collect(fx.cli.remove(&id).await.unwrap()).await.1.unwrap();
    collect(fx.cli.update(&id).await.unwrap()).await.1.unwrap();
    fx.cli.list_installed().await.unwrap();
    fx.cli.list_updates().await.unwrap();
    fx.cli.pin(&id).await.unwrap();
    fx.cli.unpin(&id).await.unwrap();
    let all = fx.invocations();
    assert_eq!(all.len(), 7);
    assert_eq!(all[5][..2], ["pin", "add"]);
    assert_eq!(all[6][..2], ["pin", "remove"]);
    for argv in all {
        assert!(!argv.iter().any(|a| a.starts_with("--accept-")), "{argv:?}");
    }
}

#[tokio::test]
async fn agreements_required_carries_dialog_data_and_logs_info() {
    for (code, source_kind, package_kind) in
        [("0x8A150041", false, true), ("0x8A150046", true, false)]
    {
        for version in [Some("2.0"), None] {
            let pool = store_lib::db::init_memory_pool().await.unwrap();
            store_lib::db::run_migrations(&pool).await.unwrap();
            let fx = Fx::build(&format!("exit {code}\n"), &[], Some(pool.clone()));
            let id = pkg("Vendor.App");
            let req = InstallRequest {
                version: version.map(str::to_string),
                scope: None,
            };
            let (events, outcome) = collect(fx.cli.install(&id, &req).await.unwrap()).await;
            assert_one_terminal_last(&events);
            assert_eq!(events.last().unwrap().stage, OperationStage::Failed);
            match outcome {
                Err(AppError::AgreementsRequired {
                    source_agreements,
                    package_agreements,
                    winget_source,
                    version: got,
                    agreement_text,
                    agreement_url,
                }) => {
                    assert_eq!(source_agreements, source_kind, "{code}");
                    assert_eq!(package_agreements, package_kind, "{code}");
                    assert_eq!(winget_source.as_deref(), Some("winget"));
                    assert_eq!(got.as_deref(), version);
                    assert!(agreement_text.is_none() && agreement_url.is_none());
                }
                other => panic!("{other:?}"),
            }
            let rows = store_lib::db::repo::operation_log::for_package(&pool, &id, 50)
                .await
                .unwrap();
            assert!(rows.iter().all(|r| r.level != LogLevel::Error), "{rows:#?}");
            assert!(
                rows.iter()
                    .any(|r| r.level == LogLevel::Info && r.message.contains("stopped")),
                "{rows:#?}"
            );
        }
    }
}

#[tokio::test]
async fn agreements_required_on_remove_and_update_has_source_and_no_version() {
    let fx = Fx::new("exit 0x8A150041\n");
    let id = pkg("A.B");
    for op in [Operation::Remove, Operation::Update] {
        let h = match op {
            Operation::Remove => fx.cli.remove(&id).await.unwrap(),
            _ => fx.cli.update(&id).await.unwrap(),
        };
        match collect(h).await.1 {
            Err(AppError::AgreementsRequired {
                winget_source,
                version,
                ..
            }) => {
                assert_eq!(winget_source.as_deref(), Some("winget"));
                assert!(version.is_none());
            }
            other => panic!("{other:?}"),
        }
    }
}

fn live_tree<'a>(
    _child: &'a mut tokio::process::Child,
    _job: Option<&'a ProcessJob>,
) -> StopFuture<'a> {
    Box::pin(async { false })
}

#[tokio::test]
async fn cancel_that_cannot_stop_the_tree_emits_one_cancel_failed_event() {
    let pool = store_lib::db::init_memory_pool().await.unwrap();
    store_lib::db::run_migrations(&pool).await.unwrap();
    let mut fx = Fx::build("sleep 1500\nexit 0\n", &[], Some(pool.clone()));
    fx.cli = fx.cli.with_stop_fn(live_tree);
    let cli = fx.cli.clone();
    let mut h = cli
        .install(&pkg("A.B"), &InstallRequest::default())
        .await
        .unwrap();
    cli.cancel(&h.operation_id).await.unwrap();
    let mut events = Vec::new();
    loop {
        let e = tokio::time::timeout(Duration::from_secs(20), h.events.recv())
            .await
            .expect("no cancel_failed event")
            .expect("stream closed early");
        let failed = e.cancel_failed;
        events.push(e);
        if failed {
            break;
        }
    }
    // a second cancel for the same operation does nothing
    cli.cancel(&h.operation_id).await.unwrap();
    tokio::time::timeout(Duration::from_secs(30), async {
        while let Some(e) = h.events.recv().await {
            events.push(e);
        }
    })
    .await
    .expect("stream did not close");
    let outcome = h.outcome.await.unwrap();
    outcome.expect("the real outcome stands: exit 0");

    let flagged: Vec<_> = events.iter().filter(|e| e.cancel_failed).collect();
    assert_eq!(flagged.len(), 1, "{events:#?}");
    assert_eq!(flagged[0].message, "cancel failed: the installer continues");
    let idx = events.iter().position(|e| e.cancel_failed).unwrap();
    assert!(idx > 0);
    assert_eq!(flagged[0].stage, events[idx - 1].stage, "stage unchanged");
    assert!(!flagged[0].stage.is_terminal());
    assert_one_terminal_last(&events);
    assert_eq!(events.last().unwrap().stage, OperationStage::Done);
    assert!(!events.last().unwrap().cancel_failed);

    let rows = store_lib::db::repo::operation_log::for_package(&pool, &pkg("A.B"), 50)
        .await
        .unwrap();
    let warns: Vec<_> = rows.iter().filter(|r| r.level == LogLevel::Warn).collect();
    assert_eq!(warns.len(), 1, "{rows:#?}");
    assert!(warns[0].message.contains("could not be killed"));
    assert!(rows.iter().all(|r| r.level != LogLevel::Error), "{rows:#?}");
}

#[tokio::test]
async fn cancel_failed_event_gives_up_after_two_seconds_with_no_room() {
    // 600 distinct bar updates fill the channel up to the slot kept for the terminal event.
    let mut script = String::from("out Found Some App [Vendor.SomeApp] Version 1.2.3\n");
    for i in 1..=600 {
        script.push_str(&format!("out ██▒▒  {i}.00 MB / 1000 MB\n"));
    }
    script.push_str("sleep 4000\nexit 0\n");
    let pool = store_lib::db::init_memory_pool().await.unwrap();
    store_lib::db::run_migrations(&pool).await.unwrap();
    let mut fx = Fx::build(&script, &[], Some(pool.clone()));
    fx.cli = fx.cli.with_stop_fn(live_tree);
    let cli = fx.cli.clone();
    let h = cli
        .install(&pkg("A.B"), &InstallRequest::default())
        .await
        .unwrap();
    // Nobody reads: wait until only the terminal slot is free, then cancel.
    tokio::time::timeout(Duration::from_secs(20), async {
        while h.events.len() < h.events.max_capacity() - 1 {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the channel did not fill");
    cli.cancel(&h.operation_id).await.unwrap();

    // The executor writes the "no room" row only after its 2 s wait.
    let no_room = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let rows = store_lib::db::repo::operation_log::for_package(&pool, &pkg("A.B"), 50)
                .await
                .unwrap();
            if rows
                .iter()
                .any(|r| r.level == LogLevel::Warn && r.message.contains("no room"))
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await;
    assert!(
        no_room.is_ok(),
        "no Warn row for the event that did not fit"
    );

    let (events, outcome) = collect(h).await;
    outcome.expect("the real outcome stands: exit 0");
    assert!(events.iter().all(|e| !e.cancel_failed), "{events:#?}");
    assert_one_terminal_last(&events);
    assert_eq!(events.last().unwrap().stage, OperationStage::Done);
}

#[tokio::test]
async fn log_rows_follow_the_outcome_for_success_and_cancel() {
    // SPEC §6.7, end table, "Log row": success and cancel give Info rows and no Error row.
    for (script, cancel, word) in [
        ("out Successfully installed\nexit 0\n", false, "finished"),
        (
            "out Downloading https://x/a.exe\nsleep 20000\nexit 0\n",
            true,
            "cancelled",
        ),
    ] {
        let pool = store_lib::db::init_memory_pool().await.unwrap();
        store_lib::db::run_migrations(&pool).await.unwrap();
        let fx = Fx::build(script, &[], Some(pool.clone()));
        let h = fx
            .cli
            .install(&pkg("A.B"), &InstallRequest::default())
            .await
            .unwrap();
        if cancel {
            fx.cli.cancel(&h.operation_id).await.unwrap();
        }
        let (_, outcome) = collect(h).await;
        assert_eq!(outcome.is_err(), cancel, "{word}");
        let rows = store_lib::db::repo::operation_log::for_package(&pool, &pkg("A.B"), 50)
            .await
            .unwrap();
        assert!(
            rows.iter()
                .any(|r| r.level == LogLevel::Info && r.message.contains(word)),
            "{word}: {rows:#?}"
        );
        assert!(
            rows.iter().all(|r| r.level != LogLevel::Error),
            "{word}: {rows:#?}"
        );
    }
}

// ---- read-only catalog calls (design catalog-winget-cli, step 2) ----------------------------

fn fixture(name: &str) -> String {
    format!(
        "{}/tests/fixtures/winget/1.29/{name}",
        env!("CARGO_MANIFEST_DIR")
    )
}

#[tokio::test]
async fn search_catalog_parses_fixture_and_passes_the_documented_argv() {
    let fx = Fx::new(&format!("file {}\n", fixture("search-vlc.txt")));
    let rows = fx.cli.search_catalog("  vlc ", 30).await.unwrap();
    assert_eq!(rows[0].id, "VideoLAN.VLC");
    assert_eq!(rows.len(), 10);
    assert_eq!(
        fx.last_argv().join(" "),
        "search --query vlc --source winget --count 30 --disable-interactivity"
    );
}

#[tokio::test]
async fn search_catalog_empty_text_starts_no_process() {
    let fx = Fx::new("");
    assert!(fx.cli.search_catalog("   ", 30).await.unwrap().is_empty());
    assert!(fx.invocations().is_empty());
}

#[tokio::test]
async fn search_catalog_no_match_is_an_empty_list() {
    let fx = Fx::new(&format!(
        "file {}\nexit 0x8A150014\n",
        fixture("search-none.txt")
    ));
    assert!(fx.cli.search_catalog("zzzz", 30).await.unwrap().is_empty());
}

#[tokio::test]
async fn search_catalog_unrecognized_output_is_source_unavailable() {
    let fx = Fx::new("out something nobody expected\n");
    let err = fx.cli.search_catalog("vlc", 30).await.unwrap_err();
    assert_eq!(err.code(), "source_unavailable");
    assert!(err.to_string().contains("not recognized"));
}

#[tokio::test]
async fn catalog_calls_map_agreement_exits_and_pass_no_agreement_flag() {
    for code in ["0x8A150046", "0x8A150041"] {
        let fx = Fx::new(&format!("out terms\nexit {code}\n"));
        for err in [
            fx.cli.search_catalog("vlc", 30).await.unwrap_err(),
            fx.cli.show_package("VideoLAN.VLC").await.unwrap_err(),
        ] {
            match err {
                AppError::AgreementsRequired {
                    source_agreements,
                    package_agreements,
                    winget_source,
                    ..
                } => {
                    assert_eq!(source_agreements, code == "0x8A150046");
                    assert_eq!(package_agreements, code == "0x8A150041");
                    assert_eq!(winget_source.as_deref(), Some("winget"));
                }
                other => panic!("{other:?}"),
            }
        }
        for argv in fx.invocations() {
            assert!(argv.iter().all(|a| !a.starts_with("--accept-")), "{argv:?}");
        }
    }
}

#[tokio::test]
async fn show_package_parses_fixture_and_maps_not_found() {
    let fx = Fx::new(&format!("file {}\n", fixture("show-vlc.txt")));
    let block = fx.cli.show_package("VideoLAN.VLC").await.unwrap();
    assert_eq!(block.publisher.as_deref(), Some("VideoLAN"));
    assert_eq!(
        fx.last_argv().join(" "),
        "show --id VideoLAN.VLC --exact --source winget --disable-interactivity"
    );

    let none = Fx::new("out No package found matching input criteria.\nexit 0x8A150014\n");
    let err = none.cli.show_package("Nope.Nope").await.unwrap_err();
    assert_eq!(err.code(), "package_not_found");
}

#[tokio::test]
async fn catalog_time_limit_names_the_limit() {
    let fx = Fx::new(
        "sleep 3000
",
    )
    .with_limits(Duration::from_millis(200), Duration::from_millis(300));
    let err = fx.cli.search_catalog("vlc", 30).await.unwrap_err();
    assert_eq!(err.code(), "source_unavailable");
    assert!(err.to_string().contains("200 ms"), "{err}");
    let err = fx.cli.show_package("A.B").await.unwrap_err();
    assert!(err.to_string().contains("300 ms"), "{err}");
}

// A read-only call that runs into its time limit stops the whole process tree, not only
// winget.exe: the grandchild of the fake is gone too.
#[cfg(windows)]
#[tokio::test]
async fn catalog_time_limit_stops_the_process_tree() {
    let dir = tempfile::tempdir().unwrap();
    let child_pid = dir.path().join("child.txt");
    let fx = Fx::build(
        "child 60000
sleep 60000
",
        &[("FAKE_WINGET_CHILD_PID", child_pid.to_str().unwrap())],
        None,
    )
    .with_limits(Duration::from_millis(2000), Duration::from_millis(2000));
    let err = fx.cli.search_catalog("vlc", 30).await.unwrap_err();
    assert_eq!(err.code(), "source_unavailable");
    let child = std::fs::read_to_string(&child_pid).expect("the fake started no grandchild");
    let alive = |pid: &str| {
        let out = std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {}", pid.trim()), "/NH"])
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).contains("fake_winget")
    };
    let mut gone = false;
    for _ in 0..50 {
        if !alive(&child) {
            gone = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(
        gone,
        "grandchild {child} still running after the time limit"
    );
    let parent = std::fs::read_to_string(fx.dir.path().join("pid.txt")).unwrap();
    assert!(!alive(&parent), "winget {parent} still running");
}
