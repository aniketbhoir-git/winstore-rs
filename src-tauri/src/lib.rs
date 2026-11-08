pub mod config;
pub mod db;
pub mod domain;
pub mod error;

use std::path::PathBuf;

use directories::ProjectDirs;
use tauri::Manager;
use tokio::sync::OnceCell;
use tracing_subscriber::EnvFilter;

/// Shared database pool, filled once the background init task finishes.
#[derive(Default)]
pub struct DbState(pub OnceCell<db::DbPool>);

#[tauri::command]
fn app_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

fn db_path() -> Option<PathBuf> {
    ProjectDirs::from("app", "store", "Store").map(|d| d.data_dir().join("store.db"))
}

pub fn run() {
    let filter = EnvFilter::try_from_env("STORE_LOG").unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    let config = match config::AppConfig::load_or_default() {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("could not read config, using defaults: {e}");
            config::AppConfig::default()
        }
    };
    tracing::debug!(?config, "config loaded");

    let db_path = db_path();

    let result = tauri::Builder::default()
        .manage(DbState::default())
        .setup(move |app| {
            let handle = app.handle().clone();
            let path = db_path.clone();
            tauri::async_runtime::spawn(async move {
                let Some(path) = path else {
                    tracing::error!("cannot resolve data directory; database disabled");
                    return;
                };
                let pool = match db::init_pool(&path).await {
                    Ok(p) => p,
                    Err(e) => {
                        tracing::error!("database init failed: {e}");
                        return;
                    }
                };
                if let Err(e) = db::run_migrations(&pool).await {
                    tracing::error!("database migration failed: {e}");
                    return;
                }
                tracing::info!(path = %path.display(), "database ready");
                let state = handle.state::<DbState>();
                let _ = state.0.set(pool);
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![app_version])
        .run(tauri::generate_context!());

    if let Err(e) = result {
        tracing::error!("application error: {e}");
    }
}
