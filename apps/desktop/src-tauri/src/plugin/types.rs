//! Plugin types and data structures

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Information about a discovered plugin
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInfo {
    /// Plugin ID (derived from package name)
    pub id: String,
    /// Plugin name
    pub name: String,
    /// Plugin version (semver)
    pub version: String,
    /// Human-readable description
    pub description: Option<String>,
    /// Plugin author
    pub author: Option<String>,
    /// Path to plugin directory
    pub path: String,
    /// Entry point file
    pub entry: String,
    /// Whether the plugin is currently enabled
    pub enabled: bool,
    /// Whether the plugin is currently loaded
    pub loaded: bool,
}

/// Plugin state stored in database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginState {
    pub id: String,
    pub enabled: bool,
    pub config: HashMap<String, serde_json::Value>,
}

/// Request data passed to plugin
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginRequest {
    /// Unique request ID
    pub id: String,
    /// HTTP method
    pub method: String,
    /// Full URL
    pub url: String,
    /// Hostname
    pub host: String,
    /// Path (without query string)
    pub path: String,
    /// Query string parameters
    pub query: HashMap<String, String>,
    /// Request headers (lowercase keys)
    pub headers: HashMap<String, String>,
    /// Request body (base64 encoded if binary)
    pub body: Option<String>,
    /// Whether body is base64 encoded
    pub body_base64: bool,
    /// Timestamp when request was received
    pub timestamp: i64,
}

/// Response data returned from plugin
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginResponse {
    /// HTTP status code
    pub status: u16,
    /// Response headers
    pub headers: HashMap<String, String>,
    /// Response body (base64 encoded if binary)
    pub body: Option<String>,
    /// Whether body is base64 encoded
    pub body_base64: bool,
}

/// Context for plugin request handling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginRequestContext {
    /// Configuration from the matched rule
    pub rule_config: HashMap<String, serde_json::Value>,
    /// Matched rule pattern
    pub matched_pattern: String,
}

/// Log entry from plugin
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginLogEntry {
    pub plugin_id: String,
    pub level: LogLevel,
    pub message: String,
    pub timestamp: i64,
    pub args: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

/// UI panel registered by plugin
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginPanel {
    /// Unique panel ID
    pub id: String,
    /// Plugin that registered this panel
    pub plugin_id: String,
    /// Panel title
    pub title: String,
    /// Icon name (from lucide-react)
    pub icon: Option<String>,
    /// HTML content or iframe URL
    pub content: PanelContent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum PanelContent {
    #[serde(rename = "html")]
    Html { html: String },
    #[serde(rename = "iframe")]
    Iframe { url: String },
}

/// Storage operation for plugin
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op")]
pub enum StorageOp {
    #[serde(rename = "get")]
    Get { key: String },
    #[serde(rename = "set")]
    Set {
        key: String,
        value: serde_json::Value,
    },
    #[serde(rename = "delete")]
    Delete { key: String },
    #[serde(rename = "has")]
    Has { key: String },
    #[serde(rename = "keys")]
    Keys,
    #[serde(rename = "clear")]
    Clear,
}

/// Result of a storage operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageResult {
    pub success: bool,
    pub value: Option<serde_json::Value>,
    pub error: Option<String>,
}

/// Message from plugin to host
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum PluginMessage {
    /// Log message
    #[serde(rename = "log")]
    Log {
        level: LogLevel,
        message: String,
        args: Vec<serde_json::Value>,
    },
    /// Storage operation
    #[serde(rename = "storage")]
    Storage { id: u64, op: StorageOp },
    /// Register UI panel
    #[serde(rename = "registerPanel")]
    RegisterPanel { panel: PluginPanel },
    /// Unregister UI panel
    #[serde(rename = "unregisterPanel")]
    UnregisterPanel { panel_id: String },
    /// Show toast notification
    #[serde(rename = "toast")]
    Toast {
        message: String,
        #[serde(rename = "toastType")]
        toast_type: Option<String>,
    },
    /// Plugin loaded successfully
    #[serde(rename = "loaded")]
    Loaded,
    /// Request response
    #[serde(rename = "response")]
    Response {
        request_id: String,
        response: Option<PluginResponse>,
    },
    /// Modified response
    #[serde(rename = "modifiedResponse")]
    ModifiedResponse {
        request_id: String,
        response: PluginResponse,
    },
    /// Error
    #[serde(rename = "error")]
    Error { message: String },
}

/// Message from host to plugin
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum HostMessage {
    /// Initialize plugin
    #[serde(rename = "init")]
    Init { config: HashMap<String, String> },
    /// Handle incoming request
    #[serde(rename = "handleRequest")]
    HandleRequest {
        request: PluginRequest,
        context: PluginRequestContext,
    },
    /// Handle/modify response
    #[serde(rename = "handleResponse")]
    HandleResponse {
        request: PluginRequest,
        response: PluginResponse,
        context: PluginRequestContext,
    },
    /// Storage operation result
    #[serde(rename = "storageResult")]
    StorageResult { id: u64, result: StorageResult },
    /// Unload plugin
    #[serde(rename = "unload")]
    Unload,
}
