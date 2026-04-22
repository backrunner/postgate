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
use tokio::sync::{mpsc, oneshot};

/// Message sent to the plugin thread to execute JS calls
#[derive(Debug)]
enum JsCall {
    HandleRequest {
        request: PluginRequest,
        context: PluginRequestContext,
        respond_to: oneshot::Sender<Result<Option<PluginResponse>>>,
    },
    HandleResponse {
        request: PluginRequest,
        response: PluginResponse,
        context: PluginRequestContext,
        respond_to: oneshot::Sender<Result<PluginResponse>>,
    },
    Shutdown,
}

/// JavaScript plugin runtime using embedded Deno Core (V8)
pub struct PluginRuntime {
    plugin_id: String,
    plugin_path: PathBuf,
    loaded: bool,
    event_rx: Option<mpsc::UnboundedReceiver<PluginEvent>>,
    panels: Vec<PluginPanel>,
    call_tx: Option<mpsc::UnboundedSender<JsCall>>,
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
            call_tx: None,
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

        let plugin_id = self.plugin_id.clone();
        let config_json = serde_json::to_string(&config).unwrap_or_default();

        // Channels to communicate with the dedicated plugin thread
        let (init_tx, init_rx) = oneshot::channel();
        let (call_tx, call_rx) = mpsc::unbounded_channel();

        // Spawn a dedicated OS thread to hold the JsRuntime (V8 isolate is !Send)
        std::thread::spawn(move || {
            Self::run_plugin_thread(op_state, plugin_source, plugin_id, config_json, call_rx, init_tx);
        });

        // Wait for the thread to finish initialization
        init_rx
            .await
            .map_err(|e| PostGateError::Plugin(format!("Plugin init channel closed: {}", e)))??;

        self.call_tx = Some(call_tx);
        self.loaded = true;

        Ok(())
    }

    /// Main loop running inside the dedicated plugin thread.
    /// Keeps the JsRuntime alive for the lifetime of the thread so that
    /// `globalThis.__plugin` persists across handleRequest / handleResponse calls.
    fn run_plugin_thread(
        op_state: PluginOpState,
        plugin_source: String,
        plugin_id: String,
        config_json: String,
        mut call_rx: mpsc::UnboundedReceiver<JsCall>,
        init_tx: oneshot::Sender<Result<()>>,
    ) {
        let rt = tokio::runtime::Runtime::new().unwrap();

        let _ = rt.block_on(async {
            let mut runtime = JsRuntime::new(RuntimeOptions {
                extensions: vec![ops::postgate_plugin::init()],
                ..Default::default()
            });

            // Put op state into the runtime
            runtime.op_state().borrow_mut().put(op_state);

            // Execute runtime initialization code
            if let Err(e) = runtime.execute_script("<runtime>", ops::RUNTIME_JS) {
                return Err(PostGateError::Plugin(format!("Failed to init runtime: {}", e)));
            }

            // Set up plugin context
            let setup_script = format!(
                r#"globalThis.__pluginConfig = {}; globalThis.__pluginContext = PostGate.createContext(globalThis.__pluginConfig);"#,
                config_json
            );
            if let Err(e) = runtime.execute_script("<setup>", setup_script) {
                return Err(PostGateError::Plugin(format!("Failed to setup plugin context: {}", e)));
            }

            // Execute plugin code
            let processed_source = preprocess_esm(&plugin_source);
            let wrapped_plugin = format!(
                r#"
                (async function() {{
                    try {{
                        const __exports = {{}};
                        const __module = {{ exports: __exports }};
                        const __esm_default = {{ value: null }};
                        (function(exports, module) {{
                            {}
                        }})(__exports, __module);
                        const pluginModule = __esm_default.value || __module.exports.default || __module.exports;
                        globalThis.__plugin = pluginModule;
                        if (pluginModule && typeof pluginModule.onLoad === 'function') {{
                            await pluginModule.onLoad(globalThis.__pluginContext);
                        }}
                        PostGate._internal.pluginLoaded();
                    }} catch (e) {{
                        PostGate._internal.pluginError(e.message || String(e));
                        throw e;
                    }}
                }})();
                "#,
                processed_source
            );

            let result = match runtime.execute_script(format!("<plugin:{}>", plugin_id), wrapped_plugin) {
                Ok(r) => r,
                Err(e) => return Err(PostGateError::Plugin(format!("Failed to execute plugin: {}", e))),
            };

            // Wait for the async onLoad (if any) to complete
            if let Err(e) = runtime.resolve(result).await {
                return Err(PostGateError::Plugin(format!("Plugin onLoad failed: {}", e)));
            }

            // -----------------------------------------------------------------
            // Plugin initialised successfully — enter the request/response loop.
            // The runtime stays alive inside this async block.
            // -----------------------------------------------------------------
            if init_tx.send(Ok(())).is_err() {
                return Ok(()); // caller dropped the receiver
            }

            while let Some(call) = call_rx.recv().await {
                match call {
                    JsCall::HandleRequest { request, context, respond_to } => {
                        let result = execute_handle_request(&mut runtime, request, context).await;
                        let _ = respond_to.send(result);
                    }
                    JsCall::HandleResponse { request, response, context, respond_to } => {
                        let result = execute_handle_response(&mut runtime, request, response, context).await;
                        let _ = respond_to.send(result);
                    }
                    JsCall::Shutdown => break,
                }
            }

            Ok::<(), PostGateError>(())
        });
    }

    /// Stop the plugin runtime
    pub async fn stop(&mut self) -> Result<()> {
        if let Some(ref tx) = self.call_tx {
            let _ = tx.send(JsCall::Shutdown);
        }
        self.call_tx = None;
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

        let tx = self.call_tx.as_ref()
            .ok_or_else(|| PostGateError::Plugin("Plugin thread not running".into()))?;

        let (respond_to, respond_rx) = oneshot::channel();
        tx.send(JsCall::HandleRequest { request, context, respond_to })
            .map_err(|e| PostGateError::Plugin(format!("Send failed: {}", e)))?;

        respond_rx.await
            .map_err(|e| PostGateError::Plugin(format!("Receive failed: {}", e)))?
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

        let tx = self.call_tx.as_ref()
            .ok_or_else(|| PostGateError::Plugin("Plugin thread not running".into()))?;

        let (respond_to, respond_rx) = oneshot::channel();
        tx.send(JsCall::HandleResponse { request, response, context, respond_to })
            .map_err(|e| PostGateError::Plugin(format!("Send failed: {}", e)))?;

        respond_rx.await
            .map_err(|e| PostGateError::Plugin(format!("Receive failed: {}", e)))?
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

/// Execute a plugin handleRequest call inside the dedicated thread.
async fn execute_handle_request(
    runtime: &mut JsRuntime,
    request: PluginRequest,
    context: PluginRequestContext,
) -> Result<Option<PluginResponse>> {
    let request_json = serde_json::to_string(&request)
        .map_err(|e| PostGateError::Plugin(format!("Serialize request failed: {}", e)))?;
    let context_json = serde_json::to_string(&context)
        .map_err(|e| PostGateError::Plugin(format!("Serialize context failed: {}", e)))?;

    let script = format!(
        r#"(async () => {{
            const plugin = globalThis.__plugin;
            if (!plugin || typeof plugin.handleRequest !== 'function') return null;
            const req = {};
            const ctx = {};
            const result = await plugin.handleRequest(req, ctx);
            return result ? JSON.stringify(result) : null;
        }})()"#,
        request_json, context_json
    );

    let result = runtime
        .execute_script("<handleRequest>", script)
        .map_err(|e| PostGateError::Plugin(format!("handleRequest script error: {}", e)))?;

    let resolved = runtime
        .resolve(result)
        .await
        .map_err(|e| PostGateError::Plugin(format!("handleRequest promise error: {}", e)))?;

    let scope_storage = std::pin::pin!(deno_core::v8::HandleScope::new(runtime.v8_isolate()));
    let pin_scope = &scope_storage.init();
    let local = deno_core::v8::Local::new(pin_scope, resolved);

    if local.is_null_or_undefined() {
        return Ok(None);
    }

    let str_local = deno_core::v8::Local::<deno_core::v8::String>::try_from(local)
        .map_err(|_| PostGateError::Plugin("Plugin handleRequest did not return a JSON string".into()))?;

    let rust_str = str_local.to_rust_string_lossy(&**pin_scope);
    let response: Option<PluginResponse> = serde_json::from_str(&rust_str)
        .map_err(|e| PostGateError::Plugin(format!("handleRequest JSON parse error: {}", e)))?;

    Ok(response)
}

/// Execute a plugin handleResponse call inside the dedicated thread.
async fn execute_handle_response(
    runtime: &mut JsRuntime,
    request: PluginRequest,
    response: PluginResponse,
    context: PluginRequestContext,
) -> Result<PluginResponse> {
    let request_json = serde_json::to_string(&request)
        .map_err(|e| PostGateError::Plugin(format!("Serialize request failed: {}", e)))?;
    let response_json = serde_json::to_string(&response)
        .map_err(|e| PostGateError::Plugin(format!("Serialize response failed: {}", e)))?;
    let context_json = serde_json::to_string(&context)
        .map_err(|e| PostGateError::Plugin(format!("Serialize context failed: {}", e)))?;

    let script = format!(
        r#"(async () => {{
            const plugin = globalThis.__plugin;
            if (!plugin || typeof plugin.handleResponse !== 'function') return null;
            const req = {};
            const res = {};
            const ctx = {};
            const result = await plugin.handleResponse(req, res, ctx);
            return result ? JSON.stringify(result) : null;
        }})()"#,
        request_json, response_json, context_json
    );

    let result = runtime
        .execute_script("<handleResponse>", script)
        .map_err(|e| PostGateError::Plugin(format!("handleResponse script error: {}", e)))?;

    let resolved = runtime
        .resolve(result)
        .await
        .map_err(|e| PostGateError::Plugin(format!("handleResponse promise error: {}", e)))?;

    let scope_storage = std::pin::pin!(deno_core::v8::HandleScope::new(runtime.v8_isolate()));
    let pin_scope = &scope_storage.init();
    let local = deno_core::v8::Local::new(pin_scope, resolved);

    if local.is_null_or_undefined() {
        return Ok(response);
    }

    let str_local = deno_core::v8::Local::<deno_core::v8::String>::try_from(local)
        .map_err(|_| PostGateError::Plugin("Plugin handleResponse did not return a JSON string".into()))?;

    let rust_str = str_local.to_rust_string_lossy(&**pin_scope);
    let modified: Option<PluginResponse> = serde_json::from_str(&rust_str)
        .map_err(|e| PostGateError::Plugin(format!("handleResponse JSON parse error: {}", e)))?;

    Ok(modified.unwrap_or(response))
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
