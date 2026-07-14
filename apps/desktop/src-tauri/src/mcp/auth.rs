use crate::error::{PostGateError, Result};
use crate::mcp::{
    all_known_scopes, McpAuditEvent, McpClient, McpClientAuthRecord, SCOPE_BODY_READ,
    SCOPE_CAPTURE_READ, SCOPE_DEBUG_READ, SCOPE_HISTORY_DELETE, SCOPE_MCP_ADMIN,
    SCOPE_PROXY_CONTROL, SCOPE_PROXY_READ, SCOPE_REPLAY_EXECUTE, SCOPE_RULES_READ,
    SCOPE_RULES_WRITE, SCOPE_VALUES_READ, SCOPE_VALUES_WRITE,
};
use crate::state::AppState;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::sync::Arc;
use uuid::Uuid;

pub fn generate_token() -> String {
    format!(
        "pgmcp_{}{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    )
}

pub fn generate_salt() -> String {
    Uuid::new_v4().simple().to_string()
}

pub fn hash_token(salt: &str, token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(salt.as_bytes());
    hasher.update(b":");
    hasher.update(token.as_bytes());
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{:02x}", byte))
        .collect()
}

pub fn validate_scopes(scopes: Vec<String>) -> Result<Vec<String>> {
    let known: HashSet<_> = all_known_scopes().into_iter().collect();
    let mut normalized = Vec::new();
    for scope in scopes {
        let scope = scope.trim().to_string();
        if scope.is_empty() {
            continue;
        }
        if !known.contains(&scope) {
            return Err(PostGateError::InvalidState(format!(
                "Unknown MCP scope '{}'",
                scope
            )));
        }
        if !normalized.contains(&scope) {
            normalized.push(scope);
        }
    }
    Ok(normalized)
}

pub async fn authenticate_bearer(
    state: &Arc<AppState>,
    authorization: Option<&str>,
) -> Result<McpClient> {
    let token = authorization
        .and_then(|value| value.strip_prefix("Bearer "))
        .ok_or_else(|| PostGateError::InvalidState("Missing MCP bearer token".into()))?;

    let db = state.get_database().await?;
    let records = db.get_mcp_client_auth_records().await?;
    for record in records {
        if token_matches(&record, token) {
            db.touch_mcp_client(&record.client.id).await?;
            return Ok(record.client);
        }
    }

    Err(PostGateError::InvalidState(
        "Invalid MCP bearer token".into(),
    ))
}

fn token_matches(record: &McpClientAuthRecord, token: &str) -> bool {
    hash_token(&record.token_salt, token) == record.token_hash
}

pub fn required_scopes_for_tool(name: &str) -> Vec<&'static str> {
    match name {
        "postgate.proxy.status" | "postgate.proxy.get_local_ips" => vec![SCOPE_PROXY_READ],
        "postgate.proxy.start" | "postgate.proxy.stop" | "postgate.proxy.set_persistence" => {
            vec![SCOPE_PROXY_CONTROL]
        }
        "postgate.rules.list_groups" | "postgate.rules.get_group" | "postgate.rules.validate" => {
            vec![SCOPE_RULES_READ]
        }
        "postgate.rules.upsert_group"
        | "postgate.rules.append_lines"
        | "postgate.rules.toggle_group"
        | "postgate.rules.delete_group" => vec![SCOPE_RULES_WRITE],
        "postgate.values.list" => vec![SCOPE_VALUES_READ],
        "postgate.values.save" | "postgate.values.rename" | "postgate.values.delete" => {
            vec![SCOPE_VALUES_WRITE]
        }
        "postgate.capture.search" | "postgate.capture.get" => vec![SCOPE_CAPTURE_READ],
        "postgate.capture.get_body" => vec![SCOPE_CAPTURE_READ, SCOPE_BODY_READ],
        "postgate.capture.clear_history" => vec![SCOPE_HISTORY_DELETE],
        "postgate.replay.execute" => vec![SCOPE_REPLAY_EXECUTE],
        "postgate.replay.import_capture" => {
            vec![SCOPE_CAPTURE_READ, SCOPE_BODY_READ, SCOPE_REPLAY_EXECUTE]
        }
        "postgate.debug.status"
        | "postgate.debug.sessions"
        | "postgate.debug.console_logs"
        | "postgate.debug.page_errors"
        | "postgate.debug.network_requests" => vec![SCOPE_DEBUG_READ],
        _ => vec![SCOPE_MCP_ADMIN],
    }
}

pub fn required_scopes_for_resource(uri: &str) -> Vec<&'static str> {
    if uri == "postgate://proxy/status" {
        vec![SCOPE_PROXY_READ]
    } else if uri == "postgate://rules/groups" {
        vec![SCOPE_RULES_READ]
    } else if uri == "postgate://captures/recent" || uri.starts_with("postgate://captures/") {
        vec![SCOPE_CAPTURE_READ]
    } else if uri == "postgate://debug/sessions" {
        vec![SCOPE_DEBUG_READ]
    } else {
        vec![SCOPE_MCP_ADMIN]
    }
}

pub fn has_required_scopes(client: &McpClient, required: &[&str]) -> bool {
    required
        .iter()
        .all(|scope| client.scopes.iter().any(|owned| owned == scope))
}

pub async fn audit_mcp(
    state: &Arc<AppState>,
    client_id: Option<String>,
    operation: impl Into<String>,
    target: Option<String>,
    allowed: bool,
    detail: Option<String>,
) {
    let event = McpAuditEvent {
        id: Uuid::new_v4().to_string(),
        timestamp: chrono::Utc::now().timestamp_millis(),
        client_id,
        operation: operation.into(),
        target,
        allowed,
        detail,
    };

    match state.get_database().await {
        Ok(db) => {
            if let Err(e) = db.insert_mcp_audit_event(&event).await {
                tracing::warn!("Failed to record MCP audit event: {}", e);
            }
        }
        Err(e) => tracing::warn!("Failed to open DB for MCP audit event: {}", e),
    }
}
