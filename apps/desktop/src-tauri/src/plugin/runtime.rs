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
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, RwLock};

/// JavaScript plugin runtime using embedded Deno Core (V8)
pub struct PluginRuntime {
    plugin_id: String,
    plugin_path: PathBuf,
    loaded: bool,
    event_rx: Option<mpsc::UnboundedReceiver<PluginEvent>>,
    pending_requests: Arc<RwLock<HashMap<String, oneshot::Sender<Option<PluginResponse>>>>>,
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
            pending_requests: Arc::new(RwLock::new(HashMap::new())),
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
        let pending_requests = self.pending_requests.clone();
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
    pub fn is_loaded(&self) -> bool {
        self.loaded
    }

    /// Get registered panels
    pub fn panels(&self) -> &[PluginPanel] {
        &self.panels
    }

    /// Handle a request through the plugin
    pub async fn handle_request(
        &self,
        request: PluginRequest,
        context: PluginRequestContext,
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
        context: PluginRequestContext,
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

/// A more sophisticated plugin runtime that keeps the JS runtime alive
/// for handling multiple requests
pub struct PersistentPluginRuntime {
    plugin_id: String,
    plugin_path: PathBuf,
    loaded: bool,
    request_tx: Option<mpsc::Sender<PluginRuntimeMessage>>,
    response_rx: Option<mpsc::Receiver<PluginRuntimeResponse>>,
    panels: Vec<PluginPanel>,
}

/// Messages sent to the plugin runtime thread
#[derive(Debug)]
pub enum PluginRuntimeMessage {
    HandleRequest {
        request: PluginRequest,
        context: PluginRequestContext,
        response_tx: oneshot::Sender<Option<PluginResponse>>,
    },
    HandleResponse {
        request: PluginRequest,
        response: PluginResponse,
        context: PluginRequestContext,
        response_tx: oneshot::Sender<PluginResponse>,
    },
    Stop,
}

/// Responses from the plugin runtime thread
#[derive(Debug)]
pub enum PluginRuntimeResponse {
    Event(PluginEvent),
    Stopped,
}

impl PersistentPluginRuntime {
    /// Create a new persistent plugin runtime
    pub fn new(plugin_id: String, plugin_path: PathBuf) -> Self {
        Self {
            plugin_id,
            plugin_path,
            loaded: false,
            request_tx: None,
            response_rx: None,
            panels: Vec::new(),
        }
    }

    /// Start the persistent runtime in a dedicated thread
    pub async fn start(
        &mut self,
        config: HashMap<String, String>,
        db_pool: SqlitePool,
        app_handle: Option<tauri::AppHandle>,
    ) -> Result<()> {
        let (request_tx, request_rx) = mpsc::channel::<PluginRuntimeMessage>(32);
        let (response_tx, response_rx) = mpsc::channel::<PluginRuntimeResponse>(32);

        self.request_tx = Some(request_tx);
        self.response_rx = Some(response_rx);

        let plugin_id = self.plugin_id.clone();
        let plugin_path = self.plugin_path.clone();

        // Spawn a dedicated thread for the JS runtime
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("Failed to create tokio runtime");

            rt.block_on(async move {
                Self::runtime_loop(
                    plugin_id,
                    plugin_path,
                    config,
                    db_pool,
                    app_handle,
                    request_rx,
                    response_tx,
                )
                .await
            });
        });

        self.loaded = true;
        Ok(())
    }

    /// The main runtime loop running in a dedicated thread
    async fn runtime_loop(
        plugin_id: String,
        plugin_path: PathBuf,
        config: HashMap<String, String>,
        db_pool: SqlitePool,
        app_handle: Option<tauri::AppHandle>,
        mut request_rx: mpsc::Receiver<PluginRuntimeMessage>,
        response_tx: mpsc::Sender<PluginRuntimeResponse>,
    ) {
        // Initialize storage
        let storage = PluginStorage::new(db_pool, plugin_id.clone());

        // Create event channel
        let (event_tx, mut event_rx) = mpsc::unbounded_channel();

        // Create op state
        let op_state = PluginOpState {
            plugin_id: plugin_id.clone(),
            storage,
            event_sender: event_tx,
            app_handle,
        };

        // Read plugin source
        let plugin_source = match tokio::fs::read_to_string(&plugin_path).await {
            Ok(s) => s,
            Err(e) => {
                tracing::error!("Failed to read plugin {}: {}", plugin_id, e);
                return;
            }
        };

        // Create runtime
        let mut runtime = JsRuntime::new(RuntimeOptions {
            extensions: vec![ops::postgate_plugin::init()],
            ..Default::default()
        });

        // Put op state
        runtime.op_state().borrow_mut().put(op_state);

        // Execute runtime init
        if let Err(e) = runtime.execute_script("<runtime>", deno_core::FastString::from_static(ops::RUNTIME_JS)) {
            tracing::error!("Failed to init runtime for {}: {}", plugin_id, e);
            return;
        }

        // Setup plugin context
        let config_json = serde_json::to_string(&config).unwrap_or_default();
        let setup_script = format!(
            r#"globalThis.__pluginConfig = {}; globalThis.__pluginContext = PostGate.createContext(globalThis.__pluginConfig);"#,
            config_json
        );
        if let Err(e) = runtime.execute_script("<setup>", setup_script.clone()) {
            tracing::error!("Failed to setup plugin context for {}: {}", plugin_id, e);
            return;
        }

        // Execute plugin
        let wrapped_plugin = format!(
            r#"
            (async function() {{
                try {{
                    const pluginModule = (function() {{
                        const exports = {{}};
                        const module = {{ exports }};
                        {}
                        return module.exports.default || module.exports;
                    }})();
                    globalThis.__plugin = pluginModule;
                    if (pluginModule && typeof pluginModule.onLoad === 'function') {{
                        await pluginModule.onLoad(globalThis.__pluginContext);
                    }}
                    PostGate._internal.pluginLoaded();
                }} catch (e) {{
                    PostGate._internal.pluginError(e.message || String(e));
                }}
            }})();
            "#,
            plugin_source
        );

        if let Err(e) = runtime.execute_script(format!("<plugin:{}>", plugin_id), wrapped_plugin.clone()) {
            tracing::error!("Failed to execute plugin {}: {}", plugin_id, e);
            return;
        }

        tracing::info!("Plugin {} runtime started", plugin_id);

        // Main loop
        loop {
            tokio::select! {
                // Handle incoming requests
                Some(msg) = request_rx.recv() => {
                    match msg {
                        PluginRuntimeMessage::Stop => {
                            let _ = response_tx.send(PluginRuntimeResponse::Stopped).await;
                            break;
                        }
                        PluginRuntimeMessage::HandleRequest { request, context, response_tx: reply_tx } => {
                            // Execute handleRequest in the runtime
                            let request_json = serde_json::to_string(&request).unwrap_or_default();
                            let context_json = serde_json::to_string(&context).unwrap_or_default();
                            
                            let script = format!(
                                r#"
                                (async function() {{
                                    if (globalThis.__plugin && typeof globalThis.__plugin.handleRequest === 'function') {{
                                        const request = {};
                                        const context = {};
                                        const response = await globalThis.__plugin.handleRequest(request, context);
                                        if (response) {{
                                            PostGate._internal.sendResponse("{}", response);
                                        }} else {{
                                            PostGate._internal.sendResponse("{}", null);
                                        }}
                                    }} else {{
                                        PostGate._internal.sendResponse("{}", null);
                                    }}
                                }})();
                                "#,
                                request_json, context_json, request.id, request.id, request.id
                            );

                            let result = runtime.execute_script("<handleRequest>", script.clone());
                            
                            if result.is_ok() {
                                // Run event loop to process async ops
                                let _ = runtime.run_event_loop(Default::default()).await;
                            } else if let Err(e) = result {
                                tracing::error!("handleRequest failed for {}: {}", plugin_id, e);
                            }
                            
                            // Send None - the actual response would come through events in a more complete implementation
                            let _ = reply_tx.send(None);
                        }
                        PluginRuntimeMessage::HandleResponse { request, response, context, response_tx: reply_tx } => {
                            // Execute handleResponse in the runtime
                            let request_json = serde_json::to_string(&request).unwrap_or_default();
                            let response_json = serde_json::to_string(&response).unwrap_or_default();
                            let context_json = serde_json::to_string(&context).unwrap_or_default();
                            
                            let script = format!(
                                r#"
                                (async function() {{
                                    if (globalThis.__plugin && typeof globalThis.__plugin.handleResponse === 'function') {{
                                        const request = {};
                                        const originalResponse = {};
                                        const context = {};
                                        const modifiedResponse = await globalThis.__plugin.handleResponse(request, originalResponse, context);
                                        PostGate._internal.sendModifiedResponse("{}", modifiedResponse || originalResponse);
                                    }} else {{
                                        PostGate._internal.sendModifiedResponse("{}", {});
                                    }}
                                }})();
                                "#,
                                request_json, response_json, context_json, request.id, request.id, response_json
                            );

                            if let Err(e) = runtime.execute_script("<handleResponse>", script.clone()) {
                                tracing::error!("handleResponse failed for {}: {}", plugin_id, e);
                            }
                            
                            // Run event loop
                            let _ = runtime.run_event_loop(Default::default()).await;
                            
                            // Send back original response for now
                            let _ = reply_tx.send(response);
                        }
                    }
                }
                // Forward events
                Some(event) = event_rx.recv() => {
                    let _ = response_tx.send(PluginRuntimeResponse::Event(event)).await;
                }
            }
        }

        tracing::info!("Plugin {} runtime stopped", plugin_id);
    }

    /// Stop the runtime
    pub async fn stop(&mut self) -> Result<()> {
        if let Some(tx) = &self.request_tx {
            let _ = tx.send(PluginRuntimeMessage::Stop).await;
        }
        self.loaded = false;
        self.request_tx = None;
        self.response_rx = None;
        Ok(())
    }

    /// Check if loaded
    pub fn is_loaded(&self) -> bool {
        self.loaded
    }

    /// Handle a request
    pub async fn handle_request(
        &self,
        request: PluginRequest,
        context: PluginRequestContext,
    ) -> Result<Option<PluginResponse>> {
        if !self.loaded {
            return Err(PostGateError::Plugin("Plugin not loaded".into()));
        }

        let (reply_tx, reply_rx) = oneshot::channel();

        if let Some(tx) = &self.request_tx {
            tx.send(PluginRuntimeMessage::HandleRequest {
                request,
                context,
                response_tx: reply_tx,
            })
            .await
            .map_err(|_| PostGateError::Plugin("Failed to send request to plugin".into()))?;
        }

        match tokio::time::timeout(std::time::Duration::from_secs(30), reply_rx).await {
            Ok(Ok(response)) => Ok(response),
            Ok(Err(_)) => Err(PostGateError::Plugin("Plugin request channel closed".into())),
            Err(_) => Err(PostGateError::Plugin("Plugin request timeout".into())),
        }
    }

    /// Handle response modification
    pub async fn handle_response(
        &self,
        request: PluginRequest,
        response: PluginResponse,
        context: PluginRequestContext,
    ) -> Result<PluginResponse> {
        if !self.loaded {
            return Ok(response);
        }

        let (reply_tx, reply_rx) = oneshot::channel();

        if let Some(tx) = &self.request_tx {
            tx.send(PluginRuntimeMessage::HandleResponse {
                request,
                response: response.clone(),
                context,
                response_tx: reply_tx,
            })
            .await
            .map_err(|_| PostGateError::Plugin("Failed to send response to plugin".into()))?;
        }

        match tokio::time::timeout(std::time::Duration::from_secs(30), reply_rx).await {
            Ok(Ok(modified)) => Ok(modified),
            Ok(Err(_)) => Ok(response),
            Err(_) => Ok(response),
        }
    }

    /// Process events
    pub async fn process_events(&mut self) -> Vec<PluginEvent> {
        let mut events = Vec::new();
        
        if let Some(ref mut rx) = self.response_rx {
            while let Ok(response) = rx.try_recv() {
                if let PluginRuntimeResponse::Event(event) = response {
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
        }
        
        events
    }

    /// Get panels
    pub fn panels(&self) -> &[PluginPanel] {
        &self.panels
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
