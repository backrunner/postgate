import { memo, CSSProperties, useMemo } from "react";
import { CapturedRequest } from "@/stores/capture";
import { ColumnConfig } from "@/stores/columns";

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

const STATUS_CLASSES = {
  success: "text-emerald-500",
  redirect: "text-blue-500",
  clientError: "text-amber-500",
  serverError: "text-red-500",
  pending: "text-zinc-400",
} as const;

const getMethodClass = (method: string): string =>
  METHOD_CLASSES[method] || "text-zinc-500";

const getStatusClass = (status: number | null): string => {
  if (status === null) return STATUS_CLASSES.pending;
  if (status < 300) return STATUS_CLASSES.success;
  if (status < 400) return STATUS_CLASSES.redirect;
  if (status < 500) return STATUS_CLASSES.clientError;
  return STATUS_CLASSES.serverError;
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

interface RequestListItemProps {
  request: CapturedRequest;
  isSelected: boolean;
  onClick: () => void;
  style: CSSProperties;
  columns: ColumnConfig[];
}

// Custom comparison function to prevent unnecessary re-renders
const areEqual = (prev: RequestListItemProps, next: RequestListItemProps): boolean => {
  if (prev.isSelected !== next.isSelected) return false;
  if (prev.style !== next.style) return false;
  if (prev.columns !== next.columns) return false;
  const p = prev.request;
  const n = next.request;
  return (
    p.id === n.id &&
    p.method === n.method &&
    p.responseStatus === n.responseStatus &&
    p.host === n.host &&
    p.path === n.path &&
    p.durationMs === n.durationMs &&
    p.responseSize === n.responseSize &&
    p.protocol === n.protocol &&
    p.remoteAddr === n.remoteAddr &&
    p.tlsInfo === n.tlsInfo &&
    p.matchedRules.length === n.matchedRules.length
  );
};

export const RequestListItem = memo(function RequestListItem({
  request,
  isSelected,
  onClick,
  style,
  columns,
}: RequestListItemProps) {
  const visibleColumns = useMemo(
    () => columns.filter((col) => col.visible),
    [columns]
  );

  const baseClass = isSelected
    ? "flex cursor-pointer items-center h-full select-none text-xs font-mono bg-accent"
    : "flex cursor-pointer items-center h-full select-none text-xs font-mono hover:bg-accent/50";

  return (
    <div style={style} onClick={onClick} className={baseClass}>
      {visibleColumns.map((col) => (
        <CellContent key={col.id} column={col} request={request} />
      ))}
    </div>
  );
}, areEqual);

interface CellContentProps {
  column: ColumnConfig;
  request: CapturedRequest;
}

function CellContent({ column, request }: CellContentProps) {
  const isFlex = column.width === 0;
  const style: React.CSSProperties = isFlex
    ? { flex: 1, minWidth: column.minWidth }
    : { width: column.width, flexShrink: 0 };

  const baseClasses = "truncate";
  
  switch (column.id) {
    case "method":
      return (
        <span
          className={`${baseClasses} pl-2 font-semibold ${getMethodClass(request.method)}`}
          style={style}
        >
          {request.method}
        </span>
      );

    case "status":
      return (
        <span
          className={`${baseClasses} text-center ${getStatusClass(request.responseStatus)}`}
          style={style}
        >
          {request.responseStatus ?? "-"}
        </span>
      );

    case "protocol":
      return (
        <span
          className={`${baseClasses} text-muted-foreground`}
          style={style}
        >
          {PROTOCOL_DISPLAY[request.protocol] || request.protocol.toUpperCase()}
        </span>
      );

    case "host":
      return (
        <span
          className={`${baseClasses} text-muted-foreground`}
          style={style}
          title={request.host}
        >
          {request.tlsInfo && <span className="text-emerald-500 mr-1">S</span>}
          {request.matchedRules.length > 0 && <span className="text-blue-500 mr-1">R</span>}
          {request.host}
        </span>
      );

    case "path":
      return (
        <span
          className={`${baseClasses} pr-2`}
          style={style}
          title={request.path}
        >
          {request.path}
        </span>
      );

    case "remoteAddr":
      return (
        <span
          className={`${baseClasses} text-muted-foreground`}
          style={style}
          title={request.remoteAddr || undefined}
        >
          {request.remoteAddr || "-"}
        </span>
      );

    case "duration":
      return (
        <span
          className={`${baseClasses} text-right text-muted-foreground`}
          style={style}
        >
          {formatDuration(request.durationMs)}
        </span>
      );

    case "size":
      return (
        <span
          className={`${baseClasses} text-right text-muted-foreground pr-2`}
          style={style}
        >
          {formatSize(request.responseSize)}
        </span>
      );

    default:
      return null;
  }
}
