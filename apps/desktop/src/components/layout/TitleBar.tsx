import { useEffect, useState, useCallback } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { Minus, Square, X, Copy } from 'lucide-react';
import { cn } from '@/lib/utils';

// Detect platform
const isMacOS = navigator.platform.toUpperCase().indexOf('MAC') >= 0;

// macOS titlebar height
const MACOS_TITLEBAR_HEIGHT = 32;
// Windows titlebar height is typically 32px
const WINDOWS_TITLEBAR_HEIGHT = 32;

// macOS traffic lights width + gap
const MACOS_TRAFFIC_LIGHTS_WIDTH = 78;

export function TitleBar() {
  const [isMaximized, setIsMaximized] = useState(false);
  const [isFocused, setIsFocused] = useState(true);

  useEffect(() => {
    const appWindow = getCurrentWindow();
    
    appWindow.isMaximized().then(setIsMaximized);
    appWindow.isFocused().then(setIsFocused);
    
    const unlistenResize = appWindow.onResized(async () => {
      const maximized = await appWindow.isMaximized();
      setIsMaximized(maximized);
    });

    const unlistenFocus = appWindow.onFocusChanged(({ payload: focused }) => {
      setIsFocused(focused);
    });

    return () => {
      unlistenResize.then(fn => fn());
      unlistenFocus.then(fn => fn());
    };
  }, []);

  const handleDragStart = useCallback(async (e: React.MouseEvent) => {
    // Only left mouse button
    if (e.button !== 0) return;
    // Don't drag if clicking on interactive elements
    const target = e.target as HTMLElement;
    if (target.closest('button')) return;
    
    // Prevent default behavior
    e.preventDefault();
    e.stopPropagation();
    
    try {
      // Start dragging
      await getCurrentWindow().startDragging();
    } catch (err) {
      console.error('Failed to start dragging:', err);
    }
  }, []);

  const handleMinimize = useCallback(() => {
    getCurrentWindow().minimize();
  }, []);

  const handleMaximize = useCallback(() => {
    getCurrentWindow().toggleMaximize();
  }, []);

  const handleClose = useCallback(() => {
    getCurrentWindow().close();
  }, []);

  // macOS: native traffic lights with title
  if (isMacOS) {
    return (
      <div
        onMouseDown={handleDragStart}
        className="w-full select-none shrink-0 flex items-center border-b border-border/40 cursor-default"
        style={{ 
          height: MACOS_TITLEBAR_HEIGHT,
        } as React.CSSProperties}
      >
        {/* Spacer for native traffic lights */}
        <div 
          className="shrink-0"
          style={{ 
            width: MACOS_TRAFFIC_LIGHTS_WIDTH,
            height: MACOS_TITLEBAR_HEIGHT,
          } as React.CSSProperties}
        />
        
        {/* Title */}
        <span 
          className={cn(
            isFocused ? "text-foreground" : "text-foreground/50"
          )}
          style={{ 
            fontSize: 13,
            fontWeight: 600,
            letterSpacing: '0.02em',
            pointerEvents: 'none',
            fontFamily: '-apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif'
          }}
        >
          PostGate
        </span>
      </div>
    );
  }

  // Windows/Linux: title on left, window controls on right
  return (
    <div
      onMouseDown={handleDragStart}
      className="w-full flex items-center justify-between select-none shrink-0 border-b border-border/40 bg-background cursor-default"
      style={{ 
        height: WINDOWS_TITLEBAR_HEIGHT,
      } as React.CSSProperties}
    >
      {/* Left side - Title */}
      <div className="flex items-center h-full flex-1 pl-3">
        <span 
          className={cn(
            isFocused ? "text-foreground" : "text-foreground/50"
          )}
          style={{ 
            fontSize: 12,
            fontWeight: 400,
            pointerEvents: 'none'
          }}
        >
          PostGate
        </span>
      </div>

      {/* Right side - Window controls */}
      <div className="flex items-center h-full shrink-0">
        <button
          onClick={handleMinimize}
          tabIndex={-1}
          className="w-[46px] h-full flex items-center justify-center hover:bg-muted/80 transition-colors text-muted-foreground hover:text-foreground focus:outline-none"
        >
          <Minus className="w-4 h-4" />
        </button>
        <button
          onClick={handleMaximize}
          tabIndex={-1}
          className="w-[46px] h-full flex items-center justify-center hover:bg-muted/80 transition-colors text-muted-foreground hover:text-foreground focus:outline-none"
        >
          {isMaximized ? (
            <Copy className="w-3 h-3 rotate-180" />
          ) : (
            <Square className="w-3 h-3" />
          )}
        </button>
        <button
          onClick={handleClose}
          tabIndex={-1}
          className="w-[46px] h-full flex items-center justify-center hover:bg-red-500 hover:text-white transition-colors text-muted-foreground focus:outline-none"
        >
          <X className="w-4 h-4" />
        </button>
      </div>
    </div>
  );
}
