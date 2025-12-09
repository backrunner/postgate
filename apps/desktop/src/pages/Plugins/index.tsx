import { Puzzle, Plus, RefreshCw } from "lucide-react";
import { Button } from "@/components/ui/button";

export function PluginsPage() {
  return (
    <div className="flex h-full flex-col">
      {/* Toolbar */}
      <div className="flex h-10 items-center justify-between border-b px-4">
        <h2 className="text-sm font-semibold">Plugins</h2>
        <div className="flex items-center gap-2">
          <Button variant="outline" size="sm" className="gap-1">
            <RefreshCw className="h-4 w-4" />
            Scan
          </Button>
          <Button size="sm" className="gap-1">
            <Plus className="h-4 w-4" />
            Install Plugin
          </Button>
        </div>
      </div>

      {/* Content */}
      <div className="flex flex-1 items-center justify-center">
        <div className="text-center text-muted-foreground">
          <Puzzle className="mx-auto h-12 w-12 mb-4 opacity-50" />
          <h3 className="font-semibold mb-1">No plugins installed</h3>
          <p className="text-sm mb-4 max-w-md">
            Install PostGate plugins to extend functionality.
            Plugins can be installed via npm with the prefix{" "}
            <code className="bg-muted px-1 rounded">postgate-plugin-*</code>
          </p>
          <div className="flex justify-center gap-2">
            <Button variant="outline" className="gap-1">
              <RefreshCw className="h-4 w-4" />
              Scan for Plugins
            </Button>
          </div>
        </div>
      </div>
    </div>
  );
}
