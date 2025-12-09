import { NavLink } from "react-router-dom";
import {
  Activity,
  FileCode,
  Send,
  Bug,
  Puzzle,
  Settings,
  Radio,
} from "lucide-react";
import { cn } from "@/lib/utils";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { useProxyStore } from "@/stores/proxy";

interface NavItem {
  to: string;
  icon: React.ReactNode;
  label: string;
}

const navItems: NavItem[] = [
  { to: "/capture", icon: <Activity className="h-5 w-5" />, label: "Capture" },
  { to: "/rules", icon: <FileCode className="h-5 w-5" />, label: "Rules" },
  { to: "/replay", icon: <Send className="h-5 w-5" />, label: "Replay" },
  { to: "/debug", icon: <Bug className="h-5 w-5" />, label: "Debug" },
  { to: "/plugins", icon: <Puzzle className="h-5 w-5" />, label: "Plugins" },
];

export function Sidebar() {
  const proxyStatus = useProxyStore((state) => state.status);

  return (
    <aside className="flex h-full w-14 flex-col items-center border-r bg-sidebar py-4">
      {/* Logo */}
      <div className="mb-6 flex h-10 w-10 items-center justify-center">
        <Radio
          className={cn(
            "h-6 w-6 transition-colors",
            proxyStatus === "running" ? "text-emerald-500" : "text-muted-foreground"
          )}
        />
      </div>

      {/* Navigation */}
      <nav className="flex flex-1 flex-col items-center gap-2">
        {navItems.map((item) => (
          <Tooltip key={item.to} delayDuration={0}>
            <TooltipTrigger asChild>
              <NavLink
                to={item.to}
                className={({ isActive }) =>
                  cn(
                    "flex h-10 w-10 items-center justify-center rounded-lg transition-colors",
                    "hover:bg-sidebar-accent hover:text-sidebar-accent-foreground",
                    isActive
                      ? "bg-sidebar-accent text-sidebar-accent-foreground"
                      : "text-sidebar-foreground"
                  )
                }
              >
                {item.icon}
              </NavLink>
            </TooltipTrigger>
            <TooltipContent side="right" sideOffset={8}>
              {item.label}
            </TooltipContent>
          </Tooltip>
        ))}
      </nav>

      {/* Settings at bottom */}
      <Tooltip delayDuration={0}>
        <TooltipTrigger asChild>
          <NavLink
            to="/settings"
            className={({ isActive }) =>
              cn(
                "flex h-10 w-10 items-center justify-center rounded-lg transition-colors",
                "hover:bg-sidebar-accent hover:text-sidebar-accent-foreground",
                isActive
                  ? "bg-sidebar-accent text-sidebar-accent-foreground"
                  : "text-sidebar-foreground"
              )
            }
          >
            <Settings className="h-5 w-5" />
          </NavLink>
        </TooltipTrigger>
        <TooltipContent side="right" sideOffset={8}>
          Settings
        </TooltipContent>
      </Tooltip>
    </aside>
  );
}
