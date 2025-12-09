import { useEffect, useState } from 'react';
import { FileCode, Plus, Save, Undo2, Book, ChevronLeft, ChevronRight } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Separator } from '@/components/ui/separator';
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip';
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
    loadGroups,
    createGroup,
    saveCurrentGroup,
    discardChanges,
  } = useRulesStore();

  const [newGroupName, setNewGroupName] = useState('');
  const [isCreating, setIsCreating] = useState(false);
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);
  const [statusPanelCollapsed, setStatusPanelCollapsed] = useState(false);

  // Load groups on mount
  useEffect(() => {
    loadGroups();
  }, [loadGroups]);

  const selectedGroup = groups.find(g => g.id === selectedGroupId);

  const handleCreateGroup = async () => {
    if (!newGroupName.trim()) return;
    
    setIsCreating(true);
    try {
      const group = await createGroup(newGroupName.trim());
      useRulesStore.getState().selectGroup(group.id);
      setNewGroupName('');
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

  return (
    <div className="flex h-full flex-col bg-background">
      {/* Toolbar */}
      <div className="flex h-12 items-center justify-between border-b px-4 bg-muted/10">
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
          <Separator orientation="vertical" className="h-6 mx-2" />
          <Tooltip>
            <TooltipTrigger asChild>
              <Button
                variant="ghost"
                size="sm"
                className="h-8 px-2 gap-1.5 text-xs text-muted-foreground hover:text-foreground"
                onClick={() => window.open('https://wproxy.org/whistle/', '_blank')}
              >
                <Book className="h-3.5 w-3.5" />
                Docs
              </Button>
            </TooltipTrigger>
            <TooltipContent>Whistle documentation</TooltipContent>
          </Tooltip>
        </div>
      </div>

      {/* Content */}
      <div className="flex flex-1 overflow-hidden">
        {/* Sidebar - Rule Groups */}
        <div
          className={cn(
            'border-r flex flex-col transition-all duration-300 ease-in-out bg-muted/10',
            sidebarCollapsed ? 'w-[40px]' : 'w-60'
          )}
        >
          {!sidebarCollapsed && (
            <>
              {/* New group input */}
              <div className="p-3 border-b">
                <div className="relative">
                  <Input
                    placeholder="New group..."
                    value={newGroupName}
                    onChange={(e) => setNewGroupName(e.target.value)}
                    className="h-8 text-xs pr-8 bg-background"
                    onKeyDown={(e) => e.key === "Enter" && handleCreateGroup()}
                  />
                  <Button
                    size="icon"
                    variant="ghost"
                    className="absolute right-0 top-0 h-8 w-8 hover:bg-transparent"
                    onClick={handleCreateGroup}
                    disabled={!newGroupName.trim() || isCreating}
                  >
                    <Plus className="h-3.5 w-3.5 text-muted-foreground hover:text-foreground" />
                  </Button>
                </div>
              </div>

              {/* Groups list */}
              <RuleGroupList className="flex-1 p-2" />
            </>
          )}

          {/* Spacer */}
          <div className="flex-1" />

          {/* Collapse toggle */}
          <Button
            variant="ghost"
            size="sm"
            className={cn("h-8 rounded-none border-t text-muted-foreground hover:text-foreground", sidebarCollapsed && "justify-center px-0")}
            onClick={() => setSidebarCollapsed(!sidebarCollapsed)}
          >
            {sidebarCollapsed ? (
              <ChevronRight className="h-3.5 w-3.5" />
            ) : (
              <div className="flex items-center gap-2 w-full">
                <ChevronLeft className="h-3.5 w-3.5" />
                <span className="text-xs">Collapse</span>
              </div>
            )}
          </Button>
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
                <div className="flex gap-2 max-w-xs w-full">
                  <Input
                    placeholder="Group name..."
                    value={newGroupName}
                    onChange={(e) => setNewGroupName(e.target.value)}
                    className="h-9"
                  />
                  <Button
                    onClick={handleCreateGroup}
                    disabled={!newGroupName.trim() || isCreating}
                  >
                    Create
                  </Button>
                </div>
              )}
            </div>
          )}
        </div>

        {/* Status panel */}
        <div
          className={cn(
            'border-l flex flex-col transition-all duration-300 ease-in-out bg-muted/5',
            statusPanelCollapsed ? 'w-[40px]' : 'w-72'
          )}
        >
           {/* Collapse toggle */}
           <div className="flex items-center h-10 border-b px-2">
            <Button
              variant="ghost"
              size="sm"
              className={cn("h-8 w-full justify-start gap-2 text-muted-foreground hover:text-foreground px-2", statusPanelCollapsed && "justify-center px-0")}
              onClick={() => setStatusPanelCollapsed(!statusPanelCollapsed)}
            >
              {statusPanelCollapsed ? (
                <ChevronLeft className="h-3.5 w-3.5" />
              ) : (
                <>
                  <ChevronRight className="h-3.5 w-3.5" />
                  <span className="text-xs font-medium">Hide Status</span>
                </>
              )}
            </Button>
          </div>

          {!statusPanelCollapsed && (
            <ParseStatus className="flex-1 p-4" />
          )}
        </div>
      </div>
    </div>
  );
}
