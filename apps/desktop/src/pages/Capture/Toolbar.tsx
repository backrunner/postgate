import { Pause, Play, Trash2, Filter, Search, X, Download } from "lucide-react";
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
import { useCaptureStore } from "@/stores/capture";
import { useState } from "react";
import { exportToHar } from "@/lib/export";

export function Toolbar() {
  const { isPaused, togglePause, clearRequests, filter, setFilter, resetFilter, requests } =
    useCaptureStore();
  const [showFilters, setShowFilters] = useState(false);

  const hasActiveFilters =
    filter.search ||
    filter.methods.length > 0 ||
    filter.statusCodes.length > 0 ||
    filter.contentTypes.length > 0 ||
    filter.hosts.length > 0 ||
    filter.hasRules !== null ||
    filter.protocols.length > 0;

  return (
    <div className="flex flex-col border-b">
      <div className="flex h-10 items-center gap-2 px-3">
        {/* Pause/Resume */}
        <Button variant="ghost" size="icon-sm" onClick={togglePause} title={isPaused ? "Resume" : "Pause"}>
          {isPaused ? <Play className="h-4 w-4" /> : <Pause className="h-4 w-4" />}
        </Button>

        {/* Clear */}
        <Button variant="ghost" size="icon-sm" onClick={clearRequests} title="Clear all requests">
          <Trash2 className="h-4 w-4" />
        </Button>

        <Separator orientation="vertical" className="h-6" />

        {/* Search */}
        <div className="relative flex-1 max-w-md">
          <Search className="absolute left-2 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
          <Input
            placeholder="Filter by URL, host, path..."
            value={filter.search}
            onChange={(e) => setFilter({ search: e.target.value })}
            className="h-8 pl-8 text-sm"
          />
          {filter.search && (
            <Button
              variant="ghost"
              size="icon-sm"
              className="absolute right-1 top-1/2 h-6 w-6 -translate-y-1/2"
              onClick={() => setFilter({ search: "" })}
            >
              <X className="h-3 w-3" />
            </Button>
          )}
        </div>

        {/* Filter Toggle */}
        <Button
          variant={showFilters ? "secondary" : "ghost"}
          size="sm"
          onClick={() => setShowFilters(!showFilters)}
          className="gap-1"
        >
          <Filter className="h-4 w-4" />
          Filters
          {hasActiveFilters && (
            <Badge variant="secondary" className="ml-1 h-5 px-1.5">
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
          <Button variant="ghost" size="sm" onClick={resetFilter} className="text-muted-foreground">
            Clear filters
          </Button>
        )}

        <div className="flex-1" />

        {/* Export */}
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button variant="ghost" size="sm" className="gap-1" disabled={requests.length === 0}>
              <Download className="h-4 w-4" />
              Export
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            <DropdownMenuItem onClick={() => exportToHar(requests)}>
              Export all as HAR ({requests.length})
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>

        {/* Request count */}
        <span className="text-sm text-muted-foreground">
          {requests.length.toLocaleString()} requests
          {isPaused && <span className="ml-2 text-amber-500">(paused)</span>}
        </span>
      </div>

      {/* Filter Panel */}
      {showFilters && (
        <div className="flex flex-wrap items-center gap-2 border-t px-3 py-2">
          {/* Method filters */}
          <div className="flex items-center gap-1">
            <span className="text-xs text-muted-foreground mr-1">Method:</span>
            {["GET", "POST", "PUT", "PATCH", "DELETE"].map((method) => (
              <Button
                key={method}
                variant={filter.methods.includes(method) ? "secondary" : "outline"}
                size="sm"
                className="h-6 px-2 text-xs"
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

          <Separator orientation="vertical" className="h-6" />

          {/* Status code filters */}
          <div className="flex items-center gap-1">
            <span className="text-xs text-muted-foreground mr-1">Status:</span>
            {["2xx", "3xx", "4xx", "5xx"].map((status) => (
              <Button
                key={status}
                variant={filter.statusCodes.includes(status) ? "secondary" : "outline"}
                size="sm"
                className="h-6 px-2 text-xs"
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

          <Separator orientation="vertical" className="h-6" />

          {/* Content type filters */}
          <div className="flex items-center gap-1">
            <span className="text-xs text-muted-foreground mr-1">Type:</span>
            {["json", "html", "js", "css", "image"].map((type) => (
              <Button
                key={type}
                variant={filter.contentTypes.includes(type) ? "secondary" : "outline"}
                size="sm"
                className="h-6 px-2 text-xs"
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
      )}
    </div>
  );
}
