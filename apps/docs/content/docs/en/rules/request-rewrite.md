---
title: Rewrite requests
description: Change request URLs, methods, headers, cookies, CORS metadata, and bodies.
navTitle: Request rewrites
order: 12
---

# Rewrite requests

Request actions run before PostGate connects to the upstream target.

## Query, path, and method

```text
api.example.com urlParams://debug=true&locale=en
api.example.com/v1 pathReplace://v1=v2
api.example.com method://POST
```

`urlParams` edits query parameters. `pathReplace` replaces matching path text. `method` changes the outgoing method.

## Headers and identity

```text
api.example.com reqHeaders://x-environment=local
api.example.com ua://PostGate-Test
api.example.com referer://https://app.example.com/
api.example.com forwardedFor://127.0.0.1
api.example.com auth://user:password
```

`reqHeaders` accepts query-style pairs or a JSON map. Dedicated actions are also available for the user agent, referrer, forwarded IP, Basic authentication, request cookies, CORS metadata, content type, and character set.

## Bodies

```text
api.example.com/v1/user reqBody://{"name":"Ada"}
api.example.com/v1/user reqMerge://{"debug":true}
api.example.com reqPrepend://prefix-
api.example.com reqAppend://-suffix
api.example.com reqReplace://old=new
```

Use `reqBody` to replace the entire body, and use `reqMerge` or `params` to merge structured data. Prepend, append, and replace actions are useful for text payloads. `reqWrite` and `reqWriteRaw` save observed request bodies to a local path.

Body changes update relevant length metadata before forwarding. Use a matching `reqType` when changing the representation, for example `reqType://application/json`.
