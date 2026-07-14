import { create } from "zustand";
import { persist } from "zustand/middleware";

export type ProxyStatus = "stopped" | "starting" | "running" | "stopping" | "error";

export interface ProxyConfig {
  port: number;
  enableHttp2: boolean;
  enableQuic: boolean;
  quicPort: number | null;
  debugPort: number;
}

interface ProxyState {
  status: ProxyStatus;
  config: ProxyConfig;
  error: string | null;
  setStatus: (status: ProxyStatus) => void;
  setConfig: (config: Partial<ProxyConfig>) => void;
  setError: (error: string | null) => void;
}

const DEFAULT_PROXY_CONFIG: ProxyConfig = {
  port: 8899,
  enableHttp2: true,
  enableQuic: false,
  quicPort: null,
  debugPort: 9229,
};

function validPort(value: unknown, fallback: number): number {
  return typeof value === "number" &&
    Number.isInteger(value) &&
    value >= 1 &&
    value <= 65535
    ? value
    : fallback;
}

function normalizeConfig(
  config: Partial<ProxyConfig>,
  fallback: ProxyConfig = DEFAULT_PROXY_CONFIG,
): ProxyConfig {
  return {
    port: validPort(config.port, fallback.port),
    enableHttp2:
      typeof config.enableHttp2 === "boolean" ? config.enableHttp2 : fallback.enableHttp2,
    enableQuic:
      typeof config.enableQuic === "boolean" ? config.enableQuic : fallback.enableQuic,
    quicPort:
      config.quicPort === null
        ? null
        : config.quicPort === undefined
          ? fallback.quicPort
          : validPort(config.quicPort, fallback.quicPort ?? fallback.port),
    debugPort: validPort(config.debugPort, fallback.debugPort),
  };
}

export const useProxyStore = create<ProxyState>()(
  persist(
    (set) => ({
      status: "stopped",
      config: DEFAULT_PROXY_CONFIG,
      error: null,
      setStatus: (status) => set({ status }),
      setConfig: (config) =>
        set((state) => ({ config: normalizeConfig(config, state.config) })),
      setError: (error) => set({ error }),
    }),
    {
      name: "postgate-proxy",
      partialize: (state) => ({ config: state.config }),
      onRehydrateStorage: () => (state) => {
        if (state) {
          state.config = normalizeConfig(state.config);
          state.status = "stopped";
          state.error = null;
        }
      },
    },
  ),
);
