import { cn } from '@/lib/utils';

interface MatchedRulesDisplayProps {
  rules: string[];
  className?: string;
}

interface ParsedRule {
  source: string;
  target: string | null;
  action: string | null;
  raw: string;
}

// Parse a rule line into source, target, and action
function parseRule(ruleLine: string): ParsedRule {
  const raw = ruleLine.trim();
  
  // Skip comments
  if (raw.startsWith('#') || raw.startsWith('//')) {
    return { source: raw, target: null, action: null, raw };
  }
  
  // Split by whitespace
  const parts = raw.split(/\s+/).filter(Boolean);
  
  if (parts.length === 0) {
    return { source: raw, target: null, action: null, raw };
  }
  
  const source = parts[0];
  let target: string | null = null;
  let action: string | null = null;
  
  // Process remaining parts to find target and action
  for (let i = 1; i < parts.length; i++) {
    const part = parts[i];
    
    // Filter operators (m:, p:, port:, etc.) - these are modifiers, not targets
    if (/^(m|method|i|ip|p|protocol|port|h|host|s|statusCode|ct|contentType):/.test(part)) {
      continue;
    }
    
    // Action with :// (host://, file://, etc.)
    const actionMatch = part.match(/^([a-zA-Z][a-zA-Z0-9-]*):\/\/(.*)$/);
    if (actionMatch) {
      action = actionMatch[1];
      target = actionMatch[2] || null;
      break;
    }
    
    // Bare URL as target (http://... or https://...)
    if (part.match(/^https?:\/\//)) {
      target = part;
      action = 'redirect';
      break;
    }
    
    // IP:port as target
    if (part.match(/^\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}:\d+$/)) {
      target = part;
      action = 'host';
      break;
    }
  }
  
  return { source, target, action, raw };
}

export function MatchedRulesDisplay({ rules, className }: MatchedRulesDisplayProps) {
  if (rules.length === 0) {
    return null;
  }

  const parsedRules = rules.map(parseRule);

  return (
    <div className={cn('space-y-1.5', className)}>
      {parsedRules.map((rule, index) => (
        <div 
          key={index} 
          className="bg-indigo-50/50 dark:bg-indigo-950/30 border border-indigo-200/50 dark:border-indigo-800/50 rounded p-2 text-xs"
        >
          {/* Source row */}
          <div className="flex items-center gap-2 min-w-0">
            <span className="text-[10px] text-muted-foreground shrink-0 w-12 uppercase">source</span>
            <span 
              className="font-mono text-indigo-700 dark:text-indigo-300 truncate flex-1 w-0"
              title={rule.source}
            >
              {rule.source}
            </span>
          </div>
          
          {/* Target row */}
          {rule.target && (
            <div className="flex items-center gap-2 min-w-0 mt-0.5">
              <span className="text-[10px] text-muted-foreground shrink-0 w-12 uppercase">target</span>
              <span 
                className="font-mono text-amber-600 dark:text-amber-400 truncate flex-1 w-0"
                title={rule.target}
              >
                {rule.target}
              </span>
            </div>
          )}
          
          {/* Action row */}
          {rule.action && (
            <div className="flex items-center gap-2 min-w-0 mt-0.5">
              <span className="text-[10px] text-muted-foreground shrink-0 w-12 uppercase">action</span>
              <span className="font-mono text-blue-600 dark:text-blue-400">
                {rule.action}
              </span>
            </div>
          )}
        </div>
      ))}
    </div>
  );
}
