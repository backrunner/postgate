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
  downloadAndInstall: () => Promise<void>;
  setAutoCheck: (enabled: boolean) => void;
  setAutoDownload: (enabled: boolean) => void;
}

interface CheckForUpdatesOptions {
  silent?: boolean;
}

// Store the update object for later use
let pendingUpdate: Update | null = null;

export const useUpdaterStore = create<UpdaterState>((set, get) => ({
  // Initial state
  currentVersion: "0.0.0",
  isChecking: false,
  isDownloading: false,
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
    set((state) => ({
      isChecking: true,
      error: silent ? state.error : null,
    }));
    
    try {
      const update = await check();
      
      if (update) {
        pendingUpdate = update;
        set({
          updateAvailable: true,
          updateInfo: {
            version: update.version,
            date: update.date || null,
            body: update.body || null,
          },
          lastChecked: Date.now(),
          isChecking: false,
        });
        
        // Auto-download if enabled
        if (get().autoDownload) {
          get().downloadAndInstall();
        }
        
        return true;
      } else {
        set({
          updateAvailable: false,
          updateInfo: null,
          lastChecked: Date.now(),
          isChecking: false,
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

  downloadAndInstall: async () => {
    if (!pendingUpdate) {
      set({ error: "No update available" });
      return;
    }

    set({ isDownloading: true, downloadProgress: 0, error: null });

    try {
      let downloaded = 0;
      let contentLength = 0;

      await pendingUpdate.downloadAndInstall((event) => {
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
            set({ isDownloading: false, isInstalling: true });
            break;
        }
      });

      // After installation, relaunch the app
      console.log("Update installed, relaunching...");
      await relaunch();
    } catch (e) {
      set({
        error: String(e),
        isDownloading: false,
        isInstalling: false,
      });
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
  const store = useUpdaterStore.getState();
  
  const autoCheck = localStorage.getItem("postgate:autoCheckUpdates");
  const autoDownload = localStorage.getItem("postgate:autoDownloadUpdates");
  
  if (autoCheck !== null) {
    store.setAutoCheck(autoCheck === "true");
  }
  if (autoDownload !== null) {
    store.setAutoDownload(autoDownload === "true");
  }
  
  // Fetch current version
  store.fetchCurrentVersion();
  
  // Auto-check for updates on startup if enabled
  if (useUpdaterStore.getState().autoCheck) {
    setTimeout(() => {
      store.checkForUpdates({ silent: true });
    }, 3000); // Delay 3 seconds after startup
  }
}
