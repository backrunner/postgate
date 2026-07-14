use crate::error::Result;
use crate::proxy::{ProxyConfig, ProxyServer, ProxyStatus};
use crate::state::{redact_headers, AppState};
use crate::storage::{PaginatedResult, StoredCapturedRequest};
use serde::Serialize;
use std::net::{IpAddr, UdpSocket};
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

    // Ensure rules are loaded from database before starting proxy
    // (Rules page may not have been visited yet)
    if state.rule_engine.get_all_groups().is_empty() {
        if let Ok(db) = state.get_database().await {
            match db.get_rule_groups().await {
                Ok(groups) => {
                    tracing::info!("Pre-loading {} rule groups from database", groups.len());
                    for group in &groups {
                        state.rule_engine.upsert_group(group.clone());
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to pre-load rules from database: {}", e);
                }
            }
        }
    }

    // The proxy can start before the Rules or DevTools pages are ever opened.
    // Start the debug server from backend state so an active debug:// rule is
    // functional from the first matching response, not only after navigation.
    if state.rule_engine.has_active_debug_rules() {
        let debug_status = state
            .get_debug_status()
            .await
            .map_err(crate::error::PostGateError::InvalidState)?;
        if !debug_status.is_running {
            state
                .start_debug_server(config.debug_port)
                .await
                .map_err(crate::error::PostGateError::InvalidState)?;
        }
    }

    // Check if proxy is already running (guard against concurrent starts)
    {
        let proxy_guard = state.proxy.read();
        if let Some(ref proxy) = *proxy_guard {
            if proxy.status() == ProxyStatus::Running {
                return Ok(ProxyStatusResponse {
                    status: ProxyStatus::Running,
                    port: proxy.config().port,
                    error: None,
                });
            }
        }
    }

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
    state.capture_index.clear();
    Ok(())
}

/// Load captured history (paginated)
#[tauri::command]
pub async fn load_captured_history(
    page: i32,
    page_size: i32,
    state: State<'_, Arc<AppState>>,
) -> Result<PaginatedResult<StoredCapturedRequest>> {
    let storage = state.get_captured_storage().await?;
    let mut result = storage.get_requests_paginated(page, page_size).await?;
    for item in &mut result.items {
        if let Some(headers) = &mut item.request_headers {
            redact_headers(headers);
        }
        if let Some(headers) = &mut item.response_headers {
            redact_headers(headers);
        }
    }
    Ok(result)
}

/// Get persisted request body
#[tauri::command]
pub async fn get_persisted_request_body(
    id: String,
    state: State<'_, Arc<AppState>>,
) -> Result<Option<Vec<u8>>> {
    let storage = state.get_captured_storage().await?;
    let body = storage.get_body(&id, true).await?;
    Ok(body.map(|b| b.to_vec()))
}

/// Get persisted response body
#[tauri::command]
pub async fn get_persisted_response_body(
    id: String,
    state: State<'_, Arc<AppState>>,
) -> Result<Option<Vec<u8>>> {
    let storage = state.get_captured_storage().await?;
    let body = storage.get_body(&id, false).await?;
    Ok(body.map(|b| b.to_vec()))
}

/// Clear all captured history (both memory and persistent)
#[tauri::command]
pub async fn clear_captured_history(state: State<'_, Arc<AppState>>) -> Result<()> {
    // Clear memory storage
    state.body_storage.clear().await;
    state.capture_index.clear();

    // Clear persistent storage
    let storage = state.get_captured_storage().await?;
    storage.clear_all().await?;

    Ok(())
}

/// Clear captured history before specified timestamp
#[tauri::command]
pub async fn clear_captured_history_before(
    before_timestamp: i64,
    state: State<'_, Arc<AppState>>,
) -> Result<u64> {
    let storage = state.get_captured_storage().await?;
    storage.clear_before(before_timestamp).await
}

/// Set persistence enabled/disabled
#[tauri::command]
pub async fn set_persistence_enabled(enabled: bool, state: State<'_, Arc<AppState>>) -> Result<()> {
    state.set_persistence_enabled(enabled);
    Ok(())
}

/// Get persistence enabled status
#[tauri::command]
pub async fn get_persistence_enabled(state: State<'_, Arc<AppState>>) -> Result<bool> {
    Ok(state.is_persistence_enabled())
}

/// Get captured history count
#[tauri::command]
pub async fn get_captured_history_count(state: State<'_, Arc<AppState>>) -> Result<i64> {
    let storage = state.get_captured_storage().await?;
    storage.count().await
}

/// Get all local network addresses for proxy configuration
#[tauri::command]
pub async fn get_local_ip() -> Result<Vec<NetworkAddress>> {
    let mut addresses: Vec<NetworkAddress> = Vec::new();

    // Always include localhost first
    addresses.push(NetworkAddress {
        ip: "127.0.0.1".to_string(),
        name: "Localhost".to_string(),
        is_default: false,
    });

    // Find the default route IP
    let default_ip = UdpSocket::bind("0.0.0.0:0")
        .and_then(|s| {
            s.connect("8.8.8.8:80")?;
            s.local_addr()
        })
        .ok()
        .map(|a| a.ip());

    // Get all network interfaces
    if let Ok(ifaces) = if_addrs::get_if_addrs() {
        for iface in ifaces {
            let ip = iface.ip();
            // Skip loopback and IPv6
            if ip.is_loopback() || matches!(ip, IpAddr::V6(_)) {
                continue;
            }
            let is_default = default_ip.as_ref() == Some(&ip);
            addresses.push(NetworkAddress {
                ip: ip.to_string(),
                name: iface.name.clone(),
                is_default,
            });
        }
    }

    Ok(addresses)
}

#[derive(Debug, Serialize, Clone)]
pub struct NetworkAddress {
    pub ip: String,
    pub name: String,
    pub is_default: bool,
}
