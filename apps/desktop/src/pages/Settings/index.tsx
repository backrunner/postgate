import { Settings, Shield, Network, Palette } from "lucide-react";
import { Switch } from "@/components/ui/switch";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { Separator } from "@/components/ui/separator";
import { ScrollArea } from "@/components/ui/scroll-area";
import { useProxyStore } from "@/stores/proxy";
import { useThemeStore } from "@/stores/theme";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";

export function SettingsPage() {
  const { config, setConfig } = useProxyStore();
  const { theme, setTheme } = useThemeStore();

  return (
    <ScrollArea className="h-full">
      <div className="max-w-2xl mx-auto py-6 px-4 space-y-8">
        {/* Proxy Settings */}
        <section>
          <div className="flex items-center gap-2 mb-4">
            <Network className="h-5 w-5" />
            <h2 className="text-lg font-semibold">Proxy Settings</h2>
          </div>
          <div className="space-y-4">
            <div className="flex items-center justify-between">
              <div>
                <label className="font-medium">Port</label>
                <p className="text-sm text-muted-foreground">
                  The port the proxy server listens on
                </p>
              </div>
              <Input
                type="number"
                value={config.port}
                onChange={(e) => setConfig({ port: parseInt(e.target.value) || 8899 })}
                className="w-24 text-right"
                min={1}
                max={65535}
              />
            </div>
            <Separator />
            <div className="flex items-center justify-between">
              <div>
                <label className="font-medium">HTTP/2 Support</label>
                <p className="text-sm text-muted-foreground">
                  Enable HTTP/2 protocol support for proxied connections
                </p>
              </div>
              <Switch
                checked={config.enableHttp2}
                onCheckedChange={(checked) => setConfig({ enableHttp2: checked })}
              />
            </div>
            <Separator />
            <div className="flex items-center justify-between">
              <div>
                <label className="font-medium">QUIC/HTTP/3 Support</label>
                <p className="text-sm text-muted-foreground">
                  Enable experimental QUIC protocol support
                </p>
              </div>
              <Switch
                checked={config.enableQuic}
                onCheckedChange={(checked) => setConfig({ enableQuic: checked })}
              />
            </div>
          </div>
        </section>

        {/* Certificate Settings */}
        <section>
          <div className="flex items-center gap-2 mb-4">
            <Shield className="h-5 w-5" />
            <h2 className="text-lg font-semibold">Certificate</h2>
          </div>
          <div className="space-y-4">
            <div className="rounded-lg border p-4 bg-muted/50">
              <p className="text-sm text-muted-foreground mb-4">
                To capture HTTPS traffic, you need to install PostGate's root
                certificate as a trusted certificate authority on your system.
              </p>
              <div className="flex gap-2">
                <Button variant="outline">Export Certificate</Button>
                <Button>Install Certificate</Button>
              </div>
            </div>
          </div>
        </section>

        {/* Appearance */}
        <section>
          <div className="flex items-center gap-2 mb-4">
            <Palette className="h-5 w-5" />
            <h2 className="text-lg font-semibold">Appearance</h2>
          </div>
          <div className="space-y-4">
            <div className="flex items-center justify-between">
              <div>
                <label className="font-medium">Theme</label>
                <p className="text-sm text-muted-foreground">
                  Choose your preferred color scheme
                </p>
              </div>
              <DropdownMenu>
                <DropdownMenuTrigger asChild>
                  <Button variant="outline" className="w-32 justify-between">
                    {theme === "light" && "Light"}
                    {theme === "dark" && "Dark"}
                    {theme === "system" && "System"}
                  </Button>
                </DropdownMenuTrigger>
                <DropdownMenuContent align="end">
                  <DropdownMenuItem onClick={() => setTheme("light")}>
                    Light
                  </DropdownMenuItem>
                  <DropdownMenuItem onClick={() => setTheme("dark")}>
                    Dark
                  </DropdownMenuItem>
                  <DropdownMenuItem onClick={() => setTheme("system")}>
                    System
                  </DropdownMenuItem>
                </DropdownMenuContent>
              </DropdownMenu>
            </div>
          </div>
        </section>

        {/* About */}
        <section>
          <div className="flex items-center gap-2 mb-4">
            <Settings className="h-5 w-5" />
            <h2 className="text-lg font-semibold">About</h2>
          </div>
          <div className="rounded-lg border p-4 bg-muted/50">
            <div className="space-y-2 text-sm">
              <div className="flex justify-between">
                <span className="text-muted-foreground">Version</span>
                <span>0.1.0</span>
              </div>
              <div className="flex justify-between">
                <span className="text-muted-foreground">Tauri</span>
                <span>2.0</span>
              </div>
              <div className="flex justify-between">
                <span className="text-muted-foreground">React</span>
                <span>19.0</span>
              </div>
            </div>
          </div>
        </section>
      </div>
    </ScrollArea>
  );
}
