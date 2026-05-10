#!/usr/bin/env bash
# ErsatzRS dev entry point. See BUILD.md for full reference.
#
# Usage:
#   ./run.sh           # alias for `dev`
#   ./run.sh dev       # cargo leptos watch (hot-reload)
#   ./run.sh build     # one-shot build + run (CI parity)
#   ./run.sh serve     # rebuild once and serve (no watch loop)
#   ./run.sh seed      # apply migrations + seed admin fixture data
#   ./run.sh log       # tail ersatzrs.log

set -euo pipefail

cd "$(dirname "$0")"

export LD_LIBRARY_PATH="$HOME/.local/ffmpeg-8.1/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
export ErsatzRS_FFMPEG__PATH="$HOME/.local/ffmpeg-8.1/bin/ffmpeg"
export ErsatzRS_FFPROBE__PATH="$HOME/.local/ffmpeg-8.1/bin/ffprobe"

cmd="${1:-dev}"
case "$cmd" in
  dev)
    cd crates/ersatzrs-web
    exec cargo leptos watch
    ;;
  build)
    (cd crates/ersatzrs-web && cargo leptos build)
    cargo run --bin ersatzrs-web 2>&1 | tee ersatzrs.log
    ;;
  serve)
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
    echo "usage: $0 [dev|build|serve|seed|log]" >&2
    exit 1
    ;;
esac
