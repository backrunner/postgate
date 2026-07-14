// WebSocket server for debug connections with HTTP /json/list endpoint

use super::session::SessionManager;
use super::types::*;
use dashmap::DashMap;
use futures_util::{SinkExt, StreamExt};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, mpsc, RwLock};
use tokio::task::JoinHandle;
use tokio_tungstenite::{accept_async, tungstenite::Message};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

const CDP_CHANNEL_CAPACITY: usize = 1024;
const CHOBITSU_JS: &str = include_str!("../../assets/chobitsu.js");

/// WebSocket server for debug connections from injected scripts
pub struct DebugServer {
    session_manager: Arc<SessionManager>,
    config: RwLock<DebugConfig>,
    running: AtomicBool,
    page_connections: DashMap<String, mpsc::UnboundedSender<ServerMessage>>,
    cdp_buses: DashMap<String, broadcast::Sender<serde_json::Value>>,
    shutdown_tx: broadcast::Sender<()>,
    accept_handle: RwLock<Option<JoinHandle<()>>>,
}

fn parse_request_path(request: &str) -> Option<&str> {
    let first_line = request.lines().next()?;
    let mut parts = first_line.split_whitespace();
    match (parts.next(), parts.next()) {
        (Some("GET"), Some(path)) => Some(path),
        _ => None,
    }
}

fn devtools_session_id(path: &str) -> Option<String> {
    route_path(path)
        .strip_prefix("/devtools/page/")
        .and_then(|id| id.split(['?', '#']).next())
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(ToOwned::to_owned)
}

fn route_path(path: &str) -> &str {
    path.split(['?', '#']).next().unwrap_or(path)
}

fn cdp_error_response(request: &serde_json::Value, message: &str) -> Option<serde_json::Value> {
    let id = request.get("id")?.clone();
    Some(serde_json::json!({
        "id": id,
        "error": {
            "code": -32000,
            "message": message
        }
    }))
}

impl DebugServer {
    pub fn new(session_manager: Arc<SessionManager>) -> Arc<Self> {
        let (shutdown_tx, _) = broadcast::channel(16);
        Arc::new(Self {
            session_manager,
            config: RwLock::new(DebugConfig::default()),
            running: AtomicBool::new(false),
            page_connections: DashMap::new(),
            cdp_buses: DashMap::new(),
            shutdown_tx,
            accept_handle: RwLock::new(None),
        })
    }

    /// Start the debug WebSocket server
    pub async fn start(self: &Arc<Self>, port: u16) -> Result<(), String> {
        if self.running.load(Ordering::Relaxed) {
            return Err("Debug server is already running".to_string());
        }

        let addr = format!("127.0.0.1:{}", port);
        let listener = TcpListener::bind(&addr)
            .await
            .map_err(|e| format!("Failed to bind debug server: {}", e))?;

        self.running.store(true, Ordering::Relaxed);
        {
            let mut config = self.config.write().await;
            config.enabled = true;
            config.port = port;
        }

        info!("Debug WebSocket server listening on ws://{}", addr);

        let server = Arc::clone(self);
        let mut shutdown_rx = self.shutdown_tx.subscribe();
        let accept_handle = tokio::spawn(async move {
            loop {
                tokio::select! {
                    incoming = listener.accept() => {
                        match incoming {
                            Ok((stream, addr)) => {
                                debug!("Debug connection from {}", addr);
                                let server = Arc::clone(&server);
                                tokio::spawn(async move {
                                    if let Err(e) = server.handle_incoming(stream).await {
                                        warn!("Debug connection error: {}", e);
                                    }
                                });
                            }
                            Err(e) => {
                                if server.running.load(Ordering::Relaxed) {
                                    error!("Failed to accept connection: {}", e);
                                }
                                break;
                            }
                        }
                    }
                    _ = shutdown_rx.recv() => {
                        break;
                    }
                }
            }

            server.running.store(false, Ordering::Relaxed);
        });
        *self.accept_handle.write().await = Some(accept_handle);

        Ok(())
    }

    /// Handle an incoming connection - detect if HTTP or WebSocket
    async fn handle_incoming(&self, stream: TcpStream) -> Result<(), String> {
        // Peek at the first few bytes to detect if it's an HTTP request or WebSocket upgrade
        let mut buf = [0u8; 1024];
        let n = stream.peek(&mut buf).await.map_err(|e| e.to_string())?;

        let request_line = String::from_utf8_lossy(&buf[..n]);

        let request_path = parse_request_path(&request_line);

        // Discovery and bundled debug assets share the same localhost server.
        if request_path
            .is_some_and(|path| path.starts_with("/json") || path.starts_with("/__postgate/"))
        {
            return self.handle_http_request(stream).await;
        }

        if let Some(session_id) = request_path.and_then(devtools_session_id) {
            return self.handle_devtools_connection(stream, session_id).await;
        }

        // Otherwise treat as a page-side injected-script WebSocket.
        self.handle_page_connection(stream).await
    }

    /// Handle HTTP requests for /json/list, /json/version etc.
    async fn handle_http_request(&self, mut stream: TcpStream) -> Result<(), String> {
        let mut buf = [0u8; 4096];
        let n = stream.read(&mut buf).await.map_err(|e| e.to_string())?;
        let request = String::from_utf8_lossy(&buf[..n]);

        // Parse the request line
        let first_line = request.lines().next().unwrap_or("");
        let path = first_line.split_whitespace().nth(1).unwrap_or("/");
        let path = route_path(path);

        let config = self.config.read().await;
        let port = config.port;
        drop(config);

        let (status, content_type, cache_control, body) = match path {
            "/json" | "/json/list" => {
                let sessions = self.session_manager.get_sessions();
                let targets: Vec<serde_json::Value> = sessions.iter()
                    .filter(|s| s.is_connected && s.cdp_enabled)
                    .map(|s| {
                        serde_json::json!({
                            "description": "",
                            "devtoolsFrontendUrl": format!("devtools://devtools/bundled/inspector.html?ws=127.0.0.1:{}/devtools/page/{}", port, s.id),
                            "id": s.id,
                            "title": s.title.clone().unwrap_or_else(|| "Untitled".to_string()),
                            "type": "page",
                            "url": s.url,
                            "webSocketDebuggerUrl": format!("ws://127.0.0.1:{}/devtools/page/{}", port, s.id)
                        })
                    })
                    .collect();
                (
                    "200 OK",
                    "application/json; charset=utf-8",
                    "no-store",
                    serde_json::to_string_pretty(&targets).unwrap(),
                )
            }
            "/json/version" => {
                let version = serde_json::json!({
                    "Browser": "PostGate/0.1.0",
                    "Protocol-Version": "1.3",
                    "User-Agent": "PostGate",
                    "V8-Version": "N/A",
                    "WebKit-Version": "N/A"
                });
                (
                    "200 OK",
                    "application/json; charset=utf-8",
                    "no-store",
                    serde_json::to_string_pretty(&version).unwrap(),
                )
            }
            "/__postgate/chobitsu.js" => (
                "200 OK",
                "text/javascript; charset=utf-8",
                "public, max-age=31536000, immutable",
                CHOBITSU_JS.to_string(),
            ),
            _ => (
                "404 Not Found",
                "text/plain; charset=utf-8",
                "no-store",
                "Not Found".to_string(),
            ),
        };

        let response = format!(
            "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: {}\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n{}",
            status,
            content_type,
            body.len(),
            cache_control,
            body
        );

        stream
            .write_all(response.as_bytes())
            .await
            .map_err(|e| e.to_string())?;

        Ok(())
    }

    /// Stop the debug server
    pub async fn stop(&self) {
        self.running.store(false, Ordering::Relaxed);
        let _ = self.shutdown_tx.send(());

        if let Some(handle) = self.accept_handle.write().await.take() {
            let _ = tokio::time::timeout(std::time::Duration::from_secs(1), handle).await;
        }

        self.page_connections.clear();
        self.cdp_buses.clear();

        let mut config = self.config.write().await;
        config.enabled = false;
        info!("Debug server stopped");
    }

    /// Check if server is running
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }

    /// Get current status
    pub async fn get_status(&self) -> DebugStatus {
        let config = self.config.read().await;
        DebugStatus {
            is_running: self.running.load(Ordering::Relaxed),
            port: config.port,
            session_count: self.session_manager.get_sessions().len(),
            total_logs: self.session_manager.get_total_log_count(),
        }
    }

    /// Get the port currently configured for debug injection.
    pub async fn port(&self) -> u16 {
        self.config.read().await.port
    }

    /// Handle a page-side WebSocket connection from the injected script.
    async fn handle_page_connection(&self, stream: TcpStream) -> Result<(), String> {
        let ws_stream = accept_async(stream)
            .await
            .map_err(|e| format!("WebSocket handshake failed: {}", e))?;

        let (mut ws_sender, mut ws_receiver) = ws_stream.split();
        let mut session_id: Option<String> = None;
        let (page_tx, mut page_rx) = mpsc::unbounded_channel::<ServerMessage>();
        let mut shutdown_rx = self.shutdown_tx.subscribe();

        loop {
            tokio::select! {
                _ = shutdown_rx.recv() => {
                    break;
                }
                msg = ws_receiver.next() => {
                    let Some(msg) = msg else { break };
                    let msg = match msg {
                        Ok(m) => m,
                        Err(e) => {
                            warn!("Page debug WebSocket error: {}", e);
                            break;
                        }
                    };

                    match msg {
                        Message::Text(text) => match serde_json::from_str::<ClientMessage>(&text) {
                            Ok(client_msg) => {
                                let response = self
                                    .handle_page_message(client_msg, &mut session_id, &page_tx)
                                    .await;
                                if let Some(resp) = response {
                                    if page_tx.send(resp).is_err() {
                                        break;
                                    }
                                }
                            }
                            Err(e) => {
                                warn!("Failed to parse page debug message: {} - {}", e, text);
                            }
                        },
                        Message::Binary(_) => {
                            // Binary messages not supported for injected-script transport.
                        }
                        Message::Ping(data) => {
                            let _ = ws_sender.send(Message::Pong(data)).await;
                        }
                        Message::Pong(_) => {}
                        Message::Close(_) => {
                            break;
                        }
                        Message::Frame(_) => {}
                    }
                }
                outbound = page_rx.recv() => {
                    let Some(outbound) = outbound else { break };
                    let json = serde_json::to_string(&outbound)
                        .map_err(|e| format!("Failed to serialize page message: {}", e))?;
                    if ws_sender.send(Message::Text(json.into())).await.is_err() {
                        break;
                    }
                }
            }
        }

        // Clean up session on disconnect
        if let Some(id) = session_id {
            self.page_connections.remove(&id);
            self.cdp_buses.remove(&id);
            self.session_manager.disconnect_session(&id);
            debug!("Debug page session {} disconnected", id);
        }

        Ok(())
    }

    /// Handle a Chrome DevTools WebSocket connection for `/devtools/page/{id}`.
    async fn handle_devtools_connection(
        &self,
        stream: TcpStream,
        session_id: String,
    ) -> Result<(), String> {
        let ws_stream = accept_async(stream)
            .await
            .map_err(|e| format!("DevTools WebSocket handshake failed: {}", e))?;

        let (mut ws_sender, mut ws_receiver) = ws_stream.split();
        let mut cdp_rx = self.cdp_bus_for_session(&session_id).subscribe();
        let mut shutdown_rx = self.shutdown_tx.subscribe();

        info!("DevTools connected to debug session {}", session_id);

        loop {
            tokio::select! {
                _ = shutdown_rx.recv() => {
                    break;
                }
                msg = ws_receiver.next() => {
                    let Some(msg) = msg else { break };
                    let msg = match msg {
                        Ok(m) => m,
                        Err(e) => {
                            warn!("DevTools WebSocket error: {}", e);
                            break;
                        }
                    };

                    match msg {
                        Message::Text(text) => {
                            self.forward_cdp_to_page(&session_id, text.as_str(), &mut ws_sender).await?;
                        }
                        Message::Binary(bytes) => {
                            if let Ok(text) = String::from_utf8(bytes.to_vec()) {
                                self.forward_cdp_to_page(&session_id, &text, &mut ws_sender).await?;
                            }
                        }
                        Message::Ping(data) => {
                            let _ = ws_sender.send(Message::Pong(data)).await;
                        }
                        Message::Pong(_) => {}
                        Message::Close(_) => break,
                        Message::Frame(_) => {}
                    }
                }
                cdp_message = cdp_rx.recv() => {
                    match cdp_message {
                        Ok(message) => {
                            let json = serde_json::to_string(&message)
                                .map_err(|e| format!("Failed to serialize CDP message: {}", e))?;
                            if ws_sender.send(Message::Text(json.into())).await.is_err() {
                                break;
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(skipped)) => {
                            warn!(
                                session_id,
                                skipped,
                                "DevTools CDP receiver lagged behind"
                            );
                        }
                        Err(broadcast::error::RecvError::Closed) => break,
                    }
                }
            }
        }

        debug!("DevTools disconnected from debug session {}", session_id);

        Ok(())
    }

    async fn forward_cdp_to_page(
        &self,
        session_id: &str,
        raw_message: &str,
        ws_sender: &mut futures_util::stream::SplitSink<
            tokio_tungstenite::WebSocketStream<TcpStream>,
            Message,
        >,
    ) -> Result<(), String> {
        let parsed = match serde_json::from_str::<serde_json::Value>(raw_message) {
            Ok(value) => value,
            Err(e) => {
                warn!("Invalid DevTools CDP JSON for {}: {}", session_id, e);
                return Ok(());
            }
        };

        let page_tx = self.page_connections.get(session_id).map(|tx| tx.clone());
        if let Some(page_tx) = page_tx {
            page_tx
                .send(ServerMessage::Cdp { message: parsed })
                .map_err(|_| format!("Debug page session {} is disconnected", session_id))?;
            self.session_manager.update_activity(session_id);
        } else if let Some(error) =
            cdp_error_response(&parsed, "PostGate page session is not connected")
        {
            let json = serde_json::to_string(&error)
                .map_err(|e| format!("Failed to serialize CDP error: {}", e))?;
            let _ = ws_sender.send(Message::Text(json.into())).await;
        }

        Ok(())
    }

    fn cdp_bus_for_session(&self, session_id: &str) -> broadcast::Sender<serde_json::Value> {
        if let Some(tx) = self.cdp_buses.get(session_id) {
            return tx.clone();
        }

        let (tx, _) = broadcast::channel(CDP_CHANNEL_CAPACITY);
        self.cdp_buses.insert(session_id.to_string(), tx.clone());
        tx
    }

    fn publish_cdp_message(&self, session_id: &str, message: serde_json::Value) {
        let tx = self.cdp_bus_for_session(session_id);
        let _ = tx.send(message);
        self.session_manager.update_activity(session_id);
    }

    /// Handle a message from the injected page script.
    async fn handle_page_message(
        &self,
        msg: ClientMessage,
        session_id: &mut Option<String>,
        page_tx: &mpsc::UnboundedSender<ServerMessage>,
    ) -> Option<ServerMessage> {
        let config = self.config.read().await;
        let port = config.port;
        drop(config);

        match msg {
            ClientMessage::GetChobitsu => Some(ServerMessage::Chobitsu {
                source: CHOBITSU_JS.to_string(),
            }),
            ClientMessage::Hello {
                url,
                title,
                user_agent,
                cdp_enabled,
            } => {
                let session = self.session_manager.create_session(
                    url,
                    title,
                    user_agent,
                    cdp_enabled.unwrap_or(false),
                    port,
                );
                *session_id = Some(session.id.clone());
                self.page_connections
                    .insert(session.id.clone(), page_tx.clone());
                self.cdp_bus_for_session(&session.id);
                info!(
                    "Debug session started: {} (CDP: {})",
                    session.id, session.cdp_enabled
                );
                Some(ServerMessage::Welcome {
                    session_id: session.id,
                })
            }

            ClientMessage::Console {
                level,
                args,
                stack,
                source_url,
                line,
                column,
            } => {
                if let Some(sid) = session_id {
                    let log = ConsoleLog {
                        id: Uuid::new_v4().to_string(),
                        session_id: sid.clone(),
                        level: ConsoleLevel::from(level.as_str()),
                        args: args.into_iter().map(parse_console_arg).collect(),
                        timestamp: chrono::Utc::now().timestamp_millis(),
                        stack_trace: stack,
                        source_url,
                        line_number: line,
                        column_number: column,
                    };
                    self.session_manager.add_console_log(sid, log);
                }
                None
            }

            ClientMessage::Error {
                error_type,
                message,
                stack,
                source_url,
                line,
                column,
            } => {
                if let Some(sid) = session_id {
                    let error = PageError {
                        id: Uuid::new_v4().to_string(),
                        session_id: sid.clone(),
                        error_type: parse_error_type(&error_type),
                        message,
                        stack,
                        source_url,
                        line_number: line,
                        column_number: column,
                        timestamp: chrono::Utc::now().timestamp_millis(),
                    };
                    self.session_manager.add_page_error(sid, error);
                }
                None
            }

            ClientMessage::Network {
                id,
                phase,
                method,
                url,
                request_headers,
                request_body,
                status,
                response_headers,
                duration_ms,
                initiator,
            } => {
                if let Some(sid) = session_id {
                    if phase == "start" {
                        let request = PageNetworkRequest {
                            id: id.clone(),
                            session_id: sid.clone(),
                            method: method.unwrap_or_default(),
                            url: url.unwrap_or_default(),
                            request_headers: request_headers.unwrap_or_default(),
                            request_body,
                            status: None,
                            response_headers: None,
                            response_body: None,
                            duration_ms: None,
                            timestamp: chrono::Utc::now().timestamp_millis(),
                            initiator,
                        };
                        self.session_manager.add_network_request(request);
                    } else if phase == "end" {
                        let updated = self.session_manager.update_network_request(&id, |req| {
                            req.status = status;
                            req.response_headers = response_headers.clone();
                            req.duration_ms = duration_ms;
                        });
                        if !updated {
                            let request = PageNetworkRequest {
                                id,
                                session_id: sid.clone(),
                                method: method.unwrap_or_default(),
                                url: url.unwrap_or_default(),
                                request_headers: request_headers.unwrap_or_default(),
                                request_body,
                                status,
                                response_headers,
                                response_body: None,
                                duration_ms,
                                timestamp: chrono::Utc::now().timestamp_millis(),
                                initiator,
                            };
                            self.session_manager.add_network_request(request);
                        }
                    }
                }
                None
            }

            ClientMessage::Cdp { message } => {
                if let Some(sid) = session_id {
                    self.publish_cdp_message(sid, message);
                }
                None
            }

            ClientMessage::Ping => Some(ServerMessage::Pong),
        }
    }

    /// Get session manager
    pub fn session_manager(&self) -> &Arc<SessionManager> {
        &self.session_manager
    }
}

/// Parse a JSON value into a ConsoleArg
fn parse_console_arg(value: serde_json::Value) -> ConsoleArg {
    match value {
        serde_json::Value::Null => ConsoleArg::Null,
        serde_json::Value::Bool(b) => ConsoleArg::Boolean(b),
        serde_json::Value::Number(n) => ConsoleArg::Number(n.as_f64().unwrap_or(0.0)),
        serde_json::Value::String(s) => {
            if s == "__undefined__" {
                ConsoleArg::Undefined
            } else if s.starts_with("__function:") {
                ConsoleArg::Function(s.strip_prefix("__function:").unwrap_or(&s).to_string())
            } else if s.starts_with("__symbol:") {
                ConsoleArg::Symbol(s.strip_prefix("__symbol:").unwrap_or(&s).to_string())
            } else if s == "__circular__" {
                ConsoleArg::Circular
            } else {
                ConsoleArg::String(s)
            }
        }
        serde_json::Value::Array(arr) => {
            ConsoleArg::Array(arr.into_iter().map(parse_console_arg).collect())
        }
        serde_json::Value::Object(obj) => {
            // Check for special types
            if let Some(serde_json::Value::String(t)) = obj.get("__type__") {
                match t.as_str() {
                    "error" => {
                        return ConsoleArg::Error {
                            name: obj
                                .get("name")
                                .and_then(|v| v.as_str())
                                .unwrap_or("Error")
                                .to_string(),
                            message: obj
                                .get("message")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string(),
                            stack: obj
                                .get("stack")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string()),
                        };
                    }
                    "element" => {
                        return ConsoleArg::Element {
                            tag: obj
                                .get("tag")
                                .and_then(|v| v.as_str())
                                .unwrap_or("unknown")
                                .to_string(),
                            id: obj
                                .get("id")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string()),
                            classes: obj
                                .get("classes")
                                .and_then(|v| v.as_array())
                                .map(|arr| {
                                    arr.iter()
                                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                        .collect()
                                })
                                .unwrap_or_default(),
                        };
                    }
                    _ => {}
                }
            }
            ConsoleArg::Object(serde_json::Value::Object(obj))
        }
    }
}

/// Parse error type string
fn parse_error_type(s: &str) -> ErrorType {
    match s.to_lowercase().as_str() {
        "syntaxerror" | "syntax" => ErrorType::Syntax,
        "referenceerror" | "reference" => ErrorType::Reference,
        "typeerror" | "type" => ErrorType::Type,
        "rangeerror" | "range" => ErrorType::Range,
        "urierror" | "uri" => ErrorType::Uri,
        "networkerror" | "network" => ErrorType::Network,
        "unhandledrejection" | "promise" => ErrorType::Promise,
        "error" | "runtime" => ErrorType::Runtime,
        _ => ErrorType::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::{SinkExt, StreamExt};
    use std::collections::HashMap;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use tokio_tungstenite::connect_async;

    async fn free_local_port() -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        listener.local_addr().unwrap().port()
    }

    #[test]
    fn test_parse_request_path_for_devtools_target() {
        let request = "GET /devtools/page/session-1?foo=bar HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n";

        assert_eq!(
            parse_request_path(request),
            Some("/devtools/page/session-1?foo=bar")
        );
        assert_eq!(
            devtools_session_id(parse_request_path(request).unwrap()).as_deref(),
            Some("session-1")
        );
    }

    #[test]
    fn test_parse_request_path_rejects_non_get() {
        let request = "POST /devtools/page/session-1 HTTP/1.1\r\n\r\n";

        assert_eq!(parse_request_path(request), None);
    }

    #[test]
    fn test_cdp_error_response_preserves_request_id() {
        let request = serde_json::json!({
            "id": 7,
            "method": "Runtime.evaluate",
            "params": { "expression": "1 + 1" }
        });

        let error = cdp_error_response(&request, "not connected").unwrap();

        assert_eq!(error["id"], 7);
        assert_eq!(error["error"]["code"], -32000);
        assert_eq!(error["error"]["message"], "not connected");
    }

    #[tokio::test]
    async fn test_debug_server_stop_releases_port_for_restart() {
        let port = free_local_port().await;
        let manager = SessionManager::new();
        let server = DebugServer::new(manager);

        server.start(port).await.unwrap();
        assert!(server.is_running());

        server.stop().await;
        assert!(!server.is_running());

        server.start(port).await.unwrap();
        assert!(server.is_running());
        server.stop().await;
    }

    #[tokio::test]
    async fn test_debug_server_serves_bundled_chobitsu() {
        let port = free_local_port().await;
        let manager = SessionManager::new();
        let server = DebugServer::new(manager);
        server.start(port).await.unwrap();

        let mut stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        stream
            .write_all(
                b"GET /__postgate/chobitsu.js HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
            )
            .await
            .unwrap();
        let mut response = Vec::new();
        stream.read_to_end(&mut response).await.unwrap();
        let response = String::from_utf8(response).unwrap();

        assert!(response.starts_with("HTTP/1.1 200 OK"));
        assert!(response.contains("Content-Type: text/javascript; charset=utf-8"));
        assert!(response.contains("setOnMessage"));
        assert!(response.contains("sendRawMessage"));

        server.stop().await;
    }

    #[tokio::test]
    async fn test_cdp_round_trip_between_page_and_devtools() {
        let port = free_local_port().await;
        let manager = SessionManager::new();
        let server = DebugServer::new(Arc::clone(&manager));
        server.start(port).await.unwrap();

        let (mut page, _) = connect_async(format!("ws://127.0.0.1:{port}/"))
            .await
            .unwrap();
        page.send(Message::Text(
            serde_json::to_string(&ClientMessage::GetChobitsu)
                .unwrap()
                .into(),
        ))
        .await
        .unwrap();

        let bootstrap = tokio::time::timeout(std::time::Duration::from_secs(1), page.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let Message::Text(bootstrap) = bootstrap else {
            panic!("expected Chobitsu bootstrap text message");
        };
        match serde_json::from_str::<ServerMessage>(&bootstrap).unwrap() {
            ServerMessage::Chobitsu { source } => {
                assert!(source.len() > 400_000);
                assert!(source.contains("setOnMessage"));
            }
            other => panic!("expected Chobitsu bootstrap, got {other:?}"),
        }

        page.send(Message::Text(
            serde_json::to_string(&ClientMessage::Hello {
                url: "https://example.com/app".into(),
                title: Some("Example".into()),
                user_agent: Some("PostGate test".into()),
                cdp_enabled: Some(true),
            })
            .unwrap()
            .into(),
        ))
        .await
        .unwrap();

        let welcome = tokio::time::timeout(std::time::Duration::from_secs(1), page.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let Message::Text(welcome) = welcome else {
            panic!("expected welcome text message");
        };
        let session_id = match serde_json::from_str::<ServerMessage>(&welcome).unwrap() {
            ServerMessage::Welcome { session_id } => session_id,
            other => panic!("expected welcome, got {other:?}"),
        };

        let mut discovery = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        discovery
            .write_all(b"GET /json/list HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
            .await
            .unwrap();
        let mut discovery_response = Vec::new();
        discovery
            .read_to_end(&mut discovery_response)
            .await
            .unwrap();
        let discovery_response = String::from_utf8(discovery_response).unwrap();
        assert!(discovery_response.contains(&session_id));
        assert!(discovery_response
            .contains(&format!("ws://127.0.0.1:{port}/devtools/page/{session_id}")));

        let (mut devtools, _) =
            connect_async(format!("ws://127.0.0.1:{port}/devtools/page/{session_id}"))
                .await
                .unwrap();

        let command = serde_json::json!({
            "id": 41,
            "method": "Runtime.evaluate",
            "params": { "expression": "6 * 7" }
        });
        devtools
            .send(Message::Text(command.to_string().into()))
            .await
            .unwrap();

        let forwarded = tokio::time::timeout(std::time::Duration::from_secs(1), page.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let Message::Text(forwarded) = forwarded else {
            panic!("expected forwarded CDP text message");
        };
        match serde_json::from_str::<ServerMessage>(&forwarded).unwrap() {
            ServerMessage::Cdp { message } => assert_eq!(message, command),
            other => panic!("expected CDP command, got {other:?}"),
        }

        let result = serde_json::json!({
            "id": 41,
            "result": { "result": { "type": "number", "value": 42 } }
        });
        page.send(Message::Text(
            serde_json::to_string(&ClientMessage::Cdp {
                message: result.clone(),
            })
            .unwrap()
            .into(),
        ))
        .await
        .unwrap();

        let returned = tokio::time::timeout(std::time::Duration::from_secs(1), devtools.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let Message::Text(returned) = returned else {
            panic!("expected returned CDP text message");
        };
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&returned).unwrap(),
            result
        );

        devtools.close(None).await.unwrap();
        page.close(None).await.unwrap();
        server.stop().await;
    }

    #[tokio::test]
    async fn test_network_end_without_start_is_recorded() {
        let manager = SessionManager::new();
        let server = DebugServer::new(Arc::clone(&manager));
        let mut session_id = None;
        let (page_tx, _page_rx) = mpsc::unbounded_channel();

        server
            .handle_page_message(
                ClientMessage::Hello {
                    url: "https://example.com".into(),
                    title: Some("Example".into()),
                    user_agent: None,
                    cdp_enabled: Some(false),
                },
                &mut session_id,
                &page_tx,
            )
            .await;

        let sid = session_id.unwrap();
        server
            .handle_page_message(
                ClientMessage::Network {
                    id: "req-1".into(),
                    phase: "end".into(),
                    method: Some("GET".into()),
                    url: Some("https://example.com/api".into()),
                    request_headers: Some(HashMap::new()),
                    request_body: None,
                    status: Some(200),
                    response_headers: Some(HashMap::new()),
                    duration_ms: Some(42),
                    initiator: Some("fetch".into()),
                },
                &mut Some(sid.clone()),
                &page_tx,
            )
            .await;

        let requests = manager.get_network_requests(&sid);
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].id, "req-1");
        assert_eq!(requests[0].status, Some(200));
        assert_eq!(requests[0].duration_ms, Some(42));
    }
}
