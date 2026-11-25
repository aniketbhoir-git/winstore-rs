//! `AppState`: the dependency graph shared by every command (SPEC §3 `ipc/state.rs`).
//!
//! The core (database, catalog, executor) is built lazily on first use, so the window opens
//! immediately and a failed start-up step is reported to the caller and retried next time.

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;

use directories::ProjectDirs;
use tokio::sync::{Mutex, OnceCell};

use crate::catalog::{Catalog, CatalogSource};
use crate::config::AppConfig;
use crate::db::{self, repo::sources, DbPool};
use crate::domain::SourceKind;
use crate::error::AppResult;
use crate::executor::winget_cli::{WingetCli, WingetOptions};
use crate::executor::PackageExecutor;
use crate::platform::windows::ToastDeps;
use crate::services::consent::ConsentLedger;
use crate::services::import::ImportPaths;
use crate::services::install::{recover_interrupted, ActiveOperations};
use crate::sources::winget_repo::WingetRepoSource;

pub type SharedExecutor = Arc<dyn PackageExecutor>;
type ExecutorFuture = Pin<Box<dyn Future<Output = AppResult<SharedExecutor>> + Send>>;
/// Creates the executor on demand (winget discovery can fail and is retried).
pub type ExecutorFactory = Arc<dyn Fn() -> ExecutorFuture + Send + Sync>;

/// Everything a command needs once start-up has finished.
pub struct Core {
    pub pool: DbPool,
    pub catalog: Catalog,
    pub config: AppConfig,
    /// Operations running in this process (install service).
    pub active: ActiveOperations,
    /// Open agreement challenges, in memory only (SPEC §6.6 "How consent is bound").
    pub consent: ConsentLedger,
    /// Toast notifier and window-focus check (SPEC §6.9). Tests put fakes here.
    pub toasts: ToastDeps,
    executor: Mutex<Option<SharedExecutor>>,
    factory: ExecutorFactory,
}

impl Core {
    pub fn new(
        pool: DbPool,
        catalog: Catalog,
        config: AppConfig,
        factory: ExecutorFactory,
    ) -> Self {
        Core {
            pool,
            catalog,
            config,
            active: ActiveOperations::default(),
            consent: ConsentLedger::default(),
            toasts: ToastDeps::default(),
            executor: Mutex::new(None),
            factory,
        }
    }

    /// The package executor, discovered on first use. A failure (for example winget missing)
    /// is returned to the caller and not cached.
    pub async fn executor(&self) -> AppResult<SharedExecutor> {
        let mut slot = self.executor.lock().await;
        if let Some(e) = slot.as_ref() {
            return Ok(Arc::clone(e));
        }
        let e = (self.factory)().await?;
        *slot = Some(Arc::clone(&e));
        Ok(e)
    }
}

/// Managed by Tauri; holds the lazily built `Core`.
pub struct AppState {
    core: OnceCell<Arc<Core>>,
    /// Files dropped on the window; the import reads only these and the exports folder.
    pub import_paths: ImportPaths,
    db_path: Option<PathBuf>,
    toasts: ToastDeps,
}

impl AppState {
    /// State that builds its core from the user's data directory on first use.
    pub fn new() -> Self {
        Self::with_toasts(ToastDeps::default())
    }

    /// Like `new`, with the toast parts that the real app builds before the window exists.
    pub fn with_toasts(toasts: ToastDeps) -> Self {
        AppState {
            core: OnceCell::new(),
            import_paths: ImportPaths::default(),
            db_path: ProjectDirs::from("app", "store", "Store")
                .map(|d| d.data_dir().join("store.db")),
            toasts,
        }
    }

    /// State around an already built core (tests).
    pub fn with_core(core: Core) -> Self {
        AppState {
            core: OnceCell::new_with(Some(Arc::new(core))),
            import_paths: ImportPaths::default(),
            db_path: None,
            toasts: ToastDeps::default(),
        }
    }

    pub async fn core(&self) -> AppResult<Arc<Core>> {
        let core = self
            .core
            .get_or_try_init(|| async {
                build_core(self.db_path.as_deref(), self.toasts.clone())
                    .await
                    .map(Arc::new)
            })
            .await?;
        Ok(Arc::clone(core))
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

async fn build_core(db_path: Option<&std::path::Path>, toasts: ToastDeps) -> AppResult<Core> {
    let path = db_path.ok_or_else(|| {
        crate::error::AppError::InvalidConfig("cannot resolve data directory".into())
    })?;
    let config = match AppConfig::load_or_default() {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("could not read config, using defaults: {e}");
            AppConfig::default()
        }
    };
    let pool = db::init_pool(path).await?;
    db::run_migrations(&pool).await?;
    sources::seed_defaults(&pool).await?;
    // Busy ledger rows from an earlier run are stale; settle them. Never fatal.
    match recover_interrupted(&pool).await {
        Ok(0) => {}
        Ok(n) => tracing::warn!(
            rows = n,
            "settled interrupted operations from a previous run"
        ),
        Err(e) => tracing::warn!("could not settle interrupted operations: {e}"),
    }
    tracing::info!(path = %path.display(), "database ready");

    let catalog = Catalog::new(
        enabled_sources(&pool, &config).await?,
        Some(pool.clone()),
        config.cache_ttl_secs,
    );
    let factory = winget_factory(pool.clone());
    let mut core = Core::new(pool, catalog, config, factory);
    core.toasts = toasts;
    Ok(core)
}

/// Adapters for the sources that are enabled in the database and not disabled in the config.
/// Only the community catalog has an adapter so far.
async fn enabled_sources(
    pool: &DbPool,
    config: &AppConfig,
) -> AppResult<Vec<Arc<dyn CatalogSource>>> {
    let mut out: Vec<Arc<dyn CatalogSource>> = Vec::new();
    for cfg in sources::list_enabled(pool).await? {
        if config
            .disabled_sources
            .iter()
            .any(|d| *d == cfg.kind.to_string())
        {
            continue;
        }
        match cfg.kind {
            SourceKind::WingetCommunity => out.push(Arc::new(WingetRepoSource::new()?)),
            other => tracing::debug!(source = %other, "no adapter yet; skipping"),
        }
    }
    Ok(out)
}

fn winget_factory(pool: DbPool) -> ExecutorFactory {
    Arc::new(move || {
        let pool = pool.clone();
        Box::pin(async move {
            let options = WingetOptions {
                log_pool: Some(pool),
                env: Vec::new(),
            };
            let cli = WingetCli::connect(options).await?;
            Ok(Arc::new(cli) as SharedExecutor)
        })
    })
}
