import { memo, CSSProperties } from "react";
import { CapturedRequest, useCaptureStore } from "@/stores/capture";
import { ColumnConfig } from "@/stores/columns";
import { StreamConnection, useStreamStore } from "@/stores/stream";

// Pre-computed class mappings for zero runtime lookup
const METHOD_CLASSES: Record<string, string> = {
  GET: "text-emerald-500",
  POST: "text-blue-500",
  PUT: "text-amber-500",
  PATCH: "text-amber-500",
  DELETE: "text-red-500",
  OPTIONS: "text-zinc-500",
  HEAD: "text-zinc-500",
};

const getMethodClass = (method: string): string =>
  METHOD_CLASSES[method] || "text-zinc-500";

const getStatusClass = (status: number | null): string => {
  if (status === null) return "text-zinc-400";
  if (status < 300) return "text-emerald-500";
  if (status < 400) return "text-blue-500";
  if (status < 500) return "text-amber-500";
  return "text-red-500";
};

const SIZE_UNITS = ["B", "K", "M", "G"];

const formatSize = (bytes: number | null): string => {
  if (bytes === null) return "-";
  if (bytes === 0) return "0";
  if (bytes < 1024) return `${bytes}`;
  const i = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), 3);
  const val = bytes / Math.pow(1024, i);
  return val < 10 ? `${val.toFixed(1)}${SIZE_UNITS[i]}` : `${Math.round(val)}${SIZE_UNITS[i]}`;
};

const formatDuration = (ms: number | null): string => {
  if (ms === null) return "-";
  if (ms < 1000) return `${ms}ms`;
  if (ms < 10000) return `${(ms / 1000).toFixed(1)}s`;
  return `${Math.round(ms / 1000)}s`;
};

const PROTOCOL_DISPLAY: Record<string, string> = {
  http1: "HTTP/1.1",
  http2: "HTTP/2",
  quic: "HTTP/3",
  websocket: "WS",
  sse: "SSE",
};

// Pre-computed base styles - avoid object creation during render
const BASE_ROW_STYLE: CSSProperties = {
  position: "absolute",
  top: 0,
  left: 0,
  width: "100%",
  contain: "layout style paint",
  willChange: "transform",
};

// Pre-computed class strings to avoid runtime concatenation
const ROW_BASE = "flex cursor-pointer items-center h-full select-none text-xs font-mono relative";
const ROW_SELECTED = `${ROW_BASE} bg-accent`;
const ROW_MATCHED = `${ROW_BASE} bg-indigo-50/50 dark:bg-indigo-950/30 hover:bg-indigo-100/50 dark:hover:bg-indigo-900/30 font-semibold`;
const ROW_NORMAL = `${ROW_BASE} hover:bg-accent/50`;

const CELL_BASE = "truncate";
const MATCHED_COLOR = "text-indigo-700 dark:text-indigo-300";
const MUTED_COLOR = "text-muted-foreground";

interface RequestListItemProps {
  requestId: string;
  isSelected: boolean;
  onSelect: (id: string) => void;
  translateY: number;
  height: number;
  columns: ColumnConfig[];
}

// Request data is subscribed by ID inside the row, so parent updates only
// need to compare stable virtualization/layout props.
const areEqual = (prev: RequestListItemProps, next: RequestListItemProps): boolean => {
  if (prev.requestId !== next.requestId) return false;
  if (prev.isSelected !== next.isSelected) return false;
  if (prev.translateY !== next.translateY) return false;
  if (prev.height !== next.height) return false;
  if (prev.columns !== next.columns) return false;
  if (prev.onSelect !== next.onSelect) return false;
  return true;
};

export const RequestListItem = memo(function RequestListItem({
  requestId,
  isSelected,
  onSelect,
  translateY,
  height,
  columns,
}: RequestListItemProps) {
  const request = useCaptureStore((state) => state.requestMap.get(requestId));
  const isStream = request?.protocol === "websocket" || request?.protocol === "sse";
  const streamConnection = useStreamStore((state) =>
    isStream ? state.connections.get(requestId) : undefined
  );

  if (!request) return null;

  const hasMatchedRules = request.matchedRules.length > 0;

  // Subscribe to ONLY this row's stream connection. For non-stream protocols
  // the selector always returns `undefined`, so those rows never re-render
  // when other streams update. For stream rows, only the row whose
  // connection actually changed will re-render — see RequestList for why
  // we do per-row subscription instead of pulling the whole Map into the
  // parent.
  // Use pre-computed class strings
  const rowClass = isSelected ? ROW_SELECTED : hasMatchedRules ? ROW_MATCHED : ROW_NORMAL;
  
  // Merge style with transform - minimal object creation
  const style: CSSProperties = {
    ...BASE_ROW_STYLE,
    height,
    transform: `translate3d(0, ${translateY}px, 0)`,
  };

  return (
    <div
      style={style}
      onClick={() => onSelect(request.id)}
      className={rowClass}
    >
      {hasMatchedRules && (
        <div className="absolute left-0 top-0 bottom-0 w-0.5 bg-indigo-500 dark:bg-indigo-400" />
      )}
      {columns.map((col) => 
        col.visible ? (
          <Cell
            key={col.id}
            columnId={col.id}
            width={col.width}
            minWidth={col.minWidth}
            request={request}
            hasMatchedRules={hasMatchedRules}
            streamConnection={streamConnection}
          />
        ) : null
      )}
    </div>
  );
}, areEqual);

// Inline cell component - no memo needed since parent handles it
interface CellProps {
  columnId: string;
  width: number;
  minWidth?: number;
  request: CapturedRequest;
  hasMatchedRules: boolean;
  streamConnection?: StreamConnection;
}

function Cell({ columnId, width, minWidth, request, hasMatchedRules, streamConnection }: CellProps) {
  const isFlex = width === 0;
  const style: CSSProperties = isFlex
    ? { flex: 1, minWidth }
    : { width, flexShrink: 0 };

  const colorClass = hasMatchedRules ? MATCHED_COLOR : MUTED_COLOR;
  
  switch (columnId) {
    case "method":
      return (
        <span
          className={`${CELL_BASE} pl-2.5 font-semibold ${hasMatchedRules ? MATCHED_COLOR : getMethodClass(request.method)}`}
          style={style}
        >
          {hasMatchedRules && (
            <svg className="inline-block h-3 w-3 mr-1 text-indigo-500" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <polygon points="13 2 3 14 12 14 11 22 21 10 12 10 13 2" />
            </svg>
          )}
          {request.method}
        </span>
      );

    case "status":
      return (
        <span
          className={`${CELL_BASE} text-center ${hasMatchedRules ? MATCHED_COLOR : getStatusClass(request.responseStatus)}`}
          style={style}
        >
          {request.responseStatus ?? "-"}
        </span>
      );

    case "protocol": {
      const isStreaming = request.protocol === "websocket" || request.protocol === "sse";
      const isLive = streamConnection && !streamConnection.isEnded;
      return (
        <span className={`${CELL_BASE} ${colorClass} flex items-center gap-1`} style={style}>
          {PROTOCOL_DISPLAY[request.protocol] || request.protocol.toUpperCase()}
          {isStreaming && isLive && (
            <span className="w-1.5 h-1.5 rounded-full bg-emerald-500 animate-pulse shrink-0" />
          )}
          {streamConnection && streamConnection.messageCount > 0 && (
            <span className="text-[10px] text-muted-foreground">
              ({streamConnection.messageCount})
            </span>
          )}
        </span>
      );
    }

    case "host": {
      const isSecure = !!request.tlsInfo;
      const hostClass = hasMatchedRules 
        ? MATCHED_COLOR 
        : (isSecure ? "text-emerald-500" : MUTED_COLOR);
      return (
        <span className={`${CELL_BASE} ${hostClass}`} style={style}>
          {request.host}
        </span>
      );
    }

    case "path":
      return (
        <span
          className={`${CELL_BASE} pr-2 ${hasMatchedRules ? MATCHED_COLOR : ""}`}
          style={style}
        >
          {request.path}
        </span>
      );

    case "remoteAddr":
      return (
        <span className={`${CELL_BASE} ${colorClass}`} style={style}>
          {request.remoteAddr || "-"}
        </span>
      );

    case "duration":
      return (
        <span className={`${CELL_BASE} text-right ${colorClass}`} style={style}>
          {formatDuration(request.durationMs)}
        </span>
      );

    case "size":
      return (
        <span className={`${CELL_BASE} text-right ${colorClass} pr-2`} style={style}>
          {formatSize(request.responseSize)}
        </span>
      );

    default:
      return null;
  }
}
