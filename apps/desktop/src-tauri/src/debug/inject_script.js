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
          userAgent: navigator.userAgent,
          cdpEnabled: cdpReady
        });

        // Flush queued messages
        while (messageQueue.length > 0) {
          const msg = messageQueue.shift();
          ws.send(JSON.stringify(msg));
        }
      };

      ws.onclose = function() {
        ws = null;
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
    if (ws && ws.readyState === WebSocket.OPEN) {
      ws.send(JSON.stringify(msg));
    } else {
      messageQueue.push(msg);
    }
  }

  // Handle messages from PostGate server
  function handleServerMessage(msg) {
    switch (msg.type) {
      case 'welcome':
        sessionId = msg.sessionId;
        console.log('[PostGate] Session established:', sessionId);
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
      errorType: event.error?.name || 'Error',
      message: event.message,
      stack: event.error?.stack,
      sourceUrl: event.filename,
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
      errorType: 'UnhandledRejection',
      message: reason?.message || String(reason),
      stack: reason?.stack,
      timestamp: Date.now()
    });
  });

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
