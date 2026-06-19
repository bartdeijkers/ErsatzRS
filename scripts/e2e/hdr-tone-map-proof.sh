#!/usr/bin/env bash
# Live HDR -> SDR tone-map proof for ErsatzRS (E2E, NOT part of CI).
#
# This gate boots ersatzrs-web on a throwaway SQLite database, seeds HDR
# calibration clips as the active channel content, captures the live MPEG-TS
# stream, and compares the decoded static HDR helper chips against the generated
# SDR reference render. TSDuck proves transport integrity; this script proves the
# decoded tone-map content path.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

LISTEN_ADDR="${HDR_PROOF_LISTEN_ADDR:-127.0.0.1:8409}"
BASE_URL="http://${LISTEN_ADDR}"
CHANNEL="${HDR_PROOF_CHANNEL:-1}"
WORKDIR=""
SERVER_PID=""
KEEP="${HDR_PROOF_KEEP:-0}"

HDR_MODES="${HDR_PROOF_MODES:-hdr10-pq hlg}"
HDR_ALGORITHM="${HDR_PROOF_TONEMAP_ALGORITHM:-hable}"
CLIP_SECONDS="${HDR_PROOF_CLIP_SECONDS:-8}"
CAPTURE_SECONDS="${HDR_PROOF_CAPTURE_SECONDS:-5}"
CALIB_DIR="${HDR_PROOF_CALIB_DIR:-${TMPDIR:-/tmp}/ersatzrs-hdr-proof-clips}"
CALIB_SIZE="${HDR_PROOF_SIZE:-1920x1080}"
CALIB_FPS="${HDR_PROOF_FPS:-30}"
MIN_PSNR="${HDR_PROOF_MIN_PSNR:-24}"
MIN_SSIM="${HDR_PROOF_MIN_SSIM:-0.90}"

die() { printf 'hdr-proof: %s\n' "$*" >&2; exit 2; }
info() { printf '[ info ] %s\n' "$*"; }
pass() { printf '[ PASS ] %s\n' "$*"; }
need() { command -v "$1" >/dev/null 2>&1 || die "missing required tool: $1"; }

setup_ffmpeg_env() {
  FFMPEG_BIN="$(command -v ffmpeg || true)"
  FFPROBE_BIN="$(command -v ffprobe || true)"
  [ -n "$FFMPEG_BIN" ] || die "ffmpeg not found on PATH"
  [ -n "$FFPROBE_BIN" ] || die "ffprobe not found on PATH"
  local libdir
  libdir="$(cd "$(dirname "$FFMPEG_BIN")/../lib" 2>/dev/null && pwd || true)"
  [ -n "$libdir" ] && export LD_LIBRARY_PATH="$libdir:${LD_LIBRARY_PATH:-}"
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

mode_slug() {
  local value="$1"
  value="${value//[^A-Za-z0-9_-]/-}"
  printf '%s\n' "$value"
}

mode_transfer() {
  case "$1" in
    hdr10-pq) printf '%s\n' smpte2084 ;;
    hlg) printf '%s\n' arib-std-b67 ;;
    *) die "unsupported HDR mode: $1" ;;
  esac
}

ensure_calibration() {
  local mode slug missing=0
  for mode in $HDR_MODES; do
    slug="$(mode_slug "$mode")"
    [ -f "$CALIB_DIR/cal_${slug}_1.mkv" ] || missing=1
    [ -f "$CALIB_DIR/cal_${slug}_to_sdr_${HDR_ALGORITHM}_1.mkv" ] || missing=1
  done
  [ "$missing" = 0 ] && return 0

  info "generating HDR calibration clips in $CALIB_DIR"
  CAL_OUT="$CALIB_DIR" \
  CAL_CLIPS=1 \
  CAL_DURATION="$CLIP_SECONDS" \
  CAL_FPS="$CALIB_FPS" \
  CAL_SIZE="$CALIB_SIZE" \
  CAL_HDR_MODES="$HDR_MODES" \
  CAL_MAKE_TS=0 \
  CAL_MAKE_SDR_REFS=1 \
  CAL_TONEMAP_ALGORITHMS="$HDR_ALGORITHM" \
    "$REPO_ROOT/scripts/calibration/gen-calibration-clips.sh"
}

probe_hdr_source() {
  local file="$1" mode="$2" transfer
  transfer="$(mode_transfer "$mode")"
  local json="$WORKDIR/source_$(mode_slug "$mode").json"
  ffprobe -v error -select_streams v:0 \
    -show_entries stream=codec_name,pix_fmt,color_primaries,color_transfer,color_space \
    -of json "$file" >"$json"

  jq -e --arg transfer "$transfer" '
    .streams[0].pix_fmt == "yuv420p10le"
    and .streams[0].color_primaries == "bt2020"
    and .streams[0].color_transfer == $transfer
    and .streams[0].color_space == "bt2020nc"
  ' "$json" >/dev/null
  pass "$mode source signaling is 10-bit BT.2020 ${transfer}"
}

probe_sdr_capture() {
  local file="$1" mode="$2" json="$WORKDIR/capture_$(mode_slug "$mode").json"
  ffprobe -v error -select_streams v:0 \
    -show_entries stream=pix_fmt,color_primaries,color_transfer,color_space \
    -of json "$file" >"$json"

  jq -e '
    .streams[0].pix_fmt == "yuv420p"
    and .streams[0].color_primaries == "bt709"
    and .streams[0].color_transfer == "bt709"
    and .streams[0].color_space == "bt709"
  ' "$json" >/dev/null
  pass "$mode live capture signaling is 8-bit BT.709 SDR"
}

seed_db() {
  local db="$1" mode="$2" source="$3" now
  local transfer src_db_path calib_db_path item_seconds
  transfer="$(mode_transfer "$mode")"
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
INSERT INTO "Library" ("Id","Name","MediaKind","MediaSourceId") VALUES (1,'HDR Calibration Movies',1,1);
INSERT INTO "LibraryPath" ("Id","Path","LibraryId","LastScan") VALUES (1,'$calib_db_path','1',datetime('now'));
INSERT INTO "MediaItem" ("Id","LibraryPathId","State") VALUES (1,1,0);
INSERT INTO "Movie" ("Id") VALUES (1);
INSERT INTO "MovieMetadata" ("DateAdded","DateUpdated","MetadataKind","MovieId","Title") VALUES (datetime('now'),datetime('now'),0,1,'HDR Calibration $mode');
INSERT INTO "MediaVersion" ("Id","DateAdded","DateUpdated","Duration","Height","MovieId","Name","RFrameRate","VideoScanKind","Width")
  VALUES (1,datetime('now'),datetime('now'),'$item_seconds',1080,1,'HDR Calibration $mode','$CALIB_FPS/1',1,1920);
INSERT INTO "MediaFile" ("Id","MediaVersionId","Path","PathHash") VALUES (1,1,'$src_db_path','hdr-proof-$mode');
INSERT INTO "MediaStream" ("Index","Codec","Profile","MediaStreamKind","Channels","Default","Forced","MediaVersionId","BitsPerRawSample","PixelFormat","ColorPrimaries","ColorRange","ColorSpace","ColorTransfer")
  VALUES (0,'hevc','Main 10',1,0,1,0,1,10,'yuv420p10le','bt2020','tv','bt2020nc','$transfer');
INSERT INTO "MediaStream" ("Index","Codec","MediaStreamKind","Language","Channels","Default","Forced","MediaVersionId")
  VALUES (1,'aac',2,'eng',2,1,0,1);
INSERT INTO "Playout" ("Id","ChannelId","ScheduleKind","Seed","OnDemandCheckpoint")
  SELECT 1,"Id",0,1,datetime('now','-1 second') FROM "Channel" WHERE "Number"='$CHANNEL' ORDER BY "Id" LIMIT 1;
INSERT INTO "PlayoutItem" ("Id","DisableWatermarks","FillerKind","Finish","GuideGroup","InPoint","MediaItemId","OutPoint","PlayoutId","Start")
  VALUES (1,0,0,datetime('now','+$CLIP_SECONDS seconds'),0,'00:00:00',1,'$item_seconds',1,datetime('now','-1 second'));
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
       "NormalizeColors" = 1,
       "NormalizeVideo" = 1,
       "NormalizeFramerate" = 0,
       "DeinterlaceVideo" = 0,
       "TonemapAlgorithm" = 5
 WHERE "Id" = (SELECT "FFmpegProfileId" FROM "Channel" WHERE "Number"='$CHANNEL' ORDER BY "Id" LIMIT 1);
UPDATE "Channel" SET "StreamingMode"=5,"IdleBehavior"=0,"PlayoutMode"=0,"TranscodeMode"=0,"IsEnabled"=1 WHERE "Number"='$CHANNEL';
COMMIT;
SQL
  pass "$mode seeded as channel $CHANNEL HDR source with SDR H.264 target"
}

capture_live_stream() {
  local mode="$1" out="$2" url="$BASE_URL/iptv/channel/$CHANNEL.ts?mode=ts"
  info "capturing ${CAPTURE_SECONDS}s live $mode output from $url"
  ffmpeg -hide_banner -loglevel error -y -t "$CAPTURE_SECONDS" -i "$url" \
    -map 0:v:0 -map 0:a? -c copy "$out"
  [ -s "$out" ] || die "$mode live capture is empty"
}

compare_static_chips() {
  local mode="$1" live="$2" reference="$3" slug stats_psnr stats_ssim log psnr ssim
  slug="$(mode_slug "$mode")"
  stats_psnr="$WORKDIR/${slug}_psnr.log"
  stats_ssim="$WORKDIR/${slug}_ssim.log"
  log="$WORKDIR/${slug}_compare.log"

  # The calibration chip panel is static. Crop only the peak-to-black and
  # RGBYCM chips so timer/sweep offsets in the live stream do not affect proof.
  ffmpeg -hide_banner -y -i "$live" -i "$reference" \
    -filter_complex "[0:v]trim=start=1:duration=1,crop=304:118:iw-338:168,format=yuv420p,setpts=PTS-STARTPTS[l];[1:v]trim=start=1:duration=1,crop=304:118:iw-338:168,format=yuv420p,setpts=PTS-STARTPTS[r];[l]split=2[l1][l2];[r]split=2[r1][r2];[l1][r1]ssim=stats_file=${stats_ssim};[l2][r2]psnr=stats_file=${stats_psnr}" \
    -frames:v 1 -f null - >"$log" 2>&1

  psnr="$(awk -F'psnr_avg:' 'NF > 1 { split($2,a," "); value=a[1] } END { print value }' "$stats_psnr")"
  ssim="$(awk -F'All:' 'NF > 1 { split($2,a," "); value=a[1] } END { print value }' "$stats_ssim")"
  [ -n "$psnr" ] || die "$mode PSNR metric missing; see $log"
  [ -n "$ssim" ] || die "$mode SSIM metric missing; see $log"

  awk -v value="$psnr" -v min="$MIN_PSNR" 'BEGIN { exit !(value + 0 >= min + 0) }' \
    || die "$mode chip PSNR $psnr is below $MIN_PSNR; see $log"
  awk -v value="$ssim" -v min="$MIN_SSIM" 'BEGIN { exit !(value + 0 >= min + 0) }' \
    || die "$mode chip SSIM $ssim is below $MIN_SSIM; see $log"
  pass "$mode live chips match SDR reference (psnr=$psnr, ssim=$ssim)"
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
Live HDR -> SDR tone-map proof for ErsatzRS (E2E, NOT part of CI).

Boots ersatzrs-web on a throwaway SQLite database, seeds HDR calibration clips
as the active channel content, captures the live MPEG-TS stream, and compares
decoded static HDR helper chips against the generated SDR reference render.

Environment:
  HDR_PROOF_MODES="${HDR_MODES}"
  HDR_PROOF_CALIB_DIR="${CALIB_DIR}"
  HDR_PROOF_CLIP_SECONDS=${CLIP_SECONDS}
  HDR_PROOF_CAPTURE_SECONDS=${CAPTURE_SECONDS}
  HDR_PROOF_SIZE=${CALIB_SIZE}
  HDR_PROOF_FPS=${CALIB_FPS}
  HDR_PROOF_MIN_PSNR=${MIN_PSNR}
  HDR_PROOF_MIN_SSIM=${MIN_SSIM}
  HDR_PROOF_KEEP=1 keeps the throwaway DB, logs, captures, and metric files.
EOF
}

main() {
  if [ "${1:-}" = "-h" ] || [ "${1:-}" = "--help" ]; then
    usage
    exit 0
  fi
  need cargo; need sqlite3; need curl; need jq; need awk
  setup_ffmpeg_env
  ensure_calibration

  [ "$(http_status "$BASE_URL/healthz" 3)" = 000 ] \
    || die "$LISTEN_ADDR already serving; stop it or set HDR_PROOF_LISTEN_ADDR"

  WORKDIR="$(mktemp -d "${TMPDIR:-/tmp}/ersatzrs-hdr-proof.XXXXXX")"
  trap cleanup EXIT
  local db="$WORKDIR/hdr-proof.db" log="$WORKDIR/ersatzrs-web.log"

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

  local mode slug source reference capture
  for mode in $HDR_MODES; do
    slug="$(mode_slug "$mode")"
    source="$CALIB_DIR/cal_${slug}_1.mkv"
    reference="$CALIB_DIR/cal_${slug}_to_sdr_${HDR_ALGORITHM}_1.mkv"
    capture="$WORKDIR/live_${slug}.ts"
    [ -f "$source" ] || die "missing HDR source: $source"
    [ -f "$reference" ] || die "missing SDR reference: $reference"

    probe_hdr_source "$source" "$mode"
    seed_db "$db" "$mode" "$source"
    capture_live_stream "$mode" "$capture"
    probe_sdr_capture "$capture" "$mode"
    compare_static_chips "$mode" "$capture" "$reference"
  done

  if grep -qE 'ffmpeg .*exited unsuccessfully|failed to spawn ffmpeg|Impossible to convert between the formats|Error submitting packet|Decode error rate|no active playout item|no playback ffmpeg profile' "$log"; then
    die "server log contains known playback failure signatures; log: $log"
  fi
  pass "server log free of known playback failure signatures"
  info "HDR tone-map proof passed"
}

main "$@"
