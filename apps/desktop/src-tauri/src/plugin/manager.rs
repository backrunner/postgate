//! Plugin manager for discovering, loading, and managing plugins

use crate::error::{PostGateError, Result};
use crate::plugin::runtime::PluginRuntime;
use crate::plugin::types::*;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use sqlx::sqlite::SqlitePool;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Plugin manager handles discovery, loading, and lifecycle of plugins
pub struct PluginManager {
    /// Directory where plugins are installed
    plugins_dir: PathBuf,
    /// Discovered plugins (not necessarily loaded)
    plugins: DashMap<String, PluginInfo>,
    /// Running plugin runtimes
    runtimes: Arc<RwLock<HashMap<String, PluginRuntime>>>,
    /// Registered panels from plugins
    panels: DashMap<String, PluginPanel>,
    /// Database connection pool for plugin storage
    db_pool: Option<SqlitePool>,
    /// Tauri app handle for emitting events
    app_handle: Option<tauri::AppHandle>,
}

impl PluginManager {
    /// Create a new plugin manager
    pub fn new(plugins_dir: PathBuf) -> Self {
        Self {
            plugins_dir,
            plugins: DashMap::new(),
            runtimes: Arc::new(RwLock::new(HashMap::new())),
            panels: DashMap::new(),
            db_pool: None,
            app_handle: None,
        }
    }

    /// Set the database pool for plugin storage
    pub fn set_db_pool(&mut self, pool: SqlitePool) {
        self.db_pool = Some(pool);
    }

    /// Set the Tauri app handle for emitting events
    pub fn set_app_handle(&mut self, handle: tauri::AppHandle) {
        self.app_handle = Some(handle);
    }

    /// Initialize the plugin manager and discover plugins
    pub async fn init(&self) -> Result<()> {
        // Create plugins directory if it doesn't exist
        if !self.plugins_dir.exists() {
            tokio::fs::create_dir_all(&self.plugins_dir).await
                .map_err(|e| PostGateError::Plugin(format!("Failed to create plugins directory: {}", e)))?;
        }

        // Discover plugins
        self.discover_plugins().await?;

        Ok(())
    }

    /// Discover plugins in the plugins directory
    pub async fn discover_plugins(&self) -> Result<Vec<PluginInfo>> {
        self.plugins.clear();

        let mut entries = tokio::fs::read_dir(&self.plugins_dir).await
            .map_err(|e| PostGateError::Plugin(format!("Failed to read plugins directory: {}", e)))?;

        while let Some(entry) = entries.next_entry().await
            .map_err(|e| PostGateError::Plugin(format!("Failed to read directory entry: {}", e)))?
        {
            let path = entry.path();
            
            if !path.is_dir() {
                continue;
            }

            // Check for package.json
            let package_json_path = path.join("package.json");
            if !package_json_path.exists() {
                continue;
            }

            // Parse package.json
            match self.parse_plugin_info(&path).await {
                Ok(info) => {
                    tracing::info!("Discovered plugin: {} v{}", info.name, info.version);
                    self.plugins.insert(info.id.clone(), info);
                }
                Err(e) => {
                    tracing::warn!("Failed to parse plugin at {:?}: {}", path, e);
                }
            }
        }

        Ok(self.get_plugins())
    }

    /// Parse plugin info from package.json
    async fn parse_plugin_info(&self, plugin_path: &PathBuf) -> Result<PluginInfo> {
        let package_json_path = plugin_path.join("package.json");
        let content = tokio::fs::read_to_string(&package_json_path).await
            .map_err(|e| PostGateError::Plugin(format!("Failed to read package.json: {}", e)))?;

        let package: PackageJson = serde_json::from_str(&content)
            .map_err(|e| PostGateError::Plugin(format!("Failed to parse package.json: {}", e)))?;

        // Validate plugin name
        let id = if package.name.starts_with("postgate-plugin-") {
            package.name.strip_prefix("postgate-plugin-").unwrap().to_string()
        } else if package.name.starts_with("@postgate/plugin-") {
            package.name.strip_prefix("@postgate/plugin-").unwrap().to_string()
        } else {
            return Err(PostGateError::Plugin(
                "Plugin name must start with 'postgate-plugin-' or '@postgate/plugin-'".into()
            ));
        };

        // Determine entry point
        let entry = package.main
            .or(package.module)
            .unwrap_or_else(|| "index.js".to_string());

        let entry_path = plugin_path.join(&entry);
        if !entry_path.exists() {
            return Err(PostGateError::Plugin(format!("Entry point not found: {}", entry)));
        }

        Ok(PluginInfo {
            id,
            name: package.name,
            version: package.version,
            description: package.description,
            author: package.author,
            path: plugin_path.to_string_lossy().to_string(),
            entry,
            enabled: false,
            loaded: false,
        })
    }

    /// Get all discovered plugins
    pub fn get_plugins(&self) -> Vec<PluginInfo> {
        self.plugins.iter().map(|r| r.value().clone()).collect()
    }

    /// Get a specific plugin
    pub fn get_plugin(&self, id: &str) -> Option<PluginInfo> {
        self.plugins.get(id).map(|r| r.value().clone())
    }

    /// Load a plugin
    pub async fn load_plugin(&self, id: &str, config: HashMap<String, String>) -> Result<()> {
        let info = self.plugins.get(id)
            .ok_or_else(|| PostGateError::Plugin(format!("Plugin not found: {}", id)))?
            .clone();

        if info.loaded {
            return Ok(());
        }

        // Ensure we have a database pool
        let db_pool = self.db_pool.clone()
            .ok_or_else(|| PostGateError::Plugin("Database pool not initialized".into()))?;

        let plugin_path = PathBuf::from(&info.path).join(&info.entry);
        
        let mut runtime = PluginRuntime::new(id.to_string(), plugin_path);
        runtime.start(config, db_pool, self.app_handle.clone()).await?;

        // Update plugin state
        if let Some(mut entry) = self.plugins.get_mut(id) {
            entry.loaded = true;
            entry.enabled = true;
        }

        // Store runtime
        let mut runtimes = self.runtimes.write().await;
        runtimes.insert(id.to_string(), runtime);

        tracing::info!("Loaded plugin: {}", id);

        Ok(())
    }

    /// Unload a plugin
    pub async fn unload_plugin(&self, id: &str) -> Result<()> {
        let mut runtimes = self.runtimes.write().await;
        
        if let Some(mut runtime) = runtimes.remove(id) {
            runtime.stop().await?;
        }

        // Update plugin state
        if let Some(mut entry) = self.plugins.get_mut(id) {
            entry.loaded = false;
        }

        // Remove panels registered by this plugin
        self.panels.retain(|_, panel| panel.plugin_id != id);

        tracing::info!("Unloaded plugin: {}", id);

        Ok(())
    }

    /// Enable/disable a plugin
    pub async fn set_plugin_enabled(&self, id: &str, enabled: bool) -> Result<()> {
        if enabled {
            self.load_plugin(id, HashMap::new()).await?;
        } else {
            self.unload_plugin(id).await?;
        }

        if let Some(mut entry) = self.plugins.get_mut(id) {
            entry.enabled = enabled;
        }

        Ok(())
    }

    /// Handle a request through a plugin
    pub async fn handle_request(
        &self,
        plugin_id: &str,
        request: PluginRequest,
        context: PluginRequestContext,
    ) -> Result<Option<PluginResponse>> {
        let runtimes = self.runtimes.read().await;
        
        let runtime = runtimes.get(plugin_id)
            .ok_or_else(|| PostGateError::Plugin(format!("Plugin not loaded: {}", plugin_id)))?;

        runtime.handle_request(request, context).await
    }

    /// Handle response modification through a plugin
    pub async fn handle_response(
        &self,
        plugin_id: &str,
        request: PluginRequest,
        response: PluginResponse,
        context: PluginRequestContext,
    ) -> Result<PluginResponse> {
        let runtimes = self.runtimes.read().await;
        
        let runtime = runtimes.get(plugin_id)
            .ok_or_else(|| PostGateError::Plugin(format!("Plugin not loaded: {}", plugin_id)))?;

        runtime.handle_response(request, response, context).await
    }

    /// Get all registered panels
    pub fn get_panels(&self) -> Vec<PluginPanel> {
        self.panels.iter().map(|r| r.value().clone()).collect()
    }

    /// Register a panel (called from plugin runtime)
    pub fn register_panel(&self, panel: PluginPanel) {
        self.panels.insert(panel.id.clone(), panel);
    }

    /// Unregister a panel
    pub fn unregister_panel(&self, panel_id: &str) {
        self.panels.remove(panel_id);
    }

    /// Shutdown all plugins
    pub async fn shutdown(&self) -> Result<()> {
        let mut runtimes = self.runtimes.write().await;
        
        for (id, mut runtime) in runtimes.drain() {
            if let Err(e) = runtime.stop().await {
                tracing::warn!("Error stopping plugin {}: {}", id, e);
            }
        }

        Ok(())
    }
}

/// package.json structure for plugins
#[derive(Debug, Deserialize)]
struct PackageJson {
    name: String,
    version: String,
    description: Option<String>,
    author: Option<String>,
    main: Option<String>,
    module: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_plugin_discovery() {
        let temp = tempdir().unwrap();
        let plugins_dir = temp.path().to_path_buf();

        // Create a mock plugin
        let plugin_dir = plugins_dir.join("postgate-plugin-test");
        std::fs::create_dir_all(&plugin_dir).unwrap();
        
        let package_json = r#"{
            "name": "postgate-plugin-test",
            "version": "1.0.0",
            "description": "Test plugin",
            "main": "index.js"
        }"#;
        std::fs::write(plugin_dir.join("package.json"), package_json).unwrap();
        std::fs::write(plugin_dir.join("index.js"), "module.exports = {}").unwrap();

        let manager = PluginManager::new(plugins_dir);
        let plugins = manager.discover_plugins().await.unwrap();

        assert_eq!(plugins.len(), 1);
        assert_eq!(plugins[0].id, "test");
        assert_eq!(plugins[0].name, "postgate-plugin-test");
        assert_eq!(plugins[0].version, "1.0.0");
    }
}
