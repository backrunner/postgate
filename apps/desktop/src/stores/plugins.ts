import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';
import { listen, UnlistenFn } from '@tauri-apps/api/event';

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

export interface PluginToast {
  message: string;
  toast_type: 'info' | 'success' | 'warning' | 'error' | null;
}

interface PluginsState {
  plugins: PluginInfo[];
  panels: PluginPanel[];
  pluginsDir: string | null;
  isLoading: boolean;
  error: string | null;
  toasts: PluginToast[];
  
  // Actions
  fetchPlugins: () => Promise<void>;
  discoverPlugins: () => Promise<void>;
  loadPlugin: (pluginId: string, config?: Record<string, string>) => Promise<void>;
  unloadPlugin: (pluginId: string) => Promise<void>;
  togglePlugin: (pluginId: string, enabled: boolean) => Promise<void>;
  uninstallPlugin: (pluginId: string) => Promise<void>;
  fetchPanels: () => Promise<void>;
  fetchPluginsDir: () => Promise<void>;
  installPluginFromNpm: (packageName: string) => Promise<PluginInfo>;
  installPluginFromPath: (sourcePath: string) => Promise<PluginInfo>;
  
  // Event handling
  addPanel: (panel: PluginPanel) => void;
  removePanel: (panelId: string) => void;
  addToast: (toast: PluginToast) => void;
  clearToast: (index: number) => void;
  setupEventListeners: () => Promise<UnlistenFn[]>;
}

export const usePluginsStore = create<PluginsState>((set, get) => ({
  plugins: [],
  panels: [],
  pluginsDir: null,
  isLoading: false,
  error: null,
  toasts: [],

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

  installPluginFromNpm: async (packageName: string) => {
    set({ isLoading: true, error: null });
    try {
      const plugin = await invoke<PluginInfo>('install_plugin_from_npm', { packageName });
      await get().fetchPlugins();
      set({ isLoading: false });
      return plugin;
    } catch (error) {
      set({ error: String(error), isLoading: false });
      throw error;
    }
  },

  installPluginFromPath: async (sourcePath: string) => {
    set({ isLoading: true, error: null });
    try {
      const plugin = await invoke<PluginInfo>('install_plugin_from_path', { sourcePath });
      await get().fetchPlugins();
      set({ isLoading: false });
      return plugin;
    } catch (error) {
      set({ error: String(error), isLoading: false });
      throw error;
    }
  },

  // Event handling for real-time updates from plugins
  addPanel: (panel: PluginPanel) => {
    set((state) => {
      // Avoid duplicates
      const exists = state.panels.some((p) => p.id === panel.id);
      if (exists) {
        return { panels: state.panels.map((p) => (p.id === panel.id ? panel : p)) };
      }
      return { panels: [...state.panels, panel] };
    });
  },

  removePanel: (panelId: string) => {
    set((state) => ({
      panels: state.panels.filter((p) => p.id !== panelId),
    }));
  },

  addToast: (toast: PluginToast) => {
    set((state) => ({
      toasts: [...state.toasts, toast],
    }));
    // Auto-clear toast after 5 seconds
    setTimeout(() => {
      set((state) => ({
        toasts: state.toasts.slice(1),
      }));
    }, 5000);
  },

  clearToast: (index: number) => {
    set((state) => ({
      toasts: state.toasts.filter((_, i) => i !== index),
    }));
  },

  setupEventListeners: async () => {
    const unlisteners: UnlistenFn[] = [];

    // Listen for panel registration events
    const unlistenPanelRegistered = await listen<PluginPanel>('plugin:panel-registered', (event) => {
      console.log('Panel registered:', event.payload);
      get().addPanel(event.payload);
    });
    unlisteners.push(unlistenPanelRegistered);

    // Listen for panel unregistration events
    const unlistenPanelUnregistered = await listen<string>('plugin:panel-unregistered', (event) => {
      console.log('Panel unregistered:', event.payload);
      get().removePanel(event.payload);
    });
    unlisteners.push(unlistenPanelUnregistered);

    // Listen for toast events
    const unlistenToast = await listen<PluginToast>('plugin:toast', (event) => {
      console.log('Plugin toast:', event.payload);
      get().addToast(event.payload);
    });
    unlisteners.push(unlistenToast);

    return unlisteners;
  },
}));
