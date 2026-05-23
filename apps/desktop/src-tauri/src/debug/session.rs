// Debug session manager

use super::types::*;
use dashmap::DashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use uuid::Uuid;

/// Event emitted when debug state changes
#[derive(Debug, Clone)]
pub enum DebugEvent {
    SessionConnected(DebugSession),
    SessionDisconnected(String),
    ConsoleLog(ConsoleLog),
    PageError(PageError),
    NetworkRequest(PageNetworkRequest),
}

/// How long a session (connected or disconnected) can stay in the manager
/// with no activity before the background reaper evicts it. Without this
/// cap, long-running proxy sessions accumulate thousands of abandoned
/// DevTools tabs plus their 10K-entry log buffers in RAM forever.
const DISCONNECTED_SESSION_TTL: Duration = Duration::from_secs(5 * 60); // 5 min
const IDLE_SESSION_TTL: Duration = Duration::from_secs(30 * 60); // 30 min
const CLEANUP_INTERVAL: Duration = Duration::from_secs(60);
/// Hard cap on concurrent sessions. If exceeded we force-evict the
/// least-recently-active disconnected ones first.
const MAX_SESSIONS: usize = 200;

/// Manages debug sessions and their data
pub struct SessionManager {
    sessions: DashMap<String, DebugSession>,
    console_logs: DashMap<String, Vec<ConsoleLog>>,
    page_errors: DashMap<String, Vec<PageError>>,
    network_requests: DashMap<String, PageNetworkRequest>,
    event_tx: broadcast::Sender<DebugEvent>,
    log_counter: AtomicUsize,
    max_logs_per_session: usize,
}

impl SessionManager {
    pub fn new() -> Arc<Self> {
        let (event_tx, _) = broadcast::channel(1000);
        let manager = Arc::new(Self {
            sessions: DashMap::new(),
            console_logs: DashMap::new(),
            page_errors: DashMap::new(),
            network_requests: DashMap::new(),
            event_tx,
            log_counter: AtomicUsize::new(0),
            max_logs_per_session: 10000,
        });

        Self::start_cleanup_task(&manager);
        manager
    }

    /// Spawn the idle-session reaper onto the ambient async runtime.
    /// Separated from `new()` so the spawn happens from an async context
    /// (lib.rs does this inside `tauri::async_runtime::spawn` at init) —
    /// calling `tokio::spawn` from a plain sync setup thread would panic
    /// if no Tokio handle is entered there. Uses a `Weak` so the task
    /// terminates once the rest of the app has dropped its references.
    fn start_cleanup_task(manager: &Arc<Self>) {
        let weak = Arc::downgrade(manager);
        // `tauri::async_runtime::spawn` is routed to Tokio under the hood
        // but is safe to call from either sync or async contexts, so we
        // use it here to avoid the "called outside Tokio runtime" panic
        // when SessionManager is constructed during Tauri's sync setup.
        tauri::async_runtime::spawn(async move {
            let mut interval = tokio::time::interval(CLEANUP_INTERVAL);
            // First tick fires immediately; we don't need that.
            interval.tick().await;
            loop {
                interval.tick().await;
                let Some(manager) = weak.upgrade() else { break };
                manager.cleanup_stale_sessions();
            }
        });
    }

    /// Create a new debug session
    pub fn create_session(
        &self,
        url: String,
        title: Option<String>,
        user_agent: Option<String>,
        cdp_enabled: bool,
        port: u16,
    ) -> DebugSession {
        let now = chrono::Utc::now().timestamp_millis();
        let id = Uuid::new_v4().to_string();
        let session = DebugSession {
            id: id.clone(),
            url,
            title,
            user_agent,
            connected_at: now,
            last_activity: now,
            is_connected: true,
            cdp_enabled,
            web_socket_debugger_url: format!("ws://127.0.0.1:{}/devtools/page/{}", port, id),
        };

        self.sessions.insert(session.id.clone(), session.clone());
        self.console_logs.insert(session.id.clone(), Vec::new());
        self.page_errors.insert(session.id.clone(), Vec::new());

        // Enforce hard cap on creation so a page opening thousands of
        // short-lived debug sessions can't blow memory before the reaper
        // catches up.
        if self.sessions.len() > MAX_SESSIONS {
            self.enforce_session_cap();
        }

        let _ = self
            .event_tx
            .send(DebugEvent::SessionConnected(session.clone()));

        session
    }

    /// Mark a session as disconnected
    pub fn disconnect_session(&self, session_id: &str) {
        if let Some(mut session) = self.sessions.get_mut(session_id) {
            session.is_connected = false;
            session.last_activity = chrono::Utc::now().timestamp_millis();
        }
        let _ = self
            .event_tx
            .send(DebugEvent::SessionDisconnected(session_id.to_string()));
    }

    /// Remove a session completely
    pub fn remove_session(&self, session_id: &str) {
        self.sessions.remove(session_id);
        self.console_logs.remove(session_id);
        self.page_errors.remove(session_id);
        // Remove network requests for this session
        self.network_requests
            .retain(|_, v| v.session_id != session_id);
    }

    /// Update session activity timestamp
    pub fn update_activity(&self, session_id: &str) {
        let now = chrono::Utc::now().timestamp_millis();
        if let Some(mut session) = self.sessions.get_mut(session_id) {
            session.last_activity = now;
        }
    }

    /// Add a console log
    pub fn add_console_log(&self, session_id: &str, log: ConsoleLog) {
        self.update_activity(session_id);
        self.log_counter.fetch_add(1, Ordering::Relaxed);

        if let Some(mut logs) = self.console_logs.get_mut(session_id) {
            // Enforce max logs limit
            if logs.len() >= self.max_logs_per_session {
                logs.remove(0);
            }
            logs.push(log.clone());
        }

        let _ = self.event_tx.send(DebugEvent::ConsoleLog(log));
    }

    /// Add a page error
    pub fn add_page_error(&self, session_id: &str, error: PageError) {
        self.update_activity(session_id);

        if let Some(mut errors) = self.page_errors.get_mut(session_id) {
            errors.push(error.clone());
        }

        let _ = self.event_tx.send(DebugEvent::PageError(error));
    }

    /// Add or update a network request
    pub fn add_network_request(&self, request: PageNetworkRequest) {
        self.update_activity(&request.session_id);
        let id = request.id.clone();

        let _ = self
            .event_tx
            .send(DebugEvent::NetworkRequest(request.clone()));
        self.network_requests.insert(id, request);
    }

    /// Update an existing network request (e.g., when response arrives)
    pub fn update_network_request(
        &self,
        request_id: &str,
        update: impl FnOnce(&mut PageNetworkRequest),
    ) -> bool {
        if let Some(mut request) = self.network_requests.get_mut(request_id) {
            update(&mut request);
            let _ = self
                .event_tx
                .send(DebugEvent::NetworkRequest(request.clone()));
            true
        } else {
            false
        }
    }

    /// Get all sessions
    pub fn get_sessions(&self) -> Vec<DebugSession> {
        self.sessions.iter().map(|r| r.value().clone()).collect()
    }

    /// Get a specific session
    pub fn get_session(&self, session_id: &str) -> Option<DebugSession> {
        self.sessions.get(session_id).map(|r| r.clone())
    }

    /// Get console logs for a session
    pub fn get_console_logs(
        &self,
        session_id: &str,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> Vec<ConsoleLog> {
        self.console_logs
            .get(session_id)
            .map(|logs| {
                let offset = offset.unwrap_or(0);
                let limit = limit.unwrap_or(logs.len());
                logs.iter().skip(offset).take(limit).cloned().collect()
            })
            .unwrap_or_default()
    }

    /// Get all console logs across all sessions
    pub fn get_all_console_logs(&self, limit: Option<usize>) -> Vec<ConsoleLog> {
        let mut all_logs: Vec<ConsoleLog> = self
            .console_logs
            .iter()
            .flat_map(|r| r.value().clone())
            .collect();

        all_logs.sort_by_key(|l| l.timestamp);

        if let Some(limit) = limit {
            all_logs.into_iter().rev().take(limit).collect()
        } else {
            all_logs
        }
    }

    /// Get page errors for a session
    pub fn get_page_errors(&self, session_id: &str) -> Vec<PageError> {
        self.page_errors
            .get(session_id)
            .map(|errors| errors.clone())
            .unwrap_or_default()
    }

    /// Get network requests for a session
    pub fn get_network_requests(&self, session_id: &str) -> Vec<PageNetworkRequest> {
        self.network_requests
            .iter()
            .filter(|r| r.value().session_id == session_id)
            .map(|r| r.value().clone())
            .collect()
    }

    /// Clear console logs for a session
    pub fn clear_console_logs(&self, session_id: &str) {
        if let Some(mut logs) = self.console_logs.get_mut(session_id) {
            logs.clear();
        }
    }

    /// Clear all data for all sessions
    pub fn clear_all(&self) {
        self.console_logs
            .iter_mut()
            .for_each(|mut r| r.value_mut().clear());
        self.page_errors
            .iter_mut()
            .for_each(|mut r| r.value_mut().clear());
        self.network_requests.clear();
        self.log_counter.store(0, Ordering::Relaxed);
    }

    /// Get total log count
    pub fn get_total_log_count(&self) -> usize {
        self.log_counter.load(Ordering::Relaxed)
    }

    /// Subscribe to debug events
    pub fn subscribe(&self) -> broadcast::Receiver<DebugEvent> {
        self.event_tx.subscribe()
    }

    /// Background reaper: drop sessions that have been idle past their TTL.
    /// Disconnected sessions are evicted aggressively (5 min) since they
    /// represent closed DevTools tabs; still-connected sessions get 30 min
    /// before we assume the client is gone but never sent a close.
    fn cleanup_stale_sessions(&self) {
        if self.sessions.is_empty() {
            return;
        }

        let now = chrono::Utc::now().timestamp_millis();
        let disconnected_ttl = DISCONNECTED_SESSION_TTL.as_millis() as i64;
        let idle_ttl = IDLE_SESSION_TTL.as_millis() as i64;

        let mut stale: Vec<String> = Vec::new();
        for entry in self.sessions.iter() {
            let session = entry.value();
            let ttl = if session.is_connected {
                idle_ttl
            } else {
                disconnected_ttl
            };
            if now - session.last_activity > ttl {
                stale.push(entry.key().clone());
            }
        }

        for id in &stale {
            self.remove_session(id);
        }

        if !stale.is_empty() {
            tracing::info!(
                count = stale.len(),
                remaining = self.sessions.len(),
                "reaped stale debug sessions"
            );
        }

        if self.sessions.len() > MAX_SESSIONS {
            self.enforce_session_cap();
        }
    }

    /// Evict oldest-activity sessions (disconnected first) until we're back
    /// under the hard cap. Called from insert paths and from the reaper.
    fn enforce_session_cap(&self) {
        if self.sessions.len() <= MAX_SESSIONS {
            return;
        }
        let mut candidates: Vec<(String, bool, i64)> = self
            .sessions
            .iter()
            .map(|r| {
                (
                    r.key().clone(),
                    r.value().is_connected,
                    r.value().last_activity,
                )
            })
            .collect();

        // Sort: disconnected before connected, then oldest activity first.
        candidates.sort_by(|a, b| match (a.1, b.1) {
            (false, true) => std::cmp::Ordering::Less,
            (true, false) => std::cmp::Ordering::Greater,
            _ => a.2.cmp(&b.2),
        });

        let to_remove = self.sessions.len().saturating_sub(MAX_SESSIONS);
        for (id, _, _) in candidates.into_iter().take(to_remove) {
            self.remove_session(&id);
        }
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        let (event_tx, _) = broadcast::channel(1000);
        Self {
            sessions: DashMap::new(),
            console_logs: DashMap::new(),
            page_errors: DashMap::new(),
            network_requests: DashMap::new(),
            event_tx,
            log_counter: AtomicUsize::new(0),
            max_logs_per_session: 10000,
        }
    }
}
