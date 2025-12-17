import { Routes, Route, Navigate } from "react-router-dom";
import { Suspense, lazy, useEffect } from "react";
import { AppLayout } from "@/components/layout/AppLayout";
import { useThemeStore } from "@/stores/theme";
import { usePluginsStore } from "@/stores/plugins";

// 路由懒加载
const CapturePage = lazy(() => import("@/pages/Capture").then(m => ({ default: m.CapturePage })));
const RulesPage = lazy(() => import("@/pages/Rules").then(m => ({ default: m.RulesPage })));
const ReplayPage = lazy(() => import("@/pages/Replay").then(m => ({ default: m.ReplayPage })));
const DebugPage = lazy(() => import("@/pages/Debug").then(m => ({ default: m.DebugPage })));
const PluginsPage = lazy(() => import("@/pages/Plugins").then(m => ({ default: m.PluginsPage })));
const SettingsPage = lazy(() => import("@/pages/Settings").then(m => ({ default: m.SettingsPage })));

// 页面加载占位符
function PageLoading() {
  return (
    <div className="flex-1 flex items-center justify-center">
      <div className="h-6 w-6 animate-spin rounded-full border-2 border-primary border-t-transparent" />
    </div>
  );
}

function App() {
  const theme = useThemeStore((state) => state.theme);
  const setupPluginEventListeners = usePluginsStore((state) => state.setupEventListeners);

  useEffect(() => {
    const root = window.document.documentElement;
    root.classList.remove("light", "dark");

    if (theme === "system") {
      const systemTheme = window.matchMedia("(prefers-color-scheme: dark)").matches
        ? "dark"
        : "light";
      root.classList.add(systemTheme);
    } else {
      root.classList.add(theme);
    }
  }, [theme]);

  // Initialize plugin event listeners on mount
  useEffect(() => {
    let unlisteners: (() => void)[] = [];

    setupPluginEventListeners().then((listeners) => {
      unlisteners = listeners;
    }).catch((error) => {
      console.error('Failed to setup plugin event listeners:', error);
    });

    return () => {
      // Cleanup listeners on unmount
      unlisteners.forEach((unlisten) => unlisten());
    };
  }, [setupPluginEventListeners]);

  return (
    <Routes>
      <Route path="/" element={<AppLayout />}>
        <Route index element={<Navigate to="/capture" replace />} />
        <Route path="capture" element={<Suspense fallback={<PageLoading />}><CapturePage /></Suspense>} />
        <Route path="rules" element={<Suspense fallback={<PageLoading />}><RulesPage /></Suspense>} />
        <Route path="replay" element={<Suspense fallback={<PageLoading />}><ReplayPage /></Suspense>} />
        <Route path="debug" element={<Suspense fallback={<PageLoading />}><DebugPage /></Suspense>} />
        <Route path="plugins" element={<Suspense fallback={<PageLoading />}><PluginsPage /></Suspense>} />
        <Route path="settings" element={<Suspense fallback={<PageLoading />}><SettingsPage /></Suspense>} />
      </Route>
    </Routes>
  );
}

export default App;
