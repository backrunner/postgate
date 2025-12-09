import { memo, CSSProperties } from "react";
import { CapturedRequest } from "@/stores/capture";
import { cn, getStatusClass, getMethodClass, formatDuration, formatBytes } from "@/lib/utils";
import { FileCode, Lock } from "lucide-react";

interface RequestListItemProps {
  request: CapturedRequest;
  isSelected: boolean;
  onClick: () => void;
  style: CSSProperties;
}

export const RequestListItem = memo(function RequestListItem({
  request,
  isSelected,
  onClick,
  style,
}: RequestListItemProps) {
  const hasRules = request.matchedRules.length > 0;
  const isTls = request.tlsInfo !== null;

  return (
    <div
      style={style}
      onClick={onClick}
      className={cn(
        "flex cursor-pointer items-center gap-2 border-b px-3 text-sm transition-colors",
        "hover:bg-accent/50",
        isSelected && "bg-accent"
      )}
    >
      {/* Method */}
      <span className={cn("w-16 shrink-0 font-mono text-xs font-semibold", getMethodClass(request.method))}>
        {request.method}
      </span>

      {/* Status */}
      <span
        className={cn(
          "w-10 shrink-0 text-center font-mono text-xs",
          getStatusClass(request.responseStatus ?? undefined)
        )}
      >
        {request.responseStatus ?? "-"}
      </span>

      {/* Icons */}
      <div className="flex w-8 shrink-0 items-center gap-1">
        {isTls && <Lock className="h-3 w-3 text-emerald-500" />}
        {hasRules && <FileCode className="h-3 w-3 text-blue-500" />}
      </div>

      {/* Host + Path */}
      <div className="flex min-w-0 flex-1 items-center gap-1 overflow-hidden">
        <span className="shrink-0 text-muted-foreground">{request.host}</span>
        <span className="truncate">{request.path}</span>
      </div>

      {/* Duration */}
      <span className="w-16 shrink-0 text-right font-mono text-xs text-muted-foreground">
        {request.durationMs !== null ? formatDuration(request.durationMs) : "-"}
      </span>

      {/* Size */}
      <span className="w-16 shrink-0 text-right font-mono text-xs text-muted-foreground">
        {request.responseSize !== null ? formatBytes(request.responseSize) : "-"}
      </span>
    </div>
  );
});
