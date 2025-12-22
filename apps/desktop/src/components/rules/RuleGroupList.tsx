import { useState, useMemo } from 'react';
import { MoreVertical, Trash2, Edit, Power, PowerOff, Pencil } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Switch } from '@/components/ui/switch';
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from '@/components/ui/context-menu';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
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
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { ScrollArea } from '@/components/ui/scroll-area';
import { useRulesStore, RuleGroup } from '@/stores/rules';
import { cn } from '@/lib/utils';

interface RuleGroupListProps {
  className?: string;
  filter?: string;
}

export function RuleGroupList({ className, filter }: RuleGroupListProps) {
  const { groups, selectedGroupId, selectGroup, toggleGroup, deleteGroup, renameGroup } = useRulesStore();
  
  // Filter groups
  const filteredGroups = useMemo(() => {
    if (!filter) return groups;
    return groups.filter(g => g.name.toLowerCase().includes(filter.toLowerCase()));
  }, [groups, filter]);
  
  // Dialog states
  const [deleteDialogOpen, setDeleteDialogOpen] = useState(false);
  const [renameDialogOpen, setRenameDialogOpen] = useState(false);
  const [targetGroup, setTargetGroup] = useState<RuleGroup | null>(null);
  const [newName, setNewName] = useState('');

  const handleToggle = async (e: React.MouseEvent, group: RuleGroup) => {
    e.stopPropagation();
    await toggleGroup(group.id, !group.enabled);
  };

  const openDeleteDialog = (group: RuleGroup) => {
    setTargetGroup(group);
    setDeleteDialogOpen(true);
  };

  const handleDelete = async () => {
    if (targetGroup) {
      await deleteGroup(targetGroup.id);
      setDeleteDialogOpen(false);
      setTargetGroup(null);
    }
  };

  const openRenameDialog = (group: RuleGroup) => {
    setTargetGroup(group);
    setNewName(group.name);
    setRenameDialogOpen(true);
  };

  const handleRename = async () => {
    if (targetGroup && newName.trim()) {
      await renameGroup(targetGroup.id, newName.trim());
      setRenameDialogOpen(false);
      setTargetGroup(null);
      setNewName('');
    }
  };

  if (filteredGroups.length === 0) {
    return (
      <div className={cn('flex flex-col items-center justify-center p-3 text-center', className)}>
        <p className="text-xs text-muted-foreground">
          {filter ? 'No matching groups' : 'No rule groups'}
        </p>
      </div>
    );
  }

  const renderGroupItem = (group: RuleGroup) => (
    <div
      className={cn(
        'flex items-center gap-1.5 rounded px-1.5 py-1 cursor-pointer transition-colors group',
        selectedGroupId === group.id
          ? 'bg-accent text-accent-foreground'
          : 'hover:bg-muted/50'
      )}
      onClick={() => selectGroup(group.id)}
    >
      {/* Toggle switch */}
      <Switch
        checked={group.enabled}
        onClick={(e) => handleToggle(e, group)}
        className="scale-[0.7] shrink-0"
      />
      
      {/* Group info - use w-0 to force truncation */}
      <div className="flex-1 w-0">
        <span 
          className={cn(
            'text-xs font-medium block truncate',
            !group.enabled && 'text-muted-foreground'
          )}
          title={group.name}
        >
          {group.name}
        </span>
      </div>
      
      {/* Actions menu */}
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button
            variant="ghost"
            size="icon"
            className="h-5 w-5 shrink-0 opacity-0 group-hover:opacity-100 data-[state=open]:opacity-100"
            onClick={(e) => e.stopPropagation()}
          >
            <MoreVertical className="h-3 w-3" />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end">
          <DropdownMenuItem onClick={() => selectGroup(group.id)}>
            <Edit className="h-4 w-4 mr-2" />
            Edit
          </DropdownMenuItem>
          <DropdownMenuItem onClick={() => openRenameDialog(group)}>
            <Pencil className="h-4 w-4 mr-2" />
            Rename
          </DropdownMenuItem>
          <DropdownMenuItem onClick={() => toggleGroup(group.id, !group.enabled)}>
            {group.enabled ? (
              <>
                <PowerOff className="h-4 w-4 mr-2" />
                Disable
              </>
            ) : (
              <>
                <Power className="h-4 w-4 mr-2" />
                Enable
              </>
            )}
          </DropdownMenuItem>
          <DropdownMenuSeparator />
          <DropdownMenuItem
            className="text-destructive focus:text-destructive"
            onClick={() => openDeleteDialog(group)}
          >
            <Trash2 className="h-4 w-4 mr-2" />
            Delete
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  );

  return (
    <>
      <ScrollArea className={className}>
        <div className="space-y-0.5 p-1">
          {filteredGroups.map((group) => (
            <ContextMenu key={group.id}>
              <ContextMenuTrigger asChild>
                {renderGroupItem(group)}
              </ContextMenuTrigger>
              <ContextMenuContent>
                <ContextMenuItem onClick={() => selectGroup(group.id)}>
                  <Edit className="h-4 w-4 mr-2" />
                  Edit
                </ContextMenuItem>
                <ContextMenuItem onClick={() => openRenameDialog(group)}>
                  <Pencil className="h-4 w-4 mr-2" />
                  Rename
                </ContextMenuItem>
                <ContextMenuItem onClick={() => toggleGroup(group.id, !group.enabled)}>
                  {group.enabled ? (
                    <>
                      <PowerOff className="h-4 w-4 mr-2" />
                      Disable
                    </>
                  ) : (
                    <>
                      <Power className="h-4 w-4 mr-2" />
                      Enable
                    </>
                  )}
                </ContextMenuItem>
                <ContextMenuSeparator />
                <ContextMenuItem
                  className="text-destructive focus:text-destructive"
                  onClick={() => openDeleteDialog(group)}
                >
                  <Trash2 className="h-4 w-4 mr-2" />
                  Delete
                </ContextMenuItem>
              </ContextMenuContent>
            </ContextMenu>
          ))}
        </div>
      </ScrollArea>

      {/* Delete Confirmation Dialog */}
      <AlertDialog open={deleteDialogOpen} onOpenChange={setDeleteDialogOpen}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Delete Rule Group</AlertDialogTitle>
            <AlertDialogDescription>
              Are you sure you want to delete "{targetGroup?.name}"? This action cannot be undone.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction
              onClick={handleDelete}
              className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
            >
              Delete
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      {/* Rename Dialog */}
      <Dialog open={renameDialogOpen} onOpenChange={setRenameDialogOpen}>
        <DialogContent className="sm:max-w-[425px]">
          <DialogHeader>
            <DialogTitle>Rename Rule Group</DialogTitle>
            <DialogDescription>
              Enter a new name for this rule group.
            </DialogDescription>
          </DialogHeader>
          <div className="py-4">
            <Input
              value={newName}
              onChange={(e) => setNewName(e.target.value)}
              placeholder="Group name"
              onKeyDown={(e) => {
                if (e.key === 'Enter' && newName.trim()) {
                  handleRename();
                }
              }}
              autoFocus
            />
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setRenameDialogOpen(false)}>
              Cancel
            </Button>
            <Button onClick={handleRename} disabled={!newName.trim()}>
              Rename
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}
