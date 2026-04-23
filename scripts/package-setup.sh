#!/usr/bin/env bash
#
# One-shot bootstrap for local cross-platform packaging.
# Installs:
#   * Rust targets:
#       - aarch64-apple-darwin   (current mac)
#       - x86_64-apple-darwin    (for universal mac dmg)
#       - x86_64-pc-windows-msvc (for Windows cross build via cargo-xwin)
#   * cargo-xwin  — cross-compiles to MSVC targets from macOS/Linux
#   * cargo-wix   — optional, lets tauri bundle into .msi
#   * Homebrew:   llvm (lld), nsis (for Windows NSIS installer)
#
# Safe to re-run. Each step is guarded.

set -euo pipefail

BLUE="\033[0;34m"; GREEN="\033[0;32m"; YELLOW="\033[0;33m"; RESET="\033[0m"
log()  { printf "${BLUE}[setup]${RESET} %s\n" "$*"; }
ok()   { printf "${GREEN}[ ok ]${RESET} %s\n" "$*"; }
warn() { printf "${YELLOW}[warn]${RESET} %s\n" "$*"; }

OS="$(uname -s)"

# --- Rust toolchain & targets --------------------------------------------
if ! command -v rustup >/dev/null 2>&1; then
  echo "rustup not found. Install from https://rustup.rs first." >&2
  exit 1
fi

add_target() {
  local t="$1"
  if rustup target list --installed | grep -q "^${t}$"; then
    ok "rust target already installed: ${t}"
  else
    log "installing rust target: ${t}"
    rustup target add "$t"
  fi
}

add_target aarch64-apple-darwin
add_target x86_64-apple-darwin
add_target x86_64-pc-windows-msvc

# --- cargo-xwin ----------------------------------------------------------
if command -v cargo-xwin >/dev/null 2>&1; then
  ok "cargo-xwin already installed"
else
  log "installing cargo-xwin"
  cargo install --locked cargo-xwin
fi

# cargo-wix is only useful on Windows; skip on mac/linux.

# --- Homebrew bits (mac only) --------------------------------------------
if [[ "$OS" == "Darwin" ]]; then
  if ! command -v brew >/dev/null 2>&1; then
    warn "brew not found; skipping llvm / nsis install. Install Homebrew to enable."
  else
    # cargo-xwin uses clang/lld to link MSVC objects; llvm provides both.
    if brew list --versions llvm >/dev/null 2>&1; then
      ok "brew: llvm already installed"
    else
      log "brew install llvm"
      brew install llvm
    fi
    # nsis (`makensis`) is needed for NSIS .exe installer. The homebrew
    # formula is literally called makensis.
    if brew list --versions makensis >/dev/null 2>&1; then
      ok "brew: makensis already installed"
    else
      log "brew install makensis"
      brew install makensis
    fi
  fi
fi

# --- xwin cache prime ----------------------------------------------------
# Pre-download the Windows SDK + CRT so the first real build doesn't stall
# on a ~600 MB fetch under cargo. Cached in ~/.cache/cargo-xwin.
if command -v xwin >/dev/null 2>&1 || cargo xwin --help >/dev/null 2>&1; then
  XWIN_CACHE="${XWIN_CACHE_DIR:-$HOME/.cache/cargo-xwin/xwin}"
  if [[ -d "$XWIN_CACHE" ]]; then
    ok "xwin sdk cache present at $XWIN_CACHE"
  else
    log "priming xwin SDK cache (first run downloads ~600 MB)..."
    # The --accept-license is required; Microsoft's distributable EULA.
    cargo xwin build --help >/dev/null 2>&1 || true
    # Explicit SDK download. Uses rustup's target_triple config.
    (cd /tmp && cargo xwin --accept-license download 2>/dev/null) || \
      warn "xwin SDK wasn't pre-downloaded; first 'package:win' will fetch it."
  fi
fi

ok "setup complete."
echo
echo "Next steps:"
echo "  pnpm package:mac             # .dmg (current arch only, fastest)"
echo "  pnpm package:mac:universal   # .dmg (arm64 + x86_64 universal)"
echo "  pnpm package:win             # .exe / .msi via cargo-xwin"
echo "  pnpm package:all             # everything"
