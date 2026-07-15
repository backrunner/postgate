---
title: Debug
description: Inject PostGate's browser bridge and inspect live pages through a local CDP-compatible service.
navTitle: Debug overview
order: 20
collapsed: false
---

# Debug

PostGate Debug connects pages matched by a rule to the desktop app through a local Chobitsu/CDP bridge. It captures console output, runtime errors, Fetch, and XHR activity without sending data to a remote service.

## Start a session

1. Make sure HTTPS capture works for the target page.
2. Add and enable a rule:

```text
example.com debug://
```

3. Reload the target page through the PostGate proxy.
4. Open **DevTools** and select the connected page.

The debug server starts automatically when an enabled debug rule exists. Its localhost port is configured under **Settings → Proxy Configuration**.

## What gets injected

For matching HTML responses, PostGate injects a lightweight client that opens a WebSocket to the local debug server and exposes Chrome DevTools Protocol commands through Chobitsu. Non-HTML resources are left unchanged.

## Discovery endpoints

The service exposes CDP-style discovery on localhost:

```text
http://127.0.0.1:<debug-port>/json/list
```

Each session includes a `webSocketDebuggerUrl` under `/devtools/page/<session-id>`. This lets a compatible local CDP client inspect the same page while PostGate remains the traffic proxy.

## Keep the boundary local

The debug server binds to `127.0.0.1`. Do not expose it through a tunnel or reverse proxy: a CDP connection can observe and control the connected page.
