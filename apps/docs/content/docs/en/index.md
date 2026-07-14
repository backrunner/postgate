---
title: Get started
description: Install PostGate, capture your first request, and add a local rewrite rule.
navTitle: Get started
order: 1
---

# Get started

PostGate is a local desktop proxy for inspecting and changing HTTP and HTTPS traffic. It combines capture, Whistle-compatible rules, request replay, browser debugging, and a sandboxed plugin runtime in one app.

## Install PostGate

Download the build for your platform from the [PostGate home page](/). The download section links directly to the assets in the latest GitHub Release.

- macOS: open the `.dmg` and move PostGate to Applications.
- Windows: run the setup `.exe` or use the `.msi` package.
- Linux: run the `.AppImage` or install the `.deb` package.

PostGate listens only on localhost by default. The default proxy address is `127.0.0.1:8899`.

## Capture your first request

1. Open **Capture** and press **Start**.
2. Set the HTTP proxy in the browser or operating system to `127.0.0.1:8899`.
3. Open an HTTP URL in that browser.
4. Select the request in PostGate to inspect its headers, body, response, and timing.

For HTTPS traffic, [install and trust the PostGate root certificate](/docs/https-certificate) before browsing HTTPS pages.

## Add your first rule

Open **Rules**, create a rule group, and add:

```text
api.example.com host://127.0.0.1:3000
```

Enable the group. Requests to `api.example.com` now keep their original URL while PostGate connects to the local service on port `3000`.

To return a fixture instead:

```text
api.example.com/v1/user file:///absolute/path/to/user.json
```

The parse status below the editor reports invalid syntax and warns about recognized but unsupported protocols.

## Where to go next

- [Capture traffic](/docs/capture) for filters, bodies, and export.
- [Rules](/docs/rules) for matching and actions.
- [Debug](/docs/debug) for console, page errors, Fetch, XHR, and CDP connections.
- [Replay](/docs/replay) for repeatable requests and collections.
- [Plugins](/docs/plugins) for JavaScript request and response handlers.
