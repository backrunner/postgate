import { useState, type KeyboardEvent, type ReactNode } from "react";
import { AlertCircle, Check } from "lucide-react";
import { cn } from "@/lib/utils";
import { Input } from "@/components/ui/input";

const MIN_PORT = 1;
const MAX_PORT = 65535;

function parsePort(value: string): number | null {
  if (!/^\d+$/.test(value)) return null;
  const parsed = Number(value);
  return Number.isInteger(parsed) && parsed >= MIN_PORT && parsed <= MAX_PORT
    ? parsed
    : null;
}

export function PortInput({
  value,
  onChange,
  disabled,
  className,
}: {
  value: number;
  onChange: (value: number) => void;
  disabled?: boolean;
  className?: string;
}) {
  const [draft, setDraft] = useState<string | null>(null);
  const displayedValue = draft ?? String(value);
  const valid = parsePort(displayedValue) !== null;

  const commit = () => {
    const next = parsePort(displayedValue);
    if (next === null) {
      setDraft(null);
      return;
    }
    onChange(next);
    setDraft(null);
  };

  const handleKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Enter") {
      event.currentTarget.blur();
    } else if (event.key === "Escape") {
      setDraft(null);
      event.currentTarget.blur();
    }
  };

  return (
    <Input
      type="number"
      inputMode="numeric"
      value={displayedValue}
      onChange={(event) => setDraft(event.target.value)}
      onBlur={commit}
      onKeyDown={handleKeyDown}
      onWheel={(event) => event.currentTarget.blur()}
      min={MIN_PORT}
      max={MAX_PORT}
      disabled={disabled}
      aria-invalid={!valid}
      className={cn("w-24 text-right font-mono h-8 text-sm", className)}
    />
  );
}

export function Section({
  title,
  children,
}: {
  title: string;
  children: ReactNode;
}) {
  return (
    <section className="rounded-lg border bg-card">
      <div className="px-4 py-3 border-b">
        <h2 className="text-sm font-medium">{title}</h2>
      </div>
      <div className="p-4">{children}</div>
    </section>
  );
}

export function SettingRow({
  icon,
  label,
  description,
  children,
}: {
  icon?: ReactNode;
  label: string;
  description?: string;
  children: ReactNode;
}) {
  return (
    <div className="flex items-center justify-between gap-4">
      <div className="flex items-center gap-3 min-w-0">
        {icon && <span className="text-muted-foreground shrink-0">{icon}</span>}
        <div className="min-w-0">
          <p className="text-sm font-medium">{label}</p>
          {description && (
            <p className="text-xs text-muted-foreground truncate">{description}</p>
          )}
        </div>
      </div>
      <div className="shrink-0">{children}</div>
    </div>
  );
}

export function InfoItem({
  label,
  value,
  mono,
}: {
  label: string;
  value: string;
  mono?: boolean;
}) {
  return (
    <div className="space-y-1">
      <p className="text-xs text-muted-foreground">{label}</p>
      <p className={cn("text-sm", mono && "font-mono")}>{value}</p>
    </div>
  );
}

export function ProfileChip({ label, active }: { label: string; active: boolean }) {
  return (
    <div className="flex h-8 items-center gap-2 rounded-md border bg-muted/20 px-2.5 text-xs">
      <span
        className={cn(
          "h-1.5 w-1.5 rounded-full",
          active ? "bg-emerald-500" : "bg-muted-foreground/40",
        )}
      />
      <span className="truncate text-muted-foreground">{label}</span>
    </div>
  );
}

export function StatusLine({
  status,
  error,
}: {
  status: string | null;
  error: string | null;
}) {
  if (!status && !error) return null;

  return (
    <div
      className={cn(
        "flex items-start gap-2 rounded-md border px-3 py-2 text-xs",
        error
          ? "border-destructive/20 bg-destructive/10 text-destructive"
          : "border-emerald-500/20 bg-emerald-500/10 text-emerald-700 dark:text-emerald-300",
      )}
    >
      {error ? (
        <AlertCircle className="mt-0.5 h-3.5 w-3.5 shrink-0" />
      ) : (
        <Check className="mt-0.5 h-3.5 w-3.5 shrink-0" />
      )}
      <span className="break-words">{error ?? status}</span>
    </div>
  );
}

export function formatTimestamp(timestamp: number) {
  return new Date(timestamp).toLocaleString();
}
