---
title: 安装与代理设置
description: 安装 PostGate，并让浏览器或应用的流量通过本机代理。
navTitle: 安装与连接
order: 2
---

# 安装与代理设置

PostGate 通过 [GitHub Releases](https://github.com/backrunner/postgate/releases) 发布已签名的 macOS 安装包。下载页会检查最新稳定版，并允许你选择适用于 Apple 芯片或 Intel Mac 的版本。

## 安装包

| 平台 | 安装包 | 架构 | 状态 |
| --- | --- | --- | --- |
| macOS | `.dmg` | Apple 芯片与 Intel | 可下载 |
| Windows | — | 计划支持 x64 | 敬请期待 |

macOS 安装包包含 HTTP/3 支持，但 QUIC 仍处于实验阶段，默认关闭。下载页会保留 Windows 选项以明确展示当前状态，但暂时不会提供安装文件。

## 连接浏览器

在 **Capture** 中启动代理。工具栏会显示当前地址和端口，默认值是：

```text
127.0.0.1:8899
```

将浏览器或系统网络设置中的 HTTP 和 HTTPS 代理都设为该地址。除非其他工具明确需要，否则请将 SOCKS 字段留空。

PostGate 只绑定 `127.0.0.1`，因此局域网内的其他设备无法访问该代理。所有流量捕获和规则处理都留在运行 PostGate 的电脑上。

## 验证连接

访问一个 HTTP 网站，确认 **Capture** 中出现请求。如果没有看到请求：

1. 确认 **Capture** 工具栏显示代理正在运行。
2. 确认浏览器或系统代理端口与 PostGate 的代理端口一致。
3. 关闭目标域名的代理绕过设置。
4. 检查该端口是否已被其他进程占用。

HTTPS 还需要安装 [PostGate 证书](/docs/https-certificate)。

## 修改端口

在 **Settings → Proxy Configuration** 中可以修改代理端口、HTTP/2、实验性 QUIC 和 DevTools 端口。更改监听端口后，需要停止并重新启动对应服务。
