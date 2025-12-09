/**
 * PostGate Plugin SDK
 *
 * This SDK provides TypeScript types and utilities for creating PostGate plugins.
 *
 * **Important**: Plugins run in an embedded V8 runtime (Deno Core), not Node.js.
 * The runtime provides global APIs via the `PostGate` namespace:
 * - `PostGate.storage` - Persistent key-value storage
 * - `PostGate.ui` - UI registration (panels, toasts)
 * - `PostGate.createLogger()` - Logging utilities
 *
 * This SDK is optional - you can use it for TypeScript type checking,
 * or write plugins in plain JavaScript using the global APIs directly.
 *
 * Plugins should be published to npm with the naming convention `postgate-plugin-*`.
 *
 * @example
 * ```typescript
 * // TypeScript with SDK types
 * import type { PostGatePlugin, PluginContext, PluginRequest, PluginResponse } from '@postgate/plugin-sdk';
 *
 * const plugin: PostGatePlugin = {
 *   name: 'my-plugin',
 *   version: '1.0.0',
 *
 *   async onLoad(ctx) {
 *     ctx.logger.info('Plugin loaded');
 *     ctx.ui.registerPanel({
 *       id: 'my-panel',
 *       plugin_id: 'my-plugin',
 *       title: 'My Panel',
 *       content: { type: 'html', html: '<h1>Hello</h1>' }
 *     });
 *   },
 *
 *   async handleRequest(request, ctx) {
 *     // Return a response to short-circuit, or null to pass through
 *     return null;
 *   },
 *
 *   async handleResponse(request, response, ctx) {
 *     // Modify and return the response
 *     return response;
 *   }
 * };
 *
 * export default plugin;
 * ```
 *
 * @example
 * ```javascript
 * // Plain JavaScript (no SDK needed)
 * export default {
 *   name: 'my-plugin',
 *   version: '1.0.0',
 *
 *   async onLoad(ctx) {
 *     ctx.logger.info('Plugin loaded');
 *   },
 *
 *   async handleRequest(request, ctx) {
 *     return {
 *       status: 200,
 *       headers: { 'content-type': 'application/json' },
 *       body: btoa(JSON.stringify({ hello: 'world' })),
 *       body_base64: true
 *     };
 *   }
 * };
 * ```
 */

export interface PostGatePlugin {
  /** Plugin name (must match npm package name without postgate-plugin- prefix) */
  name: string;

  /** Plugin version (semver) */
  version: string;

  /** Human-readable description */
  description?: string;

  /** Plugin author */
  author?: string;

  /**
   * Called when the plugin is loaded
   * Use this to initialize resources, register UI panels, etc.
   */
  onLoad?(context: PluginContext): Promise<void>;

  /**
   * Called when the plugin is unloaded
   * Use this to clean up resources
   */
  onUnload?(context: PluginContext): Promise<void>;

  /**
   * Handle an incoming request that matches this plugin's rule
   * Return a response to short-circuit the request, or null to pass through
   */
  handleRequest?(
    request: PluginRequest,
    context: RequestContext
  ): Promise<PluginResponse | null>;

  /**
   * Handle/modify a response before it's sent to the client
   * Return the (possibly modified) response
   */
  handleResponse?(
    request: PluginRequest,
    response: PluginResponse,
    context: RequestContext
  ): Promise<PluginResponse>;
}

/**
 * Plugin context provided during initialization and lifecycle events
 */
export interface PluginContext {
  /** Persistent key-value storage for this plugin */
  storage: PluginStorage;

  /** Logger interface */
  logger: PluginLogger;

  /** UI registration interface */
  ui: PluginUI;

  /** Configuration passed via rule or plugin settings */
  config: Record<string, string>;
}

/**
 * Context for handling individual requests
 */
export interface RequestContext {
  /** Configuration from the matched rule */
  ruleConfig: Record<string, unknown>;

  /** Matched rule pattern */
  matchedPattern: string;

  /** Logger scoped to this request */
  logger: PluginLogger;
}

/**
 * HTTP request received by the plugin
 */
export interface PluginRequest {
  /** Unique request ID */
  id: string;

  /** HTTP method */
  method: string;

  /** Full URL */
  url: string;

  /** Hostname */
  host: string;

  /** Path (without query string) */
  path: string;

  /** Query string parameters */
  query: Record<string, string>;

  /** Request headers (lowercase keys) */
  headers: Record<string, string>;

  /**
   * Request body
   * - If `body_base64` is true, this is a base64-encoded string
   * - Otherwise, it's the raw body string
   */
  body: string | null;

  /** Whether the body is base64 encoded */
  body_base64: boolean;

  /** Timestamp when request was received (milliseconds since epoch) */
  timestamp: number;
}

/**
 * HTTP response returned by or to the plugin
 */
export interface PluginResponse {
  /** HTTP status code */
  status: number;

  /** Response headers */
  headers: Record<string, string>;

  /**
   * Response body
   * - If `body_base64` is true, this should be a base64-encoded string
   * - Otherwise, it's the raw body string
   */
  body: string | null;

  /** Whether the body is base64 encoded */
  body_base64: boolean;
}

/**
 * Persistent storage interface
 */
export interface PluginStorage {
  /** Get a value by key */
  get<T = unknown>(key: string): Promise<T | null>;

  /** Set a value */
  set<T = unknown>(key: string, value: T): Promise<void>;

  /** Delete a value */
  delete(key: string): Promise<boolean>;

  /** Check if a key exists */
  has(key: string): Promise<boolean>;

  /** List all keys */
  keys(): Promise<string[]>;

  /** Clear all stored data */
  clear(): Promise<void>;
}

/**
 * Logger interface
 */
export interface PluginLogger {
  debug(message: string, ...args: unknown[]): void;
  info(message: string, ...args: unknown[]): void;
  warn(message: string, ...args: unknown[]): void;
  error(message: string, ...args: unknown[]): void;
}

/**
 * UI registration interface
 */
export interface PluginUI {
  /**
   * Register a panel in the PostGate UI
   */
  registerPanel(panel: UIPanel): void;

  /**
   * Unregister a panel
   */
  unregisterPanel(id: string): void;

  /**
   * Show a toast notification
   */
  toast(message: string, type?: "info" | "success" | "warning" | "error"): void;
}

/**
 * UI Panel definition
 */
export interface UIPanel {
  /** Unique panel ID */
  id: string;

  /** Plugin ID that owns this panel */
  plugin_id: string;

  /** Panel title */
  title: string;

  /** Icon name (from lucide-react) */
  icon?: string;

  /** Panel content - either HTML or an iframe URL */
  content: UIPanelContent;
}

/**
 * Panel content types
 */
export type UIPanelContent =
  | { type: "html"; html: string }
  | { type: "iframe"; url: string };

// ============================================================================
// Helper functions
// ============================================================================

/**
 * Helper to define a plugin with proper typing
 */
export function definePlugin(plugin: PostGatePlugin): PostGatePlugin {
  return plugin;
}

/**
 * Helper to create a response with base64-encoded body
 */
export function createResponse(
  status: number,
  body: string | Uint8Array | object,
  headers: Record<string, string> = {}
): PluginResponse {
  const responseHeaders = { ...headers };
  let bodyStr: string | null = null;
  let isBase64 = false;

  if (body === null || body === undefined) {
    bodyStr = null;
  } else if (typeof body === "string") {
    // Encode string as base64
    bodyStr = stringToBase64(body);
    isBase64 = true;
    if (!responseHeaders["content-type"]) {
      responseHeaders["content-type"] = "text/plain; charset=utf-8";
    }
  } else if (body instanceof Uint8Array) {
    // Encode bytes as base64
    bodyStr = uint8ArrayToBase64(body);
    isBase64 = true;
  } else {
    // JSON object - encode as base64
    bodyStr = stringToBase64(JSON.stringify(body));
    isBase64 = true;
    if (!responseHeaders["content-type"]) {
      responseHeaders["content-type"] = "application/json; charset=utf-8";
    }
  }

  return {
    status,
    headers: responseHeaders,
    body: bodyStr,
    body_base64: isBase64,
  };
}

/**
 * Helper to create a JSON response
 */
export function jsonResponse(
  data: unknown,
  status = 200,
  headers: Record<string, string> = {}
): PluginResponse {
  return createResponse(status, JSON.stringify(data), {
    "content-type": "application/json; charset=utf-8",
    ...headers,
  });
}

/**
 * Helper to create an HTML response
 */
export function htmlResponse(
  html: string,
  status = 200,
  headers: Record<string, string> = {}
): PluginResponse {
  return createResponse(status, html, {
    "content-type": "text/html; charset=utf-8",
    ...headers,
  });
}

/**
 * Helper to create a redirect response
 */
export function redirectResponse(
  url: string,
  status: 301 | 302 | 303 | 307 | 308 = 302
): PluginResponse {
  return {
    status,
    headers: { location: url },
    body: null,
    body_base64: false,
  };
}

/**
 * Parse request body as JSON
 */
export function parseJsonBody<T = unknown>(request: PluginRequest): T | null {
  if (!request.body) return null;

  try {
    const text = request.body_base64
      ? base64ToString(request.body)
      : request.body;
    return JSON.parse(text) as T;
  } catch {
    return null;
  }
}

/**
 * Parse request body as text
 */
export function parseTextBody(request: PluginRequest): string | null {
  if (!request.body) return null;
  return request.body_base64 ? base64ToString(request.body) : request.body;
}

/**
 * Parse request body as form data
 */
export function parseFormBody(
  request: PluginRequest
): Record<string, string> | null {
  if (!request.body) return null;

  try {
    const text = request.body_base64
      ? base64ToString(request.body)
      : request.body;
    const params = new URLSearchParams(text);
    const result: Record<string, string> = {};

    for (const [key, value] of params) {
      result[key] = value;
    }

    return result;
  } catch {
    return null;
  }
}

/**
 * Get response body as text
 */
export function getResponseBodyText(response: PluginResponse): string | null {
  if (!response.body) return null;
  return response.body_base64
    ? base64ToString(response.body)
    : response.body;
}

/**
 * Get response body as bytes
 */
export function getResponseBodyBytes(response: PluginResponse): Uint8Array | null {
  if (!response.body) return null;
  return response.body_base64
    ? base64ToUint8Array(response.body)
    : new TextEncoder().encode(response.body);
}

// ============================================================================
// Internal utilities
// ============================================================================

/**
 * Convert string to base64
 * Works in both browser and Deno Core runtime
 */
function stringToBase64(str: string): string {
  if (typeof btoa === "function") {
    // Use built-in btoa (available in our Deno Core runtime)
    return btoa(unescape(encodeURIComponent(str)));
  }
  // Fallback for environments without btoa
  const bytes = new TextEncoder().encode(str);
  return uint8ArrayToBase64(bytes);
}

/**
 * Convert base64 to string
 */
function base64ToString(base64: string): string {
  if (typeof atob === "function") {
    return decodeURIComponent(escape(atob(base64)));
  }
  // Fallback
  const bytes = base64ToUint8Array(base64);
  return new TextDecoder().decode(bytes);
}

/**
 * Convert Uint8Array to base64
 */
function uint8ArrayToBase64(bytes: Uint8Array): string {
  if (typeof btoa === "function") {
    let binary = "";
    for (let i = 0; i < bytes.length; i++) {
      binary += String.fromCharCode(bytes[i]);
    }
    return btoa(binary);
  }
  // Fallback implementation
  const chars =
    "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
  let result = "";
  for (let i = 0; i < bytes.length; i += 3) {
    const a = bytes[i];
    const b = bytes[i + 1];
    const c = bytes[i + 2];
    result += chars[a >> 2];
    result += chars[((a & 3) << 4) | (b >> 4)];
    result += chars[b === undefined ? 64 : ((b & 15) << 2) | (c >> 6)];
    result += chars[c === undefined ? 64 : c & 63];
  }
  return result;
}

/**
 * Convert base64 to Uint8Array
 */
function base64ToUint8Array(base64: string): Uint8Array {
  if (typeof atob === "function") {
    const binary = atob(base64);
    const bytes = new Uint8Array(binary.length);
    for (let i = 0; i < binary.length; i++) {
      bytes[i] = binary.charCodeAt(i);
    }
    return bytes;
  }
  // Fallback implementation
  const chars =
    "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
  const cleanBase64 = base64.replace(/=+$/, "");
  const bytes: number[] = [];
  for (let i = 0; i < cleanBase64.length; i += 4) {
    const a = chars.indexOf(cleanBase64[i]);
    const b = chars.indexOf(cleanBase64[i + 1]);
    const c = chars.indexOf(cleanBase64[i + 2]);
    const d = chars.indexOf(cleanBase64[i + 3]);
    bytes.push((a << 2) | (b >> 4));
    if (c !== -1) bytes.push(((b & 15) << 4) | (c >> 2));
    if (d !== -1) bytes.push(((c & 3) << 6) | d);
  }
  return new Uint8Array(bytes);
}

// ============================================================================
// Global type declarations for Deno Core runtime
// ============================================================================

/**
 * Global PostGate namespace available in the plugin runtime
 * This is provided by the Deno Core runtime, not by this SDK
 */
declare global {
  const PostGate: {
    storage: PluginStorage;
    ui: PluginUI;
    createLogger: (pluginId?: string) => PluginLogger;
    createContext: (config?: Record<string, string>) => PluginContext;
    _internal: {
      sendResponse: (requestId: string, response: PluginResponse | null) => void;
      sendModifiedResponse: (requestId: string, response: PluginResponse) => void;
      pluginLoaded: () => void;
      pluginError: (message: string) => void;
    };
  };

  // Base64 functions provided by the runtime
  function btoa(str: string): string;
  function atob(str: string): string;
}
