import { useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import { useProxyStore } from "@/stores/proxy";
import { useCaptureStore, CapturedRequest, Protocol } from "@/stores/capture";
import { useStreamStore, StreamDirection, StreamMessageType } from "@/stores/stream";

export interface ProxyConfig {
  port: number;
  enableHttp2: boolean;
  enableQuic: boolean;
  quicPort: number | null;
  debugPort: number;
}

export interface ProxyStatusResponse {
  status: "stopped" | "starting" | "running" | "stopping" | "error";
  port: number;
  error: string | null;
}

interface RequestEventData {
  id: string;
  timestamp: number;
  method: string;
  url: string;
  host: string;
  path: string;
  requestHeaders?: Record<string, string>;
  responseStatus?: number;
  responseHeaders?: Record<string, string>;
  durationMs?: number;
  matchedRules: string[];
  protocol: string;
  contentType?: string;
  requestSize: number;
  responseSize?: number;
  error?: string;
  tlsVersion?: string;
  remoteAddr?: string;
}

interface RequestEvent {
  id: string;
  eventType: "started" | "response_received" | "completed" | "error";
  data: RequestEventData;
}

// Stream event types from backend
interface StreamMessageEventPayload {
  connectionId: string;
  message: {
    id: string;
    timestamp: number;
    direction: "inbound" | "outbound";
    messageType: "sse_event" | "sse_comment" | "ws_text" | "ws_binary" | "ws_ping" | "ws_pong" | "ws_close";
    data: string;
    isBase64: boolean;
    size: number;
  };
}

interface StreamEndedEventPayload {
  connectionId: string;
  messageCount: number;
  totalBytes: number;
  durationMs: number;
  closeReason: string | null;
}

// Module-level guard to prevent concurrent startProxy calls across multiple hook instances
let _startingProxy = false;
// Module-level flag to ensure auto-start only happens once
let _autoStarted = false;
// Module-level flag to ensure Tauri event listeners are installed exactly
// once for the entire app lifetime. Multiple components call `useProxy()`
// (Header, CapturePage, Toolbar) and previously each would register its
// own set of three listeners, causing every proxy:request / stream event
// to be processed 3+ times per dispatch — 3× IPC deserialize, 3× Zustand
// set() fan-out, 3× pending-queue push. Registering once globally fixes
// that and is also correct in semantics: these events have nothing to do
// with any individual component's lifetime.
let _listenersInstalled = false;
/** Promise tracking the one-shot installation so concurrent first calls
 *  all await the same set of listeners. */
let _listenersInstallPromise: Promise<void> | null = null;
/** Cleanup handle, mostly for completeness — in practice the listeners
 *  live until the window closes (which terminates the JS runtime anyway).
 *  Exposed so HMR or future tear-down paths can reset cleanly. */
let _listenersCleanup: (() => void) | null = null;

/** Single shared ticker that prunes idle stream connections. One ticker
 *  per app session — previously this was per-useProxy-mount, which meant
 *  multiple intervals running in parallel when useProxy was used in 3+
 *  components. */
let _pruneTicker: ReturnType<typeof setInterval> | null = null;
let _visibilityHandler: (() => void) | null = null;

/**
 * Install the proxy event listeners exactly once per app session.
 * Idempotent; safe to call from any number of components in any order.
 */
function installProxyListeners(): Promise<void> {
  if (_listenersInstalled) return Promise.resolve();
  if (_listenersInstallPromise) return _listenersInstallPromise;

  _listenersInstallPromise = (async () => {
    const unlisteners: UnlistenFn[] = [];
    try {
      // We read the store lazily inside each handler via `.getState()` so
      // we don't need to capture actions/state refs up front. That keeps
      // the install path completely decoupled from React's render cycle.
      unlisteners.push(
        await listen<RequestEvent>("proxy:request", (event) => {
          const { isPaused, queueRequest, queueUpdate } =
            useCaptureStore.getState();
          if (isPaused) return;

          const { id, eventType, data } = event.payload;

          if (eventType === "started") {
            const request: CapturedRequest = {
              id: data.id,
              timestamp: data.timestamp,
              method: data.method,
              url: data.url,
              host: data.host,
              path: data.path,
              requestHeaders: data.requestHeaders || {},
              requestBody: null,
              responseStatus: null,
              responseHeaders: null,
              responseBody: null,
              durationMs: null,
              matchedRules: data.matchedRules || [],
              protocol: mapProtocol(data.protocol),
              tlsInfo: data.tlsVersion
                ? { version: data.tlsVersion, cipher: "", serverName: data.host }
                : null,
              contentType: data.contentType || null,
              requestSize: data.requestSize,
              responseSize: null,
              remoteAddr: data.remoteAddr || null,
            };
            queueRequest(request);
          } else if (
            eventType === "completed" ||
            eventType === "response_received"
          ) {
            queueUpdate(id, {
              responseStatus: data.responseStatus,
              responseHeaders: data.responseHeaders,
              durationMs: data.durationMs,
              matchedRules: data.matchedRules || [],
              contentType: data.contentType,
              responseSize: data.responseSize,
            });
          } else if (eventType === "error") {
            queueUpdate(id, {
              durationMs: data.durationMs,
            });
          }
        })
      );

      unlisteners.push(
        await listen<StreamMessageEventPayload>(
          "proxy:stream-message",
          (event) => {
            const { isPaused } = useCaptureStore.getState();
            if (isPaused) return;

            const { connectionId, message } = event.payload;
            useStreamStore.getState().addMessage({
              connectionId,
              message: {
                id: message.id,
                timestamp: message.timestamp,
                direction: message.direction as StreamDirection,
                messageType: message.messageType as StreamMessageType,
                data: message.data,
                isBase64: message.isBase64,
                size: message.size,
              },
            });
          }
        )
      );

      unlisteners.push(
        await listen<StreamEndedEventPayload>(
          "proxy:stream-ended",
          (event) => {
            const { connectionId, messageCount, totalBytes, durationMs, closeReason } =
              event.payload;
            useStreamStore.getState().endStream({
              connectionId,
              messageCount,
              totalBytes,
              durationMs,
              closeReason,
            });
          }
        )
      );

      _listenersInstalled = true;
      _listenersCleanup = () => {
        for (const fn of unlisteners) fn();
        _listenersInstalled = false;
        _listenersCleanup = null;
        _listenersInstallPromise = null;
      };
    } catch (error) {
      console.error("Failed to install proxy event listeners:", error);
      // Roll back whatever did install so the next call can retry.
      for (const fn of unlisteners) fn();
      _listenersInstallPromise = null;
      throw error;
    }
  })();

  return _listenersInstallPromise;
}

/** Exposed for tests / HMR. No-op if listeners aren't installed. */
export function __uninstallProxyListeners() {
  _listenersCleanup?.();
  if (_pruneTicker !== null) {
    clearInterval(_pruneTicker);
    _pruneTicker = null;
  }
  if (_visibilityHandler && typeof document !== "undefined") {
    document.removeEventListener("visibilitychange", _visibilityHandler);
    _visibilityHandler = null;
  }
}

/** Single shared ticker that prunes idle stream connections. One ticker
 *  per app session — previously this was per-useProxy-mount, which meant
 *  multiple intervals running in parallel when useProxy was used in 3+
 *  components. */
function ensurePruneTicker() {
  if (_pruneTicker !== null) return;
  // Slow cadence (60s) — pruneIdle itself is cheap (one Map walk) and
  // stream connections have minute-scale TTLs, so sub-minute ticks would
  // just be wasted wakeups, especially when the window is backgrounded
  // (browser throttles timers to ≥1s anyway, so we won't oversample).
  _pruneTicker = setInterval(() => {
    useStreamStore.getState().pruneIdle();
  }, 60 * 1000);

  // Also prune when the window comes back to the foreground. While the
  // window is minimized or in the background, Chromium throttles timers
  // aggressively (often to once per minute), so a user who leaves
  // PostGate in the background for hours and returns may briefly see a
  // backlog of long-dead SSE/WS connections before the next tick fires.
  // Running pruneIdle on visibility-change catches that up immediately.
  // Also handled by the `MAX_CONNECTIONS` cap inside `addMessage`, but
  // proactively pruning keeps the UI snappier on re-activation.
  if (typeof document !== "undefined" && !_visibilityHandler) {
    _visibilityHandler = () => {
      if (document.visibilityState === "visible") {
        useCaptureStore.getState().flushPending();
        useStreamStore.getState().pruneIdle();
      }
    };
    document.addEventListener("visibilitychange", _visibilityHandler);
  }
}

/**
 * Hook to manage proxy state and listen for events.
 * Safe to call from multiple components — listeners are installed exactly
 * once globally (see `installProxyListeners`), and auto-start also runs
 * only once across the whole app.
 */
export function useProxy() {
  // Use narrow selectors: `useProxyStore()` / `useStreamStore()` without a
  // selector subscribes the hook's consumer to the entire state, which means
  // any update (stream connections rebuilding per SSE/WS frame, proxy config
  // write, etc.) re-renders every component that calls `useProxy`. We only
  // need action refs + scalar status flags here, all of which are stable.
  const setStatus = useProxyStore((state) => state.setStatus);
  const setError = useProxyStore((state) => state.setError);
  const config = useProxyStore((state) => state.config);

  // Start proxy (with guard against concurrent calls)
  const startProxy = useCallback(async (proxyConfig?: Partial<ProxyConfig>) => {
    // Guard against concurrent start attempts
    if (_startingProxy) {
      const result = await invoke<ProxyStatusResponse>("get_proxy_status");
      setStatus(result.status);
      return result;
    }

    try {
      _startingProxy = true;
      setStatus("starting");
      setError(null);

      const finalConfig: ProxyConfig = {
        port: proxyConfig?.port ?? config.port,
        enableHttp2: proxyConfig?.enableHttp2 ?? config.enableHttp2,
        enableQuic: proxyConfig?.enableQuic ?? config.enableQuic,
        quicPort: proxyConfig?.quicPort ?? config.quicPort,
        debugPort: proxyConfig?.debugPort ?? config.debugPort,
      };

      const result = await invoke<ProxyStatusResponse>("start_proxy", {
        config: finalConfig,
      });

      setStatus(result.status);
      if (result.error) {
        setError(result.error);
      }

      return result;
    } catch (e) {
      setStatus("error");
      setError(String(e));
      throw e;
    } finally {
      _startingProxy = false;
    }
  }, [config, setStatus, setError]);

  // Stop proxy
  const stopProxy = useCallback(async () => {
    try {
      setStatus("stopping");

      const result = await invoke<ProxyStatusResponse>("stop_proxy");

      setStatus(result.status);
      return result;
    } catch (e) {
      setStatus("error");
      setError(String(e));
      throw e;
    }
  }, [setStatus, setError]);

  // Get proxy status
  const getStatus = useCallback(async () => {
    try {
      const result = await invoke<ProxyStatusResponse>("get_proxy_status");
      setStatus(result.status);
      return result;
    } catch (e) {
      console.error("Failed to get proxy status:", e);
      throw e;
    }
  }, [setStatus]);

  // Install the global proxy event listeners (once per app), and ensure
  // the idle-stream pruner is running. Both are idempotent — this effect
  // runs per-mount but the underlying work happens only once globally.
  useEffect(() => {
    installProxyListeners();
    ensurePruneTicker();
  }, []);

  // Auto-start proxy on first mount (only once globally)
  useEffect(() => {
    if (_autoStarted) return;
    _autoStarted = true;

    const init = async () => {
      try {
        const result = await getStatus();
        // Auto-start if not already running
        if (result.status === "stopped") {
          await startProxy();
        }
      } catch {
        // If status check fails, try to start anyway
        try {
          await startProxy();
        } catch (e) {
          console.error("Failed to auto-start proxy:", e);
        }
      }
    };
    init();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return {
    startProxy,
    stopProxy,
    getStatus,
  };
}

function mapProtocol(protocol: string): Protocol {
  switch (protocol.toLowerCase()) {
    case "http1":
    case "http/1.1":
      return "http1";
    case "http2":
    case "http/2":
    case "h2":
      return "http2";
    case "quic":
    case "http3":
    case "h3":
      return "quic";
    case "websocket":
    case "ws":
      return "websocket";
    case "sse":
    case "eventsource":
      return "sse";
    case "https":
      return "http1"; // Default HTTPS to HTTP/1.1
    default:
      return "http1";
  }
}

/**
 * Hook to fetch request/response bodies (with fallback to persistent storage)
 */
export function useRequestBody(requestId: string | null) {
  const getRequestBody = useCallback(async () => {
    if (!requestId) return null;
    try {
      // Try memory storage first
      let body = await invoke<number[] | null>("get_request_body", { id: requestId });
      
      // Fall back to persistent storage
      if (!body) {
        body = await invoke<number[] | null>("get_persisted_request_body", { id: requestId });
      }
      
      return body ? new Uint8Array(body) : null;
    } catch (e) {
      console.error("Failed to get request body:", e);
      return null;
    }
  }, [requestId]);

  const getResponseBody = useCallback(async () => {
    if (!requestId) return null;
    try {
      // Try memory storage first
      let body = await invoke<number[] | null>("get_response_body", { id: requestId });
      
      // Fall back to persistent storage
      if (!body) {
        body = await invoke<number[] | null>("get_persisted_response_body", { id: requestId });
      }
      
      return body ? new Uint8Array(body) : null;
    } catch (e) {
      console.error("Failed to get response body:", e);
      return null;
    }
  }, [requestId]);

  return { getRequestBody, getResponseBody };
}
