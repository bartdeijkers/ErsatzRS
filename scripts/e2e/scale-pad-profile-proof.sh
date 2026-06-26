#!/usr/bin/env bash
# Live Scale and Pad FFmpeg-profile proof for ErsatzRS (E2E, NOT part of CI).
#
# The proof generates a synthetic 1920x800 scope-ratio calibration clip, boots
# ersatzrs-web on a throwaway SQLite database, seeds that clip as channel
# content, forces a software H.264 1920x1080 Scale and Pad profile, captures the
# live MPEG-TS stream, then samples decoded pixels to prove the output is
# 1920x1080 with black top/bottom padding and visible source content.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

LISTEN_ADDR="${SCALE_PAD_PROOF_LISTEN_ADDR:-127.0.0.1:8409}"
BASE_URL="http://${LISTEN_ADDR}"
CHANNEL="${SCALE_PAD_PROOF_CHANNEL:-1}"
WORKDIR=""
SERVER_PID=""
KEEP="${SCALE_PAD_PROOF_KEEP:-0}"

CLIP_SECONDS="${SCALE_PAD_PROOF_CLIP_SECONDS:-8}"
CAPTURE_SECONDS="${SCALE_PAD_PROOF_CAPTURE_SECONDS:-5}"
CALIB_DIR="${SCALE_PAD_PROOF_CALIB_DIR:-${TMPDIR:-/tmp}/ersatzrs-scale-pad-proof-clips}"
CALIB_FPS="${SCALE_PAD_PROOF_FPS:-30}"
CALIB_SIZE_PRESET="${SCALE_PAD_PROOF_SIZE_PRESET:-cinema-1920x800}"
CALIB_SIZE="${SCALE_PAD_PROOF_SIZE:-}"
TARGET_WIDTH="${SCALE_PAD_PROOF_TARGET_WIDTH:-1920}"
TARGET_HEIGHT="${SCALE_PAD_PROOF_TARGET_HEIGHT:-1080}"
FFMPEG_DIR="${SCALE_PAD_PROOF_FFMPEG_DIR:-}"

FFMPEG_BIN=""
FFPROBE_BIN=""

die() { printf 'scale-pad-proof: %s\n' "$*" >&2; exit 2; }
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
      die "SCALE_PAD_PROOF_FFMPEG_DIR must contain ffmpeg/ffprobe or bin/ffmpeg/bin/ffprobe"
    fi
  else
    FFMPEG_BIN="$(command -v ffmpeg || true)"
    FFPROBE_BIN="$(command -v ffprobe || true)"
  fi

  [ -n "$FFMPEG_BIN" ] || die "ffmpeg not found; set SCALE_PAD_PROOF_FFMPEG_DIR or PATH"
  [ -x "$FFMPEG_BIN" ] || die "ffmpeg is not executable: $FFMPEG_BIN"
  [ -n "$FFPROBE_BIN" ] || die "ffprobe not found beside ffmpeg"
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
  mkdir -p "$CALIB_DIR"
  if [ ! -f "$CALIB_DIR/cal_1.mkv" ]; then
    info "generating cinema-1920x800 SDR calibration clip in $CALIB_DIR"
    if [ -n "$CALIB_SIZE" ]; then
      CAL_OUT="$CALIB_DIR" \
      CAL_CLIPS=1 \
      CAL_DURATION="$CLIP_SECONDS" \
      CAL_FPS="$CALIB_FPS" \
      CAL_SIZE="$CALIB_SIZE" \
      CAL_COLOR_MODES=sdr \
      CAL_MAKE_TS=0 \
        "$REPO_ROOT/scripts/calibration/gen-calibration-clips.sh"
    else
      CAL_OUT="$CALIB_DIR" \
      CAL_CLIPS=1 \
      CAL_DURATION="$CLIP_SECONDS" \
      CAL_FPS="$CALIB_FPS" \
      CAL_SIZE_PRESET="$CALIB_SIZE_PRESET" \
      CAL_COLOR_MODES=sdr \
      CAL_MAKE_TS=0 \
        "$REPO_ROOT/scripts/calibration/gen-calibration-clips.sh"
    fi
  fi

  [ -f "$CALIB_DIR/cal_1.mkv" ] || die "missing generated calibration source"
}

probe_dimension() {
  local file="$1" key="$2"
  "$FFPROBE_BIN" -v error -select_streams v:0 -show_entries "stream=${key}" \
    -of "default=noprint_wrappers=1:nokey=1" "$file" | head -1
}

assert_generated_source_geometry() {
  local source="$1" width height sar dar
  width="$(probe_dimension "$source" width)"
  height="$(probe_dimension "$source" height)"
  sar="$(probe_dimension "$source" sample_aspect_ratio)"
  dar="$(probe_dimension "$source" display_aspect_ratio)"
  [ "$width" = 1920 ] || die "generated source width is $width, expected 1920"
  [ "$height" = 800 ] || die "generated source height is $height, expected 800"
  [ "$sar" = "1:1" ] || die "generated source SAR is $sar, expected 1:1"
  [ "$dar" = "12:5" ] || die "generated source DAR is $dar, expected 12:5"
  pass "generated source matches cinema-1920x800 geometry: ${width}x${height} SAR $sar DAR $dar"
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
INSERT OR IGNORE INTO "Resolution" ("Id","Name","IsCustom","Width","Height")
  VALUES (9901,'Scale Pad Proof 1080p',1,$TARGET_WIDTH,$TARGET_HEIGHT);
INSERT INTO "MediaSource" ("Id") VALUES (1);
INSERT INTO "LocalMediaSource" ("Id") VALUES (1);
INSERT INTO "Library" ("Id","Name","MediaKind","MediaSourceId") VALUES (1,'Scale Pad Proof Movies',1,1);
INSERT INTO "LibraryPath" ("Id","Path","LibraryId","LastScan") VALUES (1,'$calib_db_path','1',datetime('now'));
INSERT INTO "MediaItem" ("Id","LibraryPathId","State") VALUES (1,1,0);
INSERT INTO "Movie" ("Id") VALUES (1);
INSERT INTO "MovieMetadata" ("DateAdded","DateUpdated","MetadataKind","MovieId","Title")
  VALUES (datetime('now'),datetime('now'),0,1,'Cinema Scale Pad Calibration');
INSERT INTO "MediaVersion" (
    "Id","DateAdded","DateUpdated","DisplayAspectRatio","Duration","Height",
    "MovieId","Name","RFrameRate","SampleAspectRatio","VideoScanKind","Width"
  ) VALUES (
    1,datetime('now'),datetime('now'),'12:5','$item_seconds',800,
    1,'Cinema Scale Pad Calibration','$CALIB_FPS/1','1:1',1,1920
  );
INSERT INTO "MediaFile" ("Id","MediaVersionId","Path","PathHash")
  VALUES (1,1,'$src_db_path','scale-pad-proof');
INSERT INTO "MediaStream" (
    "Index","Codec","Profile","MediaStreamKind","Channels","Default","Forced",
    "MediaVersionId","BitsPerRawSample","PixelFormat","ColorPrimaries","ColorRange",
    "ColorSpace","ColorTransfer"
  ) VALUES (
    0,'h264','High',1,0,1,0,1,8,'yuv420p','bt709','tv','bt709','bt709'
  );
INSERT INTO "MediaStream" ("Index","Codec","MediaStreamKind","Language","Channels","Default","Forced","MediaVersionId")
  VALUES (1,'aac',2,'eng',2,1,0,1);
INSERT INTO "Playout" ("Id","ChannelId","ScheduleKind","Seed","OnDemandCheckpoint")
  SELECT 1,"Id",0,1,datetime('now','-1 second') FROM "Channel" WHERE "Number"='$CHANNEL' ORDER BY "Id" LIMIT 1;
INSERT INTO "PlayoutItem" (
    "Id","DisableWatermarks","FillerKind","Finish","GuideGroup","InPoint",
    "MediaItemId","OutPoint","PlayoutId","Start"
  ) VALUES (
    1,0,0,datetime('now','+$CLIP_SECONDS seconds'),0,'00:00:00',
    1,'$item_seconds',1,datetime('now','-1 second')
  );
UPDATE "FFmpegProfile"
   SET "HardwareAcceleration" = 0,
       "VideoFormat" = 1,
       "VideoProfile" = 'high',
       "VideoPreset" = 'veryfast',
       "BitDepth" = 0,
       "VideoBitrate" = 8000,
       "VideoBufferSize" = 16000,
       "AudioFormat" = 1,
       "AudioBitrate" = 192,
       "AudioBufferSize" = 384,
       "AudioChannels" = 2,
       "AudioSampleRate" = 48,
       "NormalizeAudio" = 1,
       "NormalizeColors" = 1,
       "NormalizeVideo" = 1,
       "NormalizeFramerate" = 0,
       "ScalingBehavior" = 0,
       "PadMode" = 1,
       "ResolutionId" = 9901,
       "DeinterlaceVideo" = 0
 WHERE "Id" = (SELECT "FFmpegProfileId" FROM "Channel" WHERE "Number"='$CHANNEL' ORDER BY "Id" LIMIT 1);
UPDATE "Channel"
   SET "StreamingMode"=1,"IdleBehavior"=0,"PlayoutMode"=0,"TranscodeMode"=0,"IsEnabled"=1
 WHERE "Number"='$CHANNEL';
COMMIT;
SQL
  pass "seeded channel $CHANNEL with 1920x800 source and 1920x1080 Scale and Pad profile"
}

capture_live_stream() {
  local out="$1" url="$BASE_URL/iptv/channel/$CHANNEL.ts?mode=ts-legacy"
  info "capturing ${CAPTURE_SECONDS}s live Scale and Pad output from $url"
  "$FFMPEG_BIN" -hide_banner -loglevel error -y -t "$CAPTURE_SECONDS" -i "$url" \
    -map 0:v:0 -map 0:a? -c copy "$out"
  [ -s "$out" ] || die "live capture is empty"
}

sample_rgb_sum() {
  local file="$1" x="$2" y="$3" bytes sum
  bytes="$("$FFMPEG_BIN" -hide_banner -loglevel error -i "$file" -an -sn -dn -frames:v 1 \
    -vf "format=rgb24,crop=1:1:${x}:${y}" -f rawvideo - 2>/dev/null |
    od -An -tu1 -N3)"
  sum="$(awk 'NF >= 3 { print $1 + $2 + $3; found=1 } END { if (!found) exit 1 }' <<<"$bytes")" \
    || die "failed to sample decoded pixel at ${x},${y} from $file"
  printf '%s\n' "$sum"
}

verify_scale_pad_pixels() {
  local file="$1" width height src_width src_height scaled_width scaled_height pad_top pad_bottom
  width="$(probe_dimension "$file" width)"
  height="$(probe_dimension "$file" height)"
  [ "$width" = "$TARGET_WIDTH" ] || die "live output width is $width, expected $TARGET_WIDTH"
  [ "$height" = "$TARGET_HEIGHT" ] || die "live output height is $height, expected $TARGET_HEIGHT"

  src_width=1920
  src_height=800
  if [ $((TARGET_WIDTH * src_height)) -le $((TARGET_HEIGHT * src_width)) ]; then
    scaled_width="$TARGET_WIDTH"
    scaled_height=$((TARGET_WIDTH * src_height / src_width))
  else
    scaled_height="$TARGET_HEIGHT"
    scaled_width=$((TARGET_HEIGHT * src_width / src_height))
  fi
  scaled_width=$((scaled_width - scaled_width % 2))
  scaled_height=$((scaled_height - scaled_height % 2))
  pad_top=$(((TARGET_HEIGHT - scaled_height) / 2))
  pad_bottom=$((TARGET_HEIGHT - pad_top - 1))

  local top_sum bottom_sum content_sum
  top_sum="$(sample_rgb_sum "$file" $((TARGET_WIDTH / 2)) 20)"
  bottom_sum="$(sample_rgb_sum "$file" $((TARGET_WIDTH / 2)) $((TARGET_HEIGHT - 20)))"
  content_sum="$(sample_rgb_sum "$file" 100 $((pad_top + scaled_height / 2)))"

  [ "$top_sum" -le 48 ] || die "top pad is not black enough; RGB sum=$top_sum"
  [ "$bottom_sum" -le 48 ] || die "bottom pad is not black enough; RGB sum=$bottom_sum"
  [ "$content_sum" -gt 60 ] || die "content sample is too dark; Scale and Pad may have hidden content; RGB sum=$content_sum"

  pass "live output is ${width}x${height}; black pad rows around y=0..$((pad_top - 1)) and y=$((pad_bottom + 1))..$((TARGET_HEIGHT - 1)); content remains visible"
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
Usage: scripts/e2e/scale-pad-profile-proof.sh

Generates a cinema-1920x800 calibration source, boots ErsatzRS on a throwaway DB,
and proves the 1920x1080 Scale and Pad profile emits letterboxed output.

Environment:
  SCALE_PAD_PROOF_SIZE_PRESET=cinema-1920x800  named generator size preset (default)
  SCALE_PAD_PROOF_SIZE=1920x800      explicit generator size override
  SCALE_PAD_PROOF_CLIP_SECONDS=8
  SCALE_PAD_PROOF_CAPTURE_SECONDS=5
  SCALE_PAD_PROOF_CALIB_DIR="${CALIB_DIR}"
  SCALE_PAD_PROOF_KEEP=1             keep throwaway DB, logs, and capture
EOF
}

main() {
  if [ "${1:-}" = "-h" ] || [ "${1:-}" = "--help" ]; then
    usage
    exit 0
  fi
  need cargo; need sqlite3; need curl; need awk; need od
  setup_ffmpeg_env
  ensure_calibration

  [ "$(http_status "$BASE_URL/healthz" 3)" = 000 ] \
    || die "$LISTEN_ADDR already serving; stop it or set SCALE_PAD_PROOF_LISTEN_ADDR"

  WORKDIR="$(mktemp -d "${TMPDIR:-/tmp}/ersatzrs-scale-pad-proof.XXXXXX")"
  trap cleanup EXIT
  local db="$WORKDIR/scale-pad-proof.db" log="$WORKDIR/ersatzrs-web.log"
  local source="$CALIB_DIR/cal_1.mkv" capture="$WORKDIR/live_scale_pad.ts"

  assert_generated_source_geometry "$source"

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

  seed_db "$db" "$source"
  capture_live_stream "$capture"
  verify_scale_pad_pixels "$capture"

  if grep -qE 'ffmpeg .*exited unsuccessfully|failed to spawn ffmpeg|Impossible to convert between the formats|Error submitting packet|Decode error rate|no active playout item|no playback ffmpeg profile' "$log"; then
    die "server log contains playback failure signatures; log: $log"
  fi
  pass "server log contains no playback failure signatures"
  info "Scale and Pad profile proof passed"
}

main "$@"
