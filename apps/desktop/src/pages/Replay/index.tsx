import { Send, FolderPlus } from "lucide-react";
import { Button } from "@/components/ui/button";

export function ReplayPage() {
  return (
    <div className="flex h-full">
      {/* Sidebar - Collections */}
      <div className="w-64 border-r flex flex-col">
        <div className="flex h-10 items-center justify-between border-b px-3">
          <h2 className="text-sm font-semibold">Collections</h2>
          <Button variant="ghost" size="icon-sm" title="New Collection">
            <FolderPlus className="h-4 w-4" />
          </Button>
        </div>
        <div className="flex-1 flex items-center justify-center">
          <p className="text-sm text-muted-foreground text-center px-4">
            No collections yet. Create one to organize your requests.
          </p>
        </div>
      </div>

      {/* Main Content */}
      <div className="flex-1 flex items-center justify-center">
        <div className="text-center text-muted-foreground">
          <Send className="mx-auto h-12 w-12 mb-4 opacity-50" />
          <h3 className="font-semibold mb-1">Request Replay</h3>
          <p className="text-sm mb-4 max-w-md">
            Send HTTP requests, save them to collections, and replay them anytime.
            Similar to Postman but integrated with your proxy.
          </p>
          <Button className="gap-1">
            <Send className="h-4 w-4" />
            New Request
          </Button>
        </div>
      </div>
    </div>
  );
}
