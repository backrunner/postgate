//! Plugin management Tauri commands

use crate::error::{PostGateError, Result};
use crate::plugin::{plugin_identity, PluginInfo, PluginPanel, PluginState};
use crate::state::AppState;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::State;
use uuid::Uuid;

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
    let (requested_plugin_id, _) = plugin_identity(&package_name)?;

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
    let (plugin_id, target_dir_name) = plugin_identity(actual_name)?;
    if actual_name != package_name || plugin_id != requested_plugin_id {
        return Err(PostGateError::Plugin(format!(
            "Downloaded package identity mismatch: requested '{package_name}', archive contains '{actual_name}'"
        )));
    }
    validate_plugin_package(&package_dir, actual_name).await?;

    let target_dir = plugins_dir.join(&target_dir_name);
    let previous_state = suspend_existing_plugin(state.inner(), &plugin_id).await?;

    let transaction = match install_plugin_directory(&package_dir, &target_dir).await {
        Ok(transaction) => transaction,
        Err(error) => {
            if let Err(restore_error) = restore_plugin_state(state.inner(), previous_state).await {
                tracing::error!(
                    "Failed to restore plugin after install error: {}",
                    restore_error
                );
            }
            return Err(error);
        }
    };

    // Rediscover plugins and return the installed plugin info
    let plugins = {
        let manager = state.plugin_manager.read().await;
        match manager.discover_plugins().await {
            Ok(plugins) => plugins,
            Err(error) => {
                drop(manager);
                rollback_plugin_install(state.inner(), transaction, previous_state).await?;
                return Err(error);
            }
        }
    };

    let installed = match plugins.into_iter().find(|p| p.name == actual_name) {
        Some(plugin) => plugin,
        None => {
            rollback_plugin_install(state.inner(), transaction, previous_state).await?;
            return Err(PostGateError::Plugin(
                "Plugin installed but not found after discovery".into(),
            ));
        }
    };
    if let Err(error) = restore_plugin_state(state.inner(), previous_state.clone()).await {
        rollback_plugin_install(state.inner(), transaction, previous_state).await?;
        return Err(error);
    }
    transaction.commit().await;
    Ok(state
        .plugin_manager
        .read()
        .await
        .get_plugin(&installed.id)
        .unwrap_or(installed))
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

    let (plugin_id, target_dir_name) = plugin_identity(package_name)?;
    validate_plugin_package(&source, package_name).await?;

    let plugins_dir = state.plugins_dir.clone();
    let target_dir = plugins_dir.join(&target_dir_name);
    tokio::fs::create_dir_all(&plugins_dir)
        .await
        .map_err(|e| PostGateError::Plugin(format!("Failed to create plugins dir: {}", e)))?;

    let source_canonical = tokio::fs::canonicalize(&source).await.map_err(|e| {
        PostGateError::Plugin(format!("Failed to resolve source plugin path: {}", e))
    })?;
    if target_dir.exists() {
        let target_canonical = tokio::fs::canonicalize(&target_dir).await.map_err(|e| {
            PostGateError::Plugin(format!("Failed to resolve installed plugin path: {}", e))
        })?;
        if source_canonical == target_canonical {
            let manager = state.plugin_manager.read().await;
            let plugins = manager.discover_plugins().await?;
            return plugins
                .into_iter()
                .find(|plugin| plugin.id == plugin_id)
                .ok_or_else(|| {
                    PostGateError::Plugin(
                        "Plugin exists in the plugins directory but could not be discovered".into(),
                    )
                });
        }
    }

    let previous_state = suspend_existing_plugin(state.inner(), &plugin_id).await?;

    let transaction = match install_plugin_directory(&source, &target_dir).await {
        Ok(transaction) => transaction,
        Err(error) => {
            if let Err(restore_error) = restore_plugin_state(state.inner(), previous_state).await {
                tracing::error!(
                    "Failed to restore plugin after install error: {}",
                    restore_error
                );
            }
            return Err(error);
        }
    };

    // Rediscover plugins and return the installed plugin info
    let plugins = {
        let manager = state.plugin_manager.read().await;
        match manager.discover_plugins().await {
            Ok(plugins) => plugins,
            Err(error) => {
                drop(manager);
                rollback_plugin_install(state.inner(), transaction, previous_state).await?;
                return Err(error);
            }
        }
    };

    let installed = match plugins.into_iter().find(|p| p.name == package_name) {
        Some(plugin) => plugin,
        None => {
            rollback_plugin_install(state.inner(), transaction, previous_state).await?;
            return Err(PostGateError::Plugin(
                "Plugin copied but not found after discovery".into(),
            ));
        }
    };
    if let Err(error) = restore_plugin_state(state.inner(), previous_state.clone()).await {
        rollback_plugin_install(state.inner(), transaction, previous_state).await?;
        return Err(error);
    }
    transaction.commit().await;
    Ok(state
        .plugin_manager
        .read()
        .await
        .get_plugin(&installed.id)
        .unwrap_or(installed))
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

    // Unload first, but do not let a broken onUnload trap the installed files.
    {
        let manager = state.plugin_manager.read().await;
        if let Err(error) = manager.unload_plugin(&plugin_id).await {
            tracing::warn!(
                "Plugin {} failed to unload during uninstall: {}",
                plugin_id,
                error
            );
        }
    }

    // Remove the plugin directory
    tokio::fs::remove_dir_all(&plugin_path).await.map_err(|e| {
        crate::error::PostGateError::Plugin(format!("Failed to remove plugin: {}", e))
    })?;

    // Rediscover plugins
    {
        let manager = state.plugin_manager.read().await;
        manager.discover_plugins().await?;
        manager.remove_plugin_data(&plugin_id).await?;
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

        let file_type = entry.file_type().await?;
        if file_type.is_symlink() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!(
                    "Plugin packages cannot contain symlinks: {}",
                    src_path.display()
                ),
            ));
        }
        if file_type.is_dir() {
            Box::pin(copy_dir_recursive(&src_path, &dst_path)).await?;
        } else {
            tokio::fs::copy(&src_path, &dst_path).await?;
        }
    }

    Ok(())
}

async fn validate_plugin_package(source: &Path, expected_name: &str) -> Result<()> {
    let source_root = tokio::fs::canonicalize(source).await.map_err(|error| {
        PostGateError::Plugin(format!("Failed to resolve plugin package: {error}"))
    })?;
    let package_json_path = source.join("package.json");
    let package_json = tokio::fs::read_to_string(&package_json_path)
        .await
        .map_err(|error| PostGateError::Plugin(format!("Failed to read package.json: {error}")))?;
    let package: serde_json::Value = serde_json::from_str(&package_json)
        .map_err(|error| PostGateError::Plugin(format!("Invalid package.json: {error}")))?;
    let name = package
        .get("name")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| PostGateError::Plugin("package.json missing name field".into()))?;
    if name != expected_name {
        return Err(PostGateError::Plugin(format!(
            "Plugin package identity changed while installing: expected '{expected_name}', found '{name}'"
        )));
    }

    let entry = package
        .get("main")
        .or_else(|| package.get("module"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("index.js");
    let entry_path = Path::new(entry);
    if entry_path.as_os_str().is_empty()
        || !entry_path.components().all(|component| {
            matches!(
                component,
                std::path::Component::Normal(_) | std::path::Component::CurDir
            )
        })
    {
        return Err(PostGateError::Plugin(format!(
            "Plugin entry point must stay inside the package: {entry}"
        )));
    }

    let resolved_entry = tokio::fs::canonicalize(source.join(entry))
        .await
        .map_err(|error| {
            PostGateError::Plugin(format!("Plugin entry point not found ({entry}): {error}"))
        })?;
    let metadata = tokio::fs::symlink_metadata(source.join(entry))
        .await
        .map_err(|error| {
            PostGateError::Plugin(format!("Failed to inspect plugin entry point: {error}"))
        })?;
    if !resolved_entry.starts_with(&source_root)
        || !metadata.is_file()
        || metadata.file_type().is_symlink()
    {
        return Err(PostGateError::Plugin(format!(
            "Plugin entry point must be a regular file inside the package: {entry}"
        )));
    }

    Ok(())
}

async fn suspend_existing_plugin(
    state: &Arc<AppState>,
    plugin_id: &str,
) -> Result<Option<PluginState>> {
    let manager = state.plugin_manager.read().await;
    let Some(plugin) = manager.get_plugin(plugin_id) else {
        return Ok(None);
    };
    let mut saved_state = manager
        .get_saved_state(plugin_id)
        .await?
        .unwrap_or(PluginState {
            id: plugin_id.to_string(),
            enabled: plugin.enabled,
            config: HashMap::new(),
        });
    saved_state.enabled = plugin.enabled;
    if let Err(error) = manager.unload_plugin(plugin_id).await {
        tracing::warn!(
            "Plugin {} did not unload cleanly before update: {}",
            plugin_id,
            error
        );
    }
    Ok(Some(saved_state))
}

async fn restore_plugin_state(
    state: &Arc<AppState>,
    saved_state: Option<PluginState>,
) -> Result<()> {
    let Some(saved_state) = saved_state else {
        return Ok(());
    };
    if !saved_state.enabled {
        return Ok(());
    }
    let manager = state.plugin_manager.read().await;
    manager.restore_saved_state(saved_state).await
}

struct PluginInstallTransaction {
    target: PathBuf,
    backup: Option<PathBuf>,
}

impl PluginInstallTransaction {
    async fn commit(self) {
        if let Some(backup) = self.backup {
            if let Err(error) = tokio::fs::remove_dir_all(&backup).await {
                tracing::warn!(
                    "Failed to remove plugin backup {}: {}",
                    backup.display(),
                    error
                );
            }
        }
    }

    async fn rollback(self) -> Result<()> {
        if self.target.exists() {
            tokio::fs::remove_dir_all(&self.target)
                .await
                .map_err(|error| {
                    PostGateError::Plugin(format!("Failed to remove failed plugin update: {error}"))
                })?;
        }
        if let Some(backup) = self.backup {
            tokio::fs::rename(&backup, &self.target)
                .await
                .map_err(|error| {
                    PostGateError::Plugin(format!("Failed to restore plugin backup: {error}"))
                })?;
        }
        Ok(())
    }
}

async fn install_plugin_directory(
    source: &Path,
    target: &Path,
) -> Result<PluginInstallTransaction> {
    let parent = target.parent().ok_or_else(|| {
        PostGateError::Plugin(format!("Plugin target has no parent: {}", target.display()))
    })?;
    tokio::fs::create_dir_all(parent)
        .await
        .map_err(|e| PostGateError::Plugin(format!("Failed to create plugins dir: {}", e)))?;
    let source = tokio::fs::canonicalize(source).await.map_err(|error| {
        PostGateError::Plugin(format!("Failed to resolve plugin source: {error}"))
    })?;
    let parent = tokio::fs::canonicalize(parent).await.map_err(|error| {
        PostGateError::Plugin(format!("Failed to resolve plugins directory: {error}"))
    })?;
    if parent.starts_with(&source) {
        return Err(PostGateError::Plugin(
            "Plugin source cannot contain the destination plugins directory".into(),
        ));
    }

    let target_name = target
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| PostGateError::Plugin("Invalid plugin target directory".into()))?;
    let operation_id = Uuid::new_v4();
    let target = parent.join(target_name);
    let staging = parent.join(format!(".{target_name}.install-{operation_id}"));
    let backup = parent.join(format!(".{target_name}.backup-{operation_id}"));

    if let Err(error) = copy_dir_recursive(&source, &staging).await {
        let _ = tokio::fs::remove_dir_all(&staging).await;
        return Err(PostGateError::Plugin(format!(
            "Failed to stage plugin directory: {error}"
        )));
    }

    let had_existing = target.exists();
    if had_existing {
        tokio::fs::rename(&target, &backup).await.map_err(|error| {
            PostGateError::Plugin(format!("Failed to stage existing plugin backup: {error}"))
        })?;
    }

    if let Err(error) = tokio::fs::rename(&staging, &target).await {
        if had_existing {
            let _ = tokio::fs::rename(&backup, &target).await;
        }
        let _ = tokio::fs::remove_dir_all(&staging).await;
        return Err(PostGateError::Plugin(format!(
            "Failed to activate plugin directory: {error}"
        )));
    }

    Ok(PluginInstallTransaction {
        target,
        backup: had_existing.then_some(backup),
    })
}

async fn rollback_plugin_install(
    state: &Arc<AppState>,
    transaction: PluginInstallTransaction,
    previous_state: Option<PluginState>,
) -> Result<()> {
    transaction.rollback().await?;
    {
        let manager = state.plugin_manager.read().await;
        manager.discover_plugins().await?;
    }
    restore_plugin_state(state, previous_state).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn staged_install_replaces_existing_directory() {
        let temp = tempdir().unwrap();
        let source = temp.path().join("source");
        let target = temp.path().join("postgate-plugin-test");
        tokio::fs::create_dir_all(&source).await.unwrap();
        tokio::fs::create_dir_all(&target).await.unwrap();
        tokio::fs::write(source.join("index.js"), "new")
            .await
            .unwrap();
        tokio::fs::write(target.join("index.js"), "old")
            .await
            .unwrap();

        install_plugin_directory(&source, &target)
            .await
            .unwrap()
            .commit()
            .await;

        assert_eq!(
            tokio::fs::read_to_string(target.join("index.js"))
                .await
                .unwrap(),
            "new"
        );
        let mut entries = tokio::fs::read_dir(temp.path()).await.unwrap();
        while let Some(entry) = entries.next_entry().await.unwrap() {
            let name = entry.file_name().to_string_lossy().to_string();
            assert!(!name.contains(".install-"));
            assert!(!name.contains(".backup-"));
        }
    }

    #[tokio::test]
    async fn staged_install_can_roll_back_existing_directory() {
        let temp = tempdir().unwrap();
        let source = temp.path().join("source");
        let target = temp.path().join("postgate-plugin-test");
        tokio::fs::create_dir_all(&source).await.unwrap();
        tokio::fs::create_dir_all(&target).await.unwrap();
        tokio::fs::write(source.join("index.js"), "new")
            .await
            .unwrap();
        tokio::fs::write(target.join("index.js"), "old")
            .await
            .unwrap();

        install_plugin_directory(&source, &target)
            .await
            .unwrap()
            .rollback()
            .await
            .unwrap();

        assert_eq!(
            tokio::fs::read_to_string(target.join("index.js"))
                .await
                .unwrap(),
            "old"
        );
    }

    #[tokio::test]
    async fn package_validation_rejects_entry_outside_plugin_directory() {
        let temp = tempdir().unwrap();
        let source = temp.path().join("source");
        tokio::fs::create_dir_all(&source).await.unwrap();
        tokio::fs::write(temp.path().join("outside.js"), "outside")
            .await
            .unwrap();
        tokio::fs::write(
            source.join("package.json"),
            r#"{"name":"postgate-plugin-test","main":"../outside.js"}"#,
        )
        .await
        .unwrap();

        assert!(validate_plugin_package(&source, "postgate-plugin-test")
            .await
            .is_err());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn staged_install_rejects_symlinks_without_replacing_existing_plugin() {
        use std::os::unix::fs::symlink;

        let temp = tempdir().unwrap();
        let source = temp.path().join("source");
        let target = temp.path().join("postgate-plugin-test");
        tokio::fs::create_dir_all(&source).await.unwrap();
        tokio::fs::create_dir_all(&target).await.unwrap();
        tokio::fs::write(temp.path().join("outside.js"), "outside")
            .await
            .unwrap();
        symlink(temp.path().join("outside.js"), source.join("index.js")).unwrap();
        tokio::fs::write(target.join("index.js"), "old")
            .await
            .unwrap();

        assert!(install_plugin_directory(&source, &target).await.is_err());
        assert_eq!(
            tokio::fs::read_to_string(target.join("index.js"))
                .await
                .unwrap(),
            "old"
        );
    }
}
