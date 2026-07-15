---
title: 插件
description: 构建、安装、启用并使用 PostGate 沙箱 JavaScript 插件。
navTitle: 插件
order: 31
---

# 插件

PostGate 插件是加载到嵌入式 V8 运行时中的 JavaScript 包。插件可以直接返回响应以跳过上游请求，也可以修改上游响应、保存隔离的 JSON 状态、输出日志、显示通知，以及注册沙箱 UI 面板。

## 运行时边界

插件不在 Node.js 中运行，也无法直接访问文件系统、进程或网络等全局 API。所有运行时依赖都必须打包进 JavaScript 入口文件。每个处理函数最多运行五秒，避免异常插件长时间阻塞代理流量。

## 包约定

使用支持的 npm 包名：

- `postgate-plugin-example`
- `@postgate/plugin-example`

通过 `main` 或 `module` 声明编译后的 JavaScript 入口文件：

```json
{
  "name": "postgate-plugin-example",
  "version": "1.0.0",
  "main": "index.js"
}
```

入口文件必须位于包目录内。PostGate 支持 CommonJS 和 ESM 默认导出；对于需要分发的插件，打包后的 CommonJS 通常最可靠。

## 最小插件

```js
module.exports = {
  name: 'example',
  version: '1.0.0',

  async onLoad(context) {
    context.logger.info('example loaded');
  },

  async handleRequest(request, context) {
    return null;
  },

  async handleResponse(request, response, context) {
    return {
      ...response,
      headers: { ...response.headers, 'x-postgate-plugin': 'example' }
    };
  }
};
```

TypeScript 项目可以使用 `@postgate/plugin-sdk` 提供的类型和响应辅助函数。安装前必须先编译或打包，因为 PostGate 不会在运行时转译 TypeScript。

## 调用插件

在 **Plugins** 页面从 npm 安装，或选择本地包目录。启用后通过规则关联流量：

```text
api.example.com plugin://example?mode=fixture&tenant=local
```

查询参数会写入 `context.ruleConfig`，`context.matchedPattern` 则包含命中的请求 URL。`handleRequest` 返回响应时会跳过上游请求，返回 `null` 时按正常流程继续；`handleResponse` 会收到上游响应，并可以对其进行修改。

## 正文与辅助函数

请求和响应正文使用以下结构：

```js
{
  body: 'base64 or text',
  body_base64: true
}
```

处理任意二进制数据时，应把 `body_base64` 设为 `true`。SDK 中的 `createResponse`、`jsonResponse` 和 `htmlResponse` 会生成编码正确的响应。

## 上下文 API

- `context.storage`：隔离的 `get`、`set`、`delete`、`has`、`keys`、`clear`。
- `context.logger`：`debug`、`info`、`warn`、`error`。
- `context.ui.registerPanel`：注册 HTML 或 URL 面板，并在沙箱 iframe 中渲染。
- `context.ui.unregisterPanel`：移除已注册的面板。
- `context.ui.toast`：显示普通、成功、警告或错误通知。
- `context.config`：由 PostGate 持久化并传入插件的配置。

启用状态和配置会跨重启保存。插件被禁用、更新、卸载或 PostGate 正常退出时会调用 `onUnload`。

完整示例见 [`examples/postgate-plugin-mock-api`](https://github.com/backrunner/postgate/tree/main/examples/postgate-plugin-mock-api)。
