---
title: Installation and proxy setup
description: Install PostGate and route a browser or app through its localhost proxy.
navTitle: Install and connect
order: 2
---

# Installation and proxy setup

PostGate publishes signed macOS packages through [GitHub Releases](https://github.com/backrunner/postgate/releases). The landing page checks GitHub for the newest stable release and lets you choose the Apple silicon or Intel build.

## Supported packages

| Platform | Package | Architecture | Availability |
| --- | --- | --- | --- |
| macOS | `.dmg` | Apple silicon and Intel | Available |
| Windows | — | x64 planned | Coming soon |

The macOS release builds include HTTP/3 support, although QUIC remains experimental and disabled by default. Windows remains visible on the download page so its availability is explicit, but it does not link to an installer yet.

## Connect a browser

Start the proxy from **Capture**. The toolbar shows the active address and port; the default is:

```text
127.0.0.1:8899
```

Set both the HTTP and HTTPS proxy fields in your browser or operating-system network settings to that address. Leave any SOCKS field empty unless another tool needs it.

PostGate binds to `127.0.0.1`, so other devices on the network cannot use the proxy. This keeps captured traffic and rule actions on the computer running PostGate.

## Verify the connection

Browse to an HTTP site and check that a row appears in **Capture**. If it does not:

1. Confirm that the Capture toolbar says the proxy is running.
2. Confirm that the configured port matches PostGate's proxy port.
3. Disable proxy bypass rules for the hostname you are testing.
4. Check that another process is not already using the port.

HTTPS requests also require the [PostGate certificate](/docs/https-certificate).

## Change ports

Open **Settings → Proxy Configuration** to change the proxy port, toggle HTTP/2, configure experimental QUIC, or change the DevTools port. Stop and restart the proxy after changing a listener port.
