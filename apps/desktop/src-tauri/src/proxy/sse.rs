//! Server-Sent Events (SSE) streaming proxy support
//!
//! This module handles SSE streams, forwarding events in real-time while
//! capturing each event for the capture panel.

/// Check if a response is an SSE stream based on Content-Type header
pub fn is_sse_response(content_type: Option<&str>) -> bool {
    content_type
        .map(|ct| ct.starts_with("text/event-stream"))
        .unwrap_or(false)
}

/// SSE event parsed from the stream
#[derive(Debug, Default)]
pub struct SseEvent {
    pub event_type: Option<String>,
    pub data: String,
    pub id: Option<String>,
    pub retry: Option<u64>,
}

/// SSE parser for parsing incoming SSE data
pub struct SseParser {
    buffer: String,
    current_event: SseEvent,
}

impl SseParser {
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
            current_event: SseEvent::default(),
        }
    }

    /// Feed data into the parser and return any complete events
    pub fn feed(&mut self, data: &[u8]) -> Vec<SseEvent> {
        let text = String::from_utf8_lossy(data);
        self.buffer.push_str(&text);

        let mut events = Vec::new();

        // Process complete lines
        while let Some(newline_pos) = self.buffer.find('\n') {
            let line = self.buffer[..newline_pos].trim_end_matches('\r').to_string();
            self.buffer = self.buffer[newline_pos + 1..].to_string();

            if line.is_empty() {
                // Empty line marks end of event
                if !self.current_event.data.is_empty() {
                    // Remove trailing newline from data if present
                    if self.current_event.data.ends_with('\n') {
                        self.current_event.data.pop();
                    }
                    events.push(std::mem::take(&mut self.current_event));
                }
            } else if let Some(value) = line.strip_prefix("event:") {
                self.current_event.event_type = Some(value.trim().to_string());
            } else if let Some(value) = line.strip_prefix("data:") {
                if !self.current_event.data.is_empty() {
                    self.current_event.data.push('\n');
                }
                self.current_event.data.push_str(value.trim_start());
            } else if let Some(value) = line.strip_prefix("id:") {
                self.current_event.id = Some(value.trim().to_string());
            } else if let Some(value) = line.strip_prefix("retry:") {
                if let Ok(ms) = value.trim().parse::<u64>() {
                    self.current_event.retry = Some(ms);
                }
            }
            // Comment lines (starting with ':') are ignored
        }

        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_sse_response() {
        assert!(is_sse_response(Some("text/event-stream")));
        assert!(is_sse_response(Some("text/event-stream; charset=utf-8")));
        assert!(!is_sse_response(Some("application/json")));
        assert!(!is_sse_response(None));
    }

    #[test]
    fn test_sse_parser_simple() {
        let mut parser = SseParser::new();
        let events = parser.feed(b"data: hello\n\n");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "hello");
        assert!(events[0].event_type.is_none());
    }

    #[test]
    fn test_sse_parser_with_event_type() {
        let mut parser = SseParser::new();
        let events = parser.feed(b"event: message\ndata: hello world\n\n");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, Some("message".to_string()));
        assert_eq!(events[0].data, "hello world");
    }

    #[test]
    fn test_sse_parser_multiline_data() {
        let mut parser = SseParser::new();
        let events = parser.feed(b"data: line1\ndata: line2\ndata: line3\n\n");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "line1\nline2\nline3");
    }

    #[test]
    fn test_sse_parser_multiple_events() {
        let mut parser = SseParser::new();
        let events = parser.feed(b"data: first\n\ndata: second\n\n");
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].data, "first");
        assert_eq!(events[1].data, "second");
    }

    #[test]
    fn test_sse_parser_chunked_input() {
        let mut parser = SseParser::new();
        
        // First chunk - incomplete
        let events1 = parser.feed(b"data: hel");
        assert!(events1.is_empty());
        
        // Second chunk - still incomplete
        let events2 = parser.feed(b"lo wor");
        assert!(events2.is_empty());
        
        // Third chunk - completes the event
        let events3 = parser.feed(b"ld\n\n");
        assert_eq!(events3.len(), 1);
        assert_eq!(events3[0].data, "hello world");
    }
}
