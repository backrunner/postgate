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
    <div className="flex h-full flex-col">
      {/* Toolbar */}
      <div className="flex h-10 items-center justify-between border-b px-4">
        <div className="flex items-center gap-2">
          <h2 className="text-sm font-semibold">Rules</h2>
          {selectedGroup && (
            <>
              <Separator orientation="vertical" className="h-4" />
              <span className="text-sm text-muted-foreground">{selectedGroup.name}</span>
              {isDirty && (
                <span className="text-xs text-amber-500">(unsaved)</span>
              )}
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
                    className="gap-1"
                    onClick={discardChanges}
                    disabled={!isDirty}
                  >
                    <Undo2 className="h-4 w-4" />
                  </Button>
                </TooltipTrigger>
                <TooltipContent>Discard changes</TooltipContent>
              </Tooltip>
              <Tooltip>
                <TooltipTrigger asChild>
                  <Button
                    variant="ghost"
                    size="sm"
                    className="gap-1"
                    onClick={handleSave}
                    disabled={!isDirty}
                  >
                    <Save className="h-4 w-4" />
                    Save
                  </Button>
                </TooltipTrigger>
                <TooltipContent>Save changes (Cmd+S)</TooltipContent>
              </Tooltip>
            </>
          )}
          <Separator orientation="vertical" className="h-4 mx-1" />
          <Tooltip>
            <TooltipTrigger asChild>
              <Button
                variant="ghost"
                size="sm"
                className="gap-1"
                onClick={() => window.open('https://wproxy.org/whistle/', '_blank')}
              >
                <Book className="h-4 w-4" />
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
            'border-r flex flex-col transition-all duration-200',
            sidebarCollapsed ? 'w-10' : 'w-60'
          )}
        >
          {!sidebarCollapsed && (
            <>
              {/* New group input */}
              <div className="p-2 border-b">
                <form
                  onSubmit={(e) => {
                    e.preventDefault();
                    handleCreateGroup();
                  }}
                  className="flex gap-1"
                >
                  <Input
                    placeholder="New group name..."
                    value={newGroupName}
                    onChange={(e) => setNewGroupName(e.target.value)}
                    className="h-8 text-sm"
                  />
                  <Button
                    type="submit"
                    size="icon"
                    className="h-8 w-8 shrink-0"
                    disabled={!newGroupName.trim() || isCreating}
                  >
                    <Plus className="h-4 w-4" />
                  </Button>
                </form>
              </div>

              {/* Groups list */}
              <RuleGroupList className="flex-1" />
            </>
          )}

          {/* Collapse toggle */}
          <Button
            variant="ghost"
            size="icon"
            className="h-8 w-full rounded-none border-t"
            onClick={() => setSidebarCollapsed(!sidebarCollapsed)}
          >
            {sidebarCollapsed ? (
              <ChevronRight className="h-4 w-4" />
            ) : (
              <ChevronLeft className="h-4 w-4" />
            )}
          </Button>
        </div>

        {/* Main editor area */}
        <div className="flex-1 flex flex-col min-w-0">
          {selectedGroupId ? (
            <RuleEditor className="flex-1" />
          ) : (
            <div className="flex-1 flex items-center justify-center">
              <div className="text-center text-muted-foreground">
                <FileCode className="mx-auto h-12 w-12 mb-4 opacity-50" />
                <h3 className="font-semibold mb-1">
                  {groups.length === 0 ? 'No rule groups yet' : 'Select a rule group'}
                </h3>
                <p className="text-sm mb-4">
                  {groups.length === 0
                    ? 'Create a rule group to start intercepting requests'
                    : 'Choose a group from the sidebar to edit'}
                </p>
                {groups.length === 0 && (
                  <div className="flex justify-center gap-2">
                    <Input
                      placeholder="Group name..."
                      value={newGroupName}
                      onChange={(e) => setNewGroupName(e.target.value)}
                      className="w-48"
                    />
                    <Button
                      onClick={handleCreateGroup}
                      disabled={!newGroupName.trim() || isCreating}
                    >
                      <Plus className="h-4 w-4 mr-1" />
                      Create
                    </Button>
                  </div>
                )}
              </div>
            </div>
          )}
        </div>

        {/* Status panel */}
        <div
          className={cn(
            'border-l flex flex-col transition-all duration-200',
            statusPanelCollapsed ? 'w-10' : 'w-72'
          )}
        >
          {/* Collapse toggle */}
          <Button
            variant="ghost"
            size="icon"
            className="h-8 w-full rounded-none border-b"
            onClick={() => setStatusPanelCollapsed(!statusPanelCollapsed)}
          >
            {statusPanelCollapsed ? (
              <ChevronLeft className="h-4 w-4" />
            ) : (
              <ChevronRight className="h-4 w-4" />
            )}
          </Button>

          {!statusPanelCollapsed && (
            <ParseStatus className="flex-1" />
          )}
        </div>
      </div>
    </div>
  );
}
