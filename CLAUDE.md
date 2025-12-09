# PostGate - Development Reference

## Project Overview

PostGate is a Tauri 2.0-based MITM proxy tool designed for local frontend development. It captures HTTP/HTTPS traffic, supports rule-based request/response modification, and provides frontend debugging capabilities.

---

## Development Progress

> Last updated: 2025-12-09

### Overall Status: Phase 7 Complete (100%)

| Phase | Status | Progress |
|-------|--------|----------|
| Phase 1: Project Scaffolding | **COMPLETE** | 100% |
| Phase 2: Core Proxy Engine | **COMPLETE** | 95% |
| Phase 3: Rule Engine | **COMPLETE** | 100% |
| Phase 4: Request Viewer UI | **COMPLETE** | 100% |
| Phase 5: Plugin System | **COMPLETE** | 100% |
| Phase 6: Request Replay | **COMPLETE** | 100% |
| Phase 7: Frontend Debugging | **COMPLETE** | 100% |

### Phase 1 Completed Items

- [x] Monorepo setup with pnpm workspaces + Turborepo
- [x] Root configuration files (package.json, turbo.json, pnpm-workspace.yaml)
- [x] apps/desktop - Tauri 2.0 + React 19 + TypeScript + Vite
- [x] Tailwind CSS 3.4 + shadcn/ui zinc theme configured
- [x] Dark/light mode toggle with system preference support
- [x] Base UI layout (Sidebar, Header, routing)
- [x] 6 page scaffolds: Capture, Rules, Replay, Debug, Plugins, Settings
- [x] Zustand stores (theme, proxy, capture)
- [x] shadcn/ui components (Button, Input, Tabs, Badge, etc.)
- [x] packages/inject-client - Console capture + DevTools bridge
- [x] packages/plugin-sdk - Full plugin interface definitions
- [x] packages/shared - Shared TypeScript types

### Phase 2 Completed Items

- [x] Rust backend module structure (proxy/, cert/, rules/, storage/, commands/)
- [x] Certificate Authority generation (rcgen)
- [x] Host certificate generation with caching (DashMap)
- [x] System certificate installation helpers (macOS, Windows, Linux)
- [x] Basic HTTP/1.1 proxy server with tokio
- [x] HTTPS CONNECT tunnel handling
- [x] TLS termination and re-encryption
- [x] Tauri IPC commands (start_proxy, stop_proxy, get_proxy_status)
- [x] Request/Response body capture with size limits (10MB max)
- [x] Body storage with LRU eviction
- [x] HTTP/2 support module (h2 crate) - `src/proxy/http2.rs`
- [x] WebSocket proxy with frame capture - `src/proxy/websocket.rs`
- [x] SSE (Server-Sent Events) capture - `src/proxy/sse.rs`
- [x] Frontend useProxy hook for Tauri event listening
- [x] Real-time request events via Tauri emit
- [x] Connection pooling for upstream - `src/proxy/pool.rs`
- [x] Pool cleanup background task
- [x] Configurable pool settings (max connections, idle timeout)

### Phase 2 Remaining Items

- [ ] QUIC/HTTP/3 support (quinn crate, optional feature-gated)

### Phase 3 Completed Items

- [x] Rule types (Pattern, RuleAction, RuleGroup)
- [x] Whistle-compatible rule parser with full syntax support
- [x] Pattern matching (Exact, Wildcard, Regex, PathPrefix, Domain, Url)
- [x] RuleEngine with DashMap storage
- [x] Tauri commands (parse_rules, save_rule_group, toggle_rule_group)
- [x] SQLite database schema and storage module
- [x] Rule applicator module - `src/rules/applicator.rs`
- [x] Request rule application (header modification, body replacement, short-circuit responses)
- [x] Response rule application (header modification, body replacement, HTML/JS/CSS injection)
- [x] Delay support (request_ms, response_ms)
- [x] **Full whistle-compatible rule types** - RuleFilters, method/protocol/port filtering
- [x] **Extended RuleAction variants** - UrlParams, PathReplace, Method, UserAgent, Referer, Auth, Cookies, CORS, Proxy chaining
- [x] **Parser improvements** - JSON header syntax, regex patterns, filter operators (m:, p:, port:, ct:)
- [x] **Applicator enhancements** - URL parameter modifications, cookie formatting, upstream proxy support
- [x] **20 passing unit tests** for parser, types, and applicator
- [x] **Speed throttling** - `src/proxy/throttle.rs` with kbps-based bandwidth limiting
- [x] **Rule editor UI with Monaco** - Full whistle syntax highlighting, autocomplete
- [x] **Rule validation feedback** - Real-time parsing with error display
- [x] **Rules Zustand store** - `src/stores/rules.ts` for rule group management
- [x] **RuleGroupList component** - Sidebar with enable/disable toggles
- [x] **ParseStatus component** - Shows parsed rules and errors

### Phase 3 Remaining Items

None - Phase 3 Complete!

### Phase 4 Completed Items

- [x] Capture page with split view layout
- [x] Virtual list for requests (@tanstack/react-virtual)
- [x] RequestListItem with method/status coloring
- [x] RequestDetail panel with tabs (Overview, Request, Response, Timing)
- [x] Toolbar with pause/clear/filter
- [x] FilterBar with method/status/content-type filters
- [x] Zustand store for capture state
- [x] useProxy hook for real-time updates
- [x] useRequestBody hook for lazy body loading
- [x] Header component connected to Tauri commands
- [x] **BodyPreview component** - JSON/HTML/CSS/JS syntax highlighting, image preview, hex dump
- [x] **TimingWaterfall component** - Request timing visualization with breakdowns
- [x] **Export utilities** (`src/lib/export.ts`) - HAR export, cURL copy, fetch() code generation
- [x] **Copy as cURL** - Generate cURL commands from captured requests
- [x] **Copy as fetch()** - Generate fetch() code from captured requests
- [x] **Export to HAR** - Export requests in HTTP Archive format

### Phase 4 Remaining Items

None - Phase 4 Complete!

### Phase 5 Completed Items

- [x] **Plugin architecture design** - Types for plugin info, requests, responses, panels
- [x] **Rust plugin manager** - `src/plugin/manager.rs` with discovery, loading, lifecycle
- [x] **Plugin runtime** - `src/plugin/runtime.rs` Node.js subprocess-based plugin execution
- [x] **Plugin wrapper script** - `src/plugin/plugin_wrapper.mjs` JavaScript bootstrap
- [x] **Plugin types** - `src/plugin/types.rs` comprehensive message/data types
- [x] **Tauri IPC commands** - get_plugins, load_plugin, unload_plugin, toggle_plugin, etc.
- [x] **Frontend plugin store** - `src/stores/plugins.ts` Zustand store for plugin management
- [x] **Plugin UI page** - `src/pages/Plugins/index.tsx` with discovery, toggle, uninstall
- [x] **AlertDialog component** - `src/components/ui/alert-dialog.tsx` for confirmations
- [x] **Plugin rule syntax** - `plugin://name?config` in rules applicator
- [x] **Sample plugin** - `examples/postgate-plugin-mock-api` demonstration plugin

### Phase 5 Remaining Items

None - Phase 5 Complete!

### Phase 6 Completed Items

- [x] **Replay types** - `src-tauri/src/replay/types.rs` Collection, SavedRequest, RequestBody, ReplayResponse, RequestHistory
- [x] **Request executor** - `src-tauri/src/replay/executor.rs` HTTP client using hyper/hyper-rustls
- [x] **Database storage** - Extended `src-tauri/src/storage/database.rs` with collections, saved_requests, request_history tables
- [x] **Tauri IPC commands** - `src-tauri/src/commands/replay.rs` for CRUD operations on collections/requests
- [x] **Frontend store** - `src/stores/replay.ts` Zustand store with tree structure
- [x] **Replay page UI** - `src/pages/Replay/index.tsx` complete Postman-like interface
- [x] **CollectionTree sidebar** - Folder tree with expand/collapse, context menus
- [x] **RequestEditor** - URL bar with method selector, tabs for params/headers/body
- [x] **KeyValueEditor** - Reusable key-value pair editor with enable/disable toggles
- [x] **BodyEditor** - Multi-type body editor (none, raw, form-data, urlencoded)
- [x] **Response viewer** - Body/headers tabs with status/timing display

### Phase 6 Remaining Items

None - Phase 6 Complete!

### Phase 7 Completed Items

- [x] **Debug types** - `src-tauri/src/debug/types.rs` DebugSession, ConsoleLog, ConsoleArg, PageError, ClientMessage, ServerMessage
- [x] **Session manager** - `src-tauri/src/debug/session.rs` manages debug sessions, console logs, page errors
- [x] **WebSocket server** - `src-tauri/src/debug/server.rs` accepts connections + HTTP /json/list endpoint
- [x] **Script injector** - `src-tauri/src/debug/injector.rs` injects Chobitsu CDP debug script into HTML
- [x] **Tauri IPC commands** - `src-tauri/src/commands/debug.rs` start/stop server, get logs, clear data
- [x] **Frontend debug store** - `src/stores/debug.ts` Zustand store for debug state management
- [x] **Debug page UI** - `src/pages/Debug/index.tsx` Console viewer + DevTools panel with CDP URLs
- [x] **Chobitsu CDP integration** - Full Chrome DevTools Protocol support via Chobitsu library
- [x] **CDP target discovery** - HTTP /json/list and /json/version endpoints for DevTools compatibility
- [x] **DevTools connection panel** - UI to copy devtools:// URLs for connecting Chrome DevTools
- [x] **Debug page UI** - `src/pages/Debug/index.tsx` Console viewer with session sidebar, log filtering, search
- [x] **Proxy integration** - Handler injects debug script when `debug://` rule matches HTML responses
- [x] **Console capture** - Inline inject script captures console.log/warn/error/info/debug/trace
- [x] **Error capture** - Captures window.onerror and unhandledrejection events
- [x] **Network capture** - Captures fetch() and XMLHttpRequest from page

### Phase 7 Remaining Items

None - Phase 7 Complete!

### Files Created (95+ files)

```
postgate/
├── package.json
├── pnpm-workspace.yaml
├── turbo.json
├── .gitignore
├── CLAUDE.md
├── apps/desktop/
│   ├── package.json
│   ├── tsconfig.json
│   ├── vite.config.ts
│   ├── tailwind.config.js
│   ├── postcss.config.js
│   ├── eslint.config.js
│   ├── index.html
│   ├── public/postgate.svg
│   ├── dist/                           # Build output (created)
│   ├── src/
│   │   ├── main.tsx
│   │   ├── App.tsx
│   │   ├── index.css
│   │   ├── vite-env.d.ts
│   │   ├── lib/utils.ts
│   │   ├── stores/{theme,proxy,capture}.ts
│   │   ├── hooks/useProxy.ts           # NEW: Proxy event hook
│   │   ├── components/ui/{button,input,tabs,...}.tsx
│   │   ├── components/layout/{AppLayout,Sidebar,Header}.tsx
│   │   └── pages/{Capture,Rules,Replay,Debug,Plugins,Settings}/
│   └── src-tauri/
│       ├── Cargo.toml
│       ├── tauri.conf.json
│       ├── build.rs
│       ├── capabilities/default.json
│       ├── icons/{32x32,128x128,icon}.png  # NEW: App icons
│       └── src/
│           ├── main.rs
│           ├── lib.rs
│           ├── error.rs
│           ├── state.rs
│           ├── proxy/
│           │   ├── mod.rs
│           │   ├── server.rs              # UPDATED: Connection pool integration
│           │   ├── handler.rs             # UPDATED: Rule application, pool context
│           │   ├── tls.rs
│           │   ├── tunnel.rs
│           │   ├── body.rs                # NEW: Body capture/storage
│           │   ├── pool.rs                # NEW: Connection pooling
│           │   ├── http2.rs               # NEW: HTTP/2 support
│           │   ├── websocket.rs           # NEW: WebSocket proxy
│           │   └── sse.rs                 # NEW: SSE capture
│           ├── cert/{mod,ca,store}.rs
│           ├── rules/
│           │   ├── mod.rs
│           │   ├── types.rs
│           │   ├── engine.rs
│           │   ├── parser.rs
│           │   └── applicator.rs       # NEW: Rule application logic
│           ├── storage/{mod,database}.rs
│           └── commands/{mod,proxy,cert,rules}.rs
├── packages/inject-client/
│   ├── package.json
│   ├── tsconfig.json
│   ├── tsup.config.ts
│   └── src/{index,console/capture,transport/websocket,devtools/bridge,utils/*}.ts
├── packages/plugin-sdk/
│   ├── package.json
│   ├── tsconfig.json
│   ├── tsup.config.ts
│   └── src/index.ts
└── packages/shared/
    ├── package.json
    ├── tsconfig.json
    ├── tsup.config.ts
    └── src/index.ts
```

### New Files in This Session

| File | Description |
|------|-------------|
| `src/proxy/body.rs` | Request/response body capture with size limits |
| `src/proxy/pool.rs` | Connection pooling for upstream servers |
| `src/proxy/http2.rs` | HTTP/2 client/server handling |
| `src/proxy/websocket.rs` | WebSocket proxy with frame capture |
| `src/proxy/sse.rs` | SSE event stream parsing and capture |
| `src/proxy/throttle.rs` | Speed throttling for bandwidth limiting |
| `src/rules/applicator.rs` | Rule application to requests/responses |
| `src/hooks/useProxy.ts` | Frontend hook for Tauri events |
| `src/stores/rules.ts` | Zustand store for rule groups |
| `src/lib/editor/whistle-language.ts` | Monaco syntax highlighting for whistle |
| `src/components/rules/RuleEditor.tsx` | Monaco-based rule editor component |
| `src/components/rules/RuleGroupList.tsx` | Rule group sidebar with toggles |
| `src/components/rules/ParseStatus.tsx` | Parse result and error display |
| `src/components/capture/BodyPreview.tsx` | Body preview with syntax highlighting |
| `src/components/capture/TimingWaterfall.tsx` | Request timing visualization |
| `src/lib/export.ts` | HAR export and cURL/fetch generation |
| `src-tauri/icons/*` | Application icons for Tauri |
| `src/plugin/mod.rs` | Plugin module entry point |
| `src/plugin/types.rs` | Plugin types and data structures |
| `src/plugin/manager.rs` | Plugin discovery, loading, lifecycle management |
| `src/plugin/runtime.rs` | Node.js subprocess plugin runtime |
| `src/plugin/plugin_wrapper.mjs` | JavaScript plugin bootstrap wrapper |
| `src/commands/plugin.rs` | Tauri IPC commands for plugins |
| `src/stores/plugins.ts` | Zustand store for plugin management |
| `src/pages/Plugins/index.tsx` | Plugin management UI page |
| `src/components/ui/alert-dialog.tsx` | Alert dialog component |
| `examples/postgate-plugin-mock-api/` | Sample mock API plugin |
| `src/replay/mod.rs` | Replay module entry point |
| `src/replay/types.rs` | Replay types (Collection, SavedRequest, etc.) |
| `src/replay/executor.rs` | HTTP request executor using hyper |
| `src/commands/replay.rs` | Tauri IPC commands for replay |
| `src/stores/replay.ts` | Zustand store for replay state |
| `src/pages/Replay/index.tsx` | Complete Replay page UI |
| `src/debug/mod.rs` | Debug module entry point |
| `src/debug/types.rs` | Debug types (DebugSession, ConsoleLog, etc.) |
| `src/debug/session.rs` | Debug session manager |
| `src/debug/server.rs` | WebSocket server for debug connections |
| `src/debug/injector.rs` | Script injector for HTML responses |
| `src/commands/debug.rs` | Tauri IPC commands for debugging |
| `src/stores/debug.ts` | Zustand store for debug state |
| `src/pages/Debug/index.tsx` | Debug page with console viewer |

### Actual Dependencies (Latest Versions)

**Frontend:**
- React 19.0.0, React DOM 19.0.0
- @tauri-apps/api 2.2.0
- @tanstack/react-virtual 3.13.2
- @monaco-editor/react 4.7.0
- monaco-editor 0.55.1
- zustand 5.0.3
- react-router-dom 7.1.1
- Tailwind CSS 3.4.17
- Vite 6.0.7
- TypeScript 5.7.3

**Rust Backend:**
- tauri 2.9.x
- tokio 1.42.x
- hyper 1.5.x
- urlencoding 2.x
- rustls 0.23.x, tokio-rustls 0.26.x
- rcgen 0.13.x
- dashmap 6.1.x
- sqlx 0.8.x (SQLite)
- thiserror 2.x

### Next Steps

1. **Complete Phase 2: Core Proxy Engine**
   - Add connection pooling for better performance
   - Implement QUIC/HTTP/3 support (optional, feature-gated)
   - Performance testing and optimization

2. **Complete Phase 3: Rule Engine**
   - Implement speed throttling
   - Create rule editor UI with Monaco editor
   - Add rule validation and error feedback

3. **Complete Phase 4: Request Viewer UI**
   - Add JSON syntax highlighting for body preview
   - Implement timing waterfall visualization
   - Add HAR export functionality
   - Add "Copy as cURL" feature

4. **Start Phase 5: Plugin System**
   - Design plugin sandbox architecture
   - Implement plugin discovery and loading
   - Create sample plugins

---

## Tech Stack

| Layer | Technology |
|-------|------------|
| Desktop Framework | Tauri 2.0 |
| Frontend | React 18+ with TypeScript |
| UI Components | shadcn/ui (zinc theme) |
| Styling | Tailwind CSS |
| Backend | Rust (tokio async runtime) |
| Monorepo | Turborepo |
| Package Manager | pnpm |

---

## Monorepo Structure

```
postgate/
├── apps/
│   └── desktop/                    # Main Tauri desktop app
│       ├── src/                    # React frontend
│       │   ├── components/         # UI components
│       │   ├── hooks/              # React hooks
│       │   ├── stores/             # State management (zustand)
│       │   ├── lib/                # Utilities
│       │   ├── pages/              # Page components
│       │   └── App.tsx
│       ├── src-tauri/              # Rust backend
│       │   ├── src/
│       │   │   ├── proxy/          # Proxy server module
│       │   │   ├── cert/           # Certificate management
│       │   │   ├── rules/          # Rule engine
│       │   │   ├── plugin/         # Plugin system
│       │   │   ├── storage/        # Data persistence
│       │   │   └── main.rs
│       │   ├── Cargo.toml
│       │   └── tauri.conf.json
│       ├── index.html
│       └── package.json
├── packages/
│   ├── inject-client/              # Browser injection script
│   │   ├── src/
│   │   │   ├── console-capture.ts  # Console hijacking
│   │   │   ├── devtools-bridge.ts  # CDP bridge
│   │   │   └── index.ts
│   │   └── package.json
│   ├── plugin-sdk/                 # Plugin development SDK
│   │   ├── src/
│   │   └── package.json
│   └── shared/                     # Shared types/utilities
│       ├── src/
│       └── package.json
├── turbo.json
├── pnpm-workspace.yaml
├── package.json
└── CLAUDE.md
```

---

## Phase 1: Project Scaffolding

### Tasks

1. **Initialize monorepo**
   - Create `pnpm-workspace.yaml`
   - Create root `package.json` with workspace scripts
   - Create `turbo.json` with pipeline configuration

2. **Set up Tauri 2.0 desktop app**
   ```bash
   pnpm create tauri-app apps/desktop --template react-ts
   ```
   - Configure Tauri 2.0 permissions for network access
   - Set up IPC commands structure

3. **Configure shadcn/ui**
   - Install and configure with zinc theme
   - Set up dark/light mode toggle
   - Create base layout components

4. **Set up inject-client package**
   - TypeScript bundler (tsup or esbuild)
   - Output as self-contained IIFE script

### Key Dependencies

**Frontend (apps/desktop/package.json):**
```json
{
  "dependencies": {
    "react": "^18.3.0",
    "react-dom": "^18.3.0",
    "@tauri-apps/api": "^2.0.0",
    "@tanstack/react-virtual": "^3.0.0",
    "zustand": "^4.5.0",
    "class-variance-authority": "^0.7.0",
    "clsx": "^2.1.0",
    "tailwind-merge": "^2.2.0",
    "lucide-react": "^0.400.0"
  }
}
```

**Backend (apps/desktop/src-tauri/Cargo.toml):**
```toml
[dependencies]
tauri = { version = "2", features = ["tray-icon"] }
tokio = { version = "1", features = ["full"] }
hyper = { version = "1", features = ["full"] }
hyper-util = "0.1"
http-body-util = "0.1"
rustls = "0.23"
tokio-rustls = "0.26"
rcgen = "0.13"                    # Certificate generation
x509-parser = "0.16"
quinn = "0.11"                    # QUIC support
h2 = "0.4"                        # HTTP/2
bytes = "1"
dashmap = "6"                     # Concurrent hashmap
parking_lot = "0.12"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "1"
tracing = "0.1"
tracing-subscriber = "0.3"
uuid = { version = "1", features = ["v4"] }
regex = "1"
glob = "0.3"
sqlx = { version = "0.8", features = ["runtime-tokio", "sqlite"] }
```

---

## Phase 2: Core Proxy Engine

### Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                     Tauri Main Process                       │
├─────────────────────────────────────────────────────────────┤
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐  │
│  │ Proxy       │  │ Rule        │  │ Request Storage     │  │
│  │ Manager     │  │ Engine      │  │ (SQLite)            │  │
│  └──────┬──────┘  └──────┬──────┘  └──────────┬──────────┘  │
│         │                │                     │             │
│  ┌──────┴────────────────┴─────────────────────┴──────────┐ │
│  │              Event Channel (tokio broadcast)           │ │
│  └────────────────────────────┬───────────────────────────┘ │
│                               │                             │
│  ┌────────────────────────────┴───────────────────────────┐ │
│  │                   Proxy Server (tokio)                  │ │
│  │  ┌─────────┐  ┌─────────┐  ┌─────────┐  ┌───────────┐  │ │
│  │  │ HTTP/1.1│  │ HTTP/2  │  │ QUIC/H3 │  │ WebSocket │  │ │
│  │  │ Handler │  │ Handler │  │ Handler │  │ Handler   │  │ │
│  │  └─────────┘  └─────────┘  └─────────┘  └───────────┘  │ │
│  └────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────┘
```

### Tasks

1. **Certificate Management (`src-tauri/src/cert/`)**
   - `ca.rs` - Generate self-signed CA certificate
   - `store.rs` - Certificate storage and retrieval
   - `install.rs` - System certificate installation helpers
   - Cache generated host certificates in memory (LRU cache)

   ```rust
   // Key structures
   pub struct CertificateAuthority {
       ca_cert: Certificate,
       ca_key: PrivateKey,
       cert_cache: Arc<DashMap<String, Arc<CertifiedKey>>>,
   }
   
   impl CertificateAuthority {
       pub fn generate_ca() -> Result<Self>;
       pub fn get_cert_for_host(&self, host: &str) -> Result<Arc<CertifiedKey>>;
       pub fn install_to_system(&self) -> Result<()>;
   }
   ```

2. **Proxy Server (`src-tauri/src/proxy/`)**
   - `server.rs` - Main proxy server (single instance enforcement)
   - `handler.rs` - Request/response handling pipeline
   - `tls.rs` - TLS termination and re-encryption
   - `h2.rs` - HTTP/2 specific handling
   - `quic.rs` - QUIC/HTTP/3 support
   - `websocket.rs` - WebSocket proxy with frame capture
   - `sse.rs` - Server-Sent Events capture

   ```rust
   // Core proxy state
   pub struct ProxyServer {
       listener: TcpListener,
       ca: Arc<CertificateAuthority>,
       rule_engine: Arc<RuleEngine>,
       request_tx: broadcast::Sender<CapturedRequest>,
       running: Arc<AtomicBool>,
   }
   
   pub struct ProxyConfig {
       pub port: u16,
       pub enable_h2: bool,
       pub enable_quic: bool,
       pub quic_port: Option<u16>,
   }
   ```

3. **Request Capture Model**
   ```rust
   #[derive(Debug, Clone, Serialize)]
   pub struct CapturedRequest {
       pub id: Uuid,
       pub timestamp: DateTime<Utc>,
       pub method: String,
       pub url: String,
       pub host: String,
       pub path: String,
       pub request_headers: HashMap<String, String>,
       pub request_body: Option<Vec<u8>>,
       pub response_status: Option<u16>,
       pub response_headers: Option<HashMap<String, String>>,
       pub response_body: Option<Vec<u8>>,
       pub duration_ms: Option<u64>,
       pub matched_rules: Vec<String>,
       pub protocol: Protocol,  // HTTP1, HTTP2, QUIC, WebSocket, SSE
       pub tls_info: Option<TlsInfo>,
   }
   ```

4. **Tauri IPC Commands**
   ```rust
   #[tauri::command]
   async fn start_proxy(config: ProxyConfig) -> Result<(), String>;
   
   #[tauri::command]
   async fn stop_proxy() -> Result<(), String>;
   
   #[tauri::command]
   async fn get_proxy_status() -> ProxyStatus;
   
   #[tauri::command]
   async fn install_ca_certificate() -> Result<(), String>;
   
   #[tauri::command]
   async fn get_ca_certificate() -> Result<Vec<u8>, String>;
   ```

---

## Phase 3: Rule Engine

### Whistle Rule Compatibility

PostGate implements whistle-compatible rules. Reference: https://wproxy.org/whistle/

### Supported Rule Types

| Rule Type | Syntax | Description |
|-----------|--------|-------------|
| Host | `pattern host://ip:port` | Redirect to different host |
| File | `pattern file:///path` | Serve local file |
| Redirect | `pattern redirect://url` | HTTP redirect |
| Status | `pattern statusCode://code` | Return status code |
| Headers | `pattern reqHeaders://json` | Modify request headers |
| ResHeaders | `pattern resHeaders://json` | Modify response headers |
| ReqBody | `pattern reqBody://content` | Replace request body |
| ResBody | `pattern resBody://content` | Replace response body |
| Replace | `pattern htmlAppend://content` | Append to HTML |
| Delay | `pattern reqDelay://ms` | Delay request |
| Speed | `pattern reqSpeed://kb` | Throttle speed |
| Debug | `pattern debug://name` | Enable debugging |

### Pattern Matching

```rust
pub enum Pattern {
    Exact(String),                    // example.com/path
    Wildcard(String),                 // *.example.com
    Regex(Regex),                     // /pattern/flags
    PathPrefix(String),               // example.com/api/
}

pub struct Rule {
    pub id: Uuid,
    pub enabled: bool,
    pub pattern: Pattern,
    pub actions: Vec<RuleAction>,
    pub priority: i32,
}

pub enum RuleAction {
    Host { target: String },
    File { path: PathBuf },
    Redirect { url: String, status: u16 },
    StatusCode { code: u16 },
    RequestHeaders { modifications: HeaderModifications },
    ResponseHeaders { modifications: HeaderModifications },
    RequestBody { content: BodyContent },
    ResponseBody { content: BodyContent },
    HtmlAppend { content: String },
    HtmlPrepend { content: String },
    JsAppend { content: String },
    JsPrepend { content: String },
    CssAppend { content: String },
    CssPrepend { content: String },
    Delay { request_ms: Option<u64>, response_ms: Option<u64> },
    Speed { kbps: u64 },
    Debug { name: String },
    Plugin { name: String, config: Value },
}
```

### Rule Storage

Rules stored in SQLite with the following schema:

```sql
CREATE TABLE rule_groups (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    enabled INTEGER DEFAULT 1,
    priority INTEGER DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE rules (
    id TEXT PRIMARY KEY,
    group_id TEXT REFERENCES rule_groups(id),
    raw_rule TEXT NOT NULL,  -- Original whistle syntax
    pattern_type TEXT NOT NULL,
    pattern_value TEXT NOT NULL,
    actions TEXT NOT NULL,  -- JSON array of actions
    enabled INTEGER DEFAULT 1,
    priority INTEGER DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
```

### Tauri IPC Commands

```rust
#[tauri::command]
async fn parse_rules(content: String) -> Result<Vec<Rule>, String>;

#[tauri::command]
async fn save_rule_group(group: RuleGroup) -> Result<(), String>;

#[tauri::command]
async fn get_rule_groups() -> Result<Vec<RuleGroup>, String>;

#[tauri::command]
async fn toggle_rule(id: String, enabled: bool) -> Result<(), String>;

#[tauri::command]
async fn toggle_rule_group(id: String, enabled: bool) -> Result<(), String>;
```

---

## Phase 4: Request Viewer UI

### Components Structure

```
src/pages/
└── Capture/
    ├── index.tsx                    # Main capture page
    ├── RequestList.tsx              # Virtual list of requests
    ├── RequestListItem.tsx          # Single request row
    ├── RequestDetail.tsx            # Detail panel container
    ├── RequestDetailTabs.tsx        # Headers/Body/Timing tabs
    ├── Toolbar.tsx                  # Pause/Resume/Clear/Filter
    └── FilterBar.tsx                # Search and filter controls
```

### Virtual List Implementation

Use `@tanstack/react-virtual` for performance:

```tsx
// RequestList.tsx
import { useVirtualizer } from '@tanstack/react-virtual';

export function RequestList({ requests }: { requests: CapturedRequest[] }) {
  const parentRef = useRef<HTMLDivElement>(null);
  
  const virtualizer = useVirtualizer({
    count: requests.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 36,  // Row height
    overscan: 20,
  });
  
  return (
    <div ref={parentRef} className="h-full overflow-auto">
      <div style={{ height: virtualizer.getTotalSize() }}>
        {virtualizer.getVirtualItems().map((virtualRow) => (
          <RequestListItem
            key={requests[virtualRow.index].id}
            request={requests[virtualRow.index]}
            style={{
              position: 'absolute',
              top: virtualRow.start,
              height: virtualRow.size,
            }}
          />
        ))}
      </div>
    </div>
  );
}
```

### State Management (Zustand)

```typescript
// stores/capture.ts
interface CaptureState {
  requests: CapturedRequest[];
  selectedId: string | null;
  isPaused: boolean;
  filter: FilterOptions;
  
  // Actions
  addRequest: (request: CapturedRequest) => void;
  updateRequest: (id: string, update: Partial<CapturedRequest>) => void;
  setSelected: (id: string | null) => void;
  togglePause: () => void;
  clearRequests: () => void;
  setFilter: (filter: FilterOptions) => void;
}

interface FilterOptions {
  search: string;
  methods: string[];
  statusCodes: string[];  // "2xx", "3xx", "4xx", "5xx"
  contentTypes: string[];
  hosts: string[];
  hasRules: boolean | null;
}
```

### Request Detail Tabs

1. **Overview Tab**
   - URL, Method, Status
   - Timing breakdown (DNS, Connect, TLS, Request, Response)
   - Matched rules list

2. **Request Tab**
   - Headers (collapsible, searchable)
   - Query parameters
   - Body (with format detection: JSON, XML, Form, Binary)

3. **Response Tab**
   - Headers
   - Body (with syntax highlighting)
   - Preview (for images, HTML)

4. **Timing Tab**
   - Waterfall visualization
   - Detailed timing breakdown

---

## Phase 5: Plugin System

### Plugin Specification

Plugins are npm packages with the naming convention `postgate-plugin-*`.

```typescript
// packages/plugin-sdk/src/types.ts
export interface PostGatePlugin {
  name: string;
  version: string;
  
  // Called when plugin is loaded
  onLoad?(context: PluginContext): Promise<void>;
  
  // Called when plugin is unloaded
  onUnload?(): Promise<void>;
  
  // Handle requests matching plugin rule
  handleRequest?(
    request: PluginRequest,
    context: RequestContext
  ): Promise<PluginResponse | null>;
  
  // Handle responses (for modification)
  handleResponse?(
    request: PluginRequest,
    response: PluginResponse,
    context: RequestContext
  ): Promise<PluginResponse>;
}

export interface PluginContext {
  storage: PluginStorage;       // Persistent key-value storage
  logger: PluginLogger;         // Logging interface
  ui: PluginUI;                 // Register UI panels
}

export interface PluginRequest {
  id: string;
  method: string;
  url: string;
  headers: Record<string, string>;
  body: Uint8Array | null;
}

export interface PluginResponse {
  status: number;
  headers: Record<string, string>;
  body: Uint8Array | null;
}
```

### Plugin Discovery & Loading

```rust
// src-tauri/src/plugin/manager.rs
pub struct PluginManager {
    plugins: HashMap<String, LoadedPlugin>,
    node_runtime: NodeRuntime,  // Embedded Node.js or IPC to external process
}

impl PluginManager {
    pub async fn discover_plugins() -> Vec<PluginInfo>;
    pub async fn load_plugin(&mut self, name: &str) -> Result<()>;
    pub async fn unload_plugin(&mut self, name: &str) -> Result<()>;
    pub async fn handle_request(
        &self, 
        plugin_name: &str, 
        request: PluginRequest
    ) -> Result<Option<PluginResponse>>;
}
```

### Plugin Rule Syntax

```
# Route requests to a plugin
example.com/api plugin://my-plugin
example.com/api plugin://my-plugin?config=value
```

---

## Phase 6: Request Replay

### Data Model

```typescript
// Saved request for replay
interface SavedRequest {
  id: string;
  name: string;
  collectionId: string | null;
  
  method: string;
  url: string;
  headers: Array<{ key: string; value: string; enabled: boolean }>;
  queryParams: Array<{ key: string; value: string; enabled: boolean }>;
  body: RequestBody;
  
  createdAt: string;
  updatedAt: string;
}

interface RequestBody {
  type: 'none' | 'raw' | 'form-data' | 'x-www-form-urlencoded' | 'binary';
  raw?: { content: string; contentType: string };
  formData?: Array<{ key: string; value: string; type: 'text' | 'file' }>;
  urlencoded?: Array<{ key: string; value: string }>;
  binary?: { path: string };
}

interface Collection {
  id: string;
  name: string;
  parentId: string | null;
  requests: string[];  // Request IDs
  subCollections: string[];
  createdAt: string;
  updatedAt: string;
}
```

### UI Components

```
src/pages/
└── Replay/
    ├── index.tsx                    # Main replay page
    ├── Sidebar/
    │   ├── CollectionTree.tsx       # Collection/folder tree
    │   └── RequestItem.tsx          # Request item in tree
    ├── Editor/
    │   ├── RequestEditor.tsx        # Main editor
    │   ├── UrlBar.tsx               # Method + URL input
    │   ├── HeadersEditor.tsx        # Headers key-value editor
    │   ├── QueryParamsEditor.tsx    # Query params editor
    │   ├── BodyEditor.tsx           # Body editor with type tabs
    │   └── ResponseViewer.tsx       # Response display
    └── History/
        └── HistoryPanel.tsx         # Recent requests
```

### Storage Schema

```sql
CREATE TABLE collections (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    parent_id TEXT REFERENCES collections(id),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE saved_requests (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    collection_id TEXT REFERENCES collections(id),
    method TEXT NOT NULL,
    url TEXT NOT NULL,
    headers TEXT NOT NULL,  -- JSON
    query_params TEXT NOT NULL,  -- JSON
    body_type TEXT NOT NULL,
    body_content TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE request_history (
    id TEXT PRIMARY KEY,
    saved_request_id TEXT REFERENCES saved_requests(id),
    request_snapshot TEXT NOT NULL,  -- JSON snapshot
    response_status INTEGER,
    response_headers TEXT,
    response_body BLOB,
    duration_ms INTEGER,
    executed_at TEXT NOT NULL
);
```

---

## Phase 7: Frontend Debugging

### Debug Modes

1. **Global Console Capture**
   - Inject script into all HTML responses
   - Capture console.log, warn, error, etc.
   - Send logs back to PostGate via WebSocket

2. **Targeted Debug (`debug://name`)**
   - Only inject into requests matching the rule
   - Full DevTools Protocol support via Chobitsu
   - Embedded DevTools frontend

### Inject Client Architecture

```
packages/inject-client/
├── src/
│   ├── index.ts                     # Entry point
│   ├── console/
│   │   ├── capture.ts               # Console method overrides
│   │   └── formatter.ts             # Log formatting
│   ├── devtools/
│   │   ├── bridge.ts                # CDP bridge
│   │   ├── chobitsu-adapter.ts      # Chobitsu integration
│   │   └── protocol-handler.ts      # CDP message handling
│   ├── transport/
│   │   ├── websocket.ts             # WS connection to PostGate
│   │   └── message-queue.ts         # Offline message buffering
│   └── utils/
│       ├── serializer.ts            # Safe object serialization
│       └── stack-trace.ts           # Stack trace parsing
└── package.json
```

### Console Capture Implementation

```typescript
// packages/inject-client/src/console/capture.ts
const originalConsole = { ...console };

const METHODS = ['log', 'warn', 'error', 'info', 'debug', 'trace'] as const;

export function initConsoleCapture(transport: Transport) {
  METHODS.forEach(method => {
    console[method] = (...args: unknown[]) => {
      // Call original
      originalConsole[method](...args);
      
      // Send to PostGate
      transport.send({
        type: 'console',
        method,
        args: args.map(serialize),
        timestamp: Date.now(),
        stack: getStackTrace(),
      });
    };
  });
}
```

### DevTools Integration

PostGate embeds a DevTools frontend and connects to the target page via Chobitsu.

```
┌─────────────────┐     WebSocket      ┌─────────────────┐
│  PostGate App   │◄──────────────────►│  Target Page    │
│  ┌───────────┐  │                    │  ┌───────────┐  │
│  │ DevTools  │  │    CDP Messages    │  │ Chobitsu  │  │
│  │ Frontend  │◄─┼────────────────────┼──│ + Inject  │  │
│  └───────────┘  │                    │  └───────────┘  │
└─────────────────┘                    └─────────────────┘
```

### Tauri Commands for Debugging

```rust
#[tauri::command]
async fn get_debug_sessions() -> Vec<DebugSession>;

#[tauri::command]
async fn get_console_logs(session_id: String) -> Vec<ConsoleLog>;

#[tauri::command]
async fn clear_console_logs(session_id: String) -> Result<(), String>;

#[tauri::command]
async fn send_cdp_message(session_id: String, message: String) -> Result<String, String>;
```

---

## UI/UX Guidelines

### Design Principles

1. **Compact Layout** - Maximize information density without overwhelming
2. **Flat Design** - Minimal shadows, clear hierarchy through spacing/color
3. **Modern Aesthetics** - Clean lines, consistent spacing, subtle animations
4. **Dual Theme Support** - All components must work in light/dark mode

### Color Scheme (Zinc Theme)

```css
/* Light mode */
--background: 0 0% 100%;
--foreground: 240 10% 3.9%;
--muted: 240 4.8% 95.9%;
--muted-foreground: 240 3.8% 46.1%;
--border: 240 5.9% 90%;
--accent: 240 4.8% 95.9%;

/* Dark mode */
--background: 240 10% 3.9%;
--foreground: 0 0% 98%;
--muted: 240 3.7% 15.9%;
--muted-foreground: 240 5% 64.9%;
--border: 240 3.7% 15.9%;
--accent: 240 3.7% 15.9%;
```

### Status Color Coding

| Status | Color | Usage |
|--------|-------|-------|
| Success (2xx) | `text-emerald-500` | Successful responses |
| Redirect (3xx) | `text-blue-500` | Redirects |
| Client Error (4xx) | `text-amber-500` | Client errors |
| Server Error (5xx) | `text-red-500` | Server errors |
| Pending | `text-zinc-400` | In-progress requests |

### Layout Structure

```
┌─────────────────────────────────────────────────────────────┐
│ Sidebar │                   Main Content                    │
│  (Nav)  │                                                   │
│         │  ┌─────────────────────────────────────────────┐  │
│ [Capture│  │  Toolbar                                    │  │
│  Rules  │  ├─────────────────────────────────────────────┤  │
│  Replay │  │                                             │  │
│  Debug  │  │  Request List          │  Request Detail    │  │
│  Plugin │  │  (Virtual Scroll)      │  (Tabs)            │  │
│  Setting│  │                        │                    │  │
│         │  │                        │                    │  │
│         │  └─────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

---

## Performance Guidelines

### Rust Backend

1. **Async Everywhere** - Use tokio for all I/O operations
2. **Zero-Copy When Possible** - Use `Bytes` and avoid unnecessary cloning
3. **Connection Pooling** - Reuse upstream connections
4. **Concurrent Hash Maps** - Use `DashMap` for shared state
5. **Efficient Certificate Caching** - LRU cache for generated certificates

### Frontend

1. **Virtual Lists** - Always virtualize lists > 100 items
2. **Debounced Filters** - Debounce search/filter inputs (300ms)
3. **Memoization** - Use `useMemo`/`useCallback` for expensive computations
4. **Lazy Loading** - Lazy load response bodies on demand
5. **Web Workers** - Offload heavy parsing (large JSON) to workers

### IPC Optimization

1. **Batch Updates** - Batch multiple request updates into single IPC call
2. **Delta Updates** - Send only changed fields for request updates
3. **Streaming** - Use Tauri events for real-time request streaming

---

## Testing Strategy

### Unit Tests

- **Rust**: `cargo test` for proxy, rules, cert modules
- **TypeScript**: Vitest for utility functions

### Integration Tests

- Proxy server tests with mock upstream servers
- Rule engine tests with fixture files
- IPC command tests

### E2E Tests

- Playwright for UI testing
- Test capture flow, rule editing, replay functionality

---

## Development Workflow

### Initial Setup

```bash
# Clone and install dependencies
git clone <repo>
cd postgate
pnpm install

# Development
pnpm dev          # Start Tauri dev mode

# Build
pnpm build        # Build for production

# Test
pnpm test         # Run all tests
pnpm test:unit    # Unit tests only
pnpm test:e2e     # E2E tests only
```

### Commit Convention

Follow Conventional Commits:
- `feat:` New feature
- `fix:` Bug fix
- `refactor:` Code refactoring
- `docs:` Documentation
- `test:` Tests
- `chore:` Maintenance

---

## Security Considerations

1. **CA Certificate** - Warn users about security implications of installing CA
2. **Certificate Storage** - Store CA private key securely (OS keychain if possible)
3. **Plugin Sandbox** - Consider sandboxing plugin execution
4. **Request Data** - Sensitive headers (Authorization, Cookie) should be masked by default
5. **Local Only** - Proxy should only bind to localhost by default

---

## Future Considerations

- Mobile companion app for remote debugging
- Cloud sync for rules and collections
- Team collaboration features
- API mocking server mode
- Performance profiling integration
- HAR export/import
