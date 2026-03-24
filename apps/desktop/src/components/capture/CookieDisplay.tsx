import { useState, useMemo } from "react";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Cookie, List, FileText, Copy, Check } from "lucide-react";
import { cn } from "@/lib/utils";

interface CookieDisplayProps {
  cookies: string;
  type: "cookie" | "set-cookie";
  className?: string;
}

interface ParsedCookie {
  name: string;
  value: string;
  attributes?: Record<string, string | true>;
}

function parseCookieHeader(cookieHeader: string): ParsedCookie[] {
  if (!cookieHeader) return [];

  return cookieHeader
    .split(";")
    .map((cookie) => {
      const [name, ...valueParts] = cookie.trim().split("=");
      return {
        name: name?.trim() || "",
        value: valueParts.join("=").trim(),
      };
    })
    .filter((c) => c.name);
}

function parseSetCookieHeader(setCookieHeader: string): ParsedCookie[] {
  if (!setCookieHeader) return [];

  // Set-Cookie values could be multiple joined by comma (though usually one per header)
  return setCookieHeader.split(/,(?=[^;]*=)/).map((cookie) => {
    const parts = cookie.split(";").map((p) => p.trim());
    const [nameValue, ...attrParts] = parts;
    const [name, ...valueParts] = nameValue.split("=");

    const attributes: Record<string, string | true> = {};
    for (const attr of attrParts) {
      const [aName, ...aValueParts] = attr.split("=");
      const aKey = aName.trim().toLowerCase();
      if (aKey) {
        attributes[aKey] = aValueParts.length > 0 ? aValueParts.join("=").trim() : true;
      }
    }

    return {
      name: name?.trim() || "",
      value: valueParts.join("=").trim(),
      attributes: Object.keys(attributes).length > 0 ? attributes : undefined,
    };
  }).filter((c) => c.name);
}

const ATTRIBUTE_COLORS: Record<string, string> = {
  "httponly": "text-rose-600 dark:text-rose-400",
  "secure": "text-emerald-600 dark:text-emerald-400",
  "samesite": "text-sky-600 dark:text-sky-400",
  "path": "text-purple-600 dark:text-purple-400",
  "domain": "text-indigo-600 dark:text-indigo-400",
  "expires": "text-amber-600 dark:text-amber-400",
  "max-age": "text-amber-600 dark:text-amber-400",
};

export function CookieDisplay({ cookies, type, className }: CookieDisplayProps) {
  const [viewMode, setViewMode] = useState<"parsed" | "raw">("parsed");
  const [copiedIdx, setCopiedIdx] = useState<number | null>(null);

  const parsedCookies = useMemo(() => {
    return type === "cookie"
      ? parseCookieHeader(cookies)
      : parseSetCookieHeader(cookies);
  }, [cookies, type]);

  const handleCopyValue = (value: string, idx: number) => {
    navigator.clipboard.writeText(value);
    setCopiedIdx(idx);
    setTimeout(() => setCopiedIdx(null), 1500);
  };

  if (!cookies) return null;

  return (
    <div className={cn("rounded border bg-muted/30 overflow-hidden", className)}>
      {/* Toolbar */}
      <div className="flex items-center justify-between px-2 py-1 border-b bg-muted/50">
        <div className="flex items-center gap-1.5">
          <Cookie className="h-3 w-3 text-muted-foreground" />
          <span className="text-xs font-medium text-muted-foreground">
            {type === "cookie" ? "Cookie" : "Set-Cookie"}
          </span>
          <Badge variant="secondary" className="text-[10px] py-0 h-4 px-1.5">
            {parsedCookies.length}
          </Badge>
        </div>
        <div className="flex items-center gap-0.5 bg-background rounded-md border p-0.5">
          <Button
            variant={viewMode === "parsed" ? "secondary" : "ghost"}
            size="icon-sm"
            className="h-5 w-5"
            onClick={() => setViewMode("parsed")}
            title="Parsed view"
          >
            <List className="h-3 w-3" />
          </Button>
          <Button
            variant={viewMode === "raw" ? "secondary" : "ghost"}
            size="icon-sm"
            className="h-5 w-5"
            onClick={() => setViewMode("raw")}
            title="Raw view"
          >
            <FileText className="h-3 w-3" />
          </Button>
        </div>
      </div>

      {/* Content */}
      {viewMode === "raw" ? (
        <div className="p-2">
          <pre className="text-xs font-mono break-all whitespace-pre-wrap text-rose-600 dark:text-rose-400">
            {cookies}
          </pre>
        </div>
      ) : (
        <div className="divide-y divide-border/30">
          {parsedCookies.map((cookie, idx) => (
            <div
              key={`${cookie.name}-${idx}`}
              className="group flex items-start gap-2 px-2 py-1.5 text-xs font-mono hover:bg-muted/50 transition-colors"
            >
              <span className="font-semibold text-indigo-600 dark:text-indigo-400 shrink-0">
                {cookie.name}
              </span>
              <span className="text-muted-foreground shrink-0">=</span>
              <span className="break-all text-foreground flex-1 min-w-0">
                {cookie.value || <span className="text-muted-foreground italic">(empty)</span>}
              </span>

              {/* Copy button */}
              <button
                onClick={() => handleCopyValue(`${cookie.name}=${cookie.value}`, idx)}
                className="shrink-0 opacity-0 group-hover:opacity-100 transition-opacity p-0.5 rounded hover:bg-muted"
                title="Copy cookie"
              >
                {copiedIdx === idx ? (
                  <Check className="h-3 w-3 text-emerald-500" />
                ) : (
                  <Copy className="h-3 w-3 text-muted-foreground" />
                )}
              </button>

              {/* Set-Cookie attributes */}
              {cookie.attributes && (
                <div className="shrink-0 flex items-center gap-1 flex-wrap">
                  {Object.entries(cookie.attributes).map(([key, val]) => (
                    <Badge
                      key={key}
                      variant="outline"
                      className={cn(
                        "text-[9px] py-0 h-3.5 px-1 font-mono",
                        ATTRIBUTE_COLORS[key] || "text-muted-foreground"
                      )}
                    >
                      {val === true ? key : `${key}=${val}`}
                    </Badge>
                  ))}
                </div>
              )}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
