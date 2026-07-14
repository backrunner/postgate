---
title: 文件、模拟与注入
description: 返回本地测试数据，并向响应中注入 HTML、JavaScript 或 CSS。
navTitle: 文件与注入
order: 14
---

# 文件、模拟与注入

本地文件规则适合替换开发中的前端资源包或固定测试数据，同时让页面中的其他内容继续从远端加载。

## 用文件替换响应

请使用绝对路径：

```text
cdn.example.com/assets/app.js file:///Users/me/project/dist/app.js
api.example.com/v1/user file:///Users/me/fixtures/user.json
```

Windows 需要包含盘符的合法 File URL，PostGate 进程也必须有读取权限。

`mock` 是另一种基于文件的模拟操作：

```text
api.example.com/v1/orders mock:///absolute/path/orders.json
```

## 替换特定类型内容

内容较短时，可以直接使用正文操作：

```text
api.example.com/health json://{"ok":true,"source":"postgate"}
example.com/banner htmlBody://<aside>Local environment</aside>
```

对于可复用内容或多行内容，建议先保存到 **Values**，避免在一行规则中堆叠大量转义字符。

## 注入页面

```text
example.com htmlAppend://<div id="local-badge">LOCAL</div>
example.com jsAppend://console.info('PostGate active')
example.com cssAppend://#local-badge{position:fixed;top:8px;right:8px}
```

`htmlPrepend`、`jsPrepend` 和 `cssPrepend` 会插到已有内容前。`htmlReplace`、`jsReplace` 和 `cssReplace` 用于定向替换。`htmlBody`、`jsBody` 和 `cssBody` 会完整替换对应正文。

注入只会应用到内容类型兼容的响应。如果页面没有变化，请检查响应的 `content-type`，并在 Capture 中查看正文预览。
