#!/usr/bin/env bash
#
# Local packaging helper.
#
# Usage:
#   scripts/package.sh mac              # current arch .dmg (arm64 on Apple Silicon)
#   scripts/package.sh mac-universal    # universal .dmg (arm64 + x86_64)
#   scripts/package.sh mac-intel        # x86_64-only .dmg (rarely needed)
#   scripts/package.sh win              # .exe / .msi via cargo-xwin
#   scripts/package.sh all              # mac (current) + mac-universal + win
#
# Output:
#   apps/desktop/src-tauri/target/<triple>/release/bundle/{dmg,nsis,msi}/...
# At the end the final artifacts are copied to dist-bundles/ at repo root.

set -euo pipefail

CMD="${1:-}"
if [[ -z "$CMD" ]]; then
  echo "usage: $0 {mac|mac-universal|mac-intel|win|all}" >&2
  exit 1
fi

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DESKTOP="$ROOT/apps/desktop"
BUNDLE_OUT="$ROOT/dist-bundles"

BLUE="\033[0;34m"; GREEN="\033[0;32m"; YELLOW="\033[0;33m"; RED="\033[0;31m"; RESET="\033[0m"
log()  { printf "${BLUE}[pkg]${RESET} %s\n" "$*"; }
ok()   { printf "${GREEN}[ok]${RESET}  %s\n" "$*"; }
warn() { printf "${YELLOW}[warn]${RESET} %s\n" "$*"; }
fail() { printf "${RED}[fail]${RESET} %s\n" "$*" >&2; exit 1; }

cd "$ROOT"

mkdir -p "$BUNDLE_OUT"

# Shared frontend build once per invocation.
build_frontend() {
  log "pnpm install (frozen lockfile disabled — allow cache reuse)"
  pnpm install --prefer-offline
  log "building workspace packages (@postgate/shared, plugin-sdk, inject-client)"
  pnpm run build:packages
}

# Copy tauri's bundle output for the given triple into dist-bundles/.
collect_bundle() {
  local triple="$1"
  local src="$DESKTOP/src-tauri/target/${triple}/release/bundle"
  [[ -d "$src" ]] || { warn "no bundle dir for $triple at $src"; return 0; }

  # dmg / app for mac
  if [[ -d "$src/dmg" ]]; then
    cp -f "$src/dmg/"*.dmg "$BUNDLE_OUT/" 2>/dev/null && \
      ok "copied $(ls "$src/dmg/"*.dmg 2>/dev/null | wc -l | tr -d ' ') dmg(s) → dist-bundles/"
  fi
  # windows installers
  if [[ -d "$src/nsis" ]]; then
    cp -f "$src/nsis/"*.exe "$BUNDLE_OUT/" 2>/dev/null && \
      ok "copied nsis .exe → dist-bundles/"
  fi
  if [[ -d "$src/msi" ]]; then
    cp -f "$src/msi/"*.msi "$BUNDLE_OUT/" 2>/dev/null && \
      ok "copied msi → dist-bundles/"
  fi
}

pkg_mac() {
  build_frontend
  log "building tauri for aarch64-apple-darwin"
  pnpm --filter @postgate/desktop exec tauri build --target aarch64-apple-darwin
  collect_bundle aarch64-apple-darwin
}

pkg_mac_intel() {
  build_frontend
  log "building tauri for x86_64-apple-darwin"
  pnpm --filter @postgate/desktop exec tauri build --target x86_64-apple-darwin
  collect_bundle x86_64-apple-darwin
}

pkg_mac_universal() {
  build_frontend
  log "building tauri universal (arm64 + x86_64) .dmg"
  # Tauri understands `universal-apple-darwin` and will bundle both slices.
  pnpm --filter @postgate/desktop exec tauri build --target universal-apple-darwin
  collect_bundle universal-apple-darwin
}

pkg_win() {
  build_frontend
  command -v cargo-xwin >/dev/null 2>&1 || \
    fail "cargo-xwin not installed. Run: pnpm package:setup"
  command -v makensis >/dev/null 2>&1 || \
    warn "makensis not found — NSIS .exe won't be produced. Install via 'brew install makensis'."

  # Tell tauri to use cargo-xwin as the cargo runner so linker/sdk/crt are
  # resolved correctly without a native Windows toolchain. `--runner
  # cargo-xwin` pipes the cargo invocation through `cargo xwin`, which on
  # first run downloads MSVC headers + libs and caches them under
  # ~/.cache/cargo-xwin.
  log "building tauri for x86_64-pc-windows-msvc via cargo-xwin"
  pnpm --filter @postgate/desktop exec tauri build \
    --runner cargo-xwin \
    --target x86_64-pc-windows-msvc

  collect_bundle x86_64-pc-windows-msvc
}

case "$CMD" in
  mac)            pkg_mac ;;
  mac-intel)      pkg_mac_intel ;;
  mac-universal)  pkg_mac_universal ;;
  win)            pkg_win ;;
  all)
    pkg_mac
    pkg_mac_universal
    pkg_win
    ;;
  *) fail "unknown target: $CMD" ;;
esac

ok "done. artifacts in: $BUNDLE_OUT"
ls -lh "$BUNDLE_OUT" 2>/dev/null || true
