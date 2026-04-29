import { Play, Square, Moon, Sun, Monitor, Activity, AlertCircle } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Badge } from "@/components/ui/badge";
import { useProxyStore } from "@/stores/proxy";
import { useThemeStore } from "@/stores/theme";
import { useProxy } from "@/hooks/useProxy";
import { cn } from "@/lib/utils";

export function Header() {
  // Narrow selectors: without them, `useProxyStore()` / `useThemeStore()`
  // subscribe this component to EVERY field in those stores. Any change
  // (the frontend store is small, so less impactful than capture/stream,
  // but the principle is the same) would re-render the whole header.
  const status = useProxyStore((state) => state.status);
  const config = useProxyStore((state) => state.config);
  const proxyError = useProxyStore((state) => state.error);
  const theme = useThemeStore((state) => state.theme);
  const setTheme = useThemeStore((state) => state.setTheme);
  const { startProxy, stopProxy } = useProxy();

  const handleToggleProxy = async () => {
    if (status === "running") {
      await stopProxy().catch(() => {});
    } else if (status === "stopped" || status === "error") {
      await startProxy().catch(() => {});
    }
  };

  const getStatusBadge = () => {
    switch (status) {
      case "running":
        return (
          <Badge variant="outline" className="border-green-500/50 text-green-500 bg-green-500/10 gap-1.5 px-2">
            <span className="relative flex h-2 w-2">
              <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-green-400 opacity-75"></span>
              <span className="relative inline-flex rounded-full h-2 w-2 bg-green-500"></span>
            </span>
            Running
          </Badge>
        );
      case "starting":
        return <Badge variant="outline" className="border-blue-500 text-blue-500 bg-blue-500/10">Starting...</Badge>;
      case "stopping":
        return <Badge variant="outline" className="border-yellow-500 text-yellow-500 bg-yellow-500/10">Stopping...</Badge>;
      case "error":
        return (
          <Badge variant="destructive" className="gap-1" title={proxyError || "Unknown error"}>
            <AlertCircle className="h-3 w-3" />
            Error
          </Badge>
        );
      default:
        return <Badge variant="secondary" className="text-muted-foreground">Stopped</Badge>;
    }
  };

  return (
    <header className="flex h-12 shrink-0 items-center justify-between border-b px-4">
      <div className="flex items-center gap-4">
        {/* Proxy Control */}
        <div className="flex items-center gap-3">
          <Button
            variant={status === "running" ? "destructive" : "default"}
            size="sm"
            onClick={handleToggleProxy}
            disabled={status === "starting" || status === "stopping"}
            className={cn(
              "h-7 px-3 text-xs font-medium shadow-sm transition-all",
              status === "running" ? "hover:bg-destructive/90" : "hover:bg-primary/90"
            )}
          >
            {status === "running" ? (
              <>
                <Square className="mr-1.5 h-3.5 w-3.5 fill-current" />
                Stop Proxy
              </>
            ) : (
              <>
                <Play className="mr-1.5 h-3.5 w-3.5 fill-current" />
                Start Proxy
              </>
            )}
          </Button>
          
          {getStatusBadge()}
          
          <div className="h-4 w-px bg-border mx-1" />
          
          {/* Port Display */}
          <div className="flex items-center gap-1.5 text-xs text-muted-foreground">
            <Activity className="h-3.5 w-3.5" />
            <span>Port:</span>
            <code className="relative rounded bg-muted px-[0.3rem] py-[0.2rem] font-mono font-semibold text-foreground">
              {config.port}
            </code>
          </div>
        </div>
      </div>

      <div className="flex items-center gap-2">
        {/* Theme Toggle */}
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button variant="ghost" size="icon" className="h-8 w-8 text-muted-foreground">
              <Sun className="h-4 w-4 rotate-0 scale-100 transition-all dark:-rotate-90 dark:scale-0" />
              <Moon className="absolute h-4 w-4 rotate-90 scale-0 transition-all dark:rotate-0 dark:scale-100" />
              <span className="sr-only">Toggle theme</span>
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            <DropdownMenuItem
              onClick={() => setTheme("light")}
              className={cn(theme === "light" && "bg-accent")}
            >
              <Sun className="mr-2 h-4 w-4" />
              Light
            </DropdownMenuItem>
            <DropdownMenuItem
              onClick={() => setTheme("dark")}
              className={cn(theme === "dark" && "bg-accent")}
            >
              <Moon className="mr-2 h-4 w-4" />
              Dark
            </DropdownMenuItem>
            <DropdownMenuItem
              onClick={() => setTheme("system")}
              className={cn(theme === "system" && "bg-accent")}
            >
              <Monitor className="mr-2 h-4 w-4" />
              System
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
    </header>
  );
}
