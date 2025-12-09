import { ConsoleCapture } from "./console/capture";
import { Transport, WebSocketTransport } from "./transport/websocket";
import { DevToolsBridge } from "./devtools/bridge";

export interface PostGateClientOptions {
  /** PostGate WebSocket endpoint */
  endpoint: string;
  /** Session name for identification */
  sessionName?: string;
  /** Enable console capture */
  captureConsole?: boolean;
  /** Enable DevTools bridge */
  enableDevTools?: boolean;
  /** Reconnect on disconnect */
  autoReconnect?: boolean;
  /** Reconnect interval in ms */
  reconnectInterval?: number;
}

export class PostGateClient {
  private transport: Transport;
  private consoleCapture: ConsoleCapture | null = null;
  private devToolsBridge: DevToolsBridge | null = null;
  private options: Required<PostGateClientOptions>;

  constructor(options: PostGateClientOptions) {
    this.options = {
      endpoint: options.endpoint,
      sessionName: options.sessionName ?? document.title ?? "Unknown",
      captureConsole: options.captureConsole ?? true,
      enableDevTools: options.enableDevTools ?? false,
      autoReconnect: options.autoReconnect ?? true,
      reconnectInterval: options.reconnectInterval ?? 3000,
    };

    this.transport = new WebSocketTransport({
      endpoint: this.options.endpoint,
      sessionName: this.options.sessionName,
      autoReconnect: this.options.autoReconnect,
      reconnectInterval: this.options.reconnectInterval,
    });
  }

  /**
   * Start the client and connect to PostGate
   */
  async start(): Promise<void> {
    // Connect transport
    await this.transport.connect();

    // Start console capture if enabled
    if (this.options.captureConsole) {
      this.consoleCapture = new ConsoleCapture(this.transport);
      this.consoleCapture.start();
    }

    // Start DevTools bridge if enabled
    if (this.options.enableDevTools) {
      this.devToolsBridge = new DevToolsBridge(this.transport);
      await this.devToolsBridge.start();
    }

    console.log("[PostGate] Client started");
  }

  /**
   * Stop the client and disconnect
   */
  stop(): void {
    if (this.consoleCapture) {
      this.consoleCapture.stop();
      this.consoleCapture = null;
    }

    if (this.devToolsBridge) {
      this.devToolsBridge.stop();
      this.devToolsBridge = null;
    }

    this.transport.disconnect();
    console.log("[PostGate] Client stopped");
  }

  /**
   * Get the transport for external use
   */
  getTransport(): Transport {
    return this.transport;
  }
}

// Auto-initialize if configured via script data attributes
if (typeof document !== "undefined") {
  const script = document.currentScript as HTMLScriptElement | null;
  if (script) {
    const endpoint = script.dataset.postgate;
    const sessionName = script.dataset.session;
    const captureConsole = script.dataset.console !== "false";
    const enableDevTools = script.dataset.devtools === "true";

    if (endpoint) {
      const client = new PostGateClient({
        endpoint,
        sessionName,
        captureConsole,
        enableDevTools,
      });

      // Start automatically
      client.start().catch((err) => {
        console.error("[PostGate] Failed to start:", err);
      });

      // Expose globally
      (window as unknown as Record<string, unknown>).__POSTGATE_CLIENT__ = client;
    }
  }
}

export { ConsoleCapture } from "./console/capture";
export { WebSocketTransport } from "./transport/websocket";
export { DevToolsBridge } from "./devtools/bridge";
export type { Transport } from "./transport/websocket";
