mod cert;
mod commands;
mod error;
mod proxy;
mod rules;
mod storage;
mod state;

use state::AppState;
use std::sync::Arc;
use tauri::Manager;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Initialize logging
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "postgate=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::info!("Starting PostGate...");

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            let app_handle = app.handle().clone();
            
            // Initialize app state
            let state = Arc::new(AppState::new(app_handle));
            app.manage(state);

            tracing::info!("PostGate initialized successfully");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::proxy::start_proxy,
            commands::proxy::stop_proxy,
            commands::proxy::get_proxy_status,
            commands::proxy::get_request_body,
            commands::proxy::get_response_body,
            commands::proxy::clear_captured_data,
            commands::cert::get_ca_certificate,
            commands::cert::install_ca_certificate,
            commands::cert::export_ca_certificate,
            commands::rules::get_rule_groups,
            commands::rules::save_rule_group,
            commands::rules::delete_rule_group,
            commands::rules::toggle_rule_group,
            commands::rules::parse_rules,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
