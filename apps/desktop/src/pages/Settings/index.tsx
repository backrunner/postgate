import { useEffect } from "react";
import { Settings, Shield, Network, Palette, Download, RefreshCw, Check, Loader2 } from "lucide-react";
import { Switch } from "@/components/ui/switch";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { Separator } from "@/components/ui/separator";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Badge } from "@/components/ui/badge";
import { Progress } from "@/components/ui/progress";
import { useProxyStore } from "@/stores/proxy";
import { useThemeStore } from "@/stores/theme";
import { useUpdaterStore, initUpdaterSettings } from "@/stores/updater";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";

export function SettingsPage() {
  const { config, setConfig } = useProxyStore();
  const { theme, setTheme } = useThemeStore();
  const {
    currentVersion,
    isChecking,
    isDownloading,
    isInstalling,
    downloadProgress,
    updateAvailable,
    updateInfo,
    error,
    lastChecked,
    autoCheck,
    autoDownload,
    checkForUpdates,
    downloadAndInstall,
    setAutoCheck,
    setAutoDownload,
  } = useUpdaterStore();

  // Initialize updater settings on mount
  useEffect(() => {
    initUpdaterSettings();
  }, []);

  const formatLastChecked = () => {
    if (!lastChecked) return "Never";
    const diff = Date.now() - lastChecked;
    if (diff < 60000) return "Just now";
    if (diff < 3600000) return `${Math.floor(diff / 60000)} minutes ago`;
    if (diff < 86400000) return `${Math.floor(diff / 3600000)} hours ago`;
    return new Date(lastChecked).toLocaleDateString();
  };

  return (
    <ScrollArea className="h-full">
      <div className="max-w-2xl mx-auto py-6 px-4 space-y-8">
        {/* Updates Section */}
        <section>
          <div className="flex items-center gap-2 mb-4">
            <Download className="h-5 w-5" />
            <h2 className="text-lg font-semibold">Updates</h2>
          </div>
          <div className="space-y-4">
            {/* Current version and check for updates */}
            <div className="rounded-lg border p-4 bg-muted/50">
              <div className="flex items-center justify-between mb-4">
                <div>
                  <div className="flex items-center gap-2">
                    <span className="font-medium">PostGate</span>
                    <Badge variant="secondary">v{currentVersion}</Badge>
                  </div>
                  <p className="text-xs text-muted-foreground mt-1">
                    Last checked: {formatLastChecked()}
                  </p>
                </div>
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => checkForUpdates()}
                  disabled={isChecking || isDownloading || isInstalling}
                >
                  {isChecking ? (
                    <>
                      <Loader2 className="h-4 w-4 mr-1 animate-spin" />
                      Checking...
                    </>
                  ) : (
                    <>
                      <RefreshCw className="h-4 w-4 mr-1" />
                      Check for Updates
                    </>
                  )}
                </Button>
              </div>

              {/* Update available */}
              {updateAvailable && updateInfo && (
                <div className="border rounded-lg p-3 bg-background">
                  <div className="flex items-start justify-between">
                    <div>
                      <div className="flex items-center gap-2">
                        <span className="font-medium">Update Available</span>
                        <Badge>v{updateInfo.version}</Badge>
                      </div>
                      {updateInfo.body && (
                        <p className="text-sm text-muted-foreground mt-1 whitespace-pre-wrap">
                          {updateInfo.body.slice(0, 200)}
                          {updateInfo.body.length > 200 && "..."}
                        </p>
                      )}
                    </div>
                  </div>

                  {/* Download progress */}
                  {isDownloading && (
                    <div className="mt-3">
                      <div className="flex items-center justify-between text-xs mb-1">
                        <span>Downloading...</span>
                        <span>{downloadProgress}%</span>
                      </div>
                      <Progress value={downloadProgress} className="h-2" />
                    </div>
                  )}

                  {isInstalling && (
                    <div className="mt-3 flex items-center gap-2 text-sm">
                      <Loader2 className="h-4 w-4 animate-spin" />
                      Installing update...
                    </div>
                  )}

                  {!isDownloading && !isInstalling && (
                    <Button
                      className="mt-3 w-full"
                      onClick={() => downloadAndInstall()}
                    >
                      <Download className="h-4 w-4 mr-1" />
                      Download and Install
                    </Button>
                  )}
                </div>
              )}

              {/* No update available */}
              {!updateAvailable && lastChecked && !isChecking && (
                <div className="flex items-center gap-2 text-sm text-green-600 dark:text-green-400">
                  <Check className="h-4 w-4" />
                  You're running the latest version
                </div>
              )}

              {/* Error */}
              {error && (
                <p className="text-sm text-red-500 mt-2">{error}</p>
              )}
            </div>

            <Separator />

            {/* Auto-update settings */}
            <div className="flex items-center justify-between">
              <div>
                <label className="font-medium">Check for updates automatically</label>
                <p className="text-sm text-muted-foreground">
                  Check for new versions when the app starts
                </p>
              </div>
              <Switch
                checked={autoCheck}
                onCheckedChange={setAutoCheck}
              />
            </div>
            <Separator />
            <div className="flex items-center justify-between">
              <div>
                <label className="font-medium">Download updates automatically</label>
                <p className="text-sm text-muted-foreground">
                  Automatically download and prompt to install updates
                </p>
              </div>
              <Switch
                checked={autoDownload}
                onCheckedChange={setAutoDownload}
              />
            </div>
          </div>
        </section>

        {/* Proxy Settings */}
        <section>
          <div className="flex items-center gap-2 mb-4">
            <Network className="h-5 w-5" />
            <h2 className="text-lg font-semibold">Proxy Settings</h2>
          </div>
          <div className="space-y-4">
            <div className="flex items-center justify-between">
              <div>
                <label className="font-medium">Port</label>
                <p className="text-sm text-muted-foreground">
                  The port the proxy server listens on
                </p>
              </div>
              <Input
                type="number"
                value={config.port}
                onChange={(e) => setConfig({ port: parseInt(e.target.value) || 8899 })}
                className="w-24 text-right"
                min={1}
                max={65535}
              />
            </div>
            <Separator />
            <div className="flex items-center justify-between">
              <div>
                <label className="font-medium">HTTP/2 Support</label>
                <p className="text-sm text-muted-foreground">
                  Enable HTTP/2 protocol support for proxied connections
                </p>
              </div>
              <Switch
                checked={config.enableHttp2}
                onCheckedChange={(checked) => setConfig({ enableHttp2: checked })}
              />
            </div>
            <Separator />
            <div className="flex items-center justify-between">
              <div>
                <label className="font-medium">QUIC/HTTP/3 Support</label>
                <p className="text-sm text-muted-foreground">
                  Enable experimental QUIC protocol support
                </p>
              </div>
              <Switch
                checked={config.enableQuic}
                onCheckedChange={(checked) => setConfig({ enableQuic: checked })}
              />
            </div>
          </div>
        </section>

        {/* Certificate Settings */}
        <section>
          <div className="flex items-center gap-2 mb-4">
            <Shield className="h-5 w-5" />
            <h2 className="text-lg font-semibold">Certificate</h2>
          </div>
          <div className="space-y-4">
            <div className="rounded-lg border p-4 bg-muted/50">
              <p className="text-sm text-muted-foreground mb-4">
                To capture HTTPS traffic, you need to install PostGate's root
                certificate as a trusted certificate authority on your system.
              </p>
              <div className="flex gap-2">
                <Button variant="outline">Export Certificate</Button>
                <Button>Install Certificate</Button>
              </div>
            </div>
          </div>
        </section>

        {/* Appearance */}
        <section>
          <div className="flex items-center gap-2 mb-4">
            <Palette className="h-5 w-5" />
            <h2 className="text-lg font-semibold">Appearance</h2>
          </div>
          <div className="space-y-4">
            <div className="flex items-center justify-between">
              <div>
                <label className="font-medium">Theme</label>
                <p className="text-sm text-muted-foreground">
                  Choose your preferred color scheme
                </p>
              </div>
              <DropdownMenu>
                <DropdownMenuTrigger asChild>
                  <Button variant="outline" className="w-32 justify-between">
                    {theme === "light" && "Light"}
                    {theme === "dark" && "Dark"}
                    {theme === "system" && "System"}
                  </Button>
                </DropdownMenuTrigger>
                <DropdownMenuContent align="end">
                  <DropdownMenuItem onClick={() => setTheme("light")}>
                    Light
                  </DropdownMenuItem>
                  <DropdownMenuItem onClick={() => setTheme("dark")}>
                    Dark
                  </DropdownMenuItem>
                  <DropdownMenuItem onClick={() => setTheme("system")}>
                    System
                  </DropdownMenuItem>
                </DropdownMenuContent>
              </DropdownMenu>
            </div>
          </div>
        </section>

        {/* About */}
        <section>
          <div className="flex items-center gap-2 mb-4">
            <Settings className="h-5 w-5" />
            <h2 className="text-lg font-semibold">About</h2>
          </div>
          <div className="rounded-lg border p-4 bg-muted/50">
            <div className="space-y-2 text-sm">
              <div className="flex justify-between">
                <span className="text-muted-foreground">Version</span>
                <span>{currentVersion}</span>
              </div>
              <div className="flex justify-between">
                <span className="text-muted-foreground">Tauri</span>
                <span>2.0</span>
              </div>
              <div className="flex justify-between">
                <span className="text-muted-foreground">React</span>
                <span>19.0</span>
              </div>
            </div>
          </div>
        </section>
      </div>
    </ScrollArea>
  );
}
