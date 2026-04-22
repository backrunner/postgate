// Debug session manager

use super::types::*;
use dashmap::DashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
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
        Arc::new(Self {
            sessions: DashMap::new(),
            console_logs: DashMap::new(),
            page_errors: DashMap::new(),
            network_requests: DashMap::new(),
            event_tx,
            log_counter: AtomicUsize::new(0),
            max_logs_per_session: 10000,
        })
    }

    /// Create a new debug session
    pub fn create_session(&self, url: String, title: Option<String>, user_agent: Option<String>, cdp_enabled: bool, port: u16) -> DebugSession {
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

        let _ = self.event_tx.send(DebugEvent::SessionConnected(session.clone()));

        session
    }

    /// Mark a session as disconnected
    pub fn disconnect_session(&self, session_id: &str) {
        if let Some(mut session) = self.sessions.get_mut(session_id) {
            session.is_connected = false;
            session.last_activity = chrono::Utc::now().timestamp_millis();
        }
        let _ = self.event_tx.send(DebugEvent::SessionDisconnected(session_id.to_string()));
    }

    /// Remove a session completely
    pub fn remove_session(&self, session_id: &str) {
        self.sessions.remove(session_id);
        self.console_logs.remove(session_id);
        self.page_errors.remove(session_id);
        // Remove network requests for this session
        self.network_requests.retain(|_, v| v.session_id != session_id);
    }

    /// Update session activity timestamp
    pub fn update_activity(&self, session_id: &str) {
        if let Some(mut session) = self.sessions.get_mut(session_id) {
            session.last_activity = chrono::Utc::now().timestamp_millis();
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
        
        let _ = self.event_tx.send(DebugEvent::NetworkRequest(request.clone()));
        self.network_requests.insert(id, request);
    }

    /// Update an existing network request (e.g., when response arrives)
    pub fn update_network_request(&self, request_id: &str, update: impl FnOnce(&mut PageNetworkRequest)) {
        if let Some(mut request) = self.network_requests.get_mut(request_id) {
            update(&mut request);
            let _ = self.event_tx.send(DebugEvent::NetworkRequest(request.clone()));
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
    pub fn get_console_logs(&self, session_id: &str, limit: Option<usize>, offset: Option<usize>) -> Vec<ConsoleLog> {
        self.console_logs
            .get(session_id)
            .map(|logs| {
                let offset = offset.unwrap_or(0);
                let limit = limit.unwrap_or(logs.len());
                logs.iter()
                    .skip(offset)
                    .take(limit)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get all console logs across all sessions
    pub fn get_all_console_logs(&self, limit: Option<usize>) -> Vec<ConsoleLog> {
        let mut all_logs: Vec<ConsoleLog> = self.console_logs
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
        self.console_logs.iter_mut().for_each(|mut r| r.value_mut().clear());
        self.page_errors.iter_mut().for_each(|mut r| r.value_mut().clear());
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
