---
title: Values, debug, and plugin rules
description: Reuse stored content and connect matching traffic to Debug or a PostGate plugin.
navTitle: Values, debug, plugins
order: 16
---

# Values, debug, and plugin rules

## Reusable values

Open **Values** to create named text, JSON, HTML, JavaScript, CSS, or rule fragments. Names may contain `/` to create visual groups.

Values are useful for bodies and header maps that are too large for one rule line. PostGate can also resolve fenced inline values, local files, external rule includes, and bodies from remote HTTP(S) resources.

When a value is renamed, update every rule that references the old name.

## Debug rules

Attach the browser debug bridge to matching HTML documents:

```text
example.com debug://
example.com/app debug://checkout
```

Enabling a debug rule starts the local DevTools service. Matching pages receive the PostGate Chobitsu/CDP client and appear in **DevTools** after they load. Continue with the [Debug guide](/docs/debug).

## Plugin rules

An enabled plugin runs only for traffic attached through `plugin://`:

```text
api.example.com plugin://mock-api?mode=fixture&tenant=local
```

Query parameters are URL-decoded into `context.ruleConfig`. A request hook may return a complete response to skip the upstream request; a response hook may edit the upstream result. See [Plugins](/docs/plugins) for the package contract and runtime limits.
