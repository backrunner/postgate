---
title: Fetch, XHR, and CDP
description: Inspect page network activity and connect compatible tooling to PostGate's local CDP endpoint.
navTitle: Network and CDP
order: 22
---

# Fetch, XHR, and CDP

The debug client observes Fetch and XMLHttpRequest activity inside the page. This complements **Capture**: Capture shows traffic moving through the proxy, while Debug connects that activity to a specific browser page and runtime context.

## Network events

Use the network panel to correlate method, URL, status, and timing with console or error events from the same session. Requests that bypass the configured proxy may still appear in the page instrumentation but will not have a corresponding Capture row.

## Connect a CDP client

Query the discovery endpoint:

```bash
curl http://127.0.0.1:<debug-port>/json/list
```

Choose the page and connect to its `webSocketDebuggerUrl`:

```text
ws://127.0.0.1:<debug-port>/devtools/page/<session-id>
```

Use a client that supports the Chrome DevTools Protocol messages needed by Chobitsu. Not every browser-native DevTools feature maps perfectly to an in-page CDP implementation.

## Session lifecycle

A session closes when the page disconnects, reloads without matching the rule, or PostGate stops the debug service. Reloading a page that still matches the rule creates or refreshes its session. Remove stale sessions from the **DevTools** workspace when diagnosing reconnection problems.
