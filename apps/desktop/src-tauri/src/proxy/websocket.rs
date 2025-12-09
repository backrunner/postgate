//! WebSocket proxy support with frame capture
//!
//! This module provides WebSocket proxying with full frame capture for debugging.

use crate::error::{PostGateError, Result};
use crate::state::AppState;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tauri::Emitter;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{protocol::Message, Error as WsError},
    MaybeTlsStream, WebSocketStream,
};

/// Captured WebSocket frame
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapturedWsFrame {
    pub id: String,
    pub connection_id: String,
    pub timestamp: i64,
    pub direction: WsFrameDirection,
    pub frame_type: WsFrameType,
    pub payload: Option<Vec<u8>>,
    pub payload_text: Option<String>,
    pub size: usize,
}

/// Direction of WebSocket frame
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WsFrameDirection {
    ClientToServer,
    ServerToClient,
}

/// Type of WebSocket frame
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WsFrameType {
    Text,
    Binary,
    Ping,
    Pong,
    Close,
}

impl From<&Message> for WsFrameType {
    fn from(msg: &Message) -> Self {
        match msg {
            Message::Text(_) => WsFrameType::Text,
            Message::Binary(_) => WsFrameType::Binary,
            Message::Ping(_) => WsFrameType::Ping,
            Message::Pong(_) => WsFrameType::Pong,
            Message::Close(_) => WsFrameType::Close,
            Message::Frame(_) => WsFrameType::Binary,
        }
    }
}

/// WebSocket connection metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsConnectionInfo {
    pub id: String,
    pub url: String,
    pub host: String,
    pub path: String,
    pub timestamp: i64,
    pub protocol: Option<String>,
    pub extensions: Vec<String>,
}

/// WebSocket proxy handler
pub struct WebSocketProxy {
    app_state: Arc<AppState>,
}

impl WebSocketProxy {
    pub fn new(app_state: Arc<AppState>) -> Self {
        Self { app_state }
    }

    /// Proxy a WebSocket connection
    pub async fn proxy_connection<S>(
        &self,
        client_stream: S,
        connection_id: String,
        target_url: String,
        host: String,
    ) -> Result<()>
    where
        S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        // Accept WebSocket from client
        let client_ws = tokio_tungstenite::accept_async(client_stream)
            .await
            .map_err(|e| PostGateError::Proxy(format!("WS accept error: {}", e)))?;

        // Connect to upstream WebSocket server
        let (server_ws, _response) = connect_async(&target_url)
            .await
            .map_err(|e| PostGateError::Proxy(format!("WS connect error: {}", e)))?;

        // Emit connection event
        let _ = self.app_state.app_handle.emit(
            "ws:connected",
            WsConnectionInfo {
                id: connection_id.clone(),
                url: target_url.clone(),
                host: host.clone(),
                path: target_url
                    .split('/')
                    .skip(3)
                    .collect::<Vec<_>>()
                    .join("/"),
                timestamp: chrono::Utc::now().timestamp_millis(),
                protocol: None,
                extensions: vec![],
            },
        );

        // Split both connections for bidirectional proxying
        let (client_write, client_read) = client_ws.split();
        let (server_write, server_read) = server_ws.split();

        let conn_id_1 = connection_id.clone();
        let conn_id_2 = connection_id.clone();
        let app_state_1 = self.app_state.clone();
        let app_state_2 = self.app_state.clone();

        // Client -> Server
        let client_to_server = tokio::spawn(async move {
            Self::forward_frames(
                client_read,
                server_write,
                conn_id_1,
                WsFrameDirection::ClientToServer,
                app_state_1,
            )
            .await
        });

        // Server -> Client
        let server_to_client = tokio::spawn(async move {
            Self::forward_frames(
                server_read,
                client_write,
                conn_id_2,
                WsFrameDirection::ServerToClient,
                app_state_2,
            )
            .await
        });

        // Wait for either direction to close
        tokio::select! {
            result = client_to_server => {
                if let Err(e) = result {
                    tracing::debug!("Client to server task error: {}", e);
                }
            }
            result = server_to_client => {
                if let Err(e) = result {
                    tracing::debug!("Server to client task error: {}", e);
                }
            }
        }

        // Emit disconnection event
        let _ = self.app_state.app_handle.emit("ws:disconnected", &connection_id);

        Ok(())
    }

    /// Forward frames between WebSocket connections with capture
    async fn forward_frames<R, W>(
        mut read: R,
        mut write: W,
        connection_id: String,
        direction: WsFrameDirection,
        app_state: Arc<AppState>,
    ) -> Result<()>
    where
        R: StreamExt<Item = std::result::Result<Message, WsError>> + Unpin,
        W: SinkExt<Message, Error = WsError> + Unpin,
    {
        while let Some(message) = read.next().await {
            let msg = match message {
                Ok(m) => m,
                Err(e) => {
                    tracing::debug!("WS read error: {}", e);
                    break;
                }
            };

            // Capture the frame
            let frame = Self::capture_frame(&connection_id, &msg, &direction);
            let _ = app_state.app_handle.emit("ws:frame", &frame);

            // Forward the message
            if let Err(e) = write.send(msg).await {
                tracing::debug!("WS write error: {}", e);
                break;
            }
        }

        Ok(())
    }

    /// Capture a WebSocket frame
    fn capture_frame(
        connection_id: &str,
        message: &Message,
        direction: &WsFrameDirection,
    ) -> CapturedWsFrame {
        let (payload, payload_text, size) = match message {
            Message::Text(text) => (None, Some(text.to_string()), text.len()),
            Message::Binary(data) => (Some(data.to_vec()), None, data.len()),
            Message::Ping(data) => (Some(data.to_vec()), None, data.len()),
            Message::Pong(data) => (Some(data.to_vec()), None, data.len()),
            Message::Close(frame) => {
                let text = frame.as_ref().map(|f| f.reason.to_string());
                let size = text.as_ref().map(|t| t.len()).unwrap_or(0);
                (None, text, size)
            }
            Message::Frame(_) => (None, None, 0),
        };

        CapturedWsFrame {
            id: uuid::Uuid::new_v4().to_string(),
            connection_id: connection_id.to_string(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            direction: direction.clone(),
            frame_type: WsFrameType::from(message),
            payload,
            payload_text,
            size,
        }
    }
}

/// Check if a request is a WebSocket upgrade request
pub fn is_websocket_upgrade(headers: &HashMap<String, String>) -> bool {
    headers
        .get("upgrade")
        .map(|v| v.to_lowercase() == "websocket")
        .unwrap_or(false)
        && headers
            .get("connection")
            .map(|v| v.to_lowercase().contains("upgrade"))
            .unwrap_or(false)
}

/// Build WebSocket URL from request info
pub fn build_ws_url(host: &str, port: u16, path: &str, secure: bool) -> String {
    let scheme = if secure { "wss" } else { "ws" };
    if port == 80 || port == 443 {
        format!("{}://{}{}", scheme, host, path)
    } else {
        format!("{}://{}:{}{}", scheme, host, port, path)
    }
}
