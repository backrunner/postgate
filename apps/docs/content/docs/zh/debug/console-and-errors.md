---
title: Console 与页面错误
description: 查看已连接页面的 Console、未捕获异常和未处理 Promise Rejection。
navTitle: Console 与错误
order: 21
---

# Console 与页面错误

在 **DevTools** 中选择一个页面，可以只查看该页面的调试会话；选择 **All Sessions**，则会合并显示所有当前页面的事件。

## Console 捕获

注入的客户端会捕获常见 Console 方法，并把日志级别、参数、时间戳和来源信息发送到 PostGate。它适合用于打开浏览器原生 DevTools 可能影响测试结果的场景。

Console 事件按调试会话隔离。重复测试前请先清空面板，避免把旧输出误认为本次结果。

## 运行时错误

PostGate 会记录：

- `window.onerror` 报告的未捕获错误
- 未处理 Promise Rejection
- 浏览器提供的 Source Location 和 Stack

空面板不能证明注入前没有失败。启用 `debug://` 规则后重新加载页面，让 Bridge 从初始 HTML 响应开始工作。

## 没有出现调试会话

确认规则组已经启用、页面文档命中了规则，并且响应内容确实是 HTML。随后检查内容安全策略（CSP）、浏览器扩展或页面的 WebSocket 限制是否阻止了本地调试桥。
