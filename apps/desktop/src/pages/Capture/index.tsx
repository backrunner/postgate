import { useState } from "react";
import { RequestList } from "./RequestList";
import { RequestDetail } from "./RequestDetail";
import { Toolbar } from "./Toolbar";
import { useCaptureStore, useFilteredRequests } from "@/stores/capture";
import { useProxy } from "@/hooks/useProxy";

export function CapturePage() {
  const [splitRatio] = useState(0.5);
  const selectedId = useCaptureStore((state) => state.selectedId);
  const filteredRequests = useFilteredRequests();
  const selectedRequest = useCaptureStore((state) =>
    state.selectedId ? state.requestMap.get(state.selectedId) : undefined
  );

  // Initialize proxy event listeners
  useProxy();

  return (
    <div className="flex h-full flex-col">
      {/* Unified header (title + proxy controls + filters live together) */}
      <Toolbar />
      <div className="flex flex-1 overflow-hidden">
        {/* Request List */}
        <div
          className="flex flex-col border-r"
          style={{ width: selectedId ? `${splitRatio * 100}%` : "100%" }}
        >
          <RequestList requests={filteredRequests} />
        </div>

        {/* Request Detail */}
        {selectedId && selectedRequest && (
          <div
            className="flex flex-col overflow-hidden"
            style={{ width: `${(1 - splitRatio) * 100}%` }}
          >
            <RequestDetail request={selectedRequest} />
          </div>
        )}
      </div>
    </div>
  );
}
