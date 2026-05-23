// PostGate Debug Injection Script with Chobitsu CDP Support
// This script is injected into HTML pages when debug:// rules match
// It provides full Chrome DevTools Protocol support via Chobitsu

(function() {
  'use strict';

  // Prevent double injection
  if (window.__POSTGATE_DEBUG_INJECTED__) return;
  window.__POSTGATE_DEBUG_INJECTED__ = true;

  const POSTGATE_DEBUG_PORT = __DEBUG_PORT__;
  const WS_URL = 'ws://127.0.0.1:' + POSTGATE_DEBUG_PORT;

  let ws = null;
  let sessionId = null;
  let chobitsu = null;
  let messageQueue = [];
  let reconnectAttempts = 0;
  const MAX_RECONNECT = 5;

  function flushMessageQueue() {
    if (!ws || ws.readyState !== WebSocket.OPEN || !sessionId) return;
    while (messageQueue.length > 0) {
      const msg = messageQueue.shift();
      ws.send(JSON.stringify(msg));
    }
  }

  // Load Chobitsu dynamically from CDN
  function loadChobitsu() {
    return new Promise((resolve, reject) => {
      if (window.chobitsu) {
        resolve(window.chobitsu);
        return;
      }

      const script = document.createElement('script');
      script.src = 'https://cdn.jsdelivr.net/npm/chobitsu@1.8.6/dist/chobitsu.min.js';
      script.onload = () => {
        if (window.chobitsu) {
          resolve(window.chobitsu);
        } else {
          reject(new Error('Chobitsu failed to load'));
        }
      };
      script.onerror = () => reject(new Error('Failed to load Chobitsu script'));
      document.head.appendChild(script);
    });
  }

  // Initialize Chobitsu and set up CDP message handling
  async function initChobitsu() {
    try {
      chobitsu = await loadChobitsu();

      // Set up message handler - forward CDP responses to PostGate
      chobitsu.setOnMessage((message) => {
        send({
          type: 'cdp',
          message: typeof message === 'string' ? JSON.parse(message) : message
        });
      });

      console.log('[PostGate] Chobitsu CDP initialized');
      return true;
    } catch (err) {
      console.error('[PostGate] Failed to initialize Chobitsu:', err);
      return false;
    }
  }

  // Connect to PostGate debug server
  function connect() {
    try {
      ws = new WebSocket(WS_URL);

      ws.onopen = async function() {
        reconnectAttempts = 0;
        console.log('[PostGate] Connected to debug server');

        // Initialize Chobitsu
        const cdpReady = await initChobitsu();

        // Send hello message with page info
        send({
          type: 'hello',
          url: window.location.href,
          title: document.title,
          user_agent: navigator.userAgent,
          cdp_enabled: cdpReady
        });
      };

      ws.onclose = function() {
        ws = null;
        sessionId = null;
        console.log('[PostGate] Disconnected from debug server');
        
        if (reconnectAttempts < MAX_RECONNECT) {
          reconnectAttempts++;
          const delay = Math.min(1000 * Math.pow(2, reconnectAttempts - 1), 10000);
          setTimeout(connect, delay);
        }
      };

      ws.onerror = function(err) {
        console.error('[PostGate] WebSocket error:', err);
        ws = null;
      };

      ws.onmessage = function(event) {
        try {
          const msg = JSON.parse(event.data);
          handleServerMessage(msg);
        } catch (e) {
          console.error('[PostGate] Failed to parse message:', e);
        }
      };
    } catch (e) {
      console.error('[PostGate] Failed to connect:', e);
    }
  }

  // Send message to PostGate server
  function send(msg) {
    const isHello = msg && msg.type === 'hello';
    if (ws && ws.readyState === WebSocket.OPEN && (isHello || sessionId)) {
      ws.send(JSON.stringify(msg));
    } else {
      messageQueue.push(msg);
    }
  }

  // Handle messages from PostGate server
  function handleServerMessage(msg) {
    switch (msg.type) {
      case 'welcome':
        sessionId = msg.session_id;
        console.log('[PostGate] Session established:', sessionId);
        flushMessageQueue();
        break;

      case 'cdp':
        // Forward CDP message to Chobitsu
        if (chobitsu && msg.message) {
          chobitsu.sendRawMessage(JSON.stringify(msg.message));
        }
        break;

      case 'eval':
        // Execute JavaScript code
        try {
          const result = eval(msg.code);
          send({
            type: 'eval_result',
            id: msg.id,
            result: serialize(result)
          });
        } catch (e) {
          send({
            type: 'eval_error',
            id: msg.id,
            error: {
              name: e.name,
              message: e.message,
              stack: e.stack
            }
          });
        }
        break;

      case 'pong':
        // Keep-alive response
        break;

      default:
        console.log('[PostGate] Unknown message type:', msg.type);
    }
  }

  // Serialize values for transmission (handles circular refs, DOM elements, etc.)
  function serialize(value, depth = 0, seen = new WeakSet()) {
    if (depth > 10) return { type: 'truncated', value: '[Max depth exceeded]' };

    if (value === null) return { type: 'null' };
    if (value === undefined) return { type: 'undefined' };

    const type = typeof value;

    if (type === 'boolean') return { type: 'boolean', value };
    if (type === 'number') return { type: 'number', value };
    if (type === 'string') {
      return { type: 'string', value: value.length > 10000 ? value.slice(0, 10000) + '...' : value };
    }
    if (type === 'function') return { type: 'function', value: value.name || 'anonymous' };
    if (type === 'symbol') return { type: 'symbol', value: value.toString() };
    if (type === 'bigint') return { type: 'bigint', value: value.toString() };

    if (type === 'object') {
      if (seen.has(value)) return { type: 'circular' };
      seen.add(value);

      if (value instanceof Error) {
        return {
          type: 'error',
          value: { name: value.name, message: value.message, stack: value.stack }
        };
      }

      if (value instanceof Date) {
        return { type: 'date', value: value.toISOString() };
      }

      if (value instanceof RegExp) {
        return { type: 'regexp', value: value.toString() };
      }

      if (value instanceof Element) {
        return {
          type: 'element',
          value: {
            tag: value.tagName.toLowerCase(),
            id: value.id || null,
            classes: Array.from(value.classList),
            outerHTML: value.outerHTML.slice(0, 200)
          }
        };
      }

      if (Array.isArray(value)) {
        return {
          type: 'array',
          value: value.slice(0, 100).map(v => serialize(v, depth + 1, seen))
        };
      }

      // Plain object
      const obj = {};
      const keys = Object.keys(value).slice(0, 50);
      for (const key of keys) {
        try {
          obj[key] = serialize(value[key], depth + 1, seen);
        } catch (e) {
          obj[key] = { type: 'error', value: { message: 'Failed to serialize' } };
        }
      }
      return { type: 'object', value: obj };
    }

    return { type: 'unknown', value: String(value) };
  }

  function nextNetworkId() {
    return 'net-' + Date.now().toString(36) + '-' + Math.random().toString(36).slice(2);
  }

  function headersToObject(headers) {
    const out = {};
    if (!headers) return out;
    try {
      if (headers instanceof Headers) {
        headers.forEach((value, key) => {
          out[key] = value;
        });
        return out;
      }
      if (Array.isArray(headers)) {
        headers.forEach((pair) => {
          if (pair && pair.length >= 2) out[String(pair[0])] = String(pair[1]);
        });
        return out;
      }
      Object.keys(headers).forEach((key) => {
        const value = headers[key];
        if (value != null) out[key] = String(value);
      });
    } catch (_) {}
    return out;
  }

  function bodyPreview(body) {
    if (body == null) return null;
    if (typeof body === 'string') return body.length > 10000 ? body.slice(0, 10000) + '...' : body;
    if (body instanceof URLSearchParams) return body.toString();
    if (body instanceof FormData) return '[FormData]';
    if (body instanceof Blob) return `[Blob ${body.size} bytes]`;
    if (body instanceof ArrayBuffer) return `[ArrayBuffer ${body.byteLength} bytes]`;
    if (ArrayBuffer.isView(body)) return `[${body.constructor.name} ${body.byteLength} bytes]`;
    try {
      return JSON.stringify(body);
    } catch (_) {
      return String(body);
    }
  }

  function requestInfoFromFetch(input, init) {
    let method = init?.method || 'GET';
    let url = '';
    let headers = {};
    const requestBody = init?.body ?? null;

    try {
      if (input instanceof Request) {
        url = input.url;
        method = init?.method || input.method || method;
        headers = headersToObject(input.headers);
      } else {
        url = String(input);
      }
    } catch (_) {
      url = String(input);
    }

    if (init?.headers) {
      Object.assign(headers, headersToObject(init.headers));
    }

    return {
      method: String(method || 'GET').toUpperCase(),
      url,
      headers,
      body: bodyPreview(requestBody),
    };
  }

  function sendNetworkStart(id, info, initiator) {
    send({
      type: 'network',
      id,
      phase: 'start',
      method: info.method,
      url: info.url,
      request_headers: info.headers,
      request_body: info.body,
      initiator,
    });
  }

  function sendNetworkEnd(id, status, responseHeaders, startedAt) {
    send({
      type: 'network',
      id,
      phase: 'end',
      status,
      response_headers: responseHeaders || {},
      duration_ms: Math.round(performance.now() - startedAt),
    });
  }

  function installNetworkCapture() {
    if (window.__POSTGATE_NETWORK_CAPTURED__) return;
    window.__POSTGATE_NETWORK_CAPTURED__ = true;

    if (typeof window.fetch === 'function') {
      const originalFetch = window.fetch.bind(window);
      window.fetch = function(input, init) {
        const id = nextNetworkId();
        const startedAt = performance.now();
        const info = requestInfoFromFetch(input, init);
        sendNetworkStart(id, info, 'fetch');

        return originalFetch(input, init).then((response) => {
          sendNetworkEnd(id, response.status, headersToObject(response.headers), startedAt);
          return response;
        }, (error) => {
          sendNetworkEnd(id, 0, {}, startedAt);
          send({
            type: 'error',
            error_type: 'NetworkError',
            message: error?.message || String(error),
            stack: error?.stack,
          });
          throw error;
        });
      };
    }

    if (typeof window.XMLHttpRequest === 'function') {
      const OriginalXHR = window.XMLHttpRequest;
      const originalOpen = OriginalXHR.prototype.open;
      const originalSetRequestHeader = OriginalXHR.prototype.setRequestHeader;
      const originalSend = OriginalXHR.prototype.send;

      OriginalXHR.prototype.open = function(method, url) {
        this.__postgateNetwork = {
          id: nextNetworkId(),
          method: String(method || 'GET').toUpperCase(),
          url: String(url || ''),
          headers: {},
          startedAt: 0,
        };
        return originalOpen.apply(this, arguments);
      };

      OriginalXHR.prototype.setRequestHeader = function(name, value) {
        if (this.__postgateNetwork) {
          this.__postgateNetwork.headers[String(name)] = String(value);
        }
        return originalSetRequestHeader.apply(this, arguments);
      };

      OriginalXHR.prototype.send = function(body) {
        const info = this.__postgateNetwork || {
          id: nextNetworkId(),
          method: 'GET',
          url: '',
          headers: {},
        };
        info.startedAt = performance.now();
        sendNetworkStart(info.id, {
          method: info.method,
          url: info.url,
          headers: info.headers,
          body: bodyPreview(body),
        }, 'xhr');

        this.addEventListener('loadend', function() {
          const responseHeaders = {};
          try {
            const raw = this.getAllResponseHeaders();
            raw.trim().split(/[\r\n]+/).forEach((line) => {
              const idx = line.indexOf(':');
              if (idx > 0) responseHeaders[line.slice(0, idx).trim().toLowerCase()] = line.slice(idx + 1).trim();
            });
          } catch (_) {}
          sendNetworkEnd(info.id, this.status || 0, responseHeaders, info.startedAt);
        });

        return originalSend.apply(this, arguments);
      };
    }
  }

  // Also capture console for legacy support (in case Chobitsu fails)
  const originalConsole = {};
  const consoleMethods = ['log', 'info', 'warn', 'error', 'debug', 'trace', 'clear'];

  consoleMethods.forEach(method => {
    originalConsole[method] = console[method];
    console[method] = function(...args) {
      // Call original
      originalConsole[method].apply(console, args);

      // Send to PostGate (backup for when Chobitsu Runtime.consoleAPICalled isn't working)
      send({
        type: 'console',
        level: method,
        args: args.map(arg => serialize(arg)),
        timestamp: Date.now(),
        stack: method === 'trace' ? new Error().stack : null
      });
    };
  });

  // Capture uncaught errors
  window.addEventListener('error', function(event) {
    send({
      type: 'error',
      error_type: event.error?.name || 'Error',
      message: event.message,
      stack: event.error?.stack,
      source_url: event.filename,
      line: event.lineno,
      column: event.colno,
      timestamp: Date.now()
    });
  });

  // Capture unhandled promise rejections
  window.addEventListener('unhandledrejection', function(event) {
    const reason = event.reason;
    send({
      type: 'error',
      error_type: 'UnhandledRejection',
      message: reason?.message || String(reason),
      stack: reason?.stack,
      timestamp: Date.now()
    });
  });

  installNetworkCapture();

  // Keep-alive ping
  setInterval(() => {
    if (ws && ws.readyState === WebSocket.OPEN) {
      send({ type: 'ping' });
    }
  }, 30000);

  // Start connection
  connect();

  console.log('[PostGate] Debug injection initialized');
})();
