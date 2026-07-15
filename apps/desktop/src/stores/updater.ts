import { getVersion } from "@tauri-apps/api/app";
import { Channel, invoke } from "@tauri-apps/api/core";
import { relaunch } from "@tauri-apps/plugin-process";
import { create } from "zustand";

export type UpdateChannel = "stable" | "beta";

export interface UpdateInfo {
  version: string;
  date: string | null;
  body: string | null;
  channel: UpdateChannel;
}

type DownloadEvent =
  | { event: "Started"; data: { contentLength?: number } }
  | { event: "Progress"; data: { chunkLength: number } }
  | { event: "Finished" };

interface UpdaterState {
  currentVersion: string;
  channel: UpdateChannel;
  isChecking: boolean;
  isDownloading: boolean;
  isDownloaded: boolean;
  isInstalling: boolean;
  downloadProgress: number;
  updateAvailable: boolean;
  updateInfo: UpdateInfo | null;
  error: string | null;
  lastChecked: number | null;
  autoCheck: boolean;
  autoDownload: boolean;
  fetchCurrentVersion: () => Promise<void>;
  checkForUpdates: (options?: CheckForUpdatesOptions) => Promise<boolean>;
  downloadUpdate: () => Promise<void>;
  installUpdate: () => Promise<void>;
  downloadAndInstall: () => Promise<void>;
  setChannel: (channel: UpdateChannel) => Promise<void>;
  setAutoCheck: (enabled: boolean) => void;
  setAutoDownload: (enabled: boolean) => void;
}

interface CheckForUpdatesOptions {
  silent?: boolean;
}

let updaterInitialized = false;

const AUTO_CHECK_KEY = "postgate:autoCheckUpdates";
const AUTO_DOWNLOAD_KEY = "postgate:autoDownloadUpdates";
const UPDATE_CHANNEL_KEY = "postgate:updateChannel";
const LEGACY_LAST_CHECKED_KEY = "postgate:lastUpdateCheck";

function isUpdateChannel(value: string | null): value is UpdateChannel {
  return value === "stable" || value === "beta";
}

function lastCheckedKey(channel: UpdateChannel) {
  return `postgate:lastUpdateCheck:${channel}`;
}

function readLastChecked(channel: UpdateChannel): number | null {
  const stored = localStorage.getItem(lastCheckedKey(channel))
    ?? (channel === "stable" ? localStorage.getItem(LEGACY_LAST_CHECKED_KEY) : null);
  const timestamp = Number(stored);
  return Number.isFinite(timestamp) && timestamp > 0 ? timestamp : null;
}

function persistLastChecked(channel: UpdateChannel, timestamp: number) {
  localStorage.setItem(lastCheckedKey(channel), String(timestamp));
}

function emptyUpdateState(channel: UpdateChannel) {
  return {
    channel,
    updateAvailable: false,
    updateInfo: null,
    isDownloaded: false,
    isDownloading: false,
    isInstalling: false,
    downloadProgress: 0,
    error: null,
    lastChecked: readLastChecked(channel),
  };
}

export const useUpdaterStore = create<UpdaterState>((set, get) => ({
  currentVersion: "0.0.0",
  channel: "stable",
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

  fetchCurrentVersion: async () => {
    try {
      const version = await getVersion();
      const storedChannel = localStorage.getItem(UPDATE_CHANNEL_KEY);
      const channel = isUpdateChannel(storedChannel)
        ? storedChannel
        : version.includes("-") ? "beta" : "stable";
      set({
        currentVersion: version,
        channel,
        lastChecked: readLastChecked(channel),
      });
    } catch (error) {
      console.error("Failed to get app version:", error);
    }
  },

  checkForUpdates: async (options = {}) => {
    const { silent = false } = options;
    if (get().isChecking) {
      return get().updateAvailable;
    }

    const channel = get().channel;
    set((state) => ({
      isChecking: true,
      error: silent ? state.error : null,
    }));

    try {
      const update = await invoke<UpdateInfo | null>("check_for_update", { channel });
      if (get().channel !== channel) {
        await invoke("clear_pending_update").catch(() => undefined);
        set({ isChecking: false });
        return false;
      }

      const checkedAt = Date.now();
      persistLastChecked(channel, checkedAt);

      if (update) {
        set({
          updateAvailable: true,
          isDownloaded: false,
          downloadProgress: 0,
          updateInfo: update,
          lastChecked: checkedAt,
          isChecking: false,
          error: null,
        });

        if (get().autoDownload) {
          void get().downloadUpdate();
        }
        return true;
      }

      set({
        updateAvailable: false,
        isDownloaded: false,
        downloadProgress: 0,
        updateInfo: null,
        lastChecked: checkedAt,
        isChecking: false,
        error: null,
      });
      return false;
    } catch (error) {
      set((state) => ({
        error: silent ? state.error : String(error),
        isChecking: false,
      }));
      return false;
    }
  },

  downloadUpdate: async () => {
    if (!get().updateAvailable) {
      set({ error: "No update available" });
      return;
    }
    if (get().isDownloading || get().isDownloaded) return;

    set({ isDownloading: true, isDownloaded: false, downloadProgress: 0, error: null });
    let downloaded = 0;
    let contentLength = 0;
    const onEvent = new Channel<DownloadEvent>();
    onEvent.onmessage = (event) => {
      switch (event.event) {
        case "Started":
          contentLength = event.data.contentLength ?? 0;
          break;
        case "Progress":
          downloaded += event.data.chunkLength;
          set({
            downloadProgress: contentLength > 0
              ? Math.round((downloaded / contentLength) * 100)
              : 0,
          });
          break;
        case "Finished":
          set({ downloadProgress: 100 });
          break;
      }
    };

    try {
      await invoke("download_update", { onEvent });
      set({ isDownloading: false, isDownloaded: true, downloadProgress: 100 });
    } catch (error) {
      set({
        error: String(error),
        isDownloading: false,
        isDownloaded: false,
      });
    }
  },

  installUpdate: async () => {
    if (!get().isDownloaded) {
      set({ error: "Download the update before installing it" });
      return;
    }
    if (get().isInstalling) return;

    set({ isInstalling: true, error: null });
    try {
      await invoke("install_update");
      await relaunch();
    } catch (error) {
      set({ error: String(error), isInstalling: false });
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

  setChannel: async (channel) => {
    if (channel === get().channel) return;
    localStorage.setItem(UPDATE_CHANNEL_KEY, channel);
    set({
      ...emptyUpdateState(channel),
      isChecking: false,
    });
    await invoke("clear_pending_update").catch(() => undefined);
    await get().checkForUpdates();
  },

  setAutoCheck: (enabled) => {
    set({ autoCheck: enabled });
    localStorage.setItem(AUTO_CHECK_KEY, String(enabled));
  },

  setAutoDownload: (enabled) => {
    set({ autoDownload: enabled });
    localStorage.setItem(AUTO_DOWNLOAD_KEY, String(enabled));
  },
}));

export function initUpdaterSettings() {
  if (updaterInitialized) return;
  updaterInitialized = true;

  const autoCheck = localStorage.getItem(AUTO_CHECK_KEY);
  const autoDownload = localStorage.getItem(AUTO_DOWNLOAD_KEY);
  const storedChannel = localStorage.getItem(UPDATE_CHANNEL_KEY);
  const channel = isUpdateChannel(storedChannel) ? storedChannel : "stable";

  useUpdaterStore.setState((state) => ({
    autoCheck: autoCheck === null ? state.autoCheck : autoCheck === "true",
    autoDownload: autoDownload === null ? state.autoDownload : autoDownload === "true",
    channel,
    lastChecked: readLastChecked(channel),
  }));

  const store = useUpdaterStore.getState();
  void (async () => {
    await store.fetchCurrentVersion();
    if (!useUpdaterStore.getState().autoCheck) return;

    setTimeout(() => {
      if (useUpdaterStore.getState().autoCheck) {
        void useUpdaterStore.getState().checkForUpdates({ silent: true });
      }
    }, 3000);
  })();
}
