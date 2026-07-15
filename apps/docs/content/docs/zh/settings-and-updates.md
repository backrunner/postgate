---
title: 设置、配置档案与更新
description: 配置监听端口、迁移配置档案、同步设置，并接收来自 GitHub Releases 的签名更新。
navTitle: 设置与更新
order: 32
---

# 设置、配置档案与更新

## 代理配置

在 Settings 中可以设置本机代理端口、HTTP/2、实验性 QUIC/HTTP/3，以及 DevTools 服务端口。如果当前安装包不支持 QUIC，对应开关会被禁用并显示原因。

修改监听端口后，需要重启对应服务。请避开已被开发服务器或其他代理占用的端口。

## 软件更新

PostGate 会读取最新 GitHub Release 中经过签名的更新清单。Settings 页面提供以下选项：

- 手动 **Check for Updates**
- 启动时自动检查
- 可选的后台下载
- 下载、安装与重启进度

发布流程目前会分别构建 Apple 芯片与 Intel 的 macOS 安装包，为自动更新文件签名，并验证 `latest.json` 中的两个 Darwin 条目。只有两种架构都构建成功，Release 草稿才会正式发布。安装更新前，PostGate 还会再次校验签名。

网站下载区会调用 GitHub 的最新版本接口，并直接链接到选中的 macOS 安装文件。Windows 会显示“敬请期待”，且不提供下载操作。如果尚无 macOS Release，或 GitHub 暂时无法访问，macOS 按钮会转到仓库的 Releases 页面。

## 配置档案迁移

配置档案可以包含规则、Values、Replay 集合、证书材料、应用偏好和同步设置。导入时，PostGate 会先读取并展示档案摘要，再恢复其中的数据。也可以单独把兼容的 Whistle 规则导入一个新规则组。

包含 CA 私钥、WebDAV 密码、请求头或请求正文的配置档案属于敏感文件，请妥善保存和传输。

## 设置同步

同步与手动迁移使用相同的配置档案快照格式：

- iCloud 在支持的 macOS 构建上写入本地 Cloud Drive 文件。
- WebDAV 会把 JSON 快照上传到指定的服务地址和远端路径。

执行 **Push** 或 **Pull** 前，请先保存同步服务设置。Push 会用本地状态覆盖远端快照；Pull 会把远端快照导入 PostGate。如果当前环境已经有数据，首次 Pull 前建议先导出备份。

## 外观

可以选择浅色、深色或跟随系统。文档站也支持三种主题模式，但偏好与桌面应用分别保存。
