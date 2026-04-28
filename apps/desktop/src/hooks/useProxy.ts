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

/**
 * Hook to manage proxy state and listen for events.
 * Safe to call from multiple components - event listeners are set up per instance,
 * but auto-start only happens once globally.
 */
export function useProxy() {
  const { setStatus, setError, config } = useProxyStore();
  const queueRequest = useCaptureStore((state) => state.queueRequest);
  const queueUpdate = useCaptureStore((state) => state.queueUpdate);
  const isPaused = useCaptureStore((state) => state.isPaused);
  const { addMessage, endStream } = useStreamStore();

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

  // Listen for proxy events
  useEffect(() => {
    // `cancelled` guards against the race where the effect is cleaned up
    // before any of the `await listen()` promises resolve. Without this,
    // an in-flight listen() can install a listener AFTER cleanup ran,
    // leaving it orphaned for the rest of the page's lifetime — i.e. a
    // permanent per-remount leak.
    let cancelled = false;
    const unlisteners: UnlistenFn[] = [];

    const register = async (fn: Promise<UnlistenFn>) => {
      const unlisten = await fn;
      if (cancelled) {
        // Effect already torn down — drop the listener we just installed.
        unlisten();
        return;
      }
      unlisteners.push(unlisten);
    };

    // Fire-and-forget; errors are logged but shouldn't block the others.
    Promise.all([
      register(
        listen<RequestEvent>("proxy:request", (event) => {
          if (isPaused) return;

          const { id, eventType, data } = event.payload;

          if (eventType === "started") {
            // Add new request (queued for batching)
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
          } else if (eventType === "completed" || eventType === "response_received") {
            // Update existing request (queued for batching)
            queueUpdate(id, {
              responseStatus: data.responseStatus,
              responseHeaders: data.responseHeaders,
              durationMs: data.durationMs,
              matchedRules: data.matchedRules || [],
              contentType: data.contentType,
              responseSize: data.responseSize,
            });
          } else if (eventType === "error") {
            // Update with error (queued for batching)
            queueUpdate(id, {
              durationMs: data.durationMs,
            });
          }
        })
      ),
      register(
        listen<StreamMessageEventPayload>("proxy:stream-message", (event) => {
          if (isPaused) return;

          const { connectionId, message } = event.payload;
          addMessage({
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
        })
      ),
      register(
        listen<StreamEndedEventPayload>("proxy:stream-ended", (event) => {
          const { connectionId, messageCount, totalBytes, durationMs, closeReason } = event.payload;
          endStream({
            connectionId,
            messageCount,
            totalBytes,
            durationMs,
            closeReason,
          });
        })
      ),
    ]).catch((error) => {
      console.error("Failed to register proxy event listeners:", error);
    });

    // Prune idle stream connections on a slow cadence. The store tracks
    // per-connection `lastActivityAt`; ending a SSE/WS stream doesn't
    // automatically drop it (the user may still want to inspect history),
    // so without this sweep ended streams linger until the user manually
    // clears, which is the dominant leak path for long-lived tabs.
    const pruneInterval = setInterval(() => {
      useStreamStore.getState().pruneIdle();
    }, 60 * 1000);

    return () => {
      cancelled = true;
      clearInterval(pruneInterval);
      for (const unlisten of unlisteners) unlisten();
    };
  }, [queueRequest, queueUpdate, addMessage, endStream, isPaused]);

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
