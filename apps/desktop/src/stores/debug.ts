// Debug store for managing frontend debugging state

import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import { useProxyStore } from "./proxy";

// Types matching Rust backend

export type ConsoleLevel = "log" | "info" | "warn" | "error" | "debug" | "trace" | "clear";

export interface ConsoleArg {
  type: "string" | "number" | "boolean" | "null" | "undefined" | "object" | "array" | "function" | "symbol" | "error" | "element" | "circular" | "truncated";
  value: unknown;
}

export interface ConsoleLog {
  id: string;
  session_id: string;
  level: ConsoleLevel;
  args: ConsoleArg[];
  timestamp: number;
  stack_trace: string | null;
  source_url: string | null;
  line_number: number | null;
  column_number: number | null;
}

export interface DebugSession {
  id: string;
  url: string;
  title: string | null;
  user_agent: string | null;
  connected_at: number;
  last_activity: number;
  is_connected: boolean;
  cdp_enabled: boolean;
  webSocketDebuggerUrl: string;
}

export interface PageError {
  id: string;
  session_id: string;
  error_type: string;
  message: string;
  stack: string | null;
  source_url: string | null;
  line_number: number | null;
  column_number: number | null;
  timestamp: number;
}

export interface DebugStatus {
  is_running: boolean;
  port: number;
  session_count: number;
  total_logs: number;
}

interface DebugState {
  // Server status
  status: DebugStatus;
  isLoading: boolean;
  error: string | null;

  // Sessions
  sessions: DebugSession[];
  selectedSessionId: string | null;

  // Console logs
  logs: ConsoleLog[];
  filteredLogs: ConsoleLog[];
  levelFilter: ConsoleLevel[];
  searchFilter: string;

  // Page errors
  errors: PageError[];

  // Auto-scroll
  autoScroll: boolean;

  // Actions
  fetchStatus: () => Promise<void>;
  syncWithRules: () => Promise<void>;
  startServer: (port?: number) => Promise<void>;
  stopServer: () => Promise<void>;
  fetchSessions: () => Promise<void>;
  selectSession: (sessionId: string | null) => void;
  fetchLogs: (sessionId?: string) => Promise<void>;
  clearLogs: (sessionId?: string) => Promise<void>;
  fetchErrors: (sessionId: string) => Promise<void>;
  clearAll: () => Promise<void>;
  removeSession: (sessionId: string) => Promise<void>;
  setLevelFilter: (levels: ConsoleLevel[]) => void;
  setSearchFilter: (search: string) => void;
  toggleAutoScroll: () => void;
  addLog: (log: ConsoleLog) => void;
}

export const useDebugStore = create<DebugState>((set, get) => ({
  // Initial state
  status: {
    is_running: false,
    port: 9229,
    session_count: 0,
    total_logs: 0,
  },
  isLoading: false,
  error: null,
  sessions: [],
  selectedSessionId: null,
  logs: [],
  filteredLogs: [],
  levelFilter: [],
  searchFilter: "",
  errors: [],
  autoScroll: true,

  // Actions
  fetchStatus: async () => {
    try {
      const status = await invoke<DebugStatus>("get_debug_status");
      set({ status });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  syncWithRules: async () => {
    try {
      const hasDebugRules = await invoke<boolean>("has_active_debug_rules");
      const currentStatus = get().status;
      const debugPort = useProxyStore.getState().config.debugPort;

      if (hasDebugRules && !currentStatus.is_running) {
        // Start server if debug rules exist but server is not running
        await get().startServer(debugPort);
      } else if (!hasDebugRules && currentStatus.is_running) {
        // Stop server if no debug rules but server is running
        await get().stopServer();
      }
    } catch (e) {
      set({ error: String(e) });
    }
  },

  startServer: async (port?: number) => {
    const debugPort = port ?? useProxyStore.getState().config.debugPort;
    set({ isLoading: true, error: null });
    try {
      await invoke("start_debug_server", { port: debugPort });
      await get().fetchStatus();
    } catch (e) {
      set({ error: String(e) });
    } finally {
      set({ isLoading: false });
    }
  },

  stopServer: async () => {
    set({ isLoading: true, error: null });
    try {
      await invoke("stop_debug_server");
      await get().fetchStatus();
    } catch (e) {
      set({ error: String(e) });
    } finally {
      set({ isLoading: false });
    }
  },

  fetchSessions: async () => {
    try {
      const sessions = await invoke<DebugSession[]>("get_debug_sessions");
      set({ sessions });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  selectSession: (sessionId) => {
    set({ selectedSessionId: sessionId });
    if (sessionId) {
      get().fetchLogs(sessionId);
      get().fetchErrors(sessionId);
    }
  },

  fetchLogs: async (sessionId) => {
    try {
      const logs = await invoke<ConsoleLog[]>("get_console_logs", {
        sessionId,
        limit: 1000,
      });
      set({ logs });
      applyFilters(get, set);
    } catch (e) {
      set({ error: String(e) });
    }
  },

  clearLogs: async (sessionId) => {
    try {
      await invoke("clear_console_logs", { sessionId });
      set({ logs: [], filteredLogs: [] });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  fetchErrors: async (sessionId) => {
    try {
      const errors = await invoke<PageError[]>("get_page_errors", { sessionId });
      set({ errors });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  clearAll: async () => {
    try {
      await invoke("clear_all_debug_data");
      set({ logs: [], filteredLogs: [], errors: [] });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  removeSession: async (sessionId) => {
    try {
      await invoke("remove_debug_session", { sessionId });
      set((state) => ({
        sessions: state.sessions.filter((s) => s.id !== sessionId),
        selectedSessionId: state.selectedSessionId === sessionId ? null : state.selectedSessionId,
      }));
    } catch (e) {
      set({ error: String(e) });
    }
  },

  setLevelFilter: (levels) => {
    set({ levelFilter: levels });
    applyFilters(get, set);
  },

  setSearchFilter: (search) => {
    set({ searchFilter: search });
    applyFilters(get, set);
  },

  toggleAutoScroll: () => {
    set((state) => ({ autoScroll: !state.autoScroll }));
  },

  addLog: (log) => {
    set((state) => {
      const logs = [...state.logs, log].slice(-1000); // Keep last 1000 logs
      return { logs };
    });
    applyFilters(get, set);
  },
}));

// Helper to apply filters
function applyFilters(
  get: () => DebugState,
  set: (state: Partial<DebugState>) => void
) {
  const { logs, levelFilter, searchFilter } = get();

  let filtered = logs;

  // Apply level filter
  if (levelFilter.length > 0) {
    filtered = filtered.filter((log) => levelFilter.includes(log.level));
  }

  // Apply search filter
  if (searchFilter) {
    const search = searchFilter.toLowerCase();
    filtered = filtered.filter((log) => {
      // Search in args
      const argsStr = JSON.stringify(log.args).toLowerCase();
      if (argsStr.includes(search)) return true;

      // Search in source URL
      if (log.source_url?.toLowerCase().includes(search)) return true;

      return false;
    });
  }

  set({ filteredLogs: filtered });
}

// Event listener setup
let unlistenFns: UnlistenFn[] = [];

export async function setupDebugListeners() {
  const store = useDebugStore.getState();

  // Listen for server started
  unlistenFns.push(
    await listen("debug:server_started", () => {
      store.fetchStatus();
    })
  );

  // Listen for server stopped
  unlistenFns.push(
    await listen("debug:server_stopped", () => {
      store.fetchStatus();
    })
  );

  // Listen for new console logs (if we implement real-time updates)
  unlistenFns.push(
    await listen<ConsoleLog>("debug:console_log", (event) => {
      store.addLog(event.payload);
    })
  );
}

export function cleanupDebugListeners() {
  unlistenFns.forEach((fn) => fn());
  unlistenFns = [];
}
