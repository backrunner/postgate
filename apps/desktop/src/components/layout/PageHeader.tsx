import type { LucideIcon } from "lucide-react";
import { Separator } from "@/components/ui/separator";
import { cn } from "@/lib/utils";

interface PageHeaderProps {
  icon: LucideIcon;
  title: string;
  /** Inline extras rendered next to the title (separated by a vertical divider). */
  subtitle?: React.ReactNode;
  /** Right-aligned action slot (buttons, etc). */
  children?: React.ReactNode;
  /** Optional extra classes for the outer container. */
  className?: string;
}

/**
 * Unified page header used across all top-level pages.
 *
 * Layout:
 *   ┌─ h-12 border-b px-4 ─────────────────────┐
 *   │  [icon]  Title   │ subtitle    [actions] │
 *   └──────────────────────────────────────────┘
 *
 * All pages share the same height, typography and spacing — keep this the
 * single source of truth. If the design system changes, change it here.
 */
export function PageHeader({
  icon: Icon,
  title,
  subtitle,
  children,
  className,
}: PageHeaderProps) {
  return (
    <div
      className={cn(
        "flex h-12 items-center justify-between border-b px-4 shrink-0",
        className,
      )}
    >
      <div className="flex items-center gap-2 min-w-0">
        <Icon className="h-4 w-4 text-muted-foreground shrink-0" />
        <h1 className="text-sm font-semibold">{title}</h1>
        {subtitle !== undefined && subtitle !== null && subtitle !== false && (
          <>
            <Separator orientation="vertical" className="h-6 mx-1" />
            <div className="flex items-center gap-2 min-w-0 text-sm">
              {subtitle}
            </div>
          </>
        )}
      </div>

      {children && (
        <div className="flex items-center gap-1 shrink-0">
          {children}
        </div>
      )}
    </div>
  );
}
