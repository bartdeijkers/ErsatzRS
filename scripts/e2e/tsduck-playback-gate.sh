#!/usr/bin/env bash
# TSDuck playback-correctness gate for ErsatzRS (E2E, NOT part of CI).
#
# This is a *required manual gate*: it proves that a transport stream ErsatzRS
# serves is actually correct for playback — no packet loss (continuity-counter
# errors), valid + low-jitter PCR timing, continuous PTS (no skips/jumps),
# the expected PAT/PMT/PCR/video/audio structure, clear (unscrambled) content,
# and a sane bitrate — across all three streaming pipelines and across
# playout-item boundaries (the concat seams where ErsatzTV-style stitching is
# most likely to break).
#
# It pairs with the calibration clips from scripts/calibration: those give a
# millisecond timer + per-frame counter so a human can *visually* confirm order
# and smoothness, while TSDuck proves the *container* is intact.
#
# MODES
#   baseline [DIR]            Validate the static calibration *.ts files
#                             (default DIR: <repo>/calibration-clips). No server.
#   url URL [SECONDS]         Capture a live ErsatzRS stream (.ts or .m3u8) and
#                             validate it. Use against your own running channel.
#   gate [CHANNEL]            Full E2E: build+boot ersatzrs-web on a throwaway
#                             SQLite DB, seed the 3 calibration clips as a short
#                             3-item playout, then capture and validate every
#                             pipeline (ts-legacy, ts-hybrid, hls-segmenter,
#                             hls-direct). Tears down and scans the server log.
#
# Exit status is 0 only if every check passed — wire it as a gate.
#
# Common flags: --seconds N  --keep  --frames  -h|--help
# Thresholds are environment-overridable; see scripts/e2e/README.md.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
# shellcheck source=scripts/e2e/tsduck-checks.sh
source "$SCRIPT_DIR/tsduck-checks.sh"

LISTEN_ADDR="127.0.0.1:8409"
BASE_URL="http://${LISTEN_ADDR}"
CALIB_DIR="$REPO_ROOT/calibration-clips"
CAPTURE_SECONDS="${CAPTURE_SECONDS:-20}"   # per-pipeline live capture window
ITEM_SECONDS="${ITEM_SECONDS:-8}"          # seconds of each clip the gate playout uses
KEEP=0
FRAMES=0

die() { printf 'tsduck-gate: %s\n' "$*" >&2; exit 2; }
need() { command -v "$1" >/dev/null 2>&1 || die "missing required tool: $1"; }

# Local ffmpeg-8.1 fails silently without its lib dir; the server (and our
# frame grabs) need it. Derive it from ffmpeg on PATH.
setup_ffmpeg_env() {
  FFMPEG_BIN="$(command -v ffmpeg || true)"
  [ -n "$FFMPEG_BIN" ] || return 0
  local libdir; libdir="$(cd "$(dirname "$FFMPEG_BIN")/../lib" 2>/dev/null && pwd || true)"
  [ -n "$libdir" ] && export LD_LIBRARY_PATH="$libdir:${LD_LIBRARY_PATH:-}"
}

resolve_target_dir() {
  if [ -n "${CARGO_TARGET_DIR:-}" ]; then echo "$CARGO_TARGET_DIR"; return; fi
  local td
  td="$(sed -nE 's/^[[:space:]]*target-dir[[:space:]]*=[[:space:]]*"?([^"]+)"?.*/\1/p' \
        "$REPO_ROOT/.cargo/config.toml" "$REPO_ROOT/config.toml" 2>/dev/null | head -1)"
  echo "${td:-$REPO_ROOT/target}"
}

# curl's -w already prints 000 on connection failure; no `|| echo` (that would
# double-print and false-positive the port-free check).
http_status() { curl -s -o /dev/null -w '%{http_code}' --max-time "${2:-5}" "$1" 2>/dev/null; }

wait_http() { # URL TIMEOUT_S → 0 when the URL returns HTTP 200 (body may be empty)
  local url="$1" timeout="${2:-60}"
  local end=$(( $(date +%s) + timeout ))
  while [ "$(date +%s)" -lt "$end" ]; do
    [ "$(http_status "$url" 4)" = 200 ] && return 0
    sleep 1
  done
  return 1
}

wait_for() { # URL CONTAINS TIMEOUT_S  → 0 when body matches CONTAINS
  # NB: separate `local` statements — args on one `local` line are all expanded
  # before assignment, so `end` cannot reference `timeout` on the same line.
  local url="$1" needle="$2" timeout="${3:-60}"
  local end=$(( $(date +%s) + timeout ))
  while [ "$(date +%s)" -lt "$end" ]; do
    if curl -fsS --max-time 5 "$url" 2>/dev/null | grep -q "$needle"; then return 0; fi
    sleep 1
  done
  return 1
}

# capture_stream KIND URL SECONDS OUTFILE   (KIND: http|hls)
capture_stream() {
  local kind="$1" url="$2" secs="$3" out="$4"
  info "capturing ${secs}s from $url"
  case "$kind" in
    http) tsp -I http "$url" -P until --seconds "$secs" -O file "$out" >/dev/null 2>&1 ;;
    hls)  tsp -I hls  "$url" -P until --seconds "$secs" -O file "$out" >/dev/null 2>&1 ;;
    *) die "unknown capture kind $kind" ;;
  esac
  [ -s "$out" ]
}

# validate one captured/static TS through the full battery
validate_ts() {
  local ts="$1" label="$2"
  analyze_ts "$ts" "$label"
  check_pcr_jitter "$ts" "$label"
  check_continuity_plugin "$ts" "$label"
}

grab_frames() { # TSFILE PREFIX  — visual evidence so a human can confirm 1→2→3
  [ "$FRAMES" = 1 ] || return 0
  command -v ffmpeg >/dev/null 2>&1 || { warn "ffmpeg not found — skipping frame evidence"; return 0; }
  local ts="$1" prefix="$2" i
  for i in 1 4 7 10 13 16 19; do
    ffmpeg -hide_banner -loglevel error -y -ss "$i" -i "$ts" -frames:v 1 "${prefix}_t${i}.png" 2>/dev/null || true
  done
  info "frame evidence: ${prefix}_t*.png"
}

# The clips are gitignored (regenerated, never committed). Generate them on
# demand so a fresh clone of main can run the gate without manual setup.
ensure_clips() {
  local n missing=0
  for n in 1 2 3; do
    [ -f "$CALIB_DIR/cal_${n}.ts" ] && [ -f "$CALIB_DIR/cal_${n}.mkv" ] || missing=1
  done
  [ "$missing" = 0 ] && return 0
  info "calibration clips missing — generating once (gitignored; ~a few minutes)…"
  "$REPO_ROOT/scripts/calibration/gen-calibration-clips.sh" >&2 || die "calibration clip generation failed"
}

# ---------------------------------------------------------------------------
# MODE: baseline — validate the static calibration TS files (no server)
# ---------------------------------------------------------------------------
mode_baseline() {
  local dir="${1:-$CALIB_DIR}"
  [ "$dir" = "$CALIB_DIR" ] && ensure_clips
  [ -d "$dir" ] || die "calibration dir not found: $dir (run scripts/calibration/gen-calibration-clips.sh)"
  local found=0 f
  for f in "$dir"/cal_*.ts; do
    [ -e "$f" ] || continue
    found=1
    printf '\n=== baseline: %s ===\n' "$(basename "$f")"
    validate_ts "$f" "baseline $(basename "$f")"
  done
  [ "$found" = 1 ] || die "no cal_*.ts in $dir"
  checks_summary
}

# ---------------------------------------------------------------------------
# MODE: url — capture a live stream the caller points at
# ---------------------------------------------------------------------------
mode_url() {
  local url="${1:?usage: url <stream-url> [seconds]}" secs="${2:-$CAPTURE_SECONDS}"
  WORKDIR="$(mktemp -d "${TMPDIR:-/tmp}/tsduck-gate-url.XXXXXX")"
  trap 'rm -rf "$WORKDIR"' EXIT
  local cap="$WORKDIR/capture.ts"
  if [[ "$url" == *.m3u8* ]]; then
    check_hls_playlist "$url" "live HLS"
    capture_stream hls "$url" "$secs" "$cap" || die "HLS capture produced no data from $url"
  else
    capture_stream http "$url" "$secs" "$cap" || die "TS capture produced no data from $url"
  fi
  printf '\n=== live: %s ===\n' "$url"
  validate_ts "$cap" "live $url"
  grab_frames "$cap" "$WORKDIR/frame"
  [ "$KEEP" = 1 ] && { info "kept capture: $WORKDIR"; trap - EXIT; }
  checks_summary
}

# ---------------------------------------------------------------------------
# MODE: gate — boot the real server, seed the clips, validate every pipeline
# ---------------------------------------------------------------------------
fmt_hms() { local s=$1; printf '%02d:%02d:%02d' $((s / 3600)) $(((s % 3600) / 60)) $((s % 60)); }

seed_db() { # DBPATH CHANNEL
  local db="$1" channel="$2" now t0 finish_total
  now=$(date +%s); t0=$((now - 2)); finish_total=$((t0 + 3 * ITEM_SECONDS))
  local da; da=$(date -u -d "@$now" '+%Y-%m-%d %H:%M:%S')
  local chid
  chid=$(sqlite3 "$db" "SELECT \"Id\" FROM \"Channel\" WHERE \"Number\"='$channel' ORDER BY \"Id\" LIMIT 1;")
  [ -n "$chid" ] || die "no Channel with Number=$channel in seeded DB (bootstrap should have created it)"

  {
    echo 'BEGIN IMMEDIATE;'
    for t in PlayoutItem Playout MediaStream MediaFile MediaVersion MovieMetadata Movie MediaItem LibraryPath LocalLibrary Library LocalMediaSource MediaSource; do
      echo "DELETE FROM \"$t\";"
    done
    cat <<SQL
DELETE FROM "ConfigElement" WHERE "Key" IN ('ffmpeg.segmenter.timeout_seconds','ffmpeg.segmenter.initial_segment_count');
INSERT INTO "ConfigElement" ("Key","Value") VALUES ('ffmpeg.segmenter.timeout_seconds','30');
INSERT INTO "ConfigElement" ("Key","Value") VALUES ('ffmpeg.segmenter.initial_segment_count','1');
INSERT INTO "MediaSource" ("Id") VALUES (1);
INSERT INTO "LocalMediaSource" ("Id") VALUES (1);
INSERT INTO "Library" ("Id","Name","MediaKind","MediaSourceId") VALUES (1,'Calibration Movies',1,1);
INSERT INTO "LibraryPath" ("Id","Path","LibraryId","LastScan") VALUES (1,'$CALIB_DIR','1','$da');
INSERT INTO "Playout" ("Id","ChannelId","ScheduleKind","Seed","OnDemandCheckpoint") VALUES (1,$chid,0,1,'$(date -u -d "@$t0" '+%Y-%m-%d %H:%M:%S')');
SQL
    # 3 media items (one per calibration clip)
    local n src
    for n in 1 2 3; do
      src="$CALIB_DIR/cal_${n}.mkv"; [ -f "$src" ] || src="$CALIB_DIR/cal_${n}.ts"
      cat <<SQL
INSERT INTO "MediaItem" ("Id","LibraryPathId","State") VALUES ($n,1,0);
INSERT INTO "Movie" ("Id") VALUES ($n);
INSERT INTO "MovieMetadata" ("DateAdded","DateUpdated","MetadataKind","MovieId","Title") VALUES ('$da','$da',0,$n,'Calibration $n');
INSERT INTO "MediaVersion" ("Id","DateAdded","DateUpdated","Duration","Height","MovieId","Name","RFrameRate","VideoScanKind","Width") VALUES ($n,'$da','$da','00:03:00',1080,$n,'Calibration $n','60/1',1,1920);
INSERT INTO "MediaFile" ("Id","MediaVersionId","Path","PathHash") VALUES ($n,$n,'$src','calib-hash-$n');
INSERT INTO "MediaStream" ("Index","Codec","MediaStreamKind","Channels","Default","Forced","MediaVersionId","PixelFormat") VALUES (0,'h264',1,0,1,0,$n,'yuv420p');
INSERT INTO "MediaStream" ("Index","Codec","MediaStreamKind","Language","Channels","Default","Forced","MediaVersionId") VALUES (1,'aac',2,'eng',2,1,0,$n);
SQL
    done
    # Cyclic playout 1→2→3→1… of ITEM_SECONDS each, long enough to cover the
    # whole test window so the channel never runs dry mid-capture (which would
    # log "no active playout item"). Short items mean every capture crosses
    # several concat seams — the real integrity test.
    local cover count i clip istart ifin out_hms
    cover="${CAL_PLAYOUT_COVER:-$((CAPTURE_SECONDS * 5 + 120))}"
    count=$(((cover + ITEM_SECONDS - 1) / ITEM_SECONDS))
    out_hms="$(fmt_hms "$ITEM_SECONDS")"
    for ((i = 0; i < count; i++)); do
      clip=$((i % 3 + 1)); istart=$((t0 + i * ITEM_SECONDS)); ifin=$((istart + ITEM_SECONDS))
      printf 'INSERT INTO "PlayoutItem" ("Id","DisableWatermarks","FillerKind","Finish","GuideGroup","InPoint","MediaItemId","OutPoint","PlayoutId","Start") VALUES (%d,0,0,'"'"'%s'"'"',0,'"'"'00:00:00'"'"',%d,'"'"'%s'"'"',1,'"'"'%s'"'"');\n' \
        "$((i + 1))" "$(date -u -d "@$ifin" '+%Y-%m-%d %H:%M:%S')" "$clip" "$out_hms" "$(date -u -d "@$istart" '+%Y-%m-%d %H:%M:%S')"
    done
    cat <<SQL
UPDATE "Channel" SET "StreamingMode"=5,"IdleBehavior"=0,"PlayoutMode"=0,"IsEnabled"=1 WHERE "Id"=$chid;
COMMIT;
SQL
  } | sqlite3 "$db" || die "seeding failed"
  info "seeded channel $channel ($chid): cyclic 1→2→3 playout, ${ITEM_SECONDS}s items"
}

mode_gate() {
  local channel="${1:-1}"
  need cargo; need sqlite3
  ensure_clips

  local code; code="$(http_status "$BASE_URL/healthz")"
  if [ "$code" != 000 ]; then
    die "$LISTEN_ADDR already serving (HTTP $code) — another ersatzrs-web (or another agent's) is running. The port is hardcoded to 8409; stop that server, or use 'url' mode against it instead."
  fi

  WORKDIR="$(mktemp -d "${TMPDIR:-/tmp}/tsduck-gate.XXXXXX")"
  local db="$WORKDIR/gate.db" log="$WORKDIR/ersatzrs-web.log"
  SERVER_PID=""
  cleanup() {
    [ -n "$SERVER_PID" ] && kill "$SERVER_PID" 2>/dev/null && wait "$SERVER_PID" 2>/dev/null
    if [ "$KEEP" = 1 ]; then info "kept work dir: $WORKDIR"; else rm -rf "$WORKDIR"; fi
  }
  trap cleanup EXIT

  info "building ersatzrs-web…"
  ( cd "$REPO_ROOT" && cargo build --bin ersatzrs-web ) || die "cargo build failed"
  local bin; bin="$(resolve_target_dir)/debug/ersatzrs-web"
  [ -x "$bin" ] || die "server binary not found at $bin"

  info "booting server (db=$db)…"
  DATABASE_URL="sqlite:$db" HOME="$WORKDIR" \
    RUST_LOG="${RUST_LOG:-info,ersatzrs_infra::hls_worker=debug,ersatzrs_web::transport::iptv=debug}" \
    "$bin" >"$log" 2>&1 &
  SERVER_PID=$!

  wait_http "$BASE_URL/healthz" 90 || die "server /healthz never returned 200 (log: $log)"
  pass "server up · /healthz reachable"
  seed_db "$db" "$channel"

  # --- MPEG-TS pipelines ---
  local cap
  for spec in "ts-legacy:legacy concat" "ts:hybrid wrapper"; do
    local mode="${spec%%:*}" desc="${spec#*:}"
    cap="$WORKDIR/ts_${mode}.ts"
    printf '\n=== pipeline: MPEG-TS (%s, ?mode=%s) ===\n' "$desc" "$mode"
    if capture_stream http "$BASE_URL/iptv/channel/$channel.ts?mode=$mode" "$CAPTURE_SECONDS" "$cap"; then
      validate_ts "$cap" "ts-$mode"
      [ "$mode" = "ts-legacy" ] && grab_frames "$cap" "$WORKDIR/frame_tslegacy"
    else
      fail "ts-$mode · capture" "no data from ?mode=$mode"
    fi
  done

  # --- HLS segmenter ---
  printf '\n=== pipeline: HLS segmenter (?mode=segmenter) ===\n'
  curl -fsS --max-time 10 "$BASE_URL/iptv/channel/$channel.m3u8?mode=segmenter" >/dev/null 2>&1 \
    && pass "hls-segmenter · variant playlist served" || fail "hls-segmenter · variant playlist"
  local seg_pl="$BASE_URL/iptv/session/$channel/hls.m3u8"
  if wait_for "$seg_pl" "#EXTINF" 45; then
    check_hls_playlist "$seg_pl" "hls-segmenter"
    cap="$WORKDIR/hls_segmenter.ts"
    if capture_stream hls "$seg_pl" "$CAPTURE_SECONDS" "$cap"; then
      validate_ts "$cap" "hls-segmenter"
    else
      fail "hls-segmenter · capture" "tsp -I hls produced no data"
    fi
  else
    fail "hls-segmenter · segments" "session playlist had no #EXTINF within 45s (log: $log)"
  fi

  # --- HLS direct ---
  printf '\n=== pipeline: HLS direct (?mode=hls-direct) ===\n'
  local direct_pl="$BASE_URL/iptv/channel/$channel.m3u8?mode=hls-direct"
  if wait_for "$direct_pl" "#EXTINF" 20; then
    check_hls_playlist "$direct_pl" "hls-direct"
    cap="$WORKDIR/hls_direct.ts"
    if capture_stream hls "$direct_pl" "$CAPTURE_SECONDS" "$cap"; then
      validate_ts "$cap" "hls-direct"
    else
      fail "hls-direct · capture" "tsp -I hls produced no data"
    fi
  else
    warn "hls-direct · no segments within 20s — channel may not support direct mode for this source"
  fi

  # --- server log must be free of known runtime-failure signatures ---
  printf '\n=== server log scan ===\n'
  local sigs=(
    "ffmpeg hls segmenter exited unsuccessfully" "failed to spawn ffmpeg hls segmenter"
    "hls segmenter did not produce initial playlist segments before timeout"
    "Impossible to convert between the formats" "Error submitting packet to decoder"
    "Decode error rate" "no active playout item" "channel has no playback ffmpeg profile"
  )
  local hit=0 s
  for s in "${sigs[@]}"; do
    if grep -qF "$s" "$log"; then fail "server log clean" "found: $s"; hit=1; fi
  done
  [ "$hit" = 0 ] && pass "server log free of known failure signatures"

  checks_summary
}

# ---------------------------------------------------------------------------
usage() { sed -nE 's/^# ?//p' "${BASH_SOURCE[0]}" | sed -n '1,38p'; }

main() {
  need tsanalyze; need tsp; need jq; need curl
  setup_ffmpeg_env
  local cmd="" args=()
  while [ $# -gt 0 ]; do
    case "$1" in
      -h|--help) usage; exit 0 ;;
      --seconds) CAPTURE_SECONDS="$2"; shift 2 ;;
      --item-seconds) ITEM_SECONDS="$2"; shift 2 ;;
      --keep) KEEP=1; shift ;;
      --frames) FRAMES=1; shift ;;
      baseline|url|gate) cmd="$1"; shift ;;
      *) args+=("$1"); shift ;;
    esac
  done
  case "$cmd" in
    baseline) mode_baseline "${args[@]:-}" ;;
    url)      mode_url "${args[@]}" ;;
    gate)     mode_gate "${args[@]:-}" ;;
    "")       usage; exit 2 ;;
    *)        die "unknown command: $cmd" ;;
  esac
}

main "$@"
