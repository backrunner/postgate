/**
 * PostGate Plugin SDK
 *
 * This SDK provides the types and utilities needed to create PostGate plugins.
 *
 * Plugins should be published to npm with the naming convention `postgate-plugin-*`.
 *
 * @example
 * ```typescript
 * import { definePlugin, PluginContext, PluginRequest, PluginResponse } from '@postgate/plugin-sdk';
 *
 * export default definePlugin({
 *   name: 'my-plugin',
 *   version: '1.0.0',
 *
 *   async onLoad(ctx) {
 *     ctx.logger.info('Plugin loaded');
 *   },
 *
 *   async handleRequest(request, ctx) {
 *     // Modify or respond to the request
 *     return null; // Return null to pass through
 *   },
 *
 *   async handleResponse(request, response, ctx) {
 *     // Modify the response
 *     return response;
 *   }
 * });
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
  onUnload?(): Promise<void>;

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
 * Plugin context provided during initialization
 */
export interface PluginContext {
  /** Persistent key-value storage for this plugin */
  storage: PluginStorage;

  /** Logger interface */
  logger: PluginLogger;

  /** UI registration interface */
  ui: PluginUI;

  /** Configuration passed via rule (e.g., plugin://name?config=value) */
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

  /** Request body (null if no body) */
  body: Uint8Array | null;

  /** Timestamp when request was received */
  timestamp: number;
}

/**
 * HTTP response returned by the plugin
 */
export interface PluginResponse {
  /** HTTP status code */
  status: number;

  /** Response headers */
  headers: Record<string, string>;

  /** Response body */
  body: Uint8Array | null;
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
  delete(key: string): Promise<void>;

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

  /** Panel title */
  title: string;

  /** Icon name (from lucide-react) */
  icon?: string;

  /** HTML content or URL to iframe */
  content: string | { type: "iframe"; url: string };
}

/**
 * Helper to define a plugin with proper typing
 */
export function definePlugin(plugin: PostGatePlugin): PostGatePlugin {
  return plugin;
}

/**
 * Helper to create a simple response
 */
export function createResponse(
  status: number,
  body: string | Uint8Array | object,
  headers: Record<string, string> = {}
): PluginResponse {
  let bodyBytes: Uint8Array | null = null;
  const responseHeaders = { ...headers };

  if (typeof body === "string") {
    bodyBytes = new TextEncoder().encode(body);
    if (!responseHeaders["content-type"]) {
      responseHeaders["content-type"] = "text/plain; charset=utf-8";
    }
  } else if (body instanceof Uint8Array) {
    bodyBytes = body;
  } else {
    bodyBytes = new TextEncoder().encode(JSON.stringify(body));
    if (!responseHeaders["content-type"]) {
      responseHeaders["content-type"] = "application/json; charset=utf-8";
    }
  }

  return {
    status,
    headers: responseHeaders,
    body: bodyBytes,
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
  };
}

/**
 * Parse request body as JSON
 */
export function parseJsonBody<T = unknown>(request: PluginRequest): T | null {
  if (!request.body) return null;

  try {
    const text = new TextDecoder().decode(request.body);
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
  return new TextDecoder().decode(request.body);
}

/**
 * Parse request body as form data
 */
export function parseFormBody(request: PluginRequest): Record<string, string> | null {
  if (!request.body) return null;

  try {
    const text = new TextDecoder().decode(request.body);
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
