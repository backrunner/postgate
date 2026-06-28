use crate::api::{
    CaptureBodyEncoding, CaptureBodyInput, CaptureBodySide, CaptureBodySource, CaptureSearchInput,
};
use rmcp::schemars;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StartProxyInput {
    pub port: Option<u16>,
    #[serde(default = "default_true")]
    pub enable_http2: bool,
    #[serde(default)]
    pub enable_quic: bool,
    pub quic_port: Option<u16>,
    pub max_connections_per_host: Option<usize>,
    pub connection_idle_timeout: Option<u64>,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SetPersistenceInput {
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct IdInput {
    pub id: String,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpsertRuleGroupInput {
    pub id: Option<String>,
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub priority: i32,
    pub raw_content: String,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AppendRuleLinesInput {
    pub id: String,
    pub lines: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ToggleRuleGroupInput {
    pub id: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ValidateRulesInput {
    pub content: String,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SaveValueInput {
    pub name: String,
    pub content: String,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeleteValueInput {
    pub name: String,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RenameValueInput {
    pub old_name: String,
    pub new_name: String,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CaptureSearchToolInput {
    pub search: Option<String>,
    #[serde(default)]
    pub methods: Vec<String>,
    #[serde(default)]
    pub hosts: Vec<String>,
    #[serde(default)]
    pub protocols: Vec<String>,
    #[serde(default)]
    pub status_codes: Vec<u16>,
    #[serde(default)]
    pub content_types: Vec<String>,
    pub has_rules: Option<bool>,
    pub since: Option<i64>,
    pub until: Option<i64>,
    pub cursor: Option<String>,
    pub limit: Option<usize>,
    #[serde(default = "default_true")]
    pub redact: bool,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CaptureGetInput {
    pub id: String,
    #[serde(default = "default_true")]
    pub redact: bool,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CaptureBodyToolInput {
    pub id: String,
    pub side: String,
    pub source: Option<String>,
    pub encoding: Option<String>,
    pub max_bytes: Option<usize>,
    #[serde(default = "default_true")]
    pub redact: bool,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReplayExecuteInput {
    pub request: serde_json::Value,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ImportCaptureReplayInput {
    pub id: String,
    pub collection_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConsoleLogsInput {
    pub session_id: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionInput {
    pub session_id: String,
}

impl From<CaptureSearchToolInput> for CaptureSearchInput {
    fn from(input: CaptureSearchToolInput) -> Self {
        Self {
            search: input.search,
            methods: input.methods,
            hosts: input.hosts,
            protocols: input.protocols,
            status_codes: input.status_codes,
            content_types: input.content_types,
            has_rules: input.has_rules,
            since: input.since,
            until: input.until,
            cursor: input.cursor,
            limit: input.limit,
            redact: input.redact,
        }
    }
}

impl TryFrom<CaptureBodyToolInput> for CaptureBodyInput {
    type Error = String;

    fn try_from(input: CaptureBodyToolInput) -> Result<Self, Self::Error> {
        Ok(Self {
            id: input.id,
            side: match input.side.as_str() {
                "request" => CaptureBodySide::Request,
                "response" => CaptureBodySide::Response,
                other => return Err(format!("Invalid body side '{}'", other)),
            },
            source: match input.source.as_deref().unwrap_or("auto") {
                "auto" => CaptureBodySource::Auto,
                "memory" => CaptureBodySource::Memory,
                "persisted" => CaptureBodySource::Persisted,
                other => return Err(format!("Invalid body source '{}'", other)),
            },
            encoding: match input.encoding.as_deref().unwrap_or("auto") {
                "auto" => CaptureBodyEncoding::Auto,
                "utf8" => CaptureBodyEncoding::Utf8,
                "base64" => CaptureBodyEncoding::Base64,
                other => return Err(format!("Invalid body encoding '{}'", other)),
            },
            max_bytes: input.max_bytes,
            redact: input.redact,
        })
    }
}

fn default_true() -> bool {
    true
}
