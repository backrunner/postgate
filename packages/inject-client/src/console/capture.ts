import type { Transport } from "../transport/websocket";
import { serialize } from "../utils/serializer";
import { getStackTrace } from "../utils/stack-trace";

export type ConsoleMethod = "log" | "warn" | "error" | "info" | "debug" | "trace" | "assert" | "clear" | "count" | "countReset" | "group" | "groupCollapsed" | "groupEnd" | "table" | "time" | "timeEnd" | "timeLog";

const CAPTURED_METHODS: ConsoleMethod[] = [
  "log",
  "warn",
  "error",
  "info",
  "debug",
  "trace",
  "assert",
  "clear",
  "count",
  "countReset",
  "group",
  "groupCollapsed",
  "groupEnd",
  "table",
  "time",
  "timeEnd",
  "timeLog",
];

export interface ConsoleMessage {
  type: "console";
  method: ConsoleMethod;
  args: unknown[];
  timestamp: number;
  stack?: string;
  url: string;
}

/**
 * Captures console output and sends it to PostGate
 */
export class ConsoleCapture {
  private transport: Transport;
  private originalMethods: Partial<Record<ConsoleMethod, (...args: unknown[]) => void>> = {};
  private timers: Map<string, number> = new Map();
  private counters: Map<string, number> = new Map();
  private active = false;

  constructor(transport: Transport) {
    this.transport = transport;
  }

  /**
   * Start capturing console output
   */
  start(): void {
    if (this.active) return;
    this.active = true;

    for (const method of CAPTURED_METHODS) {
      this.captureMethod(method);
    }
  }

  /**
   * Stop capturing and restore original console
   */
  stop(): void {
    if (!this.active) return;
    this.active = false;

    for (const method of CAPTURED_METHODS) {
      if (this.originalMethods[method]) {
        (console as unknown as Record<string, unknown>)[method] = this.originalMethods[method];
      }
    }

    this.originalMethods = {};
    this.timers.clear();
    this.counters.clear();
  }

  private captureMethod(method: ConsoleMethod): void {
    const original = console[method] as (...args: unknown[]) => void;
    this.originalMethods[method] = original;

    const self = this;

    (console as unknown as Record<string, (...args: unknown[]) => void>)[method] = function (...args: unknown[]) {
      // Call original
      if (original) {
        original.apply(console, args);
      }

      // Handle special methods
      let processedArgs = args;

      switch (method) {
        case "time":
          if (typeof args[0] === "string") {
            self.timers.set(args[0], performance.now());
          }
          return;

        case "timeEnd":
        case "timeLog":
          if (typeof args[0] === "string") {
            const start = self.timers.get(args[0]);
            if (start !== undefined) {
              const duration = performance.now() - start;
              processedArgs = [`${args[0]}: ${duration.toFixed(2)}ms`];
              if (method === "timeEnd") {
                self.timers.delete(args[0]);
              }
            }
          }
          break;

        case "count":
          if (typeof args[0] === "string") {
            const count = (self.counters.get(args[0]) || 0) + 1;
            self.counters.set(args[0], count);
            processedArgs = [`${args[0]}: ${count}`];
          }
          break;

        case "countReset":
          if (typeof args[0] === "string") {
            self.counters.delete(args[0]);
          }
          return;

        case "assert":
          if (args[0]) {
            // Assertion passed, don't send
            return;
          }
          processedArgs = ["Assertion failed:", ...args.slice(1)];
          break;

        case "clear":
          processedArgs = ["Console was cleared"];
          break;
      }

      // Send to PostGate
      self.sendMessage(method, processedArgs);
    };
  }

  private sendMessage(method: ConsoleMethod, args: unknown[]): void {
    const message: ConsoleMessage = {
      type: "console",
      method,
      args: args.map((arg) => serialize(arg)),
      timestamp: Date.now(),
      url: window.location.href,
    };

    // Add stack trace for errors
    if (method === "error" || method === "warn" || method === "trace") {
      message.stack = getStackTrace();
    }

    this.transport.send(message);
  }
}
