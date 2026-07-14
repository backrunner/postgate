import { forwardRef, type CSSProperties, type MouseEvent, type ReactNode } from "react";
import { cn } from "@/lib/utils";

interface WorkspaceSidebarProps {
  title: string;
  actions?: ReactNode;
  toolbar?: ReactNode;
  children: ReactNode;
  className?: string;
  style?: CSSProperties;
  onResizeStart?: (event: MouseEvent<HTMLDivElement>) => void;
}

export const WorkspaceSidebar = forwardRef<HTMLElement, WorkspaceSidebarProps>(
  function WorkspaceSidebar(
    {
      title,
      actions,
      toolbar,
      children,
      className,
      style,
      onResizeStart,
    },
    ref,
  ) {
    return (
      <aside
        ref={ref}
        className={cn(
          "relative flex min-h-0 w-60 shrink-0 flex-col border-r bg-background/72",
          className,
        )}
        style={style}
      >
        <div className="relative z-20 flex h-10 shrink-0 items-center justify-between border-b bg-background/80 px-3 backdrop-blur-sm">
          <h2 className="truncate text-xs font-semibold uppercase text-muted-foreground">
            {title}
          </h2>
          {actions && <div className="flex shrink-0 items-center gap-0.5">{actions}</div>}
        </div>

        {toolbar && (
          <div className="relative z-20 shrink-0 border-b bg-background/70 p-2 backdrop-blur-sm">
            {toolbar}
          </div>
        )}

        <div className="min-h-0 flex-1">{children}</div>

        {onResizeStart && (
          <div
            role="separator"
            aria-orientation="vertical"
            aria-label={`Resize ${title}`}
            className="absolute inset-y-0 right-0 z-30 w-1 cursor-col-resize transition-colors hover:bg-primary/20 active:bg-primary/35"
            onMouseDown={onResizeStart}
          />
        )}
      </aside>
    );
  },
);
