import { CheckCircle, XCircle, AlertCircle, FileCode } from 'lucide-react';
import { Badge } from '@/components/ui/badge';
import { ScrollArea } from '@/components/ui/scroll-area';
import { useRulesStore } from '@/stores/rules';
import { cn } from '@/lib/utils';

interface ParseStatusProps {
  className?: string;
}

export function ParseStatus({ className }: ParseStatusProps) {
  const { parseResult, selectedGroupId } = useRulesStore();

  if (!selectedGroupId) {
    return (
      <div className={cn('flex items-center justify-center p-4 text-muted-foreground', className)}>
        <span className="text-sm">Select a rule group to edit</span>
      </div>
    );
  }

  if (!parseResult) {
    return (
      <div className={cn('flex items-center justify-center p-4', className)}>
        <span className="text-sm text-muted-foreground">Parsing...</span>
      </div>
    );
  }

  const { rules, errors } = parseResult;
  const hasErrors = errors.length > 0;

  return (
    <div className={cn('flex flex-col', className)}>
      {/* Status header */}
      <div className="flex items-center justify-between px-3 py-2 border-b bg-muted/30">
        <div className="flex items-center gap-2">
          {hasErrors ? (
            <XCircle className="h-4 w-4 text-red-500" />
          ) : (
            <CheckCircle className="h-4 w-4 text-emerald-500" />
          )}
          <span className="text-sm font-medium">
            {hasErrors ? 'Parse Errors' : 'Valid'}
          </span>
        </div>
        <div className="flex items-center gap-2">
          <Badge variant="secondary" className="text-xs">
            {rules.length} rule{rules.length !== 1 ? 's' : ''}
          </Badge>
          {hasErrors && (
            <Badge variant="destructive" className="text-xs">
              {errors.length} error{errors.length !== 1 ? 's' : ''}
            </Badge>
          )}
        </div>
      </div>

      {/* Content */}
      <ScrollArea className="flex-1">
        {hasErrors ? (
          <div className="p-2 space-y-2">
            {errors.map((error, index) => (
              <div
                key={index}
                className="flex items-start gap-2 p-2 rounded-md bg-red-500/10 border border-red-500/20"
              >
                <AlertCircle className="h-4 w-4 text-red-500 shrink-0 mt-0.5" />
                <div className="min-w-0 flex-1">
                  <div className="text-sm font-medium text-red-600 dark:text-red-400">
                    Line {error.line}
                  </div>
                  <div className="text-xs text-muted-foreground mt-0.5">
                    {error.message}
                  </div>
                  {error.content && (
                    <code className="text-xs bg-muted px-1 py-0.5 rounded mt-1 block truncate">
                      {error.content}
                    </code>
                  )}
                </div>
              </div>
            ))}
          </div>
        ) : rules.length > 0 ? (
          <div className="p-2 space-y-1">
            {rules.map((rule, index) => (
              <div
                key={rule.id || index}
                className="flex items-center gap-2 p-2 rounded-md bg-muted/50"
              >
                <FileCode className="h-3.5 w-3.5 text-muted-foreground shrink-0" />
                <div className="min-w-0 flex-1">
                  <div className="text-xs font-mono truncate">
                    {rule.rawLine || formatPattern(rule.pattern)}
                  </div>
                  <div className="flex items-center gap-1 mt-0.5">
                    <Badge variant="outline" className="text-[10px] px-1 py-0">
                      {rule.pattern.type}
                    </Badge>
                    {rule.actions.map((action, i) => (
                      <Badge
                        key={i}
                        variant="secondary"
                        className="text-[10px] px-1 py-0"
                      >
                        {action.type}
                      </Badge>
                    ))}
                    {rule.filters && (
                      <Badge variant="outline" className="text-[10px] px-1 py-0 text-purple-600">
                        filtered
                      </Badge>
                    )}
                  </div>
                </div>
              </div>
            ))}
          </div>
        ) : (
          <div className="flex flex-col items-center justify-center p-8 text-center">
            <FileCode className="h-8 w-8 text-muted-foreground/50 mb-2" />
            <p className="text-sm text-muted-foreground">No rules defined</p>
            <p className="text-xs text-muted-foreground mt-1">
              Add rules using whistle syntax
            </p>
          </div>
        )}
      </ScrollArea>
    </div>
  );
}

function formatPattern(pattern: { type: string; value?: string; host?: string }): string {
  if (pattern.value) return pattern.value;
  if (pattern.host) return pattern.host;
  return pattern.type;
}
