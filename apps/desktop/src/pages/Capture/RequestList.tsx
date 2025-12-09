import { useRef, useCallback } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { CapturedRequest, useCaptureStore } from "@/stores/capture";
import { RequestListItem } from "./RequestListItem";

interface RequestListProps {
  requests: CapturedRequest[];
}

export function RequestList({ requests }: RequestListProps) {
  const parentRef = useRef<HTMLDivElement>(null);
  const { selectedId, setSelected } = useCaptureStore();

  const virtualizer = useVirtualizer({
    count: requests.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 36,
    overscan: 20,
  });

  const handleSelect = useCallback(
    (id: string) => {
      setSelected(selectedId === id ? null : id);
    },
    [selectedId, setSelected]
  );

  if (requests.length === 0) {
    return (
      <div className="flex h-full items-center justify-center text-muted-foreground">
        <div className="text-center">
          <p className="text-sm">No requests captured</p>
          <p className="text-xs mt-1">Start the proxy to capture traffic</p>
        </div>
      </div>
    );
  }

  return (
    <div ref={parentRef} className="h-full overflow-auto">
      <div
        style={{
          height: `${virtualizer.getTotalSize()}px`,
          width: "100%",
          position: "relative",
        }}
      >
        {virtualizer.getVirtualItems().map((virtualRow) => {
          const request = requests[virtualRow.index];
          return (
            <RequestListItem
              key={request.id}
              request={request}
              isSelected={request.id === selectedId}
              onClick={() => handleSelect(request.id)}
              style={{
                position: "absolute",
                top: 0,
                left: 0,
                width: "100%",
                height: `${virtualRow.size}px`,
                transform: `translateY(${virtualRow.start}px)`,
              }}
            />
          );
        })}
      </div>
    </div>
  );
}
