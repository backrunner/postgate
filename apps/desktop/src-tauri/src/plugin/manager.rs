//! Plugin manager for discovering, loading, and managing plugins

use crate::error::{PostGateError, Result};
use crate::plugin::runtime::PluginRuntime;
use crate::plugin::storage::PluginStorage;
use crate::plugin::types::*;
use dashmap::DashMap;
use serde::Deserialize;
use sqlx::sqlite::SqlitePool;
use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};
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
    panels: Arc<DashMap<String, PluginPanel>>,
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
            panels: Arc::new(DashMap::new()),
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
            tokio::fs::create_dir_all(&self.plugins_dir)
                .await
                .map_err(|e| {
                    PostGateError::Plugin(format!("Failed to create plugins directory: {}", e))
                })?;
        }

        let plugins = self.discover_plugins().await?;
        let saved_states = self.load_saved_states().await?;

        for plugin in plugins
            .into_iter()
            .filter(|plugin| plugin.enabled && !plugin.loaded)
        {
            let config = saved_states
                .get(&plugin.id)
                .map(|state| state.config.clone())
                .unwrap_or_default();
            if let Err(error) = self.load_plugin(&plugin.id, config).await {
                tracing::error!("Failed to restore plugin {}: {}", plugin.id, error);
            }
        }

        Ok(())
    }

    /// Discover plugins in the plugins directory
    pub async fn discover_plugins(&self) -> Result<Vec<PluginInfo>> {
        tokio::fs::create_dir_all(&self.plugins_dir)
            .await
            .map_err(|e| {
                PostGateError::Plugin(format!("Failed to create plugins directory: {}", e))
            })?;

        let runtime_ids: HashSet<String> = self.runtimes.read().await.keys().cloned().collect();
        let saved_states = self.load_saved_states().await?;
        self.plugins.clear();
        let mut discovered_ids = HashSet::new();

        let mut entries = tokio::fs::read_dir(&self.plugins_dir).await.map_err(|e| {
            PostGateError::Plugin(format!("Failed to read plugins directory: {}", e))
        })?;

        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| PostGateError::Plugin(format!("Failed to read directory entry: {}", e)))?
        {
            let path = entry.path();
            let file_type = entry.file_type().await.map_err(|e| {
                PostGateError::Plugin(format!("Failed to inspect plugin directory entry: {}", e))
            })?;
            if !file_type.is_dir() || file_type.is_symlink() {
                continue;
            }

            // Check for package.json
            let package_json_path = path.join("package.json");
            if !package_json_path.exists() {
                continue;
            }

            // Parse package.json
            match self.parse_plugin_info(&path).await {
                Ok(mut info) => {
                    info.loaded = runtime_ids.contains(&info.id);
                    info.enabled = info.loaded
                        || saved_states
                            .get(&info.id)
                            .map(|state| state.enabled)
                            .unwrap_or(false);
                    tracing::info!("Discovered plugin: {} v{}", info.name, info.version);
                    discovered_ids.insert(info.id.clone());
                    self.plugins.insert(info.id.clone(), info);
                }
                Err(e) => {
                    tracing::warn!("Failed to parse plugin at {:?}: {}", path, e);
                }
            }
        }

        for orphaned_id in runtime_ids.difference(&discovered_ids) {
            if let Err(error) = self.stop_runtime(orphaned_id, None).await {
                tracing::warn!("Failed to stop removed plugin {}: {}", orphaned_id, error);
            }
        }

        Ok(self.get_plugins())
    }

    /// Parse plugin info from package.json
    async fn parse_plugin_info(&self, plugin_path: &Path) -> Result<PluginInfo> {
        let plugin_root = tokio::fs::canonicalize(plugin_path).await.map_err(|e| {
            PostGateError::Plugin(format!("Failed to resolve plugin directory: {}", e))
        })?;
        let package_json_path = plugin_path.join("package.json");
        let package_json_path = tokio::fs::canonicalize(&package_json_path)
            .await
            .map_err(|e| PostGateError::Plugin(format!("Failed to resolve package.json: {}", e)))?;
        if !package_json_path.starts_with(&plugin_root) {
            return Err(PostGateError::Plugin(
                "package.json must stay inside the plugin directory".into(),
            ));
        }
        let content = tokio::fs::read_to_string(&package_json_path)
            .await
            .map_err(|e| PostGateError::Plugin(format!("Failed to read package.json: {}", e)))?;

        let package: PackageJson = serde_json::from_str(&content)
            .map_err(|e| PostGateError::Plugin(format!("Failed to parse package.json: {}", e)))?;

        let (id, _) = plugin_identity(&package.name)?;

        // Determine entry point
        let entry = package
            .main
            .or(package.module)
            .unwrap_or_else(|| "index.js".to_string());

        if !is_safe_relative_path(Path::new(&entry)) {
            return Err(PostGateError::Plugin(format!(
                "Plugin entry point must stay inside the plugin directory: {}",
                entry
            )));
        }

        let entry_path = tokio::fs::canonicalize(plugin_path.join(&entry))
            .await
            .map_err(|e| PostGateError::Plugin(format!("Entry point not found ({entry}): {e}")))?;
        if !entry_path.starts_with(&plugin_root) || !entry_path.is_file() {
            return Err(PostGateError::Plugin(format!(
                "Plugin entry point must be a file inside the plugin directory: {}",
                entry
            )));
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
        let mut plugins: Vec<_> = self
            .plugins
            .iter()
            .map(|entry| entry.value().clone())
            .collect();
        plugins.sort_by(|left, right| left.name.cmp(&right.name));
        plugins
    }

    /// Get a specific plugin
    pub fn get_plugin(&self, id: &str) -> Option<PluginInfo> {
        self.plugins.get(id).map(|r| r.value().clone())
    }

    /// Load a plugin
    pub async fn load_plugin(&self, id: &str, config: HashMap<String, String>) -> Result<()> {
        let info = self
            .plugins
            .get(id)
            .ok_or_else(|| PostGateError::Plugin(format!("Plugin not found: {}", id)))?
            .clone();

        // Ensure we have a database pool
        let db_pool = self
            .db_pool
            .clone()
            .ok_or_else(|| PostGateError::Plugin("Database pool not initialized".into()))?;

        let config = if config.is_empty() {
            PluginStorage::get_plugin_state(&db_pool, id)
                .await?
                .map(|state| state.config)
                .unwrap_or_default()
        } else {
            config
        };

        let mut runtimes = self.runtimes.write().await;
        if runtimes.contains_key(id) {
            drop(runtimes);
            self.update_plugin_flags(id, true, true);
            self.save_state(id, true, config).await?;
            return Ok(());
        }

        let plugin_path = PathBuf::from(&info.path).join(&info.entry);
        let mut runtime = PluginRuntime::new(id.to_string(), plugin_path, self.panels.clone());
        if let Err(error) = runtime
            .start(config.clone(), db_pool, self.app_handle.clone())
            .await
        {
            self.panels.retain(|_, panel| panel.plugin_id != id);
            return Err(error);
        }
        runtimes.insert(id.to_string(), runtime);
        drop(runtimes);

        self.update_plugin_flags(id, true, true);
        self.save_state(id, true, config).await?;

        tracing::info!("Loaded plugin: {}", id);

        Ok(())
    }

    /// Unload a plugin
    pub async fn unload_plugin(&self, id: &str) -> Result<()> {
        self.stop_runtime(id, Some(false)).await
    }

    /// Enable/disable a plugin
    pub async fn set_plugin_enabled(&self, id: &str, enabled: bool) -> Result<()> {
        if enabled {
            self.load_plugin(id, HashMap::new()).await?;
        } else {
            self.unload_plugin(id).await?;
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

        let runtime = runtimes
            .get(plugin_id)
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

        let runtime = runtimes
            .get(plugin_id)
            .ok_or_else(|| PostGateError::Plugin(format!("Plugin not loaded: {}", plugin_id)))?;

        runtime.handle_response(request, response, context).await
    }

    /// Get all registered panels
    pub fn get_panels(&self) -> Vec<PluginPanel> {
        let mut panels: Vec<_> = self
            .panels
            .iter()
            .map(|entry| entry.value().clone())
            .collect();
        panels.sort_by(|left, right| {
            left.title
                .cmp(&right.title)
                .then_with(|| left.plugin_id.cmp(&right.plugin_id))
                .then_with(|| left.id.cmp(&right.id))
        });
        panels
    }

    pub async fn get_saved_state(&self, id: &str) -> Result<Option<PluginState>> {
        let pool = self
            .db_pool
            .as_ref()
            .ok_or_else(|| PostGateError::Plugin("Database pool not initialized".into()))?;
        PluginStorage::get_plugin_state(pool, id).await
    }

    pub async fn restore_saved_state(&self, state: PluginState) -> Result<()> {
        self.save_state(&state.id, state.enabled, state.config.clone())
            .await?;
        if !state.enabled {
            self.update_plugin_flags(&state.id, false, false);
            return Ok(());
        }

        match self.load_plugin(&state.id, state.config).await {
            Ok(()) => Ok(()),
            Err(error) => {
                self.update_plugin_flags(&state.id, false, true);
                Err(error)
            }
        }
    }

    pub async fn remove_plugin_data(&self, id: &str) -> Result<()> {
        let pool = self
            .db_pool
            .as_ref()
            .ok_or_else(|| PostGateError::Plugin("Database pool not initialized".into()))?;
        PluginStorage::clear_plugin_storage(pool, id).await?;
        PluginStorage::delete_plugin_state(pool, id).await?;
        Ok(())
    }

    /// Shutdown all plugins
    pub async fn shutdown(&self) -> Result<()> {
        let runtimes = {
            let mut runtimes = self.runtimes.write().await;
            runtimes.drain().collect::<Vec<_>>()
        };

        for (id, mut runtime) in runtimes {
            if let Err(e) = runtime.stop().await {
                tracing::warn!("Error stopping plugin {}: {}", id, e);
            }
        }

        Ok(())
    }

    fn update_plugin_flags(&self, id: &str, loaded: bool, enabled: bool) {
        if let Some(mut entry) = self.plugins.get_mut(id) {
            entry.loaded = loaded;
            entry.enabled = enabled;
        }
    }

    async fn stop_runtime(&self, id: &str, enabled: Option<bool>) -> Result<()> {
        let runtime = self.runtimes.write().await.remove(id);
        let stop_result = if let Some(mut runtime) = runtime {
            runtime.stop().await
        } else {
            Ok(())
        };

        let enabled_after = enabled.unwrap_or_else(|| {
            self.plugins
                .get(id)
                .map(|entry| entry.enabled)
                .unwrap_or(false)
        });
        self.update_plugin_flags(id, false, enabled_after);
        self.panels.retain(|_, panel| panel.plugin_id != id);

        if let Some(enabled) = enabled {
            let config = self
                .get_saved_state(id)
                .await?
                .map(|state| state.config)
                .unwrap_or_default();
            self.save_state(id, enabled, config).await?;
        }

        tracing::info!("Unloaded plugin: {}", id);
        stop_result
    }

    async fn load_saved_states(&self) -> Result<HashMap<String, PluginState>> {
        match &self.db_pool {
            Some(pool) => PluginStorage::load_plugin_states(pool).await,
            None => Ok(HashMap::new()),
        }
    }

    async fn save_state(
        &self,
        id: &str,
        enabled: bool,
        config: HashMap<String, String>,
    ) -> Result<()> {
        let pool = self
            .db_pool
            .as_ref()
            .ok_or_else(|| PostGateError::Plugin("Database pool not initialized".into()))?;
        PluginStorage::save_plugin_state(
            pool,
            &PluginState {
                id: id.to_string(),
                enabled,
                config,
            },
        )
        .await
    }
}

pub(crate) fn plugin_identity(package_name: &str) -> Result<(String, String)> {
    let (id, directory_name) = if let Some(id) = package_name.strip_prefix("postgate-plugin-") {
        (id, package_name.to_string())
    } else if let Some(id) = package_name.strip_prefix("@postgate/plugin-") {
        (id, format!("@postgate-plugin-{id}"))
    } else {
        return Err(PostGateError::Plugin(
            "Plugin name must start with 'postgate-plugin-' or '@postgate/plugin-'".into(),
        ));
    };

    if id.is_empty()
        || !id.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        })
    {
        return Err(PostGateError::Plugin(format!(
            "Plugin name contains an unsafe identifier: {package_name}"
        )));
    }

    Ok((id.to_string(), directory_name))
}

fn is_safe_relative_path(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_) | Component::CurDir))
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
    use base64::Engine;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use tempfile::tempdir;

    async fn create_test_pool(directory: &Path) -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(
                SqliteConnectOptions::new()
                    .filename(directory.join("plugins.db"))
                    .create_if_missing(true),
            )
            .await
            .unwrap();
        PluginStorage::init_table(&pool).await.unwrap();
        pool
    }

    fn write_plugin(plugins_dir: &Path, source: &str) {
        let plugin_dir = plugins_dir.join("postgate-plugin-test");
        std::fs::create_dir_all(&plugin_dir).unwrap();
        std::fs::write(
            plugin_dir.join("package.json"),
            r#"{
                "name": "postgate-plugin-test",
                "version": "1.0.0",
                "description": "Test plugin",
                "main": "index.js"
            }"#,
        )
        .unwrap();
        std::fs::write(plugin_dir.join("index.js"), source).unwrap();
    }

    #[tokio::test]
    async fn test_plugin_discovery() {
        let temp = tempdir().unwrap();
        let plugins_dir = temp.path().to_path_buf();

        write_plugin(&plugins_dir, "module.exports = {}");

        let manager = PluginManager::new(plugins_dir);
        let plugins = manager.discover_plugins().await.unwrap();

        assert_eq!(plugins.len(), 1);
        assert_eq!(plugins[0].id, "test");
        assert_eq!(plugins[0].name, "postgate-plugin-test");
        assert_eq!(plugins[0].version, "1.0.0");
    }

    #[tokio::test]
    async fn plugin_runtime_preserves_state_panels_and_lifecycle() {
        let temp = tempdir().unwrap();
        let plugins_dir = temp.path().to_path_buf();
        write_plugin(
            &plugins_dir,
            r#"
            module.exports = {
              async onLoad(ctx) {
                const loads = (await ctx.storage.get('loads')) || 0;
                await ctx.storage.set('loads', loads + 1);
                await ctx.storage.set('startupConfig', ctx.config.startup || null);
                ctx.ui.registerPanel({
                  id: 'status',
                  title: 'Fixture Status',
                  content: { type: 'html', html: '<strong>ready</strong>' }
                });
              },
              async onUnload(ctx) {
                await ctx.storage.set('unloaded', true);
              },
              async handleRequest(request, ctx) {
                return {
                  status: 201,
                  headers: { 'content-type': 'application/json' },
                  body: btoa(JSON.stringify({
                    mode: ctx.ruleConfig.mode,
                    matched: ctx.matchedPattern,
                    logger: Boolean(ctx.logger)
                  })),
                  body_base64: true
                };
              },
              async handleResponse(request, response) {
                return { ...response, status: 202 };
              }
            };
            "#,
        );
        let pool = create_test_pool(temp.path()).await;

        let mut manager = PluginManager::new(plugins_dir.clone());
        manager.set_db_pool(pool.clone());
        manager.init().await.unwrap();
        manager
            .load_plugin(
                "test",
                HashMap::from([("startup".to_string(), "restored".to_string())]),
            )
            .await
            .unwrap();

        let panels = manager.get_panels();
        assert_eq!(panels.len(), 1);
        assert_eq!(panels[0].plugin_id, "test");
        assert!(manager.get_plugin("test").unwrap().loaded);
        assert!(manager.discover_plugins().await.unwrap()[0].loaded);

        let request = PluginRequest {
            id: "request-1".to_string(),
            method: "GET".to_string(),
            url: "https://example.test/api".to_string(),
            host: "example.test".to_string(),
            path: "/api".to_string(),
            query: HashMap::new(),
            headers: HashMap::new(),
            body: None,
            body_base64: false,
            timestamp: 1,
        };
        let context = PluginRequestContext {
            rule_config: HashMap::from([(
                "mode".to_string(),
                serde_json::Value::String("fixture".to_string()),
            )]),
            matched_pattern: "https://example.test/api".to_string(),
        };
        let response = manager
            .handle_request("test", request.clone(), context.clone())
            .await
            .unwrap()
            .unwrap();
        let body = base64::engine::general_purpose::STANDARD
            .decode(response.body.unwrap())
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["mode"], "fixture");
        assert_eq!(body["matched"], "https://example.test/api");
        assert_eq!(body["logger"], true);

        let modified = manager
            .handle_response(
                "test",
                request,
                PluginResponse {
                    status: 200,
                    headers: HashMap::new(),
                    body: None,
                    body_base64: false,
                },
                context,
            )
            .await
            .unwrap();
        assert_eq!(modified.status, 202);

        manager.shutdown().await.unwrap();
        let storage = PluginStorage::new(pool.clone(), "test".to_string());
        assert_eq!(
            storage.get("unloaded").await.unwrap(),
            Some(serde_json::json!(true))
        );
        assert_eq!(
            storage.get("startupConfig").await.unwrap(),
            Some(serde_json::json!("restored"))
        );

        let mut restored = PluginManager::new(plugins_dir);
        restored.set_db_pool(pool.clone());
        restored.init().await.unwrap();
        let info = restored.get_plugin("test").unwrap();
        assert!(info.enabled);
        assert!(info.loaded);
        assert_eq!(
            storage.get("loads").await.unwrap(),
            Some(serde_json::json!(2))
        );
        restored.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn example_plugin_executes_end_to_end() {
        let temp = tempdir().unwrap();
        let examples_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../examples");
        let pool = create_test_pool(temp.path()).await;
        let mut manager = PluginManager::new(examples_dir);
        manager.set_db_pool(pool);
        manager.init().await.unwrap();
        manager
            .load_plugin("mock-api", HashMap::new())
            .await
            .unwrap();

        let request = PluginRequest {
            id: "example-request".to_string(),
            method: "GET".to_string(),
            url: "https://example.test/__postgate/mock".to_string(),
            host: "example.test".to_string(),
            path: "/__postgate/mock".to_string(),
            query: HashMap::new(),
            headers: HashMap::new(),
            body: None,
            body_base64: false,
            timestamp: 1,
        };
        let context = PluginRequestContext {
            rule_config: HashMap::from([(
                "mode".to_string(),
                serde_json::Value::String("fixture".to_string()),
            )]),
            matched_pattern: "https://example.test/__postgate/mock".to_string(),
        };
        let response = manager
            .handle_request("mock-api", request.clone(), context.clone())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(response.status, 200);
        let body = base64::engine::general_purpose::STANDARD
            .decode(response.body.unwrap())
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["source"], "postgate-plugin-mock-api");
        assert_eq!(body["mode"], "fixture");

        let response = manager
            .handle_response(
                "mock-api",
                request,
                PluginResponse {
                    status: 204,
                    headers: HashMap::new(),
                    body: None,
                    body_base64: false,
                },
                context,
            )
            .await
            .unwrap();
        assert_eq!(response.status, 204);
        assert_eq!(response.headers["x-postgate-plugin"], "mock-api");
        manager.shutdown().await.unwrap();
    }

    #[test]
    fn rejects_unsafe_plugin_names_and_entries() {
        assert!(plugin_identity("postgate-plugin-../../escape").is_err());
        assert!(plugin_identity("unrelated-package").is_err());
        assert!(!is_safe_relative_path(Path::new("../outside.js")));
        assert!(is_safe_relative_path(Path::new("dist/index.js")));
    }
}
