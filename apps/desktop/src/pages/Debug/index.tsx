import { useEffect, useState } from "react";
import {
  Bug,
  Info,
  WifiOff,
  Copy,
  Check,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  useDebugStore,
  DebugSession,
  setupDebugListeners,
  cleanupDebugListeners,
} from "@/stores/debug";
import { cn } from "@/lib/utils";

export function DebugPage() {
  const {
    status,
    sessions,
    selectedSessionId,
    fetchStatus,
    syncWithRules,
    fetchSessions,
    selectSession,
  } = useDebugStore();

  // Initialize and sync with rules
  useEffect(() => {
    fetchStatus();
    syncWithRules();
    setupDebugListeners();

    const interval = setInterval(() => {
      fetchStatus();
      if (status.is_running) {
        fetchSessions();
      }
    }, 2000);

    return () => {
      clearInterval(interval);
      cleanupDebugListeners();
    };
  }, []);

  return (
    <div className="flex h-full flex-col">
      {/* Toolbar */}
      <div className="flex h-12 items-center gap-3 border-b px-4 bg-muted/10">
        {/* Status indicator */}
        <div className="flex items-center gap-2">
          <Bug className="h-4 w-4 text-muted-foreground" />
          <span className="text-sm font-medium">DevTools</span>
        </div>

        <div className="h-4 w-px bg-border mx-1" />

        {/* Stats */}
        <div className="flex items-center gap-4 text-xs text-muted-foreground">
          <div className="flex items-center gap-1.5">
            <span className={cn("h-2 w-2 rounded-full", status.is_running ? "bg-green-500" : "bg-muted-foreground/30")} />
            <span>{status.is_running ? `Running on port ${status.port}` : "Not running"}</span>
          </div>
          <div className="flex items-center gap-1.5">
            <span className="font-medium text-foreground">{status.session_count}</span>
            <span>sessions</span>
          </div>
        </div>

        <div className="flex-1" />

        {!status.is_running && (
          <div className="text-xs text-muted-foreground">
            Add a <code className="px-1 py-0.5 bg-muted rounded font-mono">debug://</code> rule to enable
          </div>
        )}
      </div>

      {/* Main content */}
      <div className="flex flex-1 overflow-hidden">
        {/* Sessions sidebar */}
        <div className="w-60 border-r flex flex-col bg-muted/10">
          <div className="flex h-9 items-center px-3 border-b">
            <h3 className="text-xs font-semibold text-muted-foreground uppercase tracking-wider">Connected Pages</h3>
          </div>
          <ScrollArea className="flex-1">
            {sessions.length === 0 ? (
              <div className="p-8 text-center">
                <div className="inline-flex h-8 w-8 items-center justify-center rounded-lg bg-muted mb-3">
                  <WifiOff className="h-4 w-4 text-muted-foreground" />
                </div>
                {status.is_running ? (
                  <p className="text-xs text-muted-foreground">Waiting for connections...</p>
                ) : (
                  <p className="text-xs text-muted-foreground">Add debug:// rules to start</p>
                )}
              </div>
            ) : (
              <div className="p-2 space-y-0.5">
                <button
                  className={cn(
                    "w-full text-left px-2.5 py-2 rounded-md text-xs transition-colors",
                    selectedSessionId === null
                      ? "bg-primary/10 text-primary font-medium"
                      : "text-muted-foreground hover:bg-muted hover:text-foreground"
                  )}
                  onClick={() => selectSession(null)}
                >
                  All Sessions
                </button>
                {sessions.map((session) => (
                  <SessionItem
                    key={session.id}
                    session={session}
                    isSelected={selectedSessionId === session.id}
                    onClick={() => selectSession(session.id)}
                  />
                ))}
              </div>
            )}
          </ScrollArea>
        </div>

        {/* DevTools panel */}
        <div className="flex-1 flex flex-col bg-background">
          <DevToolsPanel 
            sessions={sessions} 
            selectedSessionId={selectedSessionId}
            status={status}
          />
        </div>
      </div>
    </div>
  );
}

// Session item component
function SessionItem({
  session,
  isSelected,
  onClick,
}: {
  session: DebugSession;
  isSelected: boolean;
  onClick: () => void;
}) {
  const hostname = new URL(session.url).hostname;

  return (
    <button
      className={cn(
        "w-full text-left px-2.5 py-2 rounded-md text-xs transition-all group",
        isSelected 
          ? "bg-primary/10 text-primary" 
          : "text-muted-foreground hover:bg-muted hover:text-foreground"
      )}
      onClick={onClick}
    >
      <div className="flex items-center gap-2 mb-0.5">
        <span className={cn(
          "h-1.5 w-1.5 rounded-full flex-shrink-0 transition-colors",
          session.is_connected ? "bg-green-500 shadow-[0_0_4px_rgba(34,197,94,0.4)]" : "bg-zinc-300 dark:bg-zinc-700"
        )} />
        <span className="truncate font-medium flex-1">{session.title || hostname}</span>
      </div>
      <div className="pl-3.5 truncate opacity-70 text-[10px]">
        {hostname}
      </div>
    </button>
  );
}

// DevTools panel component - provides access to Chrome DevTools via CDP
function DevToolsPanel({
  sessions,
  selectedSessionId,
  status,
}: {
  sessions: DebugSession[];
  selectedSessionId: string | null;
  status: { is_running: boolean; port: number };
}) {
  const [copiedUrl, setCopiedUrl] = useState<string | null>(null);

  const cdpSessions = sessions.filter(s => s.cdp_enabled && s.is_connected);

  const copyToClipboard = (url: string) => {
    navigator.clipboard.writeText(url);
    setCopiedUrl(url);
    setTimeout(() => setCopiedUrl(null), 2000);
  };

  // Generate Chrome DevTools URL
  const getDevToolsUrl = (session: DebugSession) => {
    // Chrome DevTools can connect to our WebSocket endpoint
    // Format: devtools://devtools/bundled/inspector.html?ws=HOST:PORT/devtools/page/PAGE_ID
    const wsUrl = session.webSocketDebuggerUrl.replace('ws://', '');
    return `devtools://devtools/bundled/inspector.html?ws=${wsUrl}`;
  };

  if (!status.is_running) {
    return (
      <div className="flex h-full flex-col items-center justify-center p-8 text-center">
        <div className="inline-flex h-12 w-12 items-center justify-center rounded-xl bg-muted/50 mb-4">
          <Bug className="h-6 w-6 text-muted-foreground" />
        </div>
        <h3 className="font-medium mb-1">Debug Server Not Running</h3>
        <p className="text-sm text-muted-foreground max-w-sm">
          Add a <code className="px-1 py-0.5 bg-muted rounded font-mono text-xs">debug://</code> rule 
          to any rule group to automatically start the debug server.
        </p>
        <div className="w-full max-w-sm bg-muted/50 border rounded-lg p-3 text-left mt-6">
          <p className="text-[10px] font-medium text-muted-foreground mb-1.5 uppercase tracking-wider">Example Rule</p>
          <code className="text-xs font-mono">example.com debug://mypage</code>
        </div>
      </div>
    );
  }

  if (cdpSessions.length === 0) {
    return (
      <div className="flex h-full flex-col items-center justify-center p-8 text-center">
        <div className="inline-flex h-12 w-12 items-center justify-center rounded-xl bg-muted/50 mb-4">
          <Bug className="h-6 w-6 text-muted-foreground" />
        </div>
        <h3 className="font-medium mb-1">Waiting for Connections</h3>
        <p className="text-sm text-muted-foreground max-w-sm mb-6">
          The debug server is running. Visit a page that matches your 
          <code className="px-1 py-0.5 bg-muted rounded font-mono text-xs mx-1">debug://</code> 
          rule to connect.
        </p>
        <div className="w-full max-w-sm bg-muted/50 border rounded-lg p-3 text-left">
          <p className="text-[10px] font-medium text-muted-foreground mb-1.5 uppercase tracking-wider">Server Status</p>
          <div className="flex items-center gap-2">
            <span className="relative flex h-2 w-2">
              <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-green-400 opacity-75"></span>
              <span className="relative inline-flex rounded-full h-2 w-2 bg-green-500"></span>
            </span>
            <code className="text-xs font-mono">localhost:{status.port}</code>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="h-full p-6 overflow-auto">
      <div className="max-w-3xl mx-auto">
        <div className="mb-6">
          <h3 className="text-lg font-semibold mb-1">Chrome DevTools Connections</h3>
          <p className="text-sm text-muted-foreground">
            Connect to these pages using Chrome DevTools or compatible CDP clients.
          </p>
        </div>

        <div className="grid gap-4">
          {cdpSessions.map((session) => {
            const devToolsUrl = getDevToolsUrl(session);
            const hostname = new URL(session.url).hostname;
            
            return (
              <div 
                key={session.id}
                className={cn(
                  "group border bg-card rounded-lg p-4 transition-all hover:shadow-sm",
                  selectedSessionId === session.id && "border-primary/50 ring-1 ring-primary/20"
                )}
              >
                <div className="flex items-start justify-between gap-4 mb-4">
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-2 mb-1">
                      <span className="relative flex h-2.5 w-2.5">
                        <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-green-400 opacity-75"></span>
                        <span className="relative inline-flex rounded-full h-2.5 w-2.5 bg-green-500"></span>
                      </span>
                      <span className="font-semibold text-sm truncate">
                        {session.title || hostname}
                      </span>
                    </div>
                    <p className="text-xs text-muted-foreground truncate pl-4.5">
                      {session.url}
                    </p>
                  </div>
                </div>

                <div className="grid gap-3 p-3 bg-muted/30 rounded-md">
                  {/* DevTools URL */}
                  <div>
                    <label className="text-[10px] font-medium text-muted-foreground uppercase tracking-wider mb-1.5 block">
                      DevTools URL (Paste in Chrome)
                    </label>
                    <div className="flex items-center gap-2">
                      <code className="flex-1 text-[11px] bg-background border px-2 py-1.5 rounded truncate font-mono text-muted-foreground">
                        {devToolsUrl}
                      </code>
                      <Button
                        variant="outline"
                        size="icon"
                        className="h-7 w-7 shrink-0"
                        onClick={() => copyToClipboard(devToolsUrl)}
                      >
                        {copiedUrl === devToolsUrl ? (
                          <Check className="h-3.5 w-3.5 text-green-500" />
                        ) : (
                          <Copy className="h-3.5 w-3.5" />
                        )}
                      </Button>
                    </div>
                  </div>

                  {/* WebSocket URL */}
                  <div>
                    <label className="text-[10px] font-medium text-muted-foreground uppercase tracking-wider mb-1.5 block">
                      WebSocket URL (For Clients)
                    </label>
                    <div className="flex items-center gap-2">
                      <code className="flex-1 text-[11px] bg-background border px-2 py-1.5 rounded truncate font-mono text-muted-foreground">
                        {session.webSocketDebuggerUrl}
                      </code>
                      <Button
                        variant="outline"
                        size="icon"
                        className="h-7 w-7 shrink-0"
                        onClick={() => copyToClipboard(session.webSocketDebuggerUrl)}
                      >
                        {copiedUrl === session.webSocketDebuggerUrl ? (
                          <Check className="h-3.5 w-3.5 text-green-500" />
                        ) : (
                          <Copy className="h-3.5 w-3.5" />
                        )}
                      </Button>
                    </div>
                  </div>
                </div>
              </div>
            );
          })}
        </div>

        <div className="mt-8 p-4 bg-muted/30 border rounded-lg">
          <div className="flex gap-3">
            <Info className="h-5 w-5 text-blue-500 flex-shrink-0 mt-0.5" />
            <div className="space-y-2">
              <h4 className="font-medium text-sm">Connection Instructions</h4>
              <ol className="text-xs text-muted-foreground space-y-1.5 list-decimal list-inside">
                <li>Copy the <strong>DevTools URL</strong> from a session above</li>
                <li>Open a new tab in Chrome or Edge</li>
                <li>Paste the URL into the address bar and press Enter</li>
                <li>The full DevTools interface will open, connected to the remote page</li>
              </ol>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
