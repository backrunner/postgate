import { AlertTriangle, CheckCircle2, Info, X, XCircle } from "lucide-react";
import { Button } from "@/components/ui/button";
import { usePluginsStore, type PluginToastItem } from "@/stores/plugins";
import { cn } from "@/lib/utils";

const toastStyles = {
  info: { icon: Info, className: "border-blue-500/35 text-blue-600 dark:text-blue-400" },
  success: { icon: CheckCircle2, className: "border-emerald-500/35 text-emerald-600 dark:text-emerald-400" },
  warning: { icon: AlertTriangle, className: "border-amber-500/35 text-amber-600 dark:text-amber-400" },
  error: { icon: XCircle, className: "border-red-500/35 text-red-600 dark:text-red-400" },
} as const;

function PluginToast({ toast }: { toast: PluginToastItem }) {
  const clearToast = usePluginsStore((state) => state.clearToast);
  const style = toastStyles[toast.toast_type ?? "info"];
  const Icon = style.icon;

  return (
    <div
      role="status"
      className={cn(
        "flex w-80 items-start gap-2 rounded-md border bg-background/95 p-3 text-sm shadow-lg backdrop-blur",
        style.className,
      )}
    >
      <Icon className="mt-0.5 h-4 w-4 shrink-0" />
      <p className="min-w-0 flex-1 break-words text-foreground">{toast.message}</p>
      <Button
        type="button"
        variant="ghost"
        size="icon"
        className="-mr-1 -mt-1 h-6 w-6 shrink-0 text-muted-foreground"
        aria-label="Dismiss plugin notification"
        title="Dismiss"
        onClick={() => clearToast(toast.id)}
      >
        <X className="h-3.5 w-3.5" />
      </Button>
    </div>
  );
}

export function PluginToastViewport() {
  const toasts = usePluginsStore((state) => state.toasts);

  if (toasts.length === 0) return null;

  return (
    <div className="pointer-events-none fixed right-4 top-12 z-[100] flex flex-col gap-2">
      <div className="pointer-events-auto flex flex-col gap-2">
        {toasts.map((toast) => (
          <PluginToast key={toast.id} toast={toast} />
        ))}
      </div>
    </div>
  );
}
