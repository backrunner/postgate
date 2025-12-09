//! Server-Sent Events (SSE) capture support
//!
//! This module provides SSE stream capture for debugging real-time event streams.

use crate::error::{PostGateError, Result};
use crate::state::AppState;
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::Emitter;

/// A captured SSE event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapturedSseEvent {
    pub id: String,
    pub connection_id: String,
    pub timestamp: i64,
    pub event_type: Option<String>,
    pub event_id: Option<String>,
    pub data: String,
    pub retry: Option<u64>,
}

/// SSE stream metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SseStreamInfo {
    pub id: String,
    pub url: String,
    pub host: String,
    pub path: String,
    pub timestamp: i64,
}

/// SSE parser state
pub struct SseParser {
    buffer: String,
    event_type: Option<String>,
    event_id: Option<String>,
    data_lines: Vec<String>,
    retry: Option<u64>,
}

impl SseParser {
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
            event_type: None,
            event_id: None,
            data_lines: Vec::new(),
            retry: None,
        }
    }

    /// Parse incoming bytes and return any complete events
    pub fn parse(&mut self, chunk: &[u8]) -> Vec<ParsedSseEvent> {
        let text = String::from_utf8_lossy(chunk);
        self.buffer.push_str(&text);

        let mut events = Vec::new();

        // Process complete lines
        while let Some(newline_pos) = self.buffer.find('\n') {
            let line = self.buffer[..newline_pos].trim_end_matches('\r').to_string();
            self.buffer = self.buffer[newline_pos + 1..].to_string();

            if line.is_empty() {
                // Empty line marks end of event
                if !self.data_lines.is_empty() {
                    events.push(ParsedSseEvent {
                        event_type: self.event_type.take(),
                        event_id: self.event_id.take(),
                        data: self.data_lines.join("\n"),
                        retry: self.retry.take(),
                    });
                    self.data_lines.clear();
                }
            } else if let Some(value) = line.strip_prefix("event:") {
                self.event_type = Some(value.trim().to_string());
            } else if let Some(value) = line.strip_prefix("id:") {
                self.event_id = Some(value.trim().to_string());
            } else if let Some(value) = line.strip_prefix("data:") {
                self.data_lines.push(value.trim().to_string());
            } else if let Some(value) = line.strip_prefix("retry:") {
                if let Ok(ms) = value.trim().parse::<u64>() {
                    self.retry = Some(ms);
                }
            } else if line.starts_with(':') {
                // Comment line, ignore
            } else if !line.contains(':') {
                // Field with no value, treat as empty
                // Per SSE spec, "event" becomes event: "", etc.
            }
        }

        events
    }
}

/// A parsed SSE event (internal)
#[derive(Debug)]
pub struct ParsedSseEvent {
    pub event_type: Option<String>,
    pub event_id: Option<String>,
    pub data: String,
    pub retry: Option<u64>,
}

/// SSE stream handler
pub struct SseStreamHandler {
    connection_id: String,
    app_state: Arc<AppState>,
    parser: SseParser,
    url: String,
    host: String,
    path: String,
}

impl SseStreamHandler {
    pub fn new(
        connection_id: String,
        app_state: Arc<AppState>,
        url: String,
        host: String,
        path: String,
    ) -> Self {
        Self {
            connection_id,
            app_state,
            parser: SseParser::new(),
            url,
            host,
            path,
        }
    }

    /// Emit stream started event
    pub fn emit_started(&self) {
        let _ = self.app_state.app_handle.emit(
            "sse:started",
            SseStreamInfo {
                id: self.connection_id.clone(),
                url: self.url.clone(),
                host: self.host.clone(),
                path: self.path.clone(),
                timestamp: chrono::Utc::now().timestamp_millis(),
            },
        );
    }

    /// Process incoming SSE data chunk
    pub fn process_chunk(&mut self, chunk: &[u8]) {
        let events = self.parser.parse(chunk);

        for event in events {
            let captured = CapturedSseEvent {
                id: uuid::Uuid::new_v4().to_string(),
                connection_id: self.connection_id.clone(),
                timestamp: chrono::Utc::now().timestamp_millis(),
                event_type: event.event_type,
                event_id: event.event_id,
                data: event.data,
                retry: event.retry,
            };

            let _ = self.app_state.app_handle.emit("sse:event", &captured);
        }
    }

    /// Emit stream ended event
    pub fn emit_ended(&self) {
        let _ = self
            .app_state
            .app_handle
            .emit("sse:ended", &self.connection_id);
    }
}

/// Check if a response is an SSE stream
pub fn is_sse_response(content_type: Option<&str>) -> bool {
    content_type
        .map(|ct| ct.starts_with("text/event-stream"))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sse_parser_simple() {
        let mut parser = SseParser::new();

        let events = parser.parse(b"data: hello\n\n");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "hello");
        assert!(events[0].event_type.is_none());
    }

    #[test]
    fn test_sse_parser_with_event_type() {
        let mut parser = SseParser::new();

        let events = parser.parse(b"event: message\ndata: hello\n\n");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "hello");
        assert_eq!(events[0].event_type, Some("message".to_string()));
    }

    #[test]
    fn test_sse_parser_multiline_data() {
        let mut parser = SseParser::new();

        let events = parser.parse(b"data: line1\ndata: line2\n\n");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "line1\nline2");
    }

    #[test]
    fn test_sse_parser_with_id() {
        let mut parser = SseParser::new();

        let events = parser.parse(b"id: 123\ndata: hello\n\n");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_id, Some("123".to_string()));
    }

    #[test]
    fn test_sse_parser_partial_chunks() {
        let mut parser = SseParser::new();

        // First chunk - incomplete
        let events1 = parser.parse(b"data: hel");
        assert_eq!(events1.len(), 0);

        // Second chunk - completes the event
        let events2 = parser.parse(b"lo\n\n");
        assert_eq!(events2.len(), 1);
        assert_eq!(events2[0].data, "hello");
    }

    #[test]
    fn test_sse_parser_multiple_events() {
        let mut parser = SseParser::new();

        let events = parser.parse(b"data: event1\n\ndata: event2\n\n");
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].data, "event1");
        assert_eq!(events[1].data, "event2");
    }
}
