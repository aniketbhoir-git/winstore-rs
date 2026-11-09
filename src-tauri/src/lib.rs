pub mod catalog;
pub mod config;
pub mod db;
pub mod domain;
pub mod error;
pub mod executor;
pub mod ipc;
pub mod sources;

use tauri::Manager;
use tracing_subscriber::EnvFilter;

use ipc::{commands, state::AppState};

pub fn run() {
    let filter = EnvFilter::try_from_env("STORE_LOG").unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    let result = tauri::Builder::default()
        .manage(AppState::new())
        .setup(|app| {
            // Warm up the database in the background; commands also build it on first use.
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = handle.state::<AppState>().core().await {
                    tracing::error!("start-up failed: {e}");
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_version,
            commands::search_sources,
            commands::get_app_detail,
            commands::start_install,
            commands::cancel_install,
            commands::check_updates,
            commands::update_all,
            commands::open_store_page,
            commands::start_server,
        ])
        .run(tauri::generate_context!());

    if let Err(e) = result {
        tracing::error!("application error: {e}");
    }
}
