// Updater store for managing app auto-updates

import { create } from "zustand";
import { check, Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { getVersion } from "@tauri-apps/api/app";

export interface UpdateInfo {
  version: string;
  date: string | null;
  body: string | null;
}

interface UpdaterState {
  // Current app version
  currentVersion: string;
  
  // Update status
  isChecking: boolean;
  isDownloading: boolean;
  isDownloaded: boolean;
  isInstalling: boolean;
  downloadProgress: number;
  
  // Update info
  updateAvailable: boolean;
  updateInfo: UpdateInfo | null;
  error: string | null;
  lastChecked: number | null;
  
  // Auto-update settings
  autoCheck: boolean;
  autoDownload: boolean;
  
  // Actions
  fetchCurrentVersion: () => Promise<void>;
  checkForUpdates: (options?: CheckForUpdatesOptions) => Promise<boolean>;
  downloadUpdate: () => Promise<void>;
  installUpdate: () => Promise<void>;
  downloadAndInstall: () => Promise<void>;
  setAutoCheck: (enabled: boolean) => void;
  setAutoDownload: (enabled: boolean) => void;
}

interface CheckForUpdatesOptions {
  silent?: boolean;
}

// Store the update object for later use
let pendingUpdate: Update | null = null;
let updaterInitialized = false;

const LAST_CHECKED_KEY = "postgate:lastUpdateCheck";

function persistLastChecked(timestamp: number) {
  localStorage.setItem(LAST_CHECKED_KEY, String(timestamp));
}

export const useUpdaterStore = create<UpdaterState>((set, get) => ({
  // Initial state
  currentVersion: "0.0.0",
  isChecking: false,
  isDownloading: false,
  isDownloaded: false,
  isInstalling: false,
  downloadProgress: 0,
  updateAvailable: false,
  updateInfo: null,
  error: null,
  lastChecked: null,
  autoCheck: true,
  autoDownload: false,

  // Actions
  fetchCurrentVersion: async () => {
    try {
      const version = await getVersion();
      set({ currentVersion: version });
    } catch (e) {
      console.error("Failed to get app version:", e);
    }
  },

  checkForUpdates: async (options = {}) => {
    const { silent = false } = options;
    if (get().isChecking) {
      return get().updateAvailable;
    }
    set((state) => ({
      isChecking: true,
      error: silent ? state.error : null,
    }));
    
    try {
      const update = await check();
      const checkedAt = Date.now();
      persistLastChecked(checkedAt);
      
      if (update) {
        if (pendingUpdate) {
          await pendingUpdate.close().catch(() => undefined);
        }
        pendingUpdate = update;
        set({
          updateAvailable: true,
          isDownloaded: false,
          updateInfo: {
            version: update.version,
            date: update.date || null,
            body: update.body || null,
          },
          lastChecked: checkedAt,
          isChecking: false,
          error: null,
        });
        
        // Auto-download if enabled
        if (get().autoDownload) {
          void get().downloadUpdate();
        }
        
        return true;
      } else {
        if (pendingUpdate) {
          await pendingUpdate.close().catch(() => undefined);
          pendingUpdate = null;
        }
        set({
          updateAvailable: false,
          isDownloaded: false,
          updateInfo: null,
          lastChecked: checkedAt,
          isChecking: false,
          error: null,
        });
        return false;
      }
    } catch (e) {
      set((state) => ({
        error: silent ? state.error : String(e),
        isChecking: false,
      }));
      return false;
    }
  },

  downloadUpdate: async () => {
    if (!pendingUpdate) {
      set({ error: "No update available" });
      return;
    }
    if (get().isDownloading || get().isDownloaded) return;

    set({ isDownloading: true, isDownloaded: false, downloadProgress: 0, error: null });

    try {
      let downloaded = 0;
      let contentLength = 0;

      await pendingUpdate.download((event) => {
        switch (event.event) {
          case "Started":
            contentLength = event.data.contentLength || 0;
            console.log(`Started downloading ${contentLength} bytes`);
            break;
          case "Progress": {
            downloaded += event.data.chunkLength;
            const progress = contentLength > 0 
              ? Math.round((downloaded / contentLength) * 100) 
              : 0;
            set({ downloadProgress: progress });
            break;
          }
          case "Finished":
            console.log("Download finished");
            set({ isDownloading: false, isDownloaded: true, downloadProgress: 100 });
            break;
        }
      });
      set({ isDownloading: false, isDownloaded: true, downloadProgress: 100 });
    } catch (e) {
      set({
        error: String(e),
        isDownloading: false,
        isDownloaded: false,
      });
    }
  },

  installUpdate: async () => {
    if (!pendingUpdate || !get().isDownloaded) {
      set({ error: "Download the update before installing it" });
      return;
    }
    if (get().isInstalling) return;

    set({ isInstalling: true, error: null });
    try {
      await pendingUpdate.install();
      await pendingUpdate.close().catch(() => undefined);
      pendingUpdate = null;
      await relaunch();
    } catch (e) {
      set({ error: String(e), isInstalling: false });
    }
  },

  downloadAndInstall: async () => {
    if (!get().isDownloaded) {
      await get().downloadUpdate();
    }
    if (get().isDownloaded) {
      await get().installUpdate();
    }
  },

  setAutoCheck: (enabled) => {
    set({ autoCheck: enabled });
    // Persist to localStorage
    localStorage.setItem("postgate:autoCheckUpdates", String(enabled));
  },

  setAutoDownload: (enabled) => {
    set({ autoDownload: enabled });
    // Persist to localStorage
    localStorage.setItem("postgate:autoDownloadUpdates", String(enabled));
  },
}));

// Initialize settings from localStorage
export function initUpdaterSettings() {
  if (updaterInitialized) return;
  updaterInitialized = true;
  
  const autoCheck = localStorage.getItem("postgate:autoCheckUpdates");
  const autoDownload = localStorage.getItem("postgate:autoDownloadUpdates");
  const lastChecked = Number(localStorage.getItem(LAST_CHECKED_KEY));
  
  useUpdaterStore.setState((state) => ({
    autoCheck: autoCheck === null ? state.autoCheck : autoCheck === "true",
    autoDownload: autoDownload === null ? state.autoDownload : autoDownload === "true",
    lastChecked: Number.isFinite(lastChecked) && lastChecked > 0 ? lastChecked : null,
  }));

  const store = useUpdaterStore.getState();
  
  // Fetch current version
  void store.fetchCurrentVersion();
  
  // Auto-check for updates on startup if enabled
  if (useUpdaterStore.getState().autoCheck) {
    setTimeout(() => {
      if (useUpdaterStore.getState().autoCheck) {
        void useUpdaterStore.getState().checkForUpdates({ silent: true });
      }
    }, 3000); // Delay 3 seconds after startup
  }
}
