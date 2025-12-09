import { create } from "zustand";
import { persist } from "zustand/middleware";

export interface ColumnConfig {
  id: string;
  label: string;
  visible: boolean;
  width: number;
  minWidth: number;
  resizable: boolean;
}

export type ColumnId =
  | "method"
  | "status"
  | "protocol"
  | "host"
  | "path"
  | "remoteAddr"
  | "duration"
  | "size";

const defaultColumns: ColumnConfig[] = [
  { id: "method", label: "Method", visible: true, width: 70, minWidth: 55, resizable: true },
  { id: "status", label: "Status", visible: true, width: 55, minWidth: 45, resizable: true },
  { id: "protocol", label: "Protocol", visible: false, width: 75, minWidth: 60, resizable: true },
  { id: "host", label: "Host", visible: true, width: 180, minWidth: 100, resizable: true },
  { id: "path", label: "Path", visible: true, width: 0, minWidth: 120, resizable: true }, // 0 = flex
  { id: "remoteAddr", label: "Server IP", visible: false, width: 140, minWidth: 100, resizable: true },
  { id: "duration", label: "Time", visible: true, width: 65, minWidth: 50, resizable: true },
  { id: "size", label: "Size", visible: true, width: 60, minWidth: 45, resizable: true },
];

interface ColumnsState {
  columns: ColumnConfig[];
  setColumnVisible: (id: string, visible: boolean) => void;
  setColumnWidth: (id: string, width: number) => void;
  resetColumns: () => void;
}

export const useColumnsStore = create<ColumnsState>()(
  persist(
    (set) => ({
      columns: defaultColumns,

      setColumnVisible: (id, visible) =>
        set((state) => ({
          columns: state.columns.map((col) =>
            col.id === id ? { ...col, visible } : col
          ),
        })),

      setColumnWidth: (id, width) =>
        set((state) => ({
          columns: state.columns.map((col) =>
            col.id === id ? { ...col, width: Math.max(col.minWidth, width) } : col
          ),
        })),

      resetColumns: () => set({ columns: defaultColumns }),
    }),
    {
      name: "postgate-columns",
    }
  )
);
