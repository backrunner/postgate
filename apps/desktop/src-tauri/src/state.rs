use crate::cert::CertificateAuthority;
use crate::debug::{ConsoleLog, DebugServer, DebugSession, DebugStatus, PageError, SessionManager};
use crate::plugin::PluginManager;
use crate::proxy::{BodyStorage, ProxyServer};
use crate::rules::RuleEngine;
use crate::storage::{CapturedRequestStorage, Database};
use dashmap::DashMap;
use parking_lot::RwLock;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::{broadcast, mpsc};

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
    /// In-memory values store (whistle-compatible `{name}` references).
    /// Populated lazily from SQLite on first access; kept in sync by the
    /// values commands.
    pub values_store: Arc<DashMap<String, String>>,
    /// Whether `values_store` has been populated from the database yet.
    values_loaded: AtomicBool,
    database: tokio::sync::RwLock<Option<std::sync::Arc<Database>>>,
    debug_server: tokio::sync::RwLock<Option<Arc<DebugServer>>>,
    debug_session_manager: Arc<SessionManager>,
    // Captured request persistence
    captured_storage: tokio::sync::RwLock<Option<Arc<CapturedRequestStorage>>>,
    persistence_enabled: AtomicBool,
    /// Async channel for offloading `app_handle.emit` + persistence off the
    /// proxy hot path. Populated on first use via `ensure_emit_worker`.
    emit_tx: OnceLock<mpsc::Sender<CapturedRequestEvent>>,
    /// Running count of request events dropped because `emit_tx` was full
    /// or because the broadcast channel had no capacity. Previously each
    /// drop emitted its own tracing::warn, which at 100 req/s turned into
    /// a log flood (and itself a memory pressure vector via tracing
    /// buffers). We now log a single summary warning at most once per
    /// DROP_LOG_INTERVAL_MS.
    dropped_events: AtomicU64,
    /// Epoch-ms when we last emitted a "dropped N events" warning; 0 means
    /// never. Used to rate-limit the warning to at most one per interval.
    last_drop_warn_ms: AtomicI64,
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
            values_store: Arc::new(DashMap::new()),
            values_loaded: AtomicBool::new(false),
            database: tokio::sync::RwLock::new(None),
            debug_server: tokio::sync::RwLock::new(None),
            debug_session_manager,
            captured_storage: tokio::sync::RwLock::new(None),
            persistence_enabled: AtomicBool::new(false),
            emit_tx: OnceLock::new(),
            dropped_events: AtomicU64::new(0),
            last_drop_warn_ms: AtomicI64::new(0),
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

    /// Ensure the in-memory values store is populated from the database.
    /// Safe to call multiple times; subsequent calls are cheap.
    pub async fn ensure_values_loaded(&self) -> crate::error::Result<()> {
        if self.values_loaded.load(Ordering::Relaxed) {
            return Ok(());
        }
        let db = self.get_database().await?;
        let entries = db.list_values().await?;
        self.values_store.clear();
        for entry in entries {
            self.values_store.insert(entry.name, entry.content);
        }
        self.values_loaded.store(true, Ordering::Relaxed);
        Ok(())
    }

    /// Get or initialize captured request storage
    pub async fn get_captured_storage(&self) -> crate::error::Result<Arc<CapturedRequestStorage>> {        // Check if already initialized
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

    /// Emit a request event to the frontend and persist asynchronously.
    ///
    /// The hot path (proxy code) does ONLY a non-blocking channel send here.
    /// A dedicated background task deserializes + calls `app_handle.emit`
    /// (which runs serde_json on the current thread + writes to the Tauri
    /// IPC pipe) and then kicks off persistence. Doing this work inline used
    /// to add milliseconds per request on busy pages (80+ resources each
    /// emitting started + completed events).
    pub fn emit_request_event(self: &Arc<Self>, event: &CapturedRequestEvent) {
        // Broadcast for in-process subscribers — skip the clone cost if no
        // one is listening (there are currently no subscribers in tree, but
        // the API is retained for future use).
        if self.request_tx.receiver_count() > 0 {
            if self.request_tx.send(event.clone()).is_err() {
                // Broadcast fails only if all receivers have dropped since
                // we last checked — no real user-visible problem, so just
                // bump the drop counter silently.
                self.record_drop();
            }
        }

        let tx = self.ensure_emit_worker();
        // try_send keeps the hot path non-blocking. If the worker is saturated
        // (e.g. frontend paused), drop the event rather than back-pressure the
        // proxy — the UI is informational, not critical correctness.
        if let Err(e) = tx.try_send(event.clone()) {
            self.record_drop();
            match e {
                mpsc::error::TrySendError::Full(_) => {
                    // Full queue is the common failure mode during a
                    // capture burst; log at most once per window.
                }
                mpsc::error::TrySendError::Closed(_) => {
                    // This shouldn't happen unless the worker panicked —
                    // warn unconditionally so it shows up.
                    tracing::error!("Emit worker queue closed; cannot emit request event");
                }
            }
        }
    }

    /// Rate-limited drop accounting. Increments the counter and, if the
    /// configured interval has elapsed, emits a single summary warning
    /// covering all drops since the last warning.
    fn record_drop(&self) {
        const DROP_LOG_INTERVAL_MS: i64 = 5_000;

        self.dropped_events.fetch_add(1, Ordering::Relaxed);
        let now = chrono::Utc::now().timestamp_millis();
        let last = self.last_drop_warn_ms.load(Ordering::Relaxed);
        if now - last < DROP_LOG_INTERVAL_MS {
            return;
        }
        // CAS so only one thread wins the right to log — otherwise a burst
        // of drops would all pass the check above and re-flood the log.
        if self
            .last_drop_warn_ms
            .compare_exchange(last, now, Ordering::Relaxed, Ordering::Relaxed)
            .is_err()
        {
            return;
        }
        let count = self.dropped_events.swap(0, Ordering::Relaxed);
        if count > 0 {
            tracing::warn!(
                dropped = count,
                window_ms = DROP_LOG_INTERVAL_MS,
                "emit queue saturated — dropping request events (UI may be paused or lagging)"
            );
        }
    }

    /// Emit a request event only if capture is enabled for this request.
    /// Used to implement whistle `disable://capture` — the request still
    /// proxies normally, but no UI / persistence trace is left.
    pub fn emit_request_event_if(
        self: &Arc<Self>,
        capture: bool,
        event: &CapturedRequestEvent,
    ) {
        if !capture {
            return;
        }
        self.emit_request_event(event);
    }

    /// Start (lazily) the background task that drains the emit queue. One
    /// task per `AppState`; returns the sender.
    fn ensure_emit_worker(self: &Arc<Self>) -> &mpsc::Sender<CapturedRequestEvent> {
        self.emit_tx.get_or_init(|| {
            // Bounded queue so we don't grow memory unboundedly if the UI is
            // slow to drain. 4096 is enough to buffer several seconds of
            // high-rate capture on a typical page load.
            let (tx, mut rx) = mpsc::channel::<CapturedRequestEvent>(4096);
            let this = self.clone();
            tokio::spawn(async move {
                while let Some(event) = rx.recv().await {
                    if let Err(e) = this.app_handle.emit("proxy:request", &event) {
                        tracing::warn!("Failed to emit request event: {}", e);
                    }
                    if this.is_persistence_enabled() {
                        if let Err(e) = this.persist_request(event.data).await {
                            tracing::warn!("Failed to persist captured request: {}", e);
                        }
                    }
                }
            });
            tx
        })
    }

    /// Persist a captured request to storage using the shared storage handle
    /// (avoids opening a fresh SQLite pool + re-running migrations per request).
    async fn persist_request(&self, data: CapturedRequestData) -> crate::error::Result<()> {
        let storage = self.get_captured_storage().await?;
        storage.save_request(&data).await?;
        Ok(())
    }

    /// Persist body data asynchronously
    pub fn persist_body(self: &Arc<Self>, request_id: String, body: bytes::Bytes, is_request: bool) {
        if !self.is_persistence_enabled() {
            return;
        }

        let this = self.clone();
        tokio::spawn(async move {
            if let Err(e) = this.persist_body_internal(request_id, body, is_request).await {
                tracing::warn!("Failed to persist body: {}", e);
            }
        });
    }

    async fn persist_body_internal(
        &self,
        request_id: String,
        body: bytes::Bytes,
        is_request: bool,
    ) -> crate::error::Result<()> {
        let storage = self.get_captured_storage().await?;
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

    // ==================== Streaming Events (SSE/WebSocket) ====================

    /// Emit a stream message event to the frontend
    pub fn emit_stream_message(&self, event: &StreamMessageEvent) {
        if let Err(e) = self.app_handle.emit("proxy:stream-message", event) {
            tracing::warn!("Failed to emit stream message event: {}", e);
        }
    }

    /// Emit a stream ended event to the frontend
    pub fn emit_stream_ended(&self, event: &StreamEndedEvent) {
        if let Err(e) = self.app_handle.emit("proxy:stream-ended", event) {
            tracing::warn!("Failed to emit stream ended event: {}", e);
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

// ==================== Streaming Events (SSE/WebSocket) ====================

/// Event sent when a stream message is received (SSE event or WebSocket frame)
#[derive(Debug, Clone, serde::Serialize)]
pub struct StreamMessageEvent {
    /// Connection/request ID this message belongs to
    #[serde(rename = "connectionId")]
    pub connection_id: String,
    /// The stream message data
    pub message: StreamMessage,
}

/// A single message in a stream (SSE event or WebSocket frame)
#[derive(Debug, Clone, serde::Serialize)]
pub struct StreamMessage {
    /// Unique message ID
    pub id: String,
    /// Timestamp when the message was captured
    pub timestamp: i64,
    /// Direction of the message
    pub direction: StreamDirection,
    /// Type of the message
    #[serde(rename = "messageType")]
    pub message_type: StreamMessageType,
    /// Message data (text content or base64 encoded binary)
    pub data: String,
    /// Whether the data is base64 encoded
    #[serde(rename = "isBase64")]
    pub is_base64: bool,
    /// Size in bytes
    pub size: usize,
}

/// Direction of a stream message
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum StreamDirection {
    /// Message from server to client (downstream)
    Inbound,
    /// Message from client to server (upstream)
    Outbound,
}

/// Type of stream message
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StreamMessageType {
    // SSE types
    SseEvent,
    
    // WebSocket types
    WsText,
    WsBinary,
    WsPing,
    WsPong,
    WsClose,
}

/// Event sent when a stream connection ends
#[derive(Debug, Clone, serde::Serialize)]
pub struct StreamEndedEvent {
    /// Connection/request ID
    #[serde(rename = "connectionId")]
    pub connection_id: String,
    /// Total messages received
    #[serde(rename = "messageCount")]
    pub message_count: u64,
    /// Total bytes transferred
    #[serde(rename = "totalBytes")]
    pub total_bytes: u64,
    /// Duration in milliseconds
    #[serde(rename = "durationMs")]
    pub duration_ms: u64,
    /// Close reason (if any)
    #[serde(rename = "closeReason", skip_serializing_if = "Option::is_none")]
    pub close_reason: Option<String>,
}
