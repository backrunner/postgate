export interface Transport {
  connect(): Promise<void>;
  disconnect(): void;
  send(message: unknown): void;
  onMessage(callback: (message: unknown) => void): void;
  isConnected(): boolean;
}

export interface WebSocketTransportOptions {
  endpoint: string;
  sessionName: string;
  autoReconnect?: boolean;
  reconnectInterval?: number;
}

export class WebSocketTransport implements Transport {
  private ws: WebSocket | null = null;
  private options: Required<WebSocketTransportOptions>;
  private messageQueue: unknown[] = [];
  private messageCallbacks: ((message: unknown) => void)[] = [];
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  private connected = false;
  private shouldReconnect = true;

  constructor(options: WebSocketTransportOptions) {
    this.options = {
      endpoint: options.endpoint,
      sessionName: options.sessionName,
      autoReconnect: options.autoReconnect ?? true,
      reconnectInterval: options.reconnectInterval ?? 3000,
    };
  }

  async connect(): Promise<void> {
    return new Promise((resolve, reject) => {
      this.shouldReconnect = true;

      try {
        this.ws = new WebSocket(this.options.endpoint);

        this.ws.onopen = () => {
          this.connected = true;

          // Send session info
          this.send({
            type: "session",
            name: this.options.sessionName,
            url: window.location.href,
            userAgent: navigator.userAgent,
            timestamp: Date.now(),
          });

          // Flush queued messages
          while (this.messageQueue.length > 0) {
            const msg = this.messageQueue.shift();
            if (msg) {
              this.send(msg);
            }
          }

          resolve();
        };

        this.ws.onclose = () => {
          this.connected = false;
          this.ws = null;

          if (this.shouldReconnect && this.options.autoReconnect) {
            this.scheduleReconnect();
          }
        };

        this.ws.onerror = (event) => {
          console.error("[PostGate] WebSocket error:", event);
          if (!this.connected) {
            reject(new Error("WebSocket connection failed"));
          }
        };

        this.ws.onmessage = (event) => {
          try {
            const message = JSON.parse(event.data);
            for (const callback of this.messageCallbacks) {
              callback(message);
            }
          } catch (e) {
            console.error("[PostGate] Failed to parse message:", e);
          }
        };
      } catch (e) {
        reject(e);
      }
    });
  }

  disconnect(): void {
    this.shouldReconnect = false;

    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }

    if (this.ws) {
      this.ws.close();
      this.ws = null;
    }

    this.connected = false;
  }

  send(message: unknown): void {
    if (!this.connected || !this.ws) {
      // Queue message for later
      this.messageQueue.push(message);

      // Limit queue size
      if (this.messageQueue.length > 1000) {
        this.messageQueue.shift();
      }
      return;
    }

    try {
      this.ws.send(JSON.stringify(message));
    } catch (e) {
      console.error("[PostGate] Failed to send message:", e);
      this.messageQueue.push(message);
    }
  }

  onMessage(callback: (message: unknown) => void): void {
    this.messageCallbacks.push(callback);
  }

  isConnected(): boolean {
    return this.connected;
  }

  private scheduleReconnect(): void {
    if (this.reconnectTimer) return;

    this.reconnectTimer = setTimeout(() => {
      this.reconnectTimer = null;
      this.connect().catch((e) => {
        console.error("[PostGate] Reconnect failed:", e);
      });
    }, this.options.reconnectInterval);
  }
}
