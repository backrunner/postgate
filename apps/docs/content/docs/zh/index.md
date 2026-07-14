---
title: 快速开始
description: 安装 PostGate、捕获第一个请求，并添加一条本地改写规则。
navTitle: 快速开始
order: 1
---

# 快速开始

PostGate 是一款用于查看和修改 HTTP/HTTPS 流量的本地桌面代理。流量捕获、Whistle 兼容规则、请求重放、浏览器调试和沙箱插件都可以在同一个应用中完成。

## 安装 PostGate

从 [PostGate 首页](/)下载适合当前平台的版本。首页下载区会直接链接到最新 GitHub Release 中的安装文件。

- macOS：打开 `.dmg`，将 PostGate 移到 Applications。
- Windows：运行 setup `.exe`，或使用 `.msi` 安装包。
- Linux：运行 `.AppImage`，或安装 `.deb` 软件包。

PostGate 默认只监听本机，代理地址是 `127.0.0.1:8899`。

## 捕获第一个请求

1. 打开 **Capture**，点击 **Start**。
2. 将浏览器或系统 HTTP 代理设置为 `127.0.0.1:8899`。
3. 在该浏览器中打开一个 HTTP 地址。
4. 在 PostGate 中选择请求，查看请求头、正文、响应和耗时。

要捕获 HTTPS，请先[安装并信任 PostGate 根证书](/docs/https-certificate)。

## 添加第一条规则

打开 **Rules**，新建规则组并输入：

```text
api.example.com host://127.0.0.1:3000
```

启用规则组。之后访问 `api.example.com` 时，请求 URL 保持不变，但 PostGate 会把上游连接转到本机的 `3000` 端口。

要直接返回本地文件，可以使用：

```text
api.example.com/v1/user file:///absolute/path/to/user.json
```

编辑器下方会显示解析错误，并对已识别但尚不支持的协议给出警告。

## 下一步

- [流量捕获](/docs/capture)：筛选、正文和导出。
- [规则](/docs/rules)：匹配方式和动作。
- [Debug](/docs/debug)：Console、页面错误、Fetch、XHR 和 CDP。
- [Replay](/docs/replay)：可重复执行的请求和集合。
- [插件](/docs/plugins)：JavaScript 请求与响应处理器。
