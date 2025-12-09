import { create } from "zustand";

export type ProxyStatus = "stopped" | "starting" | "running" | "stopping" | "error";

export interface ProxyConfig {
  port: number;
  enableHttp2: boolean;
  enableQuic: boolean;
  quicPort: number | null;
}

interface ProxyState {
  status: ProxyStatus;
  config: ProxyConfig;
  error: string | null;
  setStatus: (status: ProxyStatus) => void;
  setConfig: (config: Partial<ProxyConfig>) => void;
  setError: (error: string | null) => void;
}

export const useProxyStore = create<ProxyState>()((set) => ({
  status: "stopped",
  config: {
    port: 8899,
    enableHttp2: true,
    enableQuic: false,
    quicPort: null,
  },
  error: null,
  setStatus: (status) => set({ status }),
  setConfig: (config) => set((state) => ({ config: { ...state.config, ...config } })),
  setError: (error) => set({ error }),
}));
