#!/usr/bin/env bash
# ErsatzRS dev entry point. See BUILD.md for full reference.
#
# Usage:
#   ./run.sh           # alias for `dev`
#   ./run.sh dev       # cargo leptos watch (hot-reload)
#   ./run.sh check     # fast inner-loop type-check (one config; no codegen/link)
#   ./run.sh build     # one-shot build + run (CI parity)
#   ./run.sh serve     # rebuild once and serve (no watch loop)
#   ./run.sh seed      # apply migrations + seed admin fixture data
#   ./run.sh log       # tail ersatzrs.log
#
# Inner-loop tip: while iterating, prefer `./run.sh check [-p <crate>]` (or
# `./run.sh dev` for UI work) and DO NOT flip --features / --all-features /
# --all-targets between runs. Each flip is a separate compilation graph that
# re-runs rustc across the whole tree. Run the full multi-config gate
# (cargo xtask parity-check, the two clippy passes, cargo test --all-features,
# cargo leptos build) once at closeout, not per edit.

set -euo pipefail

cd "$(dirname "$0")"

export LD_LIBRARY_PATH="$HOME/.local/ffmpeg-8.1/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
export ErsatzRS_FFMPEG__PATH="$HOME/.local/ffmpeg-8.1/bin/ffmpeg"
export ErsatzRS_FFPROBE__PATH="$HOME/.local/ffmpeg-8.1/bin/ffprobe"

# Resolve the build target directory the way cargo itself does, so the size
# guard inspects the real artifact dir (the scratch disk path configured in
# .cargo/config.toml) instead of a stale local ./target. Precedence matches
# cargo: CARGO_TARGET_DIR env var, then build.target-dir in .cargo/config.toml,
# then the default "target".
resolve_target_dir() {
  if [[ -n "${CARGO_TARGET_DIR:-}" ]]; then
    printf '%s\n' "$CARGO_TARGET_DIR"
    return
  fi

  local config_target=""
  if [[ -f .cargo/config.toml ]]; then
    config_target="$(awk '
      /^[[:space:]]*\[/ { section = $0; gsub(/[[:space:]]/, "", section); next }
      section == "[build]" && /^[[:space:]]*target-dir[[:space:]]*=/ {
        sub(/^[^=]*=[[:space:]]*/, "", $0)
        gsub(/^["'"'"']|["'"'"'][[:space:]]*$/, "", $0)
        print; exit
      }
    ' .cargo/config.toml)"
  fi

  printf '%s\n' "${config_target:-target}"
}

check_target_size() {
  local target_dir
  target_dir="$(resolve_target_dir)"
  local threshold_gib="${ERSATZRS_TARGET_CLEAN_THRESHOLD_GIB:-40}"
  if [[ ! -d "$target_dir" ]] || ! command -v du >/dev/null 2>&1; then
    return
  fi

  local size_kib threshold_kib
  size_kib="$(du -sk "$target_dir" 2>/dev/null | awk '{print $1}')"
  if [[ -z "$size_kib" ]]; then
    return
  fi

  threshold_kib=$((threshold_gib * 1024 * 1024))
  if (( size_kib >= threshold_kib )); then
    local size_gib
    size_gib="$(awk -v kib="$size_kib" 'BEGIN { printf "%.1f", kib / 1024 / 1024 }')"
    echo "warning: $target_dir is ${size_gib} GiB; run 'cargo clean' before continuing large builds (threshold: ${threshold_gib} GiB)" >&2
  fi
}

cmd="${1:-dev}"
case "$cmd" in
  dev)
    check_target_size
    cd crates/ersatzrs-web
    exec cargo leptos watch
    ;;
  check)
    # Fast inner-loop type-check: no codegen, no linking (linking the big binary
    # to the 9p scratch disk is a major cost). Stays in ONE configuration so it
    # does not invalidate other caches. Scope it with cargo args, e.g.
    #   ./run.sh check -p ersatzrs-core
    #   ./run.sh check -p ersatzrs-web --all-targets
    # With no args it checks the whole workspace.
    check_target_size
    shift || true
    if [[ $# -eq 0 ]]; then
      set -- --workspace
    fi
    exec cargo check "$@"
    ;;
  build)
    check_target_size
    (cd crates/ersatzrs-web && cargo leptos build)
    cargo run --bin ersatzrs-web 2>&1 | tee ersatzrs.log
    ;;
  serve)
    check_target_size
    cd crates/ersatzrs-web
    exec cargo leptos serve
    ;;
  seed)
    db_path="${ERSATZRS_DB_PATH:-$HOME/.ersatzrs/ErsatzRS.db}"
    fixture="${ERSATZRS_SEED_FIXTURE:-tests/fixtures/seed_admin.sql}"
    if [[ ! -f "$fixture" ]]; then
      echo "seed fixture not found: $fixture" >&2
      exit 1
    fi
    mkdir -p "$(dirname "$db_path")"
    sqlite3 "$db_path" < "$fixture"
    echo "seeded $db_path from $fixture"
    ;;
  log)
    exec tail -F ersatzrs.log
    ;;
  *)
    echo "usage: $0 [dev|check|build|serve|seed|log]" >&2
    exit 1
    ;;
esac
