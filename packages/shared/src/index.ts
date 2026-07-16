/**
 * Shared types and utilities for PostGate
 */

// ============================================================================
// Protocol Types
// ============================================================================

export type Protocol = "http1" | "http2" | "quic" | "websocket" | "sse";

// ============================================================================
// Proxy Types
// ============================================================================

export type ProxyStatus = "stopped" | "starting" | "running" | "stopping" | "error";

export interface ProxyConfig {
  port: number;
  enableHttp2: boolean;
  enableQuic: boolean;
  quicPort: number | null;
}

export interface ProxyStatusResponse {
  status: ProxyStatus;
  port: number;
  error: string | null;
}

// ============================================================================
// Request Types
// ============================================================================

export interface TlsInfo {
  version: string;
  cipher: string;
  serverName: string;
}

export interface CapturedRequest {
  id: string;
  timestamp: number;
  method: string;
  url: string;
  host: string;
  path: string;
  requestHeaders: Record<string, string>;
  requestBody: Uint8Array | null;
  responseStatus: number | null;
  responseHeaders: Record<string, string> | null;
  responseBody: Uint8Array | null;
  durationMs: number | null;
  matchedRules: string[];
  protocol: Protocol;
  tlsInfo: TlsInfo | null;
  contentType: string | null;
  requestSize: number;
  responseSize: number | null;
  error: string | null;
}

export interface CapturedRequestEvent {
  id: string;
  eventType: "started" | "response_received" | "completed" | "error";
  data: Partial<CapturedRequest>;
}

// ============================================================================
// Rule Types
// ============================================================================

export interface RuleGroup {
  id: string;
  name: string;
  folder?: string | null;
  enabled: boolean;
  priority: number;
  rules: Rule[];
  rawContent: string;
  createdAt: number;
  updatedAt: number;
}

export interface Rule {
  id: string;
  pattern: Pattern;
  actions: RuleAction[];
  enabled: boolean;
  priority: number;
  rawLine: string;
}

export type Pattern =
  | { type: "exact"; value: string }
  | { type: "wildcard"; value: string }
  | { type: "regex"; value: string }
  | { type: "path_prefix"; value: string }
  | { type: "all" };

export type RuleAction =
  | { type: "host"; target: string }
  | { type: "file"; path: string }
  | { type: "redirect"; url: string; status: number }
  | { type: "status_code"; code: number }
  | { type: "request_headers"; modifications: HeaderModifications }
  | { type: "response_headers"; modifications: HeaderModifications }
  | { type: "request_body"; content: BodyContent }
  | { type: "response_body"; content: BodyContent }
  | { type: "html_append"; content: string }
  | { type: "html_prepend"; content: string }
  | { type: "js_append"; content: string }
  | { type: "js_prepend"; content: string }
  | { type: "css_append"; content: string }
  | { type: "css_prepend"; content: string }
  | { type: "delay"; requestMs: number | null; responseMs: number | null }
  | { type: "speed"; kbps: number }
  | { type: "debug"; name: string }
  | { type: "plugin"; name: string; config: unknown };

export interface HeaderModifications {
  set: Record<string, string>;
  remove: string[];
  append: Record<string, string>;
}

export type BodyContent =
  | { type: "text"; content: string; contentType: string }
  | { type: "json"; value: unknown }
  | { type: "file"; path: string }
  | { type: "base64"; data: string };

// ============================================================================
// Replay Types
// ============================================================================

export interface SavedRequest {
  id: string;
  name: string;
  collectionId: string | null;
  method: string;
  url: string;
  headers: KeyValuePair[];
  queryParams: KeyValuePair[];
  body: RequestBody;
  createdAt: number;
  updatedAt: number;
}

export interface KeyValuePair {
  key: string;
  value: string;
  enabled: boolean;
}

export type RequestBody =
  | { type: "none" }
  | { type: "raw"; content: string; contentType: string }
  | { type: "form-data"; items: FormDataItem[] }
  | { type: "x-www-form-urlencoded"; items: KeyValuePair[] }
  | { type: "binary"; path: string };

export interface FormDataItem {
  key: string;
  value: string;
  type: "text" | "file";
  enabled: boolean;
}

export interface Collection {
  id: string;
  name: string;
  parentId: string | null;
  createdAt: number;
  updatedAt: number;
}

// ============================================================================
// Debug Types
// ============================================================================

export interface DebugSession {
  id: string;
  name: string;
  url: string;
  userAgent: string;
  connectedAt: number;
  status: "connected" | "disconnected";
}

export interface ConsoleLog {
  id: string;
  sessionId: string;
  method: ConsoleMethod;
  args: unknown[];
  timestamp: number;
  stack: string | null;
  url: string;
}

export type ConsoleMethod =
  | "log"
  | "warn"
  | "error"
  | "info"
  | "debug"
  | "trace"
  | "assert"
  | "clear"
  | "count"
  | "countReset"
  | "group"
  | "groupCollapsed"
  | "groupEnd"
  | "table"
  | "time"
  | "timeEnd"
  | "timeLog";

// ============================================================================
// Plugin Types
// ============================================================================

export interface PluginInfo {
  name: string;
  version: string;
  description: string | null;
  author: string | null;
  enabled: boolean;
  path: string;
}

// ============================================================================
// Certificate Types
// ============================================================================

export interface CertificateInfo {
  installed: boolean;
  pem: string;
  fingerprint?: string;
  validFrom?: string;
  validTo?: string;
}

// ============================================================================
// Filter Types
// ============================================================================

export interface FilterOptions {
  search: string;
  methods: string[];
  statusCodes: string[];
  contentTypes: string[];
  hosts: string[];
  hasRules: boolean | null;
  protocols: Protocol[];
}

// ============================================================================
// Utility Types
// ============================================================================

export type DeepPartial<T> = {
  [P in keyof T]?: T[P] extends object ? DeepPartial<T[P]> : T[P];
};

export type Result<T, E = Error> =
  | { ok: true; value: T }
  | { ok: false; error: E };
