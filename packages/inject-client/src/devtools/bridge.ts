import type { Transport } from "../transport/websocket";

/**
 * DevTools bridge for Chrome DevTools Protocol communication
 * This integrates with chobitsu for mobile/remote debugging
 */
export class DevToolsBridge {
  private transport: Transport;
  private chobitsu: unknown = null;
  private active = false;

  constructor(transport: Transport) {
    this.transport = transport;
  }

  /**
   * Start the DevTools bridge
   */
  async start(): Promise<void> {
    if (this.active) return;
    this.active = true;

    // Listen for CDP messages from PostGate
    this.transport.onMessage((message: unknown) => {
      if (this.isCdpMessage(message)) {
        this.handleCdpMessage(message as CdpMessage);
      }
    });

    // Initialize chobitsu if available
    await this.initChobitsu();

    // Send ready message
    this.transport.send({
      type: "devtools",
      action: "ready",
      timestamp: Date.now(),
    });
  }

  /**
   * Stop the DevTools bridge
   */
  stop(): void {
    this.active = false;
    this.chobitsu = null;
  }

  private isCdpMessage(message: unknown): boolean {
    return (
      typeof message === "object" &&
      message !== null &&
      "type" in message &&
      (message as Record<string, unknown>).type === "cdp"
    );
  }

  private async initChobitsu(): Promise<void> {
    // Check if chobitsu is already loaded
    if ((window as unknown as Record<string, unknown>).chobitsu) {
      this.chobitsu = (window as unknown as Record<string, unknown>).chobitsu;
      return;
    }

    // Try to load chobitsu dynamically
    // For now, we'll provide a basic implementation
    // Full chobitsu integration would require including the library
    console.log("[PostGate] DevTools bridge initialized (basic mode)");
  }

  private handleCdpMessage(message: CdpMessage): void {
    if (!this.active) return;

    const { id, method, params } = message;

    // Handle some basic CDP methods natively
    let result: unknown = null;
    let error: unknown = null;

    try {
      switch (method) {
        case "Runtime.evaluate":
          result = this.handleRuntimeEvaluate(params ?? {});
          break;

        case "Page.reload":
          window.location.reload();
          result = {};
          break;

        case "Page.navigate":
          if (params?.url) {
            window.location.href = params.url as string;
          }
          result = { frameId: "main" };
          break;

        case "DOM.getDocument":
          result = {
            root: {
              nodeId: 1,
              nodeType: 9,
              nodeName: "#document",
              localName: "",
              nodeValue: "",
              childNodeCount: document.childNodes.length,
            },
          };
          break;

        case "Network.enable":
        case "Page.enable":
        case "Runtime.enable":
        case "DOM.enable":
        case "CSS.enable":
        case "Overlay.enable":
          result = {};
          break;

        default:
          // Forward to chobitsu if available
          if (this.chobitsu) {
            // Would call chobitsu here
            error = { code: -32601, message: `Method not found: ${method}` };
          } else {
            error = { code: -32601, message: `Method not found: ${method}` };
          }
      }
    } catch (e) {
      error = { code: -32603, message: String(e) };
    }

    // Send response
    this.transport.send({
      type: "cdp-response",
      id,
      result,
      error,
    });
  }

  private handleRuntimeEvaluate(params: Record<string, unknown>): unknown {
    const expression = params?.expression as string;
    if (!expression) {
      return { result: { type: "undefined" } };
    }

    try {
      // eslint-disable-next-line no-eval
      const value = eval(expression);
      return {
        result: this.serializeValue(value),
      };
    } catch (e) {
      return {
        exceptionDetails: {
          exceptionId: 1,
          text: String(e),
          lineNumber: 0,
          columnNumber: 0,
          exception: {
            type: "object",
            subtype: "error",
            className: "Error",
            description: String(e),
          },
        },
      };
    }
  }

  private serializeValue(value: unknown): Record<string, unknown> {
    if (value === undefined) {
      return { type: "undefined" };
    }
    if (value === null) {
      return { type: "object", subtype: "null", value: null };
    }
    if (typeof value === "boolean") {
      return { type: "boolean", value };
    }
    if (typeof value === "number") {
      return { type: "number", value, description: String(value) };
    }
    if (typeof value === "string") {
      return { type: "string", value };
    }
    if (typeof value === "function") {
      return { type: "function", className: "Function", description: value.toString() };
    }
    if (Array.isArray(value)) {
      return {
        type: "object",
        subtype: "array",
        className: "Array",
        description: `Array(${value.length})`,
      };
    }
    if (typeof value === "object") {
      return {
        type: "object",
        className: value.constructor?.name || "Object",
        description: String(value),
      };
    }
    return { type: "undefined" };
  }
}

interface CdpMessage {
  type: "cdp";
  id: number;
  method: string;
  params?: Record<string, unknown>;
}
