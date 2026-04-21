import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';

export interface ValueEntry {
  name: string;
  content: string;
  createdAt: number;
  updatedAt: number;
}

interface ValuesState {
  values: ValueEntry[];
  selectedName: string | null;
  isLoading: boolean;
  error: string | null;

  // Editor state
  editorContent: string;
  isDirty: boolean;

  // Actions
  loadValues: () => Promise<void>;
  selectValue: (name: string | null) => void;
  setEditorContent: (content: string) => void;
  saveCurrent: () => Promise<void>;
  discardChanges: () => void;
  createValue: (name: string, content?: string) => Promise<ValueEntry>;
  renameValue: (oldName: string, newName: string) => Promise<void>;
  deleteValue: (name: string) => Promise<void>;
}

export const useValuesStore = create<ValuesState>((set, get) => ({
  values: [],
  selectedName: null,
  isLoading: false,
  error: null,

  editorContent: '',
  isDirty: false,

  loadValues: async () => {
    set({ isLoading: true, error: null });
    try {
      const values = await invoke<ValueEntry[]>('list_values');
      set({ values, isLoading: false });

      // If currently selected entry still exists, sync the editor buffer; if
      // it was deleted externally, clear selection.
      const { selectedName, isDirty } = get();
      if (selectedName) {
        const match = values.find((v) => v.name === selectedName);
        if (!match) {
          set({ selectedName: null, editorContent: '', isDirty: false });
        } else if (!isDirty) {
          set({ editorContent: match.content });
        }
      }
    } catch (e) {
      set({ error: String(e), isLoading: false });
    }
  },

  selectValue: (name) => {
    const { values, isDirty } = get();
    if (isDirty) {
      // Mirror the Rules page: switching away discards pending edits. The
      // caller is responsible for confirming if that's not desired.
    }
    if (name === null) {
      set({ selectedName: null, editorContent: '', isDirty: false });
      return;
    }
    const match = values.find((v) => v.name === name);
    set({
      selectedName: name,
      editorContent: match?.content ?? '',
      isDirty: false,
    });
  },

  setEditorContent: (content) => {
    const { selectedName, values } = get();
    const original = selectedName
      ? values.find((v) => v.name === selectedName)?.content ?? ''
      : '';
    set({ editorContent: content, isDirty: content !== original });
  },

  saveCurrent: async () => {
    const { selectedName, editorContent } = get();
    if (!selectedName) return;
    try {
      const saved = await invoke<ValueEntry>('save_value', {
        name: selectedName,
        content: editorContent,
      });
      const values = get().values.map((v) =>
        v.name === saved.name ? saved : v,
      );
      // If the saved name isn't in the list yet (shouldn't happen via save),
      // append it so the UI stays consistent.
      if (!values.some((v) => v.name === saved.name)) {
        values.push(saved);
      }
      values.sort((a, b) => a.name.localeCompare(b.name));
      set({ values, isDirty: false });
    } catch (e) {
      set({ error: String(e) });
      throw e;
    }
  },

  discardChanges: () => {
    const { selectedName, values } = get();
    const match = selectedName
      ? values.find((v) => v.name === selectedName)
      : undefined;
    set({ editorContent: match?.content ?? '', isDirty: false });
  },

  createValue: async (name, content = '') => {
    const saved = await invoke<ValueEntry>('save_value', { name, content });
    const values = [...get().values.filter((v) => v.name !== saved.name), saved];
    values.sort((a, b) => a.name.localeCompare(b.name));
    set({
      values,
      selectedName: saved.name,
      editorContent: saved.content,
      isDirty: false,
    });
    return saved;
  },

  renameValue: async (oldName, newName) => {
    const saved = await invoke<ValueEntry>('rename_value', {
      oldName,
      newName,
    });
    const values = get()
      .values.filter((v) => v.name !== oldName)
      .concat(saved);
    values.sort((a, b) => a.name.localeCompare(b.name));
    const { selectedName } = get();
    set({
      values,
      selectedName: selectedName === oldName ? saved.name : selectedName,
    });
  },

  deleteValue: async (name) => {
    await invoke<boolean>('delete_value', { name });
    const values = get().values.filter((v) => v.name !== name);
    const { selectedName } = get();
    set({
      values,
      selectedName: selectedName === name ? null : selectedName,
      editorContent: selectedName === name ? '' : get().editorContent,
      isDirty: selectedName === name ? false : get().isDirty,
    });
  },
}));
