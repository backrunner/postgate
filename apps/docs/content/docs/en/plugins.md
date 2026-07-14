---
title: Plugins
description: Build, install, enable, and invoke sandboxed JavaScript plugins in PostGate.
navTitle: Plugins
order: 31
---

# Plugins

PostGate plugins are JavaScript packages loaded into an embedded V8 runtime. They can short-circuit requests, modify responses, keep isolated JSON state, log messages, show notifications, and register sandboxed UI panels.

## Runtime boundary

Plugins do not run in Node.js and do not receive filesystem, process, or network globals. Bundle runtime dependencies into the JavaScript entry file. A handler has a five-second limit so a stalled plugin cannot block proxy traffic indefinitely.

## Package contract

Use a supported npm package name:

- `postgate-plugin-example`
- `@postgate/plugin-example`

Declare a compiled JavaScript entry with `main` or `module`:

```json
{
  "name": "postgate-plugin-example",
  "version": "1.0.0",
  "main": "index.js"
}
```

The entry must stay inside the package directory. CommonJS and a default ESM export are supported; bundled CommonJS is the most predictable distribution format.

## Minimal plugin

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

Use `@postgate/plugin-sdk` for TypeScript types and response helpers. Compile or bundle TypeScript before installation; PostGate does not transpile it.

## Invoke a plugin

Install the package from npm or choose a local package directory on the **Plugins** page. Enable it, then attach it to matching traffic:

```text
api.example.com plugin://example?mode=fixture&tenant=local
```

Query parameters become `context.ruleConfig`, and `context.matchedPattern` contains the matched request URL. Returning a response from `handleRequest` skips upstream; returning `null` continues. `handleResponse` receives and may change the upstream response.

## Bodies and helpers

Request and response bodies use:

```js
{
  body: 'base64 or text',
  body_base64: true
}
```

Set `body_base64` for arbitrary bytes. The SDK helpers `createResponse`, `jsonResponse`, and `htmlResponse` produce correctly encoded responses.

## Context APIs

- `context.storage`: isolated `get`, `set`, `delete`, `has`, `keys`, and `clear` operations.
- `context.logger`: `debug`, `info`, `warn`, and `error`.
- `context.ui.registerPanel`: add an HTML or URL panel rendered in a sandboxed iframe.
- `context.ui.unregisterPanel`: remove a registered panel.
- `context.ui.toast`: show an info, success, warning, or error notification.
- `context.config`: persisted configuration supplied by the host.

Enabled state and configuration persist across restarts. `onUnload` runs when a plugin is disabled, updated, uninstalled, or PostGate exits normally.

For a working package, see [`examples/postgate-plugin-mock-api`](https://github.com/backrunner/postgate/tree/main/examples/postgate-plugin-mock-api).
