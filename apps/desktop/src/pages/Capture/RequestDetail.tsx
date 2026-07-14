import { useState, useEffect, useMemo } from "react";
import { useNavigate } from "react-router-dom";
import { CapturedRequest } from "@/stores/capture";
import { useStreamConnection, formatMessageType } from "@/stores/stream";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { X, Copy, Send, FileCode, Download, Terminal, Code, Radio, ArrowDown, ArrowUp, Zap } from "lucide-react";
import { useCaptureStore } from "@/stores/capture";
import { useReplayStore } from "@/stores/replay";
import { useRequestBody } from "@/hooks/useProxy";
import { BodyPreview } from "@/components/capture/BodyPreview";
import { MatchedRulesDisplay } from "@/components/capture/MatchedRulesDisplay";
import { CookieDisplay } from "@/components/capture/CookieDisplay";
import { SimpleTimingDisplay } from "@/components/capture/TimingWaterfall";
import { copyAsCurl, requestToFetch, exportToHar } from "@/lib/export";
import {
  cn,
  getStatusClass,
  getMethodClass,
  formatDuration,
  formatBytes,
} from "@/lib/utils";

interface RequestDetailProps {
  request: CapturedRequest;
}

function getHeaderValue(headers: Record<string, string> | null, name: string): string | null {
  if (!headers) return null;
  const entry = Object.entries(headers).find(([key]) => key.toLowerCase() === name.toLowerCase());
  return entry?.[1] ?? null;
}

export function RequestDetail({ request }: RequestDetailProps) {
  const navigate = useNavigate();
  const setSelected = useCaptureStore((state) => state.setSelected);
  const importFromCapture = useReplayStore((state) => state.importFromCapture);
  const { getRequestBody, getResponseBody } = useRequestBody(request.id);
  
  // Stream connection for SSE/WebSocket requests
  const streamConnection = useStreamConnection(request.id);
  const isStreamRequest = request.protocol === "websocket" || request.protocol === "sse";
  
  const [requestBody, setRequestBody] = useState<Uint8Array | null>(null);
  const [responseBody, setResponseBody] = useState<Uint8Array | null>(null);
  const [loadingRequest, setLoadingRequest] = useState(false);
  const [loadingResponse, setLoadingResponse] = useState(false);
  const [copied, setCopied] = useState<string | null>(null);

  // Load bodies when request changes
  useEffect(() => {
    let mounted = true;

    const loadBodies = async () => {
      setLoadingRequest(true);
      setLoadingResponse(true);

      try {
        const [reqBody, resBody] = await Promise.all([
          getRequestBody(),
          getResponseBody(),
        ]);

        if (mounted) {
          setRequestBody(reqBody);
          setResponseBody(resBody);
        }
      } finally {
        if (mounted) {
          setLoadingRequest(false);
          setLoadingResponse(false);
        }
      }
    };

    loadBodies();

    return () => {
      mounted = false;
    };
  }, [request.id, getRequestBody, getResponseBody]);

  const showCopied = (type: string) => {
    setCopied(type);
    setTimeout(() => setCopied(null), 2000);
  };

  const copyUrl = () => {
    navigator.clipboard.writeText(request.url);
    showCopied('url');
  };

  const handleCopyAsCurl = async () => {
    await copyAsCurl(request, requestBody || undefined);
    showCopied('curl');
  };

  const handleCopyAsFetch = async () => {
    const code = requestToFetch(request, requestBody || undefined);
    await navigator.clipboard.writeText(code);
    showCopied('fetch');
  };

  const handleExportHar = () => {
    const bodies = new Map<string, Uint8Array>();
    if (requestBody) bodies.set(request.id, requestBody);
    
    const resBodies = new Map<string, Uint8Array>();
    if (responseBody) resBodies.set(request.id, responseBody);
    
    exportToHar([request], bodies, resBodies);
  };

  // Format header name like Chrome DevTools: "content-type" → "Content-Type"
  const formatHeaderName = (name: string): string => {
    return name
      .split("-")
      .map((part) => part.charAt(0).toUpperCase() + part.slice(1).toLowerCase())
      .join("-");
  };

  const formatHeaders = (headers: Record<string, string>, options?: { skipCookies?: boolean }) => {
    return Object.entries(headers)
      .filter(([key]) => {
        if (options?.skipCookies) {
          const k = key.toLowerCase();
          return k !== "cookie" && k !== "set-cookie";
        }
        return true;
      })
      .map(([key, value]) => {
        // Determine header type for coloring
        const keyLower = key.toLowerCase();
        let valueClass = "break-all";

        // Color-code values based on header type
        if (keyLower === "content-type" || keyLower === "accept") {
          valueClass += " text-amber-600 dark:text-amber-400";
        } else if (keyLower === "authorization") {
          valueClass += " text-rose-600 dark:text-rose-400";
        } else if (keyLower.startsWith("x-") || keyLower.startsWith("sec-")) {
          valueClass += " text-purple-600 dark:text-purple-400";
        } else if (keyLower === "cache-control" || keyLower === "expires" || keyLower === "etag") {
          valueClass += " text-sky-600 dark:text-sky-400";
        } else if (keyLower === "location" || keyLower === "referer" || keyLower === "origin") {
          valueClass += " text-emerald-600 dark:text-emerald-400";
        }

        return (
          <div key={key} className="flex gap-2 py-0.5 text-xs border-b border-border/30 last:border-0 font-mono">
            <span className="font-semibold text-indigo-600 dark:text-indigo-400 min-w-[140px] shrink-0">{formatHeaderName(key)}:</span>
            <span className={valueClass}>{value}</span>
          </div>
        );
      });
  };

  // Extract cookie values from headers
  const requestCookie = useMemo(() => {
    const entry = Object.entries(request.requestHeaders).find(
      ([k]) => k.toLowerCase() === "cookie"
    );
    return entry?.[1] || null;
  }, [request.requestHeaders]);

  const responseCookies = useMemo(() => {
    if (!request.responseHeaders) return null;
    const entry = Object.entries(request.responseHeaders).find(
      ([k]) => k.toLowerCase() === "set-cookie"
    );
    return entry?.[1] || null;
  }, [request.responseHeaders]);

  return (
    <div className="flex h-full flex-col">
      {/* Header */}
      <div className="flex items-center justify-between border-b px-3 py-1.5">
        <div className="flex items-center gap-2 min-w-0">
          <span
            className={cn(
              "font-mono text-sm font-semibold",
              getMethodClass(request.method)
            )}
          >
            {request.method}
          </span>
          <span
            className={cn(
              "font-mono text-sm",
              getStatusClass(request.responseStatus ?? undefined)
            )}
          >
            {request.responseStatus ?? "Pending"}
          </span>
          <span className="text-sm text-muted-foreground truncate">
            {request.url}
          </span>
        </div>
        <div className="flex items-center gap-1 shrink-0">
          {/* Copy dropdown */}
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button variant="ghost" size="icon-sm" title="Copy options">
                <Copy className="h-4 w-4" />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end">
              <DropdownMenuItem onClick={copyUrl}>
                <Copy className="h-4 w-4 mr-2" />
                Copy URL
                {copied === 'url' && <span className="ml-auto text-xs text-emerald-500">Copied!</span>}
              </DropdownMenuItem>
              <DropdownMenuItem onClick={handleCopyAsCurl}>
                <Terminal className="h-4 w-4 mr-2" />
                Copy as cURL
                {copied === 'curl' && <span className="ml-auto text-xs text-emerald-500">Copied!</span>}
              </DropdownMenuItem>
              <DropdownMenuItem onClick={handleCopyAsFetch}>
                <Code className="h-4 w-4 mr-2" />
                Copy as fetch()
                {copied === 'fetch' && <span className="ml-auto text-xs text-emerald-500">Copied!</span>}
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>

          {/* Export dropdown */}
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button variant="ghost" size="icon-sm" title="Export options">
                <Download className="h-4 w-4" />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end">
              <DropdownMenuItem onClick={handleExportHar}>
                <FileCode className="h-4 w-4 mr-2" />
                Export as HAR
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>

          <Button
            variant="ghost"
            size="icon-sm"
            onClick={async () => {
              try {
                await importFromCapture({
                  id: request.id,
                  method: request.method,
                  url: request.url,
                  path: request.path,
                  request_headers: request.requestHeaders,
                });
                navigate('/replay');
              } catch (error) {
                console.error('Failed to send to replay:', error);
              }
            }}
            title="Send to Replay"
          >
            <Send className="h-4 w-4" />
          </Button>
          <Button
            variant="ghost"
            size="icon-sm"
            onClick={() => setSelected(null)}
            title="Close"
          >
            <X className="h-4 w-4" />
          </Button>
        </div>
      </div>

      {/* Tabs */}
      <Tabs defaultValue={isStreamRequest ? "stream" : "overview"} className="flex-1 flex flex-col overflow-hidden">
        <TabsList className="mx-3 mt-2 w-fit">
          <TabsTrigger value="overview">Overview</TabsTrigger>
          <TabsTrigger value="request">Request</TabsTrigger>
          {isStreamRequest ? (
            <TabsTrigger value="stream" className="flex items-center gap-1">
              <Radio className="h-3 w-3" />
              Stream
              {streamConnection && !streamConnection.isEnded && (
                <span className="w-1.5 h-1.5 rounded-full bg-emerald-500 animate-pulse" />
              )}
            </TabsTrigger>
          ) : (
            <TabsTrigger value="response">Response</TabsTrigger>
          )}
          <TabsTrigger value="timing">Timing</TabsTrigger>
        </TabsList>

        <TabsContent value="overview" className="flex-1 overflow-hidden mt-0 px-3">
          <ScrollArea className="h-full">
            <div className="space-y-3 py-3">
              {/* General Info */}
              <section>
                <h3 className="font-semibold mb-1.5 text-xs uppercase text-muted-foreground">General</h3>
                <div className="grid grid-cols-3 gap-2 text-xs bg-muted/30 p-2 rounded">
                  <div className="col-span-3">
                    <span className="text-muted-foreground">URL: </span>
                    <span className="break-all font-mono">{request.url}</span>
                  </div>
                  <div>
                    <span className="text-muted-foreground">Protocol: </span>
                    <span className="uppercase">{request.protocol}</span>
                  </div>
                  <div>
                    <span className="text-muted-foreground">Duration: </span>
                    <span>{request.durationMs !== null ? formatDuration(request.durationMs) : "-"}</span>
                  </div>
                  <div>
                    <span className="text-muted-foreground">Server IP: </span>
                    <span>{request.remoteAddr || "-"}</span>
                  </div>
                  <div>
                    <span className="text-muted-foreground">Request: </span>
                    <span>{formatBytes(request.requestSize)}</span>
                  </div>
                  <div>
                    <span className="text-muted-foreground">Response: </span>
                    <span>{request.responseSize !== null ? formatBytes(request.responseSize) : "-"}</span>
                  </div>
                  <div>
                    <span className="text-muted-foreground">Type: </span>
                    <span className="truncate">{request.contentType || "-"}</span>
                  </div>
                </div>
              </section>

              {/* TLS Info */}
              {request.tlsInfo && (
                <section>
                  <h3 className="font-semibold mb-1.5 text-xs uppercase text-muted-foreground">TLS</h3>
                  <div className="grid grid-cols-2 gap-2 text-xs bg-muted/30 p-2 rounded">
                    <div>
                      <span className="text-muted-foreground">Version: </span>
                      <span>{request.tlsInfo.version}</span>
                    </div>
                    <div>
                      <span className="text-muted-foreground">Cipher: </span>
                      <span>{request.tlsInfo.cipher || "-"}</span>
                    </div>
                  </div>
                </section>
              )}

              {/* Matched Rules */}
              {request.matchedRules.length > 0 && (
                <section>
                  <h3 className="font-semibold mb-1.5 text-xs uppercase text-muted-foreground flex items-center gap-1">
                    <Zap className="h-3 w-3 text-indigo-500" />
                    Matched Rules ({request.matchedRules.length})
                  </h3>
                  <MatchedRulesDisplay rules={request.matchedRules} />
                </section>
              )}
            </div>
          </ScrollArea>
        </TabsContent>

        <TabsContent value="request" className="flex-1 overflow-hidden mt-0 px-3">
          <ScrollArea className="h-full">
            <div className="space-y-3 py-3">
              <section>
                <h3 className="font-semibold mb-1.5 text-xs uppercase text-muted-foreground">
                  Headers ({Object.keys(request.requestHeaders).length})
                </h3>
                <div className="rounded border p-2 bg-muted/30">
                  {Object.keys(request.requestHeaders).length > 0 ? (
                    formatHeaders(request.requestHeaders, { skipCookies: true })
                  ) : (
                    <span className="text-muted-foreground italic text-xs">No headers</span>
                  )}
                </div>
              </section>

              {/* Request Cookies */}
              {requestCookie && (
                <section>
                  <h3 className="font-semibold mb-1.5 text-xs uppercase text-muted-foreground">
                    Cookies
                  </h3>
                  <CookieDisplay cookies={requestCookie} type="cookie" />
                </section>
              )}

              <section>
                <h3 className="font-semibold mb-1.5 text-xs uppercase text-muted-foreground">Body</h3>
                <div className="rounded border overflow-hidden">
                  <BodyPreview
                    body={requestBody}
                    contentType={getHeaderValue(request.requestHeaders, "content-type")}
                    loading={loadingRequest}
                    className="min-h-[150px]"
                  />
                </div>
              </section>
            </div>
          </ScrollArea>
        </TabsContent>

        <TabsContent value="response" className="flex-1 overflow-hidden mt-0 px-3">
          <ScrollArea className="h-full">
            <div className="space-y-3 py-3">
              <section>
                <h3 className="font-semibold mb-1.5 text-xs uppercase text-muted-foreground">
                  Headers ({request.responseHeaders ? Object.keys(request.responseHeaders).length : 0})
                </h3>
                <div className="rounded border p-2 bg-muted/30">
                  {request.responseHeaders ? (
                    Object.keys(request.responseHeaders).length > 0 ? (
                      formatHeaders(request.responseHeaders, { skipCookies: true })
                    ) : (
                      <span className="text-muted-foreground italic text-xs">No headers</span>
                    )
                  ) : (
                    <span className="text-muted-foreground italic text-xs">No response yet</span>
                  )}
                </div>
              </section>

              {/* Response Set-Cookies */}
              {responseCookies && (
                <section>
                  <h3 className="font-semibold mb-1.5 text-xs uppercase text-muted-foreground">
                    Set-Cookie
                  </h3>
                  <CookieDisplay cookies={responseCookies} type="set-cookie" />
                </section>
              )}

              <section>
                <h3 className="font-semibold mb-1.5 text-xs uppercase text-muted-foreground">Body</h3>
                <div className="rounded border overflow-hidden">
                  <BodyPreview
                    body={responseBody}
                    contentType={getHeaderValue(request.responseHeaders, "content-type") ?? request.contentType}
                    loading={loadingResponse}
                    className="min-h-[150px]"
                  />
                </div>
              </section>
            </div>
          </ScrollArea>
        </TabsContent>

        {/* Stream Tab - for SSE/WebSocket */}
        {isStreamRequest && (
          <TabsContent value="stream" className="flex-1 overflow-hidden mt-0 px-3">
            <div className="h-full flex flex-col">
              {/* Stream Stats */}
              {streamConnection && (
                <div className="flex items-center gap-4 text-xs py-2 border-b mb-2">
                  <div className="flex items-center gap-1">
                    <span className="text-muted-foreground">Messages:</span>
                    <span className="font-medium">{streamConnection.messageCount}</span>
                  </div>
                  <div className="flex items-center gap-1">
                    <span className="text-muted-foreground">Bytes:</span>
                    <span className="font-medium">{formatBytes(streamConnection.totalBytes)}</span>
                  </div>
                  {streamConnection.durationMs !== null && (
                    <div className="flex items-center gap-1">
                      <span className="text-muted-foreground">Duration:</span>
                      <span className="font-medium">{formatDuration(streamConnection.durationMs)}</span>
                    </div>
                  )}
                  <div className="flex items-center gap-1">
                    <span className={cn(
                      "w-2 h-2 rounded-full",
                      streamConnection.isEnded ? "bg-gray-400" : "bg-emerald-500 animate-pulse"
                    )} />
                    <span className="text-muted-foreground">
                      {streamConnection.isEnded ? "Ended" : "Live"}
                    </span>
                  </div>
                  {streamConnection.closeReason && (
                    <div className="flex items-center gap-1 text-muted-foreground">
                      <span>Reason: {streamConnection.closeReason}</span>
                    </div>
                  )}
                </div>
              )}

              {/* Message List */}
              <ScrollArea className="flex-1">
                <div className="space-y-1 py-2">
                  {streamConnection && streamConnection.messages.length > 0 ? (
                    streamConnection.messages.map((msg) => (
                      <div
                        key={msg.id}
                        className={cn(
                          "flex items-start gap-2 p-2 rounded text-xs font-mono",
                          msg.direction === "inbound"
                            ? "bg-blue-50 dark:bg-blue-950/30"
                            : "bg-green-50 dark:bg-green-950/30"
                        )}
                      >
                        {/* Direction indicator */}
                        <div className={cn(
                          "shrink-0 p-1 rounded",
                          msg.direction === "inbound"
                            ? "text-blue-600 dark:text-blue-400"
                            : "text-green-600 dark:text-green-400"
                        )}>
                          {msg.direction === "inbound" ? (
                            <ArrowDown className="h-3 w-3" />
                          ) : (
                            <ArrowUp className="h-3 w-3" />
                          )}
                        </div>
                        
                        {/* Message content */}
                        <div className="flex-1 min-w-0">
                          <div className="flex items-center gap-2 mb-1">
                            <Badge variant="outline" className="text-[10px] py-0 h-4">
                              {formatMessageType(msg.messageType)}
                            </Badge>
                            <span className="text-muted-foreground text-[10px]">
                              {new Date(msg.timestamp).toLocaleTimeString()}
                            </span>
                            <span className="text-muted-foreground text-[10px]">
                              {formatBytes(msg.size)}
                            </span>
                          </div>
                          <pre className="whitespace-pre-wrap break-all text-[11px] leading-relaxed">
                            {msg.isBase64 ? (
                              <span className="text-muted-foreground italic">
                                [Binary data - {formatBytes(msg.size)}]
                              </span>
                            ) : (
                              msg.data
                            )}
                          </pre>
                        </div>
                      </div>
                    ))
                  ) : (
                    <div className="text-center text-muted-foreground py-8">
                      {isStreamRequest ? (
                        <>
                          <Radio className="h-8 w-8 mx-auto mb-2 opacity-50" />
                          <p>Waiting for stream messages...</p>
                          <p className="text-xs mt-1">
                            {request.protocol === "websocket"
                              ? "WebSocket frames will appear here"
                              : "SSE events will appear here"}
                          </p>
                        </>
                      ) : (
                        <p>No stream data for this request</p>
                      )}
                    </div>
                  )}
                </div>
              </ScrollArea>
            </div>
          </TabsContent>
        )}

        <TabsContent value="timing" className="flex-1 overflow-hidden mt-0 px-3">
          <ScrollArea className="h-full">
            <div className="py-3">
              {request.durationMs !== null ? (
                <SimpleTimingDisplay durationMs={request.durationMs} />
              ) : (
                <p className="text-muted-foreground text-sm">
                  Timing information not available yet.
                </p>
              )}
            </div>
          </ScrollArea>
        </TabsContent>
      </Tabs>
    </div>
  );
}
