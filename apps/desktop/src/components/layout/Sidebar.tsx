import { NavLink, useLocation } from "react-router-dom";
import {
  Activity,
  FileCode,
  Send,
  Bug,
  Puzzle,
  Settings,
  type LucideIcon,
} from "lucide-react";
import { cn } from "@/lib/utils";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";

interface NavItem {
  to: string;
  icon: LucideIcon;
  label: string;
}

const mainNavItems: NavItem[] = [
  { to: "/capture", icon: Activity, label: "Capture" },
  { to: "/rules", icon: FileCode, label: "Rules" },
  { to: "/replay", icon: Send, label: "Replay" },
  { to: "/debug", icon: Bug, label: "Debug" },
  { to: "/plugins", icon: Puzzle, label: "Plugins" },
];

const bottomNavItems: NavItem[] = [
  { to: "/settings", icon: Settings, label: "Settings" },
];

function NavIcon({
  item,
  isActive,
}: {
  item: NavItem;
  isActive: boolean;
}) {
  const Icon = item.icon;

  return (
    <Tooltip delayDuration={0}>
      <TooltipTrigger asChild>
        <NavLink
          to={item.to}
          className="group relative flex h-10 w-full items-center justify-center"
        >
          {/* Active indicator bar */}
          <span
            className={cn(
              "absolute left-0 h-6 w-[3px] rounded-r-full transition-all duration-300 ease-out",
              isActive
                ? "bg-foreground opacity-100"
                : "bg-transparent opacity-0 group-hover:bg-muted-foreground/40 group-hover:opacity-100"
            )}
          />

          {/* Icon container */}
          <span
            className={cn(
              "flex h-10 w-10 items-center justify-center rounded-lg transition-colors duration-200",
              isActive
                ? "bg-foreground/[0.08] text-foreground"
                : "text-muted-foreground hover:bg-foreground/[0.04] hover:text-foreground"
            )}
          >
            <Icon className="h-[18px] w-[18px]" strokeWidth={1.75} />
          </span>
        </NavLink>
      </TooltipTrigger>
      <TooltipContent side="right" sideOffset={8}>
        {item.label}
      </TooltipContent>
    </Tooltip>
  );
}

export function Sidebar() {
  const location = useLocation();

  return (
    <aside className="relative z-50 flex h-full w-14 flex-col items-center border-r border-border/50 bg-background">
      {/* Subtle gradient overlay */}
      <div className="pointer-events-none absolute inset-0 bg-gradient-to-b from-foreground/[0.01] via-transparent to-foreground/[0.02]" />

      {/* Top spacing */}
      <div className="h-3" />

      {/* Main navigation */}
      <nav className="relative flex w-full flex-col items-center gap-1">
        {mainNavItems.map((item) => (
          <NavIcon
            key={item.to}
            item={item}
            isActive={location.pathname.startsWith(item.to)}
          />
        ))}
      </nav>

      {/* Spacer */}
      <div className="flex-1" />

      {/* Separator */}
      <div className="mb-2 h-px w-6 bg-border/60" />

      {/* Bottom navigation */}
      <nav className="relative flex w-full flex-col items-center gap-1 pb-3">
        {bottomNavItems.map((item) => (
          <NavIcon
            key={item.to}
            item={item}
            isActive={location.pathname.startsWith(item.to)}
          />
        ))}
      </nav>
    </aside>
  );
}
