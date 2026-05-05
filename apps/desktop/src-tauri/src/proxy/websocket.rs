//! WebSocket proxy support with frame capture
//!
//! This module handles WebSocket connections, proxying frames bidirectionally
//! while capturing each frame for the capture panel.

use crate::state::{
    AppState, StreamDirection, StreamEndedEvent, StreamMessage, StreamMessageEvent,
    StreamMessageType,
};
use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{protocol::Message, Error as WsError},
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
    let port_str = if (secure && port == 443) || (!secure && port == 80) {
        String::new()
    } else {
        format!(":{}", port)
    };
    format!("{}://{}{}{}", scheme, host, port_str, path)
}

/// WebSocket proxy handler
pub struct WebSocketProxy {
    connection_id: String,
    app_state: Arc<AppState>,
    message_count: Arc<AtomicU64>,
    total_bytes: Arc<AtomicU64>,
    start_time: std::time::Instant,
}

impl WebSocketProxy {
    pub fn new(connection_id: String, app_state: Arc<AppState>) -> Self {
        Self {
            connection_id,
            app_state,
            message_count: Arc::new(AtomicU64::new(0)),
            total_bytes: Arc::new(AtomicU64::new(0)),
            start_time: std::time::Instant::now(),
        }
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
            )
            .await
        });

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

        // Emit stream ended event
        self.app_state.emit_stream_ended(&StreamEndedEvent {
            connection_id: self.connection_id.clone(),
            message_count: self.message_count.load(Ordering::Relaxed),
            total_bytes: self.total_bytes.load(Ordering::Relaxed),
            duration_ms: self.start_time.elapsed().as_millis() as u64,
            close_reason,
        });

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
