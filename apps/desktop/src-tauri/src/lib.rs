mod cert;
mod commands;
mod debug;
mod error;
mod plugin;
mod proxy;
mod replay;
mod rules;
mod storage;
mod state;

use state::AppState;
use std::sync::Arc;
use tauri::Manager;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Install the default CryptoProvider for rustls (ring)
    // This must be done before any TLS operations
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

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
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            let app_handle = app.handle().clone();
            
            // Initialize app state
            let state = Arc::new(AppState::new(app_handle.clone()));
            
            // Initialize plugin system asynchronously
            let state_clone = state.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = initialize_plugin_system(state_clone, app_handle).await {
                    tracing::error!("Failed to initialize plugin system: {}", e);
                }
            });
            
            app.manage(state);

            tracing::info!("PostGate initialized successfully");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::window::show_main_window,
            commands::proxy::start_proxy,
            commands::proxy::stop_proxy,
            commands::proxy::get_proxy_status,
            commands::proxy::get_request_body,
            commands::proxy::get_response_body,
            commands::proxy::clear_captured_data,
            commands::proxy::load_captured_history,
            commands::proxy::get_persisted_request_body,
            commands::proxy::get_persisted_response_body,
            commands::proxy::clear_captured_history,
            commands::proxy::clear_captured_history_before,
            commands::proxy::set_persistence_enabled,
            commands::proxy::get_persistence_enabled,
            commands::proxy::get_captured_history_count,
            commands::proxy::get_local_ip,
            commands::cert::get_ca_certificate,
            commands::cert::install_ca_certificate,
            commands::cert::export_ca_certificate,
            commands::rules::get_rule_groups,
            commands::rules::save_rule_group,
            commands::rules::delete_rule_group,
            commands::rules::toggle_rule_group,
            commands::rules::parse_rules,
            commands::rules::has_active_debug_rules,
            commands::plugin::get_plugins,
            commands::plugin::discover_plugins,
            commands::plugin::load_plugin,
            commands::plugin::unload_plugin,
            commands::plugin::toggle_plugin,
            commands::plugin::get_plugin,
            commands::plugin::get_plugin_panels,
            commands::plugin::get_plugins_dir,
            commands::plugin::install_plugin,
            commands::plugin::uninstall_plugin,
            commands::replay::get_collection_tree,
            commands::replay::get_collections,
            commands::replay::create_collection,
            commands::replay::update_collection,
            commands::replay::delete_collection,
            commands::replay::get_saved_requests,
            commands::replay::get_saved_request,
            commands::replay::create_saved_request,
            commands::replay::update_saved_request,
            commands::replay::delete_saved_request,
            commands::replay::move_request,
            commands::replay::duplicate_request,
            commands::replay::execute_saved_request,
            commands::replay::get_request_history,
            commands::replay::clear_request_history,
            commands::replay::import_from_capture,
            commands::debug::start_debug_server,
            commands::debug::stop_debug_server,
            commands::debug::get_debug_status,
            commands::debug::get_debug_sessions,
            commands::debug::get_console_logs,
            commands::debug::clear_console_logs,
            commands::debug::get_page_errors,
            commands::debug::clear_all_debug_data,
            commands::debug::remove_debug_session,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Initialize the plugin system
async fn initialize_plugin_system(
    state: Arc<AppState>,
    app_handle: tauri::AppHandle,
) -> error::Result<()> {
    // Get database pool
    let db = state.get_database().await?;
    let pool = db.pool().clone();

    // Initialize plugin storage table
    plugin::PluginStorage::init_table(&pool).await?;

    // Configure plugin manager with db and app handle
    {
        let mut manager = state.plugin_manager.write().await;
        manager.set_db_pool(pool);
        manager.set_app_handle(app_handle);
    }

    // Initialize and discover plugins
    {
        let manager = state.plugin_manager.read().await;
        manager.init().await?;
    }

    tracing::info!("Plugin system initialized");
    Ok(())
}
