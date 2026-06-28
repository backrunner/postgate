import { useCallback, useEffect, useState } from "react";
import {
  Ban,
  Bot,
  Copy,
  KeyRound,
  Loader2,
  RotateCw,
  ShieldCheck,
  Terminal,
} from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { cn } from "@/lib/utils";
import { formatTimestamp, Section, SettingRow, StatusLine } from "./components";

interface McpStatus {
  enabled: boolean;
  running: boolean;
  port: number;
  endpoint: string;
  clientCount: number;
  error: string | null;
}

interface McpClient {
  id: string;
  name: string;
  scopes: string[];
  revoked: boolean;
  createdAt: number;
  updatedAt: number;
  lastSeenAt?: number | null;
}

interface CreatedMcpClient {
  client: McpClient;
  token: string;
  endpoint: string;
  streamableHttpConfig: unknown;
  stdioConfig: unknown;
}

interface McpClientConfig {
  endpoint: string;
  streamableHttpConfig: unknown;
  stdioConfig: unknown;
}

interface McpAuditEvent {
  id: string;
  timestamp: number;
  clientId?: string | null;
  operation: string;
  target?: string | null;
  allowed: boolean;
  detail?: string | null;
}

const mcpScopes = [
  { value: "proxy:read", label: "Proxy Read" },
  { value: "proxy:control", label: "Proxy Control" },
  { value: "rules:read", label: "Rules Read" },
  { value: "rules:write", label: "Rules Write" },
  { value: "values:read", label: "Values Read" },
  { value: "values:write", label: "Values Write" },
  { value: "capture:read", label: "Capture Read" },
  { value: "body:read", label: "Body Read" },
  { value: "history:delete", label: "History Delete" },
  { value: "replay:execute", label: "Replay Execute" },
  { value: "debug:read", label: "Debug Read" },
  { value: "mcp:admin", label: "MCP Admin" },
];

const defaultMcpScopes = mcpScopes
  .map((scope) => scope.value)
  .filter((scope) => scope !== "history:delete" && scope !== "mcp:admin");

export function McpAccessSection() {
  const [mcpStatus, setMcpStatus] = useState<McpStatus | null>(null);
  const [mcpClients, setMcpClients] = useState<McpClient[]>([]);
  const [mcpAuditEvents, setMcpAuditEvents] = useState<McpAuditEvent[]>([]);
  const [mcpConfig, setMcpConfig] = useState<McpClientConfig | null>(null);
  const [mcpPort, setMcpPort] = useState(18999);
  const [mcpClientName, setMcpClientName] = useState("Local Agent");
  const [mcpClientScopes, setMcpClientScopes] = useState<string[]>(defaultMcpScopes);
  const [mcpCreatedClient, setMcpCreatedClient] = useState<CreatedMcpClient | null>(null);
  const [mcpStatusText, setMcpStatusText] = useState<string | null>(null);
  const [mcpError, setMcpError] = useState<string | null>(null);
  const [mcpBusy, setMcpBusy] = useState<"toggle" | "client" | "scope" | "copy" | null>(null);

  const loadMcpAccess = useCallback(async (isCurrent: () => boolean = () => true) => {
    try {
      const [status, clients, audit, config] = await Promise.all([
        invoke<McpStatus>("get_mcp_status"),
        invoke<McpClient[]>("list_mcp_clients"),
        invoke<McpAuditEvent[]>("list_mcp_audit_events", { limit: 8 }),
        invoke<McpClientConfig>("get_mcp_client_config"),
      ]);
      if (!isCurrent()) return;
      setMcpStatus(status);
      setMcpPort(status.port);
      setMcpClients(clients);
      setMcpAuditEvents(audit);
      setMcpConfig(config);
    } catch (e) {
      if (!isCurrent()) return;
      setMcpError(String(e));
    }
  }, []);

  useEffect(() => {
    let isCurrent = true;
    queueMicrotask(() => {
      void loadMcpAccess(() => isCurrent);
    });
    return () => {
      isCurrent = false;
    };
  }, [loadMcpAccess]);

  const handleToggleMcpServer = async (enabled: boolean) => {
    setMcpBusy("toggle");
    setMcpError(null);
    setMcpStatusText(null);
    try {
      const status = enabled
        ? await invoke<McpStatus>("start_mcp_server", {
            input: { port: mcpPort },
          })
        : await invoke<McpStatus>("stop_mcp_server");
      setMcpStatus(status);
      setMcpPort(status.port);
      await loadMcpAccess();
      setMcpStatusText(enabled ? "MCP server started." : "MCP server stopped.");
    } catch (e) {
      setMcpError(String(e));
    } finally {
      setMcpBusy(null);
    }
  };

  const handleCreateMcpClient = async () => {
    setMcpBusy("client");
    setMcpError(null);
    setMcpStatusText(null);
    try {
      const created = await invoke<CreatedMcpClient>("create_mcp_client", {
        input: {
          name: mcpClientName.trim() || "Local Agent",
          scopes: mcpClientScopes,
        },
      });
      setMcpCreatedClient(created);
      setMcpStatusText(`Created ${created.client.name}. Copy the token now.`);
      await loadMcpAccess();
    } catch (e) {
      setMcpError(String(e));
    } finally {
      setMcpBusy(null);
    }
  };

  const handleRevokeMcpClient = async (id: string) => {
    setMcpBusy("client");
    setMcpError(null);
    setMcpStatusText(null);
    try {
      await invoke<boolean>("revoke_mcp_client", { id });
      await loadMcpAccess();
      setMcpStatusText("Client revoked.");
    } catch (e) {
      setMcpError(String(e));
    } finally {
      setMcpBusy(null);
    }
  };

  const handleRotateMcpClient = async (id: string) => {
    setMcpBusy("client");
    setMcpError(null);
    setMcpStatusText(null);
    try {
      const created = await invoke<CreatedMcpClient>("rotate_mcp_client_token", { id });
      setMcpCreatedClient(created);
      await loadMcpAccess();
      setMcpStatusText(`Rotated token for ${created.client.name}.`);
    } catch (e) {
      setMcpError(String(e));
    } finally {
      setMcpBusy(null);
    }
  };

  const handleSetMcpClientScopes = async (id: string, scopes: string[]) => {
    setMcpBusy("scope");
    setMcpError(null);
    setMcpStatusText(null);
    try {
      await invoke<boolean>("set_mcp_client_scopes", { id, scopes });
      await loadMcpAccess();
      setMcpStatusText("Scopes updated.");
    } catch (e) {
      setMcpError(String(e));
    } finally {
      setMcpBusy(null);
    }
  };

  const handleCopyText = async (value: string, label: string) => {
    setMcpBusy("copy");
    setMcpError(null);
    try {
      await navigator.clipboard.writeText(value);
      setMcpStatusText(`${label} copied.`);
    } catch (e) {
      setMcpError(String(e));
    } finally {
      setMcpBusy(null);
    }
  };

  const endpoint = mcpStatus?.endpoint ?? mcpConfig?.endpoint ?? `http://127.0.0.1:${mcpPort}/mcp`;

  return (
    <Section title="MCP Access">
      <div className="space-y-4">
        <SettingRow
          icon={<Bot className="h-4 w-4" />}
          label="Agent Control"
          description="Expose local proxy controls and captured traffic to trusted MCP clients"
        >
          <Switch
            checked={mcpStatus?.running ?? false}
            onCheckedChange={handleToggleMcpServer}
            disabled={mcpBusy === "toggle"}
          />
        </SettingRow>

        <SettingRow
          icon={<Terminal className="h-4 w-4" />}
          label="MCP Port"
          description="Streamable HTTP endpoint bound to 127.0.0.1"
        >
          <Input
            type="number"
            value={mcpPort}
            onChange={(e) => setMcpPort(parseInt(e.target.value) || 18999)}
            className="w-24 text-right font-mono h-8 text-sm"
            min={1}
            max={65535}
            disabled={mcpStatus?.running}
          />
        </SettingRow>

        <div className="flex items-center justify-between gap-3 rounded-md border bg-muted/20 p-3">
          <div className="min-w-0">
            <p className="text-xs text-muted-foreground">Endpoint</p>
            <p className="truncate font-mono text-xs">{endpoint}</p>
          </div>
          <Button
            variant="outline"
            size="sm"
            className="h-8 gap-1.5 text-xs"
            onClick={() => handleCopyText(endpoint, "Endpoint")}
          >
            <Copy className="h-3.5 w-3.5" />
            Copy
          </Button>
        </div>

        <div className="space-y-3 rounded-md border bg-muted/20 p-3">
          <div className="flex flex-wrap items-center gap-2">
            <Input
              value={mcpClientName}
              onChange={(e) => setMcpClientName(e.target.value)}
              className="h-8 min-w-48 flex-1 text-xs"
              placeholder="Client name"
            />
            <ScopeMenu scopes={mcpClientScopes} onChange={setMcpClientScopes} disabled={mcpBusy !== null} />
            <Button
              size="sm"
              className="h-8 gap-1.5 text-xs"
              onClick={handleCreateMcpClient}
              disabled={mcpBusy !== null}
            >
              {mcpBusy === "client" ? (
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
              ) : (
                <KeyRound className="h-3.5 w-3.5" />
              )}
              New Client
            </Button>
          </div>

          {mcpCreatedClient && (
            <div className="space-y-2 rounded-md border bg-background/60 p-3">
              <div className="flex items-center gap-2">
                <ShieldCheck className="h-3.5 w-3.5 text-muted-foreground" />
                <span className="text-xs font-medium">{mcpCreatedClient.client.name}</span>
                <Badge variant="secondary" className="h-5 text-[10px]">token shown once</Badge>
              </div>
              <p className="break-all font-mono text-[11px] text-muted-foreground">{mcpCreatedClient.token}</p>
              <div className="flex flex-wrap gap-2">
                <Button
                  variant="outline"
                  size="sm"
                  className="h-7 gap-1.5 text-xs"
                  onClick={() => handleCopyText(mcpCreatedClient.token, "Token")}
                >
                  <Copy className="h-3.5 w-3.5" />
                  Token
                </Button>
                <Button
                  variant="outline"
                  size="sm"
                  className="h-7 gap-1.5 text-xs"
                  onClick={() => handleCopyText(JSON.stringify(mcpCreatedClient.streamableHttpConfig, null, 2), "HTTP config")}
                >
                  <Copy className="h-3.5 w-3.5" />
                  HTTP Config
                </Button>
                <Button
                  variant="outline"
                  size="sm"
                  className="h-7 gap-1.5 text-xs"
                  onClick={() => handleCopyText(JSON.stringify(mcpCreatedClient.stdioConfig, null, 2), "stdio config")}
                >
                  <Copy className="h-3.5 w-3.5" />
                  stdio Config
                </Button>
              </div>
            </div>
          )}
        </div>

        <div className="space-y-2">
          {mcpClients.length === 0 ? (
            <p className="rounded-md border bg-muted/20 px-3 py-2 text-xs text-muted-foreground">
              No MCP clients yet.
            </p>
          ) : (
            mcpClients.map((client) => (
              <div key={client.id} className="flex flex-wrap items-center justify-between gap-3 rounded-md border bg-muted/20 p-3">
                <div className="min-w-0">
                  <div className="flex items-center gap-2">
                    <p className="truncate text-sm font-medium">{client.name}</p>
                    {client.revoked && <Badge variant="destructive" className="h-5 text-[10px]">revoked</Badge>}
                  </div>
                  <p className="text-[11px] text-muted-foreground">
                    {client.lastSeenAt ? `Last seen ${formatTimestamp(client.lastSeenAt)}` : `Created ${formatTimestamp(client.createdAt)}`}
                  </p>
                  <div className="mt-2 flex max-w-xl flex-wrap gap-1">
                    {client.scopes.slice(0, 8).map((scope) => (
                      <Badge key={scope} variant="outline" className="h-5 text-[10px]">{scope}</Badge>
                    ))}
                    {client.scopes.length > 8 && (
                      <Badge variant="secondary" className="h-5 text-[10px]">+{client.scopes.length - 8}</Badge>
                    )}
                  </div>
                </div>
                <div className="flex shrink-0 flex-wrap gap-2">
                  <ScopeMenu
                    scopes={client.scopes}
                    onChange={(scopes) => handleSetMcpClientScopes(client.id, scopes)}
                    disabled={mcpBusy !== null || client.revoked}
                  />
                  <Button
                    variant="outline"
                    size="sm"
                    className="h-8 gap-1.5 text-xs"
                    onClick={() => handleRotateMcpClient(client.id)}
                    disabled={mcpBusy !== null || client.revoked}
                  >
                    <RotateCw className="h-3.5 w-3.5" />
                    Rotate
                  </Button>
                  <Button
                    variant="outline"
                    size="sm"
                    className="h-8 gap-1.5 text-xs"
                    onClick={() => handleRevokeMcpClient(client.id)}
                    disabled={mcpBusy !== null || client.revoked}
                  >
                    <Ban className="h-3.5 w-3.5" />
                    Revoke
                  </Button>
                </div>
              </div>
            ))
          )}
        </div>

        {mcpAuditEvents.length > 0 && (
          <div className="space-y-2 rounded-md border bg-muted/20 p-3">
            <div className="flex items-center justify-between">
              <p className="text-xs font-medium">Recent Audit</p>
              <Badge variant="secondary" className="h-5 text-[10px]">{mcpAuditEvents.length}</Badge>
            </div>
            <div className="space-y-1">
              {mcpAuditEvents.slice(0, 5).map((event) => (
                <div key={event.id} className="flex items-center justify-between gap-3 text-[11px]">
                  <span className={cn("truncate", event.allowed ? "text-muted-foreground" : "text-destructive")}>
                    {event.operation}{event.target ? ` · ${event.target}` : ""}
                  </span>
                  <span className="shrink-0 font-mono text-muted-foreground">{formatTimestamp(event.timestamp)}</span>
                </div>
              ))}
            </div>
          </div>
        )}

        <StatusLine status={mcpStatusText} error={mcpError ?? mcpStatus?.error ?? null} />
      </div>
    </Section>
  );
}

function ScopeMenu({
  scopes,
  onChange,
  disabled,
}: {
  scopes: string[];
  onChange: (scopes: string[]) => void;
  disabled?: boolean;
}) {
  const toggleScope = (scope: string, checked: boolean) => {
    if (checked) {
      onChange(scopes.includes(scope) ? scopes : [...scopes, scope]);
    } else {
      onChange(scopes.filter((item) => item !== scope));
    }
  };

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          variant="outline"
          size="sm"
          className="h-8 gap-1.5 text-xs"
          disabled={disabled}
        >
          <ShieldCheck className="h-3.5 w-3.5" />
          Scopes
          <Badge variant="secondary" className="ml-0.5 h-4 px-1 text-[10px]">
            {scopes.length}
          </Badge>
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-56">
        <DropdownMenuLabel className="text-xs">Allowed Tools</DropdownMenuLabel>
        <DropdownMenuSeparator />
        {mcpScopes.map((scope) => (
          <DropdownMenuCheckboxItem
            key={scope.value}
            className="text-xs"
            checked={scopes.includes(scope.value)}
            onCheckedChange={(checked) => toggleScope(scope.value, checked === true)}
            onSelect={(event) => event.preventDefault()}
          >
            {scope.label}
          </DropdownMenuCheckboxItem>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
