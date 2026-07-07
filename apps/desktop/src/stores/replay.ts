import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';

// Types matching Rust backend
export interface Collection {
  id: string;
  name: string;
  parent_id: string | null;
  created_at: number;
  updated_at: number;
}

export interface KeyValuePair {
  key: string;
  value: string;
  enabled: boolean;
  description?: string;
}

export interface FormDataField {
  key: string;
  value: string;
  type: 'text' | 'file';
  enabled: boolean;
  fileName?: string;
  contentType?: string;
}

export type RequestBody = 
  | { type: 'none' }
  | { type: 'raw'; content: string; contentType: string }
  | { type: 'json'; content: string }
  | { type: 'form-data'; fields: FormDataField[] }
  | { type: 'x-www-form-urlencoded'; fields: KeyValuePair[] }
  | { type: 'binary'; fileName?: string; data?: string };

export interface SavedRequest {
  id: string;
  name: string;
  collection_id: string | null;
  method: string;
  url: string;
  headers: KeyValuePair[];
  query_params: KeyValuePair[];
  body: RequestBody;
  created_at: number;
  updated_at: number;
}

export interface ReplayResponse {
  status: number;
  statusText: string;
  headers: Record<string, string>;
  body: string | null;
  bodySize: number;
  contentType: string | null;
  durationMs: number;
}

export interface RequestHistory {
  id: string;
  saved_request_id: string | null;
  request: SavedRequest;
  response: ReplayResponse | null;
  error: string | null;
  executed_at: number;
}

export interface CollectionNode {
  collection: Collection;
  children: CollectionNode[];
  requests: SavedRequest[];
}

export interface CollectionTree {
  collections: CollectionNode[];
  root_requests: SavedRequest[];
}

interface ReplayState {
  // Data
  tree: CollectionTree | null;
  collections: Collection[];
  selectedRequest: SavedRequest | null;
  currentRequest: SavedRequest;
  response: ReplayResponse | null;
  history: RequestHistory[];
  
  // UI state
  isLoading: boolean;
  isExecuting: boolean;
  error: string | null;
  
  // Actions
  fetchTree: () => Promise<void>;
  fetchHistory: (limit?: number) => Promise<void>;
  
  // Collection actions
  createCollection: (name: string, parentId?: string) => Promise<Collection>;
  updateCollection: (id: string, name: string) => Promise<void>;
  deleteCollection: (id: string, deleteContents?: boolean) => Promise<void>;
  
  // Request actions
  selectRequest: (request: SavedRequest | null) => void;
  createRequest: (request: Partial<SavedRequest>, collectionId?: string) => Promise<SavedRequest>;
  updateRequest: (request: SavedRequest) => Promise<void>;
  deleteRequest: (id: string) => Promise<void>;
  duplicateRequest: (id: string) => Promise<SavedRequest>;
  moveRequest: (requestId: string, collectionId: string | null) => Promise<void>;
  
  // Current request editing
  setCurrentRequest: (request: SavedRequest) => void;
  updateCurrentRequest: (updates: Partial<SavedRequest>) => void;
  
  // Execution
  executeRequest: () => Promise<void>;
  clearResponse: () => void;
  clearHistory: () => Promise<void>;
  
  // History for current request
  getHistoryForRequest: (requestId: string) => RequestHistory[];
  loadHistoryItem: (historyItem: RequestHistory) => void;
  
  // Import
  importFromCapture: (data: { id?: string; method: string; url: string; path: string; request_headers?: Record<string, string> }, collectionId?: string) => Promise<SavedRequest>;
}

const defaultRequest: SavedRequest = {
  id: '',
  name: 'New Request',
  collection_id: null,
  method: 'GET',
  url: '',
  headers: [{ key: 'Content-Type', value: 'application/json', enabled: true }],
  query_params: [],
  body: { type: 'none' },
  created_at: 0,
  updated_at: 0,
};

export const useReplayStore = create<ReplayState>((set, get) => ({
  tree: null,
  collections: [],
  selectedRequest: null,
  currentRequest: { ...defaultRequest },
  response: null,
  history: [],
  isLoading: false,
  isExecuting: false,
  error: null,

  fetchTree: async () => {
    set({ isLoading: true, error: null });
    try {
      const tree = await invoke<CollectionTree>('get_collection_tree');
      const collections = await invoke<Collection[]>('get_collections');
      set({ tree, collections, isLoading: false });
    } catch (error) {
      set({ error: String(error), isLoading: false });
    }
  },

  fetchHistory: async (limit = 50) => {
    try {
      const history = await invoke<RequestHistory[]>('get_request_history', { limit });
      set({ history });
    } catch (error) {
      console.error('Failed to fetch history:', error);
    }
  },

  createCollection: async (name: string, parentId?: string) => {
    const collection = await invoke<Collection>('create_collection', { 
      name, 
      parentId: parentId || null 
    });
    await get().fetchTree();
    return collection;
  },

  updateCollection: async (id: string, name: string) => {
    await invoke('update_collection', { id, name });
    await get().fetchTree();
  },

  deleteCollection: async (id: string, deleteContents = true) => {
    await invoke('delete_collection', { id, deleteContents });
    await get().fetchTree();
  },

  selectRequest: (request: SavedRequest | null) => {
    set({ 
      selectedRequest: request, 
      currentRequest: request ? { ...request } : { ...defaultRequest },
      response: null,
    });
  },

  createRequest: async (request: Partial<SavedRequest>, collectionId?: string) => {
    const newRequest: SavedRequest = {
      ...defaultRequest,
      ...request,
      collection_id: collectionId || null,
    };
    const saved = await invoke<SavedRequest>('create_saved_request', { request: newRequest });
    await get().fetchTree();
    set({ selectedRequest: saved, currentRequest: { ...saved } });
    return saved;
  },

  updateRequest: async (request: SavedRequest) => {
    await invoke('update_saved_request', { request });
    await get().fetchTree();
    set({ selectedRequest: request, currentRequest: { ...request } });
  },

  deleteRequest: async (id: string) => {
    await invoke('delete_saved_request', { id });
    await get().fetchTree();
    const { selectedRequest } = get();
    if (selectedRequest?.id === id) {
      set({ selectedRequest: null, currentRequest: { ...defaultRequest } });
    }
  },

  duplicateRequest: async (id: string) => {
    const duplicate = await invoke<SavedRequest>('duplicate_request', { id });
    await get().fetchTree();
    set({ selectedRequest: duplicate, currentRequest: { ...duplicate } });
    return duplicate;
  },

  moveRequest: async (requestId: string, collectionId: string | null) => {
    await invoke('move_request', { requestId, collectionId });
    await get().fetchTree();
  },

  setCurrentRequest: (request: SavedRequest) => {
    set({ currentRequest: request });
  },

  updateCurrentRequest: (updates: Partial<SavedRequest>) => {
    const { currentRequest } = get();
    set({ currentRequest: { ...currentRequest, ...updates } });
  },

  executeRequest: async () => {
    const { currentRequest } = get();
    set({ isExecuting: true, error: null, response: null });
    
    try {
      const response = await invoke<ReplayResponse>('execute_saved_request', { 
        request: currentRequest 
      });
      set({ response, isExecuting: false });
      await get().fetchHistory();
    } catch (error) {
      set({ error: String(error), isExecuting: false });
    }
  },

  clearResponse: () => {
    set({ response: null, error: null });
  },

  clearHistory: async () => {
    await invoke('clear_request_history');
    set({ history: [] });
  },

  getHistoryForRequest: (requestId: string) => {
    const { history } = get();
    return history.filter(h => h.saved_request_id === requestId);
  },

  loadHistoryItem: (historyItem: RequestHistory) => {
    set({
      currentRequest: { ...historyItem.request },
      response: historyItem.response,
      error: historyItem.error,
    });
  },

  importFromCapture: async (data, collectionId) => {
    const request = await invoke<SavedRequest>('import_from_capture', {
      capturedRequest: data,
      collectionId: collectionId || null,
    });
    await get().fetchTree();
    set({ selectedRequest: request, currentRequest: { ...request } });
    return request;
  },
}));
