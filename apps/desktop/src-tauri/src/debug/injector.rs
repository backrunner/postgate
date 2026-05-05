// Script injector for HTML responses with Chobitsu CDP support

use regex::Regex;
use std::sync::OnceLock;

static HEAD_REGEX: OnceLock<Regex> = OnceLock::new();
static BODY_REGEX: OnceLock<Regex> = OnceLock::new();

/// Script injector for inserting debug scripts into HTML responses
pub struct ScriptInjector {
    debug_port: u16,
}

impl ScriptInjector {
    pub fn new(debug_port: u16) -> Self {
        Self { debug_port }
    }

    /// Get the inject script content with Chobitsu CDP support
    pub fn get_inject_script(&self) -> String {
        format!(
            r#"<script data-postgate-inject="true">
(function() {{
  'use strict';

  // Prevent double injection
  if (window.__POSTGATE_DEBUG_INJECTED__) return;
  window.__POSTGATE_DEBUG_INJECTED__ = true;

  const POSTGATE_DEBUG_PORT = {port};
  const WS_URL = 'ws://127.0.0.1:' + POSTGATE_DEBUG_PORT;

  let ws = null;
  let sessionId = null;
  let chobitsu = null;
  let messageQueue = [];
  let reconnectAttempts = 0;
  const MAX_RECONNECT = 5;

  // Load Chobitsu dynamically from CDN
  function loadChobitsu() {{
    return new Promise((resolve, reject) => {{
      if (window.chobitsu) {{
        resolve(window.chobitsu);
        return;
      }}

      const script = document.createElement('script');
      script.src = 'https://cdn.jsdelivr.net/npm/chobitsu@1.8.6/dist/chobitsu.min.js';
      script.onload = () => {{
        if (window.chobitsu) {{
          resolve(window.chobitsu);
        }} else {{
          reject(new Error('Chobitsu failed to load'));
        }}
      }};
      script.onerror = () => reject(new Error('Failed to load Chobitsu script'));
      document.head.appendChild(script);
    }});
  }}

  // Initialize Chobitsu and set up CDP message handling
  async function initChobitsu() {{
    try {{
      chobitsu = await loadChobitsu();

      // Set up message handler - forward CDP responses to PostGate
      chobitsu.setOnMessage((message) => {{
        send({{
          type: 'cdp',
          message: typeof message === 'string' ? JSON.parse(message) : message
        }});
      }});

      console.log('[PostGate] Chobitsu CDP initialized');
      return true;
    }} catch (err) {{
      console.error('[PostGate] Failed to initialize Chobitsu:', err);
      return false;
    }}
  }}

  // Connect to PostGate debug server
  function connect() {{
    try {{
      ws = new WebSocket(WS_URL);

      ws.onopen = async function() {{
        reconnectAttempts = 0;
        console.log('[PostGate] Connected to debug server');

        // Initialize Chobitsu
        const cdpReady = await initChobitsu();

        // Send hello message with page info
        send({{
          type: 'hello',
          url: window.location.href,
          title: document.title,
          user_agent: navigator.userAgent,
          cdp_enabled: cdpReady
        }});

        // Flush queued messages
        while (messageQueue.length > 0) {{
          const msg = messageQueue.shift();
          ws.send(JSON.stringify(msg));
        }}
      }};

      ws.onclose = function() {{
        ws = null;
        console.log('[PostGate] Disconnected from debug server');
        
        if (reconnectAttempts < MAX_RECONNECT) {{
          reconnectAttempts++;
          const delay = Math.min(1000 * Math.pow(2, reconnectAttempts - 1), 10000);
          setTimeout(connect, delay);
        }}
      }};

      ws.onerror = function(err) {{
        console.error('[PostGate] WebSocket error:', err);
        ws = null;
      }};

      ws.onmessage = function(event) {{
        try {{
          const msg = JSON.parse(event.data);
          handleServerMessage(msg);
        }} catch (e) {{
          console.error('[PostGate] Failed to parse message:', e);
        }}
      }};
    }} catch (e) {{
      console.error('[PostGate] Failed to connect:', e);
    }}
  }}

  // Send message to PostGate server
  function send(msg) {{
    if (ws && ws.readyState === WebSocket.OPEN) {{
      ws.send(JSON.stringify(msg));
    }} else {{
      messageQueue.push(msg);
    }}
  }}

  // Handle messages from PostGate server
  function handleServerMessage(msg) {{
    switch (msg.type) {{
      case 'welcome':
        sessionId = msg.session_id;
        console.log('[PostGate] Session established:', sessionId);
        break;

      case 'cdp':
        // Forward CDP message to Chobitsu
        if (chobitsu && msg.message) {{
          chobitsu.sendRawMessage(JSON.stringify(msg.message));
        }}
        break;

      case 'eval':
        // Execute JavaScript code
        try {{
          const result = eval(msg.code);
          send({{
            type: 'eval_result',
            id: msg.id,
            result: serialize(result)
          }});
        }} catch (e) {{
          send({{
            type: 'eval_error',
            id: msg.id,
            error: {{ name: e.name, message: e.message, stack: e.stack }}
          }});
        }}
        break;

      case 'pong':
        break;

      default:
        console.log('[PostGate] Unknown message type:', msg.type);
    }}
  }}

  // Serialize values for transmission
  function serialize(value, depth, seen) {{
    depth = depth || 0;
    seen = seen || new WeakSet();

    if (depth > 10) return {{ type: 'truncated', value: '[Max depth]' }};
    if (value === null) return {{ type: 'null' }};
    if (value === undefined) return {{ type: 'undefined' }};

    var t = typeof value;
    if (t === 'boolean') return {{ type: 'boolean', value: value }};
    if (t === 'number') return {{ type: 'number', value: value }};
    if (t === 'string') return {{ type: 'string', value: value.length > 10000 ? value.slice(0, 10000) + '...' : value }};
    if (t === 'function') return {{ type: 'function', value: value.name || 'anonymous' }};
    if (t === 'symbol') return {{ type: 'symbol', value: value.toString() }};

    if (t === 'object') {{
      if (seen.has(value)) return {{ type: 'circular' }};
      seen.add(value);

      if (value instanceof Error) {{
        return {{ type: 'error', value: {{ name: value.name, message: value.message, stack: value.stack }} }};
      }}
      if (value instanceof Date) return {{ type: 'date', value: value.toISOString() }};
      if (value instanceof RegExp) return {{ type: 'regexp', value: value.toString() }};
      if (value instanceof Element) {{
        return {{ type: 'element', value: {{ tag: value.tagName.toLowerCase(), id: value.id || null, classes: Array.from(value.classList) }} }};
      }}
      if (Array.isArray(value)) {{
        return {{ type: 'array', value: value.slice(0, 100).map(function(v) {{ return serialize(v, depth + 1, seen); }}) }};
      }}

      var obj = {{}};
      var keys = Object.keys(value).slice(0, 50);
      for (var i = 0; i < keys.length; i++) {{
        try {{ obj[keys[i]] = serialize(value[keys[i]], depth + 1, seen); }}
        catch (e) {{ obj[keys[i]] = {{ type: 'error', value: {{ message: 'Failed to serialize' }} }}; }}
      }}
      return {{ type: 'object', value: obj }};
    }}

    return {{ type: 'unknown', value: String(value) }};
  }}

  // Capture console for fallback/legacy support
  var originalConsole = {{}};
  var consoleMethods = ['log', 'info', 'warn', 'error', 'debug', 'trace', 'clear'];

  consoleMethods.forEach(function(method) {{
    originalConsole[method] = console[method];
    console[method] = function() {{
      var args = Array.prototype.slice.call(arguments);
      originalConsole[method].apply(console, args);
      send({{
        type: 'console',
        level: method,
        args: args.map(function(arg) {{ return serialize(arg); }}),
        timestamp: Date.now(),
        stack: method === 'trace' ? new Error().stack : null
      }});
    }};
  }});

  // Capture uncaught errors
  window.addEventListener('error', function(event) {{
    send({{
      type: 'error',
      error_type: event.error && event.error.name ? event.error.name : 'Error',
      message: event.message,
      stack: event.error ? event.error.stack : null,
      source_url: event.filename,
      line: event.lineno,
      column: event.colno,
      timestamp: Date.now()
    }});
  }});

  // Capture unhandled promise rejections
  window.addEventListener('unhandledrejection', function(event) {{
    var reason = event.reason;
    send({{
      type: 'error',
      error_type: 'UnhandledRejection',
      message: reason && reason.message ? reason.message : String(reason),
      stack: reason ? reason.stack : null,
      timestamp: Date.now()
    }});
  }});

  // Keep-alive ping
  setInterval(function() {{
    if (ws && ws.readyState === WebSocket.OPEN) {{
      send({{ type: 'ping' }});
    }}
  }}, 30000);

  // Start connection
  connect();

  console.log('[PostGate] Debug injection initialized (CDP enabled)');
}})();
</script>"#,
            port = self.debug_port
        )
    }

    /// Inject the debug script into an HTML response body
    pub fn inject_into_html(&self, html: &str) -> String {
        let script = self.get_inject_script();

        // Try to inject after <head>
        let head_regex = HEAD_REGEX.get_or_init(|| Regex::new(r"(?i)(<head[^>]*>)").unwrap());

        if let Some(caps) = head_regex.captures(html) {
            if let Some(m) = caps.get(1) {
                let pos = m.end();
                let mut result = String::with_capacity(html.len() + script.len());
                result.push_str(&html[..pos]);
                result.push_str(&script);
                result.push_str(&html[pos..]);
                return result;
            }
        }

        // Fallback: inject after <body>
        let body_regex = BODY_REGEX.get_or_init(|| Regex::new(r"(?i)(<body[^>]*>)").unwrap());

        if let Some(caps) = body_regex.captures(html) {
            if let Some(m) = caps.get(1) {
                let pos = m.end();
                let mut result = String::with_capacity(html.len() + script.len());
                result.push_str(&html[..pos]);
                result.push_str(&script);
                result.push_str(&html[pos..]);
                return result;
            }
        }

        // Last resort: prepend to document
        format!("{}{}", script, html)
    }

    /// Check if a content type is HTML
    pub fn is_html_content_type(content_type: &str) -> bool {
        let ct = content_type.to_lowercase();
        ct.contains("text/html") || ct.contains("application/xhtml")
    }

    /// Check if already injected
    pub fn is_already_injected(html: &str) -> bool {
        html.contains("data-postgate-inject")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inject_after_head() {
        let injector = ScriptInjector::new(9229);
        let html = "<html><head><title>Test</title></head><body>Hello</body></html>";
        let result = injector.inject_into_html(html);

        assert!(result.contains("data-postgate-inject"));
        assert!(result.contains("<head><script"));
        assert!(result.contains("chobitsu"));
    }

    #[test]
    fn test_inject_after_body_fallback() {
        let injector = ScriptInjector::new(9229);
        let html = "<html><body>Hello</body></html>";
        let result = injector.inject_into_html(html);

        assert!(result.contains("data-postgate-inject"));
        assert!(result.contains("<body><script"));
    }

    #[test]
    fn test_is_html_content_type() {
        assert!(ScriptInjector::is_html_content_type("text/html"));
        assert!(ScriptInjector::is_html_content_type(
            "text/html; charset=utf-8"
        ));
        assert!(ScriptInjector::is_html_content_type(
            "application/xhtml+xml"
        ));
        assert!(!ScriptInjector::is_html_content_type("application/json"));
    }

    #[test]
    fn test_already_injected() {
        let injector = ScriptInjector::new(9229);
        let html = "<html><head></head><body>Hello</body></html>";
        let injected = injector.inject_into_html(html);

        assert!(ScriptInjector::is_already_injected(&injected));
        assert!(!ScriptInjector::is_already_injected(html));
    }
}
