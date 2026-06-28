use crate::api::PostGateApi;
use crate::error::{PostGateError, Result};
use crate::mcp::auth::{
    audit_mcp, authenticate_bearer, generate_salt, generate_token, has_required_scopes, hash_token,
    required_scopes_for_resource, required_scopes_for_tool, validate_scopes,
};
use crate::mcp::server::PostGateMcpServer;
use crate::mcp::{
    default_client_scopes, CreateMcpClientInput, CreatedMcpClient, McpClient, McpClientAuthRecord,
    McpClientConfig, McpStatus,
};
use crate::state::AppState;
use axum::{
    body::{to_bytes, Body},
    extract::State,
    http::{Request, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    Router,
};
use rmcp::transport::streamable_http_server::{
    session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
};
use serde_json::json;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

const MAX_MCP_REQUEST_BYTES: usize = 16 * 1024 * 1024;

pub struct McpRuntime {
    pub port: u16,
    cancellation: CancellationToken,
    handle: JoinHandle<()>,
}

impl McpRuntime {
    pub async fn stop(self) {
        self.cancellation.cancel();
        if let Err(e) = self.handle.await {
            tracing::warn!("MCP server task join failed: {}", e);
        }
    }
}

#[derive(Clone)]
struct McpHttpState {
    state: Arc<AppState>,
    port: u16,
    allowed_origins: Vec<String>,
}

pub async fn start_runtime(
    state: Arc<AppState>,
    port: u16,
    allowed_origins: Vec<String>,
) -> Result<McpRuntime> {
    let addr: SocketAddr = format!("127.0.0.1:{port}")
        .parse()
        .map_err(|e| PostGateError::InvalidState(format!("Invalid MCP address: {}", e)))?;
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| PostGateError::InvalidState(format!("Failed to bind MCP server: {}", e)))?;

    let cancellation = CancellationToken::new();
    let api = PostGateApi::new(Arc::clone(&state));
    let service: StreamableHttpService<PostGateMcpServer, LocalSessionManager> =
        StreamableHttpService::new(
            move || Ok(PostGateMcpServer::new(api.clone())),
            Default::default(),
            StreamableHttpServerConfig::default()
                .with_allowed_hosts([
                    "localhost".to_string(),
                    "127.0.0.1".to_string(),
                    "::1".to_string(),
                    format!("localhost:{port}"),
                    format!("127.0.0.1:{port}"),
                ])
                .with_allowed_origins(loopback_origins(port, &allowed_origins))
                .with_cancellation_token(cancellation.child_token()),
        );

    let http_state = McpHttpState {
        state,
        port,
        allowed_origins,
    };
    let router = Router::new()
        .nest_service("/mcp", service)
        .layer(middleware::from_fn_with_state(
            http_state,
            mcp_auth_middleware,
        ));

    let shutdown = cancellation.clone();
    let handle = tokio::spawn(async move {
        tracing::info!("PostGate MCP server listening on http://{addr}/mcp");
        if let Err(e) = axum::serve(listener, router)
            .with_graceful_shutdown(async move { shutdown.cancelled_owned().await })
            .await
        {
            tracing::error!("MCP server failed: {}", e);
        }
    });

    Ok(McpRuntime {
        port,
        cancellation,
        handle,
    })
}

pub async fn status(state: &Arc<AppState>, error: Option<String>) -> Result<McpStatus> {
    let settings = state.get_database().await?.get_mcp_settings().await?;
    let running = state.mcp_runtime.read().await.is_some();
    let port = state
        .mcp_runtime
        .read()
        .await
        .as_ref()
        .map(|runtime| runtime.port)
        .unwrap_or(settings.port);
    let client_count = state.get_database().await?.get_mcp_clients().await?.len();

    Ok(McpStatus {
        enabled: settings.enabled,
        running,
        port,
        endpoint: endpoint(port),
        client_count,
        error,
    })
}

pub async fn start_server(
    state: Arc<AppState>,
    port: Option<u16>,
    allowed_origins: Option<Vec<String>>,
) -> Result<McpStatus> {
    let db = state.get_database().await?;
    let mut settings = db.get_mcp_settings().await?;
    if let Some(port) = port {
        settings.port = port;
    }
    if let Some(origins) = allowed_origins {
        settings.allowed_origins = origins;
    }
    settings.enabled = true;

    {
        let runtime_guard = state.mcp_runtime.read().await;
        if let Some(runtime) = runtime_guard.as_ref() {
            if runtime.port == settings.port {
                let saved = db.save_mcp_settings(&settings).await?;
                return Ok(McpStatus {
                    enabled: saved.enabled,
                    running: true,
                    port: runtime.port,
                    endpoint: endpoint(runtime.port),
                    client_count: db.get_mcp_clients().await?.len(),
                    error: None,
                });
            }
        }
    }

    stop_server(Arc::clone(&state), false).await?;
    let runtime = start_runtime(
        Arc::clone(&state),
        settings.port,
        settings.allowed_origins.clone(),
    )
    .await?;
    *state.mcp_runtime.write().await = Some(runtime);
    db.save_mcp_settings(&settings).await?;
    status(&state, None).await
}

pub async fn stop_server(state: Arc<AppState>, update_settings: bool) -> Result<McpStatus> {
    if let Some(runtime) = state.mcp_runtime.write().await.take() {
        runtime.stop().await;
    }

    if update_settings {
        let db = state.get_database().await?;
        let mut settings = db.get_mcp_settings().await?;
        settings.enabled = false;
        db.save_mcp_settings(&settings).await?;
    }

    status(&state, None).await
}

pub async fn create_client(
    state: &Arc<AppState>,
    input: CreateMcpClientInput,
) -> Result<CreatedMcpClient> {
    let scopes = if input.scopes.is_empty() {
        default_client_scopes()
    } else {
        validate_scopes(input.scopes)?
    };
    let token = generate_token();
    let token_salt = generate_salt();
    let token_hash = hash_token(&token_salt, &token);
    let now = chrono::Utc::now().timestamp_millis();
    let client = McpClient {
        id: Uuid::new_v4().to_string(),
        name: input.name,
        scopes,
        revoked: false,
        created_at: now,
        updated_at: now,
        last_seen_at: None,
    };
    let record = McpClientAuthRecord {
        client: client.clone(),
        token_salt,
        token_hash,
    };
    let db = state.get_database().await?;
    db.save_mcp_client_auth_record(&record).await?;

    let port = db.get_mcp_settings().await?.port;
    let endpoint = endpoint(port);
    Ok(CreatedMcpClient {
        client,
        token: token.clone(),
        endpoint: endpoint.clone(),
        streamable_http_config: streamable_http_config(&endpoint, Some(&token)),
        stdio_config: stdio_config(&endpoint, Some(&token)),
    })
}

pub async fn rotate_client_token(
    state: &Arc<AppState>,
    id: &str,
) -> Result<Option<CreatedMcpClient>> {
    let token = generate_token();
    let token_salt = generate_salt();
    let token_hash = hash_token(&token_salt, &token);
    let db = state.get_database().await?;
    let Some(client) = db
        .rotate_mcp_client_token(id, &token_salt, &token_hash)
        .await?
    else {
        return Ok(None);
    };
    let endpoint = endpoint(db.get_mcp_settings().await?.port);
    Ok(Some(CreatedMcpClient {
        client,
        token: token.clone(),
        endpoint: endpoint.clone(),
        streamable_http_config: streamable_http_config(&endpoint, Some(&token)),
        stdio_config: stdio_config(&endpoint, Some(&token)),
    }))
}

pub async fn client_config(state: &Arc<AppState>) -> Result<McpClientConfig> {
    let endpoint = endpoint(state.get_database().await?.get_mcp_settings().await?.port);
    Ok(McpClientConfig {
        endpoint: endpoint.clone(),
        streamable_http_config: streamable_http_config(&endpoint, None),
        stdio_config: stdio_config(&endpoint, None),
    })
}

fn endpoint(port: u16) -> String {
    format!("http://127.0.0.1:{port}/mcp")
}

fn streamable_http_config(endpoint: &str, token: Option<&str>) -> serde_json::Value {
    match token {
        Some(token) => json!({
            "type": "streamable-http",
            "url": endpoint,
            "headers": { "Authorization": format!("Bearer {token}") }
        }),
        None => json!({
            "type": "streamable-http",
            "url": endpoint
        }),
    }
}

fn stdio_config(endpoint: &str, token: Option<&str>) -> serde_json::Value {
    json!({
        "command": "postgate-mcp-stdio",
        "args": [],
        "env": {
            "POSTGATE_MCP_ENDPOINT": endpoint,
            "POSTGATE_MCP_TOKEN": token.unwrap_or("<rotate-or-create-a-client-token>")
        }
    })
}

async fn mcp_auth_middleware(
    State(http_state): State<McpHttpState>,
    req: Request<Body>,
    next: Next,
) -> Response {
    if !origin_allowed(&http_state, req.headers()) {
        return (StatusCode::FORBIDDEN, "Origin is not allowed").into_response();
    }

    let auth_header = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let client = match authenticate_bearer(&http_state.state, auth_header.as_deref()).await {
        Ok(client) => client,
        Err(e) => {
            audit_mcp(
                &http_state.state,
                None,
                "auth",
                None,
                false,
                Some(e.to_string()),
            )
            .await;
            return (StatusCode::UNAUTHORIZED, "Invalid MCP bearer token").into_response();
        }
    };

    let (parts, body) = req.into_parts();
    let bytes = match to_bytes(body, MAX_MCP_REQUEST_BYTES).await {
        Ok(bytes) => bytes,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                format!("Failed to read MCP request body: {e}"),
            )
                .into_response();
        }
    };

    if let Some(decision) = inspect_scope_decision(&bytes) {
        let allowed = has_required_scopes(&client, &decision.required_scopes);
        audit_mcp(
            &http_state.state,
            Some(client.id.clone()),
            decision.operation.clone(),
            decision.target.clone(),
            allowed,
            (!allowed).then(|| {
                format!(
                    "Missing required scope(s): {}",
                    decision.required_scopes.join(", ")
                )
            }),
        )
        .await;
        if !allowed {
            return (StatusCode::FORBIDDEN, "MCP client scope is not sufficient").into_response();
        }
    }

    let req = Request::from_parts(parts, Body::from(bytes));
    next.run(req).await
}

struct ScopeDecision {
    operation: String,
    target: Option<String>,
    required_scopes: Vec<&'static str>,
}

fn inspect_scope_decision(bytes: &[u8]) -> Option<ScopeDecision> {
    let value: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    let method = value.get("method")?.as_str()?;
    match method {
        "tools/call" => {
            let name = value.get("params")?.get("name")?.as_str()?;
            Some(ScopeDecision {
                operation: "tools/call".to_string(),
                target: Some(name.to_string()),
                required_scopes: required_scopes_for_tool(name),
            })
        }
        "resources/read" => {
            let uri = value.get("params")?.get("uri")?.as_str()?;
            Some(ScopeDecision {
                operation: "resources/read".to_string(),
                target: Some(uri.to_string()),
                required_scopes: required_scopes_for_resource(uri),
            })
        }
        _ => None,
    }
}

fn origin_allowed(http_state: &McpHttpState, headers: &axum::http::HeaderMap) -> bool {
    let Some(origin) = headers
        .get(axum::http::header::ORIGIN)
        .and_then(|value| value.to_str().ok())
    else {
        return true;
    };

    loopback_origins(http_state.port, &http_state.allowed_origins)
        .iter()
        .any(|allowed| allowed == origin)
}

fn loopback_origins(port: u16, configured: &[String]) -> Vec<String> {
    let mut origins = vec![
        format!("http://127.0.0.1:{port}"),
        format!("http://localhost:{port}"),
    ];
    for origin in configured {
        if !origins.contains(origin) {
            origins.push(origin.clone());
        }
    }
    origins
}
