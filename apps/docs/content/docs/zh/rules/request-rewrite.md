---
title: 改写请求
description: 修改请求 URL、方法、请求头、Cookie、CORS 信息和正文。
navTitle: 请求改写
order: 12
---

# 改写请求

请求动作会在 PostGate 连接上游前执行。

## 查询参数、路径和方法

```text
api.example.com urlParams://debug=true&locale=zh
api.example.com/v1 pathReplace://v1=v2
api.example.com method://POST
```

`urlParams` 修改查询参数，`pathReplace` 替换路径中的文本，`method` 修改发往上游的请求方法。

## 请求头与身份信息

```text
api.example.com reqHeaders://x-environment=local
api.example.com ua://PostGate-Test
api.example.com referer://https://app.example.com/
api.example.com forwardedFor://127.0.0.1
api.example.com auth://user:password
```

`reqHeaders` 接受查询字符串风格的键值对或 JSON 对象。其他操作可以设置 User Agent、Referer、转发 IP、Basic Auth、请求 Cookie、CORS、内容类型和字符集。

## 请求正文

```text
api.example.com/v1/user reqBody://{"name":"Ada"}
api.example.com/v1/user reqMerge://{"debug":true}
api.example.com reqPrepend://prefix-
api.example.com reqAppend://-suffix
api.example.com reqReplace://old=new
```

`reqBody` 会替换完整正文，`reqMerge`/`params` 用于合并结构化数据。前置、追加和替换操作适合处理文本正文。`reqWrite` 与 `reqWriteRaw` 可以把收到的请求正文写入本地文件。

修改正文后，PostGate 会在转发前更新相关长度信息。改变数据格式时，还应同时设置 `reqType`，例如 `reqType://application/json`。
