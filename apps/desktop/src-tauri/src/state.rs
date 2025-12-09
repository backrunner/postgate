use crate::cert::CertificateAuthority;
use crate::debug::{ConsoleLog, DebugServer, DebugSession, DebugStatus, PageError, SessionManager};
use crate::plugin::PluginManager;
use crate::proxy::{BodyStorage, ProxyServer};
use crate::rules::RuleEngine;
use crate::storage::{CapturedRequestStorage, Database};
use parking_lot::RwLock;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::broadcast;

/// Application-wide shared state
pub struct AppState {
    pub app_handle: AppHandle,
    pub ca: Arc<RwLock<Option<CertificateAuthority>>>,
    pub proxy: Arc<RwLock<Option<ProxyServer>>>,
    pub rule_engine: Arc<RuleEngine>,
    pub request_tx: broadcast::Sender<CapturedRequestEvent>,
    pub body_storage: Arc<BodyStorage>,
    pub plugin_manager: tokio::sync::RwLock<PluginManager>,
    pub plugins_dir: PathBuf,
    pub data_dir: PathBuf,
    database: tokio::sync::RwLock<Option<std::sync::Arc<Database>>>,
    debug_server: tokio::sync::RwLock<Option<Arc<DebugServer>>>,
    debug_session_manager: Arc<SessionManager>,
    // Captured request persistence
    captured_storage: tokio::sync::RwLock<Option<Arc<CapturedRequestStorage>>>,
    persistence_enabled: AtomicBool,
}

impl AppState {
    pub fn new(app_handle: AppHandle) -> Self {
        let (request_tx, _) = broadcast::channel(10000);

        // Get app data directory
        let data_dir = app_handle.path().app_data_dir()
            .unwrap_or_else(|_| PathBuf::from("."));

        let plugins_dir = data_dir.join("plugins");
        let plugin_manager = PluginManager::new(plugins_dir.clone());
        let debug_session_manager = SessionManager::new();

        Self {
            app_handle,
            ca: Arc::new(RwLock::new(None)),
            proxy: Arc::new(RwLock::new(None)),
            rule_engine: Arc::new(RuleEngine::new()),
            request_tx,
            body_storage: Arc::new(BodyStorage::default()),
            plugin_manager: tokio::sync::RwLock::new(plugin_manager),
            plugins_dir,
            data_dir,
            database: tokio::sync::RwLock::new(None),
            debug_server: tokio::sync::RwLock::new(None),
            debug_session_manager,
            captured_storage: tokio::sync::RwLock::new(None),
            persistence_enabled: AtomicBool::new(false),
        }
    }

    /// Get or initialize the database
    pub async fn get_database(&self) -> crate::error::Result<std::sync::Arc<Database>> {
        // Check if already initialized
        {
            let guard = self.database.read().await;
            if let Some(ref db) = *guard {
                return Ok(db.clone());
            }
        }

        // Initialize database
        let db_path = self.data_dir.join("postgate.db");
        let db = std::sync::Arc::new(Database::new(&db_path).await?);
        
        {
            let mut guard = self.database.write().await;
            *guard = Some(db.clone());
        }

        Ok(db)
    }

    /// Get or initialize captured request storage
    pub async fn get_captured_storage(&self) -> crate::error::Result<Arc<CapturedRequestStorage>> {
        // Check if already initialized
        {
            let guard = self.captured_storage.read().await;
            if let Some(ref storage) = *guard {
                return Ok(storage.clone());
            }
        }

        // Initialize storage
        let db = self.get_database().await?;
        let storage = Arc::new(CapturedRequestStorage::new(db.pool().clone(), &self.data_dir));

        {
            let mut guard = self.captured_storage.write().await;
            *guard = Some(storage.clone());
        }

        Ok(storage)
    }

    /// Set whether persistence is enabled
    pub fn set_persistence_enabled(&self, enabled: bool) {
        self.persistence_enabled.store(enabled, Ordering::Relaxed);
    }

    /// Check if persistence is enabled
    pub fn is_persistence_enabled(&self) -> bool {
        self.persistence_enabled.load(Ordering::Relaxed)
    }

    /// Initialize or get the Certificate Authority (loads from disk if exists)
    pub fn get_or_init_ca(&self) -> crate::error::Result<CertificateAuthority> {
        let mut ca_guard = self.ca.write();

        if let Some(ref ca) = *ca_guard {
            return Ok(ca.clone());
        }

        // Load existing CA or generate new one (persists to disk)
        let ca = CertificateAuthority::load_or_create(&self.data_dir)?;
        *ca_guard = Some(ca.clone());

        Ok(ca)
    }

    /// Emit a request event to the frontend and persist asynchronously
    pub fn emit_request_event(&self, event: &CapturedRequestEvent) {
        // Send via broadcast channel for internal use
        let _ = self.request_tx.send(event.clone());

        // Emit to frontend via Tauri
        if let Err(e) = self.app_handle.emit("proxy:request", event) {
            tracing::warn!("Failed to emit request event: {}", e);
        }

        // Async persistence (only for completed events to avoid duplicates)
        if self.is_persistence_enabled() {
            let data = event.data.clone();
            let data_dir = self.data_dir.clone();
            let app_handle = self.app_handle.clone();
            
            tokio::spawn(async move {
                if let Err(e) = Self::persist_request(app_handle, data_dir, data).await {
                    tracing::warn!("Failed to persist captured request: {}", e);
                }
            });
        }
    }

    /// Persist a captured request to storage
    async fn persist_request(
        app_handle: AppHandle,
        data_dir: PathBuf,
        data: CapturedRequestData,
    ) -> crate::error::Result<()> {
        // Get database path and create storage
        let db_path = data_dir.join("postgate.db");
        let db = Database::new(&db_path).await?;
        let storage = CapturedRequestStorage::new(db.pool().clone(), &data_dir);
        storage.save_request(&data).await?;
        Ok(())
    }

    /// Persist body data asynchronously
    pub fn persist_body(&self, request_id: String, body: bytes::Bytes, is_request: bool) {
        if !self.is_persistence_enabled() {
            return;
        }

        let data_dir = self.data_dir.clone();

        tokio::spawn(async move {
            if let Err(e) = Self::persist_body_internal(data_dir, request_id, body, is_request).await {
                tracing::warn!("Failed to persist body: {}", e);
            }
        });
    }

    async fn persist_body_internal(
        data_dir: PathBuf,
        request_id: String,
        body: bytes::Bytes,
        is_request: bool,
    ) -> crate::error::Result<()> {
        let db_path = data_dir.join("postgate.db");
        let db = Database::new(&db_path).await?;
        let storage = CapturedRequestStorage::new(db.pool().clone(), &data_dir);
        storage.save_body(&request_id, &body, is_request).await?;
        Ok(())
    }

    // Debug server methods

    /// Start the debug WebSocket server
    pub async fn start_debug_server(&self, port: u16) -> Result<(), String> {
        let mut server_guard = self.debug_server.write().await;
        
        if server_guard.is_some() {
            return Err("Debug server is already running".to_string());
        }

        let server = DebugServer::new(Arc::clone(&self.debug_session_manager));
        server.start(port).await?;
        *server_guard = Some(server);

        // Emit event to frontend
        let _ = self.app_handle.emit("debug:server_started", port);

        Ok(())
    }

    /// Stop the debug server
    pub async fn stop_debug_server(&self) {
        let mut server_guard = self.debug_server.write().await;
        
        if let Some(server) = server_guard.take() {
            server.stop().await;
        }

        let _ = self.app_handle.emit("debug:server_stopped", ());
    }

    /// Get debug server status
    pub async fn get_debug_status(&self) -> Result<DebugStatus, String> {
        let server_guard = self.debug_server.read().await;
        
        if let Some(ref server) = *server_guard {
            Ok(server.get_status().await)
        } else {
            Ok(DebugStatus {
                is_running: false,
                port: 9229,
                session_count: 0,
                total_logs: 0,
            })
        }
    }

    /// Get all debug sessions
    pub fn get_debug_sessions(&self) -> Vec<DebugSession> {
        self.debug_session_manager.get_sessions()
    }

    /// Get console logs
    pub fn get_console_logs(&self, session_id: Option<&str>, limit: Option<usize>, offset: Option<usize>) -> Vec<ConsoleLog> {
        if let Some(sid) = session_id {
            self.debug_session_manager.get_console_logs(sid, limit, offset)
        } else {
            self.debug_session_manager.get_all_console_logs(limit)
        }
    }

    /// Clear console logs
    pub fn clear_console_logs(&self, session_id: Option<&str>) {
        if let Some(sid) = session_id {
            self.debug_session_manager.clear_console_logs(sid);
        } else {
            self.debug_session_manager.clear_all();
        }
    }

    /// Get page errors
    pub fn get_page_errors(&self, session_id: &str) -> Vec<PageError> {
        self.debug_session_manager.get_page_errors(session_id)
    }

    /// Clear all debug data
    pub fn clear_all_debug_data(&self) {
        self.debug_session_manager.clear_all();
    }

    /// Remove a debug session
    pub fn remove_debug_session(&self, session_id: &str) {
        self.debug_session_manager.remove_session(session_id);
    }

    /// Get the debug session manager (for use by proxy handler)
    pub fn get_debug_session_manager(&self) -> &Arc<SessionManager> {
        &self.debug_session_manager
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
