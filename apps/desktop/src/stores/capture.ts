import { create } from "zustand";
import { useMemo } from "react";

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

interface CaptureState {
  // Use Map for O(1) lookups and updates
  requestMap: Map<string, CapturedRequest>;
  // Keep ordered array of IDs for display order
  requestIds: string[];
  selectedId: string | null;
  isPaused: boolean;
  filter: FilterOptions;
  maxRequests: number;

  addRequest: (request: CapturedRequest) => void;
  updateRequest: (id: string, update: Partial<CapturedRequest>) => void;
  setSelected: (id: string | null) => void;
  togglePause: () => void;
  clearRequests: () => void;
  setFilter: (filter: Partial<FilterOptions>) => void;
  resetFilter: () => void;
  // Helper to get requests as array (computed from map + ids)
  getRequests: () => CapturedRequest[];
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

export const useCaptureStore = create<CaptureState>()((set, get) => ({
  requestMap: new Map(),
  requestIds: [],
  selectedId: null,
  isPaused: false,
  filter: defaultFilter,
  maxRequests: 10000,

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
      let newIds = [request.id, ...state.requestIds];

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

  updateRequest: (id, update) => {
    set((state) => {
      const existing = state.requestMap.get(id);
      if (!existing) return state;

      const newMap = new Map(state.requestMap);
      newMap.set(id, { ...existing, ...update });

      return { requestMap: newMap };
    });
  },

  setSelected: (id) => set({ selectedId: id }),

  togglePause: () => set((state) => ({ isPaused: !state.isPaused })),

  clearRequests: () =>
    set({
      requestMap: new Map(),
      requestIds: [],
      selectedId: null,
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
}));

// Selector for requests array with stable reference
export const useRequests = () => {
  const requestMap = useCaptureStore((state) => state.requestMap);
  const requestIds = useCaptureStore((state) => state.requestIds);

  return useMemo(() => {
    return requestIds
      .map((id) => requestMap.get(id))
      .filter((r): r is CapturedRequest => r !== undefined);
  }, [requestMap, requestIds]);
};

// Optimized filtered requests selector
export const useFilteredRequests = () => {
  const requests = useRequests();
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
  }, [requests, filter]);
};
