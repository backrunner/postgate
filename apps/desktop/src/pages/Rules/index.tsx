import { useEffect, useState, useCallback, useRef } from 'react';
import { AlertTriangle, FileCode, Plus, Save, Undo2, ChevronLeft, Search, CheckCircle, XCircle } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip';
import { PageHeader } from '@/components/layout/PageHeader';
import { PanelEmptyState } from '@/components/layout/PanelEmptyState';
import { WorkspaceSidebar } from '@/components/layout/WorkspaceSidebar';
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
    editorContent,
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
  const [sidebarWidth, setSidebarWidth] = useState(240);
  const [isResizing, setIsResizing] = useState(false);
  const sidebarRef = useRef<HTMLElement>(null);

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
      const sidebarLeft = sidebarRef.current?.getBoundingClientRect().left ?? 0;
      const newWidth = e.clientX - sidebarLeft;
      setSidebarWidth(Math.min(400, Math.max(200, newWidth)));
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

  const hasNonCommentRuleLine = editorContent.split(/\r?\n/).some((line) => {
    const trimmed = line.trim();
    return trimmed.length > 0 && !trimmed.startsWith('#');
  });
  const statusPanelVisible = !!selectedGroupId && (
    parseResult
      ? parseResult.rules.length > 0 ||
        parseResult.errors.length > 0 ||
        (parseResult.warnings?.length ?? 0) > 0
      : hasNonCommentRuleLine
  );
  const hasStatusErrors = (parseResult?.errors.length ?? 0) > 0;
  const hasStatusWarnings = (parseResult?.warnings?.length ?? 0) > 0;

  return (
    <div className="flex h-full flex-col">
      {/* Unified page header */}
      <PageHeader
        icon={FileCode}
        title="Rules"
        subtitle={
          selectedGroup && (
            <>
              <span className="font-medium truncate">{selectedGroup.name}</span>
              {isDirty && (
                <span className="text-xs text-amber-500 bg-amber-500/10 px-1.5 py-0.5 rounded font-medium">Unsaved</span>
              )}
            </>
          )
        }
      >
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
      </PageHeader>

      {/* Content */}
      <div className="flex flex-1 overflow-hidden">
        {/* Sidebar - Rule Groups */}
        <WorkspaceSidebar
          ref={sidebarRef}
          title="Rule Groups"
          style={{ width: sidebarWidth }}
          onResizeStart={startResizing}
          actions={(
            <Tooltip>
              <TooltipTrigger asChild>
                <Button
                  size="icon"
                  variant="ghost"
                  className="h-7 w-7"
                  onClick={() => setCreateDialogOpen(true)}
                >
                  <Plus className="h-3.5 w-3.5" />
                </Button>
              </TooltipTrigger>
              <TooltipContent>New group</TooltipContent>
            </Tooltip>
          )}
          toolbar={(
            <div className="relative">
              <Search className="absolute left-2 top-1/2 h-3 w-3 -translate-y-1/2 text-muted-foreground" />
              <Input
                placeholder="Search..."
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
                className="h-7 bg-background/80 pl-7 text-xs"
              />
            </div>
          )}
        >
          <RuleGroupList className="h-full" filter={searchQuery} />
        </WorkspaceSidebar>

        {/* Main editor area */}
        <div className="flex-1 flex flex-col min-w-0 bg-background/65">
          {selectedGroupId ? (
            <RuleEditor className="flex-1" />
          ) : (
            <PanelEmptyState
              icon={FileCode}
              title="No Rule Group Selected"
              description={
                groups.length === 0
                  ? 'Create a rule group to start intercepting and modifying requests.'
                  : 'Select a group from the sidebar to edit its rules.'
              }
              action={groups.length === 0 ? (
                <Button onClick={() => setCreateDialogOpen(true)}>
                  <Plus className="h-4 w-4 mr-2" />
                  Create Group
                </Button>
              ) : undefined}
              className="flex-1"
            />
          )}
        </div>

        {/* Status panel */}
        {statusPanelVisible && (
          <div
            className={cn(
              'flex min-h-0 shrink-0 flex-col border-l bg-background/75 transition-[width] duration-150',
              statusPanelCollapsed ? 'w-9' : 'w-64'
            )}
          >
            {statusPanelCollapsed ? (
              <>
                <div className="flex h-9 shrink-0 items-center justify-center border-b">
                  <Button
                    variant="ghost"
                    size="icon"
                    className="h-7 w-7 text-muted-foreground"
                    onClick={() => setStatusPanelCollapsed(false)}
                    aria-label="Expand status"
                    title="Expand status"
                  >
                    <ChevronLeft className="h-3.5 w-3.5" />
                  </Button>
                </div>
                <div className="flex justify-center pt-2">
                  {hasStatusErrors ? (
                    <XCircle className="h-3.5 w-3.5 text-red-500" aria-label="Rule errors" />
                  ) : hasStatusWarnings ? (
                    <AlertTriangle className="h-3.5 w-3.5 text-amber-500" aria-label="Rule warnings" />
                  ) : (
                    <CheckCircle className="h-3.5 w-3.5 text-emerald-500" aria-label="Rules valid" />
                  )}
                </div>
              </>
            ) : (
              <ParseStatus
                className="min-h-0 flex-1"
                onCollapse={() => setStatusPanelCollapsed(true)}
              />
            )}
          </div>
        )}
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
