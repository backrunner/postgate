import { useEffect, useState, useCallback, useMemo } from 'react';
import {
  Database,
  Plus,
  Save,
  Undo2,
  Search,
  Trash2,
  Pencil,
  ChevronRight,
  ChevronDown,
  FileText,
  FolderOpen,
} from 'lucide-react';
import Editor, { OnChange } from '@monaco-editor/react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog';
import { PageHeader } from '@/components/layout/PageHeader';
import { useValuesStore, ValueEntry } from '@/stores/values';
import { useThemeStore } from '@/stores/theme';
import { cn } from '@/lib/utils';

/** Infer a Monaco language id from the value name's extension. */
function languageForName(name: string): string {
  const ext = name.toLowerCase().split('.').pop() ?? '';
  switch (ext) {
    case 'json':
      return 'json';
    case 'html':
    case 'htm':
      return 'html';
    case 'js':
    case 'mjs':
    case 'cjs':
      return 'javascript';
    case 'ts':
      return 'typescript';
    case 'css':
      return 'css';
    case 'xml':
      return 'xml';
    case 'yml':
    case 'yaml':
      return 'yaml';
    case 'md':
      return 'markdown';
    default:
      return 'plaintext';
  }
}

/** Tree node used to render `/`-separated names as a folder hierarchy. */
interface TreeNode {
  name: string; // last segment (display label)
  fullName?: string; // only present on leaves: full value name used for selection
  children: TreeNode[];
}

function buildTree(values: ValueEntry[]): TreeNode[] {
  const root: TreeNode = { name: '', children: [] };
  for (const v of values) {
    const parts = v.name.split('/');
    let cursor = root;
    for (let i = 0; i < parts.length; i++) {
      const segment = parts[i];
      const isLeaf = i === parts.length - 1;
      let child = cursor.children.find(
        (c) => c.name === segment && (isLeaf ? !!c.fullName : !c.fullName),
      );
      if (!child) {
        child = {
          name: segment,
          fullName: isLeaf ? v.name : undefined,
          children: [],
        };
        cursor.children.push(child);
      }
      cursor = child;
    }
  }
  // Sort: folders first, then alpha.
  const sortTree = (node: TreeNode) => {
    node.children.sort((a, b) => {
      const aFolder = !a.fullName;
      const bFolder = !b.fullName;
      if (aFolder !== bFolder) return aFolder ? -1 : 1;
      return a.name.localeCompare(b.name);
    });
    node.children.forEach(sortTree);
  };
  sortTree(root);
  return root.children;
}

export function ValuesPage() {
  const { theme } = useThemeStore();
  const {
    values,
    selectedName,
    editorContent,
    isDirty,
    loadValues,
    selectValue,
    setEditorContent,
    saveCurrent,
    discardChanges,
    createValue,
    renameValue,
    deleteValue,
  } = useValuesStore();

  const [searchQuery, setSearchQuery] = useState('');
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [createOpen, setCreateOpen] = useState(false);
  const [newValueName, setNewValueName] = useState('');
  const [isCreating, setIsCreating] = useState(false);

  const [renameTarget, setRenameTarget] = useState<string | null>(null);
  const [renameInput, setRenameInput] = useState('');

  const [deleteTarget, setDeleteTarget] = useState<string | null>(null);

  // Resizable sidebar — same pattern as Rules page.
  const [sidebarWidth, setSidebarWidth] = useState(220);
  const [isResizing, setIsResizing] = useState(false);

  useEffect(() => {
    loadValues();
  }, [loadValues]);

  useEffect(() => {
    const handleMouseMove = (e: MouseEvent) => {
      if (!isResizing) return;
      const newWidth = e.clientX;
      setSidebarWidth(Math.min(480, Math.max(160, newWidth)));
    };
    const handleMouseUp = () => setIsResizing(false);
    if (isResizing) {
      document.addEventListener('mousemove', handleMouseMove);
      document.addEventListener('mouseup', handleMouseUp);
      document.body.style.cursor = 'col-resize';
      document.body.style.userSelect = 'none';
    }
    return () => {
      document.removeEventListener('mousemove', handleMouseMove);
      document.removeEventListener('mouseup', handleMouseUp);
      document.body.style.cursor = '';
      document.body.style.userSelect = '';
    };
  }, [isResizing]);

  const startResizing = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    setIsResizing(true);
  }, []);

  // Cmd/Ctrl+S saves, matching the Rules page.
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === 's') {
        e.preventDefault();
        if (isDirty && selectedName) {
          saveCurrent().catch((err) => console.error('Failed to save value:', err));
        }
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [isDirty, selectedName, saveCurrent]);

  const filtered = useMemo(() => {
    const q = searchQuery.trim().toLowerCase();
    if (!q) return values;
    return values.filter((v) => v.name.toLowerCase().includes(q));
  }, [values, searchQuery]);

  const tree = useMemo(() => buildTree(filtered), [filtered]);

  const toggleFolder = (path: string) => {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  };

  const handleCreate = async () => {
    const name = newValueName.trim();
    if (!name) return;
    setIsCreating(true);
    try {
      await createValue(name);
      setNewValueName('');
      setCreateOpen(false);
    } catch (e) {
      console.error('Failed to create value:', e);
    } finally {
      setIsCreating(false);
    }
  };

  const handleRename = async () => {
    if (!renameTarget) return;
    const next = renameInput.trim();
    if (!next || next === renameTarget) {
      setRenameTarget(null);
      return;
    }
    try {
      await renameValue(renameTarget, next);
      setRenameTarget(null);
    } catch (e) {
      console.error('Failed to rename value:', e);
    }
  };

  const handleDelete = async () => {
    if (!deleteTarget) return;
    try {
      await deleteValue(deleteTarget);
      setDeleteTarget(null);
    } catch (e) {
      console.error('Failed to delete value:', e);
    }
  };

  const editorTheme =
    theme === 'dark' ||
    (theme === 'system' && window.matchMedia('(prefers-color-scheme: dark)').matches)
      ? 'vs-dark'
      : 'vs';

  const handleEditorChange: OnChange = (value) => {
    if (value !== undefined) setEditorContent(value);
  };

  const selectedEntry = selectedName ? values.find((v) => v.name === selectedName) : undefined;
  const editorLanguage = selectedName ? languageForName(selectedName) : 'plaintext';

  return (
    <div className="flex h-full flex-col bg-background">
      {/* Unified page header */}
      <PageHeader
        icon={Database}
        title="Values"
        subtitle={
          selectedEntry && (
            <>
              <span className="font-medium truncate">{selectedEntry.name}</span>
              {isDirty && (
                <span className="text-xs text-amber-500 bg-amber-500/10 px-1.5 py-0.5 rounded font-medium">
                  Unsaved
                </span>
              )}
            </>
          )
        }
      >
        {selectedName && (
          <>
            <Tooltip>
              <TooltipTrigger asChild>
                <Button
                  variant="ghost"
                  size="sm"
                  className="h-8 px-2 gap-1.5 text-xs text-muted-foreground hover:text-foreground"
                  onClick={discardChanges}
                  disabled={!isDirty}
                >
                  <Undo2 className="h-3.5 w-3.5" />
                  Discard
                </Button>
              </TooltipTrigger>
              <TooltipContent>Discard changes</TooltipContent>
            </Tooltip>
            <Tooltip>
              <TooltipTrigger asChild>
                <Button
                  variant={isDirty ? 'default' : 'outline'}
                  size="sm"
                  className={cn(
                    'h-8 px-3 gap-1.5 text-xs transition-all',
                    isDirty ? 'shadow-sm' : 'text-muted-foreground',
                  )}
                  onClick={() => saveCurrent().catch(console.error)}
                  disabled={!isDirty}
                >
                  <Save className="h-3.5 w-3.5" />
                  Save
                </Button>
              </TooltipTrigger>
              <TooltipContent>Save changes (Cmd+S)</TooltipContent>
            </Tooltip>
          </>
        )}
      </PageHeader>

      {/* Content */}
      <div className="flex flex-1 overflow-hidden">
        {/* Sidebar */}
        <div className="border-r flex flex-col relative" style={{ width: sidebarWidth }}>
          {/* Search + add */}
          <div className="px-1.5 py-1.5 border-b flex items-center gap-1">
            <div className="relative flex-1">
              <Search className="absolute left-2 top-1/2 -translate-y-1/2 h-3 w-3 text-muted-foreground" />
              <Input
                placeholder="Search..."
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
                className="h-7 text-xs pl-7 bg-background"
              />
            </div>
            <Tooltip>
              <TooltipTrigger asChild>
                <Button
                  size="icon"
                  variant="ghost"
                  className="h-7 w-7 shrink-0"
                  onClick={() => setCreateOpen(true)}
                >
                  <Plus className="h-4 w-4" />
                </Button>
              </TooltipTrigger>
              <TooltipContent>New value</TooltipContent>
            </Tooltip>
          </div>

          {/* Tree */}
          <div className="flex-1 overflow-auto py-1">
            {tree.length === 0 ? (
              <div className="flex flex-col items-center justify-center py-10 px-4 text-center text-muted-foreground">
                <Database className="h-6 w-6 mb-2 opacity-30" />
                <p className="text-xs">
                  {searchQuery ? 'No matches' : 'No values yet'}
                </p>
              </div>
            ) : (
              <ul className="text-xs">
                {tree.map((node) => (
                  <TreeItem
                    key={node.fullName ?? node.name}
                    node={node}
                    depth={0}
                    path={node.name}
                    expanded={expanded}
                    toggleFolder={toggleFolder}
                    selectedName={selectedName}
                    onSelect={selectValue}
                    onRename={(name) => {
                      setRenameTarget(name);
                      setRenameInput(name);
                    }}
                    onDelete={(name) => setDeleteTarget(name)}
                  />
                ))}
              </ul>
            )}
          </div>

          {/* Resize handle */}
          <div
            className="absolute top-0 right-0 w-1 h-full cursor-col-resize hover:bg-primary/20 active:bg-primary/40 transition-colors"
            onMouseDown={startResizing}
          />
        </div>

        {/* Editor */}
        <div className="flex-1 flex flex-col min-w-0 bg-background">
          {selectedName ? (
            <Editor
              height="100%"
              language={editorLanguage}
              theme={editorTheme}
              value={editorContent}
              onChange={handleEditorChange}
              options={{
                fontSize: 13,
                fontFamily: 'JetBrains Mono, Menlo, Monaco, Consolas, monospace',
                lineNumbers: 'on',
                minimap: { enabled: false },
                scrollBeyondLastLine: false,
                wordWrap: 'on',
                wrappingIndent: 'indent',
                automaticLayout: true,
                tabSize: 2,
                insertSpaces: true,
                folding: true,
                lineDecorationsWidth: 10,
              }}
            />
          ) : (
            <div className="flex-1 flex flex-col items-center justify-center p-8 text-center text-muted-foreground">
              <div className="h-16 w-16 bg-muted/30 rounded-2xl flex items-center justify-center mb-6">
                <Database className="h-8 w-8 opacity-20" />
              </div>
              <h3 className="text-lg font-semibold mb-2 text-foreground">No Value Selected</h3>
              <p className="text-sm max-w-sm mb-6 leading-relaxed">
                {values.length === 0
                  ? 'Create a value to reference it from your rules using {name} or `{name}` templates.'
                  : 'Select a value from the sidebar to edit its content.'}
              </p>
              {values.length === 0 && (
                <Button onClick={() => setCreateOpen(true)}>
                  <Plus className="h-4 w-4 mr-2" />
                  Create Value
                </Button>
              )}
            </div>
          )}
        </div>
      </div>

      {/* Create dialog */}
      <Dialog open={createOpen} onOpenChange={setCreateOpen}>
        <DialogContent className="sm:max-w-[440px]">
          <DialogHeader>
            <DialogTitle>New Value</DialogTitle>
            <DialogDescription>
              Name your value — use <code className="font-mono text-xs">/</code> to group values into
              folders (e.g. <code className="font-mono text-xs">mock/users.json</code>). The file
              extension determines the editor syntax highlighting.
            </DialogDescription>
          </DialogHeader>
          <div className="py-4">
            <Input
              value={newValueName}
              onChange={(e) => setNewValueName(e.target.value)}
              placeholder="mock/users.json"
              onKeyDown={(e) => {
                if (e.key === 'Enter' && newValueName.trim()) handleCreate();
              }}
              autoFocus
            />
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setCreateOpen(false)}>
              Cancel
            </Button>
            <Button onClick={handleCreate} disabled={!newValueName.trim() || isCreating}>
              Create
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Rename dialog */}
      <Dialog open={!!renameTarget} onOpenChange={(open) => !open && setRenameTarget(null)}>
        <DialogContent className="sm:max-w-[440px]">
          <DialogHeader>
            <DialogTitle>Rename Value</DialogTitle>
            <DialogDescription>
              Any rule referencing the old name will stop resolving — update your rules accordingly.
            </DialogDescription>
          </DialogHeader>
          <div className="py-4">
            <Input
              value={renameInput}
              onChange={(e) => setRenameInput(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter') handleRename();
              }}
              autoFocus
            />
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setRenameTarget(null)}>
              Cancel
            </Button>
            <Button onClick={handleRename} disabled={!renameInput.trim()}>
              Rename
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Delete confirmation */}
      <AlertDialog open={!!deleteTarget} onOpenChange={(open) => !open && setDeleteTarget(null)}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Delete value?</AlertDialogTitle>
            <AlertDialogDescription>
              Permanently delete <code className="font-mono text-xs">{deleteTarget}</code>. Rules
              still referencing this name will resolve to empty content.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction onClick={handleDelete}>Delete</AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}

interface TreeItemProps {
  node: TreeNode;
  depth: number;
  path: string;
  expanded: Set<string>;
  toggleFolder: (path: string) => void;
  selectedName: string | null;
  onSelect: (name: string | null) => void;
  onRename: (name: string) => void;
  onDelete: (name: string) => void;
}

function TreeItem({
  node,
  depth,
  path,
  expanded,
  toggleFolder,
  selectedName,
  onSelect,
  onRename,
  onDelete,
}: TreeItemProps) {
  const isFolder = !node.fullName;
  const isOpen = expanded.has(path);
  const [hover, setHover] = useState(false);

  if (isFolder) {
    return (
      <li>
        <button
          className={cn(
            'w-full flex items-center gap-1 px-2 py-1 hover:bg-muted/50 text-left',
          )}
          style={{ paddingLeft: 8 + depth * 12 }}
          onClick={() => toggleFolder(path)}
        >
          {isOpen ? (
            <ChevronDown className="h-3 w-3 text-muted-foreground shrink-0" />
          ) : (
            <ChevronRight className="h-3 w-3 text-muted-foreground shrink-0" />
          )}
          <FolderOpen className="h-3.5 w-3.5 text-muted-foreground shrink-0" />
          <span className="truncate">{node.name}</span>
        </button>
        {isOpen && node.children.length > 0 && (
          <ul>
            {node.children.map((child) => (
              <TreeItem
                key={child.fullName ?? `${path}/${child.name}`}
                node={child}
                depth={depth + 1}
                path={`${path}/${child.name}`}
                expanded={expanded}
                toggleFolder={toggleFolder}
                selectedName={selectedName}
                onSelect={onSelect}
                onRename={onRename}
                onDelete={onDelete}
              />
            ))}
          </ul>
        )}
      </li>
    );
  }

  const isSelected = node.fullName === selectedName;
  return (
    <li
      className="group relative"
      onMouseEnter={() => setHover(true)}
      onMouseLeave={() => setHover(false)}
    >
      <button
        className={cn(
          'w-full flex items-center gap-1 pr-14 px-2 py-1 text-left truncate',
          isSelected ? 'bg-primary/10 text-primary font-medium' : 'hover:bg-muted/50',
        )}
        style={{ paddingLeft: 8 + depth * 12 + 12 /* chevron gap */ }}
        onClick={() => onSelect(node.fullName!)}
      >
        <FileText className="h-3.5 w-3.5 text-muted-foreground shrink-0" />
        <span className="truncate">{node.name}</span>
      </button>
      {hover && (
        <div className="absolute right-1 top-1/2 -translate-y-1/2 flex items-center gap-0.5">
          <Tooltip>
            <TooltipTrigger asChild>
              <Button
                variant="ghost"
                size="icon"
                className="h-5 w-5"
                onClick={(e) => {
                  e.stopPropagation();
                  onRename(node.fullName!);
                }}
              >
                <Pencil className="h-3 w-3" />
              </Button>
            </TooltipTrigger>
            <TooltipContent>Rename</TooltipContent>
          </Tooltip>
          <Tooltip>
            <TooltipTrigger asChild>
              <Button
                variant="ghost"
                size="icon"
                className="h-5 w-5 text-muted-foreground hover:text-destructive"
                onClick={(e) => {
                  e.stopPropagation();
                  onDelete(node.fullName!);
                }}
              >
                <Trash2 className="h-3 w-3" />
              </Button>
            </TooltipTrigger>
            <TooltipContent>Delete</TooltipContent>
          </Tooltip>
        </div>
      )}
    </li>
  );
}
