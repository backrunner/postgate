---
title: Values、Debug 与插件规则
description: 复用存储内容，并把命中流量接入 Debug 或 PostGate 插件。
navTitle: Values、Debug、插件
order: 16
---

# Values、Debug 与插件规则

## 可复用 Values

打开 **Values** 可以创建命名的文本、JSON、HTML、JavaScript、CSS 或规则片段。名称可使用 `/` 形成视觉分组。

Values 适合存放无法在单行规则中清晰表达的正文和请求头映射。PostGate 还支持围栏内联值、本地文件、外部规则引用和远端 HTTP(S) 正文资源。

重命名 Value 后，需要同步更新所有引用旧名称的规则。

## Debug 规则

为命中的 HTML 页面注入浏览器调试 Bridge：

```text
example.com debug://
example.com/app debug://checkout
```

启用 Debug 规则后，本地 DevTools 服务会自动启动。PostGate 会向命中页面注入 Chobitsu/CDP 客户端；页面加载后，就会出现在 **DevTools** 中。继续阅读 [Debug 指南](/docs/debug)。

## 插件规则

已启用插件只会处理通过 `plugin://` 关联的流量：

```text
api.example.com plugin://mock-api?mode=fixture&tenant=local
```

查询参数会先进行 URL 解码，再写入 `context.ruleConfig`。请求钩子可以直接返回完整响应，从而跳过上游请求；响应钩子则可以修改上游返回的结果。包格式和运行限制见[插件](/docs/plugins)。
