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
}

export const useStreamStore = create<StreamState>()((set, get) => ({
  connections: new Map(),
  maxMessagesPerConnection: 5000,

  addMessage: (event) => {
    set((state) => {
      const newConnections = new Map(state.connections);
      const existing = newConnections.get(event.connectionId);

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
        });
      }

      return { connections: newConnections };
    });
  },

  endStream: (event) => {
    set((state) => {
      const newConnections = new Map(state.connections);
      const existing = newConnections.get(event.connectionId);

      if (existing) {
        newConnections.set(event.connectionId, {
          ...existing,
          messageCount: event.messageCount,
          totalBytes: event.totalBytes,
          durationMs: event.durationMs,
          closeReason: event.closeReason,
          isEnded: true,
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
}));

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
