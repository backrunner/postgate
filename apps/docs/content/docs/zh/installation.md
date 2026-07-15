---
title: 安装与代理设置
description: 安装 PostGate，并让浏览器或应用通过本机代理访问网络。
navTitle: 安装与连接
order: 2
---

# 安装与代理设置

PostGate 通过 [GitHub Releases](https://github.com/backrunner/postgate/releases) 发布签名 macOS 安装包。首页会检查最新稳定版，并可选择 Apple 芯片或 Intel 版本。

## 安装包

| 平台 | 安装包 | 架构 | 状态 |
| --- | --- | --- | --- |
| macOS | `.dmg` | Apple 芯片与 Intel | 可下载 |
| Windows | — | 计划支持 x64 | 敬请期待 |

macOS 正式安装包包含 HTTP/3 支持，但 QUIC 仍是实验性功能，默认关闭。下载页会保留 Windows 入口以明确展示支持状态，但目前不会链接到安装文件。

## 连接浏览器

在 **Capture** 中启动代理。工具栏会显示当前地址和端口，默认值是：

```text
127.0.0.1:8899
```

把浏览器或系统网络设置里的 HTTP 和 HTTPS 代理都设为该地址。除非其他工具需要，否则 SOCKS 字段留空。

PostGate 只绑定 `127.0.0.1`，因此局域网内的其他设备无法连接。这样可以把捕获数据和规则操作限制在运行 PostGate 的电脑上。

## 验证连接

打开一个 HTTP 网站，确认 **Capture** 出现请求。如果没有：

1. 确认 Capture 工具栏显示代理正在运行。
2. 确认浏览器端口与 PostGate 一致。
3. 关闭目标域名的代理绕过设置。
4. 检查该端口是否已被其他进程占用。

HTTPS 还需要安装 [PostGate 证书](/docs/https-certificate)。

## 修改端口

在 **Settings → Proxy Configuration** 中修改代理端口、HTTP/2、实验性 QUIC 和 DevTools 端口。修改监听端口后需停止并重新启动对应服务。
