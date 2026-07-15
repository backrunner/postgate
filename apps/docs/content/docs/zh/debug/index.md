---
title: Debug
description: 注入 PostGate 浏览器调试桥，并通过本地 CDP 兼容服务检查实时页面。
navTitle: Debug 概览
order: 20
collapsed: false
---

# Debug

PostGate Debug 通过本地 Chobitsu/CDP 调试桥，将命中的页面连接到桌面应用。整个过程不会把数据发送到远端服务，可以直接捕获 Console 输出、运行时错误、Fetch 和 XHR 活动。

## 启动调试会话

1. 确认目标页面的 HTTPS 捕获正常。
2. 添加并启用规则：

```text
example.com debug://
```

3. 让目标页面通过 PostGate 代理重新加载。
4. 打开 **DevTools**，选择已连接页面。

只要存在已启用的 Debug 规则，调试服务就会自动启动。服务使用的本机端口可以在 **Settings → Proxy Configuration** 中设置。

## 注入内容

PostGate 会向命中的 HTML 响应注入一个轻量客户端。该客户端通过 WebSocket 连接本地调试服务，并借助 Chobitsu 提供 Chrome DevTools Protocol 命令。非 HTML 资源不会被修改。

## 发现端点

本地服务提供与 CDP 兼容的发现端点：

```text
http://127.0.0.1:<debug-port>/json/list
```

每个调试会话都会提供一个指向 `/devtools/page/<session-id>` 的 `webSocketDebuggerUrl`。兼容的本地 CDP 客户端可以通过该地址检查同一页面，而 PostGate 会继续代理页面流量。

## 保持本机边界

调试服务只绑定 `127.0.0.1`。不要通过隧道或反向代理把它暴露到外网，因为 CDP 连接能够观察并控制命中的页面。
