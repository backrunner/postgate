---
title: 改写响应
description: 修改状态码、重定向、响应头、Cookie、CORS、缓存和响应正文。
navTitle: 响应改写
order: 13
---

# 改写响应

响应操作会在上游服务器返回响应之后、PostGate 将响应发送给客户端之前执行。

## 状态码与重定向

```text
api.example.com/maintenance statusCode://503
example.com/old redirect://https://example.com/new
example.com/temporary 307://https://example.com/new
```

PostGate 支持替换状态码和 `301`、`302`、`307`、`308` 重定向。

## 响应头、Cookie、CORS 与缓存

```text
api.example.com resHeaders://x-served-by=postgate
api.example.com resType://application/json
api.example.com resCharset://utf-8
api.example.com resCors://*
downloads.example.com/file attachment://report.pdf
assets.example.com cache://max-age=60
```

`resHeaders` 接受查询字符串风格的键值对或 JSON 对象。其他专用操作可以管理响应 Cookie、CORS、内容类型、字符集、附件文件名和缓存策略。

## 响应正文

```text
api.example.com/v1/user resBody://{"ok":true}
api.example.com/v1/user resMerge://{"source":"postgate"}
api.example.com resPrepend://before-
api.example.com resAppend://-after
api.example.com resReplace://production=local
```

`resBody` 会替换完整正文，`resMerge` 用于合并结构化内容。前置、追加和替换操作用于修改响应内容；`resWrite` 和 `resWriteRaw` 则会把收到的正文保存到磁盘。

修改压缩响应时，PostGate 可能需要先解码，再重新编码。添加改写后，请在 Capture 中核对最终的内容类型和正文。
