import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';

// Plugin types matching Rust backend
export interface PluginInfo {
  id: string;
  name: string;
  version: string;
  description: string | null;
  author: string | null;
  path: string;
  entry: string;
  enabled: boolean;
  loaded: boolean;
}

export interface PluginPanel {
  id: string;
  plugin_id: string;
  title: string;
  icon: string | null;
  content: { type: 'html'; html: string } | { type: 'iframe'; url: string };
}

interface PluginsState {
  plugins: PluginInfo[];
  panels: PluginPanel[];
  pluginsDir: string | null;
  isLoading: boolean;
  error: string | null;
  
  // Actions
  fetchPlugins: () => Promise<void>;
  discoverPlugins: () => Promise<void>;
  loadPlugin: (pluginId: string, config?: Record<string, string>) => Promise<void>;
  unloadPlugin: (pluginId: string) => Promise<void>;
  togglePlugin: (pluginId: string, enabled: boolean) => Promise<void>;
  uninstallPlugin: (pluginId: string) => Promise<void>;
  fetchPanels: () => Promise<void>;
  fetchPluginsDir: () => Promise<void>;
}

export const usePluginsStore = create<PluginsState>((set, get) => ({
  plugins: [],
  panels: [],
  pluginsDir: null,
  isLoading: false,
  error: null,

  fetchPlugins: async () => {
    set({ isLoading: true, error: null });
    try {
      const plugins = await invoke<PluginInfo[]>('get_plugins');
      set({ plugins, isLoading: false });
    } catch (error) {
      set({ error: String(error), isLoading: false });
    }
  },

  discoverPlugins: async () => {
    set({ isLoading: true, error: null });
    try {
      const plugins = await invoke<PluginInfo[]>('discover_plugins');
      set({ plugins, isLoading: false });
    } catch (error) {
      set({ error: String(error), isLoading: false });
    }
  },

  loadPlugin: async (pluginId: string, config?: Record<string, string>) => {
    set({ isLoading: true, error: null });
    try {
      await invoke('load_plugin', { pluginId, config });
      // Refresh plugins list
      await get().fetchPlugins();
      // Refresh panels
      await get().fetchPanels();
    } catch (error) {
      set({ error: String(error), isLoading: false });
      throw error;
    }
  },

  unloadPlugin: async (pluginId: string) => {
    set({ isLoading: true, error: null });
    try {
      await invoke('unload_plugin', { pluginId });
      await get().fetchPlugins();
      await get().fetchPanels();
    } catch (error) {
      set({ error: String(error), isLoading: false });
      throw error;
    }
  },

  togglePlugin: async (pluginId: string, enabled: boolean) => {
    set({ isLoading: true, error: null });
    try {
      await invoke('toggle_plugin', { pluginId, enabled });
      await get().fetchPlugins();
      await get().fetchPanels();
    } catch (error) {
      set({ error: String(error), isLoading: false });
      throw error;
    }
  },

  uninstallPlugin: async (pluginId: string) => {
    set({ isLoading: true, error: null });
    try {
      await invoke('uninstall_plugin', { pluginId });
      await get().fetchPlugins();
      await get().fetchPanels();
    } catch (error) {
      set({ error: String(error), isLoading: false });
      throw error;
    }
  },

  fetchPanels: async () => {
    try {
      const panels = await invoke<PluginPanel[]>('get_plugin_panels');
      set({ panels });
    } catch (error) {
      console.error('Failed to fetch plugin panels:', error);
    }
  },

  fetchPluginsDir: async () => {
    try {
      const pluginsDir = await invoke<string>('get_plugins_dir');
      set({ pluginsDir });
    } catch (error) {
      console.error('Failed to fetch plugins directory:', error);
    }
  },
}));
