---
title: Fetch、XHR 与 CDP
description: 检查页面网络活动，并把兼容工具连接到 PostGate 的本地 CDP 端点。
navTitle: Network 与 CDP
order: 22
---

# Fetch、XHR 与 CDP

Debug 客户端会观察页面中的 Fetch 和 XMLHttpRequest。它与 **Capture** 相互补充：Capture 展示经过代理的实际流量，Debug 则将网络活动关联到具体的浏览器页面和运行时上下文。

## Network 事件

在同一个调试会话中，可以把请求方法、URL、状态码和耗时与 Console 输出或页面错误对应起来。绕过代理的请求仍可能被页面内的调试代码观察到，但不会在 Capture 中留下对应记录。

## 连接 CDP 客户端

先查询发现端点：

```bash
curl http://127.0.0.1:<debug-port>/json/list
```

选择页面并连接它的 `webSocketDebuggerUrl`：

```text
ws://127.0.0.1:<debug-port>/devtools/page/<session-id>
```

客户端需要支持 Chobitsu 所实现的 Chrome DevTools Protocol 消息。由于这里的 CDP 实现在页面内部，浏览器原生 DevTools 的部分功能可能无法完整使用。

## 调试会话的生命周期

当页面断开连接、重新加载后不再命中规则，或 PostGate 停止调试服务时，对应会话会关闭。如果页面重新加载后仍然命中规则，PostGate 会创建或刷新该会话。排查重连问题时，可以先删除 **DevTools** 工作区中的过期会话。
