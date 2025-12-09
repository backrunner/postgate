import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';

export interface RuleGroup {
  id: string;
  name: string;
  enabled: boolean;
  priority: number;
  rules: Rule[];
  rawContent: string;
  createdAt: number;
  updatedAt: number;
}

export interface Rule {
  id: string;
  pattern: Pattern;
  filters?: RuleFilters;
  actions: RuleAction[];
  enabled: boolean;
  priority: number;
  rawLine: string;
}

export interface Pattern {
  type: 'Exact' | 'Wildcard' | 'Regex' | 'PathPrefix' | 'All' | 'Domain' | 'Url';
  value?: string;
  protocol?: string;
  host?: string;
  path?: string;
}

export interface RuleFilters {
  methods: string[];
  protocols: string[];
  ports: number[];
  headers: Record<string, string>;
  contentTypes: string[];
  exclude: string[];
  include: string[];
}

export interface RuleAction {
  type: string;
  [key: string]: unknown;
}

export interface ParseResult {
  success: boolean;
  rules: Rule[];
  errors: ParseError[];
}

export interface ParseError {
  line: number;
  message: string;
  content: string;
}

interface RulesState {
  groups: RuleGroup[];
  selectedGroupId: string | null;
  isLoading: boolean;
  error: string | null;
  
  // Editor state
  editorContent: string;
  parseResult: ParseResult | null;
  isDirty: boolean;
  
  // Actions
  loadGroups: () => Promise<void>;
  createGroup: (name: string) => Promise<RuleGroup>;
  updateGroup: (group: RuleGroup) => Promise<void>;
  deleteGroup: (id: string) => Promise<void>;
  toggleGroup: (id: string, enabled: boolean) => Promise<void>;
  selectGroup: (id: string | null) => void;
  
  // Editor actions
  setEditorContent: (content: string) => void;
  parseContent: (content: string) => Promise<ParseResult>;
  saveCurrentGroup: () => Promise<void>;
  discardChanges: () => void;
}

export const useRulesStore = create<RulesState>((set, get) => ({
  groups: [],
  selectedGroupId: null,
  isLoading: false,
  error: null,
  
  editorContent: '',
  parseResult: null,
  isDirty: false,
  
  loadGroups: async () => {
    set({ isLoading: true, error: null });
    try {
      const groups = await invoke<RuleGroup[]>('get_rule_groups');
      set({ groups, isLoading: false });
    } catch (e) {
      set({ error: String(e), isLoading: false });
    }
  },
  
  createGroup: async (name: string) => {
    const group: RuleGroup = {
      id: crypto.randomUUID(),
      name,
      enabled: true,
      priority: 0,
      rules: [],
      rawContent: `# ${name}\n# Write your whistle-compatible rules here\n# Example: example.com host://127.0.0.1:8080\n\n`,
      createdAt: Date.now(),
      updatedAt: Date.now(),
    };
    
    await invoke('save_rule_group', { group });
    await get().loadGroups();
    return group;
  },
  
  updateGroup: async (group: RuleGroup) => {
    await invoke('save_rule_group', { group: { ...group, updatedAt: Date.now() } });
    await get().loadGroups();
  },
  
  deleteGroup: async (id: string) => {
    await invoke('delete_rule_group', { id });
    const { selectedGroupId } = get();
    if (selectedGroupId === id) {
      set({ selectedGroupId: null, editorContent: '', parseResult: null, isDirty: false });
    }
    await get().loadGroups();
  },
  
  toggleGroup: async (id: string, enabled: boolean) => {
    await invoke('toggle_rule_group', { id, enabled });
    await get().loadGroups();
  },
  
  selectGroup: (id: string | null) => {
    const { groups, isDirty } = get();
    
    if (isDirty) {
      // TODO: Show confirmation dialog
      console.warn('Unsaved changes will be lost');
    }
    
    const group = groups.find(g => g.id === id);
    set({
      selectedGroupId: id,
      editorContent: group?.rawContent || '',
      parseResult: null,
      isDirty: false,
    });
    
    // Parse the content
    if (group?.rawContent) {
      get().parseContent(group.rawContent);
    }
  },
  
  setEditorContent: (content: string) => {
    const { selectedGroupId, groups } = get();
    const group = groups.find(g => g.id === selectedGroupId);
    const isDirty = content !== group?.rawContent;
    set({ editorContent: content, isDirty });
  },
  
  parseContent: async (content: string) => {
    try {
      const result = await invoke<ParseResult>('parse_rules', { content });
      set({ parseResult: result });
      return result;
    } catch (e) {
      const errorResult: ParseResult = {
        success: false,
        rules: [],
        errors: [{ line: 0, message: String(e), content: '' }],
      };
      set({ parseResult: errorResult });
      return errorResult;
    }
  },
  
  saveCurrentGroup: async () => {
    const { selectedGroupId, groups, editorContent, parseResult } = get();
    if (!selectedGroupId) return;
    
    const group = groups.find(g => g.id === selectedGroupId);
    if (!group) return;
    
    const updatedGroup: RuleGroup = {
      ...group,
      rawContent: editorContent,
      rules: parseResult?.rules || [],
      updatedAt: Date.now(),
    };
    
    await invoke('save_rule_group', { group: updatedGroup });
    await get().loadGroups();
    set({ isDirty: false });
  },
  
  discardChanges: () => {
    const { selectedGroupId, groups } = get();
    const group = groups.find(g => g.id === selectedGroupId);
    set({
      editorContent: group?.rawContent || '',
      isDirty: false,
    });
    
    if (group?.rawContent) {
      get().parseContent(group.rawContent);
    }
  },
}));
