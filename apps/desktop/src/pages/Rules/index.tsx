import { useEffect, useState, useCallback, useRef } from 'react';
import { FileCode, Plus, Save, Undo2, ChevronLeft, ChevronRight, Search, CheckCircle, XCircle } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Separator } from '@/components/ui/separator';
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { RuleEditor } from '@/components/rules/RuleEditor';
import { RuleGroupList } from '@/components/rules/RuleGroupList';
import { ParseStatus } from '@/components/rules/ParseStatus';
import { useRulesStore } from '@/stores/rules';
import { cn } from '@/lib/utils';

export function RulesPage() {
  const {
    groups,
    selectedGroupId,
    isDirty,
    parseResult,
    loadGroups,
    createGroup,
    saveCurrentGroup,
    discardChanges,
  } = useRulesStore();

  const [searchQuery, setSearchQuery] = useState('');
  const [isCreating, setIsCreating] = useState(false);
  const [createDialogOpen, setCreateDialogOpen] = useState(false);
  const [newGroupName, setNewGroupName] = useState('');
  const [statusPanelCollapsed, setStatusPanelCollapsed] = useState(true); // Default collapsed
  
  // Resizable sidebar state
  const [sidebarWidth, setSidebarWidth] = useState(200);
  const [isResizing, setIsResizing] = useState(false);
  const sidebarRef = useRef<HTMLDivElement>(null);

  // Load groups on mount
  useEffect(() => {
    loadGroups();
  }, [loadGroups]);

  // Auto-select first enabled group when groups load and nothing is selected
  useEffect(() => {
    if (groups.length > 0 && !selectedGroupId) {
      const firstEnabled = groups.find(g => g.enabled);
      if (firstEnabled) {
        useRulesStore.getState().selectGroup(firstEnabled.id);
      } else if (groups.length > 0) {
        useRulesStore.getState().selectGroup(groups[0].id);
      }
    }
  }, [groups, selectedGroupId]);

  // Handle sidebar resize
  const startResizing = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    setIsResizing(true);
  }, []);

  useEffect(() => {
    const handleMouseMove = (e: MouseEvent) => {
      if (!isResizing) return;
      const newWidth = e.clientX;
      setSidebarWidth(Math.min(400, Math.max(140, newWidth)));
    };

    const handleMouseUp = () => {
      setIsResizing(false);
    };

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

  // Global keyboard shortcuts
  const handleKeyDown = useCallback((e: KeyboardEvent) => {
    if ((e.metaKey || e.ctrlKey) && e.key === 's') {
      e.preventDefault();
      if (isDirty && selectedGroupId) {
        saveCurrentGroup();
      }
    }
  }, [isDirty, selectedGroupId, saveCurrentGroup]);

  useEffect(() => {
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [handleKeyDown]);

  const selectedGroup = groups.find(g => g.id === selectedGroupId);

  const handleCreateGroup = async () => {
    if (!newGroupName.trim()) return;
    
    setIsCreating(true);
    try {
      const group = await createGroup(newGroupName.trim());
      useRulesStore.getState().selectGroup(group.id);
      setNewGroupName('');
      setCreateDialogOpen(false);
    } catch (e) {
      console.error('Failed to create group:', e);
    } finally {
      setIsCreating(false);
    }
  };

  const handleSave = async () => {
    try {
      await saveCurrentGroup();
    } catch (e) {
      console.error('Failed to save:', e);
    }
  };

  // Check if current rules are valid
  const isValid = parseResult ? parseResult.errors.length === 0 : true;

  return (
    <div className="flex h-full flex-col bg-background">
      {/* Toolbar */}
      <div className="flex h-12 items-center justify-between border-b px-4">
        <div className="flex items-center gap-3">
          <h2 className="text-sm font-semibold text-muted-foreground uppercase tracking-wider">Rules</h2>
          {selectedGroup && (
            <>
              <Separator orientation="vertical" className="h-6" />
              <div className="flex items-center gap-2">
                <span className="text-sm font-medium">{selectedGroup.name}</span>
                {isDirty && (
                  <span className="text-xs text-amber-500 bg-amber-500/10 px-1.5 py-0.5 rounded font-medium">Unsaved</span>
                )}
              </div>
            </>
          )}
        </div>
        <div className="flex items-center gap-1">
          {selectedGroupId && (
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
                    variant={isDirty ? "default" : "outline"}
                    size="sm"
                    className={cn("h-8 px-3 gap-1.5 text-xs transition-all", isDirty ? "shadow-sm" : "text-muted-foreground")}
                    onClick={handleSave}
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
        </div>
      </div>

      {/* Content */}
      <div className="flex flex-1 overflow-hidden">
        {/* Sidebar - Rule Groups */}
        <div
          ref={sidebarRef}
          className="border-r flex flex-col relative"
          style={{ width: sidebarWidth }}
        >
          {/* Search and Add */}
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
                  onClick={() => setCreateDialogOpen(true)}
                >
                  <Plus className="h-4 w-4" />
                </Button>
              </TooltipTrigger>
              <TooltipContent>New group</TooltipContent>
            </Tooltip>
          </div>

          {/* Groups list */}
          <RuleGroupList className="flex-1" filter={searchQuery} />

          {/* Resize handle */}
          <div
            className="absolute top-0 right-0 w-1 h-full cursor-col-resize hover:bg-primary/20 active:bg-primary/40 transition-colors"
            onMouseDown={startResizing}
          />
        </div>

        {/* Main editor area */}
        <div className="flex-1 flex flex-col min-w-0 bg-background">
          {selectedGroupId ? (
            <RuleEditor className="flex-1" />
          ) : (
            <div className="flex-1 flex flex-col items-center justify-center p-8 text-center text-muted-foreground">
              <div className="h-16 w-16 bg-muted/30 rounded-2xl flex items-center justify-center mb-6">
                <FileCode className="h-8 w-8 opacity-20" />
              </div>
              <h3 className="text-lg font-semibold mb-2 text-foreground">No Rule Group Selected</h3>
              <p className="text-sm max-w-sm mb-8 leading-relaxed">
                {groups.length === 0
                  ? 'Create a rule group to start intercepting and modifying requests.'
                  : 'Select a group from the sidebar to edit its rules.'}
              </p>
              
              {groups.length === 0 && (
                <Button onClick={() => setCreateDialogOpen(true)}>
                  <Plus className="h-4 w-4 mr-2" />
                  Create Group
                </Button>
              )}
            </div>
          )}
        </div>

        {/* Status panel */}
        <div
          className={cn(
            'border-l flex flex-col',
            statusPanelCollapsed ? 'w-10' : 'w-64'
          )}
        >
          {/* Header - different layout when collapsed vs expanded */}
          {statusPanelCollapsed ? (
            // Collapsed: centered button + status indicator
            <div className="flex flex-col items-center py-2 border-b">
              <Button
                variant="ghost"
                size="sm"
                className="h-7 w-7 p-0"
                onClick={() => setStatusPanelCollapsed(false)}
              >
                <ChevronLeft className="h-4 w-4" />
              </Button>
              
              {/* Show status indicator when collapsed */}
              {selectedGroupId && (
                <div className="mt-2">
                  {isValid ? (
                    <CheckCircle className="h-4 w-4 text-emerald-500" />
                  ) : (
                    <XCircle className="h-4 w-4 text-red-500" />
                  )}
                </div>
              )}
            </div>
          ) : (
            // Expanded: button on left + title
            <div className="flex items-center h-9 border-b px-1">
              <Button
                variant="ghost"
                size="sm"
                className="h-7 w-full justify-start gap-2 text-muted-foreground hover:text-foreground px-2 text-xs"
                onClick={() => setStatusPanelCollapsed(true)}
              >
                <ChevronRight className="h-3.5 w-3.5" />
                <span className="font-medium">Status</span>
              </Button>
            </div>
          )}

          {!statusPanelCollapsed && (
            <ParseStatus className="flex-1" />
          )}
        </div>
      </div>

      {/* Create Group Dialog */}
      <Dialog open={createDialogOpen} onOpenChange={setCreateDialogOpen}>
        <DialogContent className="sm:max-w-[400px]">
          <DialogHeader>
            <DialogTitle>Create Rule Group</DialogTitle>
            <DialogDescription>
              Enter a name for the new rule group.
            </DialogDescription>
          </DialogHeader>
          <div className="py-4">
            <Input
              value={newGroupName}
              onChange={(e) => setNewGroupName(e.target.value)}
              placeholder="Group name"
              onKeyDown={(e) => {
                if (e.key === 'Enter' && newGroupName.trim()) {
                  handleCreateGroup();
                }
              }}
              autoFocus
            />
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setCreateDialogOpen(false)}>
              Cancel
            </Button>
            <Button onClick={handleCreateGroup} disabled={!newGroupName.trim() || isCreating}>
              Create
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
