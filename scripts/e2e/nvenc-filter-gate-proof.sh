#!/usr/bin/env bash
# Live NVENC encoder-only filter-gate proof for ErsatzRS (E2E, NOT part of CI).
#
# This gate boots ersatzrs-web on a throwaway SQLite database, seeds a short
# SDR calibration clip as the active channel content, forces the channel's
# profile to NVENC H.264, and captures the live MPEG-TS stream. It is only valid
# when the selected FFmpeg build reports NVENC encoders plus cuda hwaccel while
# omitting the CUDA filter kernels ErsatzRS gates on: scale_cuda, yadif_cuda,
# and overlay_cuda.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

LISTEN_ADDR="${NVENC_PROOF_LISTEN_ADDR:-127.0.0.1:8409}"
BASE_URL="http://${LISTEN_ADDR}"
CHANNEL="${NVENC_PROOF_CHANNEL:-1}"
WORKDIR=""
SERVER_PID=""
KEEP="${NVENC_PROOF_KEEP:-0}"

CLIP_SECONDS="${NVENC_PROOF_CLIP_SECONDS:-8}"
CAPTURE_SECONDS="${NVENC_PROOF_CAPTURE_SECONDS:-5}"
CALIB_DIR="${NVENC_PROOF_CALIB_DIR:-${TMPDIR:-/tmp}/ersatzrs-nvenc-proof-clips}"
CALIB_SIZE="${NVENC_PROOF_SIZE:-1920x1080}"
CALIB_FPS="${NVENC_PROOF_FPS:-30}"
FFMPEG_DIR="${NVENC_PROOF_FFMPEG_DIR:-}"

REQUIRED_ABSENT_CUDA_FILTERS="scale_cuda yadif_cuda overlay_cuda"

die() { printf 'nvenc-proof: %s\n' "$*" >&2; exit 2; }
info() { printf '[ info ] %s\n' "$*"; }
pass() { printf '[ PASS ] %s\n' "$*"; }
need() { command -v "$1" >/dev/null 2>&1 || die "missing required tool: $1"; }

setup_ffmpeg_env() {
  if [ -n "$FFMPEG_DIR" ]; then
    if [ -x "$FFMPEG_DIR/ffmpeg" ]; then
      FFMPEG_BIN="$FFMPEG_DIR/ffmpeg"
      FFPROBE_BIN="$FFMPEG_DIR/ffprobe"
    elif [ -x "$FFMPEG_DIR/bin/ffmpeg" ]; then
      FFMPEG_BIN="$FFMPEG_DIR/bin/ffmpeg"
      FFPROBE_BIN="$FFMPEG_DIR/bin/ffprobe"
    else
      die "NVENC_PROOF_FFMPEG_DIR must contain ffmpeg/ffprobe or bin/ffmpeg/bin/ffprobe"
    fi
  else
    FFMPEG_BIN="$(command -v ffmpeg || true)"
    FFPROBE_BIN="$(command -v ffprobe || true)"
  fi

  [ -n "${FFMPEG_BIN:-}" ] || die "ffmpeg not found; set NVENC_PROOF_FFMPEG_DIR or PATH"
  [ -x "$FFMPEG_BIN" ] || die "ffmpeg is not executable: $FFMPEG_BIN"
  [ -n "${FFPROBE_BIN:-}" ] || die "ffprobe not found beside ffmpeg"
  [ -x "$FFPROBE_BIN" ] || die "ffprobe is not executable: $FFPROBE_BIN"

  local bindir libdir
  bindir="$(cd "$(dirname "$FFMPEG_BIN")" && pwd)"
  libdir="$(cd "$bindir/lib" 2>/dev/null && pwd || true)"
  [ -n "$libdir" ] || libdir="$(cd "$bindir/../lib" 2>/dev/null && pwd || true)"
  [ -n "$libdir" ] && export LD_LIBRARY_PATH="$libdir:${LD_LIBRARY_PATH:-}"
  export PATH="$bindir:$PATH"
  export ERSATZRS_FFMPEG__PATH="$FFMPEG_BIN"
  export ERSATZRS_FFPROBE__PATH="$FFPROBE_BIN"
}

require_capabilities() {
  local hwaccels encoders filters present_filter present_filters=""
  hwaccels="$("$FFMPEG_BIN" -hide_banner -hwaccels 2>&1)"
  encoders="$("$FFMPEG_BIN" -hide_banner -encoders 2>&1)"
  filters="$("$FFMPEG_BIN" -hide_banner -filters 2>&1)"

  printf '%s\n' "$hwaccels" | grep -qw cuda \
    || die "selected FFmpeg does not report cuda hwaccel: $FFMPEG_BIN"
  printf '%s\n' "$encoders" | grep -qw h264_nvenc \
    || die "selected FFmpeg does not report h264_nvenc: $FFMPEG_BIN"
  printf '%s\n' "$filters" | grep -qw drawvg \
    || die "selected FFmpeg does not report drawvg: $FFMPEG_BIN"
  printf '%s\n' "$filters" | grep -qw zscale \
    || die "selected FFmpeg does not report zscale: $FFMPEG_BIN"

  for present_filter in $REQUIRED_ABSENT_CUDA_FILTERS; do
    if printf '%s\n' "$filters" | grep -qw "$present_filter"; then
      present_filters="${present_filters}${present_filters:+ }${present_filter}"
    fi
  done

  if [ -n "$present_filters" ]; then
    die "selected FFmpeg is a full CUDA-filter build, not the encoder-only proof build; present filters: $present_filters"
  fi

  pass "FFmpeg reports cuda + h264_nvenc and omits $REQUIRED_ABSENT_CUDA_FILTERS"
}

resolve_target_dir() {
  if [ -n "${CARGO_TARGET_DIR:-}" ]; then
    printf '%s\n' "$CARGO_TARGET_DIR"
    return
  fi
  local target_dir
  target_dir="$(sed -nE 's/^[[:space:]]*target-dir[[:space:]]*=[[:space:]]*"?([^"]+)"?.*/\1/p' \
    "$REPO_ROOT/.cargo/config.toml" "$REPO_ROOT/config.toml" 2>/dev/null | head -1)"
  printf '%s\n' "${target_dir:-$REPO_ROOT/target}"
}

host_path() {
  if command -v cygpath >/dev/null 2>&1; then
    cygpath -m "$1" 2>/dev/null && return 0
  fi
  printf '%s\n' "$1"
}

http_status() {
  curl -s -o /dev/null -w '%{http_code}' --max-time "${2:-5}" "$1" 2>/dev/null
}

wait_http() {
  local url="$1" timeout="${2:-60}" end
  end=$(( $(date +%s) + timeout ))
  while [ "$(date +%s)" -lt "$end" ]; do
    [ "$(http_status "$url" 4)" = 200 ] && return 0
    sleep 1
  done
  return 1
}

ensure_calibration() {
  [ -f "$CALIB_DIR/cal_1.mkv" ] && return 0

  info "generating SDR calibration clip in $CALIB_DIR"
  CAL_OUT="$CALIB_DIR" \
  CAL_CLIPS=1 \
  CAL_DURATION="$CLIP_SECONDS" \
  CAL_FPS="$CALIB_FPS" \
  CAL_SIZE="$CALIB_SIZE" \
  CAL_COLOR_MODES=sdr \
  CAL_MAKE_TS=0 \
    "$REPO_ROOT/scripts/calibration/gen-calibration-clips.sh"
}

seed_db() {
  local db="$1" source="$2" src_db_path calib_db_path item_seconds
  src_db_path="$(host_path "$source")"
  calib_db_path="$(host_path "$CALIB_DIR")"
  item_seconds="$(printf '00:00:%02d' "$CLIP_SECONDS")"

  sqlite3 "$db" <<SQL
.timeout 30000
BEGIN IMMEDIATE;
DELETE FROM "PlayoutItem";
DELETE FROM "Playout";
DELETE FROM "MediaStream";
DELETE FROM "MediaFile";
DELETE FROM "MediaVersion";
DELETE FROM "MovieMetadata";
DELETE FROM "Movie";
DELETE FROM "MediaItem";
DELETE FROM "LibraryPath";
DELETE FROM "LocalLibrary";
DELETE FROM "Library";
DELETE FROM "LocalMediaSource";
DELETE FROM "MediaSource";
DELETE FROM "ConfigElement" WHERE "Key" IN ('ffmpeg.segmenter.timeout_seconds','ffmpeg.segmenter.initial_segment_count');
INSERT INTO "ConfigElement" ("Key","Value") VALUES ('ffmpeg.segmenter.timeout_seconds','30');
INSERT INTO "ConfigElement" ("Key","Value") VALUES ('ffmpeg.segmenter.initial_segment_count','1');
INSERT INTO "MediaSource" ("Id") VALUES (1);
INSERT INTO "LocalMediaSource" ("Id") VALUES (1);
INSERT INTO "Library" ("Id","Name","MediaKind","MediaSourceId") VALUES (1,'NVENC Calibration Movies',1,1);
INSERT INTO "LibraryPath" ("Id","Path","LibraryId","LastScan") VALUES (1,'$calib_db_path','1',datetime('now'));
INSERT INTO "MediaItem" ("Id","LibraryPathId","State") VALUES (1,1,0);
INSERT INTO "Movie" ("Id") VALUES (1);
INSERT INTO "MovieMetadata" ("DateAdded","DateUpdated","MetadataKind","MovieId","Title") VALUES (datetime('now'),datetime('now'),0,1,'NVENC Filter Gate Calibration');
INSERT INTO "MediaVersion" ("Id","DateAdded","DateUpdated","Duration","Height","MovieId","Name","RFrameRate","VideoScanKind","Width")
  VALUES (1,datetime('now'),datetime('now'),'$item_seconds',1080,1,'NVENC Filter Gate Calibration','$CALIB_FPS/1',1,1920);
INSERT INTO "MediaFile" ("Id","MediaVersionId","Path","PathHash") VALUES (1,1,'$src_db_path','nvenc-filter-gate-proof');
INSERT INTO "MediaStream" ("Index","Codec","Profile","MediaStreamKind","Channels","Default","Forced","MediaVersionId","BitsPerRawSample","PixelFormat","ColorPrimaries","ColorRange","ColorSpace","ColorTransfer")
  VALUES (0,'h264','High',1,0,1,0,1,8,'yuv420p','bt709','tv','bt709','bt709');
INSERT INTO "MediaStream" ("Index","Codec","MediaStreamKind","Language","Channels","Default","Forced","MediaVersionId")
  VALUES (1,'aac',2,'eng',2,1,0,1);
INSERT INTO "Playout" ("Id","ChannelId","ScheduleKind","Seed","OnDemandCheckpoint")
  SELECT 1,"Id",0,1,datetime('now','-1 second') FROM "Channel" WHERE "Number"='$CHANNEL' ORDER BY "Id" LIMIT 1;
INSERT INTO "PlayoutItem" ("Id","DisableWatermarks","FillerKind","Finish","GuideGroup","InPoint","MediaItemId","OutPoint","PlayoutId","Start")
  VALUES (1,0,0,datetime('now','+$CLIP_SECONDS seconds'),0,'00:00:00',1,'$item_seconds',1,datetime('now','-1 second'));
UPDATE "FFmpegProfile"
   SET "HardwareAcceleration" = 2,
       "VideoFormat" = 1,
       "VideoProfile" = 'high',
       "VideoPreset" = 'llhq',
       "BitDepth" = 0,
       "VideoBitrate" = 8000,
       "VideoBufferSize" = 16000,
       "AudioFormat" = 1,
       "AudioBitrate" = 192,
       "AudioBufferSize" = 384,
       "AudioChannels" = 2,
       "NormalizeColors" = 1,
       "NormalizeVideo" = 1,
       "NormalizeFramerate" = 0,
       "DeinterlaceVideo" = 1
 WHERE "Id" = (SELECT "FFmpegProfileId" FROM "Channel" WHERE "Number"='$CHANNEL' ORDER BY "Id" LIMIT 1);
UPDATE "Channel" SET "StreamingMode"=5,"IdleBehavior"=0,"PlayoutMode"=0,"TranscodeMode"=0,"IsEnabled"=1 WHERE "Number"='$CHANNEL';
COMMIT;
SQL
  pass "seeded channel $CHANNEL with SDR source and NVENC H.264 target"
}

capture_live_stream() {
  local out="$1" url="$BASE_URL/iptv/channel/$CHANNEL.ts?mode=ts"
  info "capturing ${CAPTURE_SECONDS}s live NVENC output from $url"
  "$FFMPEG_BIN" -hide_banner -loglevel error -y -t "$CAPTURE_SECONDS" -i "$url" \
    -map 0:v:0 -map 0:a? -c copy "$out"
  [ -s "$out" ] || die "live capture is empty"
}

probe_capture() {
  local file="$1" json="$WORKDIR/capture.json"
  "$FFPROBE_BIN" -v error -select_streams v:0 \
    -show_entries stream=codec_name,pix_fmt,color_primaries,color_transfer,color_space \
    -of json "$file" >"$json"

  jq -e '
    .streams[0].codec_name == "h264"
    and .streams[0].pix_fmt == "yuv420p"
    and .streams[0].color_primaries == "bt709"
    and .streams[0].color_transfer == "bt709"
    and .streams[0].color_space == "bt709"
  ' "$json" >/dev/null
  pass "live capture is H.264 yuv420p BT.709"
}

cleanup() {
  [ -n "$SERVER_PID" ] && kill "$SERVER_PID" 2>/dev/null && wait "$SERVER_PID" 2>/dev/null || true
  if [ -n "$WORKDIR" ] && [ -d "$WORKDIR" ]; then
    if [ "$KEEP" = 1 ]; then
      info "kept work dir: $WORKDIR"
    else
      rm -rf "$WORKDIR"
    fi
  fi
}

usage() {
  cat <<EOF
Live NVENC encoder-only filter-gate proof for ErsatzRS (E2E, NOT part of CI).

Requires an FFmpeg build that reports:
  - cuda hwaccel
  - h264_nvenc
  - drawvg and zscale
  - no scale_cuda, yadif_cuda, or overlay_cuda filters

Environment:
  NVENC_PROOF_FFMPEG_DIR=/path/to/ffmpeg-package/ffmpeg
  NVENC_PROOF_CALIB_DIR="${CALIB_DIR}"
  NVENC_PROOF_CLIP_SECONDS=${CLIP_SECONDS}
  NVENC_PROOF_CAPTURE_SECONDS=${CAPTURE_SECONDS}
  NVENC_PROOF_SIZE=${CALIB_SIZE}
  NVENC_PROOF_FPS=${CALIB_FPS}
  NVENC_PROOF_KEEP=1 keeps the throwaway DB, logs, and capture.
EOF
}

main() {
  if [ "${1:-}" = "-h" ] || [ "${1:-}" = "--help" ]; then
    usage
    exit 0
  fi
  need cargo; need sqlite3; need curl; need jq
  setup_ffmpeg_env
  require_capabilities
  ensure_calibration

  [ "$(http_status "$BASE_URL/healthz" 3)" = 000 ] \
    || die "$LISTEN_ADDR already serving; stop it or set NVENC_PROOF_LISTEN_ADDR"

  WORKDIR="$(mktemp -d "${TMPDIR:-/tmp}/ersatzrs-nvenc-proof.XXXXXX")"
  trap cleanup EXIT
  local db="$WORKDIR/nvenc-proof.db" log="$WORKDIR/ersatzrs-web.log"

  info "building ersatzrs-web"
  (cd "$REPO_ROOT" && cargo build --bin ersatzrs-web)
  local bin
  bin="$(resolve_target_dir)/debug/ersatzrs-web"
  [ -x "$bin" ] || die "server binary not found at $bin"

  info "booting server (db=$db)"
  DATABASE_URL="sqlite:$(host_path "$db")" \
  HOME="$(host_path "$WORKDIR")" TEMP="$(host_path "$WORKDIR")" TMP="$(host_path "$WORKDIR")" TMPDIR="$(host_path "$WORKDIR")" \
  RUST_LOG="${RUST_LOG:-info,ersatzrs_infra::playback=debug,ersatzrs_web::transport::iptv=debug}" \
    "$bin" >"$log" 2>&1 &
  SERVER_PID=$!
  wait_http "$BASE_URL/healthz" 90 || die "server /healthz never returned 200; log: $log"
  pass "server up"

  local source capture
  source="$CALIB_DIR/cal_1.mkv"
  capture="$WORKDIR/live_nvenc.ts"
  [ -f "$source" ] || die "missing calibration source: $source"

  seed_db "$db" "$source"
  capture_live_stream "$capture"
  probe_capture "$capture"

  if grep -qE 'No such filter|scale_cuda|yadif_cuda|overlay_cuda|ffmpeg .*exited unsuccessfully|failed to spawn ffmpeg|Impossible to convert between the formats|Error submitting packet|Decode error rate|no active playout item|no playback ffmpeg profile' "$log"; then
    die "server log contains CUDA-filter or playback failure signatures; log: $log"
  fi
  pass "server log contains no CUDA-filter or playback failure signatures"
  info "NVENC encoder-only filter-gate proof passed"
}

main "$@"
