use crate::error::{PostGateError, Result};
use crate::mcp::auth::validate_scopes;
use crate::mcp::manager;
use crate::mcp::{
    CreateMcpClientInput, CreatedMcpClient, McpAuditEvent, McpClient, McpClientConfig, McpStatus,
};
use crate::state::AppState;
use serde::Deserialize;
use std::sync::Arc;
use tauri::State;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartMcpServerInput {
    pub port: Option<u16>,
    pub allowed_origins: Option<Vec<String>>,
}

#[tauri::command]
pub async fn get_mcp_status(state: State<'_, Arc<AppState>>) -> Result<McpStatus> {
    manager::status(&state, None).await
}

#[tauri::command]
pub async fn start_mcp_server(
    input: Option<StartMcpServerInput>,
    state: State<'_, Arc<AppState>>,
) -> Result<McpStatus> {
    let input = input.unwrap_or(StartMcpServerInput {
        port: None,
        allowed_origins: None,
    });
    manager::start_server(Arc::clone(&state), input.port, input.allowed_origins).await
}

#[tauri::command]
pub async fn stop_mcp_server(state: State<'_, Arc<AppState>>) -> Result<McpStatus> {
    manager::stop_server(Arc::clone(&state), true).await
}

#[tauri::command]
pub async fn create_mcp_client(
    input: CreateMcpClientInput,
    state: State<'_, Arc<AppState>>,
) -> Result<CreatedMcpClient> {
    manager::create_client(&state, input).await
}

#[tauri::command]
pub async fn list_mcp_clients(state: State<'_, Arc<AppState>>) -> Result<Vec<McpClient>> {
    let db = state.get_database().await?;
    db.get_mcp_clients().await
}

#[tauri::command]
pub async fn revoke_mcp_client(id: String, state: State<'_, Arc<AppState>>) -> Result<bool> {
    let db = state.get_database().await?;
    db.revoke_mcp_client(&id).await
}

#[tauri::command]
pub async fn rotate_mcp_client_token(
    id: String,
    state: State<'_, Arc<AppState>>,
) -> Result<CreatedMcpClient> {
    manager::rotate_client_token(&state, &id)
        .await?
        .ok_or_else(|| PostGateError::NotFound(format!("MCP client '{}' not found", id)))
}

#[tauri::command]
pub async fn set_mcp_client_scopes(
    id: String,
    scopes: Vec<String>,
    state: State<'_, Arc<AppState>>,
) -> Result<bool> {
    let scopes = validate_scopes(scopes)?;
    let db = state.get_database().await?;
    db.update_mcp_client_scopes(&id, &scopes).await
}

#[tauri::command]
pub async fn get_mcp_client_config(state: State<'_, Arc<AppState>>) -> Result<McpClientConfig> {
    manager::client_config(&state).await
}

#[tauri::command]
pub async fn list_mcp_audit_events(
    limit: Option<i32>,
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<McpAuditEvent>> {
    let db = state.get_database().await?;
    db.list_mcp_audit_events(limit.unwrap_or(100)).await
}
