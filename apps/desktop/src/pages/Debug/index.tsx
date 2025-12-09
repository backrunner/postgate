import { useEffect, useRef, useState } from "react";
import {
  Bug,
  Terminal,
  Play,
  Square,
  Trash2,
  Search,
  Filter,
  AlertTriangle,
  Info,
  AlertCircle,
  ChevronRight,
  ChevronDown,
  X,
  Wifi,
  WifiOff,
  Copy,
  Check,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Badge } from "@/components/ui/badge";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Switch } from "@/components/ui/switch";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuCheckboxItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import {
  useDebugStore,
  ConsoleLog,
  ConsoleLevel,
  DebugSession,
  setupDebugListeners,
  cleanupDebugListeners,
} from "@/stores/debug";
import { cn } from "@/lib/utils";

export function DebugPage() {
  const {
    status,
    isLoading,
    sessions,
    selectedSessionId,
    filteredLogs,
    levelFilter,
    searchFilter,
    autoScroll,
    fetchStatus,
    startServer,
    stopServer,
    fetchSessions,
    selectSession,
    fetchLogs,
    clearLogs,
    setLevelFilter,
    setSearchFilter,
    toggleAutoScroll,
  } = useDebugStore();

  const [expandedLogs, setExpandedLogs] = useState<Set<string>>(new Set());
  const logsEndRef = useRef<HTMLDivElement>(null);

  // Initialize
  useEffect(() => {
    fetchStatus();
    setupDebugListeners();

    const interval = setInterval(() => {
      if (status.is_running) {
        fetchSessions();
        fetchLogs(selectedSessionId || undefined);
      }
    }, 2000);

    return () => {
      clearInterval(interval);
      cleanupDebugListeners();
    };
  }, []);

  // Auto-scroll to bottom
  useEffect(() => {
    if (autoScroll && logsEndRef.current) {
      logsEndRef.current.scrollIntoView({ behavior: "smooth" });
    }
  }, [filteredLogs, autoScroll]);

  const toggleLogExpanded = (logId: string) => {
    const next = new Set(expandedLogs);
    if (next.has(logId)) {
      next.delete(logId);
    } else {
      next.add(logId);
    }
    setExpandedLogs(next);
  };

  const levels: ConsoleLevel[] = ["log", "info", "warn", "error", "debug", "trace"];

  return (
    <div className="flex h-full flex-col">
      {/* Toolbar */}
      <div className="flex h-10 items-center gap-2 border-b px-4">
        {/* Server Controls */}
        <div className="flex items-center gap-2">
          {status.is_running ? (
            <Button
              variant="ghost"
              size="sm"
              onClick={() => stopServer()}
              disabled={isLoading}
              className="gap-1"
            >
              <Square className="h-3.5 w-3.5" />
              Stop
            </Button>
          ) : (
            <Button
              variant="ghost"
              size="sm"
              onClick={() => startServer()}
              disabled={isLoading}
              className="gap-1"
            >
              <Play className="h-3.5 w-3.5" />
              Start
            </Button>
          )}
          <Badge variant={status.is_running ? "default" : "secondary"} className="gap-1">
            {status.is_running ? (
              <Wifi className="h-3 w-3" />
            ) : (
              <WifiOff className="h-3 w-3" />
            )}
            Port {status.port}
          </Badge>
        </div>

        <div className="h-4 w-px bg-border" />

        {/* Stats */}
        <div className="flex items-center gap-2 text-xs text-muted-foreground">
          <span>{status.session_count} sessions</span>
          <span>{filteredLogs.length} logs</span>
        </div>

        <div className="flex-1" />

        {/* Auto-scroll toggle */}
        <div className="flex items-center gap-2">
          <span className="text-xs text-muted-foreground">Auto-scroll</span>
          <Switch checked={autoScroll} onCheckedChange={toggleAutoScroll} />
        </div>

        {/* Filter dropdown */}
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button variant="ghost" size="sm" className="gap-1">
              <Filter className="h-3.5 w-3.5" />
              Filter
              {levelFilter.length > 0 && (
                <Badge variant="secondary" className="ml-1 h-4 w-4 rounded-full p-0 text-[10px]">
                  {levelFilter.length}
                </Badge>
              )}
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            {levels.map((level) => (
              <DropdownMenuCheckboxItem
                key={level}
                checked={levelFilter.includes(level)}
                onCheckedChange={(checked) => {
                  if (checked) {
                    setLevelFilter([...levelFilter, level]);
                  } else {
                    setLevelFilter(levelFilter.filter((l) => l !== level));
                  }
                }}
              >
                <LogLevelIcon level={level} className="mr-2 h-3.5 w-3.5" />
                {level}
              </DropdownMenuCheckboxItem>
            ))}
          </DropdownMenuContent>
        </DropdownMenu>

        {/* Search */}
        <div className="relative">
          <Search className="absolute left-2 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground" />
          <Input
            placeholder="Search..."
            value={searchFilter}
            onChange={(e) => setSearchFilter(e.target.value)}
            className="h-7 w-40 pl-7 text-xs"
          />
          {searchFilter && (
            <Button
              variant="ghost"
              size="sm"
              className="absolute right-0.5 top-1/2 h-5 w-5 -translate-y-1/2 p-0"
              onClick={() => setSearchFilter("")}
            >
              <X className="h-3 w-3" />
            </Button>
          )}
        </div>

        {/* Clear */}
        <Button
          variant="ghost"
          size="sm"
          onClick={() => clearLogs(selectedSessionId || undefined)}
          className="gap-1"
        >
          <Trash2 className="h-3.5 w-3.5" />
          Clear
        </Button>
      </div>

      {/* Main content */}
      <div className="flex flex-1 overflow-hidden">
        {/* Sessions sidebar */}
        <div className="w-64 border-r flex flex-col">
          <div className="p-2 border-b">
            <h3 className="text-xs font-medium text-muted-foreground">Connected Pages</h3>
          </div>
          <ScrollArea className="flex-1">
            {sessions.length === 0 ? (
              <div className="p-4 text-center text-xs text-muted-foreground">
                {status.is_running ? (
                  <p>Waiting for connections...</p>
                ) : (
                  <p>Start the debug server to capture console logs</p>
                )}
              </div>
            ) : (
              <div className="p-2 space-y-1">
                <button
                  className={cn(
                    "w-full text-left px-2 py-1.5 rounded text-xs",
                    selectedSessionId === null
                      ? "bg-accent text-accent-foreground"
                      : "hover:bg-muted"
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

        {/* Console output */}
        <div className="flex-1 flex flex-col">
          <Tabs defaultValue="console" className="flex-1 flex flex-col">
            <div className="flex h-8 items-center border-b px-4">
              <TabsList className="h-7">
                <TabsTrigger value="console" className="h-6 gap-1 text-xs">
                  <Terminal className="h-3.5 w-3.5" />
                  Console
                </TabsTrigger>
                <TabsTrigger value="devtools" className="h-6 gap-1 text-xs">
                  <Bug className="h-3.5 w-3.5" />
                  DevTools
                </TabsTrigger>
                <TabsTrigger value="errors" className="h-6 gap-1 text-xs">
                  <AlertCircle className="h-3.5 w-3.5" />
                  Errors
                </TabsTrigger>
              </TabsList>
            </div>

            <TabsContent value="console" className="flex-1 mt-0 overflow-hidden">
              <ScrollArea className="h-full">
                <div className="font-mono text-xs">
                  {filteredLogs.length === 0 ? (
                    <div className="flex h-full items-center justify-center p-8">
                      <div className="text-center text-muted-foreground">
                        <Terminal className="mx-auto h-8 w-8 mb-2 opacity-50" />
                        <p>No console logs yet</p>
                      </div>
                    </div>
                  ) : (
                    filteredLogs.map((log) => (
                      <ConsoleLogEntry
                        key={log.id}
                        log={log}
                        isExpanded={expandedLogs.has(log.id)}
                        onToggle={() => toggleLogExpanded(log.id)}
                      />
                    ))
                  )}
                  <div ref={logsEndRef} />
                </div>
              </ScrollArea>
            </TabsContent>

            <TabsContent value="devtools" className="flex-1 mt-0 overflow-hidden">
              <DevToolsPanel 
                sessions={sessions} 
                selectedSessionId={selectedSessionId}
                status={status}
              />
            </TabsContent>

            <TabsContent value="errors" className="flex-1 mt-0 overflow-hidden">
              <div className="flex h-full items-center justify-center">
                <div className="text-center text-muted-foreground">
                  <AlertCircle className="mx-auto h-8 w-8 mb-2 opacity-50" />
                  <p className="text-xs">Error tracking coming soon</p>
                </div>
              </div>
            </TabsContent>
          </Tabs>
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
        "w-full text-left px-2 py-1.5 rounded text-xs",
        isSelected ? "bg-accent text-accent-foreground" : "hover:bg-muted"
      )}
      onClick={onClick}
    >
      <div className="flex items-center gap-1.5">
        {session.is_connected ? (
          <span className="h-1.5 w-1.5 rounded-full bg-green-500" />
        ) : (
          <span className="h-1.5 w-1.5 rounded-full bg-zinc-400" />
        )}
        <span className="truncate flex-1">{session.title || hostname}</span>
      </div>
      <div className="text-[10px] text-muted-foreground truncate mt-0.5">
        {hostname}
      </div>
    </button>
  );
}

// Console log entry component
function ConsoleLogEntry({
  log,
  isExpanded,
  onToggle,
}: {
  log: ConsoleLog;
  isExpanded: boolean;
  onToggle: () => void;
}) {
  const [copied, setCopied] = useState(false);

  const handleCopy = () => {
    const text = formatLogArgs(log.args);
    navigator.clipboard.writeText(text);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  const hasExpandableContent = log.args.some(
    (arg) => arg.type === "object" || arg.type === "array" || arg.type === "error"
  );

  const bgColor = {
    log: "",
    info: "",
    warn: "bg-yellow-500/5",
    error: "bg-red-500/10",
    debug: "",
    trace: "",
    clear: "",
  }[log.level];

  return (
    <div
      className={cn(
        "group border-b border-border/50 px-4 py-1 hover:bg-muted/30",
        bgColor
      )}
    >
      <div className="flex items-start gap-2">
        {/* Expand toggle */}
        {hasExpandableContent ? (
          <button onClick={onToggle} className="mt-0.5 p-0.5 hover:bg-muted rounded">
            {isExpanded ? (
              <ChevronDown className="h-3 w-3" />
            ) : (
              <ChevronRight className="h-3 w-3" />
            )}
          </button>
        ) : (
          <div className="w-4" />
        )}

        {/* Level icon */}
        <LogLevelIcon level={log.level} className="mt-0.5 h-3.5 w-3.5 flex-shrink-0" />

        {/* Content */}
        <div className="flex-1 min-w-0">
          <div className="flex items-start gap-2">
            <div className="flex-1 min-w-0 break-words whitespace-pre-wrap">
              {isExpanded ? (
                <pre className="text-xs">{formatLogArgsExpanded(log.args)}</pre>
              ) : (
                <span>{formatLogArgs(log.args)}</span>
              )}
            </div>

            {/* Actions */}
            <div className="flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
              <Button
                variant="ghost"
                size="sm"
                className="h-5 w-5 p-0"
                onClick={handleCopy}
              >
                {copied ? (
                  <Check className="h-3 w-3 text-green-500" />
                ) : (
                  <Copy className="h-3 w-3" />
                )}
              </Button>
            </div>
          </div>

          {/* Source location */}
          {log.source_url && (
            <div className="text-[10px] text-muted-foreground mt-0.5 flex items-center gap-1">
              <span className="truncate max-w-xs">
                {log.source_url.split("/").pop()}
              </span>
              {log.line_number && (
                <span>
                  :{log.line_number}
                  {log.column_number && `:${log.column_number}`}
                </span>
              )}
            </div>
          )}

          {/* Stack trace */}
          {isExpanded && log.stack_trace && (
            <pre className="text-[10px] text-muted-foreground mt-1 whitespace-pre-wrap">
              {log.stack_trace}
            </pre>
          )}
        </div>

        {/* Timestamp */}
        <span className="text-[10px] text-muted-foreground flex-shrink-0">
          {new Date(log.timestamp).toLocaleTimeString()}
        </span>
      </div>
    </div>
  );
}

// Log level icon
function LogLevelIcon({
  level,
  className,
}: {
  level: ConsoleLevel;
  className?: string;
}) {
  switch (level) {
    case "warn":
      return <AlertTriangle className={cn("text-yellow-500", className)} />;
    case "error":
      return <AlertCircle className={cn("text-red-500", className)} />;
    case "info":
      return <Info className={cn("text-blue-500", className)} />;
    case "debug":
    case "trace":
      return <Bug className={cn("text-zinc-500", className)} />;
    default:
      return <ChevronRight className={cn("text-zinc-500", className)} />;
  }
}

// Format log args for display
function formatLogArgs(args: ConsoleLog["args"]): string {
  return args
    .map((arg) => {
      switch (arg.type) {
        case "string":
          return String(arg.value);
        case "number":
        case "boolean":
          return String(arg.value);
        case "null":
          return "null";
        case "undefined":
          return "undefined";
        case "function":
          return `[Function: ${arg.value}]`;
        case "symbol":
          return String(arg.value);
        case "circular":
          return "[Circular]";
        case "truncated":
          return `${arg.value}...`;
        case "error": {
          const err = arg.value as { name: string; message: string };
          return `${err.name}: ${err.message}`;
        }
        case "element": {
          const el = arg.value as { tag: string; id?: string; classes: string[] };
          let str = `<${el.tag}`;
          if (el.id) str += `#${el.id}`;
          if (el.classes.length) str += `.${el.classes.join(".")}`;
          return str + ">";
        }
        case "object":
        case "array":
          return JSON.stringify(arg.value, null, 0).slice(0, 100);
        default:
          return String(arg.value);
      }
    })
    .join(" ");
}

// Format log args expanded
function formatLogArgsExpanded(args: ConsoleLog["args"]): string {
  return args
    .map((arg) => {
      switch (arg.type) {
        case "object":
        case "array":
          return JSON.stringify(arg.value, null, 2);
        case "error": {
          const err = arg.value as { name: string; message: string; stack?: string };
          return `${err.name}: ${err.message}${err.stack ? "\n" + err.stack : ""}`;
        }
        default:
          return formatLogArgs([arg]);
      }
    })
    .join("\n");
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
      <div className="flex h-full items-center justify-center p-8">
        <div className="text-center text-muted-foreground">
          <Bug className="mx-auto h-12 w-12 mb-4 opacity-50" />
          <h3 className="font-medium mb-2">Debug Server Not Running</h3>
          <p className="text-sm max-w-md">
            Start the debug server to enable Chrome DevTools Protocol support.
            Pages with <code className="px-1 py-0.5 bg-muted rounded">debug://</code> rules 
            will connect automatically.
          </p>
        </div>
      </div>
    );
  }

  if (cdpSessions.length === 0) {
    return (
      <div className="flex h-full items-center justify-center p-8">
        <div className="text-center text-muted-foreground">
          <Bug className="mx-auto h-12 w-12 mb-4 opacity-50" />
          <h3 className="font-medium mb-2">No CDP-Enabled Pages</h3>
          <p className="text-sm max-w-md mb-4">
            Add a <code className="px-1 py-0.5 bg-muted rounded">debug://name</code> rule 
            to inject the Chobitsu CDP debugger into target pages.
          </p>
          <div className="text-xs text-left bg-muted p-3 rounded max-w-sm mx-auto font-mono">
            <p className="text-muted-foreground mb-1"># Example rule:</p>
            <p>example.com debug://mypage</p>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="h-full p-4 overflow-auto">
      <div className="max-w-2xl mx-auto">
        <h3 className="font-medium mb-4">Chrome DevTools Connections</h3>
        
        <p className="text-sm text-muted-foreground mb-4">
          Connect to these pages using Chrome DevTools. Copy the URL and paste it 
          into Chrome's address bar, or use the WebSocket URL with a custom DevTools frontend.
        </p>

        <div className="space-y-3">
          {cdpSessions.map((session) => {
            const devToolsUrl = getDevToolsUrl(session);
            const hostname = new URL(session.url).hostname;
            
            return (
              <div 
                key={session.id}
                className={cn(
                  "border rounded-lg p-4",
                  selectedSessionId === session.id && "border-primary"
                )}
              >
                <div className="flex items-start justify-between gap-4">
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-2 mb-1">
                      <span className="h-2 w-2 rounded-full bg-green-500" />
                      <span className="font-medium truncate">
                        {session.title || hostname}
                      </span>
                    </div>
                    <p className="text-xs text-muted-foreground truncate mb-2">
                      {session.url}
                    </p>
                  </div>
                </div>

                <div className="space-y-2 mt-3">
                  {/* DevTools URL */}
                  <div>
                    <label className="text-[10px] text-muted-foreground uppercase tracking-wider">
                      DevTools URL (paste in Chrome)
                    </label>
                    <div className="flex items-center gap-2 mt-1">
                      <code className="flex-1 text-xs bg-muted px-2 py-1.5 rounded truncate">
                        {devToolsUrl}
                      </code>
                      <Button
                        variant="outline"
                        size="sm"
                        className="h-7 px-2"
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
                    <label className="text-[10px] text-muted-foreground uppercase tracking-wider">
                      WebSocket URL (for custom clients)
                    </label>
                    <div className="flex items-center gap-2 mt-1">
                      <code className="flex-1 text-xs bg-muted px-2 py-1.5 rounded truncate">
                        {session.webSocketDebuggerUrl}
                      </code>
                      <Button
                        variant="outline"
                        size="sm"
                        className="h-7 px-2"
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

        <div className="mt-6 p-4 bg-muted/50 rounded-lg">
          <h4 className="font-medium text-sm mb-2">How to Connect</h4>
          <ol className="text-xs text-muted-foreground space-y-1 list-decimal list-inside">
            <li>Copy the DevTools URL above</li>
            <li>Open Chrome and paste the URL in the address bar</li>
            <li>Chrome DevTools will open and connect to the page</li>
          </ol>
          <p className="text-xs text-muted-foreground mt-3">
            <strong>Note:</strong> The page must have the PostGate debug script injected 
            (via <code>debug://</code> rule) and Chobitsu loaded for full DevTools support.
          </p>
        </div>
      </div>
    </div>
  );
}
