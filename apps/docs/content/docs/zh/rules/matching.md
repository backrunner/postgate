---
title: 匹配与筛选
description: 按域名、URL、路径、通配符、正则表达式、方法、协议、请求头和状态码匹配流量。
navTitle: 匹配与筛选
order: 11
---

# 匹配与筛选

每条规则的第一个字段用于选择需要匹配的流量。匹配范围应尽量精确，只覆盖真正需要处理的请求。

## 常用匹配方式

```text
# 发往某个域名的全部请求
api.example.com host://localhost:3000

# URL 或路径前缀
https://api.example.com/v1/ host://localhost:3000

# 通配符
*.example.com resHeaders://x-environment=local

# 正则表达式
/^https:\/\/api\.example\.com\/v[12]\//i reqHeaders://x-debug=1

# 端口
:8080 reqDelay://100
```

PostGate 支持域名、完整 URL、路径前缀、精确匹配、通配符、正则表达式、不含协议的地址和端口匹配。对于支持取反的规则，可以在匹配条件前加上 `!`，排除对应流量。

## 行内过滤条件

过滤条件会进一步缩小匹配范围，但不会改变规则操作：

```text
api.example.com filter://m:POST reqHeaders://x-write=1
api.example.com filter://p:https resHeaders://strict-transport-security=
api.example.com filter://port:443 reqHeaders://x-tls=1
api.example.com filter://h:content-type=json resDelay://200
api.example.com filter:///\/v2\//i reqHeaders://x-api-version=2
```

可以按请求方法、协议、端口、内容类型、请求头、主机名、客户端 IP、包含或排除条件，以及响应状态码过滤。

## 规则顺序

多条已启用规则同时命中时，PostGate 会按规则顺序收集适用操作。过于宽泛的规则可能意外覆盖更精确的模拟或路由规则。建议将全局请求头和流量控制放在独立规则组中，方便分别启用或停用。
