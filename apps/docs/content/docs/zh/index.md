---
title: 快速开始
description: 安装 PostGate、捕获第一个请求，并创建一条本地改写规则。
navTitle: 快速开始
order: 1
---

# 快速开始

PostGate 是一款用于检查和改写 HTTP/HTTPS 流量的本地桌面代理。流量捕获、Whistle 兼容规则、请求重放、浏览器调试和沙箱插件都集中在同一个应用中。

## 安装 PostGate

从 [PostGate 首页](/)下载 macOS 版本，并根据 Mac 的处理器选择 Apple 芯片或 Intel 安装包。下载按钮会直接指向最新 GitHub Release 中的对应文件。

- macOS：打开 `.dmg`，将 PostGate 移到 Applications。
- Windows：原生版本正在准备中，下载页会显示“敬请期待”。

PostGate 默认只监听本机，代理地址是 `127.0.0.1:8899`。

## 捕获第一个请求

1. 打开 **Capture**，点击 **Start**。
2. 将浏览器或系统的 HTTP 代理设置为 `127.0.0.1:8899`。
3. 在该浏览器中访问一个 HTTP 页面。
4. 在 PostGate 中选择请求，查看请求头、响应头、正文和耗时。

如需捕获 HTTPS 流量，请先[安装并信任 PostGate 根证书](/docs/https-certificate)。

## 添加第一条规则

打开 **Rules**，新建规则组并输入：

```text
api.example.com host://127.0.0.1:3000
```

启用规则组后，再访问 `api.example.com`。请求 URL 会保持不变，但 PostGate 会把上游连接转到本机的 `3000` 端口。

要直接返回本地文件，可以使用：

```text
api.example.com/v1/user file:///absolute/path/to/user.json
```

编辑器下方的解析状态会报告语法错误，并标出 PostGate 已识别但尚不支持的协议。

## 下一步

- [流量捕获](/docs/capture)：筛选请求、检查正文并导出会话。
- [规则](/docs/rules)：匹配方式和动作。
- [Debug](/docs/debug)：Console、页面错误、Fetch、XHR 和 CDP。
- [Replay](/docs/replay)：保存、编辑并重复执行请求。
- [插件](/docs/plugins)：JavaScript 请求与响应处理器。
