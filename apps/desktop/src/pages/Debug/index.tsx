import { Bug, Terminal, Smartphone } from "lucide-react";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";

export function DebugPage() {
  return (
    <div className="flex h-full flex-col">
      <Tabs defaultValue="console" className="flex-1 flex flex-col">
        <div className="flex h-10 items-center border-b px-4">
          <TabsList>
            <TabsTrigger value="console" className="gap-1">
              <Terminal className="h-4 w-4" />
              Console
            </TabsTrigger>
            <TabsTrigger value="devtools" className="gap-1">
              <Bug className="h-4 w-4" />
              DevTools
            </TabsTrigger>
          </TabsList>
        </div>

        <TabsContent value="console" className="flex-1 flex flex-col mt-0">
          <div className="flex-1 flex items-center justify-center">
            <div className="text-center text-muted-foreground">
              <Terminal className="mx-auto h-12 w-12 mb-4 opacity-50" />
              <h3 className="font-semibold mb-1">Console Capture</h3>
              <p className="text-sm max-w-md">
                Console logs from injected pages will appear here.
                Enable console injection in the Rules page using the inject protocol.
              </p>
            </div>
          </div>
        </TabsContent>

        <TabsContent value="devtools" className="flex-1 flex flex-col mt-0">
          <div className="flex-1 flex items-center justify-center">
            <div className="text-center text-muted-foreground">
              <Smartphone className="mx-auto h-12 w-12 mb-4 opacity-50" />
              <h3 className="font-semibold mb-1">Remote DevTools</h3>
              <p className="text-sm max-w-md">
                Debug remote pages using Chrome DevTools Protocol.
                Use the debug:// rule to enable debugging for specific pages.
              </p>
            </div>
          </div>
        </TabsContent>
      </Tabs>
    </div>
  );
}
