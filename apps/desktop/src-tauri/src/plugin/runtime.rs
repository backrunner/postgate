//! JavaScript runtime for executing plugins using Deno Core
//!
//! This module provides an embedded JavaScript runtime for executing plugins
//! using deno_core (V8). This eliminates the need for external Node.js installation.

use crate::error::{PostGateError, Result};
use crate::plugin::ops::{self, PluginEvent, PluginOpState};
use crate::plugin::storage::PluginStorage;
use crate::plugin::types::*;
use deno_core::{JsRuntime, RuntimeOptions};
use sqlx::sqlite::SqlitePool;
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::sync::mpsc;

/// JavaScript plugin runtime using embedded Deno Core (V8)
pub struct PluginRuntime {
    plugin_id: String,
    plugin_path: PathBuf,
    loaded: bool,
    event_rx: Option<mpsc::UnboundedReceiver<PluginEvent>>,
    panels: Vec<PluginPanel>,
}

impl PluginRuntime {
    /// Create a new plugin runtime
    pub fn new(plugin_id: String, plugin_path: PathBuf) -> Self {
        Self {
            plugin_id,
            plugin_path,
            loaded: false,
            event_rx: None,
            panels: Vec::new(),
        }
    }

    /// Start the plugin runtime
    pub async fn start(
        &mut self,
        config: HashMap<String, String>,
        db_pool: SqlitePool,
        app_handle: Option<tauri::AppHandle>,
    ) -> Result<()> {
        // Initialize storage for this plugin
        let storage = PluginStorage::new(db_pool.clone(), self.plugin_id.clone());

        // Create event channel
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        self.event_rx = Some(event_rx);

        // Create op state
        let op_state = PluginOpState {
            plugin_id: self.plugin_id.clone(),
            storage,
            event_sender: event_tx,
            app_handle,
        };

        // Read plugin source
        let plugin_source = tokio::fs::read_to_string(&self.plugin_path)
            .await
            .map_err(|e| PostGateError::Plugin(format!("Failed to read plugin: {}", e)))?;

        // Create JS runtime with our extension
        let plugin_id = self.plugin_id.clone();
        let config_json = serde_json::to_string(&config).unwrap_or_default();

        // Run in a blocking task since JsRuntime is not Send
        let result = tokio::task::spawn_blocking(move || {
            Self::run_plugin_sync(op_state, plugin_source, plugin_id, config_json)
        })
        .await
        .map_err(|e| PostGateError::Plugin(format!("Runtime task failed: {}", e)))?;

        result?;
        self.loaded = true;

        Ok(())
    }

    /// Run plugin synchronously (called from blocking task)
    fn run_plugin_sync(
        op_state: PluginOpState,
        plugin_source: String,
        plugin_id: String,
        config_json: String,
    ) -> Result<()> {
        // Create runtime with our extension
        let mut runtime = JsRuntime::new(RuntimeOptions {
            extensions: vec![ops::postgate_plugin::init()],
            ..Default::default()
        });

        // Put op state into the runtime
        runtime.op_state().borrow_mut().put(op_state);

        // Execute runtime initialization code
        runtime
            .execute_script("<runtime>", deno_core::FastString::from_static(ops::RUNTIME_JS))
            .map_err(|e| PostGateError::Plugin(format!("Failed to init runtime: {}", e)))?;

        // Set up plugin context
        let setup_script = format!(
            r#"
            globalThis.__pluginConfig = {};
            globalThis.__pluginContext = PostGate.createContext(globalThis.__pluginConfig);
            "#,
            config_json
        );
        runtime
            .execute_script("<setup>", setup_script.clone())
            .map_err(|e| PostGateError::Plugin(format!("Failed to setup plugin context: {}", e)))?;

        // Execute plugin code
        // We need to handle both CommonJS and ESM syntax
        // Convert ESM to something we can execute
        let processed_source = preprocess_esm(&plugin_source);
        
        let wrapped_plugin = format!(
            r#"
            (async function() {{
                try {{
                    // Execute plugin code in a function scope
                    const __exports = {{}};
                    const __module = {{ exports: __exports }};
                    
                    // Define export helpers for ESM syntax handling
                    const __esm_default = {{ value: null }};
                    
                    // Execute the processed plugin code
                    (function(exports, module) {{
                        {}
                    }})(__exports, __module);
                    
                    // Get the plugin object (prefer ESM default, then module.exports, then exports)
                    const pluginModule = __esm_default.value || __module.exports.default || __module.exports;
                    
                    // Store plugin reference
                    globalThis.__plugin = pluginModule;
                    
                    // Call onLoad if available
                    if (pluginModule && typeof pluginModule.onLoad === 'function') {{
                        await pluginModule.onLoad(globalThis.__pluginContext);
                    }}
                    
                    // Signal loaded
                    PostGate._internal.pluginLoaded();
                }} catch (e) {{
                    PostGate._internal.pluginError(e.message || String(e));
                    throw e;
                }}
            }})();
            "#,
            processed_source
        );

        runtime
            .execute_script(format!("<plugin:{}>", plugin_id), wrapped_plugin.clone())
            .map_err(|e| PostGateError::Plugin(format!("Failed to execute plugin: {}", e)))?;

        Ok(())
    }

    /// Stop the plugin runtime
    pub async fn stop(&mut self) -> Result<()> {
        self.loaded = false;
        self.event_rx = None;
        self.panels.clear();
        Ok(())
    }

    /// Check if runtime is loaded
    #[allow(dead_code)]
    pub fn is_loaded(&self) -> bool {
        self.loaded
    }

    /// Get registered panels
    #[allow(dead_code)]
    pub fn panels(&self) -> &[PluginPanel] {
        &self.panels
    }

    /// Handle a request through the plugin
    #[allow(dead_code)]
    pub async fn handle_request(
        &self,
        request: PluginRequest,
        _context: PluginRequestContext,
    ) -> Result<Option<PluginResponse>> {
        if !self.loaded {
            return Err(PostGateError::Plugin("Plugin not loaded".into()));
        }

        // For now, we need to implement a different approach since JsRuntime
        // cannot be shared across threads. We'll use a message-passing approach.
        
        // TODO: Implement proper request handling with a dedicated runtime thread
        // For now, return None to indicate no response modification
        tracing::debug!(
            "handle_request called for plugin {} with request {}",
            self.plugin_id,
            request.id
        );

        Ok(None)
    }

    /// Handle response modification through the plugin
    pub async fn handle_response(
        &self,
        request: PluginRequest,
        response: PluginResponse,
        _context: PluginRequestContext,
    ) -> Result<PluginResponse> {
        if !self.loaded {
            return Ok(response);
        }

        // TODO: Implement proper response handling with a dedicated runtime thread
        // For now, return the response unmodified
        tracing::debug!(
            "handle_response called for plugin {} with request {}",
            self.plugin_id,
            request.id
        );

        Ok(response)
    }

    /// Process events from the plugin
    #[allow(dead_code)]
    pub async fn process_events(&mut self) -> Vec<PluginEvent> {
        let mut events = Vec::new();
        
        if let Some(ref mut rx) = self.event_rx {
            while let Ok(event) = rx.try_recv() {
                // Handle panel registration internally
                match &event {
                    PluginEvent::PanelRegistered { panel } => {
                        self.panels.push(panel.clone());
                    }
                    PluginEvent::PanelUnregistered { panel_id } => {
                        self.panels.retain(|p| p.id != *panel_id);
                    }
                    _ => {}
                }
                events.push(event);
            }
        }
        
        events
    }
}

/// Preprocess ESM source code to make it compatible with our wrapper
/// Converts `export default` to an assignment to `__esm_default.value`
fn preprocess_esm(source: &str) -> String {
    // Replace `export default {` with `__esm_default.value = {`
    // This is a simple regex-based transformation
    let mut result = source.to_string();
    
    // Handle `export default {` (object literal)
    result = regex::Regex::new(r"export\s+default\s+\{")
        .unwrap()
        .replace_all(&result, "__esm_default.value = {")
        .to_string();
    
    // Handle `export default async function` 
    result = regex::Regex::new(r"export\s+default\s+async\s+function\s*(\w*)")
        .unwrap()
        .replace_all(&result, "__esm_default.value = async function $1")
        .to_string();
    
    // Handle `export default function`
    result = regex::Regex::new(r"export\s+default\s+function\s*(\w*)")
        .unwrap()
        .replace_all(&result, "__esm_default.value = function $1")
        .to_string();
    
    // Handle `export default class`
    result = regex::Regex::new(r"export\s+default\s+class\s+(\w+)")
        .unwrap()
        .replace_all(&result, "__esm_default.value = class $1")
        .to_string();
    
    // Handle `export default <expression>` (must be last, most generic)
    // This catches things like `export default someVariable`
    result = regex::Regex::new(r"export\s+default\s+([^;\n{]+)")
        .unwrap()
        .replace_all(&result, "__esm_default.value = $1")
        .to_string();
    
    // Remove remaining ESM imports (plugins should use PostGate globals)
    // This handles `import ... from '...'` statements
    result = regex::Regex::new(r#"import\s+.*?\s+from\s+['"].*?['"];?\n?"#)
        .unwrap()
        .replace_all(&result, "// [import removed - use PostGate globals]\n")
        .to_string();
    
    // Also handle `import '...'` (side-effect imports)
    result = regex::Regex::new(r#"import\s+['"].*?['"];?\n?"#)
        .unwrap()
        .replace_all(&result, "// [import removed]\n")
        .to_string();
    
    result
}
