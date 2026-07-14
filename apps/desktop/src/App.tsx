import { Routes, Route, Navigate } from "react-router-dom";
import { Suspense, lazy, useEffect } from "react";
import { AppLayout } from "@/components/layout/AppLayout";
import { PluginToastViewport } from "@/components/plugins/PluginToastViewport";
import { useThemeStore } from "@/stores/theme";
import { usePluginsStore } from "@/stores/plugins";
import { useRulesStore } from "@/stores/rules";
import { initUpdaterSettings } from "@/stores/updater";

// 路由懒加载
const CapturePage = lazy(() => import("@/pages/Capture").then(m => ({ default: m.CapturePage })));
const RulesPage = lazy(() => import("@/pages/Rules").then(m => ({ default: m.RulesPage })));
const ValuesPage = lazy(() => import("@/pages/Values").then(m => ({ default: m.ValuesPage })));
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
  const fetchPluginPanels = usePluginsStore((state) => state.fetchPanels);
  const setupRuleEventListeners = useRulesStore((state) => state.setupEventListeners);

  useEffect(() => {
    const root = window.document.documentElement;
    const systemTheme = window.matchMedia("(prefers-color-scheme: dark)");
    const applyTheme = () => {
      root.classList.remove("light", "dark");
      if (theme !== "system") {
        root.classList.add(theme);
        return;
      }
      const resolvedTheme = systemTheme.matches
        ? "dark"
        : "light";
      root.classList.add(resolvedTheme);
    };

    applyTheme();
    if (theme === "system") {
      systemTheme.addEventListener("change", applyTheme);
      return () => systemTheme.removeEventListener("change", applyTheme);
    }
  }, [theme]);

  useEffect(() => {
    initUpdaterSettings();
  }, []);

  // Initialize plugin event listeners on mount
  useEffect(() => {
    // `cancelled` handles the race where the component unmounts before
    // setupPluginEventListeners() resolves. Without it, listeners installed
    // after cleanup runs would leak for the rest of the page lifetime.
    let cancelled = false;
    let unlisteners: (() => void)[] = [];

    setupPluginEventListeners()
      .then(async (listeners) => {
        if (cancelled) {
          // Already unmounted — immediately release what we just installed.
          listeners.forEach((unlisten) => unlisten());
          return;
        }
        unlisteners = listeners;
        await fetchPluginPanels();
      })
      .catch((error) => {
        console.error('Failed to setup plugin event listeners:', error);
      });

    return () => {
      cancelled = true;
      unlisteners.forEach((unlisten) => unlisten());
    };
  }, [fetchPluginPanels, setupPluginEventListeners]);

  useEffect(() => {
    let cancelled = false;
    let unlisteners: (() => void)[] = [];

    setupRuleEventListeners()
      .then((listeners) => {
        if (cancelled) {
          listeners.forEach((unlisten) => unlisten());
          return;
        }
        unlisteners = listeners;
      })
      .catch((error) => {
        console.error('Failed to setup rule event listeners:', error);
      });

    return () => {
      cancelled = true;
      unlisteners.forEach((unlisten) => unlisten());
    };
  }, [setupRuleEventListeners]);

  return (
    <>
      <Routes>
        <Route path="/" element={<AppLayout />}>
          <Route index element={<Navigate to="/capture" replace />} />
          <Route path="capture" element={<Suspense fallback={<PageLoading />}><CapturePage /></Suspense>} />
          <Route path="rules" element={<Suspense fallback={<PageLoading />}><RulesPage /></Suspense>} />
          <Route path="values" element={<Suspense fallback={<PageLoading />}><ValuesPage /></Suspense>} />
          <Route path="replay" element={<Suspense fallback={<PageLoading />}><ReplayPage /></Suspense>} />
          <Route path="debug" element={<Suspense fallback={<PageLoading />}><DebugPage /></Suspense>} />
          <Route path="plugins" element={<Suspense fallback={<PageLoading />}><PluginsPage /></Suspense>} />
          <Route path="settings" element={<Suspense fallback={<PageLoading />}><SettingsPage /></Suspense>} />
        </Route>
      </Routes>
      <PluginToastViewport />
    </>
  );
}

export default App;
