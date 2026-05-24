//! WebSocket proxy support with frame capture
//!
//! This module handles WebSocket connections, proxying frames bidirectionally
//! while capturing each frame for the capture panel.

use crate::state::{
    AppState, CapturedRequestData, CapturedRequestEvent, RequestEventType, StreamDirection,
    StreamEndedEvent, StreamMessage, StreamMessageEvent, StreamMessageType,
};
use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use http_body_util::{combinators::BoxBody, BodyExt, Empty};
use hyper::body::Incoming;
use hyper::header::{HeaderMap, HeaderName};
use hyper::{Request, Response, StatusCode};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{
        client::IntoClientRequest,
        handshake::server::create_response_with_body,
        protocol::{Message, Role},
        Error as WsError,
    },
    MaybeTlsStream, WebSocketStream,
};

/// Check if a request is a WebSocket upgrade request
pub fn is_websocket_upgrade(headers: &HashMap<String, String>) -> bool {
    let upgrade = headers
        .get("upgrade")
        .map(|v| v.to_lowercase() == "websocket")
        .unwrap_or(false);

    let connection = headers
        .get("connection")
        .map(|v| v.to_lowercase().contains("upgrade"))
        .unwrap_or(false);

    let has_key = headers.contains_key("sec-websocket-key");

    upgrade && connection && has_key
}

/// Build WebSocket URL from request info
pub fn build_ws_url(host: &str, port: u16, path: &str, secure: bool) -> String {
    let scheme = if secure { "wss" } else { "ws" };
    let formatted_host = if host.contains(':') && !host.starts_with('[') {
        format!("[{}]", host)
    } else {
        host.to_string()
    };
    let port_str = if (secure && port == 443) || (!secure && port == 80) {
        String::new()
    } else {
        format!(":{}", port)
    };
    format!("{}://{}{}{}", scheme, formatted_host, port_str, path)
}

/// Metadata needed to represent a WebSocket handshake/stream in the capture UI.
#[derive(Clone)]
pub struct WebSocketCaptureMeta {
    pub request_id: String,
    pub timestamp: i64,
    pub method: String,
    pub url: String,
    pub host: String,
    pub path: String,
    pub request_headers: HashMap<String, String>,
    pub matched_rules: Vec<String>,
    pub tls_version: Option<String>,
    pub remote_addr: Option<String>,
    pub capture: bool,
}

/// WebSocket proxy handler
pub struct WebSocketProxy {
    connection_id: String,
    app_state: Arc<AppState>,
    capture_meta: Option<WebSocketCaptureMeta>,
    capture: bool,
    message_count: Arc<AtomicU64>,
    total_bytes: Arc<AtomicU64>,
    start_time: std::time::Instant,
}

impl WebSocketProxy {
    pub fn new(connection_id: String, app_state: Arc<AppState>) -> Self {
        Self {
            connection_id,
            app_state,
            capture_meta: None,
            capture: true,
            message_count: Arc::new(AtomicU64::new(0)),
            total_bytes: Arc::new(AtomicU64::new(0)),
            start_time: std::time::Instant::now(),
        }
    }

    pub fn with_capture_meta(mut self, meta: WebSocketCaptureMeta) -> Self {
        self.capture = meta.capture;
        self.capture_meta = Some(meta);
        self
    }

    /// Proxy a WebSocket connection bidirectionally
    pub async fn proxy<S>(self, client_stream: S, target_url: &str) -> crate::error::Result<()>
    where
        S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        // Accept WebSocket from client
        let client_ws = tokio_tungstenite::accept_async(client_stream)
            .await
            .map_err(|e| crate::error::PostGateError::Proxy(format!("WS accept error: {}", e)))?;

        // Connect to upstream WebSocket server
        let (server_ws, _response) = connect_async(target_url)
            .await
            .map_err(|e| crate::error::PostGateError::Proxy(format!("WS connect error: {}", e)))?;

        tracing::debug!("WebSocket connection established: {}", target_url);

        self.proxy_streams(client_ws, server_ws).await
    }

    /// Proxy a WebSocket after hyper has already sent the client handshake.
    pub async fn proxy_after_handshake<S>(
        self,
        client_stream: S,
        server_ws: WebSocketStream<MaybeTlsStream<TcpStream>>,
    ) -> crate::error::Result<()>
    where
        S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        let client_ws = WebSocketStream::from_raw_socket(client_stream, Role::Server, None).await;
        self.proxy_streams(client_ws, server_ws).await
    }

    async fn proxy_streams<C, S>(
        self,
        client_ws: WebSocketStream<C>,
        server_ws: WebSocketStream<S>,
    ) -> crate::error::Result<()>
    where
        C: AsyncRead + AsyncWrite + Unpin + Send + 'static,
        S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        // Split both connections for bidirectional proxying
        let (client_write, client_read) = client_ws.split();
        let (server_write, server_read) = server_ws.split();

        let conn_id_1 = self.connection_id.clone();
        let conn_id_2 = self.connection_id.clone();
        let app_state_1 = self.app_state.clone();
        let app_state_2 = self.app_state.clone();
        let msg_count_1 = self.message_count.clone();
        let msg_count_2 = self.message_count.clone();
        let bytes_1 = self.total_bytes.clone();
        let bytes_2 = self.total_bytes.clone();
        let capture = self.capture;

        // Client -> Server task
        let client_to_server = tokio::spawn(async move {
            Self::forward_frames(
                client_read,
                server_write,
                &conn_id_1,
                StreamDirection::Outbound, // Client sending to server
                app_state_1,
                msg_count_1,
                bytes_1,
                capture,
            )
            .await
        });

        let capture = self.capture;

        // Server -> Client task
        let server_to_client = tokio::spawn(async move {
            Self::forward_frames(
                server_read,
                client_write,
                &conn_id_2,
                StreamDirection::Inbound, // Server sending to client
                app_state_2,
                msg_count_2,
                bytes_2,
                capture,
            )
            .await
        });

        // Wait for either direction to close
        let close_reason = tokio::select! {
            result = client_to_server => {
                match result {
                    Ok(Ok(reason)) => reason,
                    Ok(Err(e)) => Some(format!("Client error: {}", e)),
                    Err(e) => Some(format!("Task error: {}", e)),
                }
            }
            result = server_to_client => {
                match result {
                    Ok(Ok(reason)) => reason,
                    Ok(Err(e)) => Some(format!("Server error: {}", e)),
                    Err(e) => Some(format!("Task error: {}", e)),
                }
            }
        };

        let message_count = self.message_count.load(Ordering::Relaxed);
        let total_bytes = self.total_bytes.load(Ordering::Relaxed);
        let duration_ms = self.start_time.elapsed().as_millis() as u64;

        if self.capture {
            self.app_state.emit_stream_ended(&StreamEndedEvent {
                connection_id: self.connection_id.clone(),
                message_count,
                total_bytes,
                duration_ms,
                close_reason,
            });

            if let Some(meta) = self.capture_meta {
                self.app_state.emit_request_event(&CapturedRequestEvent {
                    id: meta.request_id.clone(),
                    event_type: RequestEventType::Completed,
                    data: CapturedRequestData {
                        id: meta.request_id,
                        timestamp: meta.timestamp,
                        method: meta.method,
                        url: meta.url,
                        host: meta.host,
                        path: meta.path,
                        request_headers: Some(meta.request_headers),
                        response_status: Some(StatusCode::SWITCHING_PROTOCOLS.as_u16()),
                        duration_ms: Some(duration_ms),
                        matched_rules: meta.matched_rules,
                        protocol: "websocket".to_string(),
                        request_size: 0,
                        response_size: Some(total_bytes),
                        tls_version: meta.tls_version,
                        remote_addr: meta.remote_addr,
                        ..Default::default()
                    },
                });
            }
        }

        Ok(())
    }

    /// Forward WebSocket frames between connections with capture
    async fn forward_frames<R, W>(
        mut read: R,
        mut write: W,
        connection_id: &str,
        direction: StreamDirection,
        app_state: Arc<AppState>,
        message_count: Arc<AtomicU64>,
        total_bytes: Arc<AtomicU64>,
        capture: bool,
    ) -> Result<Option<String>, String>
    where
        R: StreamExt<Item = Result<Message, WsError>> + Unpin,
        W: SinkExt<Message, Error = WsError> + Unpin,
    {
        let mut close_reason = None;

        while let Some(message_result) = read.next().await {
            let msg = match message_result {
                Ok(m) => m,
                Err(e) => {
                    tracing::debug!("WS read error: {}", e);
                    return Err(e.to_string());
                }
            };

            if !capture {
                if let Err(e) = write.send(msg).await {
                    tracing::debug!("WS write error: {}", e);
                    return Err(e.to_string());
                }
                continue;
            }

            // Capture the frame
            let (msg_type, data, is_base64, size) = match &msg {
                Message::Text(text) => (
                    StreamMessageType::WsText,
                    text.to_string(),
                    false,
                    text.len(),
                ),
                Message::Binary(data) => (
                    StreamMessageType::WsBinary,
                    base64::Engine::encode(&base64::engine::general_purpose::STANDARD, data),
                    true,
                    data.len(),
                ),
                Message::Ping(data) => (
                    StreamMessageType::WsPing,
                    base64::Engine::encode(&base64::engine::general_purpose::STANDARD, data),
                    true,
                    data.len(),
                ),
                Message::Pong(data) => (
                    StreamMessageType::WsPong,
                    base64::Engine::encode(&base64::engine::general_purpose::STANDARD, data),
                    true,
                    data.len(),
                ),
                Message::Close(frame) => {
                    let reason = frame
                        .as_ref()
                        .map(|f| format!("{}: {}", f.code, f.reason))
                        .unwrap_or_else(|| "Normal close".to_string());
                    close_reason = Some(reason.clone());
                    (StreamMessageType::WsClose, reason, false, 0)
                }
                Message::Frame(_) => continue, // Skip raw frames
            };

            // Update counters
            message_count.fetch_add(1, Ordering::Relaxed);
            total_bytes.fetch_add(size as u64, Ordering::Relaxed);

            // Emit stream message event
            let stream_msg = StreamMessage {
                id: uuid::Uuid::new_v4().to_string(),
                timestamp: chrono::Utc::now().timestamp_millis(),
                direction: direction.clone(),
                message_type: msg_type,
                data,
                is_base64,
                size,
            };

            app_state.emit_stream_message(&StreamMessageEvent {
                connection_id: connection_id.to_string(),
                message: stream_msg,
            });

            // Forward the message
            if let Err(e) = write.send(msg).await {
                tracing::debug!("WS write error: {}", e);
                return Err(e.to_string());
            }
        }

        Ok(close_reason)
    }
}

/// Complete the client-side hyper upgrade response, connect upstream, and
/// spawn frame proxying for an already-identified WebSocket request.
pub async fn handle_hyper_upgrade(
    req: Request<Incoming>,
    target_url: String,
    original_headers: HeaderMap,
    app_state: Arc<AppState>,
    meta: WebSocketCaptureMeta,
) -> crate::error::Result<Response<BoxBody<Bytes, hyper::Error>>> {
    let client_response_template = create_response_with_body(&req, || ()).map_err(|e| {
        crate::error::PostGateError::Proxy(format!("Invalid WebSocket handshake: {}", e))
    })?;

    if meta.capture {
        emit_started(&app_state, &meta);
    }

    let upstream_request = build_upstream_request(&target_url, &original_headers)?;
    let (server_ws, upstream_response) = match connect_async(upstream_request).await {
        Ok(result) => result,
        Err(e) => {
            if meta.capture {
                emit_error(&app_state, &meta, &e.to_string(), 0);
            }
            return Err(crate::error::PostGateError::Proxy(format!(
                "WS connect error: {}",
                e
            )));
        }
    };

    let response = client_handshake_response(
        &client_response_template,
        upstream_response.headers(),
        original_headers.get("sec-websocket-protocol").is_some(),
    )?;

    let request_id = meta.request_id.clone();
    let app_state_clone = app_state.clone();
    let proxy = WebSocketProxy::new(request_id.clone(), app_state).with_capture_meta(meta);
    let upgrade_error_meta = if proxy.capture {
        proxy.capture_meta.clone()
    } else {
        None
    };

    tokio::task::spawn(async move {
        match hyper::upgrade::on(req).await {
            Ok(upgraded) => {
                let io = hyper_util::rt::TokioIo::new(upgraded);
                if let Err(e) = proxy.proxy_after_handshake(io, server_ws).await {
                    tracing::debug!("WebSocket proxy error for {}: {}", request_id, e);
                }
            }
            Err(e) => {
                tracing::error!("WebSocket upgrade error: {}", e);
                if let Some(meta) = upgrade_error_meta.as_ref() {
                    emit_error(
                        &app_state_clone,
                        meta,
                        &format!("WebSocket upgrade error: {}", e),
                        0,
                    );
                }
            }
        }
    });

    Ok(response)
}

fn build_upstream_request(
    target_url: &str,
    original_headers: &HeaderMap,
) -> crate::error::Result<tokio_tungstenite::tungstenite::handshake::client::Request> {
    let mut request = target_url
        .into_client_request()
        .map_err(|e| crate::error::PostGateError::Proxy(format!("WS request error: {}", e)))?;

    for name in [
        "origin",
        "sec-websocket-protocol",
        "cookie",
        "authorization",
        "user-agent",
    ] {
        let header_name = HeaderName::from_static(name);
        for value in original_headers.get_all(header_name.clone()).iter() {
            request
                .headers_mut()
                .append(header_name.clone(), value.clone());
        }
    }

    Ok(request)
}

fn client_handshake_response(
    template: &tokio_tungstenite::tungstenite::handshake::server::Response,
    upstream_headers: &HeaderMap,
    client_requested_protocol: bool,
) -> crate::error::Result<Response<BoxBody<Bytes, hyper::Error>>> {
    let mut builder = Response::builder().status(template.status());

    for (name, value) in template.headers() {
        builder = builder.header(name, value);
    }

    if client_requested_protocol {
        if let Some(protocol) = upstream_headers.get("sec-websocket-protocol") {
            builder = builder.header("Sec-WebSocket-Protocol", protocol);
        }
    }

    builder
        .body(
            Empty::<Bytes>::new()
                .map_err(|_: std::convert::Infallible| unreachable!())
                .boxed(),
        )
        .map_err(|e| crate::error::PostGateError::Proxy(format!("WS response error: {}", e)))
}

fn emit_started(app_state: &Arc<AppState>, meta: &WebSocketCaptureMeta) {
    app_state.emit_request_event(&CapturedRequestEvent {
        id: meta.request_id.clone(),
        event_type: RequestEventType::Started,
        data: CapturedRequestData {
            id: meta.request_id.clone(),
            timestamp: meta.timestamp,
            method: meta.method.clone(),
            url: meta.url.clone(),
            host: meta.host.clone(),
            path: meta.path.clone(),
            request_headers: Some(meta.request_headers.clone()),
            matched_rules: meta.matched_rules.clone(),
            protocol: "websocket".to_string(),
            request_size: 0,
            tls_version: meta.tls_version.clone(),
            remote_addr: meta.remote_addr.clone(),
            ..Default::default()
        },
    });
}

fn emit_error(app_state: &Arc<AppState>, meta: &WebSocketCaptureMeta, error: &str, duration: u64) {
    app_state.emit_request_event(&CapturedRequestEvent {
        id: meta.request_id.clone(),
        event_type: RequestEventType::Error,
        data: CapturedRequestData {
            id: meta.request_id.clone(),
            timestamp: meta.timestamp,
            method: meta.method.clone(),
            url: meta.url.clone(),
            host: meta.host.clone(),
            path: meta.path.clone(),
            request_headers: Some(meta.request_headers.clone()),
            duration_ms: Some(duration),
            matched_rules: meta.matched_rules.clone(),
            protocol: "websocket".to_string(),
            request_size: 0,
            error: Some(error.to_string()),
            tls_version: meta.tls_version.clone(),
            remote_addr: meta.remote_addr.clone(),
            ..Default::default()
        },
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_websocket_upgrade() {
        let mut headers = HashMap::new();
        headers.insert("upgrade".to_string(), "websocket".to_string());
        headers.insert("connection".to_string(), "Upgrade".to_string());
        headers.insert(
            "sec-websocket-key".to_string(),
            "dGhlIHNhbXBsZSBub25jZQ==".to_string(),
        );

        assert!(is_websocket_upgrade(&headers));
    }

    #[test]
    fn test_is_not_websocket_upgrade() {
        let headers = HashMap::new();
        assert!(!is_websocket_upgrade(&headers));

        let mut headers = HashMap::new();
        headers.insert("upgrade".to_string(), "h2c".to_string());
        assert!(!is_websocket_upgrade(&headers));
    }

    #[test]
    fn test_build_ws_url() {
        assert_eq!(
            build_ws_url("example.com", 80, "/ws", false),
            "ws://example.com/ws"
        );
        assert_eq!(
            build_ws_url("example.com", 443, "/ws", true),
            "wss://example.com/ws"
        );
        assert_eq!(
            build_ws_url("example.com", 8080, "/ws", false),
            "ws://example.com:8080/ws"
        );
        assert_eq!(
            build_ws_url("example.com", 8443, "/ws", true),
            "wss://example.com:8443/ws"
        );
    }
}
