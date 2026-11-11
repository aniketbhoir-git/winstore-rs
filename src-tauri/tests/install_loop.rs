//! Issue #6 acceptance, end to end below the UI: IPC handlers -> install service -> winget
//! executor -> fake winget (`tests/fixtures/fake_winget.rs`) -> SQLite ledger and log.
//! Real winget and the network are never touched.

mod common;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use store_lib::catalog::{BoxFuture, Catalog, CatalogSource, SearchQuery};
use store_lib::config::AppConfig;
use store_lib::db::{self, repo::installs, DbPool};
use store_lib::domain::{
    InstallState, InstallerMetadata, LogLevel, PackageDetail, PackageId, PackageSummary,
    PackagingKind, SourceCapabilities, SourceKind,
};
use store_lib::error::AppResult;
use store_lib::executor::winget_cli::{WingetCli, WingetOptions};
use store_lib::ipc::commands::{
    run_cancel, run_install_status, run_operation_log, run_start_install, run_start_uninstall,
    InstallOptions,
};
use store_lib::ipc::events::{EventSink, SharedSink, EVENT_OPERATION_FINISHED, EVENT_PROGRESS};
use store_lib::ipc::state::{AppState, Core, ExecutorFactory, SharedExecutor};

const PKG: &str = "winget_community/Vendor.SomeApp";

const INSTALL_OK: &str = "\
out Found Some App [Vendor.SomeApp] Version 1.2.3
out Downloading https://example.invalid/installer.exe
out   ██████████████  8.00 MB / 8.00 MB
out Successfully verified installer hash
out Starting package install...
out Successfully installed
exit 0
";

const INSTALL_SLOW: &str = "\
out Found Some App [Vendor.SomeApp] Version 1.2.3
out Downloading https://example.invalid/installer.exe
out ██▒▒▒▒▒▒  1.00 MB / 8.00 MB
sleep 60000
exit 0
";

const UNINSTALL_OK: &str = "\
out Found Some App [Vendor.SomeApp]
out Starting package uninstall...
out Successfully uninstalled
exit 0
";

const UNINSTALL_SLOW: &str = "\
out Found Some App [Vendor.SomeApp]
out Starting package uninstall...
sleep 60000
exit 0
";

struct Source;

impl CatalogSource for Source {
    fn kind(&self) -> SourceKind {
        SourceKind::WingetCommunity
    }
    fn capabilities(&self) -> SourceCapabilities {
        SourceCapabilities {
            search: true,
            detail: true,
            ..SourceCapabilities::default()
        }
    }
    fn search<'a>(&'a self, _q: &'a SearchQuery) -> BoxFuture<'a, AppResult<Vec<PackageSummary>>> {
        Box::pin(async { Ok(vec![summary()]) })
    }
    fn get_detail<'a>(&'a self, _id: &'a str) -> BoxFuture<'a, AppResult<PackageDetail>> {
        Box::pin(async {
            Ok(PackageDetail {
                summary: summary(),
                full_description: "An app".into(),
                screenshots: vec![],
                website: None,
                license: Some("MIT".into()),
                release_notes: None,
                installer: InstallerMetadata {
                    architecture: Some("x64".into()),
                    install_type: PackagingKind::Exe,
                    size_bytes: None,
                    sha256: None,
                },
                versions: BTreeMap::new(),
            })
        })
    }
}

fn summary() -> PackageSummary {
    PackageSummary {
        id: id(),
        name: "Some App".into(),
        publisher: "Vendor".into(),
        latest_version: "1.2.3".into(),
        description: "An app".into(),
        icon_url: None,
        rating: None,
        rating_count: None,
        source: SourceKind::WingetCommunity,
        is_paid: false,
        packaging: PackagingKind::Exe,
    }
}

fn id() -> PackageId {
    PackageId::new(SourceKind::WingetCommunity, "Vendor.SomeApp")
}

#[derive(Default)]
struct Recorder(Mutex<Vec<(String, serde_json::Value)>>);

impl EventSink for Recorder {
    fn emit_json(&self, event: &str, payload: serde_json::Value) {
        self.0.lock().unwrap().push((event.to_string(), payload));
    }
}

impl Recorder {
    fn named(&self, name: &str) -> Vec<serde_json::Value> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .filter(|(n, _)| n == name)
            .map(|(_, p)| p.clone())
            .collect()
    }

    async fn finished(&self, n: usize) -> serde_json::Value {
        for _ in 0..2000 {
            if let Some(v) = self.named(EVENT_OPERATION_FINISHED).get(n - 1) {
                return v.clone();
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("finish event {n} never arrived");
    }

    /// Waits for a progress event of the operation that reached `stage`.
    async fn reached(&self, op: &str, stage: &str) {
        for _ in 0..2000 {
            if self
                .named(EVENT_PROGRESS)
                .iter()
                .any(|e| e["operation_id"] == op && e["stage"] == stage)
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("operation {op} never reached {stage}");
    }
}

struct Fx {
    _dir: tempfile::TempDir,
    script: PathBuf,
    state: AppState,
    pool: DbPool,
    sink: Arc<Recorder>,
}

impl Fx {
    async fn new() -> Fx {
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("script.txt");
        let pool = db::init_memory_pool().await.unwrap();
        db::run_migrations(&pool).await.unwrap();
        let cli = WingetCli::new(
            common::fake_exe(),
            WingetOptions {
                accept_agreements: false,
                log_pool: Some(pool.clone()),
                env: vec![(
                    "FAKE_WINGET_SCRIPT".to_string(),
                    script.display().to_string(),
                )],
            },
        );
        let exec: SharedExecutor = Arc::new(cli);
        let factory: ExecutorFactory = Arc::new(move || {
            let e = Arc::clone(&exec);
            Box::pin(async move { Ok(e) })
        });
        let catalog = Catalog::new(vec![Arc::new(Source)], Some(pool.clone()), 3600);
        let core = Core::new(pool.clone(), catalog, AppConfig::default(), factory);
        Fx {
            _dir: dir,
            script,
            state: AppState::with_core(core),
            pool,
            sink: Arc::new(Recorder::default()),
        }
    }

    fn sink(&self) -> SharedSink {
        self.sink.clone()
    }

    /// The fake reads its script when it starts, so each operation can have its own.
    fn script(&self, text: &str) {
        std::fs::write(&self.script, text).unwrap();
    }

    async fn ledger(&self) -> (InstallState, Option<String>, Option<String>) {
        let r = installs::get(&self.pool, &id()).await.unwrap().unwrap();
        (r.state, r.installed_version, r.target_version)
    }
}

#[tokio::test]
async fn install_cancel_uninstall_loop_is_repeatable_against_the_fake() {
    use InstallState::*;
    let fx = Fx::new().await;
    let mut n = 0;
    for _round in 0..2 {
        // Cancel mid-download: process killed, ledger back to NotInstalled, nothing active.
        fx.script(INSTALL_SLOW);
        let op = run_start_install(&fx.state, fx.sink(), PKG, InstallOptions::default())
            .await
            .unwrap()
            .operation_id;
        fx.sink.reached(&op, "downloading").await;
        let st = run_install_status(&fx.state, PKG).await.unwrap();
        assert_eq!(st.state, Installing);
        assert_eq!(st.active.unwrap().operation_id, op);
        run_cancel(&fx.state, &op).await.unwrap();
        n += 1;
        let fin = fx.sink.finished(n).await;
        assert_eq!(fin["error"]["code"], "cancelled", "{fin}");
        assert_eq!(fx.ledger().await, (NotInstalled, None, None));
        assert!(run_install_status(&fx.state, PKG)
            .await
            .unwrap()
            .active
            .is_none());

        // Install.
        fx.script(INSTALL_OK);
        run_start_install(&fx.state, fx.sink(), PKG, InstallOptions::default())
            .await
            .unwrap();
        n += 1;
        assert_eq!(fx.sink.finished(n).await["ok"], true);
        assert_eq!(fx.ledger().await, (Installed, Some("1.2.3".into()), None));

        // Cancel an uninstall: the app stays installed.
        fx.script(UNINSTALL_SLOW);
        let op = run_start_uninstall(&fx.state, fx.sink(), PKG)
            .await
            .unwrap()
            .operation_id;
        fx.sink.reached(&op, "resolving").await;
        assert_eq!(fx.ledger().await.0, Uninstalling);
        run_cancel(&fx.state, &op).await.unwrap();
        n += 1;
        assert_eq!(fx.sink.finished(n).await["error"]["code"], "cancelled");
        assert_eq!(fx.ledger().await, (Installed, Some("1.2.3".into()), None));

        // Uninstall.
        fx.script(UNINSTALL_OK);
        run_start_uninstall(&fx.state, fx.sink(), PKG)
            .await
            .unwrap();
        n += 1;
        assert_eq!(fx.sink.finished(n).await["ok"], true);
        assert_eq!(fx.ledger().await, (NotInstalled, None, None));
    }

    // The log tail has every start and outcome, oldest first.
    let log = run_operation_log(&fx.state, PKG, Some(500)).await.unwrap();
    let msgs: Vec<_> = log.iter().map(|e| e.message.as_str()).collect();
    assert_eq!(
        msgs.iter().filter(|m| **m == "install cancelled").count(),
        2
    );
    assert_eq!(msgs.iter().filter(|m| **m == "install finished").count(), 2);
    assert_eq!(msgs.iter().filter(|m| **m == "remove cancelled").count(), 2);
    assert_eq!(msgs.iter().filter(|m| **m == "remove finished").count(), 2);
    assert_eq!(msgs.first(), Some(&"install started"));
    assert_eq!(msgs.last(), Some(&"remove finished"));
    assert!(log.iter().all(|e| e.level != LogLevel::Error));
}

#[tokio::test]
async fn failed_install_records_failed_and_the_reason() {
    let fx = Fx::new().await;
    fx.script(
        "out Found Some App [Vendor.SomeApp] Version 1.2.3\nerr installer exploded\nexit 1\n",
    );
    run_start_install(&fx.state, fx.sink(), PKG, InstallOptions::default())
        .await
        .unwrap();
    let fin = fx.sink.finished(1).await;
    assert_eq!(fin["ok"], false);
    assert_eq!(fx.ledger().await, (InstallState::Failed, None, None));
    let log = run_operation_log(&fx.state, PKG, None).await.unwrap();
    assert!(log.iter().any(|e| e.level == LogLevel::Error));
}
