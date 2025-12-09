import { FileCode, MoreVertical, Trash2, Edit, Power, PowerOff } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Switch } from '@/components/ui/switch';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { ScrollArea } from '@/components/ui/scroll-area';
import { useRulesStore, RuleGroup } from '@/stores/rules';
import { cn } from '@/lib/utils';

interface RuleGroupListProps {
  className?: string;
}

export function RuleGroupList({ className }: RuleGroupListProps) {
  const { groups, selectedGroupId, selectGroup, toggleGroup, deleteGroup } = useRulesStore();

  const handleToggle = async (e: React.MouseEvent, group: RuleGroup) => {
    e.stopPropagation();
    await toggleGroup(group.id, !group.enabled);
  };

  const handleDelete = async (group: RuleGroup) => {
    if (confirm(`Delete rule group "${group.name}"?`)) {
      await deleteGroup(group.id);
    }
  };

  if (groups.length === 0) {
    return (
      <div className={cn('flex flex-col items-center justify-center p-4 text-center', className)}>
        <FileCode className="h-8 w-8 text-muted-foreground/50 mb-2" />
        <p className="text-sm text-muted-foreground">No rule groups</p>
      </div>
    );
  }

  return (
    <ScrollArea className={className}>
      <div className="space-y-1 p-2">
        {groups.map((group) => (
          <div
            key={group.id}
            className={cn(
              'flex items-center gap-2 rounded-md px-2 py-1.5 cursor-pointer transition-colors',
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
              className="scale-75"
            />
            
            {/* Group info */}
            <div className="flex-1 min-w-0">
              <div className="flex items-center gap-1.5">
                <FileCode className={cn(
                  'h-3.5 w-3.5 shrink-0',
                  group.enabled ? 'text-emerald-500' : 'text-muted-foreground'
                )} />
                <span className={cn(
                  'text-sm font-medium truncate',
                  !group.enabled && 'text-muted-foreground'
                )}>
                  {group.name}
                </span>
              </div>
              <div className="text-xs text-muted-foreground">
                {group.rules.length} rule{group.rules.length !== 1 ? 's' : ''}
              </div>
            </div>
            
            {/* Actions menu */}
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button
                  variant="ghost"
                  size="icon"
                  className="h-6 w-6 shrink-0 opacity-0 group-hover:opacity-100 data-[state=open]:opacity-100"
                  onClick={(e) => e.stopPropagation()}
                >
                  <MoreVertical className="h-3.5 w-3.5" />
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end">
                <DropdownMenuItem onClick={() => selectGroup(group.id)}>
                  <Edit className="h-4 w-4 mr-2" />
                  Edit
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
                  onClick={() => handleDelete(group)}
                >
                  <Trash2 className="h-4 w-4 mr-2" />
                  Delete
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          </div>
        ))}
      </div>
    </ScrollArea>
  );
}
