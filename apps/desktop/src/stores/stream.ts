import { create } from "zustand";
import { useMemo } from "react";

export type StreamDirection = "inbound" | "outbound";

export type StreamMessageType =
  | "sse_event"
  | "ws_text"
  | "ws_binary"
  | "ws_ping"
  | "ws_pong"
  | "ws_close";

export interface StreamMessage {
  id: string;
  timestamp: number;
  direction: StreamDirection;
  messageType: StreamMessageType;
  data: string;
  isBase64: boolean;
  size: number;
}

export interface StreamConnection {
  connectionId: string;
  messages: StreamMessage[];
  messageCount: number;
  totalBytes: number;
  durationMs: number | null;
  closeReason: string | null;
  isEnded: boolean;
  /** Wall-clock of the last message or lifecycle event. Used by the idle
   * pruner to decide when a connection can be evicted. */
  lastActivityAt: number;
}

interface StreamMessageEvent {
  connectionId: string;
  message: StreamMessage;
}

interface StreamEndedEvent {
  connectionId: string;
  messageCount: number;
  totalBytes: number;
  durationMs: number;
  closeReason: string | null;
}

/** How long an ended connection sticks around with no activity before we
 * prune it from the store. Active (not-yet-ended) streams are kept for a
 * longer window so the user can still inspect them. */
const ENDED_IDLE_TTL_MS = 5 * 60 * 1000; // 5 min
const ACTIVE_IDLE_TTL_MS = 30 * 60 * 1000; // 30 min
/** Hard cap. When crossed we force-evict the least-recently-active
 * connections regardless of TTL. */
const MAX_CONNECTIONS = 200;

interface StreamState {
  // Map of connectionId to stream connection data
  connections: Map<string, StreamConnection>;
  // Max messages per connection to keep in memory
  maxMessagesPerConnection: number;

  // Actions
  addMessage: (event: StreamMessageEvent) => void;
  endStream: (event: StreamEndedEvent) => void;
  clearConnection: (connectionId: string) => void;
  clearAllConnections: () => void;
  getConnection: (connectionId: string) => StreamConnection | undefined;
  /** Drop connections that have been idle past their TTL, and enforce the
   * `MAX_CONNECTIONS` hard cap by evicting the least-recently-active.
   * Called periodically (see useProxy) and on insert to keep memory bounded
   * even when the user never manually clears. */
  pruneIdle: () => void;
}

export const useStreamStore = create<StreamState>()((set, get) => ({
  connections: new Map(),
  maxMessagesPerConnection: 5000,

  addMessage: (event) => {
    set((state) => {
      const newConnections = new Map(state.connections);
      const existing = newConnections.get(event.connectionId);
      const now = Date.now();

      if (existing) {
        // Add to existing connection
        const messages = [...existing.messages, event.message];
        // Trim if exceeds max
        if (messages.length > state.maxMessagesPerConnection) {
          messages.shift();
        }
        newConnections.set(event.connectionId, {
          ...existing,
          messages,
          messageCount: existing.messageCount + 1,
          totalBytes: existing.totalBytes + event.message.size,
          lastActivityAt: now,
        });
      } else {
        // Create new connection
        newConnections.set(event.connectionId, {
          connectionId: event.connectionId,
          messages: [event.message],
          messageCount: 1,
          totalBytes: event.message.size,
          durationMs: null,
          closeReason: null,
          isEnded: false,
          lastActivityAt: now,
        });

        // Enforce hard cap on creation. Active streams have a much longer
        // TTL, so without this cap a page spamming new short-lived SSE
        // connections could still blow past memory.
        if (newConnections.size > MAX_CONNECTIONS) {
          enforceHardCap(newConnections);
        }
      }

      return { connections: newConnections };
    });
  },

  endStream: (event) => {
    set((state) => {
      const newConnections = new Map(state.connections);
      const existing = newConnections.get(event.connectionId);
      const now = Date.now();

      if (existing) {
        newConnections.set(event.connectionId, {
          ...existing,
          messageCount: event.messageCount,
          totalBytes: event.totalBytes,
          durationMs: event.durationMs,
          closeReason: event.closeReason,
          isEnded: true,
          lastActivityAt: now,
        });
      } else {
        // Connection ended without any messages captured (unlikely but handle it)
        newConnections.set(event.connectionId, {
          connectionId: event.connectionId,
          messages: [],
          messageCount: event.messageCount,
          totalBytes: event.totalBytes,
          durationMs: event.durationMs,
          closeReason: event.closeReason,
          isEnded: true,
          lastActivityAt: now,
        });
      }

      return { connections: newConnections };
    });
  },

  clearConnection: (connectionId) => {
    set((state) => {
      const newConnections = new Map(state.connections);
      newConnections.delete(connectionId);
      return { connections: newConnections };
    });
  },

  clearAllConnections: () => {
    set({ connections: new Map() });
  },

  getConnection: (connectionId) => {
    return get().connections.get(connectionId);
  },

  pruneIdle: () => {
    set((state) => {
      if (state.connections.size === 0) return state;

      const now = Date.now();
      let removed = 0;
      const newConnections = new Map(state.connections);

      for (const [id, conn] of newConnections) {
        const ttl = conn.isEnded ? ENDED_IDLE_TTL_MS : ACTIVE_IDLE_TTL_MS;
        if (now - conn.lastActivityAt > ttl) {
          newConnections.delete(id);
          removed++;
        }
      }

      if (newConnections.size > MAX_CONNECTIONS) {
        enforceHardCap(newConnections);
      }

      if (removed === 0 && newConnections.size === state.connections.size) {
        return state;
      }
      return { connections: newConnections };
    });
  },
}));

/** Evict least-recently-active connections until we're back under
 * `MAX_CONNECTIONS`. Prefers dropping ended connections first. */
function enforceHardCap(connections: Map<string, StreamConnection>) {
  if (connections.size <= MAX_CONNECTIONS) return;

  const sorted = Array.from(connections.values()).sort((a, b) => {
    // Ended connections are expendable before active ones.
    if (a.isEnded !== b.isEnded) return a.isEnded ? -1 : 1;
    return a.lastActivityAt - b.lastActivityAt;
  });

  const toRemove = connections.size - MAX_CONNECTIONS;
  for (let i = 0; i < toRemove; i++) {
    connections.delete(sorted[i].connectionId);
  }
}

// Hook to get a specific connection's messages
export const useStreamConnection = (connectionId: string | null) => {
  const connections = useStreamStore((state) => state.connections);

  return useMemo(() => {
    if (!connectionId) return undefined;
    return connections.get(connectionId);
  }, [connections, connectionId]);
};

// Hook to get messages filtered by direction
export const useStreamMessages = (
  connectionId: string | null,
  direction?: StreamDirection
) => {
  const connection = useStreamConnection(connectionId);

  return useMemo(() => {
    if (!connection) return [];
    if (!direction) return connection.messages;
    return connection.messages.filter((m) => m.direction === direction);
  }, [connection, direction]);
};

// Format stream message type for display
export function formatMessageType(type: StreamMessageType): string {
  switch (type) {
    case "sse_event":
      return "SSE Event";
    case "ws_text":
      return "Text";
    case "ws_binary":
      return "Binary";
    case "ws_ping":
      return "Ping";
    case "ws_pong":
      return "Pong";
    case "ws_close":
      return "Close";
    default:
      return type;
  }
}

// Format direction for display
export function formatDirection(direction: StreamDirection): string {
  return direction === "inbound" ? "Received" : "Sent";
}
