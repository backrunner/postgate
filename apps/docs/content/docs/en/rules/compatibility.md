---
title: Whistle compatibility
description: Understand which Whistle protocols PostGate supports and where behavior differs.
navTitle: Compatibility
order: 17
---

# Whistle compatibility

PostGate's compatibility baseline is Whistle v2.10.6 at commit `5e9ac58c979d3713a59fdc3471df296cd0f66071` (July 11, 2026). Here, “supported” means that PostGate both parses the rule and applies it across HTTP/1.1, HTTPS MITM, and HTTP/2 traffic.

## Supported families

- Matching: domain, URL/path prefix, exact, wildcard, regular expression, no-schema, port, negation, and method/protocol/port/content-type/header/host/client-IP/include/exclude/status filters.
- Routing: `host`, `hosts`, direct `http`/`https` mapping, `proxy`, `http-proxy`, `https-proxy`, `socks`, `socks4`, and `socks5`.
- Request rewriting: `urlParams`, `params`/`reqMerge`, `pathReplace`, `method`, request headers, identity, cookies, CORS, type, charset, body, prepend/append/replace, and body writes.
- Response rewriting: status and redirects, response headers, charset, cookies, CORS, type, body, merge/prepend/append/replace, HTML/JS/CSS actions, cache, attachment, and body writes.
- Traffic controls: request/response delay, request/response speed, and request timeout.
- Resources: fenced values, global values, local files, external rules, and remote HTTP(S) body resources.

## Partial or PostGate-specific

- `xhost` currently behaves like `host`; Whistle's fallback behavior after a failed connection is not implemented.
- `delete` removes request or response headers, but not every Whistle body-property, cookie, or trailer form.
- `headerReplace` is response-header modification rather than Whistle's complete regex replacement model.
- `enable` and `disable` implement capture/hide, abort, forced body writes, and larger merge limits. Other flags may have no transport effect.
- `weinre` and `debug` use PostGate's Chobitsu/CDP bridge, not Weinre.
- PostGate plugins use `@postgate/plugin-sdk` and are not binary-compatible with `whistle.*` packages.

## Unsupported

The parser preserves these actions as unsupported warnings rather than silently discarding them:

- PAC execution: `pac`
- dynamic scripts: `rulesFile`, `reqRules`, `reqScript`, `resRules`, `resScript`, `frameScript`
- stream pipes and response trailers: `pipe`, `trailers`
- per-rule TLS callbacks: `cipher`, `tlsOptions`, `sniCallback`
- Whistle UI styling: `style`
- fallback proxies: `xproxy`, `xhttp-proxy`, `xhttps-proxy`, `xsocks`
- raw/template files: `rawfile`, `xrawfile`, `tpl`, `xtpl`

## HTTP/3 boundary

macOS release builds enable the optional QUIC feature and expose a localhost HTTP/3 listener that shares the existing rule pipeline. This listener is not a MASQUE proxy: PostGate cannot decrypt or rewrite HTTP/3 `CONNECT`, `CONNECT-UDP`, or arbitrary end-to-end QUIC datagrams, so those requests return `501 Not Implemented`.
