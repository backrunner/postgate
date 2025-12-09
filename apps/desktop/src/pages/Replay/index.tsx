import { useEffect, useState } from "react";
import { 
  Send, 
  FolderPlus, 
  FilePlus, 
  ChevronRight, 
  ChevronDown,
  Folder,
  MoreHorizontal,
  Trash2,
  Copy,
  Loader2,
  Play,
  Save,
  X,
  Plus,
  Search,
  ArrowRight,
  Clock,
  Database
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Badge } from "@/components/ui/badge";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Separator } from "@/components/ui/separator";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
  DropdownMenuSeparator,
} from "@/components/ui/dropdown-menu";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  useReplayStore,
  SavedRequest,
  CollectionNode,
  KeyValuePair,
  RequestBody,
} from "@/stores/replay";
import { cn, getMethodClass, getStatusClass, formatDuration, formatBytes } from "@/lib/utils";

export function ReplayPage() {
  const {
    tree,
    selectedRequest,
    currentRequest,
    response,
    isLoading,
    isExecuting,
    error,
    fetchTree,
    createCollection,
    deleteCollection,
    selectRequest,
    createRequest,
    updateRequest,
    deleteRequest,
    duplicateRequest,
    updateCurrentRequest,
    executeRequest,
  } = useReplayStore();

  const [newCollectionName, setNewCollectionName] = useState("");
  const [showNewCollection, setShowNewCollection] = useState(false);
  const [expandedCollections, setExpandedCollections] = useState<Set<string>>(new Set());
  const [filter, setFilter] = useState("");

  useEffect(() => {
    fetchTree();
  }, [fetchTree]);

  const toggleCollection = (id: string) => {
    const next = new Set(expandedCollections);
    if (next.has(id)) {
      next.delete(id);
    } else {
      next.add(id);
    }
    setExpandedCollections(next);
  };

  const handleCreateCollection = async () => {
    if (!newCollectionName.trim()) return;
    await createCollection(newCollectionName.trim());
    setNewCollectionName("");
    setShowNewCollection(false);
  };

  const handleNewRequest = async (collectionId?: string) => {
    await createRequest({
      name: "New Request",
      method: "GET",
      url: "https://",
    }, collectionId || undefined);
  };

  const handleSave = async () => {
    if (currentRequest.id) {
      await updateRequest(currentRequest);
    } else {
      await createRequest(currentRequest);
    }
  };

  return (
    <div className="flex h-full bg-background">
      {/* Sidebar - Collections */}
      <div className="w-60 border-r flex flex-col bg-muted/10">
        <div className="flex h-12 items-center justify-between border-b px-3 bg-muted/10">
          <h2 className="text-xs font-semibold text-muted-foreground uppercase tracking-wider">Collections</h2>
          <div className="flex items-center gap-0.5">
            <Button 
              variant="ghost" 
              size="icon" 
              className="h-7 w-7"
              title="New Collection"
              onClick={() => setShowNewCollection(true)}
            >
              <FolderPlus className="h-3.5 w-3.5" />
            </Button>
            <Button 
              variant="ghost" 
              size="icon" 
              className="h-7 w-7"
              title="New Request"
              onClick={() => handleNewRequest()}
            >
              <FilePlus className="h-3.5 w-3.5" />
            </Button>
          </div>
        </div>

        {/* Search & Filter */}
        <div className="p-2 border-b">
          <div className="relative">
            <Search className="absolute left-2 top-1/2 h-3 w-3 -translate-y-1/2 text-muted-foreground" />
            <Input 
              placeholder="Filter..." 
              value={filter}
              onChange={(e) => setFilter(e.target.value)}
              className="h-7 pl-7 text-xs bg-background" 
            />
          </div>
        </div>

        {/* New Collection Input */}
        {showNewCollection && (
          <div className="p-2 border-b bg-muted/20 animate-in slide-in-from-top-2 duration-200">
            <div className="flex items-center gap-1.5">
              <Folder className="h-3.5 w-3.5 text-blue-500" />
              <Input
                value={newCollectionName}
                onChange={(e) => setNewCollectionName(e.target.value)}
                placeholder="Collection Name"
                className="h-7 text-xs flex-1"
                onKeyDown={(e) => e.key === "Enter" && handleCreateCollection()}
                autoFocus
              />
              <Button size="icon" variant="ghost" className="h-7 w-7" onClick={handleCreateCollection}>
                <Plus className="h-3.5 w-3.5" />
              </Button>
              <Button size="icon" variant="ghost" className="h-7 w-7" onClick={() => setShowNewCollection(false)}>
                <X className="h-3.5 w-3.5" />
              </Button>
            </div>
          </div>
        )}

        <ScrollArea className="flex-1">
          {isLoading ? (
            <div className="flex items-center justify-center p-8">
              <Loader2 className="h-4 w-4 animate-spin text-muted-foreground" />
            </div>
          ) : tree ? (
            <div className="p-2 space-y-0.5">
              {/* Root requests */}
              {tree.root_requests.map((request) => (
                <RequestItem
                  key={request.id}
                  request={request}
                  isSelected={selectedRequest?.id === request.id}
                  onSelect={() => selectRequest(request)}
                  onDelete={() => deleteRequest(request.id)}
                  onDuplicate={() => duplicateRequest(request.id)}
                />
              ))}
              
              {/* Collections */}
              {tree.collections.map((node) => (
                <CollectionItem
                  key={node.collection.id}
                  node={node}
                  selectedId={selectedRequest?.id}
                  expanded={expandedCollections}
                  onToggle={toggleCollection}
                  onSelect={selectRequest}
                  onDelete={deleteRequest}
                  onDuplicate={duplicateRequest}
                  onDeleteCollection={deleteCollection}
                  onNewRequest={handleNewRequest}
                />
              ))}

              {tree.root_requests.length === 0 && tree.collections.length === 0 && (
                <div className="flex flex-col items-center justify-center p-8 text-center text-muted-foreground">
                  <Database className="h-8 w-8 mb-2 opacity-20" />
                  <p className="text-xs">No collections yet</p>
                </div>
              )}
            </div>
          ) : null}
        </ScrollArea>
      </div>

      {/* Main Content */}
      <div className="flex-1 flex flex-col min-w-0">
        {currentRequest.url ? (
          <>
            {/* URL Bar */}
            <div className="h-12 border-b flex items-center px-4 gap-3 bg-background">
              <div className="flex-1 flex items-center gap-2">
                <Select
                  value={currentRequest.method}
                  onValueChange={(value) => updateCurrentRequest({ method: value })}
                >
                  <SelectTrigger
                    className={cn(
                      "h-8 w-[100px] px-3 text-xs font-mono font-bold",
                      getMethodClass(currentRequest.method)
                    )}
                  >
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"].map((m) => (
                      <SelectItem key={m} value={m} className={cn("text-xs font-mono font-bold", getMethodClass(m))}>
                        {m}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
                
                <div className="flex-1 relative">
                  <Input
                    value={currentRequest.url}
                    onChange={(e) => updateCurrentRequest({ url: e.target.value })}
                    placeholder="https://api.example.com/endpoint"
                    className="h-8 font-mono text-xs border-muted-foreground/20 focus-visible:ring-primary/20"
                  />
                </div>
              </div>

              <div className="flex items-center gap-2">
                <Button 
                  onClick={executeRequest} 
                  disabled={isExecuting}
                  className="h-8 px-4 text-xs font-medium gap-1.5"
                >
                  {isExecuting ? (
                    <Loader2 className="h-3.5 w-3.5 animate-spin" />
                  ) : (
                    <Play className="h-3.5 w-3.5 fill-current" />
                  )}
                  Send
                </Button>
                <Separator orientation="vertical" className="h-6" />
                <Button 
                  variant="outline"
                  size="sm" 
                  onClick={handleSave}
                  className="h-8 px-3 text-xs gap-1.5"
                >
                  <Save className="h-3.5 w-3.5" />
                  Save
                </Button>
              </div>
            </div>

            {/* Request/Response Split */}
            <div className="flex-1 flex overflow-hidden">
              {/* Request Panel */}
              <div className="flex-1 border-r overflow-hidden flex flex-col bg-background">
                <Tabs defaultValue="params" className="flex-1 flex flex-col">
                  <div className="border-b px-4 bg-muted/5">
                    <TabsList className="h-9 p-0 bg-transparent gap-4">
                      <TabsTrigger 
                        value="params" 
                        className="h-full rounded-none border-b-2 border-transparent px-2 pb-2 pt-1.5 text-xs data-[state=active]:border-primary data-[state=active]:bg-transparent data-[state=active]:shadow-none font-medium text-muted-foreground data-[state=active]:text-foreground"
                      >
                        Params
                      </TabsTrigger>
                      <TabsTrigger 
                        value="headers"
                        className="h-full rounded-none border-b-2 border-transparent px-2 pb-2 pt-1.5 text-xs data-[state=active]:border-primary data-[state=active]:bg-transparent data-[state=active]:shadow-none font-medium text-muted-foreground data-[state=active]:text-foreground"
                      >
                        Headers
                      </TabsTrigger>
                      <TabsTrigger 
                        value="body"
                        className="h-full rounded-none border-b-2 border-transparent px-2 pb-2 pt-1.5 text-xs data-[state=active]:border-primary data-[state=active]:bg-transparent data-[state=active]:shadow-none font-medium text-muted-foreground data-[state=active]:text-foreground"
                      >
                        Body
                      </TabsTrigger>
                    </TabsList>
                  </div>

                  <TabsContent value="params" className="flex-1 overflow-auto m-0 p-4">
                    <KeyValueEditor
                      items={currentRequest.query_params}
                      onChange={(query_params) => updateCurrentRequest({ query_params })}
                      placeholder="Add query parameter"
                    />
                  </TabsContent>

                  <TabsContent value="headers" className="flex-1 overflow-auto m-0 p-4">
                    <KeyValueEditor
                      items={currentRequest.headers}
                      onChange={(headers) => updateCurrentRequest({ headers })}
                      placeholder="Add header"
                    />
                  </TabsContent>

                  <TabsContent value="body" className="flex-1 overflow-auto m-0 p-4">
                    <BodyEditor
                      body={currentRequest.body}
                      onChange={(body) => updateCurrentRequest({ body })}
                    />
                  </TabsContent>
                </Tabs>
              </div>

              {/* Response Panel */}
              <div className="flex-1 overflow-hidden flex flex-col bg-muted/5">
                <div className="h-10 border-b px-4 flex items-center justify-between bg-muted/10">
                  <span className="text-xs font-semibold text-muted-foreground uppercase tracking-wider">Response</span>
                  {response && (
                    <div className="flex items-center gap-3 text-xs">
                      <Badge variant="outline" className={cn("rounded-sm px-1.5 py-0.5", getStatusClass(response.status))}>
                        {response.status} {response.statusText}
                      </Badge>
                      <div className="flex items-center gap-1.5 text-muted-foreground">
                        <Clock className="h-3 w-3" />
                        <span>{formatDuration(response.durationMs)}</span>
                      </div>
                      <div className="flex items-center gap-1.5 text-muted-foreground">
                        <ArrowRight className="h-3 w-3" />
                        <span>{formatBytes(response.bodySize)}</span>
                      </div>
                    </div>
                  )}
                </div>

                <ScrollArea className="flex-1">
                  {error ? (
                    <div className="p-6">
                      <div className="rounded-md bg-destructive/10 p-4 border border-destructive/20">
                        <div className="flex items-center gap-2 text-destructive mb-2">
                          <X className="h-4 w-4" />
                          <h4 className="text-sm font-semibold">Request Failed</h4>
                        </div>
                        <p className="text-xs text-destructive/80 font-mono">{error}</p>
                      </div>
                    </div>
                  ) : response ? (
                    <Tabs defaultValue="body" className="flex-1">
                      <div className="border-b px-4 bg-background/50">
                        <TabsList className="h-8 p-0 bg-transparent gap-4">
                          <TabsTrigger 
                            value="body"
                            className="h-full rounded-none border-b-2 border-transparent px-2 text-xs data-[state=active]:border-primary data-[state=active]:bg-transparent data-[state=active]:shadow-none font-medium text-muted-foreground data-[state=active]:text-foreground"
                          >
                            Body
                          </TabsTrigger>
                          <TabsTrigger 
                            value="headers"
                            className="h-full rounded-none border-b-2 border-transparent px-2 text-xs data-[state=active]:border-primary data-[state=active]:bg-transparent data-[state=active]:shadow-none font-medium text-muted-foreground data-[state=active]:text-foreground"
                          >
                            Headers
                          </TabsTrigger>
                        </TabsList>
                      </div>

                      <TabsContent value="body" className="m-0 p-0 relative group">
                         <div className="absolute top-2 right-2 opacity-0 group-hover:opacity-100 transition-opacity z-10">
                          <Button 
                            variant="secondary" 
                            size="sm" 
                            className="h-6 text-[10px] gap-1 shadow-sm"
                            onClick={() => navigator.clipboard.writeText(response.body ?? "")}
                          >
                            <Copy className="h-3 w-3" />
                            Copy
                          </Button>
                        </div>
                        <pre className="text-[11px] font-mono p-4 overflow-auto min-h-[200px] bg-background/50">
                          {response.body || <span className="text-muted-foreground italic">(empty response)</span>}
                        </pre>
                      </TabsContent>

                      <TabsContent value="headers" className="m-0 p-4">
                        <div className="grid gap-2">
                          {Object.entries(response.headers).map(([key, value]) => (
                            <div key={key} className="flex gap-3 text-xs border-b border-border/40 pb-1.5 last:border-0">
                              <span className="font-semibold text-muted-foreground min-w-[120px] shrink-0">{key}</span>
                              <span className="break-all font-mono text-foreground/80">{value}</span>
                            </div>
                          ))}
                        </div>
                      </TabsContent>
                    </Tabs>
                  ) : (
                    <div className="flex flex-col items-center justify-center min-h-[300px] h-[calc(100vh-200px)] text-muted-foreground p-8">
                      <div className="h-16 w-16 rounded-full bg-muted/30 flex items-center justify-center mb-4">
                        <Send className="h-8 w-8 opacity-20" />
                      </div>
                      <p className="text-sm font-medium">Ready to send request</p>
                      <p className="text-xs text-muted-foreground mt-1">Enter URL and method to start</p>
                    </div>
                  )}
                </ScrollArea>
              </div>
            </div>
          </>
        ) : (
          <div className="flex-1 flex flex-col items-center justify-center bg-muted/5">
            <div className="text-center max-w-md p-8">
              <div className="mx-auto h-16 w-16 bg-primary/5 rounded-2xl flex items-center justify-center mb-6 ring-1 ring-primary/10">
                <Send className="h-8 w-8 text-primary/60" />
              </div>
              <h3 className="text-lg font-semibold mb-2">Request Replay</h3>
              <p className="text-sm text-muted-foreground mb-8 leading-relaxed">
                Test APIs directly from PostGate. Create collections, save requests, 
                and inspect responses with a powerful HTTP client.
              </p>
              <div className="flex items-center justify-center gap-3">
                <Button onClick={() => handleNewRequest()} className="gap-2">
                  <FilePlus className="h-4 w-4" />
                  Create Request
                </Button>
                <Button variant="outline" onClick={() => setShowNewCollection(true)} className="gap-2">
                  <FolderPlus className="h-4 w-4" />
                  New Collection
                </Button>
              </div>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

// Collection Tree Item
interface CollectionItemProps {
  node: CollectionNode;
  selectedId?: string;
  expanded: Set<string>;
  onToggle: (id: string) => void;
  onSelect: (request: SavedRequest) => void;
  onDelete: (id: string) => void;
  onDuplicate: (id: string) => void;
  onDeleteCollection: (id: string) => void;
  onNewRequest: (collectionId: string) => void;
  depth?: number;
}

function CollectionItem({
  node,
  selectedId,
  expanded,
  onToggle,
  onSelect,
  onDelete,
  onDuplicate,
  onDeleteCollection,
  onNewRequest,
  depth = 0,
}: CollectionItemProps) {
  const isExpanded = expanded.has(node.collection.id);

  return (
    <div>
      <div 
        className="flex items-center gap-1.5 py-1.5 px-2 rounded-md hover:bg-muted/60 cursor-pointer group transition-colors select-none"
        style={{ paddingLeft: depth * 12 + 8 }}
      >
        <button 
          onClick={(e) => { e.stopPropagation(); onToggle(node.collection.id); }}
          className="p-0.5 hover:bg-muted rounded-sm transition-colors text-muted-foreground"
        >
          {isExpanded ? <ChevronDown className="h-3 w-3" /> : <ChevronRight className="h-3 w-3" />}
        </button>
        <Folder className={cn("h-3.5 w-3.5 fill-blue-500/20 text-blue-500")} />
        <span className="text-xs font-medium flex-1 truncate text-foreground/80">{node.collection.name}</span>
        
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button variant="ghost" size="icon" className="opacity-0 group-hover:opacity-100 h-5 w-5 hover:bg-muted">
              <MoreHorizontal className="h-3 w-3" />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end" className="w-40">
            <DropdownMenuItem onClick={() => onNewRequest(node.collection.id)}>
              <FilePlus className="h-3.5 w-3.5 mr-2" />
              New Request
            </DropdownMenuItem>
            <DropdownMenuSeparator />
            <DropdownMenuItem onClick={() => onDeleteCollection(node.collection.id)} className="text-destructive focus:text-destructive">
              <Trash2 className="h-3.5 w-3.5 mr-2" />
              Delete
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>

      {isExpanded && (
        <div>
          {node.requests.map((request) => (
            <RequestItem
              key={request.id}
              request={request}
              isSelected={selectedId === request.id}
              onSelect={() => onSelect(request)}
              onDelete={() => onDelete(request.id)}
              onDuplicate={() => onDuplicate(request.id)}
              depth={depth + 1}
            />
          ))}
          {node.children.map((child) => (
            <CollectionItem
              key={child.collection.id}
              node={child}
              selectedId={selectedId}
              expanded={expanded}
              onToggle={onToggle}
              onSelect={onSelect}
              onDelete={onDelete}
              onDuplicate={onDuplicate}
              onDeleteCollection={onDeleteCollection}
              onNewRequest={onNewRequest}
              depth={depth + 1}
            />
          ))}
        </div>
      )}
    </div>
  );
}

// Request Item
interface RequestItemProps {
  request: SavedRequest;
  isSelected: boolean;
  onSelect: () => void;
  onDelete: () => void;
  onDuplicate: () => void;
  depth?: number;
}

function RequestItem({ request, isSelected, onSelect, onDelete, onDuplicate, depth = 0 }: RequestItemProps) {
  return (
    <div
      className={cn(
        "flex items-center gap-2 py-1.5 px-2 rounded-md cursor-pointer group transition-colors select-none",
        isSelected ? "bg-primary/10 text-primary font-medium" : "hover:bg-muted/60 text-muted-foreground"
      )}
      style={{ paddingLeft: depth * 12 + 20 }}
      onClick={onSelect}
    >
      <span className={cn("text-[9px] font-mono font-bold uppercase w-8 shrink-0", 
        request.method === "GET" ? "text-blue-500" :
        request.method === "POST" ? "text-green-500" :
        request.method === "PUT" ? "text-orange-500" :
        request.method === "DELETE" ? "text-red-500" : "text-muted-foreground"
      )}>
        {request.method.substring(0, 4)}
      </span>
      <span className="text-xs flex-1 truncate">{request.name}</span>
      
      <DropdownMenu>
        <DropdownMenuTrigger asChild onClick={(e) => e.stopPropagation()}>
          <Button variant="ghost" size="icon" className="opacity-0 group-hover:opacity-100 h-5 w-5 hover:bg-background/50">
            <MoreHorizontal className="h-3 w-3" />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" className="w-40">
          <DropdownMenuItem onClick={(e) => { e.stopPropagation(); onDuplicate(); }}>
            <Copy className="h-3.5 w-3.5 mr-2" />
            Duplicate
          </DropdownMenuItem>
          <DropdownMenuSeparator />
          <DropdownMenuItem onClick={(e) => { e.stopPropagation(); onDelete(); }} className="text-destructive focus:text-destructive">
            <Trash2 className="h-3.5 w-3.5 mr-2" />
            Delete
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  );
}

// Key-Value Editor
interface KeyValueEditorProps {
  items: KeyValuePair[];
  onChange: (items: KeyValuePair[]) => void;
  placeholder?: string;
}

function KeyValueEditor({ items, onChange, placeholder }: KeyValueEditorProps) {
  const addItem = () => {
    onChange([...items, { key: "", value: "", enabled: true }]);
  };

  const updateItem = (index: number, updates: Partial<KeyValuePair>) => {
    const next = [...items];
    next[index] = { ...next[index], ...updates };
    onChange(next);
  };

  const removeItem = (index: number) => {
    onChange(items.filter((_, i) => i !== index));
  };

  return (
    <div className="space-y-2">
      <div className="grid grid-cols-[auto_1fr_1fr_auto] gap-2 items-center text-xs font-medium text-muted-foreground mb-2 px-1">
        <div className="w-4" />
        <div>Key</div>
        <div>Value</div>
        <div className="w-7" />
      </div>
      
      {items.map((item, index) => (
        <div key={index} className="grid grid-cols-[auto_1fr_1fr_auto] gap-2 items-center group">
          <input
            type="checkbox"
            checked={item.enabled}
            onChange={(e) => updateItem(index, { enabled: e.target.checked })}
            className="h-3.5 w-3.5 rounded border-muted-foreground/30 text-primary focus:ring-primary/20"
          />
          <Input
            value={item.key}
            onChange={(e) => updateItem(index, { key: e.target.value })}
            placeholder="Key"
            className={cn("h-8 text-xs font-mono", !item.enabled && "opacity-50 line-through")}
          />
          <Input
            value={item.value}
            onChange={(e) => updateItem(index, { value: e.target.value })}
            placeholder="Value"
            className={cn("h-8 text-xs font-mono", !item.enabled && "opacity-50 line-through")}
          />
          <Button 
            variant="ghost" 
            size="icon" 
            onClick={() => removeItem(index)}
            className="h-8 w-8 opacity-0 group-hover:opacity-100 transition-opacity hover:text-destructive"
          >
            <Trash2 className="h-3.5 w-3.5" />
          </Button>
        </div>
      ))}
      
      <Button 
        variant="outline" 
        size="sm" 
        onClick={addItem} 
        className="text-xs h-7 border-dashed border-border/60 hover:border-primary/50 text-muted-foreground hover:text-primary mt-2"
      >
        <Plus className="h-3 w-3 mr-1.5" />
        {placeholder || "Add Parameter"}
      </Button>
    </div>
  );
}

// Body Editor
interface BodyEditorProps {
  body: RequestBody;
  onChange: (body: RequestBody) => void;
}

function BodyEditor({ body, onChange }: BodyEditorProps) {
  const bodyType = body.type;

  return (
    <div className="space-y-4">
      <div className="flex gap-1 p-1 bg-muted/20 rounded-md w-fit">
        {(["none", "raw", "json", "x-www-form-urlencoded"] as const).map((type) => (
          <Button
            key={type}
            variant={bodyType === type ? "secondary" : "ghost"}
            size="sm"
            className="h-7 text-xs px-3"
            onClick={() => {
              if (type === "none") {
                onChange({ type: "none" });
              } else if (type === "raw") {
                onChange({ type: "raw", content: "", contentType: "text/plain" });
              } else if (type === "json") {
                onChange({ type: "json", content: "{}" });
              } else if (type === "x-www-form-urlencoded") {
                onChange({ type: "x-www-form-urlencoded", fields: [] });
              }
            }}
          >
            {type === "x-www-form-urlencoded" ? "Form URL Encoded" : type.charAt(0).toUpperCase() + type.slice(1)}
          </Button>
        ))}
      </div>

      <div className="border rounded-md min-h-[200px] relative bg-background">
        {body.type === "raw" && (
          <textarea
            value={body.content}
            onChange={(e) => onChange({ ...body, content: e.target.value })}
            placeholder="Raw body content"
            className="w-full h-full min-h-[200px] p-3 text-xs font-mono bg-transparent border-none resize-y focus:ring-0 outline-none"
          />
        )}

        {body.type === "json" && (
          <textarea
            value={body.content}
            onChange={(e) => onChange({ ...body, content: e.target.value })}
            placeholder='{"key": "value"}'
            className="w-full h-full min-h-[200px] p-3 text-xs font-mono bg-transparent border-none resize-y focus:ring-0 outline-none"
          />
        )}

        {body.type === "x-www-form-urlencoded" && (
          <div className="p-4">
            <KeyValueEditor
              items={body.fields}
              onChange={(fields) => onChange({ ...body, fields })}
              placeholder="Add Form Field"
            />
          </div>
        )}

        {body.type === "none" && (
          <div className="flex flex-col items-center justify-center min-h-[200px] text-muted-foreground/50">
            <p className="text-xs">This request does not have a body</p>
          </div>
        )}
      </div>
    </div>
  );
}
