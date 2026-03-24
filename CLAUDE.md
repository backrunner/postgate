# PostGate - Development Reference

## Project Overview

PostGate is a Tauri 2.0-based MITM proxy tool for local frontend development. It captures HTTP/HTTPS traffic, supports rule-based request/response modification, and provides frontend debugging capabilities.

**Status: All 7 phases complete.** Only remaining item: QUIC/HTTP/3 support (optional, feature-gated with quinn crate).

---

## Tech Stack

| Layer | Technology |
|-------|------------|
| Desktop Framework | Tauri 2.0 |
| Frontend | React 19 + TypeScript 5.7 + Vite 6 |
| UI Components | shadcn/ui (zinc theme) |
| Styling | Tailwind CSS 3.4 |
| State Management | Zustand 5 |
| Code Editor | Monaco Editor |
| Virtual List | @tanstack/react-virtual |
| Backend | Rust (tokio async runtime) |
| TLS | rustls 0.23 + tokio-rustls 0.26 |
| HTTP | hyper 1.5 |
| Database | SQLite via sqlx 0.8 |
| Certs | rcgen 0.13 |
| Concurrency | DashMap 6 |
| Monorepo | pnpm workspaces + Turborepo |

---

## Project Structure

```
postgate/
├── apps/desktop/
│   ├── src/                        # React frontend
│   │   ├── components/
│   │   │   ├── ui/                 # shadcn/ui components
│   │   │   ├── layout/            # AppLayout, Sidebar, Header
│   │   │   ├── capture/           # BodyPreview, TimingWaterfall
│   │   │   └── rules/             # RuleEditor, RuleGroupList, ParseStatus
│   │   ├── hooks/                  # useProxy, useRequestBody
│   │   ├── stores/                 # theme, proxy, capture, rules, plugins, replay, debug
│   │   ├── lib/                    # utils, export, editor/whistle-language
│   │   ├── pages/                  # Capture, Rules, Replay, Debug, Plugins, Settings
│   │   └── App.tsx
│   └── src-tauri/src/
│       ├── proxy/                  # server, handler, tls, tunnel, body, pool, http2, websocket, sse, throttle
│       ├── cert/                   # mod, ca, store
│       ├── rules/                  # mod, types, engine, parser, applicator
│       ├── plugin/                 # mod, types, manager, runtime, plugin_wrapper.mjs
│       ├── replay/                 # mod, types, executor
│       ├── debug/                  # mod, types, session, server, injector
│       ├── storage/                # mod, database
│       ├── commands/               # mod, proxy, cert, rules, plugin, replay, debug
│       ├── state.rs, error.rs, lib.rs, main.rs
│       └── Cargo.toml
├── packages/
│   ├── inject-client/              # Browser injection script (console capture + CDP bridge)
│   ├── plugin-sdk/                 # Plugin development SDK
│   └── shared/                     # Shared TypeScript types
├── examples/
│   └── postgate-plugin-mock-api/   # Sample plugin
└── package.json, turbo.json, pnpm-workspace.yaml
```

---

## Key Architecture

### Proxy Pipeline
```
Client → Proxy Server (tokio) → Rule Engine Match → Apply Request Rules
  → Forward to Upstream (with connection pooling)
  → Apply Response Rules → Return to Client
  → Emit CapturedRequest event to frontend
```

### Rule System (Whistle-compatible)
- Pattern types: Exact, Wildcard, Regex, PathPrefix, Domain, Url
- Actions: Host, File, Redirect, StatusCode, Headers, Body, HTML/JS/CSS injection, Delay, Speed, Debug, Plugin, CORS, Cookies, Auth, etc.
- Filters: method (m:), protocol (p:), port (port:), content-type (ct:)
- Storage: SQLite (rule_groups + rules tables)
- Plugin rules: `plugin://name?config`
- Reference: https://wproxy.org/whistle/

### Debug System
- Injects Chobitsu CDP script into HTML responses matching `debug://` rules
- WebSocket server for debug connections + CDP target discovery endpoints
- Captures: console, errors (onerror + unhandledrejection), network (fetch + XHR)

---

## Development Commands

```bash
pnpm install          # Install dependencies
pnpm dev              # Start Tauri dev mode
pnpm build            # Build for production
pnpm test             # Run all tests
```

## Commit Convention

Conventional Commits: `feat:`, `fix:`, `refactor:`, `docs:`, `test:`, `chore:`

---

## UI/UX Guidelines

- **Design**: Compact, flat, modern, dual theme (light/dark)
- **Color scheme**: Zinc theme (shadcn/ui)
- **Status colors**: 2xx=emerald, 3xx=blue, 4xx=amber, 5xx=red, pending=zinc-400
- **Layout**: Sidebar nav + main content with split panels

## Performance Guidelines

- **Rust**: Async everywhere (tokio), zero-copy (Bytes), DashMap, connection pooling, cert caching
- **Frontend**: Virtual lists (>100 items), debounced filters (300ms), memoization, lazy body loading
- **IPC**: Batch updates, delta updates, Tauri event streaming

## Security

- CA cert: Warn about security implications; store private key securely
- Plugin sandbox: Consider sandboxing execution
- Sensitive headers: Mask Authorization/Cookie by default
- Bind to localhost only by default

---

## Future Considerations

- QUIC/HTTP/3 support (quinn crate, feature-gated)
- Mobile companion app for remote debugging
- Cloud sync for rules and collections
- Team collaboration features
- API mocking server mode
- Performance profiling integration
