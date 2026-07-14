import { useRef, useCallback, useEffect, useLayoutEffect, useMemo } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { useCaptureStore } from "@/stores/capture";
import { useColumnsStore } from "@/stores/columns";
import { RequestListItem } from "./RequestListItem";
import { TableHeader } from "./TableHeader";

const ROW_HEIGHT = 28;
const HEADER_HEIGHT = 28;
// Higher overscan for smoother scrolling
const OVERSCAN = 20;

interface RequestListProps {
  requestIds: string[];
}

export function RequestList({ requestIds }: RequestListProps) {
  const parentRef = useRef<HTMLDivElement>(null);
  const previousIdsRef = useRef<string[]>([]);
  const selectedId = useCaptureStore((state) => state.selectedId);
  const setSelected = useCaptureStore((state) => state.setSelected);
  const columns = useColumnsStore((state) => state.columns);

  // NOTE: we intentionally do NOT subscribe to `useStreamStore().connections`
  // at the list level. That Map reference changes on every SSE/WS frame (the
  // store rebuilds it with `new Map(...)` for React state-identity), which
  // would re-render the entire virtualized list — and its memoized items —
  // dozens of times per second for busy streams. Instead, each item
  // subscribes to *its own* connection via `useStreamConnection(id)` below,
  // so only rows whose stream actually changed re-render.
  //
  // Memoize visible columns to avoid filtering on every render
  const visibleColumns = useMemo(
    () => columns.filter((col) => col.visible),
    [columns]
  );

  const getItemKey = useCallback(
    (index: number) => requestIds[index] ?? `idx-${index}`,
    [requestIds]
  );

  // eslint-disable-next-line react-hooks/incompatible-library -- TanStack Virtual manages scroll state internally and is intentionally kept outside React Compiler memoization.
  const virtualizer = useVirtualizer({
    count: requestIds.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: OVERSCAN,
    getItemKey,
  });

  useEffect(() => {
    if (requestIds.length === 0) {
      virtualizer.scrollToOffset(0);
    }
  }, [requestIds.length, virtualizer]);

  // New captures are prepended. If the user is inspecting older rows, keep
  // the same request anchored in the viewport instead of replacing the view
  // on every incoming request. At the top we intentionally follow new traffic.
  useLayoutEffect(() => {
    const scrollElement = parentRef.current;
    const previousIds = previousIdsRef.current;

    if (scrollElement && scrollElement.scrollTop > ROW_HEIGHT && previousIds.length > 0) {
      const previousFirst = previousIds[0];
      const insertionCount = requestIds.indexOf(previousFirst);
      const sampleSize = Math.min(previousIds.length, 3);
      const stillAligned = insertionCount > 0 &&
        Array.from({ length: sampleSize }, (_, index) =>
          requestIds[insertionCount + index] === previousIds[index]
        ).every(Boolean);

      if (stillAligned) {
        scrollElement.scrollTop += insertionCount * ROW_HEIGHT;
      }
    }

    previousIdsRef.current = requestIds.slice(0, 3);
  }, [requestIds]);

  // Stable callback - doesn't depend on selectedId
  const handleSelect = useCallback(
    (id: string) => {
      setSelected((prev: string | null) => (prev === id ? null : id));
    },
    [setSelected]
  );

  const virtualItems = virtualizer.getVirtualItems();
  const totalSize = virtualizer.getTotalSize();

  if (requestIds.length === 0) {
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
        className="min-h-0 min-w-0 flex-1 overflow-auto"
        style={{
          // Optimize scroll container
          contain: "strict",
          overscrollBehavior: "contain",
          scrollbarGutter: "stable",
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
            const requestId = requestIds[virtualRow.index];
            if (!requestId) return null;

            // Each item subscribes to its own stream connection internally;
            // see RequestListItem. Only rows that are actually streams pay
            // the subscription cost, and only the affected row re-renders
            // when a frame arrives.
            return (
              <RequestListItem
                key={virtualRow.key}
                requestId={requestId}
                isSelected={requestId === selectedId}
                onSelect={handleSelect}
                translateY={virtualRow.start}
                height={ROW_HEIGHT}
                columns={visibleColumns}
              />
            );
          })}
        </div>
      </div>
    </div>
  );
}
