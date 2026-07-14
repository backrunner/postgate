---
title: Capture traffic
description: Filter requests, inspect headers and bodies, read timing data, and export captures.
navTitle: Capture
order: 4
---

# Capture traffic

The Capture workspace is the live view of traffic passing through PostGate.

## Start and pause

Use **Start** to begin listening on the configured proxy port. **Pause** stops the listener; it does not delete captured rows. The address beside the button can be copied into browser or system proxy settings.

Use the trash button to clear saved capture history. Persistence is configurable, so long-running sessions do not have to keep every request in memory.

## Filter requests

The search field matches URL, host, and path. The filter menu narrows the list by:

- HTTP method
- status-code family
- content type
- host
- protocol
- whether a rule matched

Filters change only the visible list; they do not prevent the proxy from forwarding traffic.

## Inspect a request

Select a row to inspect request and response details. PostGate loads large bodies on demand and shows a suitable preview for text, JSON, HTML, images, and binary data. The timing waterfall separates connection, request, upstream, and response phases.

Sensitive headers such as `Authorization` and `Cookie` should be treated as credentials when sharing captures.

## Export and replay

Capture data can be exported as HAR for use in browser developer tools or other HTTP tooling. A captured request can also be imported into [Replay](/docs/replay), where the URL, method, query, headers, and body can be edited and executed repeatedly.

## Streaming traffic

WebSocket and Server-Sent Events connections remain visible as new messages arrive. HTTP/2 requests use the same capture and rule pipeline as HTTP/1.1. Experimental HTTP/3 ingress is available only in builds that include the QUIC feature.
