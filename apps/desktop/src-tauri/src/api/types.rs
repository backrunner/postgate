use crate::proxy::ProxyStatus;
use crate::rules::Rule;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyStatusView {
    pub status: ProxyStatus,
    pub port: u16,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkAddress {
    pub ip: String,
    pub name: String,
    pub is_default: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RuleParseIssue {
    pub line: usize,
    pub message: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RuleParseResult {
    pub success: bool,
    pub rules: Vec<Rule>,
    pub errors: Vec<RuleParseIssue>,
    pub warnings: Vec<RuleParseIssue>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CaptureSearchInput {
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureSearchResult {
    pub items: Vec<crate::state::CapturedRequestData>,
    pub total: usize,
    pub cursor: Option<String>,
    pub has_more: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CaptureBodySide {
    Request,
    Response,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CaptureBodySource {
    Auto,
    Memory,
    Persisted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CaptureBodyEncoding {
    Auto,
    Utf8,
    Base64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureBodyInput {
    pub id: String,
    pub side: CaptureBodySide,
    #[serde(default = "default_body_source")]
    pub source: CaptureBodySource,
    #[serde(default = "default_body_encoding")]
    pub encoding: CaptureBodyEncoding,
    pub max_bytes: Option<usize>,
    #[serde(default = "default_true")]
    pub redact: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureBodyResult {
    pub id: String,
    pub side: String,
    pub source: String,
    pub content_type: Option<String>,
    pub size: usize,
    pub captured_bytes: usize,
    pub truncated: bool,
    pub encoding: String,
    pub content: String,
    pub sha256: String,
    pub redacted: bool,
}

fn default_true() -> bool {
    true
}

fn default_body_source() -> CaptureBodySource {
    CaptureBodySource::Auto
}

fn default_body_encoding() -> CaptureBodyEncoding {
    CaptureBodyEncoding::Auto
}
