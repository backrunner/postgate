import { useMemo } from 'react';
import { cn, formatDuration } from '@/lib/utils';
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip';

export interface TimingData {
  // Connection timing
  dns?: number;
  connect?: number;
  tls?: number;
  // Request timing
  blocked?: number;
  send?: number;
  wait?: number;
  receive?: number;
  // Total
  total: number;
}

interface TimingWaterfallProps {
  timing: TimingData;
  className?: string;
}

const TIMING_PHASES = [
  { key: 'blocked', label: 'Blocked', color: 'bg-zinc-400', description: 'Time spent in queue' },
  { key: 'dns', label: 'DNS', color: 'bg-cyan-500', description: 'DNS resolution time' },
  { key: 'connect', label: 'Connect', color: 'bg-orange-500', description: 'TCP connection time' },
  { key: 'tls', label: 'TLS', color: 'bg-purple-500', description: 'TLS handshake time' },
  { key: 'send', label: 'Send', color: 'bg-emerald-500', description: 'Request send time' },
  { key: 'wait', label: 'Wait', color: 'bg-sky-500', description: 'Time to first byte (TTFB)' },
  { key: 'receive', label: 'Receive', color: 'bg-blue-500', description: 'Response receive time' },
] as const;

export function TimingWaterfall({ timing, className }: TimingWaterfallProps) {
  const phases = useMemo(() => {
    const total = timing.total || 1;
    let offset = 0;
    
    return TIMING_PHASES.map(phase => {
      const value = timing[phase.key as keyof TimingData] as number | undefined;
      if (value === undefined || value <= 0) {
        return null;
      }
      
      const width = (value / total) * 100;
      const left = (offset / total) * 100;
      offset += value;
      
      return {
        ...phase,
        value,
        width,
        left,
      };
    }).filter(Boolean);
  }, [timing]);

  return (
    <div className={cn('space-y-4', className)}>
      {/* Waterfall bar */}
      <div className="space-y-2">
        <div className="flex items-center justify-between text-xs text-muted-foreground">
          <span>0ms</span>
          <span>{formatDuration(timing.total)}</span>
        </div>
        <div className="relative h-8 bg-muted/50 rounded overflow-hidden">
          {phases.map((phase) => (
            <Tooltip key={phase!.key}>
              <TooltipTrigger asChild>
                <div
                  className={cn(
                    'absolute top-0 bottom-0 transition-opacity hover:opacity-80 cursor-pointer',
                    phase!.color
                  )}
                  style={{
                    left: `${phase!.left}%`,
                    width: `${Math.max(phase!.width, 0.5)}%`,
                  }}
                />
              </TooltipTrigger>
              <TooltipContent>
                <div className="text-xs">
                  <div className="font-medium">{phase!.label}</div>
                  <div className="text-muted-foreground">{phase!.description}</div>
                  <div className="mt-1 font-mono">{formatDuration(phase!.value)}</div>
                </div>
              </TooltipContent>
            </Tooltip>
          ))}
        </div>
      </div>

      {/* Legend */}
      <div className="grid grid-cols-2 sm:grid-cols-4 gap-2">
        {phases.map((phase) => (
          <div key={phase!.key} className="flex items-center gap-2 text-xs">
            <div className={cn('w-3 h-3 rounded-sm', phase!.color)} />
            <span className="text-muted-foreground">{phase!.label}</span>
            <span className="font-mono ml-auto">{formatDuration(phase!.value)}</span>
          </div>
        ))}
      </div>

      {/* Detailed breakdown */}
      <div className="space-y-1">
        {TIMING_PHASES.map(phase => {
          const value = timing[phase.key as keyof TimingData] as number | undefined;
          if (value === undefined) return null;
          
          const percentage = ((value / timing.total) * 100).toFixed(1);
          
          return (
            <div key={phase.key} className="flex items-center gap-3 text-sm">
              <div className={cn('w-2 h-2 rounded-full shrink-0', phase.color)} />
              <span className="min-w-[80px] text-muted-foreground">{phase.label}</span>
              <div className="flex-1 bg-muted/50 rounded-full h-2 overflow-hidden">
                <div 
                  className={cn('h-full rounded-full transition-all', phase.color)}
                  style={{ width: `${percentage}%` }}
                />
              </div>
              <span className="font-mono text-xs w-16 text-right">{formatDuration(value)}</span>
              <span className="text-xs text-muted-foreground w-12 text-right">{percentage}%</span>
            </div>
          );
        })}
        
        {/* Total */}
        <div className="flex items-center gap-3 text-sm pt-2 border-t mt-2">
          <div className="w-2 h-2 rounded-full shrink-0 bg-foreground" />
          <span className="min-w-[80px] font-medium">Total</span>
          <div className="flex-1" />
          <span className="font-mono text-xs w-16 text-right font-medium">{formatDuration(timing.total)}</span>
          <span className="text-xs text-muted-foreground w-12 text-right">100%</span>
        </div>
      </div>
    </div>
  );
}

// Simple timing display when detailed data isn't available
export function SimpleTimingDisplay({ durationMs }: { durationMs: number }) {
  const category = useMemo(() => {
    if (durationMs < 100) return { label: 'Fast', color: 'text-emerald-500', bg: 'bg-emerald-500' };
    if (durationMs < 500) return { label: 'Normal', color: 'text-sky-500', bg: 'bg-sky-500' };
    if (durationMs < 1000) return { label: 'Slow', color: 'text-amber-500', bg: 'bg-amber-500' };
    return { label: 'Very Slow', color: 'text-red-500', bg: 'bg-red-500' };
  }, [durationMs]);

  return (
    <div className="space-y-4">
      {/* Main display */}
      <div className="flex items-center gap-4 p-4 bg-muted/30 rounded-lg">
        <div className="flex-1">
          <div className="text-xs text-muted-foreground mb-1">Total Duration</div>
          <div className="text-2xl font-mono font-bold">{formatDuration(durationMs)}</div>
        </div>
        <div className={cn('px-3 py-1 rounded-full text-xs font-medium', category.bg, 'text-white')}>
          {category.label}
        </div>
      </div>

      {/* Estimated breakdown */}
      <div className="space-y-2">
        <h4 className="text-xs text-muted-foreground uppercase font-medium">Estimated Breakdown</h4>
        <div className="relative h-6 bg-muted/50 rounded overflow-hidden">
          {/* Simulated phases based on typical distribution */}
          <div 
            className="absolute top-0 bottom-0 bg-cyan-500" 
            style={{ left: '0%', width: '5%' }}
            title="DNS (~5%)"
          />
          <div 
            className="absolute top-0 bottom-0 bg-orange-500" 
            style={{ left: '5%', width: '10%' }}
            title="Connect (~10%)"
          />
          <div 
            className="absolute top-0 bottom-0 bg-purple-500" 
            style={{ left: '15%', width: '10%' }}
            title="TLS (~10%)"
          />
          <div 
            className="absolute top-0 bottom-0 bg-emerald-500" 
            style={{ left: '25%', width: '5%' }}
            title="Send (~5%)"
          />
          <div 
            className="absolute top-0 bottom-0 bg-sky-500" 
            style={{ left: '30%', width: '40%' }}
            title="Wait (~40%)"
          />
          <div 
            className="absolute top-0 bottom-0 bg-blue-500" 
            style={{ left: '70%', width: '30%' }}
            title="Receive (~30%)"
          />
        </div>
        <div className="flex flex-wrap gap-3 text-xs">
          <div className="flex items-center gap-1">
            <div className="w-2 h-2 rounded-full bg-cyan-500" />
            <span className="text-muted-foreground">DNS</span>
          </div>
          <div className="flex items-center gap-1">
            <div className="w-2 h-2 rounded-full bg-orange-500" />
            <span className="text-muted-foreground">Connect</span>
          </div>
          <div className="flex items-center gap-1">
            <div className="w-2 h-2 rounded-full bg-purple-500" />
            <span className="text-muted-foreground">TLS</span>
          </div>
          <div className="flex items-center gap-1">
            <div className="w-2 h-2 rounded-full bg-emerald-500" />
            <span className="text-muted-foreground">Send</span>
          </div>
          <div className="flex items-center gap-1">
            <div className="w-2 h-2 rounded-full bg-sky-500" />
            <span className="text-muted-foreground">Wait (TTFB)</span>
          </div>
          <div className="flex items-center gap-1">
            <div className="w-2 h-2 rounded-full bg-blue-500" />
            <span className="text-muted-foreground">Receive</span>
          </div>
        </div>
      </div>

      <p className="text-xs text-muted-foreground">
        Detailed timing breakdown requires enhanced capture mode. 
        The breakdown above shows typical distribution patterns.
      </p>
    </div>
  );
}
