import { useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import { useProxyStore } from "@/stores/proxy";
import { useCaptureStore, CapturedRequest, Protocol } from "@/stores/capture";

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

/**
 * Hook to manage proxy state and listen for events
 */
export function useProxy() {
  const { setStatus, setError, config } = useProxyStore();
  const { addRequest, updateRequest, isPaused, loadHistory, historyLoaded } = useCaptureStore();

  // Start proxy
  const startProxy = useCallback(async (proxyConfig?: Partial<ProxyConfig>) => {
    try {
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
    let unlistenFn: UnlistenFn | null = null;

    const setupListener = async () => {
      unlistenFn = await listen<RequestEvent>("proxy:request", (event) => {
        if (isPaused) return;

        const { id, eventType, data } = event.payload;

        if (eventType === "started") {
          // Add new request
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
          addRequest(request);
        } else if (eventType === "completed" || eventType === "response_received") {
          // Update existing request
          updateRequest(id, {
            responseStatus: data.responseStatus,
            responseHeaders: data.responseHeaders,
            durationMs: data.durationMs,
            matchedRules: data.matchedRules || [],
            contentType: data.contentType,
            responseSize: data.responseSize,
          });
        } else if (eventType === "error") {
          // Update with error
          updateRequest(id, {
            durationMs: data.durationMs,
          });
        }
      });
    };

    setupListener();

    return () => {
      if (unlistenFn) {
        unlistenFn();
      }
    };
  }, [addRequest, updateRequest, isPaused]);

  // Get initial status on mount and load history
  useEffect(() => {
    getStatus();
    // Load captured history on first mount
    if (!historyLoaded) {
      loadHistory();
    }
  }, [getStatus, historyLoaded, loadHistory]);

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
