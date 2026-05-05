// Debug types for frontend debugging

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A debug session represents a connected browser page
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebugSession {
    pub id: String,
    pub url: String,
    pub title: Option<String>,
    pub user_agent: Option<String>,
    pub connected_at: i64,
    pub last_activity: i64,
    pub is_connected: bool,
    pub cdp_enabled: bool,
    /// WebSocket debug URL for Chrome DevTools
    #[serde(rename = "webSocketDebuggerUrl")]
    pub web_socket_debugger_url: String,
}

/// Console log levels
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ConsoleLevel {
    Log,
    Info,
    Warn,
    Error,
    Debug,
    Trace,
    Clear,
}

impl From<&str> for ConsoleLevel {
    fn from(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "info" => ConsoleLevel::Info,
            "warn" | "warning" => ConsoleLevel::Warn,
            "error" => ConsoleLevel::Error,
            "debug" => ConsoleLevel::Debug,
            "trace" => ConsoleLevel::Trace,
            "clear" => ConsoleLevel::Clear,
            _ => ConsoleLevel::Log,
        }
    }
}

/// A captured console log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsoleLog {
    pub id: String,
    pub session_id: String,
    pub level: ConsoleLevel,
    pub args: Vec<ConsoleArg>,
    pub timestamp: i64,
    pub stack_trace: Option<String>,
    pub source_url: Option<String>,
    pub line_number: Option<u32>,
    pub column_number: Option<u32>,
}

/// A console argument (serialized value)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum ConsoleArg {
    #[serde(rename = "string")]
    String(String),
    #[serde(rename = "number")]
    Number(f64),
    #[serde(rename = "boolean")]
    Boolean(bool),
    #[serde(rename = "null")]
    Null,
    #[serde(rename = "undefined")]
    Undefined,
    #[serde(rename = "object")]
    Object(serde_json::Value),
    #[serde(rename = "array")]
    Array(Vec<ConsoleArg>),
    #[serde(rename = "function")]
    Function(String),
    #[serde(rename = "symbol")]
    Symbol(String),
    #[serde(rename = "error")]
    Error {
        name: String,
        message: String,
        stack: Option<String>,
    },
    #[serde(rename = "element")]
    Element {
        tag: String,
        id: Option<String>,
        classes: Vec<String>,
    },
    #[serde(rename = "circular")]
    Circular,
    #[serde(rename = "truncated")]
    Truncated(String),
}

/// Network request captured from the page
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageNetworkRequest {
    pub id: String,
    pub session_id: String,
    pub method: String,
    pub url: String,
    pub request_headers: HashMap<String, String>,
    pub request_body: Option<String>,
    pub status: Option<u16>,
    pub response_headers: Option<HashMap<String, String>>,
    pub response_body: Option<String>,
    pub duration_ms: Option<u64>,
    pub timestamp: i64,
    pub initiator: Option<String>,
}

/// Error captured from the page
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageError {
    pub id: String,
    pub session_id: String,
    pub error_type: ErrorType,
    pub message: String,
    pub stack: Option<String>,
    pub source_url: Option<String>,
    pub line_number: Option<u32>,
    pub column_number: Option<u32>,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ErrorType {
    Runtime,
    Syntax,
    Reference,
    Type,
    Range,
    Uri,
    Network,
    Promise,
    Unknown,
}

/// Message from inject client to debug server
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ClientMessage {
    #[serde(rename = "hello")]
    Hello {
        url: String,
        title: Option<String>,
        user_agent: Option<String>,
        cdp_enabled: Option<bool>,
    },
    #[serde(rename = "console")]
    Console {
        level: String,
        args: Vec<serde_json::Value>,
        stack: Option<String>,
        source_url: Option<String>,
        line: Option<u32>,
        column: Option<u32>,
    },
    #[serde(rename = "error")]
    Error {
        error_type: String,
        message: String,
        stack: Option<String>,
        source_url: Option<String>,
        line: Option<u32>,
        column: Option<u32>,
    },
    #[serde(rename = "network")]
    Network {
        id: String,
        phase: String, // "start" | "end"
        method: Option<String>,
        url: Option<String>,
        request_headers: Option<HashMap<String, String>>,
        request_body: Option<String>,
        status: Option<u16>,
        response_headers: Option<HashMap<String, String>>,
        duration_ms: Option<u64>,
        initiator: Option<String>,
    },
    #[serde(rename = "cdp")]
    Cdp { message: serde_json::Value },
    #[serde(rename = "ping")]
    Ping,
}

/// Message from debug server to inject client
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ServerMessage {
    #[serde(rename = "welcome")]
    Welcome { session_id: String },
    #[serde(rename = "eval")]
    Eval { id: String, code: String },
    #[serde(rename = "cdp")]
    Cdp { message: serde_json::Value },
    #[serde(rename = "pong")]
    Pong,
}

/// Debug configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebugConfig {
    pub enabled: bool,
    pub port: u16,
    pub capture_console: bool,
    pub capture_network: bool,
    pub capture_errors: bool,
    pub inject_pattern: Option<String>, // URL pattern to inject into
}

impl Default for DebugConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            port: 9229,
            capture_console: true,
            capture_network: true,
            capture_errors: true,
            inject_pattern: None,
        }
    }
}

/// Debug server status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebugStatus {
    pub is_running: bool,
    pub port: u16,
    pub session_count: usize,
    pub total_logs: usize,
}
