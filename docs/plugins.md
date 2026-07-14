# PostGate Plugins

PostGate plugins are JavaScript packages loaded into an embedded V8 runtime. They do not run in Node.js and do not receive filesystem, process, or network globals. Bundle runtime dependencies into the entry file and use the APIs exposed through the plugin context.

## Package Contract

The npm package name must use one of these forms:

- `postgate-plugin-example`
- `@postgate/plugin-example`

The package must declare a JavaScript entry through `main` or `module`. The entry path must remain inside the package directory. CommonJS and a default ESM export are supported; bundled CommonJS is the most predictable format.

```json
{
  "name": "postgate-plugin-example",
  "version": "1.0.0",
  "main": "index.js"
}
```

```js
module.exports = {
  async onLoad(context) {},
  async onUnload(context) {},
  async handleRequest(request, context) {
    return null;
  },
  async handleResponse(request, response, context) {
    return response;
  },
};
```

Use `@postgate/plugin-sdk` for TypeScript types and response helpers. Compile or bundle TypeScript before installation; PostGate executes JavaScript and does not transpile TypeScript.

## Rules

Enable the installed plugin on the Plugins page, then attach it to traffic with a rule:

```text
api.example.com plugin://example?mode=fixture&tenant=local
```

Query parameters are URL-decoded and exposed as `context.ruleConfig`. `context.matchedPattern` contains the matched request URL and `context.logger` writes to the PostGate log.

`handleRequest` may return a response to skip the upstream request, or `null` to continue. `handleResponse` may change the status, headers, and body returned by the upstream server. A plugin call is limited to five seconds so a stalled plugin cannot block proxy traffic indefinitely.

## Bodies

Request and response bodies use this shape:

```js
{
  body: "base64 or text",
  body_base64: true
}
```

Set `body_base64` to `true` for arbitrary bytes. The SDK helpers `createResponse`, `jsonResponse`, and `htmlResponse` generate valid encoded responses.

## Context APIs

- `context.storage`: isolated persistent JSON storage with `get`, `set`, `delete`, `has`, `keys`, and `clear`.
- `context.logger`: `debug`, `info`, `warn`, and `error` logging methods.
- `context.ui.registerPanel(panel)`: registers an HTML or URL panel on the Plugins page. PostGate assigns the owning plugin ID and renders the panel in a sandboxed iframe.
- `context.ui.unregisterPanel(id)`: removes a registered panel.
- `context.ui.toast(message, type)`: shows an `info`, `success`, `warning`, or `error` notification.
- `context.config`: configuration supplied when the plugin is enabled through the host API.

Enabled state and configuration persist across app restarts. `onUnload` runs when the plugin is disabled, updated, uninstalled, or when PostGate exits normally.

## Installation

Use the Plugins page to install a published package from npm or select a local package directory. Installation is staged inside PostGate's plugin directory before replacing an existing version. Package names, entry paths, and symlinks are validated to keep an install inside that directory.

For local testing, install [the mock API example](../examples/postgate-plugin-mock-api) and use:

```text
example.test plugin://mock-api?mode=fixture
```
