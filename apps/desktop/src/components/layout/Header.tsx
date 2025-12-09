import { Play, Square, Moon, Sun, Monitor } from "lucide-react";
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
  const { status, config } = useProxyStore();
  const { theme, setTheme } = useThemeStore();
  const { startProxy, stopProxy } = useProxy();

  const handleToggleProxy = async () => {
    if (status === "running") {
      await stopProxy();
    } else if (status === "stopped" || status === "error") {
      await startProxy();
    }
  };

  const getStatusBadge = () => {
    switch (status) {
      case "running":
        return <Badge variant="success">Running</Badge>;
      case "starting":
        return <Badge variant="info">Starting...</Badge>;
      case "stopping":
        return <Badge variant="warning">Stopping...</Badge>;
      case "error":
        return <Badge variant="destructive">Error</Badge>;
      default:
        return <Badge variant="secondary">Stopped</Badge>;
    }
  };

  return (
    <header className="flex h-12 items-center justify-between border-b bg-background px-4">
      <div className="flex items-center gap-4">
        {/* Proxy Control */}
        <div className="flex items-center gap-2">
          <Button
            variant={status === "running" ? "destructive" : "default"}
            size="sm"
            onClick={handleToggleProxy}
            disabled={status === "starting" || status === "stopping"}
          >
            {status === "running" ? (
              <>
                <Square className="h-4 w-4" />
                Stop
              </>
            ) : (
              <>
                <Play className="h-4 w-4" />
                Start
              </>
            )}
          </Button>
          {getStatusBadge()}
        </div>

        {/* Port Display */}
        <span className="text-sm text-muted-foreground">
          Port: <span className="font-mono">{config.port}</span>
        </span>
      </div>

      <div className="flex items-center gap-2">
        {/* Theme Toggle */}
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button variant="ghost" size="icon-sm">
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
