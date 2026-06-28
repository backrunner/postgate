use serde::{Deserialize, Serialize};

pub const DEFAULT_MCP_PORT: u16 = 18_999;

pub const SCOPE_PROXY_READ: &str = "proxy:read";
pub const SCOPE_PROXY_CONTROL: &str = "proxy:control";
pub const SCOPE_RULES_READ: &str = "rules:read";
pub const SCOPE_RULES_WRITE: &str = "rules:write";
pub const SCOPE_VALUES_READ: &str = "values:read";
pub const SCOPE_VALUES_WRITE: &str = "values:write";
pub const SCOPE_CAPTURE_READ: &str = "capture:read";
pub const SCOPE_BODY_READ: &str = "body:read";
pub const SCOPE_HISTORY_DELETE: &str = "history:delete";
pub const SCOPE_REPLAY_EXECUTE: &str = "replay:execute";
pub const SCOPE_DEBUG_READ: &str = "debug:read";
pub const SCOPE_MCP_ADMIN: &str = "mcp:admin";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpSettings {
    pub enabled: bool,
    pub port: u16,
    #[serde(default)]
    pub allowed_origins: Vec<String>,
    pub updated_at: i64,
}

impl Default for McpSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            port: DEFAULT_MCP_PORT,
            allowed_origins: vec![],
            updated_at: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpClient {
    pub id: String,
    pub name: String,
    pub scopes: Vec<String>,
    pub revoked: bool,
    pub created_at: i64,
    pub updated_at: i64,
    pub last_seen_at: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct McpClientAuthRecord {
    pub client: McpClient,
    pub token_salt: String,
    pub token_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpAuditEvent {
    pub id: String,
    pub timestamp: i64,
    pub client_id: Option<String>,
    pub operation: String,
    pub target: Option<String>,
    pub allowed: bool,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpStatus {
    pub enabled: bool,
    pub running: bool,
    pub port: u16,
    pub endpoint: String,
    pub client_count: usize,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateMcpClientInput {
    pub name: String,
    #[serde(default)]
    pub scopes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatedMcpClient {
    pub client: McpClient,
    pub token: String,
    pub endpoint: String,
    pub streamable_http_config: serde_json::Value,
    pub stdio_config: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpClientConfig {
    pub endpoint: String,
    pub streamable_http_config: serde_json::Value,
    pub stdio_config: serde_json::Value,
}

pub fn default_client_scopes() -> Vec<String> {
    [
        SCOPE_PROXY_READ,
        SCOPE_PROXY_CONTROL,
        SCOPE_RULES_READ,
        SCOPE_RULES_WRITE,
        SCOPE_VALUES_READ,
        SCOPE_VALUES_WRITE,
        SCOPE_CAPTURE_READ,
        SCOPE_BODY_READ,
        SCOPE_REPLAY_EXECUTE,
        SCOPE_DEBUG_READ,
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

pub fn all_known_scopes() -> Vec<String> {
    [
        SCOPE_PROXY_READ,
        SCOPE_PROXY_CONTROL,
        SCOPE_RULES_READ,
        SCOPE_RULES_WRITE,
        SCOPE_VALUES_READ,
        SCOPE_VALUES_WRITE,
        SCOPE_CAPTURE_READ,
        SCOPE_BODY_READ,
        SCOPE_HISTORY_DELETE,
        SCOPE_REPLAY_EXECUTE,
        SCOPE_DEBUG_READ,
        SCOPE_MCP_ADMIN,
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}
