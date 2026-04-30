import { create } from "zustand";
import { useMemo } from "react";
import { invoke } from "@tauri-apps/api/core";

export type Protocol = "http1" | "http2" | "quic" | "websocket" | "sse";

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
  remoteAddr: string | null;
}

export interface FilterOptions {
  search: string;
  methods: string[];
  statusCodes: string[];
  contentTypes: string[];
  hosts: string[];
  hasRules: boolean | null;
  protocols: Protocol[];
}

// Stored captured request from backend
interface StoredCapturedRequest {
  id: string;
  timestamp: number;
  method: string;
  url: string;
  host: string;
  path: string;
  protocol: string;
  requestHeaders: Record<string, string> | null;
  requestSize: number;
  responseStatus: number | null;
  responseHeaders: Record<string, string> | null;
  responseSize: number | null;
  contentType: string | null;
  durationMs: number | null;
  matchedRules: string[];
  error: string | null;
  tlsVersion: string | null;
  remoteAddr: string | null;
  isComplete: boolean;
}

interface PaginatedResult<T> {
  items: T[];
  total: number;
  page: number;
  pageSize: number;
  hasMore: boolean;
}

interface CaptureState {
  // Use Map for O(1) lookups and updates
  requestMap: Map<string, CapturedRequest>;
  // Keep ordered array of IDs for display order
  requestIds: string[];
  selectedId: string | null;
  isPaused: boolean;
  filter: FilterOptions;
  maxRequests: number;

  // History loading state
  isLoadingHistory: boolean;
  historyLoaded: boolean;
  historyTotal: number;

  addRequest: (request: CapturedRequest) => void;
  addRequests: (requests: CapturedRequest[]) => void;
  updateRequest: (id: string, update: Partial<CapturedRequest>) => void;
  batchUpdateRequests: (updates: Array<{ id: string; update: Partial<CapturedRequest> }>) => void;
  setSelected: (idOrFn: string | null | ((prev: string | null) => string | null)) => void;
  togglePause: () => void;
  clearRequests: () => void;
  setFilter: (filter: Partial<FilterOptions>) => void;
  resetFilter: () => void;
  // Helper to get requests as array (computed from map + ids)
  getRequests: () => CapturedRequest[];
  // History management
  loadHistory: () => Promise<void>;
  clearHistory: () => Promise<void>;
  // Batching helpers for high-frequency updates
  _pendingRequests: CapturedRequest[];
  _pendingUpdates: Map<string, Partial<CapturedRequest>>;
  _flushTimer: ReturnType<typeof setTimeout> | null;
  queueRequest: (request: CapturedRequest) => void;
  queueUpdate: (id: string, update: Partial<CapturedRequest>) => void;
  flushPending: () => void;
}

const defaultFilter: FilterOptions = {
  search: "",
  methods: [],
  statusCodes: [],
  contentTypes: [],
  hosts: [],
  hasRules: null,
  protocols: [],
};

const FLUSH_DELAY_MS = 16;
const PENDING_UPDATE_MULTIPLIER = 2;
const PENDING_REQUEST_TRIM_BATCH = 1000;

function trimPendingRequests(
  requests: CapturedRequest[],
  maxRequests: number
): void {
  if (requests.length > maxRequests + PENDING_REQUEST_TRIM_BATCH) {
    const overflow = requests.length - maxRequests;
    // Keep the newest queued requests. Trim in chunks instead of shifting on
    // every event once capped; otherwise long background captures pay O(n)
    // array movement per incoming request.
    requests.splice(0, overflow);
  }
}

function trimPendingUpdates(
  updates: Map<string, Partial<CapturedRequest>>,
  maxRequests: number
): void {
  const maxPendingUpdates = maxRequests * PENDING_UPDATE_MULTIPLIER;
  while (updates.size > maxPendingUpdates) {
    const oldestId = updates.keys().next().value;
    if (oldestId === undefined) return;
    updates.delete(oldestId);
  }
}

function mapProtocol(protocol: string): Protocol {
  switch (protocol.toLowerCase()) {
    case "http1":
    case "http/1.1":
      return "http1";
    case "http2":
    case "http/2":
    case "h2":
      return "http2";
    case "quic":
    case "http3":
    case "h3":
      return "quic";
    case "websocket":
    case "ws":
      return "websocket";
    case "sse":
    case "eventsource":
      return "sse";
    default:
      return "http1";
  }
}

function convertStoredToCaptured(stored: StoredCapturedRequest): CapturedRequest {
  return {
    id: stored.id,
    timestamp: stored.timestamp,
    method: stored.method,
    url: stored.url,
    host: stored.host,
    path: stored.path,
    requestHeaders: stored.requestHeaders || {},
    requestBody: null, // Lazy loaded
    responseStatus: stored.responseStatus,
    responseHeaders: stored.responseHeaders,
    responseBody: null, // Lazy loaded
    durationMs: stored.durationMs,
    matchedRules: stored.matchedRules || [],
    protocol: mapProtocol(stored.protocol),
    tlsInfo: stored.tlsVersion
      ? { version: stored.tlsVersion, cipher: "", serverName: stored.host }
      : null,
    contentType: stored.contentType,
    requestSize: stored.requestSize,
    responseSize: stored.responseSize,
    remoteAddr: stored.remoteAddr,
  };
}

export const useCaptureStore = create<CaptureState>()((set, get) => ({
  requestMap: new Map(),
  requestIds: [],
  selectedId: null,
  isPaused: false,
  filter: defaultFilter,
  maxRequests: 10000,
  isLoadingHistory: false,
  historyLoaded: false,
  historyTotal: 0,
  
  // Batching state
  _pendingRequests: [],
  _pendingUpdates: new Map(),
  _flushTimer: null,

  addRequest: (request) => {
    if (get().isPaused) return;

    set((state) => {
      // Check for duplicate - O(1) lookup
      if (state.requestMap.has(request.id)) {
        return state;
      }

      // Create new Map with the new request
      const newMap = new Map(state.requestMap);
      newMap.set(request.id, request);

      // Add to beginning of IDs array
      const newIds = [request.id, ...state.requestIds];

      // Trim if exceeds max
      if (newIds.length > state.maxRequests) {
        const removedId = newIds.pop()!;
        newMap.delete(removedId);
      }

      return {
        requestMap: newMap,
        requestIds: newIds,
      };
    });
  },

  addRequests: (requests) => {
    set((state) => {
      const newMap = new Map(state.requestMap);
      const newIds = [...state.requestIds];

      // Sort by timestamp descending (newest first)
      const sortedRequests = [...requests].sort((a, b) => b.timestamp - a.timestamp);

      for (const request of sortedRequests) {
        if (!newMap.has(request.id)) {
          newMap.set(request.id, request);
          newIds.unshift(request.id);
        }
      }

      // Trim if exceeds max
      while (newIds.length > state.maxRequests) {
        const removedId = newIds.pop()!;
        newMap.delete(removedId);
      }

      return {
        requestMap: newMap,
        requestIds: newIds,
      };
    });
  },

  updateRequest: (id, update) => {
    set((state) => {
      const existing = state.requestMap.get(id);
      if (!existing) return state;

      const newMap = new Map(state.requestMap);
      newMap.set(id, { ...existing, ...update });

      return { requestMap: newMap };
    });
  },

  batchUpdateRequests: (updates) => {
    set((state) => {
      const newMap = new Map(state.requestMap);
      let changed = false;
      
      for (const { id, update } of updates) {
        const existing = newMap.get(id);
        if (existing) {
          newMap.set(id, { ...existing, ...update });
          changed = true;
        }
      }

      return changed ? { requestMap: newMap } : state;
    });
  },

  // Queue a request for batched addition (used for high-frequency events)
  queueRequest: (request) => {
    if (get().isPaused) return;
    
    const state = get();
    state._pendingRequests.push(request);
    trimPendingRequests(state._pendingRequests, state.maxRequests);
    
    // Schedule flush if not already scheduled
    if (!state._flushTimer) {
      const timer = setTimeout(() => {
        get().flushPending();
      }, FLUSH_DELAY_MS); // ~1 frame at 60fps
      
      set({ _flushTimer: timer });
    }
  },

  // Queue an update for batched processing
  queueUpdate: (id, update) => {
    const state = get();
    state._pendingUpdates.set(id, {
      ...state._pendingUpdates.get(id),
      ...update,
    });
    trimPendingUpdates(state._pendingUpdates, state.maxRequests);
    
    // Schedule flush if not already scheduled
    if (!state._flushTimer) {
      const timer = setTimeout(() => {
        get().flushPending();
      }, FLUSH_DELAY_MS);
      
      set({ _flushTimer: timer });
    }
  },

  // Flush all pending requests and updates in one batch
  flushPending: () => {
    const state = get();
    const pendingRequests = [...state._pendingRequests];
    const pendingUpdates = Array.from(state._pendingUpdates, ([id, update]) => ({
      id,
      update,
    }));
    
    // Clear pending arrays and timer
    state._pendingRequests.length = 0;
    state._pendingUpdates.clear();
    if (state._flushTimer) {
      clearTimeout(state._flushTimer);
    }
    
    if (pendingRequests.length === 0 && pendingUpdates.length === 0) {
      set({ _flushTimer: null });
      return;
    }
    
    set((currentState) => {
      const newMap = new Map(currentState.requestMap);
      const addedIds: string[] = [];
      
      // Process new requests (sorted by timestamp, newest first)
      if (pendingRequests.length > 0) {
        const sortedRequests = pendingRequests.sort((a, b) => b.timestamp - a.timestamp);
        
        for (const request of sortedRequests) {
          if (!newMap.has(request.id)) {
            newMap.set(request.id, request);
            addedIds.push(request.id);
          }
        }
      }

      const newIds = addedIds.length > 0
        ? [...addedIds, ...currentState.requestIds]
        : [...currentState.requestIds];
      
      // Process updates
      for (const { id, update } of pendingUpdates) {
        const existing = newMap.get(id);
        if (existing) {
          newMap.set(id, { ...existing, ...update });
        }
      }
      
      // Trim if exceeds max
      while (newIds.length > currentState.maxRequests) {
        const removedId = newIds.pop()!;
        newMap.delete(removedId);
      }
      
      return {
        requestMap: newMap,
        requestIds: newIds,
        _flushTimer: null,
      };
    });
  },

  setSelected: (idOrFn) => set((state) => ({
    selectedId: typeof idOrFn === 'function' ? idOrFn(state.selectedId) : idOrFn
  })),

  togglePause: () => set((state) => ({ isPaused: !state.isPaused })),

  clearRequests: () =>
    set((state) => {
      // Also cancel any in-flight batched work — otherwise queued requests
      // from before the clear will sneak back in on the next flush tick.
      if (state._flushTimer) {
        clearTimeout(state._flushTimer);
      }
      state._pendingRequests.length = 0;
      state._pendingUpdates.clear();
      return {
        requestMap: new Map(),
        requestIds: [],
        selectedId: null,
        _flushTimer: null,
      };
    }),

  setFilter: (filter) =>
    set((state) => ({ filter: { ...state.filter, ...filter } })),

  resetFilter: () => set({ filter: defaultFilter }),

  getRequests: () => {
    const state = get();
    return state.requestIds
      .map((id) => state.requestMap.get(id))
      .filter((r): r is CapturedRequest => r !== undefined);
  },

  loadHistory: async () => {
    const state = get();
    if (state.isLoadingHistory || state.historyLoaded) return;

    set({ isLoadingHistory: true });

    try {
      // Load first page of history (most recent requests)
      const result = await invoke<PaginatedResult<StoredCapturedRequest>>(
        "load_captured_history",
        { page: 1, pageSize: 500 }
      );

      const requests = result.items.map(convertStoredToCaptured);

      set((state) => {
        const newMap = new Map(state.requestMap);
        const existingIds = new Set(state.requestIds);
        const newIds = [...state.requestIds];

        // Add historical requests that don't already exist
        for (const req of requests) {
          if (!existingIds.has(req.id)) {
            newMap.set(req.id, req);
            newIds.push(req.id); // Add to end (older requests)
          }
        }

        return {
          requestMap: newMap,
          requestIds: newIds,
          historyLoaded: true,
          historyTotal: result.total,
          isLoadingHistory: false,
        };
      });
    } catch (e) {
      console.error("Failed to load history:", e);
      set({ isLoadingHistory: false, historyLoaded: true });
    }
  },

  clearHistory: async () => {
    try {
      await invoke("clear_captured_history");
      set((state) => {
        if (state._flushTimer) {
          clearTimeout(state._flushTimer);
        }
        state._pendingRequests.length = 0;
        state._pendingUpdates.clear();
        return {
          requestMap: new Map(),
          requestIds: [],
          selectedId: null,
          historyLoaded: false,
          historyTotal: 0,
          _flushTimer: null,
        };
      });
    } catch (e) {
      console.error("Failed to clear history:", e);
    }
  },
}));

// Selector for requests array with stable reference
// Use shallow comparison to avoid unnecessary re-renders
export const useRequests = () => {
  const requestMap = useCaptureStore((state) => state.requestMap);
  const requestIds = useCaptureStore((state) => state.requestIds);

  return useMemo(() => {
    return requestIds
      .map((id) => requestMap.get(id))
      .filter((r): r is CapturedRequest => r !== undefined);
  }, [requestMap, requestIds]);
};

// Optimized: Get request count without triggering re-renders on content changes
export const useRequestCount = () => {
  return useCaptureStore((state) => state.requestIds.length);
};

// Optimized filtered requests selector with memoized filter check
export const useFilteredRequests = () => {
  const requestMap = useCaptureStore((state) => state.requestMap);
  const requestIds = useCaptureStore((state) => state.requestIds);
  const filter = useCaptureStore((state) => state.filter);

  return useMemo(() => {
    // Quick check if filter is empty (common case)
    const isEmptyFilter =
      !filter.search &&
      filter.methods.length === 0 &&
      filter.statusCodes.length === 0 &&
      filter.contentTypes.length === 0 &&
      filter.hosts.length === 0 &&
      filter.hasRules === null &&
      filter.protocols.length === 0;

    // Build requests array
    const requests = requestIds
      .map((id) => requestMap.get(id))
      .filter((r): r is CapturedRequest => r !== undefined);

    if (isEmptyFilter) {
      return requests;
    }

    // Pre-compute lowercase search once
    const searchLower = filter.search.toLowerCase();

    return requests.filter((req) => {
      // Search filter
      if (searchLower) {
        if (
          !req.url.toLowerCase().includes(searchLower) &&
          !req.host.toLowerCase().includes(searchLower) &&
          !req.path.toLowerCase().includes(searchLower)
        ) {
          return false;
        }
      }

      // Method filter
      if (filter.methods.length > 0 && !filter.methods.includes(req.method)) {
        return false;
      }

      // Status code filter
      if (filter.statusCodes.length > 0 && req.responseStatus) {
        const statusGroup = Math.floor(req.responseStatus / 100) + "xx";
        if (!filter.statusCodes.includes(statusGroup)) {
          return false;
        }
      }

      // Content type filter
      if (filter.contentTypes.length > 0 && req.contentType) {
        if (!filter.contentTypes.some((ct: string) => req.contentType?.includes(ct))) {
          return false;
        }
      }

      // Host filter
      if (filter.hosts.length > 0 && !filter.hosts.includes(req.host)) {
        return false;
      }

      // Has rules filter
      if (filter.hasRules === true && req.matchedRules.length === 0) {
        return false;
      }
      if (filter.hasRules === false && req.matchedRules.length > 0) {
        return false;
      }

      // Protocol filter
      if (
        filter.protocols.length > 0 &&
        !filter.protocols.includes(req.protocol)
      ) {
        return false;
      }

      return true;
    });
  }, [requestMap, requestIds, filter]);
};
