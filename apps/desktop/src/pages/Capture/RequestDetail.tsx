import { useState, useEffect } from "react";
import { useNavigate } from "react-router-dom";
import { CapturedRequest } from "@/stores/capture";
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
import { X, Copy, Send, FileCode, Download, Terminal, Code } from "lucide-react";
import { useCaptureStore } from "@/stores/capture";
import { useReplayStore } from "@/stores/replay";
import { useRequestBody } from "@/hooks/useProxy";
import { BodyPreview } from "@/components/capture/BodyPreview";
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

export function RequestDetail({ request }: RequestDetailProps) {
  const navigate = useNavigate();
  const setSelected = useCaptureStore((state) => state.setSelected);
  const importFromCapture = useReplayStore((state) => state.importFromCapture);
  const { getRequestBody, getResponseBody } = useRequestBody(request.id);
  
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

  const formatHeaders = (headers: Record<string, string>) => {
    return Object.entries(headers).map(([key, value]) => (
      <div key={key} className="flex gap-1 py-0.5 text-xs border-b border-border/30 last:border-0">
        <span className="font-medium text-muted-foreground min-w-[120px] shrink-0">{key}:</span>
        <span className="break-all">{value}</span>
      </div>
    ));
  };

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
      <Tabs defaultValue="overview" className="flex-1 flex flex-col overflow-hidden">
        <TabsList className="mx-3 mt-1.5 w-fit h-8">
          <TabsTrigger value="overview">Overview</TabsTrigger>
          <TabsTrigger value="request">Request</TabsTrigger>
          <TabsTrigger value="response">Response</TabsTrigger>
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
                    <FileCode className="h-3 w-3" />
                    Matched Rules
                  </h3>
                  <div className="flex flex-wrap gap-1">
                    {request.matchedRules.map((rule, i) => (
                      <Badge key={i} variant="outline" className="text-xs py-0">
                        {rule}
                      </Badge>
                    ))}
                  </div>
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
                    formatHeaders(request.requestHeaders)
                  ) : (
                    <span className="text-muted-foreground italic text-xs">No headers</span>
                  )}
                </div>
              </section>

              <section>
                <h3 className="font-semibold mb-1.5 text-xs uppercase text-muted-foreground">Body</h3>
                <div className="rounded border overflow-hidden">
                  <BodyPreview
                    body={requestBody}
                    contentType={request.requestHeaders["content-type"]}
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
                      formatHeaders(request.responseHeaders)
                    ) : (
                      <span className="text-muted-foreground italic text-xs">No headers</span>
                    )
                  ) : (
                    <span className="text-muted-foreground italic text-xs">No response yet</span>
                  )}
                </div>
              </section>

              <section>
                <h3 className="font-semibold mb-1.5 text-xs uppercase text-muted-foreground">Body</h3>
                <div className="rounded border overflow-hidden">
                  <BodyPreview
                    body={responseBody}
                    contentType={request.responseHeaders?.["content-type"] ?? request.contentType}
                    loading={loadingResponse}
                    className="min-h-[150px]"
                  />
                </div>
              </section>
            </div>
          </ScrollArea>
        </TabsContent>

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
