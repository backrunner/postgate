---
title: Matching and filters
description: Match by domain, URL, path, wildcard, regular expression, method, protocol, headers, and status.
navTitle: Matching and filters
order: 11
---

# Matching and filters

The first token in a rule selects the traffic to match. Use the narrowest pattern that still expresses your intent.

## Common patterns

```text
# all requests to a domain
api.example.com host://localhost:3000

# URL or path prefix
https://api.example.com/v1/ host://localhost:3000

# wildcard
*.example.com resHeaders://x-environment=local

# regular expression
/^https:\/\/api\.example\.com\/v[12]\//i reqHeaders://x-debug=1

# port
:8080 reqDelay://100
```

PostGate supports domain, full-URL, path-prefix, exact, wildcard, regular-expression, scheme-free, and port patterns. Where negation is supported, prefix the pattern with `!` to exclude matching traffic.

## Inline filters

Filters narrow a matching pattern without changing its action:

```text
api.example.com filter://m:POST reqHeaders://x-write=1
api.example.com filter://p:https resHeaders://strict-transport-security=
api.example.com filter://port:443 reqHeaders://x-tls=1
api.example.com filter://h:content-type=json resDelay://200
api.example.com filter:///\/v2\//i reqHeaders://x-api-version=2
```

Supported filters include method, protocol, port, content type, header, host, client IP, include/exclude patterns, and response status.

## Rule ordering

When several enabled rules match, PostGate collects the applicable actions in rule order. Avoid broad rules that unintentionally overlap more specific mocks or routes. Keep environment-wide headers and traffic controls in separate groups so they can be enabled or disabled independently.
