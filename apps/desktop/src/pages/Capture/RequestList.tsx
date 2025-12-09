import { useRef, useCallback, useEffect, useMemo } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { CapturedRequest, useCaptureStore } from "@/stores/capture";
import { useColumnsStore } from "@/stores/columns";
import { RequestListItem } from "./RequestListItem";
import { TableHeader } from "./TableHeader";

const ROW_HEIGHT = 28;
const HEADER_HEIGHT = 28;
const OVERSCAN = 15;

interface RequestListProps {
  requests: CapturedRequest[];
}

export function RequestList({ requests }: RequestListProps) {
  const parentRef = useRef<HTMLDivElement>(null);
  const selectedId = useCaptureStore((state) => state.selectedId);
  const setSelected = useCaptureStore((state) => state.setSelected);
  const columns = useColumnsStore((state) => state.columns);

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

  const handleSelect = useCallback(
    (id: string) => {
      setSelected(selectedId === id ? null : id);
    },
    [selectedId, setSelected]
  );

  const virtualItems = virtualizer.getVirtualItems();
  const totalSize = virtualizer.getTotalSize();

  const itemStyles = useMemo(() => {
    return virtualItems.map((virtualRow) => ({
      position: "absolute" as const,
      top: 0,
      left: 0,
      width: "100%",
      height: ROW_HEIGHT,
      transform: `translateY(${virtualRow.start}px)`,
    }));
  }, [virtualItems]);

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
      <div ref={parentRef} className="flex-1 overflow-auto">
        <div
          style={{
            height: totalSize,
            width: "100%",
            position: "relative",
          }}
        >
          {virtualItems.map((virtualRow, idx) => {
            const request = requests[virtualRow.index];
            if (!request) return null;
            return (
              <RequestListItem
                key={virtualRow.key}
                request={request}
                isSelected={request.id === selectedId}
                onClick={() => handleSelect(request.id)}
                style={itemStyles[idx]}
                columns={columns}
              />
            );
          })}
        </div>
      </div>
    </div>
  );
}
