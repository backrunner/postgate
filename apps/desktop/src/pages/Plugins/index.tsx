import { useEffect, useState } from "react";
import { 
  Puzzle, 
  RefreshCw, 
  FolderOpen, 
  Power, 
  PowerOff, 
  Trash2, 
  AlertCircle,
  CheckCircle2,
  Loader2,
  ExternalLink
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import { Badge } from "@/components/ui/badge";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger,
} from "@/components/ui/alert-dialog";
import { usePluginsStore, PluginInfo } from "@/stores/plugins";
import { cn } from "@/lib/utils";

export function PluginsPage() {
  const { 
    plugins, 
    pluginsDir, 
    isLoading, 
    error,
    fetchPlugins, 
    discoverPlugins,
    togglePlugin,
    uninstallPlugin,
    fetchPluginsDir 
  } = usePluginsStore();
  
  const [actionLoading, setActionLoading] = useState<string | null>(null);

  useEffect(() => {
    fetchPlugins();
    fetchPluginsDir();
  }, [fetchPlugins, fetchPluginsDir]);

  const handleDiscover = async () => {
    await discoverPlugins();
  };

  const handleToggle = async (plugin: PluginInfo) => {
    setActionLoading(plugin.id);
    try {
      await togglePlugin(plugin.id, !plugin.enabled);
    } catch (error) {
      console.error('Failed to toggle plugin:', error);
    } finally {
      setActionLoading(null);
    }
  };

  const handleUninstall = async (pluginId: string) => {
    setActionLoading(pluginId);
    try {
      await uninstallPlugin(pluginId);
    } catch (error) {
      console.error('Failed to uninstall plugin:', error);
    } finally {
      setActionLoading(null);
    }
  };

  const openPluginsDir = () => {
    if (pluginsDir) {
      // Use Tauri shell plugin to open directory
      import('@tauri-apps/plugin-shell').then(({ open }) => {
        open(pluginsDir);
      }).catch(() => {
        // Fallback: just show the path
        navigator.clipboard.writeText(pluginsDir);
      });
    }
  };

  return (
    <div className="flex h-full flex-col">
      {/* Toolbar */}
      <div className="flex h-10 items-center justify-between border-b px-4">
        <h2 className="text-sm font-semibold">Plugins</h2>
        <div className="flex items-center gap-2">
          <Button 
            variant="outline" 
            size="sm" 
            className="gap-1"
            onClick={handleDiscover}
            disabled={isLoading}
          >
            {isLoading ? (
              <Loader2 className="h-4 w-4 animate-spin" />
            ) : (
              <RefreshCw className="h-4 w-4" />
            )}
            Scan
          </Button>
          <Button 
            variant="outline" 
            size="sm" 
            className="gap-1"
            onClick={openPluginsDir}
            title={pluginsDir || 'Plugins directory'}
          >
            <FolderOpen className="h-4 w-4" />
            Open Folder
          </Button>
        </div>
      </div>

      {/* Error Banner */}
      {error && (
        <div className="mx-4 mt-4 p-3 bg-destructive/10 border border-destructive/20 rounded-lg flex items-center gap-2 text-sm">
          <AlertCircle className="h-4 w-4 text-destructive" />
          <span className="text-destructive">{error}</span>
        </div>
      )}

      {/* Content */}
      {plugins.length === 0 ? (
        <div className="flex flex-1 items-center justify-center">
          <div className="text-center text-muted-foreground">
            <Puzzle className="mx-auto h-12 w-12 mb-4 opacity-50" />
            <h3 className="font-semibold mb-1">No plugins installed</h3>
            <p className="text-sm mb-4 max-w-md">
              Install PostGate plugins to extend functionality.
              Plugins should be installed to:
            </p>
            {pluginsDir && (
              <code className="block bg-muted px-3 py-2 rounded text-xs mb-4 max-w-lg break-all">
                {pluginsDir}
              </code>
            )}
            <p className="text-sm mb-4 max-w-md">
              Plugins must be npm packages named{" "}
              <code className="bg-muted px-1 rounded">postgate-plugin-*</code>
            </p>
            <div className="flex justify-center gap-2">
              <Button 
                variant="outline" 
                className="gap-1"
                onClick={handleDiscover}
                disabled={isLoading}
              >
                {isLoading ? (
                  <Loader2 className="h-4 w-4 animate-spin" />
                ) : (
                  <RefreshCw className="h-4 w-4" />
                )}
                Scan for Plugins
              </Button>
              <Button 
                variant="outline" 
                className="gap-1"
                onClick={openPluginsDir}
              >
                <FolderOpen className="h-4 w-4" />
                Open Plugins Folder
              </Button>
            </div>
          </div>
        </div>
      ) : (
        <ScrollArea className="flex-1">
          <div className="p-4 space-y-3">
            {plugins.map((plugin) => (
              <PluginCard
                key={plugin.id}
                plugin={plugin}
                isLoading={actionLoading === plugin.id}
                onToggle={() => handleToggle(plugin)}
                onUninstall={() => handleUninstall(plugin.id)}
              />
            ))}
          </div>
        </ScrollArea>
      )}

      {/* Help Footer */}
      <div className="border-t p-3 text-xs text-muted-foreground">
        <p className="flex items-center gap-1">
          <ExternalLink className="h-3 w-3" />
          Learn how to create plugins in the{" "}
          <a 
            href="https://github.com/postgate/postgate/docs/plugins" 
            target="_blank" 
            rel="noopener noreferrer"
            className="text-primary hover:underline"
          >
            documentation
          </a>
        </p>
      </div>
    </div>
  );
}

interface PluginCardProps {
  plugin: PluginInfo;
  isLoading: boolean;
  onToggle: () => void;
  onUninstall: () => void;
}

function PluginCard({ plugin, isLoading, onToggle, onUninstall }: PluginCardProps) {
  return (
    <div className={cn(
      "p-4 border rounded-lg transition-colors",
      plugin.enabled ? "bg-background" : "bg-muted/30"
    )}>
      <div className="flex items-start justify-between gap-4">
        <div className="flex items-start gap-3 min-w-0">
          <div className={cn(
            "p-2 rounded-lg shrink-0",
            plugin.loaded ? "bg-emerald-500/10" : "bg-muted"
          )}>
            <Puzzle className={cn(
              "h-5 w-5",
              plugin.loaded ? "text-emerald-500" : "text-muted-foreground"
            )} />
          </div>
          <div className="min-w-0">
            <div className="flex items-center gap-2">
              <h3 className="font-medium truncate">{plugin.name}</h3>
              <Badge variant="outline" className="text-xs shrink-0">
                v{plugin.version}
              </Badge>
              {plugin.loaded && (
                <Badge variant="default" className="text-xs shrink-0 bg-emerald-500">
                  <CheckCircle2 className="h-3 w-3 mr-1" />
                  Running
                </Badge>
              )}
            </div>
            {plugin.description && (
              <p className="text-sm text-muted-foreground mt-1 line-clamp-2">
                {plugin.description}
              </p>
            )}
            {plugin.author && (
              <p className="text-xs text-muted-foreground mt-1">
                by {plugin.author}
              </p>
            )}
            <p className="text-xs text-muted-foreground mt-1 font-mono truncate">
              {plugin.path}
            </p>
          </div>
        </div>
        
        <div className="flex items-center gap-2 shrink-0">
          {/* Enable/Disable Toggle */}
          <div className="flex items-center gap-2">
            {isLoading ? (
              <Loader2 className="h-4 w-4 animate-spin" />
            ) : plugin.enabled ? (
              <Power className="h-4 w-4 text-emerald-500" />
            ) : (
              <PowerOff className="h-4 w-4 text-muted-foreground" />
            )}
            <Switch
              checked={plugin.enabled}
              onCheckedChange={onToggle}
              disabled={isLoading}
            />
          </div>
          
          {/* Uninstall Button */}
          <AlertDialog>
            <AlertDialogTrigger asChild>
              <Button
                variant="ghost"
                size="icon"
                className="h-8 w-8 text-muted-foreground hover:text-destructive"
                disabled={isLoading}
              >
                <Trash2 className="h-4 w-4" />
              </Button>
            </AlertDialogTrigger>
            <AlertDialogContent>
              <AlertDialogHeader>
                <AlertDialogTitle>Uninstall Plugin</AlertDialogTitle>
                <AlertDialogDescription>
                  Are you sure you want to uninstall "{plugin.name}"? 
                  This will remove the plugin from your plugins directory.
                </AlertDialogDescription>
              </AlertDialogHeader>
              <AlertDialogFooter>
                <AlertDialogCancel>Cancel</AlertDialogCancel>
                <AlertDialogAction
                  onClick={onUninstall}
                  className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
                >
                  Uninstall
                </AlertDialogAction>
              </AlertDialogFooter>
            </AlertDialogContent>
          </AlertDialog>
        </div>
      </div>
    </div>
  );
}
