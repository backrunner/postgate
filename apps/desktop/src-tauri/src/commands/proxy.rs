use crate::error::Result;
use crate::proxy::{ProxyConfig, ProxyServer, ProxyStatus};
use crate::state::AppState;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::State;

#[derive(Debug, Serialize)]
pub struct ProxyStatusResponse {
    pub status: ProxyStatus,
    pub port: u16,
    pub error: Option<String>,
}

/// Start the proxy server
#[tauri::command]
pub async fn start_proxy(
    config: ProxyConfig,
    state: State<'_, Arc<AppState>>,
) -> Result<ProxyStatusResponse> {
    tracing::info!("Starting proxy with config: {:?}", config);

    // Get or create CA
    let ca = state.get_or_init_ca()?;

    // Create proxy server with new API
    let mut proxy = ProxyServer::new(
        config.clone(),
        ca,
        state.rule_engine.clone(),
        state.body_storage.clone(),
        Arc::clone(&state),
    );

    // Start the proxy
    proxy.start().await?;

    // Store the proxy
    *state.proxy.write() = Some(proxy);

    Ok(ProxyStatusResponse {
        status: ProxyStatus::Running,
        port: config.port,
        error: None,
    })
}

/// Stop the proxy server
#[tauri::command]
pub async fn stop_proxy(state: State<'_, Arc<AppState>>) -> Result<ProxyStatusResponse> {
    tracing::info!("Stopping proxy");

    // Take the proxy out of the lock to avoid holding the guard across await
    let proxy = {
        let mut proxy_guard = state.proxy.write();
        proxy_guard.take()
    };

    // Stop the proxy if it exists
    if let Some(mut proxy) = proxy {
        proxy.stop().await?;
    }

    // Clear body storage
    state.body_storage.clear().await;

    Ok(ProxyStatusResponse {
        status: ProxyStatus::Stopped,
        port: 0,
        error: None,
    })
}

/// Get the current proxy status
#[tauri::command]
pub async fn get_proxy_status(state: State<'_, Arc<AppState>>) -> Result<ProxyStatusResponse> {
    let proxy_guard = state.proxy.read();

    if let Some(ref proxy) = *proxy_guard {
        Ok(ProxyStatusResponse {
            status: proxy.status(),
            port: proxy.config().port,
            error: None,
        })
    } else {
        Ok(ProxyStatusResponse {
            status: ProxyStatus::Stopped,
            port: 0,
            error: None,
        })
    }
}

/// Get request body by ID
#[tauri::command]
pub async fn get_request_body(
    id: String,
    state: State<'_, Arc<AppState>>,
) -> Result<Option<Vec<u8>>> {
    let body = state.body_storage.get_request_body(&id).await;
    Ok(body.map(|b| b.data.to_vec()))
}

/// Get response body by ID
#[tauri::command]
pub async fn get_response_body(
    id: String,
    state: State<'_, Arc<AppState>>,
) -> Result<Option<Vec<u8>>> {
    let body = state.body_storage.get_response_body(&id).await;
    Ok(body.map(|b| b.data.to_vec()))
}

/// Clear all captured data
#[tauri::command]
pub async fn clear_captured_data(state: State<'_, Arc<AppState>>) -> Result<()> {
    state.body_storage.clear().await;
    Ok(())
}
