---
title: Installation and proxy setup
description: Install PostGate and route a browser or app through its localhost proxy.
navTitle: Install and connect
order: 2
---

# Installation and proxy setup

PostGate publishes signed macOS packages through [GitHub Releases](https://github.com/backrunner/postgate/releases). The download page checks for the latest stable release and lets you choose the build for Apple silicon or Intel Macs.

## Supported packages

| Platform | Package | Architecture | Availability |
| --- | --- | --- | --- |
| macOS | `.dmg` | Apple silicon and Intel | Available |
| Windows | — | x64 planned | Coming soon |

The macOS builds include HTTP/3 support, but QUIC remains experimental and is disabled by default. The Windows option stays visible to make its status clear, but it does not link to an installer yet.

## Connect a browser

Start the proxy from **Capture**. The toolbar shows the active address and port; the default is:

```text
127.0.0.1:8899
```

Set both the HTTP and HTTPS proxy fields in your browser or operating system's network settings to that address. Leave the SOCKS field empty unless another tool requires it.

PostGate binds to `127.0.0.1`, so the proxy is not reachable from other devices on the network. Captured traffic and rule processing stay on the computer running PostGate.

## Verify the connection

Visit an HTTP site and check that a request appears in **Capture**. If no request appears:

1. Confirm that the **Capture** toolbar shows the proxy as running.
2. Confirm that the browser or system proxy port matches PostGate's proxy port.
3. Disable proxy bypass rules for the hostname you are testing.
4. Check that another process is not already using the port.

HTTPS requests also require the [PostGate certificate](/docs/https-certificate).

## Change ports

Open **Settings → Proxy Configuration** to change the proxy port, toggle HTTP/2, configure experimental QUIC, or change the DevTools port. Stop and restart the proxy after changing a listener port.
