// WebSocket server for debug connections with HTTP /json/list endpoint

use super::types::*;
use super::session::SessionManager;
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::RwLock;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_tungstenite::{accept_async, tungstenite::Message};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// WebSocket server for debug connections from injected scripts
pub struct DebugServer {
    session_manager: Arc<SessionManager>,
    config: RwLock<DebugConfig>,
    running: AtomicBool,
}

impl DebugServer {
    pub fn new(session_manager: Arc<SessionManager>) -> Arc<Self> {
        Arc::new(Self {
            session_manager,
            config: RwLock::new(DebugConfig::default()),
            running: AtomicBool::new(false),
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
        tokio::spawn(async move {
            while server.running.load(Ordering::Relaxed) {
                match listener.accept().await {
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
        });

        Ok(())
    }

    /// Handle an incoming connection - detect if HTTP or WebSocket
    async fn handle_incoming(&self, stream: TcpStream) -> Result<(), String> {
        // Peek at the first few bytes to detect if it's an HTTP request or WebSocket upgrade
        let mut buf = [0u8; 1024];
        let n = stream.peek(&mut buf).await.map_err(|e| e.to_string())?;
        
        let request_line = String::from_utf8_lossy(&buf[..n]);
        
        // Check if it's an HTTP GET request for /json endpoints
        if request_line.starts_with("GET /json") {
            return self.handle_http_request(stream).await;
        }

        // Otherwise treat as WebSocket connection
        self.handle_connection(stream).await
    }

    /// Handle HTTP requests for /json/list, /json/version etc.
    async fn handle_http_request(&self, mut stream: TcpStream) -> Result<(), String> {
        let mut buf = [0u8; 4096];
        let n = stream.read(&mut buf).await.map_err(|e| e.to_string())?;
        let request = String::from_utf8_lossy(&buf[..n]);
        
        // Parse the request line
        let first_line = request.lines().next().unwrap_or("");
        let path = first_line.split_whitespace().nth(1).unwrap_or("/");
        
        let config = self.config.read().await;
        let port = config.port;
        drop(config);

        let (status, body) = match path {
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
                ("200 OK", serde_json::to_string_pretty(&targets).unwrap())
            }
            "/json/version" => {
                let version = serde_json::json!({
                    "Browser": "PostGate/0.1.0",
                    "Protocol-Version": "1.3",
                    "User-Agent": "PostGate",
                    "V8-Version": "N/A",
                    "WebKit-Version": "N/A",
                    "webSocketDebuggerUrl": format!("ws://127.0.0.1:{}", port)
                });
                ("200 OK", serde_json::to_string_pretty(&version).unwrap())
            }
            _ => {
                ("404 Not Found", "Not Found".to_string())
            }
        };

        let response = format!(
            "HTTP/1.1 {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n{}",
            status,
            body.len(),
            body
        );

        stream.write_all(response.as_bytes()).await.map_err(|e| e.to_string())?;
        
        Ok(())
    }

    /// Stop the debug server
    pub async fn stop(&self) {
        self.running.store(false, Ordering::Relaxed);
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

    /// Handle a single WebSocket connection
    async fn handle_connection(&self, stream: TcpStream) -> Result<(), String> {
        let ws_stream = accept_async(stream)
            .await
            .map_err(|e| format!("WebSocket handshake failed: {}", e))?;

        let (mut ws_sender, mut ws_receiver) = ws_stream.split();
        let mut session_id: Option<String> = None;

        while let Some(msg) = ws_receiver.next().await {
            let msg = match msg {
                Ok(m) => m,
                Err(e) => {
                    warn!("WebSocket error: {}", e);
                    break;
                }
            };

            match msg {
                Message::Text(text) => {
                    match serde_json::from_str::<ClientMessage>(&text) {
                        Ok(client_msg) => {
                            let response = self.handle_message(client_msg, &mut session_id).await;
                            if let Some(resp) = response {
                                let json = serde_json::to_string(&resp).unwrap();
                                if ws_sender.send(Message::Text(json.into())).await.is_err() {
                                    break;
                                }
                            }
                        }
                        Err(e) => {
                            warn!("Failed to parse client message: {} - {}", e, text);
                        }
                    }
                }
                Message::Binary(_) => {
                    // Binary messages not supported yet
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

        // Clean up session on disconnect
        if let Some(id) = session_id {
            self.session_manager.disconnect_session(&id);
            debug!("Debug session {} disconnected", id);
        }

        Ok(())
    }

    /// Handle a client message
    async fn handle_message(
        &self,
        msg: ClientMessage,
        session_id: &mut Option<String>,
    ) -> Option<ServerMessage> {
        let config = self.config.read().await;
        let port = config.port;
        drop(config);

        match msg {
            ClientMessage::Hello { url, title, user_agent, cdp_enabled } => {
                let session = self.session_manager.create_session(
                    url, 
                    title, 
                    user_agent, 
                    cdp_enabled.unwrap_or(false),
                    port
                );
                *session_id = Some(session.id.clone());
                info!("Debug session started: {} (CDP: {})", session.id, session.cdp_enabled);
                Some(ServerMessage::Welcome {
                    session_id: session.id,
                })
            }

            ClientMessage::Console { level, args, stack, source_url, line, column } => {
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

            ClientMessage::Error { error_type, message, stack, source_url, line, column } => {
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

            ClientMessage::Network { id, phase, method, url, request_headers, request_body, status, response_headers, duration_ms, initiator } => {
                if let Some(sid) = session_id {
                    if phase == "start" {
                        let request = PageNetworkRequest {
                            id,
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
                        self.session_manager.update_network_request(&id, |req| {
                            req.status = status;
                            req.response_headers = response_headers.clone();
                            req.duration_ms = duration_ms;
                        });
                    }
                }
                None
            }

            ClientMessage::Cdp { message: _ } => {
                // CDP messages for DevTools integration
                // TODO: Forward to DevTools frontend
                None
            }

            ClientMessage::Ping => {
                Some(ServerMessage::Pong)
            }
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
        serde_json::Value::Number(n) => {
            ConsoleArg::Number(n.as_f64().unwrap_or(0.0))
        }
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
                            name: obj.get("name").and_then(|v| v.as_str()).unwrap_or("Error").to_string(),
                            message: obj.get("message").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                            stack: obj.get("stack").and_then(|v| v.as_str()).map(|s| s.to_string()),
                        };
                    }
                    "element" => {
                        return ConsoleArg::Element {
                            tag: obj.get("tag").and_then(|v| v.as_str()).unwrap_or("unknown").to_string(),
                            id: obj.get("id").and_then(|v| v.as_str()).map(|s| s.to_string()),
                            classes: obj.get("classes")
                                .and_then(|v| v.as_array())
                                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
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
