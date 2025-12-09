//! Deno Core ops for plugin runtime
//!
//! This module defines the custom ops (operations) that plugins can call
//! to interact with the host application. These ops are registered with
//! the Deno Core runtime and exposed to JavaScript code.

use crate::plugin::storage::PluginStorage;
use crate::plugin::types::{LogLevel, PluginPanel, PluginResponse};
use deno_core::{extension, op2, OpState};
use deno_error::JsError;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::rc::Rc;
use tokio::sync::mpsc;

/// Custom error type for plugin operations that implements JsErrorClass
#[derive(Debug, thiserror::Error, JsError)]
#[class(generic)]
pub enum PluginOpError {
    #[error("Storage error: {0}")]
    Storage(String),
    #[error("Plugin error: {0}")]
    Plugin(String),
}

/// Events that can be sent from ops to the runtime
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum PluginEvent {
    Log {
        level: String,
        message: String,
        args: Vec<serde_json::Value>,
    },
    PanelRegistered {
        panel: PluginPanel,
    },
    PanelUnregistered {
        panel_id: String,
    },
    Toast {
        message: String,
        toast_type: Option<String>,
    },
    Response {
        request_id: String,
        response: Option<PluginResponse>,
    },
    ModifiedResponse {
        request_id: String,
        response: PluginResponse,
    },
    Loaded,
    Error {
        message: String,
    },
}

/// Op state that is shared with all ops
pub struct PluginOpState {
    pub plugin_id: String,
    pub storage: PluginStorage,
    pub event_sender: mpsc::UnboundedSender<PluginEvent>,
    pub app_handle: Option<tauri::AppHandle>,
}

// ============================================================================
// Console/Logging Ops
// ============================================================================

#[op2(fast)]
fn op_log(
    state: &mut OpState,
    #[string] level: String,
    #[string] message: String,
) {
    let op_state = state.borrow::<PluginOpState>();
    let plugin_id = &op_state.plugin_id;

    // Log to tracing
    match level.as_str() {
        "debug" => tracing::debug!("[plugin:{}] {}", plugin_id, message),
        "info" => tracing::info!("[plugin:{}] {}", plugin_id, message),
        "warn" => tracing::warn!("[plugin:{}] {}", plugin_id, message),
        "error" => tracing::error!("[plugin:{}] {}", plugin_id, message),
        _ => tracing::info!("[plugin:{}] {}", plugin_id, message),
    }

    // Send event
    let _ = op_state.event_sender.send(PluginEvent::Log {
        level,
        message,
        args: vec![],
    });
}

// ============================================================================
// Storage Ops
// ============================================================================

#[op2(async)]
#[serde]
async fn op_storage_get(
    state: Rc<RefCell<OpState>>,
    #[string] key: String,
) -> Result<Option<serde_json::Value>, PluginOpError> {
    let storage = {
        let state = state.borrow();
        let op_state = state.borrow::<PluginOpState>();
        op_state.storage.clone()
    };

    storage
        .get(&key)
        .await
        .map_err(|e| PluginOpError::Storage(e.to_string()))
}

#[op2(async)]
async fn op_storage_set(
    state: Rc<RefCell<OpState>>,
    #[string] key: String,
    #[serde] value: serde_json::Value,
) -> Result<(), PluginOpError> {
    let storage = {
        let state = state.borrow();
        let op_state = state.borrow::<PluginOpState>();
        op_state.storage.clone()
    };

    storage
        .set(&key, &value)
        .await
        .map_err(|e| PluginOpError::Storage(e.to_string()))
}

#[op2(async)]
async fn op_storage_delete(
    state: Rc<RefCell<OpState>>,
    #[string] key: String,
) -> Result<bool, PluginOpError> {
    let storage = {
        let state = state.borrow();
        let op_state = state.borrow::<PluginOpState>();
        op_state.storage.clone()
    };

    storage
        .delete(&key)
        .await
        .map_err(|e| PluginOpError::Storage(e.to_string()))
}

#[op2(async)]
async fn op_storage_has(
    state: Rc<RefCell<OpState>>,
    #[string] key: String,
) -> Result<bool, PluginOpError> {
    let storage = {
        let state = state.borrow();
        let op_state = state.borrow::<PluginOpState>();
        op_state.storage.clone()
    };

    storage
        .has(&key)
        .await
        .map_err(|e| PluginOpError::Storage(e.to_string()))
}

#[op2(async)]
#[serde]
async fn op_storage_keys(
    state: Rc<RefCell<OpState>>,
) -> Result<Vec<String>, PluginOpError> {
    let storage = {
        let state = state.borrow();
        let op_state = state.borrow::<PluginOpState>();
        op_state.storage.clone()
    };

    storage
        .keys()
        .await
        .map_err(|e| PluginOpError::Storage(e.to_string()))
}

#[op2(async)]
async fn op_storage_clear(
    state: Rc<RefCell<OpState>>,
) -> Result<(), PluginOpError> {
    let storage = {
        let state = state.borrow();
        let op_state = state.borrow::<PluginOpState>();
        op_state.storage.clone()
    };

    storage
        .clear()
        .await
        .map(|_| ())
        .map_err(|e| PluginOpError::Storage(e.to_string()))
}

// ============================================================================
// UI Ops
// ============================================================================

#[op2]
fn op_ui_register_panel(
    state: &mut OpState,
    #[serde] panel: PluginPanel,
) {
    use tauri::Emitter;
    let op_state = state.borrow::<PluginOpState>();

    // Emit to frontend via Tauri
    if let Some(ref app_handle) = op_state.app_handle {
        let _ = app_handle.emit("plugin:panel-registered", &panel);
    }

    // Send event to runtime
    let _ = op_state
        .event_sender
        .send(PluginEvent::PanelRegistered { panel });
}

#[op2(fast)]
fn op_ui_unregister_panel(
    state: &mut OpState,
    #[string] panel_id: String,
) {
    use tauri::Emitter;
    let op_state = state.borrow::<PluginOpState>();

    // Emit to frontend via Tauri
    if let Some(ref app_handle) = op_state.app_handle {
        let _ = app_handle.emit("plugin:panel-unregistered", &panel_id);
    }

    // Send event to runtime
    let _ = op_state.event_sender.send(PluginEvent::PanelUnregistered {
        panel_id,
    });
}

#[op2]
fn op_ui_toast(
    state: &mut OpState,
    #[string] message: String,
    #[string] toast_type: Option<String>,
) {
    use tauri::Emitter;
    let op_state = state.borrow::<PluginOpState>();

    // Emit to frontend via Tauri
    if let Some(ref app_handle) = op_state.app_handle {
        #[derive(Serialize)]
        struct ToastPayload {
            message: String,
            toast_type: Option<String>,
        }
        let _ = app_handle.emit(
            "plugin:toast",
            &ToastPayload {
                message: message.clone(),
                toast_type: toast_type.clone(),
            },
        );
    }

    // Send event to runtime
    let _ = op_state.event_sender.send(PluginEvent::Toast {
        message,
        toast_type,
    });
}

// ============================================================================
// Response Ops (for request/response handling)
// ============================================================================

#[op2]
fn op_resp_send(
    state: &mut OpState,
    #[string] request_id: String,
    #[serde] response: Option<PluginResponse>,
) {
    let op_state = state.borrow::<PluginOpState>();

    let _ = op_state.event_sender.send(PluginEvent::Response {
        request_id,
        response,
    });
}

#[op2]
fn op_resp_send_modified(
    state: &mut OpState,
    #[string] request_id: String,
    #[serde] response: PluginResponse,
) {
    let op_state = state.borrow::<PluginOpState>();

    let _ = op_state.event_sender.send(PluginEvent::ModifiedResponse {
        request_id,
        response,
    });
}

#[op2(fast)]
fn op_lifecycle_loaded(state: &mut OpState) {
    let op_state = state.borrow::<PluginOpState>();
    let _ = op_state.event_sender.send(PluginEvent::Loaded);
}

#[op2(fast)]
fn op_lifecycle_error(
    state: &mut OpState,
    #[string] message: String,
) {
    let op_state = state.borrow::<PluginOpState>();
    let _ = op_state
        .event_sender
        .send(PluginEvent::Error { message });
}

// ============================================================================
// Extension definition using extension! macro
// ============================================================================

extension!(
    postgate_plugin,
    ops = [
        op_log,
        op_storage_get,
        op_storage_set,
        op_storage_delete,
        op_storage_has,
        op_storage_keys,
        op_storage_clear,
        op_ui_register_panel,
        op_ui_unregister_panel,
        op_ui_toast,
        op_resp_send,
        op_resp_send_modified,
        op_lifecycle_loaded,
        op_lifecycle_error,
    ],
);

/// JavaScript runtime code that sets up the plugin environment
pub const RUNTIME_JS: &str = r#"
// PostGate Plugin Runtime
// This code runs before any plugin code and sets up the global environment

// Console implementation
globalThis.console = {
  log: (...args) => {
    const message = args.map(arg => 
      typeof arg === 'object' ? JSON.stringify(arg) : String(arg)
    ).join(' ');
    Deno.core.ops.op_log('info', message);
  },
  info: (...args) => {
    const message = args.map(arg => 
      typeof arg === 'object' ? JSON.stringify(arg) : String(arg)
    ).join(' ');
    Deno.core.ops.op_log('info', message);
  },
  warn: (...args) => {
    const message = args.map(arg => 
      typeof arg === 'object' ? JSON.stringify(arg) : String(arg)
    ).join(' ');
    Deno.core.ops.op_log('warn', message);
  },
  error: (...args) => {
    const message = args.map(arg => 
      typeof arg === 'object' ? JSON.stringify(arg) : String(arg)
    ).join(' ');
    Deno.core.ops.op_log('error', message);
  },
  debug: (...args) => {
    const message = args.map(arg => 
      typeof arg === 'object' ? JSON.stringify(arg) : String(arg)
    ).join(' ');
    Deno.core.ops.op_log('debug', message);
  },
};

// Storage API
const storage = {
  get: async (key) => await Deno.core.opAsync('op_storage_get', key),
  set: async (key, value) => await Deno.core.opAsync('op_storage_set', key, value),
  delete: async (key) => await Deno.core.opAsync('op_storage_delete', key),
  has: async (key) => await Deno.core.opAsync('op_storage_has', key),
  keys: async () => await Deno.core.opAsync('op_storage_keys'),
  clear: async () => await Deno.core.opAsync('op_storage_clear'),
};

// Logger API
const createLogger = (pluginId) => ({
  debug: (message) => Deno.core.ops.op_log('debug', String(message)),
  info: (message) => Deno.core.ops.op_log('info', String(message)),
  warn: (message) => Deno.core.ops.op_log('warn', String(message)),
  error: (message) => Deno.core.ops.op_log('error', String(message)),
});

// UI API
const ui = {
  registerPanel: (panel) => Deno.core.ops.op_ui_register_panel(panel),
  unregisterPanel: (panelId) => Deno.core.ops.op_ui_unregister_panel(panelId),
  toast: (message, type) => Deno.core.ops.op_ui_toast(message, type),
};

// TextEncoder/TextDecoder for body handling
globalThis.TextEncoder = class TextEncoder {
  encode(str) {
    const arr = [];
    for (let i = 0; i < str.length; i++) {
      const code = str.charCodeAt(i);
      if (code < 128) {
        arr.push(code);
      } else if (code < 2048) {
        arr.push(192 | (code >> 6), 128 | (code & 63));
      } else {
        arr.push(224 | (code >> 12), 128 | ((code >> 6) & 63), 128 | (code & 63));
      }
    }
    return new Uint8Array(arr);
  }
};

globalThis.TextDecoder = class TextDecoder {
  decode(bytes) {
    if (!bytes) return '';
    const arr = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes);
    let str = '';
    for (let i = 0; i < arr.length; i++) {
      str += String.fromCharCode(arr[i]);
    }
    return str;
  }
};

// Base64 encoding/decoding helpers
const BASE64_CHARS = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/=';

globalThis.btoa = function(str) {
  let output = '';
  for (let i = 0; i < str.length; i += 3) {
    const a = str.charCodeAt(i);
    const b = str.charCodeAt(i + 1);
    const c = str.charCodeAt(i + 2);
    output += BASE64_CHARS[a >> 2];
    output += BASE64_CHARS[((a & 3) << 4) | (b >> 4)];
    output += BASE64_CHARS[isNaN(b) ? 64 : ((b & 15) << 2) | (c >> 6)];
    output += BASE64_CHARS[isNaN(c) ? 64 : (c & 63)];
  }
  return output;
};

globalThis.atob = function(str) {
  str = str.replace(/=+$/, '');
  let output = '';
  for (let i = 0; i < str.length; i += 4) {
    const a = BASE64_CHARS.indexOf(str[i]);
    const b = BASE64_CHARS.indexOf(str[i + 1]);
    const c = BASE64_CHARS.indexOf(str[i + 2]);
    const d = BASE64_CHARS.indexOf(str[i + 3]);
    output += String.fromCharCode((a << 2) | (b >> 4));
    if (c !== 64 && c !== -1) output += String.fromCharCode(((b & 15) << 4) | (c >> 2));
    if (d !== 64 && d !== -1) output += String.fromCharCode(((c & 3) << 6) | d);
  }
  return output;
};

// setTimeout/setInterval (basic implementation)
const __timers = new Map();
let __timerId = 0;

globalThis.setTimeout = function(fn, delay = 0) {
  const id = ++__timerId;
  // Note: This is a simplified implementation. In real async context,
  // use Deno.core.opAsync for proper delay handling.
  __timers.set(id, { fn, delay, type: 'timeout' });
  // For now, execute immediately (plugins should use await for delays)
  Promise.resolve().then(() => {
    if (__timers.has(id)) {
      __timers.delete(id);
      fn();
    }
  });
  return id;
};

globalThis.clearTimeout = function(id) {
  __timers.delete(id);
};

// PostGate namespace for plugin APIs
globalThis.PostGate = {
  storage,
  ui,
  createLogger,
  
  // Internal APIs used by the runtime
  _internal: {
    sendResponse: (requestId, response) => Deno.core.ops.op_resp_send(requestId, response),
    sendModifiedResponse: (requestId, response) => Deno.core.ops.op_resp_send_modified(requestId, response),
    pluginLoaded: () => Deno.core.ops.op_lifecycle_loaded(),
    pluginError: (message) => Deno.core.ops.op_lifecycle_error(message),
  },
};

// Helper to create plugin context
globalThis.PostGate.createContext = (config) => ({
  storage,
  logger: createLogger('plugin'),
  ui,
  config: config || {},
});
"#;
