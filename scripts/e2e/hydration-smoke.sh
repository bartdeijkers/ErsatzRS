#!/usr/bin/env bash
# Whole-app island hydration smoke (release gate).
#
# Boots ersatzrs-web on a throwaway SQLite database with freshly built Leptos
# hydration assets, waits for /healthz, then runs the Playwright island
# hydration spec against every route that renders an `#[island]`. Exits non-zero
# if any island route fails to hydrate.
#
# This pairs with `cargo xtask smoke` (streaming/runtime parity) and the TSDuck
# playback gate (playback correctness). Run it before every release.
#
# Usage:
#   scripts/e2e/hydration-smoke.sh                 # build assets + boot + test
#   ERSATZRS_SKIP_LEPTOS_BUILD=1 scripts/e2e/hydration-smoke.sh   # reuse assets
#   ERSATZRS_E2E_KEEP_TEMP=1 scripts/e2e/hydration-smoke.sh       # keep temp dir
#
# Requirements: cargo, cargo-leptos, node + npx, an FFmpeg 8.1 runtime (see the
# FFmpeg env below; an operator override skips the managed startup download).

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$REPO_ROOT"

PORT="${ERSATZRS_E2E_PORT:-8409}"
BASE_URL="http://127.0.0.1:${PORT}"
WEB_CRATE="crates/ersatzrs-web"

log() { printf '\033[1;34m[hydration-smoke]\033[0m %s\n' "$*"; }
err() { printf '\033[1;31m[hydration-smoke]\033[0m %s\n' "$*" >&2; }

# --- FFmpeg runtime: prefer an operator override so startup skips the managed
# --- download. Mirrors run.sh. Only exported if the path actually exists.
LOCAL_FFMPEG_DIR="${ERSATZRS_FFMPEG_DIR:-$HOME/.local/ffmpeg-8.1}"
if [[ -x "$LOCAL_FFMPEG_DIR/bin/ffmpeg" ]]; then
  export LD_LIBRARY_PATH="$LOCAL_FFMPEG_DIR/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
  export ErsatzRS_FFMPEG__PATH="$LOCAL_FFMPEG_DIR/bin/ffmpeg"
  export ErsatzRS_FFPROBE__PATH="$LOCAL_FFMPEG_DIR/bin/ffprobe"
  log "using FFmpeg override at $LOCAL_FFMPEG_DIR (managed download skipped)"
else
  log "no FFmpeg override at $LOCAL_FFMPEG_DIR; startup may acquire the managed runtime (slower)"
fi

# --- Resolve the cargo target dir the same way cargo does (CARGO_TARGET_DIR,
# --- then build.target-dir in .cargo/config.toml, then ./target).
resolve_target_dir() {
  if [[ -n "${CARGO_TARGET_DIR:-}" ]]; then
    printf '%s\n' "$CARGO_TARGET_DIR"
    return
  fi
  local config_target=""
  if [[ -f .cargo/config.toml ]]; then
    config_target="$(awk '
      /^[[:space:]]*\[/ { section=$0; gsub(/[[:space:]]/,"",section); next }
      section=="[build]" && /^[[:space:]]*target-dir[[:space:]]*=/ {
        sub(/^[^=]*=[[:space:]]*/,"",$0); gsub(/^["'"'"']|["'"'"'][[:space:]]*$/,"",$0); print; exit
      }' .cargo/config.toml)"
  fi
  printf '%s\n' "${config_target:-target}"
}
TARGET_DIR="$(resolve_target_dir)"

# --- Preconditions.
command -v cargo >/dev/null 2>&1 || { err "cargo not found"; exit 1; }
command -v npx >/dev/null 2>&1 || { err "npx (node) not found"; exit 1; }
if command -v ss >/dev/null 2>&1 && ss -ltn "( sport = :$PORT )" 2>/dev/null | grep -q ":$PORT"; then
  err "port $PORT already in use; stop the running server first"
  exit 1
fi

# --- Build hydration assets (SSR binary + wasm bundle).
if [[ "${ERSATZRS_SKIP_LEPTOS_BUILD:-0}" != "1" ]]; then
  if ! command -v cargo-leptos >/dev/null 2>&1; then
    err "cargo-leptos not found. Install it with: cargo install cargo-leptos --locked"
    exit 1
  fi
  log "building Leptos hydration assets (cargo leptos build)…"
  (cd "$WEB_CRATE" && cargo leptos build)
else
  log "ERSATZRS_SKIP_LEPTOS_BUILD=1 — reusing existing hydration assets"
fi

# --- Use the server binary that `cargo leptos build` produced. Do NOT rebuild
# --- it with a separate `cargo build --bin`: that yields a server binary that is
# --- hydration-INCOMPATIBLE with the cargo-leptos WASM (mismatched build hash),
# --- which silently breaks island hydration for every page that resolves a
# --- server resource. The binary and the WASM must come from the SAME
# --- `cargo leptos build`. See BUILD.md "Hydration build consistency".
SERVER_BIN="$TARGET_DIR/debug/ersatzrs-web"
if [[ ! -x "$SERVER_BIN" ]]; then
  err "server binary not found at $SERVER_BIN. Run without ERSATZRS_SKIP_LEPTOS_BUILD so 'cargo leptos build' produces it (never 'cargo build --bin')."
  exit 1
fi
log "using cargo-leptos server binary: $SERVER_BIN"

# --- Locate the freshly built site dir (pkg/ersatzrs-web.js) and pin it as an
# --- absolute LEPTOS_SITE_ROOT so the server finds assets regardless of cwd.
# cargo-leptos writes the site bundle to a relative `target/site` (repo-root
# `target/`), independent of the cargo `build.target-dir` used for compilation,
# so search the likely roots and pick the most recently built bundle.
PKG_JS="$(find "$REPO_ROOT/target" "$TARGET_DIR" "$WEB_CRATE/target" \
  -type f -name 'ersatzrs-web.js' -path '*/site/pkg/*' -printf '%T@ %p\n' 2>/dev/null \
  | sort -rn | head -1 | cut -d' ' -f2- || true)"
if [[ -z "$PKG_JS" ]]; then
  err "could not find built hydration bundle (pkg/ersatzrs-web.js). Did 'cargo leptos build' succeed?"
  exit 1
fi
SITE_ROOT="$(cd "$(dirname "$(dirname "$PKG_JS")")" && pwd)"
log "serving hydration assets from $SITE_ROOT"

# --- Throwaway HOME + DB so we never touch the developer's ~/.ersatzrs data.
TMP_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/ersatzrs-hydration-smoke.XXXXXX")"
DB_PATH="$TMP_ROOT/ErsatzRS.db"
LOG_PATH="$TMP_ROOT/ersatzrs-web.log"
SERVER_PID=""

cleanup() {
  if [[ -n "$SERVER_PID" ]] && kill -0 "$SERVER_PID" 2>/dev/null; then
    kill "$SERVER_PID" 2>/dev/null || true
    wait "$SERVER_PID" 2>/dev/null || true
  fi
  if [[ "${ERSATZRS_E2E_KEEP_TEMP:-0}" == "1" ]]; then
    log "kept temp dir $TMP_ROOT (log: $LOG_PATH)"
  else
    rm -rf "$TMP_ROOT"
  fi
}
trap cleanup EXIT INT TERM

log "starting ersatzrs-web on $BASE_URL (db: $DB_PATH)…"
HOME="$TMP_ROOT" \
DATABASE_URL="sqlite:$DB_PATH" \
LEPTOS_SITE_ROOT="$SITE_ROOT" \
LEPTOS_OUTPUT_NAME="ersatzrs-web" \
LEPTOS_SITE_ADDR="127.0.0.1:$PORT" \
RUST_LOG="${RUST_LOG:-warn,ersatzrs_web=info}" \
  "$SERVER_BIN" >"$LOG_PATH" 2>&1 &
SERVER_PID=$!

# --- Wait for readiness.
log "waiting for $BASE_URL/healthz…"
HEALTHY=0
for _ in $(seq 1 120); do
  if ! kill -0 "$SERVER_PID" 2>/dev/null; then
    err "server exited during startup; last log lines:"
    tail -n 40 "$LOG_PATH" >&2 || true
    exit 1
  fi
  if curl -fsS -m 2 "$BASE_URL/healthz" >/dev/null 2>&1; then
    HEALTHY=1
    break
  fi
  sleep 1
done
if [[ "$HEALTHY" != "1" ]]; then
  err "/healthz never became reachable; last log lines:"
  tail -n 40 "$LOG_PATH" >&2 || true
  exit 1
fi
log "server healthy; running island hydration spec…"

# --- Ensure Playwright's Chromium is available, then run the spec.
if [[ "${ERSATZRS_SKIP_PW_INSTALL:-0}" != "1" ]]; then
  npx --yes playwright install chromium >/dev/null 2>&1 || \
    log "playwright install chromium skipped/failed; assuming a browser is present"
fi

set +e
ERSATZRS_BASE_URL="$BASE_URL" \
E2E_DB_PATH="$DB_PATH" \
ERSATZRS_DB_PATH="$DB_PATH" \
  npx --yes playwright test tests/e2e/island-hydration.spec.ts --project=chromium
STATUS=$?
set -e

if [[ "$STATUS" -eq 0 ]]; then
  log "PASS — all island routes hydrated cleanly"
else
  err "FAIL — one or more island routes did not hydrate (see report above)"
  err "server log: $LOG_PATH"
fi
exit "$STATUS"
