import { useRef, useCallback, useEffect, useMemo } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { CapturedRequest, useCaptureStore } from "@/stores/capture";
import { useColumnsStore } from "@/stores/columns";
import { useStreamStore } from "@/stores/stream";
import { RequestListItem } from "./RequestListItem";
import { TableHeader } from "./TableHeader";

const ROW_HEIGHT = 28;
const HEADER_HEIGHT = 28;
// Higher overscan for smoother scrolling
const OVERSCAN = 20;

interface RequestListProps {
  requests: CapturedRequest[];
}

export function RequestList({ requests }: RequestListProps) {
  const parentRef = useRef<HTMLDivElement>(null);
  const selectedId = useCaptureStore((state) => state.selectedId);
  const setSelected = useCaptureStore((state) => state.setSelected);
  const columns = useColumnsStore((state) => state.columns);
  
  // Get stream connections once at list level
  const streamConnections = useStreamStore((state) => state.connections);

  // Memoize visible columns to avoid filtering on every render
  const visibleColumns = useMemo(
    () => columns.filter((col) => col.visible),
    [columns]
  );

  const getItemKey = useCallback(
    (index: number) => requests[index]?.id ?? `idx-${index}`,
    [requests]
  );

  const virtualizer = useVirtualizer({
    count: requests.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: OVERSCAN,
    getItemKey,
  });

  useEffect(() => {
    if (requests.length === 0) {
      virtualizer.scrollToOffset(0);
    }
  }, [requests.length, virtualizer]);

  // Stable callback - doesn't depend on selectedId
  const handleSelect = useCallback(
    (id: string) => {
      setSelected((prev: string | null) => (prev === id ? null : id));
    },
    [setSelected]
  );

  const virtualItems = virtualizer.getVirtualItems();
  const totalSize = virtualizer.getTotalSize();

  if (requests.length === 0) {
    return (
      <div className="flex flex-col h-full">
        <TableHeader height={HEADER_HEIGHT} />
        <div className="flex flex-1 items-center justify-center text-muted-foreground">
          <div className="text-center">
            <p className="text-sm">No requests captured</p>
            <p className="text-xs mt-1">Start the proxy to capture traffic</p>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full">
      <TableHeader height={HEADER_HEIGHT} />
      <div
        ref={parentRef}
        className="flex-1 overflow-auto"
        style={{
          // Optimize scroll container
          contain: "strict",
          overscrollBehavior: "contain",
        }}
      >
        <div
          style={{
            height: totalSize,
            width: "100%",
            position: "relative",
            // Prevent layout thrashing
            contain: "layout size style",
          }}
        >
          {virtualItems.map((virtualRow) => {
            const request = requests[virtualRow.index];
            if (!request) return null;
            
            // Only look up stream connection for stream protocols
            const streamConnection = 
              (request.protocol === "websocket" || request.protocol === "sse")
                ? streamConnections.get(request.id)
                : undefined;
            
            return (
              <RequestListItem
                key={virtualRow.key}
                request={request}
                isSelected={request.id === selectedId}
                onSelect={handleSelect}
                translateY={virtualRow.start}
                height={ROW_HEIGHT}
                columns={visibleColumns}
                streamConnection={streamConnection}
              />
            );
          })}
        </div>
      </div>
    </div>
  );
}
