---
title: 路由与流量控制
description: 修改上游连接，使用代理，并模拟延迟、带宽和超时。
navTitle: 路由与流量
order: 15
---

# 路由与流量控制

## 路由到其他主机

`host` 会改变上游连接地址，同时保留原始请求 URL 和 `Host` 请求头：

```text
api.example.com host://127.0.0.1:3000
api.example.com host://local-api.internal:8443
```

也可以用完整的 HTTP/HTTPS 地址同时替换上游目标和路径前缀：

```text
https://example.com/api/ http://127.0.0.1:3000/local-api/
```

## 上游代理

PostGate 可以通过 HTTP、HTTPS、SOCKS4 或 SOCKS5 代理转发命中流量：

```text
example.com http-proxy://127.0.0.1:8080
example.com https-proxy://user:password@proxy.example.com:8443
example.com socks5://127.0.0.1:1080
```

代理凭证属于敏感信息，不要提交到共享规则文件。

## 延迟与带宽

延迟以毫秒为单位，限速值以字节每秒为单位：

```text
api.example.com reqDelay://200
api.example.com resDelay://800
uploads.example.com reqSpeed://65536
downloads.example.com resSpeed://131072
```

请求与响应可以分别控制，以模拟慢上传、服务器延迟和受限下载。

## 超时与中止

```text
api.example.com timeout://3000
api.example.com enable://abort
```

超时值以毫秒为单位。PostGate 还支持部分 `enable`/`disable` 传输选项，用于控制捕获记录的显示与隐藏、中止请求、强制写入正文，以及放宽合并大小限制。其他 Whistle 选项可能会被保留，但不会影响实际传输，详见[兼容性](/docs/rules/compatibility)。
