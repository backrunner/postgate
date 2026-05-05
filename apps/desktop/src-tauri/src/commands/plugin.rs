//! Plugin management Tauri commands

use crate::error::{PostGateError, Result};
use crate::plugin::{PluginInfo, PluginPanel};
use crate::state::AppState;
use std::collections::HashMap;
use std::sync::Arc;
use tauri::State;

/// Get list of all discovered plugins
#[tauri::command]
pub async fn get_plugins(state: State<'_, Arc<AppState>>) -> Result<Vec<PluginInfo>> {
    let manager = state.plugin_manager.read().await;
    Ok(manager.get_plugins())
}

/// Discover plugins in the plugins directory
#[tauri::command]
pub async fn discover_plugins(state: State<'_, Arc<AppState>>) -> Result<Vec<PluginInfo>> {
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
    manager
        .load_plugin(&plugin_id, config.unwrap_or_default())
        .await
}

/// Unload a plugin
#[tauri::command]
pub async fn unload_plugin(state: State<'_, Arc<AppState>>, plugin_id: String) -> Result<()> {
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
pub async fn get_plugin_panels(state: State<'_, Arc<AppState>>) -> Result<Vec<PluginPanel>> {
    let manager = state.plugin_manager.read().await;
    Ok(manager.get_panels())
}

/// Get the plugins directory path
#[tauri::command]
pub async fn get_plugins_dir(state: State<'_, Arc<AppState>>) -> Result<String> {
    Ok(state.plugins_dir.to_string_lossy().to_string())
}

/// Install a plugin from npm registry
#[tauri::command]
pub async fn install_plugin_from_npm(
    state: State<'_, Arc<AppState>>,
    package_name: String,
) -> Result<PluginInfo> {
    // Validate package name format
    if !package_name.starts_with("postgate-plugin-")
        && !package_name.starts_with("@postgate/plugin-")
    {
        return Err(PostGateError::Plugin(
            format!("Invalid package name '{}'. Plugin names must start with 'postgate-plugin-' or '@postgate/plugin-'", package_name)
        ));
    }

    let plugins_dir = state.plugins_dir.clone();

    // Create temp directory for download
    let temp_dir = tempfile::tempdir()
        .map_err(|e| PostGateError::Plugin(format!("Failed to create temp dir: {}", e)))?;

    // Run npm pack to download the package
    let output = tokio::process::Command::new("npm")
        .args([
            "pack",
            &package_name,
            "--pack-destination",
            temp_dir.path().to_str().unwrap_or("."),
        ])
        .output()
        .await
        .map_err(|e| {
            PostGateError::Plugin(format!(
                "Failed to run npm pack: {}. Make sure npm is installed.",
                e
            ))
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(PostGateError::Plugin(format!(
            "npm pack failed: {}",
            stderr
        )));
    }

    // Find the downloaded .tgz file
    let mut tgz_file = None;
    let mut entries = tokio::fs::read_dir(temp_dir.path())
        .await
        .map_err(|e| PostGateError::Plugin(format!("Failed to read temp dir: {}", e)))?;

    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|e| PostGateError::Plugin(format!("Failed to read temp dir entry: {}", e)))?
    {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.ends_with(".tgz") {
            tgz_file = Some(entry.path());
            break;
        }
    }

    let tgz_path = tgz_file
        .ok_or_else(|| PostGateError::Plugin("npm pack did not produce a .tgz file".into()))?;

    // Extract the tarball
    let extract_dir = temp_dir.path().join("extracted");
    tokio::fs::create_dir_all(&extract_dir)
        .await
        .map_err(|e| PostGateError::Plugin(format!("Failed to create extract dir: {}", e)))?;

    // Decompress and extract in a blocking task
    let tgz_path_clone = tgz_path.clone();
    let extract_dir_clone = extract_dir.clone();
    tokio::task::spawn_blocking(move || {
        let file = std::fs::File::open(&tgz_path_clone)
            .map_err(|e| PostGateError::Plugin(format!("Failed to open .tgz: {}", e)))?;
        let decoder = flate2::read::GzDecoder::new(file);
        let mut archive = tar::Archive::new(decoder);
        archive
            .unpack(&extract_dir_clone)
            .map_err(|e| PostGateError::Plugin(format!("Failed to extract .tgz: {}", e)))?;
        Ok::<(), PostGateError>(())
    })
    .await
    .map_err(|e| PostGateError::Plugin(format!("Extract task failed: {}", e)))??;

    // Find the extracted package directory (usually "package/")
    let package_dir = extract_dir.join("package");
    if !package_dir.exists() {
        return Err(PostGateError::Plugin(
            "Extracted tarball did not contain a 'package' directory".into(),
        ));
    }

    // Read and validate package.json
    let package_json_path = package_dir.join("package.json");
    let package_json_content = tokio::fs::read_to_string(&package_json_path)
        .await
        .map_err(|e| PostGateError::Plugin(format!("Failed to read package.json: {}", e)))?;
    let package: serde_json::Value = serde_json::from_str(&package_json_content)
        .map_err(|e| PostGateError::Plugin(format!("Invalid package.json: {}", e)))?;
    let actual_name = package
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| PostGateError::Plugin("package.json missing name field".into()))?;

    // Determine target directory name
    let target_dir_name = if actual_name.starts_with("@postgate/") {
        actual_name.replace('/', "-")
    } else {
        actual_name.to_string()
    };
    let target_dir = plugins_dir.join(&target_dir_name);

    // Remove existing plugin directory if it exists
    if target_dir.exists() {
        tokio::fs::remove_dir_all(&target_dir).await.map_err(|e| {
            PostGateError::Plugin(format!("Failed to remove existing plugin: {}", e))
        })?;
    }

    // Move the extracted package to the plugins directory
    tokio::fs::rename(&package_dir, &target_dir)
        .await
        .map_err(|e| {
            PostGateError::Plugin(format!("Failed to move plugin to plugins dir: {}", e))
        })?;

    // Rediscover plugins and return the installed plugin info
    let manager = state.plugin_manager.read().await;
    let plugins = manager.discover_plugins().await?;

    plugins
        .into_iter()
        .find(|p| p.name == actual_name)
        .ok_or_else(|| {
            PostGateError::Plugin("Plugin installed but not found after discovery".into())
        })
}

/// Install a plugin from a local directory path
#[tauri::command]
pub async fn install_plugin_from_path(
    state: State<'_, Arc<AppState>>,
    source_path: String,
) -> Result<PluginInfo> {
    let source = std::path::PathBuf::from(&source_path);

    // Validate source path
    if !source.exists() || !source.is_dir() {
        return Err(PostGateError::Plugin(format!(
            "Source path '{}' does not exist or is not a directory",
            source_path
        )));
    }

    // Read and validate package.json
    let package_json_path = source.join("package.json");
    let package_json_content = tokio::fs::read_to_string(&package_json_path)
        .await
        .map_err(|e| PostGateError::Plugin(format!("Failed to read package.json: {}", e)))?;
    let package: serde_json::Value = serde_json::from_str(&package_json_content)
        .map_err(|e| PostGateError::Plugin(format!("Invalid package.json: {}", e)))?;
    let package_name = package
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| PostGateError::Plugin("package.json missing name field".into()))?;

    // Validate plugin name
    if !package_name.starts_with("postgate-plugin-")
        && !package_name.starts_with("@postgate/plugin-")
    {
        return Err(PostGateError::Plugin(
            format!("Invalid package name '{}'. Plugin names must start with 'postgate-plugin-' or '@postgate/plugin-'", package_name)
        ));
    }

    let plugins_dir = state.plugins_dir.clone();
    let target_dir_name = if package_name.starts_with("@postgate/") {
        package_name.replace('/', "-")
    } else {
        package_name.to_string()
    };
    let target_dir = plugins_dir.join(&target_dir_name);

    // Remove existing plugin directory if it exists
    if target_dir.exists() {
        tokio::fs::remove_dir_all(&target_dir).await.map_err(|e| {
            PostGateError::Plugin(format!("Failed to remove existing plugin: {}", e))
        })?;
    }

    // Copy the source directory to the plugins directory
    copy_dir_recursive(&source, &target_dir)
        .await
        .map_err(|e| PostGateError::Plugin(format!("Failed to copy plugin directory: {}", e)))?;

    // Rediscover plugins and return the installed plugin info
    let manager = state.plugin_manager.read().await;
    let plugins = manager.discover_plugins().await?;

    plugins
        .into_iter()
        .find(|p| p.name == package_name)
        .ok_or_else(|| PostGateError::Plugin("Plugin copied but not found after discovery".into()))
}

/// Legacy install_plugin command — redirects to npm install
#[tauri::command]
pub async fn install_plugin(
    state: State<'_, Arc<AppState>>,
    package_name: String,
) -> Result<PluginInfo> {
    install_plugin_from_npm(state, package_name).await
}

/// Uninstall a plugin (remove from plugins directory)
#[tauri::command]
pub async fn uninstall_plugin(state: State<'_, Arc<AppState>>, plugin_id: String) -> Result<()> {
    // First unload the plugin
    {
        let manager = state.plugin_manager.read().await;
        let _ = manager.unload_plugin(&plugin_id).await;
    }

    // Get plugin info to find the path
    let plugin_path = {
        let manager = state.plugin_manager.read().await;
        manager
            .get_plugin(&plugin_id)
            .map(|p| p.path.clone())
            .ok_or_else(|| {
                crate::error::PostGateError::NotFound(format!("Plugin not found: {}", plugin_id))
            })?
    };

    // Remove the plugin directory
    tokio::fs::remove_dir_all(&plugin_path).await.map_err(|e| {
        crate::error::PostGateError::Plugin(format!("Failed to remove plugin: {}", e))
    })?;

    // Rediscover plugins
    {
        let manager = state.plugin_manager.read().await;
        manager.discover_plugins().await?;
    }

    Ok(())
}

/// Copy a directory recursively
async fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    tokio::fs::create_dir_all(dst).await?;
    let mut entries = tokio::fs::read_dir(src).await?;

    while let Some(entry) = entries.next_entry().await? {
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if entry.file_type().await?.is_dir() {
            Box::pin(copy_dir_recursive(&src_path, &dst_path)).await?;
        } else {
            tokio::fs::copy(&src_path, &dst_path).await?;
        }
    }

    Ok(())
}
