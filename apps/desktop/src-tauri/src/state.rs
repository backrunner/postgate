use crate::cert::CertificateAuthority;
use crate::proxy::{BodyStorage, ProxyServer};
use crate::rules::RuleEngine;
use parking_lot::RwLock;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::sync::broadcast;

/// Application-wide shared state
pub struct AppState {
    pub app_handle: AppHandle,
    pub ca: Arc<RwLock<Option<CertificateAuthority>>>,
    pub proxy: Arc<RwLock<Option<ProxyServer>>>,
    pub rule_engine: Arc<RuleEngine>,
    pub request_tx: broadcast::Sender<CapturedRequestEvent>,
    pub body_storage: Arc<BodyStorage>,
}

impl AppState {
    pub fn new(app_handle: AppHandle) -> Self {
        let (request_tx, _) = broadcast::channel(10000);

        Self {
            app_handle,
            ca: Arc::new(RwLock::new(None)),
            proxy: Arc::new(RwLock::new(None)),
            rule_engine: Arc::new(RuleEngine::new()),
            request_tx,
            body_storage: Arc::new(BodyStorage::default()),
        }
    }

    /// Initialize or get the Certificate Authority
    pub fn get_or_init_ca(&self) -> crate::error::Result<CertificateAuthority> {
        let mut ca_guard = self.ca.write();

        if let Some(ref ca) = *ca_guard {
            return Ok(ca.clone());
        }

        // Generate new CA
        let ca = CertificateAuthority::new()?;
        *ca_guard = Some(ca.clone());

        Ok(ca)
    }

    /// Emit a request event to the frontend
    pub fn emit_request_event(&self, event: &CapturedRequestEvent) {
        // Send via broadcast channel for internal use
        let _ = self.request_tx.send(event.clone());

        // Emit to frontend via Tauri
        if let Err(e) = self.app_handle.emit("proxy:request", event) {
            tracing::warn!("Failed to emit request event: {}", e);
        }
    }
}

/// Event sent when a request is captured or updated
#[derive(Debug, Clone, serde::Serialize)]
pub struct CapturedRequestEvent {
    pub id: String,
    #[serde(rename = "eventType")]
    pub event_type: RequestEventType,
    pub data: CapturedRequestData,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestEventType {
    Started,
    ResponseReceived,
    Completed,
    Error,
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct CapturedRequestData {
    pub id: String,
    pub timestamp: i64,
    pub method: String,
    pub url: String,
    pub host: String,
    pub path: String,
    #[serde(rename = "requestHeaders", skip_serializing_if = "Option::is_none")]
    pub request_headers: Option<std::collections::HashMap<String, String>>,
    #[serde(rename = "responseStatus", skip_serializing_if = "Option::is_none")]
    pub response_status: Option<u16>,
    #[serde(rename = "responseHeaders", skip_serializing_if = "Option::is_none")]
    pub response_headers: Option<std::collections::HashMap<String, String>>,
    #[serde(rename = "durationMs", skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(rename = "matchedRules")]
    pub matched_rules: Vec<String>,
    pub protocol: String,
    #[serde(rename = "contentType", skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    #[serde(rename = "requestSize")]
    pub request_size: u64,
    #[serde(rename = "responseSize", skip_serializing_if = "Option::is_none")]
    pub response_size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(rename = "tlsVersion", skip_serializing_if = "Option::is_none")]
    pub tls_version: Option<String>,
    #[serde(rename = "remoteAddr", skip_serializing_if = "Option::is_none")]
    pub remote_addr: Option<String>,
}
