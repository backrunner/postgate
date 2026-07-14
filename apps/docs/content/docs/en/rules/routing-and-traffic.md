---
title: Routing and traffic control
description: Route upstream connections, use proxies, and simulate delay, bandwidth, or timeout conditions.
navTitle: Routing and traffic
order: 15
---

# Routing and traffic control

## Route to another host

`host` changes the upstream connection while preserving the original request URL and host information:

```text
api.example.com host://127.0.0.1:3000
api.example.com host://local-api.internal:8443
```

A direct HTTP or HTTPS target can also replace the upstream destination and path base:

```text
https://example.com/api/ http://127.0.0.1:3000/local-api/
```

## Upstream proxies

PostGate can forward matching traffic through HTTP, HTTPS, SOCKS4, or SOCKS5 proxies:

```text
example.com http-proxy://127.0.0.1:8080
example.com https-proxy://user:password@proxy.example.com:8443
example.com socks5://127.0.0.1:1080
```

Proxy credentials are sensitive. Do not commit them in shared rule files.

## Delay and bandwidth

Values are milliseconds for delays and bytes per second for speed limits:

```text
api.example.com reqDelay://200
api.example.com resDelay://800
uploads.example.com reqSpeed://65536
downloads.example.com resSpeed://131072
```

Use request and response controls independently to model slow uploads, server latency, and constrained downloads.

## Timeout and abort behavior

```text
api.example.com timeout://3000
api.example.com enable://abort
```

Timeout values are milliseconds. PostGate also supports selected `enable` and `disable` transport flags for capture visibility, abort behavior, forced body writes, and larger merges. Other Whistle flags may be retained without affecting transport; consult [Compatibility](/docs/rules/compatibility).
