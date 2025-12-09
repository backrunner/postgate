import { Routes, Route, Navigate } from "react-router-dom";
import { useEffect } from "react";
import { AppLayout } from "@/components/layout/AppLayout";
import { CapturePage } from "@/pages/Capture";
import { RulesPage } from "@/pages/Rules";
import { ReplayPage } from "@/pages/Replay";
import { DebugPage } from "@/pages/Debug";
import { PluginsPage } from "@/pages/Plugins";
import { SettingsPage } from "@/pages/Settings";
import { useThemeStore } from "@/stores/theme";

function App() {
  const theme = useThemeStore((state) => state.theme);

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

  return (
    <Routes>
      <Route path="/" element={<AppLayout />}>
        <Route index element={<Navigate to="/capture" replace />} />
        <Route path="capture" element={<CapturePage />} />
        <Route path="rules" element={<RulesPage />} />
        <Route path="replay" element={<ReplayPage />} />
        <Route path="debug" element={<DebugPage />} />
        <Route path="plugins" element={<PluginsPage />} />
        <Route path="settings" element={<SettingsPage />} />
      </Route>
    </Routes>
  );
}

export default App;
