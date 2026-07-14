---
title: Rules
description: Understand PostGate rule syntax, groups, values, and execution order.
navTitle: Rules overview
order: 10
collapsed: false
---

# Rules

Rules match traffic and apply one or more actions before a request is forwarded or before its response is returned. PostGate follows the familiar Whistle rule format:

```text
pattern action [action ...] [filter ...]
```

For example:

```text
api.example.com host://127.0.0.1:3000 reqHeaders://x-postgate=local
```

## Rule groups

Rules live in named groups. A group must be enabled before its rules enter the proxy pipeline. Groups can be created, renamed, reordered, and disabled without deleting their content.

The editor parses changes as you type. Errors stop invalid rules from running, while warnings identify protocols that PostGate recognizes but does not apply.

## Comments and multiple actions

Lines beginning with `#` are comments. Put additional actions on the same line:

```text
# route locally, add a header, and slow the response
api.example.com host://localhost:3000 reqHeaders://x-env=local resDelay://250
```

## Values

Large JSON, HTML, JavaScript, or header maps do not need to live on one rule line. Save reusable content in **Values** and reference it from a compatible action. Rules can also load local files, external rule files, and bodies from HTTP(S) resources.

## Compatibility

PostGate targets Whistle v2.10.6 syntax, but it is not a byte-for-byte Whistle runtime. See [Compatibility](/docs/rules/compatibility) before migrating a large ruleset.
