---
title: Get started
description: Install PostGate, capture your first request, and create a local rewrite rule.
navTitle: Get started
order: 1
---

# Get started

PostGate is a local desktop proxy for inspecting and rewriting HTTP and HTTPS traffic. Capture, Whistle-compatible rules, request replay, browser debugging, and sandboxed plugins all live in one app.

## Install PostGate

Download PostGate for macOS from the [PostGate home page](/). Choose the Apple silicon or Intel build; the download links point directly to the matching file in the latest GitHub Release.

- macOS: open the `.dmg` and move PostGate to Applications.
- Windows: the native build is in preparation and marked **Coming soon** on the download page.

PostGate listens only on localhost by default. The default proxy address is `127.0.0.1:8899`.

## Capture your first request

1. Open **Capture** and press **Start**.
2. Set the HTTP proxy in the browser or operating system to `127.0.0.1:8899`.
3. Visit an HTTP page in that browser.
4. Select the request in PostGate to inspect the request and response headers, bodies, and timing.

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

The parse status below the editor reports syntax errors and flags protocols that PostGate recognizes but does not yet support.

## Where to go next

- [Capture traffic](/docs/capture) to filter requests, inspect bodies, and export sessions.
- [Rules](/docs/rules) for matching and actions.
- [Debug](/docs/debug) for console, page errors, Fetch, XHR, and CDP connections.
- [Replay](/docs/replay) to save, edit, and repeat requests.
- [Plugins](/docs/plugins) for JavaScript request and response handlers.
