import React from "react";
import ReactDOM from "react-dom/client";
import { BrowserRouter } from "react-router-dom";
import App from "./App";
import "./index.css";

// 显示窗口的函数
async function showWindow() {
  try {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    const currentWindow = getCurrentWindow();
    await currentWindow.show();
    await currentWindow.setFocus();
  } catch {
    // 在非 Tauri 环境（开发模式浏览器）忽略错误
  }
}

// 渲染完成后显示内容和窗口
function onAppReady() {
  const root = document.getElementById("root");
  if (root) {
    root.classList.add("loaded");
  }
  // 延迟一帧确保渲染完成
  requestAnimationFrame(() => {
    showWindow();
  });
}

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <BrowserRouter>
      <App />
    </BrowserRouter>
  </React.StrictMode>
);

// 使用 requestIdleCallback 或 setTimeout 确保首次渲染完成
if ("requestIdleCallback" in window) {
  requestIdleCallback(onAppReady, { timeout: 100 });
} else {
  setTimeout(onAppReady, 50);
}
