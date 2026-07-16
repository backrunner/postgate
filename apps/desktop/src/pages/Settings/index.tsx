import { useCallback, useEffect, useState } from "react";
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
  Cloud,
  Bug,
  FileDown,
  FileUp,
  HardDriveUpload,
  ChevronDown,
  Sun,
  Moon,
  Monitor,
  Clock3
} from "lucide-react";
import { Switch } from "@/components/ui/switch";
import { Button } from "@/components/ui/button";
import { Separator } from "@/components/ui/separator";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Badge } from "@/components/ui/badge";
import { Progress } from "@/components/ui/progress";
import { useProxyStore } from "@/stores/proxy";
import { useThemeStore } from "@/stores/theme";
import { useUpdaterStore, type UpdateChannel } from "@/stores/updater";
import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { PageHeader } from "@/components/layout/PageHeader";
import { cn } from "@/lib/utils";
import { useRulesStore } from "@/stores/rules";
import { useValuesStore } from "@/stores/values";
import { useReplayStore } from "@/stores/replay";
import { useColumnsStore, type ColumnConfig } from "@/stores/columns";
import { McpAccessSection } from "./McpAccessSection";
import {
  formatTimestamp,
  InfoItem,
  ProfileChip,
  PortInput,
  Section,
  SettingRow,
  StatusLine,
} from "./components";

type SyncProvider = "cloudkit" | "icloud" | "webdav";

interface ProfileOptions {
  includeRules: boolean;
  includeValues: boolean;
  includeReplay: boolean;
  includeCertificate: boolean;
  includeAppSettings: boolean;
  includeSyncSettings: boolean;
}

interface ProfileSummary {
  exportedAt: number;
  ruleGroups: number;
  values: number;
  collections: number;
  savedRequests: number;
  includesCertificate: boolean;
  includesAppSettings: boolean;
  includesSyncSettings: boolean;
}

interface ProxySettingsBackup {
  port: number;
  enableHttp2: boolean;
  enableQuic: boolean;
  quicPort: number | null;
  debugPort: number;
}

interface AppSettingsBackup {
  proxy?: ProxySettingsBackup;
  theme?: "light" | "dark" | "system";
  columns?: ColumnConfig[] | { state?: { columns?: ColumnConfig[] } };
  updates?: {
    autoCheck: boolean;
    autoDownload: boolean;
    channel?: UpdateChannel;
  };
}

interface SyncSettings {
  enabled: boolean;
  provider: SyncProvider;
  remotePath?: string | null;
  webdav?: {
    endpoint: string;
    username: string;
    password: string;
  } | null;
  cloudkitChangeTag?: string | null;
  lastSyncedAt?: number | null;
}

interface ImportResult {
  summary: ProfileSummary;
  appSettings?: AppSettingsBackup | null;
  syncSettings?: SyncSettings | null;
}

interface WhistleImportResult {
  groups: Array<{ name: string }>;
  ruleCount: number;
}

interface SyncStatus {
  config: SyncSettings;
  localPath: string;
  remoteAvailable: boolean;
  remoteChangeTag?: string | null;
}

interface SyncPullResult {
  importResult: ImportResult;
  path: string;
}

interface CertificateInfo {
  installed: boolean;
  pem: string;
}

interface RuntimeCapabilities {
  quic: boolean;
  cloudkitSync: boolean;
  icloudSync: boolean;
}

const profileOptions: ProfileOptions = {
  includeRules: true,
  includeValues: true,
  includeReplay: true,
  includeCertificate: true,
  includeAppSettings: true,
  includeSyncSettings: true,
};

const defaultSyncSettings: SyncSettings = {
  enabled: false,
  provider: "cloudkit",
  remotePath: null,
  webdav: {
    endpoint: "",
    username: "",
    password: "",
  },
  cloudkitChangeTag: null,
  lastSyncedAt: null,
};

export function SettingsPage() {
  const { config, status: proxyStatus, setConfig } = useProxyStore();
  const { theme, setTheme } = useThemeStore();
  const columns = useColumnsStore((state) => state.columns);
  const loadRuleGroups = useRulesStore((state) => state.loadGroups);
  const loadValues = useValuesStore((state) => state.loadValues);
  const fetchReplayTree = useReplayStore((state) => state.fetchTree);
  const {
    currentVersion,
    isChecking,
    isDownloading,
    isDownloaded,
    isInstalling,
    downloadProgress,
    updateAvailable,
    updateInfo,
    error,
    lastChecked,
    autoCheck,
    autoDownload,
    channel,
    checkForUpdates,
    downloadAndInstall,
    installUpdate,
    setChannel,
    setAutoCheck,
    setAutoDownload,
  } = useUpdaterStore();

  const [isInstallingCert, setIsInstallingCert] = useState(false);
  const [isExportingCert, setIsExportingCert] = useState(false);
  const [certInstalled, setCertInstalled] = useState(false);
  const [certExported, setCertExported] = useState<string | null>(null);
  const [certError, setCertError] = useState<string | null>(null);
  const [now, setNow] = useState(() => Date.now());
  const [profileStatus, setProfileStatus] = useState<string | null>(null);
  const [profileError, setProfileError] = useState<string | null>(null);
  const [profileSummary, setProfileSummary] = useState<ProfileSummary | null>(null);
  const [profileBusy, setProfileBusy] = useState<"export" | "import" | "whistle" | null>(null);
  const [syncSettings, setSyncSettings] = useState<SyncSettings>(defaultSyncSettings);
  const [syncStatusText, setSyncStatusText] = useState<string | null>(null);
  const [syncError, setSyncError] = useState<string | null>(null);
  const [syncBusy, setSyncBusy] = useState(false);
  const [runtimeCapabilities, setRuntimeCapabilities] = useState<RuntimeCapabilities | null>(null);

  useEffect(() => {
    let isCurrent = true;

    void (async () => {
      const [certificateResult, capabilitiesResult] = await Promise.allSettled([
        invoke<CertificateInfo>("get_ca_certificate"),
        invoke<RuntimeCapabilities>("get_runtime_capabilities"),
      ]);
      if (!isCurrent) return;

      if (certificateResult.status === "fulfilled") {
        setCertInstalled(certificateResult.value.installed);
      } else {
        setCertError(String(certificateResult.reason));
      }

      if (capabilitiesResult.status === "fulfilled") {
        setRuntimeCapabilities(capabilitiesResult.value);
      } else {
        setRuntimeCapabilities({ quic: false, cloudkitSync: false, icloudSync: false });
      }
    })();

    return () => {
      isCurrent = false;
    };
  }, []);

  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now()), 60000);
    return () => clearInterval(timer);
  }, []);

  const currentAppSettings = (): AppSettingsBackup => ({
    proxy: config,
    theme,
    columns,
    updates: {
      autoCheck,
      autoDownload,
      channel,
    },
  });

  const normalizeSyncSettings = useCallback((settings: SyncSettings): SyncSettings => {
    const normalized = {
      ...defaultSyncSettings,
      ...settings,
      webdav: {
        ...defaultSyncSettings.webdav!,
        ...(settings.webdav ?? {}),
      },
    };
    if (runtimeCapabilities?.icloudSync === false && normalized.provider === "icloud") {
      normalized.provider = "webdav";
    }
    if (runtimeCapabilities?.cloudkitSync === false && normalized.provider === "cloudkit") {
      normalized.provider = "webdav";
    }
    return normalized;
  }, [runtimeCapabilities?.cloudkitSync, runtimeCapabilities?.icloudSync]);

  const loadSyncStatus = useCallback(async (isCurrent: () => boolean = () => true) => {
    try {
      const status = await invoke<SyncStatus>("get_sync_status");
      if (!isCurrent()) return;
      const normalized = normalizeSyncSettings(status.config);
      setSyncSettings(normalized);
    } catch (e) {
      if (!isCurrent()) return;
      setSyncError(String(e));
    }
  }, [normalizeSyncSettings]);

  useEffect(() => {
    let isCurrent = true;
    queueMicrotask(() => {
      void loadSyncStatus(() => isCurrent);
    });
    return () => {
      isCurrent = false;
    };
  }, [loadSyncStatus]);

  const refreshDataStores = async () => {
    await Promise.all([
      loadRuleGroups(),
      loadValues(),
      fetchReplayTree(),
    ]);
  };

  const applyImportedSettings = async (settings?: AppSettingsBackup | null) => {
    if (!settings) return;

    if (settings.proxy) {
      setConfig(settings.proxy);
    }
    if (settings.theme && ["light", "dark", "system"].includes(settings.theme)) {
      setTheme(settings.theme);
    }
    if (settings.updates) {
      setAutoCheck(settings.updates.autoCheck);
      setAutoDownload(settings.updates.autoDownload);
      if (settings.updates.channel) {
        await setChannel(settings.updates.channel);
      }
    }
    if (settings.columns) {
      const nextColumns = Array.isArray(settings.columns)
        ? settings.columns
        : settings.columns.state?.columns;
      if (Array.isArray(nextColumns)) {
        localStorage.setItem(
          "postgate-columns",
          JSON.stringify({
            state: { columns: nextColumns },
            version: 0,
          }),
        );
        useColumnsStore.setState({ columns: nextColumns });
      }
    }
  };

  const handleExportProfile = async () => {
    setProfileBusy("export");
    setProfileError(null);
    setProfileStatus(null);
    try {
      const selected = await save({
        title: "Export PostGate profile",
        defaultPath: `postgate-profile-${new Date().toISOString().slice(0, 10)}.json`,
        filters: [{ name: "PostGate Profile", extensions: ["json"] }],
      });
      if (!selected) return;

      const summary = await invoke<ProfileSummary>("export_profile", {
        input: {
          path: selected,
          appSettings: currentAppSettings(),
          syncSettings,
          options: profileOptions,
        },
      });
      setProfileSummary(summary);
      setProfileStatus(`Exported ${summary.ruleGroups} rule groups and ${summary.values} values.`);
    } catch (e) {
      setProfileError(String(e));
    } finally {
      setProfileBusy(null);
    }
  };

  const handleImportProfile = async () => {
    setProfileBusy("import");
    setProfileError(null);
    setProfileStatus(null);
    try {
      const selected = await open({
        title: "Import PostGate profile",
        multiple: false,
        filters: [{ name: "PostGate Profile", extensions: ["json"] }],
      });
      if (!selected || typeof selected !== "string") return;

      const result = await invoke<ImportResult>("import_profile", {
        input: {
          path: selected,
          options: {
            profileOptions,
            replaceExisting: true,
          },
        },
      });
      await applyImportedSettings(result.appSettings);
      if (result.syncSettings) {
        setSyncSettings(normalizeSyncSettings(result.syncSettings));
        await loadSyncStatus();
      }
      await refreshDataStores();
      setProfileSummary(result.summary);
      setProfileStatus(`Imported ${result.summary.ruleGroups} rule groups and ${result.summary.savedRequests} replay requests.`);
    } catch (e) {
      setProfileError(String(e));
    } finally {
      setProfileBusy(null);
    }
  };

  const handleImportWhistleRules = async () => {
    setProfileBusy("whistle");
    setProfileError(null);
    setProfileStatus(null);
    setProfileSummary(null);
    try {
      const selected = await open({
        title: "Import Whistle rules",
        multiple: false,
      });
      if (!selected || typeof selected !== "string") return;

      const result = await invoke<WhistleImportResult>("import_whistle_rules", {
        input: {
          path: selected,
        },
      });
      await loadRuleGroups();
      setProfileStatus(
        `Imported ${result.ruleCount} rules across ${result.groups.length} rule ${result.groups.length === 1 ? "group" : "groups"}.`,
      );
    } catch (e) {
      setProfileError(String(e));
    } finally {
      setProfileBusy(null);
    }
  };

  const handleICloudSync = async () => {
    setSyncBusy(true);
    setSyncError(null);
    setSyncStatusText(null);
    try {
      const nextSettings: SyncSettings = {
        ...syncSettings,
        enabled: true,
        provider: "cloudkit",
        remotePath: null,
      };
      const status = await invoke<SyncStatus>("save_sync_settings", {
        settings: nextSettings,
      });
      setSyncSettings(normalizeSyncSettings(status.config));

      const remoteChanged =
        (status.remoteChangeTag ?? null) !== (status.config.cloudkitChangeTag ?? null);
      if (status.remoteAvailable && remoteChanged) {
        const result = await invoke<SyncPullResult>("pull_sync_profile");
        await applyImportedSettings(result.importResult.appSettings);
        if (result.importResult.syncSettings) {
          setSyncSettings(normalizeSyncSettings(result.importResult.syncSettings));
        }
        await refreshDataStores();
        setSyncStatusText("Synced from iCloud.");
      } else {
        await invoke<ProfileSummary>("push_sync_profile", {
          appSettings: currentAppSettings(),
        });
        setSyncStatusText("Synced to iCloud.");
      }

      await loadSyncStatus();
    } catch (e) {
      setSyncError(String(e));
    } finally {
      setSyncBusy(false);
    }
  };

  const handleInstallCertificate = async () => {
    setIsInstallingCert(true);
    setCertError(null);
    try {
      await invoke("install_ca_certificate");
      const info = await invoke<CertificateInfo>("get_ca_certificate");
      setCertInstalled(info.installed);
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
      const exportPath = await save({
        title: "Export PostGate root certificate",
        defaultPath: "postgate-ca.pem",
        filters: [{ name: "PEM Certificate", extensions: ["pem", "crt"] }],
      });
      if (!exportPath) return;
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
    const diff = now - lastChecked;
    if (diff < 60000) return "Just now";
    if (diff < 3600000) return `${Math.floor(diff / 60000)} minutes ago`;
    if (diff < 86400000) return `${Math.floor(diff / 3600000)} hours ago`;
    return new Date(lastChecked).toLocaleDateString();
  };

  const proxyConfigLocked = ["starting", "running", "stopping"].includes(proxyStatus);
  const iCloudSyncStatus = runtimeCapabilities?.cloudkitSync === false
    ? "Unavailable"
    : syncBusy
      ? "Syncing"
      : syncError
        ? "Failed"
        : syncSettings.lastSyncedAt
          ? "Synced"
          : "Not synced";

  return (
    <div className="flex h-full flex-col">
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
                      <Badge variant="outline" className="text-[10px] uppercase">
                        {channel}
                      </Badge>
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
                      <Button
                        size="sm"
                        onClick={() => isDownloaded ? installUpdate() : downloadAndInstall()}
                      >
                        {isDownloaded ? "Install & Restart" : "Download & Install"}
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

                  {isDownloaded && !isInstalling && (
                    <div className="mt-3 flex items-center gap-2 text-xs text-muted-foreground">
                      <Check className="h-3.5 w-3.5" />
                      Update downloaded and ready to install
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
                label="Update Channel"
                description={channel === "stable"
                  ? "Receive production-ready releases only"
                  : "Receive preview builds before stable releases"}
              >
                <div
                  role="radiogroup"
                  aria-label="Update channel"
                  className="inline-flex h-8 items-center rounded-md border bg-muted/30 p-0.5"
                >
                  {(["stable", "beta"] as const).map((value) => (
                    <button
                      key={value}
                      type="button"
                      role="radio"
                      aria-checked={channel === value}
                      disabled={isChecking || isDownloading || isInstalling}
                      onClick={() => void setChannel(value)}
                      className={cn(
                        "h-6 rounded px-2.5 text-xs font-medium capitalize text-muted-foreground transition-colors",
                        "focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring",
                        "disabled:pointer-events-none disabled:opacity-50",
                        channel === value && "bg-background text-foreground shadow-sm",
                      )}
                    >
                      {value}
                    </button>
                  ))}
                </div>
              </SettingRow>

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
                <PortInput
                  value={config.port}
                  onChange={(port) => setConfig({ port })}
                  disabled={proxyConfigLocked}
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
                  disabled={proxyConfigLocked}
                />
              </SettingRow>

              <SettingRow
                icon={<Network className="h-4 w-4" />}
                label="QUIC / HTTP/3"
                description={runtimeCapabilities?.quic === false
                  ? "Not included in this build"
                  : "Experimental support for QUIC protocol"}
              >
                <Switch
                  checked={config.enableQuic}
                  onCheckedChange={(checked) => setConfig({ enableQuic: checked })}
                  disabled={proxyConfigLocked || runtimeCapabilities?.quic !== true}
                />
              </SettingRow>

              <SettingRow
                icon={<Bug className="h-4 w-4" />}
                label="Debug Server Port"
                description="Port for Chrome DevTools Protocol debugging"
              >
                <PortInput
                  value={config.debugPort}
                  onChange={(debugPort) => setConfig({ debugPort })}
                  disabled={proxyConfigLocked}
                />
              </SettingRow>
            </div>
          </Section>

          <McpAccessSection />

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

          {/* Profile Transfer */}
          <Section title="Profile Transfer">
            <div className="space-y-4">
              <div className="flex items-start justify-between gap-4">
                <div className="flex items-start gap-3 min-w-0">
                  <div className="h-9 w-9 rounded-lg bg-muted/50 flex items-center justify-center shrink-0 mt-0.5">
                    <HardDriveUpload className="h-4 w-4 text-muted-foreground" />
                  </div>
                  <div className="min-w-0">
                    <p className="text-sm font-medium">Portable Profile</p>
                    <p className="text-xs text-muted-foreground mt-0.5 leading-relaxed">
                      Export or restore rules, values, replay collections, certificate material, UI preferences, and sync setup. Treat profile files as sensitive.
                    </p>
                  </div>
                </div>
                <div className="flex shrink-0 flex-wrap justify-end gap-2">
                  <Button
                    variant="outline"
                    size="sm"
                    className="h-8 gap-1.5 text-xs"
                    onClick={handleImportProfile}
                    disabled={profileBusy !== null}
                  >
                    {profileBusy === "import" ? (
                      <Loader2 className="h-3.5 w-3.5 animate-spin" />
                    ) : (
                      <FileUp className="h-3.5 w-3.5" />
                    )}
                    Import
                  </Button>
                  <Button
                    variant="outline"
                    size="sm"
                    className="h-8 gap-1.5 text-xs whitespace-nowrap"
                    onClick={handleImportWhistleRules}
                    disabled={profileBusy !== null}
                  >
                    {profileBusy === "whistle" ? (
                      <Loader2 className="h-3.5 w-3.5 animate-spin" />
                    ) : (
                      <FileUp className="h-3.5 w-3.5" />
                    )}
                    Import from Whistle
                  </Button>
                  <Button
                    size="sm"
                    className="h-8 gap-1.5 text-xs"
                    onClick={handleExportProfile}
                    disabled={profileBusy !== null}
                  >
                    {profileBusy === "export" ? (
                      <Loader2 className="h-3.5 w-3.5 animate-spin" />
                    ) : (
                      <FileDown className="h-3.5 w-3.5" />
                    )}
                    Export
                  </Button>
                </div>
              </div>

              <div className="grid grid-cols-2 gap-2 md:grid-cols-3">
                <ProfileChip label="Rules" active />
                <ProfileChip label="Values" active />
                <ProfileChip label="Replay" active />
                <ProfileChip label="Certificate" active />
                <ProfileChip label="UI Settings" active />
                <ProfileChip label="Sync" active />
              </div>

              {profileSummary && (
                <div className="grid grid-cols-2 gap-3 rounded-md border bg-muted/20 p-3 md:grid-cols-4">
                  <InfoItem label="Rule Groups" value={String(profileSummary.ruleGroups)} mono />
                  <InfoItem label="Values" value={String(profileSummary.values)} mono />
                  <InfoItem label="Collections" value={String(profileSummary.collections)} mono />
                  <InfoItem label="Requests" value={String(profileSummary.savedRequests)} mono />
                </div>
              )}

              <StatusLine status={profileStatus} error={profileError} />
            </div>
          </Section>

          {/* Sync */}
          <Section title="iCloud Sync">
            <div className="space-y-3">
              <div className="flex items-center justify-between gap-4">
                <div className="flex min-w-0 items-center gap-3">
                  <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-md bg-muted">
                    <Cloud className="h-4 w-4 text-muted-foreground" />
                  </div>
                  <div className="flex min-w-0 items-center gap-2 text-xs text-muted-foreground">
                    <Clock3 className="h-3.5 w-3.5 shrink-0" />
                    <span className="truncate">
                      {syncSettings.lastSyncedAt
                        ? `Last sync ${formatTimestamp(syncSettings.lastSyncedAt)}`
                        : "Not synced yet"}
                    </span>
                    <Badge
                      variant="outline"
                      className={cn(
                        "h-5 shrink-0 text-[10px]",
                        iCloudSyncStatus === "Synced" && "border-emerald-500/30 text-emerald-600 dark:text-emerald-400",
                        iCloudSyncStatus === "Failed" && "border-red-500/30 text-red-600 dark:text-red-400",
                      )}
                    >
                      {iCloudSyncStatus}
                    </Badge>
                  </div>
                </div>
                <Button
                  size="sm"
                  className="h-8 shrink-0 gap-1.5 text-xs"
                  onClick={handleICloudSync}
                  disabled={syncBusy || runtimeCapabilities?.cloudkitSync === false}
                >
                  {syncBusy ? (
                    <Loader2 className="h-3.5 w-3.5 animate-spin" />
                  ) : (
                    <RefreshCw className="h-3.5 w-3.5" />
                  )}
                  Sync
                </Button>
              </div>

              <StatusLine status={syncStatusText} error={syncError} />
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
