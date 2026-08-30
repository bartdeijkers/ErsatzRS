#!/usr/bin/env bash
# Structural gate against the recurring "wasm checksum mismatch" release failure.
#
# The release builds the Leptos WASM (web-assets job, LEPTOS_HASH_FILES=false)
# and the server binary (binaries job) in SEPARATE jobs. If their file-hashing
# settings disagree, the running server emits `<script src="/pkg/ersatzrs-web.<hash>.js">`
# (or `.wasm`) URLs that do not exist in the bundled `site/pkg/`, so the browser
# never loads the WASM and NOTHING on the admin UI hydrates. This has shipped
# broken several times because no step ever booted the assembled bundle.
#
# This gate boots the ACTUAL server binary against the ACTUAL site assets and
# asserts that every `/pkg/*` asset the server references in its shell HTML
# resolves with HTTP 200. A checksum/hash mismatch makes a referenced asset 404,
# which fails this gate BEFORE the archive is published.
#
# Usage:
#   ERSATZRS_WEB_BIN=/path/to/ersatzrs-web \
#   ERSATZRS_SITE_ROOT=/path/to/site \
#   scripts/release/verify-web-bundle.sh
#
# Defaults: discovers the binary under target*/release|debug and the site under
# release-web-assets/site or target/site.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$REPO_ROOT"

PORT="${ERSATZRS_VERIFY_PORT:-8409}"
BASE_URL="http://127.0.0.1:${PORT}"

log() { printf '\033[1;34m[verify-web-bundle]\033[0m %s\n' "$*"; }
err() { printf '\033[1;31m[verify-web-bundle]\033[0m %s\n' "$*" >&2; }

WINDOWS_POSIX=0
case "$(uname -s)" in
  MINGW*|MSYS*|CYGWIN*) WINDOWS_POSIX=1 ;;
esac

# Git Bash tools need POSIX paths, while a native Windows server process needs
# drive-letter paths in its environment. Keep both forms so mktemp/cleanup
# remain shell-native and SQLite/site/runtime paths remain process-native.
native_env_path() {
  if [[ "$WINDOWS_POSIX" -eq 1 ]]; then
    cygpath -m "$1"
  else
    printf '%s\n' "$1"
  fi
}

# --- Locate the server binary.
SERVER_BIN="${ERSATZRS_WEB_BIN:-}"
if [[ -z "$SERVER_BIN" ]]; then
  SERVER_BIN="$(find target release-web-assets -type f \( -name 'ersatzrs-web' -o -name 'ersatzrs-web.exe' \) 2>/dev/null \
    | grep -E '/(release|debug)/ersatzrs-web(\.exe)?$' | head -1 || true)"
fi
[[ -n "$SERVER_BIN" && -x "$SERVER_BIN" ]] || { err "server binary not found (set ERSATZRS_WEB_BIN)"; exit 1; }

# --- Locate the site assets.
SITE_ROOT="${ERSATZRS_SITE_ROOT:-}"
if [[ -z "$SITE_ROOT" ]]; then
  for c in "release-web-assets/site" "target/site" "$REPO_ROOT/target/site"; do
    [[ -f "$c/pkg/ersatzrs-web.js" ]] && { SITE_ROOT="$(cd "$c" && pwd)"; break; }
  done
fi
[[ -n "$SITE_ROOT" && -d "$SITE_ROOT/pkg" ]] || { err "site assets not found (set ERSATZRS_SITE_ROOT)"; exit 1; }
log "binary: $SERVER_BIN"
log "site:   $SITE_ROOT"

# The channel preview bridge lazy-loads the pinned Hls.js ESM asset. Verify the
# assembled site contains both files before booting so a release cannot ship a
# player whose hydration succeeds but whose playback dependency 404s.
for preview_asset in \
  "channel-preview.js" \
  "vendor/hls/hls.mjs" \
  "vendor/hls/LICENSE"; do
  [[ -f "$SITE_ROOT/$preview_asset" ]] || {
    err "channel preview asset missing from assembled site: $preview_asset"
    exit 1
  }
done

# --- Prefer a local FFmpeg 9 override so startup need not download a managed
# runtime in CI. PATH is runner-owned and can drift to an unsupported major;
# only pass an auto-discovered binary after checking the version contract. If
# no compatible local binary exists, leave the override unset so this gate
# exercises the same managed ErsatzRS-ffmpeg acquisition used in production.
FF_BIN="${ErsatzRS_FFMPEG__PATH:-}"
if [[ -z "$FF_BIN" ]]; then
  for c in "$HOME/.local/ffmpeg-9.0/bin/ffmpeg" "$(command -v ffmpeg 2>/dev/null || true)"; do
    [[ -n "$c" && -x "$c" ]] || continue
    version_line="$("$c" -version 2>/dev/null | head -1 || true)"
    if [[ "$version_line" =~ ^ffmpeg[[:space:]]version[[:space:]](n)?9([.[:space:]-]|$) ]]; then
      FF_BIN="$c"
      break
    fi
    log "ignoring incompatible FFmpeg candidate: $c (${version_line:-version unavailable})"
  done
fi
TMP_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/ersatzrs-verify-bundle.XXXXXX")"
TMP_ROOT_ENV="$(native_env_path "$TMP_ROOT")"
SITE_ROOT_ENV="$(native_env_path "$SITE_ROOT")"
LOG_PATH="$TMP_ROOT/server.log"
SERVER_PID=""
cleanup() {
  if [[ -n "$SERVER_PID" ]]; then
    kill "$SERVER_PID" 2>/dev/null || true
    wait "$SERVER_PID" 2>/dev/null || true
  fi
  for _ in $(seq 1 20); do
    if rm -rf "$TMP_ROOT" 2>/dev/null; then
      return 0
    fi
    sleep 0.1
  done
  err "failed to remove temporary bundle-verification directory: $TMP_ROOT"
  return 1
}
trap cleanup EXIT INT TERM

# Build the full server env in one array (never empty, so `"${SRV_ENV[@]}"` is
# safe under `set -u` on older bash / Git Bash).
declare -a SRV_ENV=(
  "HOME=$TMP_ROOT_ENV"
  "DATABASE_URL=sqlite:$TMP_ROOT_ENV/verify.db"
  "LEPTOS_SITE_ROOT=$SITE_ROOT_ENV"
  "LEPTOS_OUTPUT_NAME=ersatzrs-web"
  "LEPTOS_SITE_ADDR=127.0.0.1:$PORT"
  "RUST_LOG=warn"
)
if [[ -n "$FF_BIN" ]]; then
  SRV_ENV+=(
    "ErsatzRS_FFMPEG__PATH=$(native_env_path "$FF_BIN")"
    "ErsatzRS_FFPROBE__PATH=$(native_env_path "$(dirname "$FF_BIN")/ffprobe")"
  )
  if [[ "$WINDOWS_POSIX" -eq 0 ]]; then
    FF_DIR="$(cd "$(dirname "$FF_BIN")/.." && pwd)"
    SRV_ENV+=("LD_LIBRARY_PATH=$FF_DIR/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}")
  fi
  log "ffmpeg override: $FF_BIN"
else
  log "no local FFmpeg 9 override; using managed ErsatzRS-ffmpeg acquisition"
fi

log "booting server on $BASE_URL…"
env "${SRV_ENV[@]}" "$SERVER_BIN" >"$LOG_PATH" 2>&1 &
SERVER_PID=$!

for _ in $(seq 1 120); do
  kill -0 "$SERVER_PID" 2>/dev/null || { err "server exited during startup:"; tail -n 30 "$LOG_PATH" >&2; exit 1; }
  curl -fsS -m 2 "$BASE_URL/healthz" >/dev/null 2>&1 && break
  sleep 1
done
curl -fsS -m 2 "$BASE_URL/healthz" >/dev/null 2>&1 || { err "/healthz never came up:"; tail -n 30 "$LOG_PATH" >&2; exit 1; }

# --- Extract every /pkg asset the shell HTML references and verify each loads.
SHELL_HTML="$(curl -fsS "$BASE_URL/" 2>/dev/null || true)"
if [[ -z "$SHELL_HTML" ]]; then
  err "shell HTML failed to render (empty/error response) — the server cannot"
  err "build its hydration markup, which is exactly what a wasm hash/checksum"
  err "mismatch (e.g. LEPTOS_HASH_FILES set but no hash.txt in the bundle) looks like:"
  tail -n 20 "$LOG_PATH" >&2
  exit 1
fi
mapfile -t ASSETS < <(printf '%s' "$SHELL_HTML" | grep -oE '/pkg/[A-Za-z0-9._/-]+' | sort -u)

if [[ "${#ASSETS[@]}" -eq 0 ]]; then
  err "shell HTML referenced no /pkg/* assets — HydrationScripts missing?"
  exit 1
fi
has_js=0; has_wasm=0; failed=0
for asset in "${ASSETS[@]}"; do
  code="$(curl -s -o /dev/null -w '%{http_code}' "$BASE_URL$asset")"
  [[ "$asset" == *.js ]] && has_js=1
  [[ "$asset" == *.wasm ]] && has_wasm=1
  if [[ "$code" == "200" ]]; then
    log "OK   $asset"
  else
    err "MISS $asset -> HTTP $code  (wasm checksum/hash mismatch: server references an asset the bundle does not contain)"
    failed=1
  fi
done

if [[ "$has_js" -ne 1 || "$has_wasm" -ne 1 ]]; then
  err "shell did not reference both a .js and a .wasm hydration asset (js=$has_js wasm=$has_wasm)"
  failed=1
fi

if [[ "$failed" -ne 0 ]]; then
  err "FAIL — web bundle is inconsistent; do NOT publish this release."
  exit 1
fi
log "PASS — server binary and WASM bundle are consistent; all hydration assets resolve."
