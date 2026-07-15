# PostGate Design Tokens

## Sources Of Truth

- `apps/desktop/src/index.css`: core light and dark CSS variables, status colors, and HTTP method colors
- `apps/desktop/tailwind.config.js`: semantic token mappings and radius scale
- `apps/desktop/src/components/layout/Sidebar.tsx`: navigation states and compact surface treatment
- `apps/desktop/src/components/capture/TimingWaterfall.tsx`: timing phase colors
- `apps/desktop/src/lib/editor/whistle-language.ts`: Whistle editor syntax colors
- `apps/desktop/src-tauri/icons/icon.png`: packaged application icon

When these sources change, update this reference in the same change.

## Core Palette

PostGate has no separate chromatic brand color. Neutral zinc is the structural palette; near-black and near-white are the primary action colors.

| Role | Light | Dark |
| --- | --- | --- |
| Background | `hsl(0 0% 100%)` / `#ffffff` | `hsl(240 10% 3.9%)` / `#09090b` |
| Foreground | `hsl(240 10% 3.9%)` / `#09090b` | `hsl(0 0% 98%)` / `#fafafa` |
| Card | `#ffffff` | `#09090b` |
| Primary | `hsl(240 5.9% 10%)` / `#18181b` | `#fafafa` |
| Primary foreground | `#fafafa` | `#18181b` |
| Secondary, muted, accent | `hsl(240 4.8% 95.9%)` / `#f4f4f5` | `hsl(240 3.7% 15.9%)` / `#27272a` |
| Muted foreground | `hsl(240 3.8% 46.1%)` / `#71717a` | `hsl(240 5% 64.9%)` / `#a1a1aa` |
| Border and input | `hsl(240 5.9% 90%)` / `#e4e4e7` | `hsl(240 3.7% 15.9%)` / `#27272a` |
| Sidebar background | `hsl(0 0% 98%)` / `#fafafa` | `hsl(240 5.9% 6%)` / approximately `#0f0f10` |
| Destructive surface | `hsl(0 84.2% 60.2%)` | `hsl(0 62.8% 30.6%)` |
| Focus ring | `hsl(240 5.9% 10%)` | `hsl(240 4.9% 83.9%)` |

Use `8px` (`0.5rem`) as the base radius. Derive medium and small radii by subtracting `2px` and `4px`.

## Semantic Colors

| Meaning | Tailwind | Hex |
| --- | --- | --- |
| Success, GET, 2xx | `emerald-500` | `#10b981` |
| Redirect, POST, 3xx | `blue-500` | `#3b82f6` |
| Warning, PUT, PATCH, 4xx | `amber-500` | `#f59e0b` |
| Error, DELETE, 5xx | `red-500` | `#ef4444` |
| Pending | `zinc-400` | `#a1a1aa` |
| OPTIONS, HEAD, neutral method | `zinc-500` | `#71717a` |
| Interactive text link | `blue-600` light / `blue-400` dark | `#2563eb` / `#60a5fa` |

Use status colors for text, dots, compact badges, and data visualization. Use the interactive link pair for inline links and secondary text navigation, not primary buttons. Do not use semantic colors for main navigation, section numbers, primary buttons, or large backgrounds.

## Timing Colors

| Phase | Tailwind | Hex |
| --- | --- | --- |
| Blocked | `zinc-400` | `#a1a1aa` |
| DNS | `cyan-500` | `#06b6d4` |
| Connect | `orange-500` | `#f97316` |
| TLS | `purple-500` | `#a855f7` |
| Send | `emerald-500` | `#10b981` |
| Wait / TTFB | `sky-500` | `#0ea5e9` |
| Receive | `blue-500` | `#3b82f6` |

## Whistle Editor

| Token | Light | Dark |
| --- | --- | --- |
| Comment | `#008000` | `#6a9955` |
| Source | `#0e7490` | `#4ec9b0` |
| Regex source | `#c41a16` | `#d16969` |
| Target | `#a31515` | `#ce9178` |
| Action | `#0000ff` | `#569cd6` |
| Modifier | `#af00db` | `#c586c0` |

## Typography And Material

- Sans: `ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif`
- Mono: `ui-monospace, "SFMono-Regular", "Cascadia Code", monospace`
- Letter spacing: `0`
- Use translucent glass with a subtle border, backdrop blur, and low-contrast shadow only for floating controls.
- Provide an opaque fallback for reduced-transparency preferences.
- Avoid nesting cards and do not place ordinary page sections inside floating containers.

## Icon

The official icon is the packaged `512x512` PNG with a black rounded-square background and white Gate mark. Reuse `../assets/postgate-icon.png`. Do not substitute `apps/desktop/public/postgate.svg`; that green radar graphic is legacy artwork and does not represent the packaged application.
