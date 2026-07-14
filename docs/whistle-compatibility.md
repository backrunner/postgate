# Whistle Rule Compatibility

PostGate's compatibility baseline is Whistle v2.10.6, commit
`5e9ac58c979d3713a59fdc3471df296cd0f66071` (2026-07-11).

Compatibility means both parsing and applying a rule through the HTTP/1.1,
HTTPS MITM, and HTTP/2 request paths. A protocol that is only parsed is not
classified as supported.

## Supported

- Matching: domain, URL/path prefix, exact, wildcard, regular expression,
  no-schema, port, negation, method, protocol, port, content type, header,
  host, client IP, include, exclude, and response status filters.
- Routing: `host`, `hosts`, direct `http`/`https` target mapping, `proxy`,
  `http-proxy`, `https-proxy`, `socks`, `socks4`, and `socks5`.
- Request rewriting: `urlParams`, `params`/`reqMerge`, `pathReplace`,
  `method`, `reqHeaders`, `forwardedFor`, `ua`, `auth`, `referer`,
  `reqCharset`, `reqCookies`, `reqCors`, `reqType`, `reqBody`, `reqPrepend`,
  `reqAppend`, `reqReplace`, `reqWrite`, and `reqWriteRaw`.
- Response rewriting: `statusCode`, `replaceStatus`, redirects, `resHeaders`,
  `responseFor`, `resCharset`, `resCookies`, `attachment`, `resCors`,
  `resType`, `resBody`, `resMerge`, `resPrepend`, `resAppend`, `resReplace`,
  HTML/CSS/JS body/prepend/append/replace actions, `cache`, `resWrite`, and
  `resWriteRaw`.
- Traffic controls: `reqDelay`, `resDelay`, `reqSpeed`, `resSpeed`, and
  request timeout.
- Values: fenced inline values, global values, local files, external rule
  includes, and remote HTTP(S) body resources.

## Partial Or PostGate-Specific

- `xhost` currently behaves as `host`; Whistle's failure fallback distinction
  is not implemented.
- `delete` removes request/response headers. Whistle's body-property, cookie,
  and trailer deletion forms are not implemented.
- `headerReplace` is handled as response header modification, not Whistle's
  complete regex-based request/response/trailer replacement model.
- `enable` and `disable` support capture/hide, abort, forced body writes, and
  large merge limits. Other Whistle flags are retained but may not affect the
  transport.
- `weinre`/`debug` uses PostGate's Chobitsu/CDP injection rather than Whistle's
  Weinre implementation.
- Plugins use the PostGate plugin SDK and are not binary-compatible with npm
  `whistle.*` plugins.

## Explicitly Unsupported

- PAC execution: `pac`.
- Dynamic rule scripts: `rulesFile`, `reqRules`, `reqScript`, `resRules`,
  `resScript`, and `frameScript`.
- Stream/plugin pipes: `pipe`.
- Response trailers: `trailers`.
- Per-rule TLS negotiation and certificate callbacks: `cipher`, `tlsOptions`,
  and `sniCallback`.
- Whistle UI-only rule styling: `style`.
- Fallback proxy variants: `xproxy`, `xhttp-proxy`, `xhttps-proxy`, and
  `xsocks`.
- Raw/template file families: `rawfile`, `xrawfile`, `tpl`, and `xtpl`.

These protocols are preserved as `Unsupported` actions and surfaced as parse
warnings. They are not silently discarded.

## QUIC And HTTP/3

With the Cargo `quic` feature enabled, PostGate exposes a localhost HTTP/3
ingress using Quinn and Hyperium h3. Requests retain their absolute target URI
and stream through a pooled loopback bridge into the existing proxy pipeline,
so supported rules and capture behavior are shared with HTTP/1.1 and HTTP/2.

This ingress is not a MASQUE proxy. HTTP/3 `CONNECT` and `CONNECT-UDP` are
rejected with `501 Not Implemented`; arbitrary end-to-end QUIC datagrams cannot
be decrypted or rewritten by the HTTP rule engine. Builds without the `quic`
feature return a startup error when QUIC is enabled instead of ignoring the
setting.
