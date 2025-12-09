//! Plugin management Tauri commands

use crate::error::Result;
use crate::plugin::{PluginInfo, PluginManager, PluginPanel};
use crate::state::AppState;
use std::collections::HashMap;
use std::sync::Arc;
use tauri::State;

/// Get list of all discovered plugins
#[tauri::command]
pub async fn get_plugins(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<PluginInfo>> {
    let manager = state.plugin_manager.read().await;
    Ok(manager.get_plugins())
}

/// Discover plugins in the plugins directory
#[tauri::command]
pub async fn discover_plugins(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<PluginInfo>> {
    let manager = state.plugin_manager.read().await;
    manager.discover_plugins().await
}

/// Load a plugin
#[tauri::command]
pub async fn load_plugin(
    state: State<'_, Arc<AppState>>,
    plugin_id: String,
    config: Option<HashMap<String, String>>,
) -> Result<()> {
    let manager = state.plugin_manager.read().await;
    manager.load_plugin(&plugin_id, config.unwrap_or_default()).await
}

/// Unload a plugin
#[tauri::command]
pub async fn unload_plugin(
    state: State<'_, Arc<AppState>>,
    plugin_id: String,
) -> Result<()> {
    let manager = state.plugin_manager.read().await;
    manager.unload_plugin(&plugin_id).await
}

/// Enable or disable a plugin
#[tauri::command]
pub async fn toggle_plugin(
    state: State<'_, Arc<AppState>>,
    plugin_id: String,
    enabled: bool,
) -> Result<()> {
    let manager = state.plugin_manager.read().await;
    manager.set_plugin_enabled(&plugin_id, enabled).await
}

/// Get a specific plugin's info
#[tauri::command]
pub async fn get_plugin(
    state: State<'_, Arc<AppState>>,
    plugin_id: String,
) -> Result<Option<PluginInfo>> {
    let manager = state.plugin_manager.read().await;
    Ok(manager.get_plugin(&plugin_id))
}

/// Get all registered UI panels from plugins
#[tauri::command]
pub async fn get_plugin_panels(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<PluginPanel>> {
    let manager = state.plugin_manager.read().await;
    Ok(manager.get_panels())
}

/// Get the plugins directory path
#[tauri::command]
pub async fn get_plugins_dir(
    state: State<'_, Arc<AppState>>,
) -> Result<String> {
    Ok(state.plugins_dir.to_string_lossy().to_string())
}

/// Install a plugin from npm (placeholder - would need npm CLI integration)
#[tauri::command]
pub async fn install_plugin(
    _state: State<'_, Arc<AppState>>,
    _package_name: String,
) -> Result<PluginInfo> {
    // This would require integration with npm CLI
    // For now, return an error indicating manual installation
    Err(crate::error::PostGateError::Plugin(
        "Plugin installation from npm is not yet supported. \
         Please install plugins manually to the plugins directory.".into()
    ))
}

/// Uninstall a plugin (remove from plugins directory)
#[tauri::command]
pub async fn uninstall_plugin(
    state: State<'_, Arc<AppState>>,
    plugin_id: String,
) -> Result<()> {
    // First unload the plugin
    {
        let manager = state.plugin_manager.read().await;
        let _ = manager.unload_plugin(&plugin_id).await;
    }

    // Get plugin info to find the path
    let plugin_path = {
        let manager = state.plugin_manager.read().await;
        manager.get_plugin(&plugin_id)
            .map(|p| p.path.clone())
            .ok_or_else(|| crate::error::PostGateError::NotFound(format!("Plugin not found: {}", plugin_id)))?
    };

    // Remove the plugin directory
    tokio::fs::remove_dir_all(&plugin_path).await
        .map_err(|e| crate::error::PostGateError::Plugin(format!("Failed to remove plugin: {}", e)))?;

    // Rediscover plugins
    {
        let manager = state.plugin_manager.read().await;
        manager.discover_plugins().await?;
    }

    Ok(())
}
