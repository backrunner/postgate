import React from "react";
import ReactDOM from "react-dom/client";
import { BrowserRouter } from "react-router-dom";
import { invoke } from "@tauri-apps/api/core";
import App from "./App";
import { ErrorBoundary } from "@/components/ErrorBoundary";
import "./index.css";

// 显示主窗口
async function showMainWindow() {
  try {
    await invoke("show_main_window");
  } catch {
    // 在非 Tauri 环境（开发模式浏览器）忽略错误
  }
}

// 渲染完成后显示内容和窗口
function onAppRendered() {
  const root = document.getElementById("root");
  if (root) {
    root.classList.add("loaded");
  }
  // 显示窗口
  showMainWindow();
}

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <ErrorBoundary>
      <BrowserRouter>
        <App />
      </BrowserRouter>
    </ErrorBoundary>
  </React.StrictMode>
);

// React 渲染完成后显示内容
if ("requestIdleCallback" in window) {
  requestIdleCallback(onAppRendered, { timeout: 100 });
} else {
  setTimeout(onAppRendered, 50);
}
