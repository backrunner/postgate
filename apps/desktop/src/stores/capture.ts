import { create } from "zustand";

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
  requests: CapturedRequest[];
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
  requests: [],
  selectedId: null,
  isPaused: false,
  filter: defaultFilter,
  maxRequests: 10000,

  addRequest: (request) => {
    if (get().isPaused) return;

    set((state) => {
      const newRequests = [request, ...state.requests];
      // Keep only maxRequests
      if (newRequests.length > state.maxRequests) {
        newRequests.pop();
      }
      return { requests: newRequests };
    });
  },

  updateRequest: (id, update) => {
    set((state) => ({
      requests: state.requests.map((req) => (req.id === id ? { ...req, ...update } : req)),
    }));
  },

  setSelected: (id) => set({ selectedId: id }),

  togglePause: () => set((state) => ({ isPaused: !state.isPaused })),

  clearRequests: () => set({ requests: [], selectedId: null }),

  setFilter: (filter) => set((state) => ({ filter: { ...state.filter, ...filter } })),

  resetFilter: () => set({ filter: defaultFilter }),
}));

// Selector for filtered requests
export const useFilteredRequests = () => {
  const { requests, filter } = useCaptureStore();

  return requests.filter((req) => {
    // Search filter
    if (filter.search) {
      const searchLower = filter.search.toLowerCase();
      const matchesSearch =
        req.url.toLowerCase().includes(searchLower) ||
        req.host.toLowerCase().includes(searchLower) ||
        req.path.toLowerCase().includes(searchLower);
      if (!matchesSearch) return false;
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
      if (!filter.contentTypes.some((ct) => req.contentType?.includes(ct))) {
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
    if (filter.protocols.length > 0 && !filter.protocols.includes(req.protocol)) {
      return false;
    }

    return true;
  });
};
