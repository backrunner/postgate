---
title: Files, mocks, and injection
description: Serve local fixtures and inject HTML, JavaScript, or CSS into responses.
navTitle: Files and injection
order: 14
---

# Files, mocks, and injection

Local file rules let a page load a development bundle or fixed test fixture while everything else continues to come from the remote server.

## Replace a response with a file

Use an absolute path:

```text
cdn.example.com/assets/app.js file:///Users/me/project/dist/app.js
api.example.com/v1/user file:///Users/me/fixtures/user.json
```

On Windows, use a valid file URL with the drive component. The PostGate process must be able to read the target.

`mock` provides another file-oriented mock action:

```text
api.example.com/v1/orders mock:///absolute/path/orders.json
```

## Replace typed content

Use a body action when the value is short enough to remain readable:

```text
api.example.com/health json://{"ok":true,"source":"postgate"}
example.com/banner htmlBody://<aside>Local environment</aside>
```

For reusable or multi-line payloads, save the content in **Values** and reference it rather than escaping it on one line.

## Inject into a document

```text
example.com htmlAppend://<div id="local-badge">LOCAL</div>
example.com jsAppend://console.info('PostGate active')
example.com cssAppend://#local-badge{position:fixed;top:8px;right:8px}
```

Use `htmlPrepend`, `jsPrepend`, and `cssPrepend` to insert before existing content. `htmlReplace`, `jsReplace`, and `cssReplace` perform targeted replacement. Typed `htmlBody`, `jsBody`, and `cssBody` actions replace the corresponding body completely.

Injection is applied only to compatible response content. Check the response `content-type` and Capture body preview if an injection does not appear.
