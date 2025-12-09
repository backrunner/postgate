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
  Minus,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Badge } from "@/components/ui/badge";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
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
    <div className="flex h-full">
      {/* Sidebar - Collections */}
      <div className="w-64 border-r flex flex-col bg-muted/20">
        <div className="flex h-10 items-center justify-between border-b px-3">
          <h2 className="text-sm font-semibold">Collections</h2>
          <div className="flex items-center gap-1">
            <Button 
              variant="ghost" 
              size="icon-sm" 
              title="New Request"
              onClick={() => handleNewRequest()}
            >
              <FilePlus className="h-4 w-4" />
            </Button>
            <Button 
              variant="ghost" 
              size="icon-sm" 
              title="New Collection"
              onClick={() => setShowNewCollection(true)}
            >
              <FolderPlus className="h-4 w-4" />
            </Button>
          </div>
        </div>

        {/* New Collection Input */}
        {showNewCollection && (
          <div className="p-2 border-b flex gap-1">
            <Input
              value={newCollectionName}
              onChange={(e) => setNewCollectionName(e.target.value)}
              placeholder="Collection name"
              className="h-7 text-xs"
              onKeyDown={(e) => e.key === "Enter" && handleCreateCollection()}
              autoFocus
            />
            <Button size="icon-sm" variant="ghost" onClick={handleCreateCollection}>
              <Plus className="h-3 w-3" />
            </Button>
            <Button size="icon-sm" variant="ghost" onClick={() => setShowNewCollection(false)}>
              <X className="h-3 w-3" />
            </Button>
          </div>
        )}

        <ScrollArea className="flex-1">
          {isLoading ? (
            <div className="flex items-center justify-center p-4">
              <Loader2 className="h-4 w-4 animate-spin" />
            </div>
          ) : tree ? (
            <div className="p-2">
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
                <p className="text-xs text-muted-foreground text-center p-4">
                  No collections yet. Create one to organize your requests.
                </p>
              )}
            </div>
          ) : null}
        </ScrollArea>
      </div>

      {/* Main Content */}
      <div className="flex-1 flex flex-col">
        {currentRequest.url ? (
          <>
            {/* URL Bar */}
            <div className="border-b p-3 flex gap-2">
              <select
                value={currentRequest.method}
                onChange={(e) => updateCurrentRequest({ method: e.target.value })}
                className={cn(
                  "h-9 px-2 rounded border bg-background text-sm font-mono font-semibold",
                  getMethodClass(currentRequest.method)
                )}
              >
                {["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"].map((m) => (
                  <option key={m} value={m}>{m}</option>
                ))}
              </select>
              <Input
                value={currentRequest.url}
                onChange={(e) => updateCurrentRequest({ url: e.target.value })}
                placeholder="https://api.example.com/endpoint"
                className="flex-1 font-mono text-sm"
              />
              <Button 
                onClick={executeRequest} 
                disabled={isExecuting}
                className="gap-1"
              >
                {isExecuting ? (
                  <Loader2 className="h-4 w-4 animate-spin" />
                ) : (
                  <Play className="h-4 w-4" />
                )}
                Send
              </Button>
              <Button 
                variant="outline" 
                onClick={handleSave}
                className="gap-1"
              >
                <Save className="h-4 w-4" />
                Save
              </Button>
            </div>

            {/* Request/Response Split */}
            <div className="flex-1 flex overflow-hidden">
              {/* Request Panel */}
              <div className="flex-1 border-r overflow-hidden flex flex-col">
                <Tabs defaultValue="params" className="flex-1 flex flex-col">
                  <TabsList className="mx-3 mt-2 w-fit">
                    <TabsTrigger value="params">Params</TabsTrigger>
                    <TabsTrigger value="headers">Headers</TabsTrigger>
                    <TabsTrigger value="body">Body</TabsTrigger>
                  </TabsList>

                  <TabsContent value="params" className="flex-1 overflow-auto m-0 p-3">
                    <KeyValueEditor
                      items={currentRequest.query_params}
                      onChange={(query_params) => updateCurrentRequest({ query_params })}
                      placeholder="Add query parameter"
                    />
                  </TabsContent>

                  <TabsContent value="headers" className="flex-1 overflow-auto m-0 p-3">
                    <KeyValueEditor
                      items={currentRequest.headers}
                      onChange={(headers) => updateCurrentRequest({ headers })}
                      placeholder="Add header"
                    />
                  </TabsContent>

                  <TabsContent value="body" className="flex-1 overflow-auto m-0 p-3">
                    <BodyEditor
                      body={currentRequest.body}
                      onChange={(body) => updateCurrentRequest({ body })}
                    />
                  </TabsContent>
                </Tabs>
              </div>

              {/* Response Panel */}
              <div className="flex-1 overflow-hidden flex flex-col">
                <div className="h-10 border-b px-3 flex items-center justify-between">
                  <span className="text-sm font-semibold">Response</span>
                  {response && (
                    <div className="flex items-center gap-2 text-xs">
                      <Badge variant="outline" className={getStatusClass(response.status)}>
                        {response.status} {response.statusText}
                      </Badge>
                      <span className="text-muted-foreground">
                        {formatDuration(response.durationMs)}
                      </span>
                      <span className="text-muted-foreground">
                        {formatBytes(response.bodySize)}
                      </span>
                    </div>
                  )}
                </div>

                <ScrollArea className="flex-1">
                  {error ? (
                    <div className="p-4 text-sm text-destructive">
                      {error}
                    </div>
                  ) : response ? (
                    <Tabs defaultValue="body" className="flex-1">
                      <TabsList className="mx-3 mt-2 w-fit">
                        <TabsTrigger value="body">Body</TabsTrigger>
                        <TabsTrigger value="headers">Headers</TabsTrigger>
                      </TabsList>

                      <TabsContent value="body" className="m-0 p-3">
                        <pre className="text-xs font-mono bg-muted/30 p-3 rounded overflow-auto max-h-[500px]">
                          {response.body || "(empty response)"}
                        </pre>
                      </TabsContent>

                      <TabsContent value="headers" className="m-0 p-3">
                        <div className="space-y-1">
                          {Object.entries(response.headers).map(([key, value]) => (
                            <div key={key} className="flex gap-2 text-xs">
                              <span className="font-medium text-muted-foreground min-w-[150px]">{key}:</span>
                              <span className="break-all">{value}</span>
                            </div>
                          ))}
                        </div>
                      </TabsContent>
                    </Tabs>
                  ) : (
                    <div className="flex items-center justify-center h-full text-muted-foreground text-sm">
                      Send a request to see the response
                    </div>
                  )}
                </ScrollArea>
              </div>
            </div>
          </>
        ) : (
          <div className="flex-1 flex items-center justify-center">
            <div className="text-center text-muted-foreground">
              <Send className="mx-auto h-12 w-12 mb-4 opacity-50" />
              <h3 className="font-semibold mb-1">Request Replay</h3>
              <p className="text-sm mb-4 max-w-md">
                Send HTTP requests, save them to collections, and replay them anytime.
              </p>
              <Button onClick={() => handleNewRequest()} className="gap-1">
                <FilePlus className="h-4 w-4" />
                New Request
              </Button>
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
  const hasChildren = node.children.length > 0 || node.requests.length > 0;

  return (
    <div>
      <div 
        className="flex items-center gap-1 py-1 px-1 rounded hover:bg-muted/50 cursor-pointer group"
        style={{ paddingLeft: depth * 12 + 4 }}
      >
        <button 
          onClick={() => onToggle(node.collection.id)}
          className="p-0.5"
        >
          {hasChildren ? (
            isExpanded ? <ChevronDown className="h-3 w-3" /> : <ChevronRight className="h-3 w-3" />
          ) : (
            <span className="w-3" />
          )}
        </button>
        <Folder className="h-3.5 w-3.5 text-muted-foreground" />
        <span className="text-xs flex-1 truncate">{node.collection.name}</span>
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button variant="ghost" size="icon-sm" className="opacity-0 group-hover:opacity-100 h-5 w-5">
              <MoreHorizontal className="h-3 w-3" />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            <DropdownMenuItem onClick={() => onNewRequest(node.collection.id)}>
              <FilePlus className="h-3.5 w-3.5 mr-2" />
              New Request
            </DropdownMenuItem>
            <DropdownMenuItem onClick={() => onDeleteCollection(node.collection.id)} className="text-destructive">
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
        "flex items-center gap-1 py-1 px-1 rounded cursor-pointer group",
        isSelected ? "bg-primary/10" : "hover:bg-muted/50"
      )}
      style={{ paddingLeft: depth * 12 + 16 }}
      onClick={onSelect}
    >
      <Badge 
        variant="outline" 
        className={cn("text-[10px] px-1 py-0 h-4 font-mono", getMethodClass(request.method))}
      >
        {request.method.substring(0, 3)}
      </Badge>
      <span className="text-xs flex-1 truncate">{request.name}</span>
      <DropdownMenu>
        <DropdownMenuTrigger asChild onClick={(e) => e.stopPropagation()}>
          <Button variant="ghost" size="icon-sm" className="opacity-0 group-hover:opacity-100 h-5 w-5">
            <MoreHorizontal className="h-3 w-3" />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end">
          <DropdownMenuItem onClick={(e) => { e.stopPropagation(); onDuplicate(); }}>
            <Copy className="h-3.5 w-3.5 mr-2" />
            Duplicate
          </DropdownMenuItem>
          <DropdownMenuItem onClick={(e) => { e.stopPropagation(); onDelete(); }} className="text-destructive">
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
    <div className="space-y-1">
      {items.map((item, index) => (
        <div key={index} className="flex items-center gap-1">
          <input
            type="checkbox"
            checked={item.enabled}
            onChange={(e) => updateItem(index, { enabled: e.target.checked })}
            className="h-3.5 w-3.5"
          />
          <Input
            value={item.key}
            onChange={(e) => updateItem(index, { key: e.target.value })}
            placeholder="Key"
            className="h-7 text-xs flex-1"
          />
          <Input
            value={item.value}
            onChange={(e) => updateItem(index, { value: e.target.value })}
            placeholder="Value"
            className="h-7 text-xs flex-1"
          />
          <Button variant="ghost" size="icon-sm" onClick={() => removeItem(index)}>
            <Minus className="h-3 w-3" />
          </Button>
        </div>
      ))}
      <Button variant="ghost" size="sm" onClick={addItem} className="text-xs">
        <Plus className="h-3 w-3 mr-1" />
        {placeholder || "Add"}
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
    <div className="space-y-3">
      <div className="flex gap-2">
        {(["none", "raw", "json", "x-www-form-urlencoded"] as const).map((type) => (
          <Button
            key={type}
            variant={bodyType === type ? "default" : "outline"}
            size="sm"
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
            {type === "x-www-form-urlencoded" ? "form" : type}
          </Button>
        ))}
      </div>

      {body.type === "raw" && (
        <textarea
          value={body.content}
          onChange={(e) => onChange({ ...body, content: e.target.value })}
          placeholder="Raw body content"
          className="w-full h-48 p-2 text-xs font-mono bg-muted/30 rounded border resize-none"
        />
      )}

      {body.type === "json" && (
        <textarea
          value={body.content}
          onChange={(e) => onChange({ ...body, content: e.target.value })}
          placeholder='{"key": "value"}'
          className="w-full h-48 p-2 text-xs font-mono bg-muted/30 rounded border resize-none"
        />
      )}

      {body.type === "x-www-form-urlencoded" && (
        <KeyValueEditor
          items={body.fields}
          onChange={(fields) => onChange({ ...body, fields })}
          placeholder="Add field"
        />
      )}

      {body.type === "none" && (
        <p className="text-xs text-muted-foreground">
          This request does not have a body
        </p>
      )}
    </div>
  );
}
