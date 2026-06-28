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
  Bug,
  FileDown,
  FileUp,
  FolderSync,
  HardDriveDownload,
  HardDriveUpload,
  ChevronDown,
  Sun,
  Moon,
  Monitor,
  UploadCloud,
  DownloadCloud,
  Clock3
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
import { open, save } from "@tauri-apps/plugin-dialog";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { PageHeader } from "@/components/layout/PageHeader";
import { cn } from "@/lib/utils";
import { useRulesStore, type RuleGroup } from "@/stores/rules";
import { useValuesStore } from "@/stores/values";
import { useReplayStore } from "@/stores/replay";
import { useColumnsStore, type ColumnConfig } from "@/stores/columns";
import { McpAccessSection } from "./McpAccessSection";
import {
  formatTimestamp,
  InfoItem,
  ProfileChip,
  Section,
  SettingRow,
  StatusLine,
} from "./components";

type SyncProvider = "icloud" | "webdav";

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
  lastSyncedAt?: number | null;
}

interface ImportResult {
  summary: ProfileSummary;
  appSettings?: AppSettingsBackup | null;
  syncSettings?: SyncSettings | null;
}

interface SyncStatus {
  config: SyncSettings;
  localPath: string;
  remoteAvailable: boolean;
}

interface SyncPullResult {
  importResult: ImportResult;
  path: string;
}

interface CertificateInfo {
  installed: boolean;
  pem: string;
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
  provider: "icloud",
  remotePath: null,
  webdav: {
    endpoint: "",
    username: "",
    password: "",
  },
  lastSyncedAt: null,
};

export function SettingsPage() {
  const { config, setConfig } = useProxyStore();
  const { theme, setTheme } = useThemeStore();
  const columns = useColumnsStore((state) => state.columns);
  const loadRuleGroups = useRulesStore((state) => state.loadGroups);
  const loadValues = useValuesStore((state) => state.loadValues);
  const fetchReplayTree = useReplayStore((state) => state.fetchTree);
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
  const [now, setNow] = useState(() => Date.now());
  const [profileStatus, setProfileStatus] = useState<string | null>(null);
  const [profileError, setProfileError] = useState<string | null>(null);
  const [profileSummary, setProfileSummary] = useState<ProfileSummary | null>(null);
  const [profileBusy, setProfileBusy] = useState<"export" | "import" | "whistle" | null>(null);
  const [syncSettings, setSyncSettings] = useState<SyncSettings>(defaultSyncSettings);
  const [syncPath, setSyncPath] = useState<string | null>(null);
  const [syncRemoteAvailable, setSyncRemoteAvailable] = useState(false);
  const [syncStatusText, setSyncStatusText] = useState<string | null>(null);
  const [syncError, setSyncError] = useState<string | null>(null);
  const [syncBusy, setSyncBusy] = useState<"save" | "push" | "pull" | null>(null);

  useEffect(() => {
    initUpdaterSettings();
  }, []);

  useEffect(() => {
    let isCurrent = true;

    void (async () => {
      try {
        const info = await invoke<CertificateInfo>("get_ca_certificate");
        if (isCurrent) {
          setCertInstalled(info.installed);
        }
      } catch (e) {
        if (isCurrent) {
          setCertError(String(e));
        }
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
    },
  });

  const normalizeSyncSettings = useCallback((settings: SyncSettings): SyncSettings => ({
    ...defaultSyncSettings,
    ...settings,
    webdav: {
      ...defaultSyncSettings.webdav!,
      ...(settings.webdav ?? {}),
    },
  }), []);

  const loadSyncStatus = useCallback(async (isCurrent: () => boolean = () => true) => {
    try {
      const status = await invoke<SyncStatus>("get_sync_status");
      if (!isCurrent()) return;
      setSyncSettings(normalizeSyncSettings(status.config));
      setSyncPath(status.localPath);
      setSyncRemoteAvailable(status.remoteAvailable);
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

      const importedGroup = await invoke<RuleGroup>("import_whistle_rules", {
        input: {
          path: selected,
        },
      });
      await loadRuleGroups();
      setProfileStatus(
        `Imported ${importedGroup.rules.length} rules into "${importedGroup.name}".`,
      );
    } catch (e) {
      setProfileError(String(e));
    } finally {
      setProfileBusy(null);
    }
  };

  const handleSaveSyncSettings = async () => {
    setSyncBusy("save");
    setSyncError(null);
    setSyncStatusText(null);
    try {
      const status = await invoke<SyncStatus>("save_sync_settings", {
        settings: syncSettings,
      });
      setSyncSettings(normalizeSyncSettings(status.config));
      setSyncPath(status.localPath);
      setSyncRemoteAvailable(status.remoteAvailable);
      setSyncStatusText("Sync settings saved.");
    } catch (e) {
      setSyncError(String(e));
    } finally {
      setSyncBusy(null);
    }
  };

  const handlePushSync = async () => {
    setSyncBusy("push");
    setSyncError(null);
    setSyncStatusText(null);
    try {
      await invoke<SyncStatus>("save_sync_settings", { settings: syncSettings });
      const summary = await invoke<ProfileSummary>("push_sync_profile", {
        appSettings: currentAppSettings(),
      });
      await loadSyncStatus();
      setSyncStatusText(`Pushed ${summary.ruleGroups} rule groups and ${summary.values} values.`);
    } catch (e) {
      setSyncError(String(e));
    } finally {
      setSyncBusy(null);
    }
  };

  const handlePullSync = async () => {
    setSyncBusy("pull");
    setSyncError(null);
    setSyncStatusText(null);
    try {
      await invoke<SyncStatus>("save_sync_settings", { settings: syncSettings });
      const result = await invoke<SyncPullResult>("pull_sync_profile");
      await applyImportedSettings(result.importResult.appSettings);
      if (result.importResult.syncSettings) {
        setSyncSettings(normalizeSyncSettings(result.importResult.syncSettings));
      }
      await refreshDataStores();
      await loadSyncStatus();
      setSyncStatusText(`Pulled ${result.importResult.summary.ruleGroups} rule groups from sync.`);
    } catch (e) {
      setSyncError(String(e));
    } finally {
      setSyncBusy(null);
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
    const diff = now - lastChecked;
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
          <Section title="Settings Sync">
            <div className="space-y-4">
              <SettingRow
                icon={<FolderSync className="h-4 w-4" />}
                label="Enable Sync"
                description="Use the same profile snapshot format for manual transfer and cloud sync"
              >
                <Switch
                  checked={syncSettings.enabled}
                  onCheckedChange={(enabled) => setSyncSettings((prev) => ({ ...prev, enabled }))}
                />
              </SettingRow>

              <SettingRow
                icon={<Globe className="h-4 w-4" />}
                label="Provider"
                description="iCloud writes a local Cloud Drive file; WebDAV uploads the same JSON profile"
              >
                <DropdownMenu>
                  <DropdownMenuTrigger asChild>
                    <Button variant="outline" size="sm" className="h-8 w-32 justify-between text-xs">
                      {syncSettings.provider === "icloud" ? "iCloud" : "WebDAV"}
                      <ChevronDown className="h-3.5 w-3.5 opacity-50" />
                    </Button>
                  </DropdownMenuTrigger>
                  <DropdownMenuContent align="end" className="w-32">
                    <DropdownMenuItem
                      className="text-xs"
                      onClick={() => setSyncSettings((prev) => ({ ...prev, provider: "icloud" }))}
                    >
                      iCloud
                    </DropdownMenuItem>
                    <DropdownMenuItem
                      className="text-xs"
                      onClick={() => setSyncSettings((prev) => ({ ...prev, provider: "webdav" }))}
                    >
                      WebDAV
                    </DropdownMenuItem>
                  </DropdownMenuContent>
                </DropdownMenu>
              </SettingRow>

              {syncSettings.provider === "icloud" ? (
                <SettingRow
                  icon={<HardDriveDownload className="h-4 w-4" />}
                  label="iCloud Folder"
                  description="Leave empty to use Cloud Drive / Documents / PostGate"
                >
                  <Input
                    value={syncSettings.remotePath ?? ""}
                    onChange={(e) => setSyncSettings((prev) => ({ ...prev, remotePath: e.target.value || null }))}
                    placeholder="Default iCloud path"
                    className="h-8 w-72 text-xs"
                  />
                </SettingRow>
              ) : (
                <div className="space-y-3 rounded-md border bg-muted/20 p-3">
                  <Input
                    value={syncSettings.webdav?.endpoint ?? ""}
                    onChange={(e) =>
                      setSyncSettings((prev) => ({
                        ...prev,
                        webdav: { ...defaultSyncSettings.webdav!, ...prev.webdav, endpoint: e.target.value },
                      }))
                    }
                    placeholder="https://dav.example.com/remote.php/dav/files/me"
                    className="h-8 text-xs"
                  />
                  <div className="grid grid-cols-2 gap-2">
                    <Input
                      value={syncSettings.webdav?.username ?? ""}
                      onChange={(e) =>
                        setSyncSettings((prev) => ({
                          ...prev,
                          webdav: { ...defaultSyncSettings.webdav!, ...prev.webdav, username: e.target.value },
                        }))
                      }
                      placeholder="Username"
                      className="h-8 text-xs"
                    />
                    <Input
                      type="password"
                      value={syncSettings.webdav?.password ?? ""}
                      onChange={(e) =>
                        setSyncSettings((prev) => ({
                          ...prev,
                          webdav: { ...defaultSyncSettings.webdav!, ...prev.webdav, password: e.target.value },
                        }))
                      }
                      placeholder="Password or app token"
                      className="h-8 text-xs"
                    />
                  </div>
                  <Input
                    value={syncSettings.remotePath ?? ""}
                    onChange={(e) => setSyncSettings((prev) => ({ ...prev, remotePath: e.target.value || null }))}
                    placeholder="Optional folder or file path, e.g. PostGate/postgate-profile.json"
                    className="h-8 text-xs"
                  />
                </div>
              )}

              <div className="flex items-center justify-between gap-4 rounded-md border bg-muted/20 p-3">
                <div className="min-w-0 space-y-1">
                  <div className="flex items-center gap-2 text-xs text-muted-foreground">
                    <Clock3 className="h-3.5 w-3.5" />
                    <span>{syncSettings.lastSyncedAt ? `Last sync ${formatTimestamp(syncSettings.lastSyncedAt)}` : "Not synced yet"}</span>
                    {syncRemoteAvailable && <Badge variant="secondary" className="h-5 text-[10px]">Remote ready</Badge>}
                  </div>
                  {syncPath && (
                    <p className="truncate font-mono text-[11px] text-muted-foreground">{syncPath}</p>
                  )}
                </div>
                <div className="flex shrink-0 gap-2">
                  <Button
                    variant="outline"
                    size="sm"
                    className="h-8 gap-1.5 text-xs"
                    onClick={handleSaveSyncSettings}
                    disabled={syncBusy !== null}
                  >
                    {syncBusy === "save" && <Loader2 className="h-3.5 w-3.5 animate-spin" />}
                    Save
                  </Button>
                  <Button
                    variant="outline"
                    size="sm"
                    className="h-8 gap-1.5 text-xs"
                    onClick={handlePullSync}
                    disabled={syncBusy !== null || !syncSettings.enabled}
                  >
                    {syncBusy === "pull" ? (
                      <Loader2 className="h-3.5 w-3.5 animate-spin" />
                    ) : (
                      <DownloadCloud className="h-3.5 w-3.5" />
                    )}
                    Pull
                  </Button>
                  <Button
                    size="sm"
                    className="h-8 gap-1.5 text-xs"
                    onClick={handlePushSync}
                    disabled={syncBusy !== null || !syncSettings.enabled}
                  >
                    {syncBusy === "push" ? (
                      <Loader2 className="h-3.5 w-3.5 animate-spin" />
                    ) : (
                      <UploadCloud className="h-3.5 w-3.5" />
                    )}
                    Push
                  </Button>
                </div>
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
