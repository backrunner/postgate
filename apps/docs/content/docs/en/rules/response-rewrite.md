---
title: Rewrite responses
description: Change status, redirects, headers, cookies, CORS, caching, and response bodies.
navTitle: Response rewrites
order: 13
---

# Rewrite responses

Response actions run after the upstream server answers and before the response is sent to the client.

## Status and redirects

```text
api.example.com/maintenance statusCode://503
example.com/old redirect://https://example.com/new
example.com/temporary 307://https://example.com/new
```

PostGate supports status replacement and `301`, `302`, `307`, and `308` redirects.

## Headers, cookies, CORS, and cache

```text
api.example.com resHeaders://x-served-by=postgate
api.example.com resType://application/json
api.example.com resCharset://utf-8
api.example.com resCors://*
downloads.example.com/file attachment://report.pdf
assets.example.com cache://max-age=60
```

`resHeaders` accepts query-style modifications or a JSON map. Dedicated actions manage response cookies, CORS, content type, charset, attachment filenames, and caching.

## Bodies

```text
api.example.com/v1/user resBody://{"ok":true}
api.example.com/v1/user resMerge://{"source":"postgate"}
api.example.com resPrepend://before-
api.example.com resAppend://-after
api.example.com resReplace://production=local
```

`resBody` replaces the entire body. `resMerge` merges structured content. Prepend, append, and replace operate on response content, while `resWrite` and `resWriteRaw` save the observed body to disk.

Changing a compressed response may require PostGate to decode and re-encode the body. Verify the resulting content type and body in Capture after adding a rewrite.
