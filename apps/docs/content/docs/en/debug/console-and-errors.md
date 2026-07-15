---
title: Console and page errors
description: Read console calls, uncaught exceptions, and unhandled promise rejections from connected pages.
navTitle: Console and errors
order: 21
---

# Console and page errors

Select a connected page in **DevTools** to view only its events, or choose **All Sessions** to combine events from every active page.

## Console capture

The injected client captures standard console methods and forwards their level, arguments, timestamp, and source context to PostGate. This is useful when opening the browser's built-in developer tools would interfere with the scenario being tested.

Console events remain scoped to their debug session. Clear the panel when repeating a scenario so old output is not mistaken for the new run.

## Runtime errors

PostGate records:

- uncaught errors reported through `window.onerror`
- unhandled promise rejections
- relevant source location and stack information supplied by the browser

An empty panel does not prove that no code failed before injection. Reload the page after enabling the `debug://` rule so the bridge is present from the initial HTML response.

## No session appears

Check that the rule group is enabled, the document matches the rule, and the response is HTML. Then verify that Content Security Policy, a browser extension, or a page-level WebSocket restriction is not blocking the local bridge.
