import type { ReactNode } from "react";
import type { LucideIcon } from "lucide-react";
import { cn } from "@/lib/utils";

interface PanelEmptyStateProps {
  icon: LucideIcon;
  title: ReactNode;
  description?: ReactNode;
  action?: ReactNode;
  children?: ReactNode;
  compact?: boolean;
  className?: string;
}

export function PanelEmptyState({
  icon: Icon,
  title,
  description,
  action,
  children,
  compact = false,
  className,
}: PanelEmptyStateProps) {
  return (
    <div
      className={cn(
        "h-full min-h-0 w-full text-center text-muted-foreground",
        compact ? "px-4 py-6" : "p-8",
        className,
      )}
    >
      <div className="grid h-full min-h-0 w-full grid-rows-[minmax(0,1fr)_auto_minmax(0,1fr)] justify-items-center">
        <div
          className={cn(
            "flex items-end justify-center",
            compact ? "pb-3" : "pb-5",
          )}
        >
          <div
            className={cn(
              "flex shrink-0 items-center justify-center rounded-lg bg-muted/40 text-muted-foreground",
              compact ? "h-9 w-9" : "h-14 w-14",
            )}
          >
            <Icon
              className={compact ? "h-4 w-4 opacity-60" : "h-7 w-7 opacity-50"}
              strokeWidth={1.75}
            />
          </div>
        </div>

        <h3
          className={cn(
            "font-semibold text-foreground",
            compact ? "text-xs" : "text-lg",
          )}
        >
          {title}
        </h3>

        <div className={cn("flex min-h-0 flex-col items-center", compact ? "pt-1" : "pt-2")}>
          {description && (
            <div
              className={cn(
                "leading-relaxed",
                compact ? "max-w-44 text-[11px]" : "max-w-sm text-sm",
              )}
            >
              {description}
            </div>
          )}

          {action && <div className={compact ? "mt-4" : "mt-6"}>{action}</div>}
          {children}
        </div>
      </div>
    </div>
  );
}
