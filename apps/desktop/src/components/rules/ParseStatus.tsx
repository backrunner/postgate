import { AlertTriangle, ArrowRight, CheckCircle, ChevronRight, ListFilter, Loader2, XCircle } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { ScrollArea } from '@/components/ui/scroll-area';
import { useRulesStore, Rule, RuleAction, ParseError } from '@/stores/rules';
import { cn } from '@/lib/utils';

interface ParseStatusProps {
  className?: string;
  onCollapse: () => void;
}

export function ParseStatus({ className, onCollapse }: ParseStatusProps) {
  const { parseResult, selectedGroupId } = useRulesStore();

  if (!selectedGroupId) {
    return null;
  }

  const rules = parseResult?.rules ?? [];
  const errors = parseResult?.errors ?? [];
  const warnings = parseResult?.warnings ?? [];
  const isParsing = !parseResult;
  const hasErrors = errors.length > 0;
  const hasWarnings = warnings.length > 0;
  const summary = isParsing
    ? 'Parsing'
    : hasErrors
      ? `${errors.length} ${errors.length === 1 ? 'error' : 'errors'}`
      : hasWarnings
        ? `${warnings.length} ${warnings.length === 1 ? 'warning' : 'warnings'}`
        : `${rules.length} ${rules.length === 1 ? 'rule' : 'rules'}`;

  return (
    <section className={cn('flex min-h-0 flex-col overflow-hidden bg-background', className)}>
      <div className="flex h-9 shrink-0 items-center gap-1.5 border-b px-2">
        {isParsing ? (
          <Loader2 className="h-3.5 w-3.5 shrink-0 animate-spin text-muted-foreground" />
        ) : hasErrors ? (
          <XCircle className="h-3.5 w-3.5 shrink-0 text-red-500" />
        ) : hasWarnings ? (
          <AlertTriangle className="h-3.5 w-3.5 shrink-0 text-amber-500" />
        ) : (
          <CheckCircle className="h-3.5 w-3.5 shrink-0 text-emerald-500" />
        )}
        <span className="text-xs font-medium text-foreground">Status</span>
        <span className="min-w-0 flex-1 truncate text-[10px] text-muted-foreground">
          {summary}
        </span>
        <Button
          variant="ghost"
          size="icon"
          className="h-7 w-7 shrink-0 text-muted-foreground"
          onClick={onCollapse}
          aria-label="Collapse status"
          title="Collapse status"
        >
          <ChevronRight className="h-3.5 w-3.5" />
        </Button>
      </div>

      {!isParsing && (
        <ScrollArea className="min-h-0 flex-1">
          {(hasErrors || hasWarnings) ? (
            <div className="divide-y divide-border/60">
              {errors.map((error, index) => (
                <DiagnosticItem key={`error-${index}`} diagnostic={error} tone="error" />
              ))}
              {warnings.map((warning, index) => (
                <DiagnosticItem key={`warning-${index}`} diagnostic={warning} tone="warning" />
              ))}
            </div>
          ) : (
            <div className="divide-y divide-border/60">
            {rules.map((rule, index) => (
              <RuleItem key={rule.id || index} rule={rule} />
            ))}
            </div>
          )}
        </ScrollArea>
      )}
    </section>
  );
}

function DiagnosticItem({
  diagnostic,
  tone,
}: {
  diagnostic: ParseError;
  tone: 'error' | 'warning';
}) {
  const lineClass = tone === 'error'
    ? 'text-red-600 dark:text-red-400'
    : 'text-amber-600 dark:text-amber-400';

  return (
    <div className="px-2.5 py-2">
      <div className="flex min-w-0 items-start gap-2">
        <span className={cn('shrink-0 text-[10px] font-semibold leading-4', lineClass)}>
          L{diagnostic.line}
        </span>
        <p className="min-w-0 flex-1 break-words text-[10px] leading-4 text-foreground/80">
          {diagnostic.message}
        </p>
      </div>
      {diagnostic.content && (
        <code
          className="mt-1 block max-h-9 overflow-hidden break-all rounded-sm bg-muted/60 px-1.5 py-1 text-[9px] leading-3.5 text-muted-foreground"
          title={diagnostic.content}
        >
          {diagnostic.content}
        </code>
      )}
    </div>
  );
}

function RuleItem({ rule }: { rule: Rule }) {
  const source = formatSource(rule);
  const target = formatTarget(rule.actions);
  const actionType = getMainActionType(rule.actions);
  const isFiltered = !!rule.filters && Object.keys(rule.filters).some((key) => {
    const value = rule.filters?.[key as keyof typeof rule.filters];
    return Array.isArray(value) ? value.length > 0 : !!value;
  });

  return (
    <div className="px-2.5 py-2 hover:bg-muted/25">
      <div className="flex min-w-0 items-center gap-1.5">
        <span className="min-w-0 flex-1 truncate font-mono text-[10px] text-foreground" title={source}>
          {source}
        </span>
        {isFiltered && (
          <ListFilter className="h-3 w-3 shrink-0 text-muted-foreground" aria-label="Filtered rule" />
        )}
        <span className="shrink-0 rounded-sm border border-border/70 px-1 py-0.5 text-[9px] leading-none text-muted-foreground">
          {actionType}
        </span>
      </div>

      {target && (
        <div className="mt-0.5 flex min-w-0 items-center gap-1 text-[9px] leading-4">
          <ArrowRight className="h-2.5 w-2.5 shrink-0 text-muted-foreground" />
          <span className="min-w-0 flex-1 truncate font-mono text-blue-600 dark:text-blue-400" title={target}>
            {target}
          </span>
        </div>
      )}
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
