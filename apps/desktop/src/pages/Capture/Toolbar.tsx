import { Pause, Play, Trash2, Filter, Search, X, Download, Upload } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Badge } from "@/components/ui/badge";
import { Separator } from "@/components/ui/separator";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { useCaptureStore, useRequests } from "@/stores/capture";
import { useState, useCallback } from "react";
import { exportToHar, importFromHar } from "@/lib/export";

export function Toolbar() {
  const { isPaused, togglePause, clearRequests, filter, setFilter, resetFilter, addRequests } =
    useCaptureStore();
  const requests = useRequests();
  const [showFilters, setShowFilters] = useState(false);
  const [isImporting, setIsImporting] = useState(false);

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
      <div className="flex h-12 items-center gap-3 px-4">
        {/* Pause/Resume */}
        <div className="flex items-center gap-2">
          <Button 
            variant={isPaused ? "secondary" : "default"} 
            size="sm" 
            onClick={togglePause} 
            className="h-7 text-xs px-3 gap-1.5"
            title={isPaused ? "Resume capturing" : "Pause capturing"}
          >
            {isPaused ? <Play className="h-3.5 w-3.5 fill-current" /> : <Pause className="h-3.5 w-3.5 fill-current" />}
            {isPaused ? "Resume" : "Pause"}
          </Button>

          <Button 
            variant="ghost" 
            size="sm" 
            onClick={clearRequests} 
            title="Clear all requests"
            className="h-7 px-2 text-muted-foreground hover:text-destructive"
          >
            <Trash2 className="h-4 w-4" />
          </Button>
        </div>

        <Separator orientation="vertical" className="h-6 mx-1" />

        {/* Search */}
        <div className="relative flex-1 max-w-md group">
          <Search className="absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground group-focus-within:text-foreground transition-colors" />
          <Input
            placeholder="Filter by URL, host, path..."
            value={filter.search}
            onChange={(e) => setFilter({ search: e.target.value })}
            className="h-8 pl-8 text-xs bg-background/50 border-muted-foreground/20 focus-visible:ring-primary/20 transition-all focus-visible:bg-background"
          />
          {filter.search && (
            <Button
              variant="ghost"
              size="icon"
              className="absolute right-1 top-1/2 h-6 w-6 -translate-y-1/2 hover:bg-transparent"
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
          className="h-8 gap-1.5 text-xs bg-background/50"
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
            className="h-8 px-2 text-xs text-muted-foreground hover:text-foreground"
          >
            Clear
          </Button>
        )}

        <div className="flex-1" />

        {/* Import */}
        <Button 
          variant="ghost" 
          size="sm" 
          className="h-8 gap-1.5 text-xs text-muted-foreground"
          onClick={handleImport}
          disabled={isImporting}
        >
          <Upload className="h-4 w-4" />
          {isImporting ? "Importing..." : "Import"}
        </Button>

        {/* Export */}
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button variant="ghost" size="sm" className="h-8 gap-1.5 text-xs text-muted-foreground" disabled={requests.length === 0}>
              <Download className="h-4 w-4" />
              Export
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            <DropdownMenuItem onClick={() => exportToHar(requests)} className="text-xs">
              Export all as HAR ({requests.length})
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>

        <Separator orientation="vertical" className="h-6 mx-1" />

        {/* Request count */}
        <div className="flex items-center gap-2 text-xs text-muted-foreground font-mono">
          <span className="font-semibold text-foreground">{requests.length.toLocaleString()}</span> requests
          {isPaused && <span className="text-amber-500 font-medium px-1.5 py-0.5 bg-amber-500/10 rounded ml-1">PAUSED</span>}
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
