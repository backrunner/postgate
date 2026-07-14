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
  ExternalLink,
  Download,
  FolderInput,
  X,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { Badge } from "@/components/ui/badge";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { PageHeader } from "@/components/layout/PageHeader";
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
import { usePluginsStore, PluginInfo, type PluginPanel } from "@/stores/plugins";
import { cn } from "@/lib/utils";

const PLUGIN_DOCS_HINT_DISMISSED_KEY = "postgate:pluginDocsHintDismissed";

export function PluginsPage() {
  const {
    plugins,
    panels,
    pluginsDir,
    isLoading,
    error,
    fetchPlugins,
    discoverPlugins,
    togglePlugin,
    uninstallPlugin,
    installPluginFromNpm,
    installPluginFromPath,
    fetchPluginsDir,
    fetchPanels,
  } = usePluginsStore();

  const [actionLoading, setActionLoading] = useState<string | null>(null);
  const [npmPackage, setNpmPackage] = useState("");
  const [selectedPanelKey, setSelectedPanelKey] = useState("");
  const [showDocsHint, setShowDocsHint] = useState(
    () => localStorage.getItem(PLUGIN_DOCS_HINT_DISMISSED_KEY) !== "true",
  );
  const activePanelKey = panels.some(
    (panel) => pluginPanelKey(panel) === selectedPanelKey,
  )
    ? selectedPanelKey
    : panels[0]
      ? pluginPanelKey(panels[0])
      : "";

  useEffect(() => {
    fetchPlugins();
    fetchPluginsDir();
    fetchPanels();
  }, [fetchPanels, fetchPlugins, fetchPluginsDir]);

  const handleDiscover = async () => {
    await discoverPlugins();
  };

  const handleToggle = async (plugin: PluginInfo) => {
    setActionLoading(plugin.id);
    try {
      await togglePlugin(plugin.id, !plugin.enabled);
    } catch (error) {
      console.error("Failed to toggle plugin:", error);
    } finally {
      setActionLoading(null);
    }
  };

  const handleUninstall = async (pluginId: string) => {
    setActionLoading(pluginId);
    try {
      await uninstallPlugin(pluginId);
    } catch (error) {
      console.error("Failed to uninstall plugin:", error);
    } finally {
      setActionLoading(null);
    }
  };

  const handleInstallFromNpm = async () => {
    const pkg = npmPackage.trim();
    if (!pkg) return;
    setActionLoading("npm-install");
    try {
      await installPluginFromNpm(pkg);
      setNpmPackage("");
    } catch (error) {
      console.error("Failed to install plugin from npm:", error);
    } finally {
      setActionLoading(null);
    }
  };

  const handleInstallFromPath = async () => {
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const selected = await open({
        directory: true,
        multiple: false,
        title: "Select plugin folder",
      });
      if (selected && typeof selected === "string") {
        setActionLoading("path-install");
        try {
          await installPluginFromPath(selected);
        } catch (error) {
          console.error("Failed to install plugin from path:", error);
        } finally {
          setActionLoading(null);
        }
      }
    } catch (error) {
      console.error("Failed to open dialog:", error);
    }
  };

  const openPluginsDir = () => {
    if (pluginsDir) {
      import("@tauri-apps/plugin-shell")
        .then(({ open }) => {
          open(pluginsDir);
        })
        .catch(() => {
          navigator.clipboard.writeText(pluginsDir);
        });
    }
  };

  const openPluginDocs = () => {
    void import("@tauri-apps/plugin-shell")
      .then(({ open }) => open("https://github.com/backrunner/postgate/blob/main/docs/plugins.md"))
      .catch((error) => {
        console.error("Failed to open plugin documentation:", error);
      });
  };

  const dismissDocsHint = () => {
    localStorage.setItem(PLUGIN_DOCS_HINT_DISMISSED_KEY, "true");
    setShowDocsHint(false);
  };

  return (
    <div className="flex h-full flex-col">
      {/* Unified page header */}
      <PageHeader icon={Puzzle} title="Plugins">
        <div className="flex items-center gap-2">
          <Input
            placeholder="npm package name"
            className="h-8 w-48 text-xs"
            value={npmPackage}
            onChange={(e) => setNpmPackage(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") handleInstallFromNpm();
            }}
          />
          <Button
            variant="default"
            size="sm"
            className="h-8 gap-1.5 text-xs"
            onClick={handleInstallFromNpm}
            disabled={isLoading || !npmPackage.trim() || actionLoading === "npm-install"}
          >
            {actionLoading === "npm-install" ? (
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
            ) : (
              <Download className="h-3.5 w-3.5" />
            )}
            Install
          </Button>
          <Button
            variant="outline"
            size="sm"
            className="h-8 gap-1.5 text-xs"
            onClick={handleInstallFromPath}
            disabled={isLoading || actionLoading === "path-install"}
          >
            {actionLoading === "path-install" ? (
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
            ) : (
              <FolderInput className="h-3.5 w-3.5" />
            )}
            Local
          </Button>
        </div>
        <Button
          variant="outline"
          size="sm"
          className="h-8 gap-1.5 text-xs"
          onClick={handleDiscover}
          disabled={isLoading}
        >
          {isLoading ? (
            <Loader2 className="h-3.5 w-3.5 animate-spin" />
          ) : (
            <RefreshCw className="h-3.5 w-3.5" />
          )}
          Scan
        </Button>
        <Button
          variant="outline"
          size="sm"
          className="h-8 gap-1.5 text-xs"
          onClick={openPluginsDir}
          title={pluginsDir || "Plugins directory"}
        >
          <FolderOpen className="h-3.5 w-3.5" />
          Open Folder
        </Button>
      </PageHeader>

      {/* Error Banner */}
      {error && (
        <div className="mx-4 mt-4 p-3 bg-destructive/10 border border-destructive/20 rounded-lg flex items-center gap-2 text-sm">
          <AlertCircle className="h-4 w-4 text-destructive" />
          <span className="text-destructive">{error}</span>
        </div>
      )}

      {panels.length > 0 && (
        <PluginPanels
          panels={panels}
          selectedPanelKey={activePanelKey}
          onSelectPanel={setSelectedPanelKey}
        />
      )}

      {/* Content */}
      {plugins.length === 0 ? (
        <div className="flex flex-1 items-center justify-center">
          <div className="text-center text-muted-foreground">
            <Puzzle className="mx-auto h-12 w-12 mb-4 opacity-50" />
            <h3 className="font-semibold mb-1">No plugins installed</h3>
            <p className="text-sm mb-4 max-w-md">
              Install PostGate plugins to extend functionality. Plugins should
              be installed to:
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
              <Button variant="outline" className="gap-1" onClick={openPluginsDir}>
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
      {showDocsHint && (
        <div className="flex items-center justify-between gap-3 border-t px-3 py-2 text-xs text-muted-foreground">
          <p className="flex min-w-0 items-center gap-1">
            <ExternalLink className="h-3 w-3 shrink-0" />
            <span>Learn how to create plugins in the</span>
            <button
              type="button"
              className="shrink-0 text-primary hover:underline"
              onClick={openPluginDocs}
            >
              documentation
            </button>
          </p>
          <Button
            variant="ghost"
            size="icon"
            className="h-6 w-6 shrink-0"
            onClick={dismissDocsHint}
            aria-label="Dismiss plugin documentation hint"
            title="Dismiss"
          >
            <X className="h-3.5 w-3.5" />
          </Button>
        </div>
      )}
    </div>
  );
}

function pluginPanelKey(panel: PluginPanel): string {
  return `${panel.plugin_id}:${panel.id}`;
}

function PluginPanels({
  panels,
  selectedPanelKey,
  onSelectPanel,
}: {
  panels: PluginPanel[];
  selectedPanelKey: string;
  onSelectPanel: (key: string) => void;
}) {
  return (
    <section className="h-[340px] shrink-0 border-b bg-background/55">
      <Tabs
        value={selectedPanelKey}
        onValueChange={onSelectPanel}
        className="flex h-full min-h-0 flex-col"
      >
        <div className="flex h-10 shrink-0 items-center border-b px-3">
          <TabsList className="h-7 max-w-full justify-start overflow-x-auto">
            {panels.map((panel) => (
              <TabsTrigger
                key={pluginPanelKey(panel)}
                value={pluginPanelKey(panel)}
                className="h-6 max-w-52 truncate px-2 text-xs"
              >
                {panel.title}
              </TabsTrigger>
            ))}
          </TabsList>
        </div>
        {panels.map((panel) => (
          <TabsContent
            key={pluginPanelKey(panel)}
            value={pluginPanelKey(panel)}
            className="m-0 min-h-0 flex-1 data-[state=inactive]:hidden"
          >
            <PluginPanelFrame panel={panel} />
          </TabsContent>
        ))}
      </Tabs>
    </section>
  );
}

function PluginPanelFrame({ panel }: { panel: PluginPanel }) {
  const commonProps = {
    title: panel.title,
    sandbox: "allow-forms allow-modals allow-popups allow-scripts",
    referrerPolicy: "no-referrer" as const,
    className: "h-full w-full border-0 bg-background",
  };

  return panel.content.type === "html" ? (
    <iframe {...commonProps} srcDoc={panel.content.html} />
  ) : (
    <iframe {...commonProps} src={panel.content.url} />
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
    <div
      className={cn(
        "p-4 border rounded-lg transition-colors",
        plugin.enabled ? "bg-background" : "bg-muted/30"
      )}
    >
      <div className="flex items-start justify-between gap-4">
        <div className="flex items-start gap-3 min-w-0">
          <div
            className={cn(
              "p-2 rounded-lg shrink-0",
              plugin.loaded ? "bg-emerald-500/10" : "bg-muted"
            )}
          >
            <Puzzle
              className={cn(
                "h-5 w-5",
                plugin.loaded ? "text-emerald-500" : "text-muted-foreground"
              )}
            />
          </div>
          <div className="min-w-0">
            <div className="flex items-center gap-2">
              <h3 className="font-medium truncate">{plugin.name}</h3>
              <Badge variant="outline" className="text-xs shrink-0">
                v{plugin.version}
              </Badge>
              {plugin.loaded && (
                <Badge
                  variant="default"
                  className="text-xs shrink-0 bg-emerald-500"
                >
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
                  Are you sure you want to uninstall "{plugin.name}"? This will
                  remove the plugin from your plugins directory.
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
