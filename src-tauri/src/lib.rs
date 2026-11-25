pub mod catalog;
pub mod config;
pub mod db;
pub mod domain;
pub mod error;
pub mod executor;
pub mod ipc;
pub mod platform;
pub mod services;
pub mod sources;

use tauri::Manager;
use tracing_subscriber::EnvFilter;

use ipc::{commands, state::AppState};

pub fn run() {
    let filter = EnvFilter::try_from_env("STORE_LOG").unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    let notifier = std::sync::Arc::new(platform::windows::WindowsNotifier::default());
    let focus = std::sync::Arc::new(platform::windows::TauriWindowFocus::default());
    let toasts = platform::windows::ToastDeps {
        notifier: notifier.clone(),
        focus: focus.clone(),
    };

    let result = tauri::Builder::default()
        .manage(AppState::with_toasts(toasts))
        .setup(move |app| {
            notifier.attach(app.handle());
            focus.attach(app.handle());
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
            commands::app_status,
            commands::search_sources,
            commands::get_app_detail,
            commands::start_install,
            commands::cancel_install,
            commands::start_uninstall,
            commands::get_install_status,
            commands::get_operation_log,
            commands::check_updates,
            commands::update_all,
            commands::open_store_page,
            commands::open_agreement_link,
            commands::start_server,
            commands::export_packages,
            commands::open_exports_folder,
        ])
        .run(tauri::generate_context!());

    if let Err(e) = result {
        tracing::error!("application error: {e}");
    }
}
