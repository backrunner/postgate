import { Play, Trash2, Filter, Search, X, Download, Upload, Square, Copy, Check, AlertCircle, XCircle } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Badge } from "@/components/ui/badge";
import { Separator } from "@/components/ui/separator";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { useCaptureStore, useRequests } from "@/stores/capture";
import { useProxyStore } from "@/stores/proxy";
import { useProxy } from "@/hooks/useProxy";
import { useState, useCallback, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { exportToHar, importFromHar } from "@/lib/export";

interface NetworkAddress {
  ip: string;
  name: string;
  is_default: boolean;
}

export function Toolbar() {
  const { isPaused, clearRequests, filter, setFilter, resetFilter, addRequests } =
    useCaptureStore();
  const requests = useRequests();
  const { status, config, error: proxyError, setError } = useProxyStore();
  const { startProxy, stopProxy } = useProxy();
  const [showFilters, setShowFilters] = useState(false);
  const [isImporting, setIsImporting] = useState(false);
  const [addresses, setAddresses] = useState<NetworkAddress[]>([]);
  const [copiedIp, setCopiedIp] = useState<string | null>(null);

  const isRunning = status === "running";
  const isTransitioning = status === "starting" || status === "stopping";

  // Fetch network addresses on mount
  useEffect(() => {
    invoke<NetworkAddress[]>("get_local_ip")
      .then(setAddresses)
      .catch(() => setAddresses([{ ip: "127.0.0.1", name: "Localhost", is_default: false }]));
  }, []);

  // Find the default or first non-localhost IP for display
  const defaultAddr = addresses.find((a) => a.is_default) || addresses.find((a) => a.ip !== "127.0.0.1") || addresses[0];
  const displayIp = defaultAddr?.ip || "127.0.0.1";

  const handleToggleProxy = async () => {
    if (isRunning) {
      await stopProxy().catch(() => {});
    } else if (status === "stopped" || status === "error") {
      await startProxy().catch(() => {});
    }
  };

  const handleCopy = useCallback(async (ip: string) => {
    const url = `${ip}:${config.port}`;
    try {
      await navigator.clipboard.writeText(url);
      setCopiedIp(ip);
      setTimeout(() => setCopiedIp(null), 1500);
    } catch {
      // fallback
    }
  }, [config.port]);

  const handleImport = useCallback(async () => {
    setIsImporting(true);
    try {
      const importedRequests = await importFromHar();
      if (importedRequests.length > 0) {
        addRequests(importedRequests);
      }
    } catch (error) {
      console.error("Failed to import HAR:", error);
    } finally {
      setIsImporting(false);
    }
  }, [addRequests]);

  const hasActiveFilters =
    filter.search ||
    filter.methods.length > 0 ||
    filter.statusCodes.length > 0 ||
    filter.contentTypes.length > 0 ||
    filter.hosts.length > 0 ||
    filter.hasRules !== null ||
    filter.protocols.length > 0;

  return (
    <div className="flex flex-col border-b bg-muted/10">
      {/* Proxy Error Banner */}
      {status === "error" && proxyError && (
        <div className="flex items-center gap-2 px-3 py-2 bg-destructive/10 border-b border-destructive/20 text-destructive animate-in slide-in-from-top-1 duration-200">
          <AlertCircle className="h-3.5 w-3.5 shrink-0" />
          <span className="text-xs font-medium flex-1 truncate">{proxyError}</span>
          <Button
            variant="ghost"
            size="icon"
            className="h-5 w-5 shrink-0 text-destructive hover:text-destructive hover:bg-destructive/20"
            onClick={() => setError(null)}
          >
            <XCircle className="h-3.5 w-3.5" />
          </Button>
        </div>
      )}
      <div className="flex h-10 items-center gap-2 px-3">
        {/* Proxy Start/Stop */}
        <Button
          variant="outline"
          size="sm"
          onClick={handleToggleProxy}
          disabled={isTransitioning}
          className="h-7 px-3 text-xs font-medium transition-all"
        >
          {isRunning ? (
            <>
              <Square className="mr-1.5 h-3 w-3" />
              Pause
            </>
          ) : (
            <>
              <Play className="mr-1.5 h-3 w-3" />
              Start
            </>
          )}
        </Button>

        {/* Proxy Address with Popover */}
        <Popover>
          <PopoverTrigger asChild>
            <button
              className="flex items-center gap-1 rounded-md border px-2 py-1 text-xs font-mono text-muted-foreground hover:text-foreground hover:bg-muted/50 transition-colors cursor-pointer select-none"
            >
              {displayIp}:{config.port}
            </button>
          </PopoverTrigger>
          <PopoverContent align="start" className="w-auto min-w-[260px] p-2">
            <div className="space-y-1">
              <div className="px-2 py-1.5 text-[10px] font-semibold text-muted-foreground uppercase tracking-wider">
                Proxy Addresses
              </div>
              {addresses.map((addr) => (
                <button
                  key={addr.ip}
                  onClick={() => handleCopy(addr.ip)}
                  className="flex items-center justify-between w-full rounded-md px-2 py-1.5 text-xs hover:bg-muted/80 transition-colors group"
                >
                  <div className="flex items-center gap-2 min-w-0">
                    <code className="font-mono font-medium text-foreground">
                      {addr.ip}:{config.port}
                    </code>
                    {addr.is_default && (
                      <Badge variant="secondary" className="h-4 px-1 text-[9px] leading-none shrink-0">
                        default
                      </Badge>
                    )}
                    <span className="text-[10px] text-muted-foreground truncate">
                      {addr.name}
                    </span>
                  </div>
                  <div className="ml-2 shrink-0">
                    {copiedIp === addr.ip ? (
                      <Check className="h-3 w-3 text-green-500" />
                    ) : (
                      <Copy className="h-3 w-3 opacity-0 group-hover:opacity-50 transition-opacity" />
                    )}
                  </div>
                </button>
              ))}
            </div>
          </PopoverContent>
        </Popover>

        <Separator orientation="vertical" className="h-5 mx-1" />

        {/* Clear */}
        <Button
          variant="ghost"
          size="sm"
          onClick={clearRequests}
          title="Clear all requests"
          className="h-7 px-2 text-muted-foreground hover:text-destructive"
        >
          <Trash2 className="h-3.5 w-3.5" />
        </Button>

        <Separator orientation="vertical" className="h-5 mx-1" />

        {/* Search */}
        <div className="relative flex-1 max-w-md group">
          <Search className="absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground group-focus-within:text-foreground transition-colors" />
          <Input
            placeholder="Filter by URL, host, path..."
            value={filter.search}
            onChange={(e) => setFilter({ search: e.target.value })}
            className="h-7 pl-8 text-xs bg-background/50 border-muted-foreground/20 focus-visible:ring-primary/20 transition-all focus-visible:bg-background"
          />
          {filter.search && (
            <Button
              variant="ghost"
              size="icon"
              className="absolute right-1 top-1/2 h-5 w-5 -translate-y-1/2 hover:bg-transparent"
              onClick={() => setFilter({ search: "" })}
            >
              <X className="h-3 w-3 text-muted-foreground hover:text-foreground" />
            </Button>
          )}
        </div>

        {/* Filter Toggle */}
        <Button
          variant={showFilters ? "secondary" : "outline"}
          size="sm"
          onClick={() => setShowFilters(!showFilters)}
          className="h-7 gap-1.5 text-xs"
        >
          <Filter className="h-3.5 w-3.5" />
          Filters
          {hasActiveFilters && (
            <Badge variant="secondary" className="ml-0.5 h-4 min-w-4 px-1 rounded-sm text-[9px] leading-none justify-center">
              {
                [
                  filter.methods.length,
                  filter.statusCodes.length,
                  filter.contentTypes.length,
                  filter.hosts.length,
                  filter.protocols.length,
                  filter.hasRules !== null ? 1 : 0,
                ].reduce((a, b) => a + b, 0)
              }
            </Badge>
          )}
        </Button>

        {hasActiveFilters && (
          <Button
            variant="ghost"
            size="sm"
            onClick={resetFilter}
            className="h-7 px-2 text-xs text-muted-foreground hover:text-foreground"
          >
            Clear
          </Button>
        )}

        <div className="flex-1" />

        {/* Import */}
        <Button
          variant="ghost"
          size="sm"
          className="h-7 gap-1.5 text-xs text-muted-foreground"
          onClick={handleImport}
          disabled={isImporting}
        >
          <Upload className="h-3.5 w-3.5" />
          {isImporting ? "Importing..." : "Import"}
        </Button>

        {/* Export */}
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button variant="ghost" size="sm" className="h-7 gap-1.5 text-xs text-muted-foreground" disabled={requests.length === 0}>
              <Download className="h-3.5 w-3.5" />
              Export
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            <DropdownMenuItem onClick={() => exportToHar(requests)} className="text-xs">
              Export all as HAR ({requests.length})
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>

        <Separator orientation="vertical" className="h-5 mx-1" />

        {/* Request count */}
        <div className="flex items-center gap-1.5 text-xs text-muted-foreground font-mono">
          <span className="font-semibold text-foreground">{requests.length.toLocaleString()}</span> reqs
          {isPaused && <span className="text-amber-500 font-medium px-1 py-0.5 bg-amber-500/10 rounded text-[10px]">PAUSED</span>}
        </div>
      </div>

      {/* Filter Panel */}
      {showFilters && (
        <div className="flex flex-wrap items-center gap-4 border-t px-4 py-3 bg-muted/20 animate-in slide-in-from-top-1 duration-200">
          {/* Method filters */}
          <div className="flex items-center gap-2">
            <span className="text-[10px] font-semibold text-muted-foreground uppercase tracking-wider">Method</span>
            <div className="flex gap-1">
              {["GET", "POST", "PUT", "PATCH", "DELETE"].map((method) => (
                <Button
                  key={method}
                  variant={filter.methods.includes(method) ? "secondary" : "ghost"}
                  size="sm"
                  className="h-6 px-2 text-[10px] font-mono border border-transparent data-[state=active]:border-border hover:bg-muted/50"
                  onClick={() => {
                    const methods = filter.methods.includes(method)
                      ? filter.methods.filter((m) => m !== method)
                      : [...filter.methods, method];
                    setFilter({ methods });
                  }}
                >
                  {method}
                </Button>
              ))}
            </div>
          </div>

          <Separator orientation="vertical" className="h-6" />

          {/* Status code filters */}
          <div className="flex items-center gap-2">
            <span className="text-[10px] font-semibold text-muted-foreground uppercase tracking-wider">Status</span>
            <div className="flex gap-1">
              {["2xx", "3xx", "4xx", "5xx"].map((status) => (
                <Button
                  key={status}
                  variant={filter.statusCodes.includes(status) ? "secondary" : "ghost"}
                  size="sm"
                  className="h-6 px-2 text-[10px] font-mono border border-transparent data-[state=active]:border-border hover:bg-muted/50"
                  onClick={() => {
                    const statusCodes = filter.statusCodes.includes(status)
                      ? filter.statusCodes.filter((s) => s !== status)
                      : [...filter.statusCodes, status];
                    setFilter({ statusCodes });
                  }}
                >
                  {status}
                </Button>
              ))}
            </div>
          </div>

          <Separator orientation="vertical" className="h-6" />

          {/* Content type filters */}
          <div className="flex items-center gap-2">
            <span className="text-[10px] font-semibold text-muted-foreground uppercase tracking-wider">Type</span>
            <div className="flex gap-1">
              {["json", "html", "js", "css", "image"].map((type) => (
                <Button
                  key={type}
                  variant={filter.contentTypes.includes(type) ? "secondary" : "ghost"}
                  size="sm"
                  className="h-6 px-2 text-[10px] font-mono border border-transparent data-[state=active]:border-border hover:bg-muted/50"
                  onClick={() => {
                    const contentTypes = filter.contentTypes.includes(type)
                      ? filter.contentTypes.filter((t) => t !== type)
                      : [...filter.contentTypes, type];
                    setFilter({ contentTypes });
                  }}
                >
                  {type}
                </Button>
              ))}
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
