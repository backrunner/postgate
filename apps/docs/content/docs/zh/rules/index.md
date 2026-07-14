---
title: 规则
description: 理解 PostGate 的规则语法、规则组、Values 和执行顺序。
navTitle: 规则概览
order: 10
collapsed: false
---

# 规则

规则用于匹配流量，并在请求发往上游前或响应返回客户端前执行一个或多个操作。PostGate 使用与 Whistle 相似的格式：

```text
pattern action [action ...] [filter ...]
```

例如：

```text
api.example.com host://127.0.0.1:3000 reqHeaders://x-postgate=local
```

## 规则组

规则保存在命名规则组中。只有启用的规则组才会进入代理处理链路。规则组支持创建、重命名、排序和停用，无需删除其中的内容。

编辑器会在输入时实时解析规则。错误会阻止无效规则运行；警告则会标出 PostGate 能够识别、但尚不会执行的协议。

## 注释与多个动作

以 `#` 开头的行是注释。一行可以包含多个动作：

```text
# 路由到本地、添加请求头，并延迟响应
api.example.com host://localhost:3000 reqHeaders://x-env=local resDelay://250
```

## Values

较大的 JSON、HTML、JavaScript 或请求头映射不必直接写在规则中。可以先把可复用内容保存到 **Values**，再从支持该功能的操作中引用。规则也可以读取本地文件、外部规则文件和远端 HTTP(S) 正文资源。

## 兼容性

PostGate 以 Whistle v2.10.6 语法为兼容基线，但两者的运行时并不完全相同。迁移大型规则集前，请先查看[兼容性说明](/docs/rules/compatibility)。
