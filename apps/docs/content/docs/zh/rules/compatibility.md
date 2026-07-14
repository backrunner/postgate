---
title: Whistle 兼容性
description: PostGate 对 Whistle 规则协议的完整支持、部分支持和不支持范围。
navTitle: 兼容性
order: 17
---

# Whistle 兼容性

PostGate 的兼容基线是 Whistle v2.10.6，对应提交 `5e9ac58c979d3713a59fdc3471df296cd0f66071`（2026-07-11）。“支持”不仅表示语法能够解析，也表示规则可以在 HTTP/1.1、HTTPS 中间人代理和 HTTP/2 链路中真正执行。

## 支持的能力

- 匹配：域名、URL 或路径前缀、精确匹配、通配符、正则表达式、无协议地址、端口和取反条件；还可以按方法、协议、端口、内容类型、请求头、主机名、客户端 IP、包含或排除条件以及状态码过滤。
- 路由：`host`、`hosts`、直接 `http`/`https` 映射、`proxy`、`http-proxy`、`https-proxy`、`socks`、`socks4`、`socks5`。
- 请求改写：查询参数、路径、方法、请求头、身份信息、Cookie、CORS、内容类型、字符集、正文，以及正文的前置、追加、替换和写入。
- 响应改写：状态码与重定向、响应头、字符集、Cookie、CORS、内容类型、正文合并与替换、HTML/JavaScript/CSS 注入、缓存、附件和正文写入。
- 流量控制：请求与响应延迟、请求与响应限速，以及请求超时。
- 资源：围栏内联值、全局 Value、本地文件、外部规则和远端 HTTP(S) 正文资源。

## 部分支持或 PostGate 特有

- `xhost` 当前等同 `host`，没有 Whistle 的失败回退差异。
- `delete` 可以删除请求头或响应头，但尚未覆盖 Whistle 的所有正文属性、Cookie 和 Trailer 删除形式。
- `headerReplace` 只会修改响应头，尚未实现 Whistle 完整的正则替换模型。
- `enable`/`disable` 已实现捕获记录显示与隐藏、中止请求、强制写入正文和放宽合并大小；其他选项可能不会影响实际传输。
- `weinre`/`debug` 使用 PostGate 的 Chobitsu/CDP 调试桥，而不是 Weinre。
- PostGate 插件使用 `@postgate/plugin-sdk`，不与 `whistle.*` 包二进制兼容。

## 不支持

以下操作会保留为解析警告，不会被静默丢弃：

- PAC：`pac`
- 动态脚本：`rulesFile`、`reqRules`、`reqScript`、`resRules`、`resScript`、`frameScript`
- 流式管道与响应尾部字段：`pipe`、`trailers`
- 单规则 TLS 回调：`cipher`、`tlsOptions`、`sniCallback`
- Whistle 界面样式：`style`
- 回退代理：`xproxy`、`xhttp-proxy`、`xhttps-proxy`、`xsocks`
- 原始文件与模板文件：`rawfile`、`xrawfile`、`tpl`、`xtpl`

## HTTP/3 边界

正式发布的安装包会启用可选的 QUIC 功能，并提供与现有规则链路共用处理流程的本机 HTTP/3 入口。它不是 MASQUE 代理：PostGate 无法解密或改写 HTTP/3 `CONNECT`、`CONNECT-UDP`，以及任意端到端 QUIC 数据报；这些请求会返回 `501 Not Implemented`。
