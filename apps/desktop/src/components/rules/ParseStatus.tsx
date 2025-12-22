import { CheckCircle, XCircle } from 'lucide-react';
import { Badge } from '@/components/ui/badge';
import { ScrollArea } from '@/components/ui/scroll-area';
import { useRulesStore, Rule, RuleAction } from '@/stores/rules';
import { cn } from '@/lib/utils';

interface ParseStatusProps {
  className?: string;
}

export function ParseStatus({ className }: ParseStatusProps) {
  const { parseResult, selectedGroupId } = useRulesStore();

  if (!selectedGroupId) {
    return (
      <div className={cn('flex items-center justify-center px-3 py-2 text-muted-foreground', className)}>
        <span className="text-[11px]">Select a rule group</span>
      </div>
    );
  }

  if (!parseResult) {
    return (
      <div className={cn('flex items-center justify-center px-3 py-2', className)}>
        <span className="text-[11px] text-muted-foreground">Parsing...</span>
      </div>
    );
  }

  const { rules, errors } = parseResult;
  const hasErrors = errors.length > 0;

  return (
    <div className={cn('flex flex-col overflow-hidden', className)}>
      {/* Status header - match parent header: h-9 with px-3 inner content */}
      <div className="flex items-center justify-between h-9 px-3 border-b bg-muted/30">
        <div className="flex items-center gap-1.5">
          {hasErrors ? (
            <XCircle className="h-3 w-3 text-red-500 shrink-0" />
          ) : (
            <CheckCircle className="h-3 w-3 text-emerald-500 shrink-0" />
          )}
          <span className="text-xs font-medium">
            {hasErrors ? 'Errors' : 'Valid'}
          </span>
        </div>
        <div className="flex items-center gap-1">
          <Badge variant="secondary" className="text-[10px] px-1 py-0 h-4">
            {rules.length}
          </Badge>
          {hasErrors && (
            <Badge variant="destructive" className="text-[10px] px-1 py-0 h-4">
              {errors.length}
            </Badge>
          )}
        </div>
      </div>

      {/* Content - same px-3 as header for alignment */}
      <ScrollArea className="flex-1">
        {hasErrors ? (
          <div className="px-3 py-2 space-y-1.5">
            {errors.map((error, index) => (
              <div
                key={index}
                className="p-1.5 rounded bg-red-500/10 border border-red-500/20 overflow-hidden"
              >
                <div className="flex items-center gap-1">
                  <span className="text-[10px] font-medium text-red-600 dark:text-red-400 shrink-0">
                    L{error.line}
                  </span>
                  <span className="text-[10px] text-muted-foreground truncate flex-1 w-0" title={error.message}>
                    {error.message}
                  </span>
                </div>
                {error.content && (
                  <code className="text-[10px] bg-muted px-1 py-0.5 rounded mt-1 block truncate" title={error.content}>
                    {error.content}
                  </code>
                )}
              </div>
            ))}
          </div>
        ) : rules.length > 0 ? (
          <div className="px-3 py-2 space-y-1.5">
            {rules.map((rule, index) => (
              <RuleItem key={rule.id || index} rule={rule} />
            ))}
          </div>
        ) : (
          <div className="flex flex-col items-center justify-center px-3 py-4 text-center">
            <p className="text-[11px] text-muted-foreground">No rules</p>
          </div>
        )}
      </ScrollArea>
    </div>
  );
}

// Separate component for each rule item with detailed breakdown
function RuleItem({ rule }: { rule: Rule }) {
  const source = formatSource(rule);
  const target = formatTarget(rule.actions);
  const actionType = getMainActionType(rule.actions);

  return (
    <div className="p-1.5 rounded bg-muted/50 space-y-0.5 overflow-hidden">
      {/* Source (pattern) */}
      <div className="flex items-center gap-1.5">
        <span className="text-[9px] text-muted-foreground shrink-0 w-9">source</span>
        <span className="text-[10px] font-mono truncate flex-1 w-0" title={source}>
          {source}
        </span>
      </div>
      
      {/* Target (action destination) */}
      {target && (
        <div className="flex items-center gap-1.5">
          <span className="text-[9px] text-muted-foreground shrink-0 w-9">target</span>
          <span className="text-[10px] font-mono truncate flex-1 w-0 text-blue-600 dark:text-blue-400" title={target}>
            {target}
          </span>
        </div>
      )}
      
      {/* Action type */}
      <div className="flex items-center gap-1.5">
        <span className="text-[9px] text-muted-foreground shrink-0 w-9">action</span>
        <div className="flex items-center gap-0.5 flex-1 w-0 overflow-hidden">
          <Badge variant="outline" className="text-[9px] px-1 py-0 h-3.5 shrink-0">
            {actionType}
          </Badge>
          {rule.filters && Object.keys(rule.filters).some(k => {
            const v = rule.filters?.[k as keyof typeof rule.filters];
            return Array.isArray(v) ? v.length > 0 : !!v;
          }) && (
            <Badge variant="secondary" className="text-[9px] px-1 py-0 h-3.5 text-purple-600 dark:text-purple-400 shrink-0">
              filtered
            </Badge>
          )}
        </div>
      </div>
    </div>
  );
}

// Format the source pattern for display
function formatSource(rule: Rule): string {
  const { pattern } = rule;
  
  // If we have the raw line, extract just the source part
  if (rule.rawLine) {
    const parts = rule.rawLine.trim().split(/\s+/);
    if (parts.length > 0 && !parts[0].startsWith('#')) {
      return parts[0];
    }
  }
  
  // Build from pattern
  if (pattern.value) return pattern.value;
  
  let source = '';
  if (pattern.protocol) source += `${pattern.protocol}://`;
  if (pattern.host) source += pattern.host;
  if (pattern.path) source += pattern.path;
  
  return source || pattern.type;
}

// Format the target (destination) from actions
function formatTarget(actions: RuleAction[]): string | null {
  if (!actions || actions.length === 0) return null;
  
  for (const action of actions) {
    // Host redirection
    if (action.type === 'Host' && action.target) {
      return String(action.target);
    }
    
    // File serving
    if (action.type === 'File' && action.path) {
      return String(action.path);
    }
    
    // Mock data
    if (action.type === 'ResBody' && action.content) {
      const content = String(action.content);
      return content.length > 30 ? content.slice(0, 30) + '...' : content;
    }
    
    // Status code
    if (action.type === 'StatusCode' && action.code) {
      return `HTTP ${action.code}`;
    }
    
    // JSON body
    if (action.type === 'JsonBody' && action.json) {
      const json = JSON.stringify(action.json);
      return json.length > 30 ? json.slice(0, 30) + '...' : json;
    }
    
    // Delay
    if (action.type === 'Delay' && action.ms) {
      return `${action.ms}ms`;
    }
    
    // Headers
    if (action.type === 'ReqHeaders' || action.type === 'ResHeaders') {
      return 'modify headers';
    }
  }
  
  return null;
}

// Get the main action type for display
function getMainActionType(actions: RuleAction[]): string {
  if (!actions || actions.length === 0) return 'none';
  
  // Return the first meaningful action type
  const mainAction = actions[0];
  
  // Map to friendly names
  const typeMap: Record<string, string> = {
    'Host': 'proxy',
    'File': 'file',
    'ResBody': 'body',
    'StatusCode': 'status',
    'JsonBody': 'json',
    'Delay': 'delay',
    'ReqHeaders': 'req-hdr',
    'ResHeaders': 'res-hdr',
    'Log': 'log',
    'Disable': 'disable',
  };
  
  return typeMap[mainAction.type] || mainAction.type.toLowerCase();
}
