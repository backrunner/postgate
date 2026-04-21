import { useEffect, useState } from "react";
import { 
  Settings, 
  Shield, 
  Network, 
  Palette, 
  RefreshCw, 
  Check, 
  Loader2,
  Server,
  Lock,
  Globe,
  Bug,
  FileDown,
  ChevronDown,
  Sun,
  Moon,
  Monitor
} from "lucide-react";
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
import { invoke } from "@tauri-apps/api/core";
import { downloadDir } from "@tauri-apps/api/path";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { PageHeader } from "@/components/layout/PageHeader";
import { cn } from "@/lib/utils";

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

  const [isInstallingCert, setIsInstallingCert] = useState(false);
  const [isExportingCert, setIsExportingCert] = useState(false);
  const [certInstalled, setCertInstalled] = useState(false);
  const [certExported, setCertExported] = useState<string | null>(null);
  const [certError, setCertError] = useState<string | null>(null);

  useEffect(() => {
    initUpdaterSettings();
  }, []);

  const handleInstallCertificate = async () => {
    setIsInstallingCert(true);
    setCertError(null);
    try {
      await invoke("install_ca_certificate");
      setCertInstalled(true);
    } catch (e) {
      setCertError(String(e));
    } finally {
      setIsInstallingCert(false);
    }
  };

  const handleExportCertificate = async () => {
    setIsExportingCert(true);
    setCertError(null);
    setCertExported(null);
    try {
      const downloadsPath = await downloadDir();
      const exportPath = `${downloadsPath}/postgate-ca.pem`;
      await invoke("export_ca_certificate", { path: exportPath });
      setCertExported(exportPath);
    } catch (e) {
      setCertError(String(e));
    } finally {
      setIsExportingCert(false);
    }
  };

  const formatLastChecked = () => {
    if (!lastChecked) return "Never";
    const diff = Date.now() - lastChecked;
    if (diff < 60000) return "Just now";
    if (diff < 3600000) return `${Math.floor(diff / 60000)} minutes ago`;
    if (diff < 86400000) return `${Math.floor(diff / 3600000)} hours ago`;
    return new Date(lastChecked).toLocaleDateString();
  };

  return (
    <div className="flex h-full flex-col bg-background">
      {/* Unified page header */}
      <PageHeader icon={Settings} title="Settings" />

      <ScrollArea className="flex-1">
        <div className="max-w-3xl mx-auto py-8 px-6 space-y-6">
          
          {/* Updates Section */}
          <Section title="Software Updates">
            <div className="space-y-4">
              <div className="flex items-center justify-between">
                <div className="flex items-center gap-3">
                  <div className="h-10 w-10 rounded-lg bg-muted/50 flex items-center justify-center">
                    <RefreshCw className={cn("h-5 w-5 text-muted-foreground", isChecking && "animate-spin")} />
                  </div>
                  <div>
                    <div className="flex items-center gap-2">
                      <span className="text-sm font-medium">PostGate Desktop</span>
                      <Badge variant="secondary" className="font-mono text-xs">v{currentVersion}</Badge>
                    </div>
                    <p className="text-xs text-muted-foreground">
                      Last checked: {formatLastChecked()}
                    </p>
                  </div>
                </div>
                
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => checkForUpdates()}
                  disabled={isChecking || isDownloading || isInstalling}
                >
                  {isChecking ? (
                    <>
                      <Loader2 className="h-3.5 w-3.5 mr-1.5 animate-spin" />
                      Checking
                    </>
                  ) : (
                    "Check for Updates"
                  )}
                </Button>
              </div>

              {/* Update Available */}
              {updateAvailable && updateInfo && (
                <div className="rounded-md border bg-muted/30 p-4">
                  <div className="flex items-center justify-between">
                    <div>
                      <div className="flex items-center gap-2">
                        <span className="text-sm font-medium">Update Available</span>
                        <Badge variant="outline" className="font-mono text-xs">v{updateInfo.version}</Badge>
                      </div>
                      {updateInfo.body && (
                        <p className="text-xs text-muted-foreground mt-1.5 max-w-md">
                          {updateInfo.body.slice(0, 200)}
                          {updateInfo.body.length > 200 && "..."}
                        </p>
                      )}
                    </div>
                    {!isDownloading && !isInstalling && (
                      <Button size="sm" onClick={() => downloadAndInstall()}>
                        Download & Install
                      </Button>
                    )}
                  </div>

                  {isDownloading && (
                    <div className="mt-3 space-y-1.5">
                      <div className="flex justify-between text-xs text-muted-foreground">
                        <span>Downloading...</span>
                        <span>{downloadProgress}%</span>
                      </div>
                      <Progress value={downloadProgress} className="h-1.5" />
                    </div>
                  )}

                  {isInstalling && (
                    <div className="mt-3 flex items-center gap-2 text-xs text-muted-foreground">
                      <Loader2 className="h-3 w-3 animate-spin" />
                      Installing update...
                    </div>
                  )}
                </div>
              )}

              {/* Up to date message */}
              {!updateAvailable && !isChecking && lastChecked && (
                <div className="flex items-center gap-2 text-xs text-muted-foreground">
                  <Check className="h-3.5 w-3.5" />
                  PostGate is up to date
                </div>
              )}

              {error && (
                <div className="text-xs text-destructive">{error}</div>
              )}

              <Separator />

              <SettingRow
                label="Auto Check"
                description="Automatically check for updates on startup"
              >
                <Switch checked={autoCheck} onCheckedChange={setAutoCheck} />
              </SettingRow>

              <SettingRow
                label="Auto Download"
                description="Download available updates in the background"
              >
                <Switch checked={autoDownload} onCheckedChange={setAutoDownload} />
              </SettingRow>
            </div>
          </Section>

          {/* Proxy Configuration */}
          <Section title="Proxy Configuration">
            <div className="space-y-4">
              <SettingRow
                icon={<Server className="h-4 w-4" />}
                label="Port"
                description="The local port the proxy server listens on"
              >
                <Input
                  type="number"
                  value={config.port}
                  onChange={(e) => setConfig({ port: parseInt(e.target.value) || 8899 })}
                  className="w-24 text-right font-mono h-8 text-sm"
                  min={1}
                  max={65535}
                />
              </SettingRow>

              <SettingRow
                icon={<Globe className="h-4 w-4" />}
                label="HTTP/2"
                description="Enable HTTP/2 protocol support"
              >
                <Switch
                  checked={config.enableHttp2}
                  onCheckedChange={(checked) => setConfig({ enableHttp2: checked })}
                />
              </SettingRow>

              <SettingRow
                icon={<Network className="h-4 w-4" />}
                label="QUIC / HTTP/3"
                description="Experimental support for QUIC protocol"
              >
                <Switch
                  checked={config.enableQuic}
                  onCheckedChange={(checked) => setConfig({ enableQuic: checked })}
                />
              </SettingRow>

              <SettingRow
                icon={<Bug className="h-4 w-4" />}
                label="Debug Server Port"
                description="Port for Chrome DevTools Protocol debugging"
              >
                <Input
                  type="number"
                  value={config.debugPort}
                  onChange={(e) => setConfig({ debugPort: parseInt(e.target.value) || 9229 })}
                  className="w-24 text-right font-mono h-8 text-sm"
                  min={1}
                  max={65535}
                />
              </SettingRow>
            </div>
          </Section>

          {/* HTTPS & Security */}
          <Section title="HTTPS & Security">
            <div className="space-y-4">
              <div className="flex items-start gap-3">
                <div className="h-9 w-9 rounded-lg bg-muted/50 flex items-center justify-center shrink-0 mt-0.5">
                  <Lock className="h-4 w-4 text-muted-foreground" />
                </div>
                <div className="flex-1 min-w-0">
                  <p className="text-sm font-medium">Root Certificate</p>
                  <p className="text-xs text-muted-foreground mt-0.5 leading-relaxed">
                    To decrypt and inspect HTTPS traffic, PostGate's root certificate must be trusted by your system.
                  </p>
                  
                  {certInstalled && (
                    <div className="mt-3 flex items-center gap-1.5 text-xs text-muted-foreground">
                      <Check className="h-3.5 w-3.5" />
                      Certificate installed successfully
                    </div>
                  )}
                  
                  {certExported && (
                    <div className="mt-3 flex items-center gap-1.5 text-xs text-muted-foreground">
                      <Check className="h-3.5 w-3.5" />
                      <span className="truncate">Exported to: {certExported}</span>
                    </div>
                  )}
                  
                  {certError && (
                    <div className="mt-3 text-xs text-destructive">{certError}</div>
                  )}
                  
                  <div className="flex gap-2 mt-3">
                    <Button 
                      variant="outline" 
                      size="sm"
                      className="h-8 text-xs"
                      onClick={handleExportCertificate}
                      disabled={isExportingCert}
                    >
                      {isExportingCert ? (
                        <Loader2 className="h-3.5 w-3.5 mr-1.5 animate-spin" />
                      ) : (
                        <FileDown className="h-3.5 w-3.5 mr-1.5" />
                      )}
                      Export
                    </Button>
                    <Button 
                      size="sm"
                      className="h-8 text-xs"
                      onClick={handleInstallCertificate}
                      disabled={isInstallingCert || certInstalled}
                    >
                      {isInstallingCert ? (
                        <Loader2 className="h-3.5 w-3.5 mr-1.5 animate-spin" />
                      ) : certInstalled ? (
                        <Check className="h-3.5 w-3.5 mr-1.5" />
                      ) : (
                        <Shield className="h-3.5 w-3.5 mr-1.5" />
                      )}
                      {certInstalled ? "Installed" : "Install to System"}
                    </Button>
                  </div>
                </div>
              </div>
            </div>
          </Section>

          {/* Appearance */}
          <Section title="Appearance">
            <SettingRow
              icon={<Palette className="h-4 w-4" />}
              label="Theme"
              description="Select your preferred color mode"
            >
              <DropdownMenu>
                <DropdownMenuTrigger asChild>
                  <Button variant="outline" size="sm" className="h-8 w-32 justify-between text-xs">
                    <span className="flex items-center gap-1.5">
                      {theme === "light" && <Sun className="h-3.5 w-3.5" />}
                      {theme === "dark" && <Moon className="h-3.5 w-3.5" />}
                      {theme === "system" && <Monitor className="h-3.5 w-3.5" />}
                      {theme.charAt(0).toUpperCase() + theme.slice(1)}
                    </span>
                    <ChevronDown className="h-3.5 w-3.5 opacity-50" />
                  </Button>
                </DropdownMenuTrigger>
                <DropdownMenuContent align="end" className="w-32">
                  <DropdownMenuItem onClick={() => setTheme("light")} className="text-xs">
                    <Sun className="h-3.5 w-3.5 mr-2" />
                    Light
                  </DropdownMenuItem>
                  <DropdownMenuItem onClick={() => setTheme("dark")} className="text-xs">
                    <Moon className="h-3.5 w-3.5 mr-2" />
                    Dark
                  </DropdownMenuItem>
                  <DropdownMenuItem onClick={() => setTheme("system")} className="text-xs">
                    <Monitor className="h-3.5 w-3.5 mr-2" />
                    System
                  </DropdownMenuItem>
                </DropdownMenuContent>
              </DropdownMenu>
            </SettingRow>
          </Section>

          {/* About */}
          <Section title="About">
            <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
              <InfoItem label="Version" value={currentVersion} mono />
              <InfoItem label="Framework" value="Tauri 2.0" mono />
              <InfoItem label="UI" value="React 19" mono />
              <InfoItem label="License" value="MIT" mono />
            </div>
          </Section>

          <div className="h-8" />
        </div>
      </ScrollArea>
    </div>
  );
}

function Section({ 
  title, 
  children 
}: { 
  title: string; 
  children: React.ReactNode;
}) {
  return (
    <section className="rounded-lg border bg-card">
      <div className="px-4 py-3 border-b">
        <h2 className="text-sm font-medium">{title}</h2>
      </div>
      <div className="p-4">
        {children}
      </div>
    </section>
  );
}

function SettingRow({
  icon,
  label,
  description,
  children,
}: {
  icon?: React.ReactNode;
  label: string;
  description?: string;
  children: React.ReactNode;
}) {
  return (
    <div className="flex items-center justify-between gap-4">
      <div className="flex items-center gap-3 min-w-0">
        {icon && (
          <span className="text-muted-foreground shrink-0">{icon}</span>
        )}
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

function InfoItem({ 
  label, 
  value, 
  mono 
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
