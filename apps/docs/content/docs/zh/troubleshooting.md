---
title: 故障排查
description: 排查流量捕获、证书信任、规则匹配、Debug、插件和更新问题。
navTitle: 故障排查
order: 40
---

# 故障排查

## 没有捕获到请求

- 确认 Capture 显示代理正在运行。
- 把工具栏的完整地址复制到 HTTP 和 HTTPS 代理字段。
- 检查目标域名是否位于浏览器或系统绕过列表。
- 确认端口未被其他进程占用。
- 先测试 HTTP 页面，把代理路由问题与证书信任问题分开。

## HTTPS 出现证书警告

- 在 **Settings → HTTPS & Security** 安装根 CA。
- 手动导入时，将证书作为受信任根证书，而不是客户端证书。
- 修改信任后重启浏览器。
- 检查浏览器是否使用独立于操作系统的证书库。
- 安装新 CA 前删除过期的 PostGate CA。

## 规则没有生效

- 启用规则所在的规则组。
- 查看编辑器解析状态中的错误或“不支持该协议”警告。
- 缩小匹配范围，并在 Capture 中核对实际请求 URL。
- 检查方法、协议、请求头、状态码，以及包含和排除条件。
- 检查前面的宽泛规则是否已经修改同一请求。

## 文件或注入内容没有出现

- 使用绝对路径，并确认 PostGate 有读取权限。
- 确认注入目标的 `content-type` 兼容。
- 在 Capture 中查看压缩和改写后的响应头。
- 修改前端资源替换规则后，禁用浏览器缓存再刷新。

## Debug 没有连接页面

- 重载页面前启用匹配的 `debug://` 规则。
- 确认主文档是 HTML，并通过 PostGate。
- 检查 DevTools 端口冲突。
- 检查内容安全策略（CSP）或浏览器扩展是否阻止了本地 WebSocket。

## 插件无法加载

- 包名使用 `postgate-plugin-*` 或 `@postgate/plugin-*`。
- 确认 `main` 或 `module` 指向包内 JavaScript。
- 将运行时依赖打包进插件，不要使用 Node.js 全局对象。
- 启用插件，并使用匹配的 `plugin://name` 规则。
- 请求和响应处理器必须在五秒限制内完成。

## 更新检查失败

- 打开 [GitHub Releases](https://github.com/backrunner/postgate/releases)，确认网络可用且存在稳定版。
- 确认安装包包含用于生产环境更新校验的公钥。
- 不要重命名或修改 GitHub Release 中的 `latest.json` 及其签名文件。
- 测试预览版时，请在 Settings 中选择 Beta 更新渠道；Stable 会按设计忽略预览版。

提交 Issue 时，请提供 PostGate 版本、操作系统、能够复现问题的最小规则、相关 Capture 元数据和完整错误信息。提交前请先移除凭证、Cookie、证书密钥，以及私有的请求或响应正文。
