import { Outlet } from "react-router-dom";
import { Sidebar } from "./Sidebar";
import { Header } from "./Header";
import { TooltipProvider } from "@/components/ui/tooltip";

export function AppLayout() {
  return (
    <TooltipProvider delayDuration={0}>
      <div className="flex h-screen w-screen overflow-hidden bg-background text-foreground antialiased selection:bg-primary/20">
        <Sidebar />
        <div className="flex flex-1 flex-col min-w-0 overflow-hidden">
          <Header />
          <main className="flex-1 overflow-hidden relative">
            <Outlet />
          </main>
        </div>
      </div>
    </TooltipProvider>
  );
}
