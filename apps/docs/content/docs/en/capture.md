---
title: Capture traffic
description: Filter requests, inspect headers and bodies, read timing data, and export captures.
navTitle: Capture
order: 4
---

# Capture traffic

The **Capture** workspace shows traffic as it passes through PostGate.

## Start and pause

Use **Start** to begin listening on the configured proxy port. **Pause** stops new rows from being added to the list while the proxy continues forwarding traffic; it does not delete anything already captured. Copy the address beside the button into your browser or system proxy settings.

Use the trash button to clear the capture history. Retention and persistence can be configured in **Settings** for longer sessions.

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

Select a row to inspect the request and response. PostGate loads large bodies on demand and provides dedicated previews for text, JSON, HTML, images, and binary data. The timing waterfall separates connection, request, upstream wait, and response phases.

Sensitive headers such as `Authorization` and `Cookie` should be treated as credentials when sharing captures.

## Export and replay

Capture data can be exported as HAR for use in browser developer tools or other HTTP tooling. A captured request can also be imported into [Replay](/docs/replay), where the URL, method, query, headers, and body can be edited and executed repeatedly.

## Streaming traffic

WebSocket and Server-Sent Events (SSE) connections remain visible as new messages arrive. HTTP/2 requests use the same capture and rule pipeline as HTTP/1.1. The experimental HTTP/3 listener is available only in builds that include the QUIC feature.
