#!/usr/bin/env bash
# Generate three playout-calibration video clips for ErsatzRS streaming tests.
#
# Each clip is a solid, per-clip background colour with:
#   * a big "1" / "2" / "3" top-centre        -> playback ORDER
#   * a millisecond ABS timecode (0:00:00.000 -> 0:09:00.000 across the trio,
#     black box / white text) plus a per-CLIP elapsed timer that resets
#   * absolute + per-clip frame counters and a 0..fps-1 "f/N" sub-second index
#     -> any skipped value == a dropped/duplicated frame
#   * a cyan bar sweeping one screen-width per second   -> visible stutter/freeze
#   * a full-frame edge border (overscan/crop) + a centred perfect square
#     (pixel-aspect / scaling distortion)   -> transcode geometry calibration
#   * a faint grid (scaling/block artefacts)
#   * audio: a 50 ms pip on every whole second, distinct pitch per clip
#     (1000 / 1500 / 2000 Hz)   -> A/V sync + audible skips + a 2nd TS PID
#
# Outputs, per clip N:
#   <out>/cal_N.mkv   high-bitrate H.264 source for ErsatzRS library ingest
#   <out>/cal_N.ts    clean MPEG-TS mux for direct TSDuck analysis
#
# Requires FFmpeg 8.1 (drawtext/drawbox/drawgrid; libfreetype). The local
# ~/.local/ffmpeg-8.1 build fails silently without its lib dir on
# LD_LIBRARY_PATH, so this script derives and exports it from the ffmpeg
# location automatically.
#
# Tunables (env):
#   CAL_OUT=./calibration-clips   output directory
#   CAL_DURATION=180              seconds per clip
#   CAL_FPS=60                    frame rate
#   CAL_SIZE=1920x1080            WxH
#   CAL_CRF=14                    libx264 quality (lower = better/larger)
#   CAL_PRESET=medium             libx264 preset
#   CAL_CLIPS="1 2 3"             which clips to build
#   CAL_MAKE_TS=1                 also emit the .ts mux (0 to skip)
#   CAL_ANALYZE=0                 run `tsanalyze` on each .ts after muxing

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TMPL="$SCRIPT_DIR/calibration.fg.tmpl"

OUT="${CAL_OUT:-./calibration-clips}"
DUR="${CAL_DURATION:-180}"
FPS="${CAL_FPS:-60}"
SIZE="${CAL_SIZE:-1920x1080}"
CRF="${CAL_CRF:-14}"
PRESET="${CAL_PRESET:-medium}"
CLIPS="${CAL_CLIPS:-1 2 3}"
MAKE_TS="${CAL_MAKE_TS:-1}"
ANALYZE="${CAL_ANALYZE:-0}"

# --- FFmpeg + its private lib dir (silent-fail trap) -------------------------
FFMPEG_BIN="$(command -v ffmpeg || true)"
[ -n "$FFMPEG_BIN" ] || { echo "ERROR: ffmpeg not found on PATH" >&2; exit 1; }
LIBDIR="$(cd "$(dirname "$FFMPEG_BIN")/../lib" 2>/dev/null && pwd || true)"
[ -n "$LIBDIR" ] && export LD_LIBRARY_PATH="$LIBDIR:${LD_LIBRARY_PATH:-}"

# --- fonts ------------------------------------------------------------------
pick_font() { for f in "$@"; do [ -f "$f" ] && { echo "$f"; return; }; done; echo "$1"; }
FMONO="$(pick_font \
  /usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf \
  /usr/share/fonts/truetype/liberation/LiberationMono-Regular.ttf)"
FBOLD="$(pick_font \
  /usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf \
  /usr/share/fonts/truetype/liberation/LiberationSans-Bold.ttf)"

# --- per-clip parameters ----------------------------------------------------
#            clip:   1         2         3
BG=(        ""      "0x12305A" "0x1C4A2A" "0x5A1A28")  # navy / forest / maroon
BGNAME=(    ""      "navy"     "forest"   "maroon")
PITCH=(     ""      "1000"     "1500"     "2000")      # Hz

mkdir -p "$OUT"
WH="${SIZE/x/x}"   # already WxH; used only for the on-screen label

echo "==> ffmpeg: $FFMPEG_BIN"
echo "==> lib:    ${LIBDIR:-<system>}"
echo "==> out:    $OUT   ${SIZE}@${FPS}  ${DUR}s/clip  crf=${CRF} preset=${PRESET}"

for N in $CLIPS; do
  bg="${BG[$N]}"; bgname="${BGNAME[$N]}"; pitch="${PITCH[$N]}"
  offsec=$(( (N - 1) * DUR ))
  offfr=$(( (N - 1) * DUR * FPS ))
  fg="$OUT/.cal_${N}.fg"
  mkv="$OUT/cal_${N}.mkv"
  ts="$OUT/cal_${N}.ts"

  # Render template -> one-line filtergraph (the parser needs a single line).
  sed -e "s|__FMONO__|$FMONO|g" \
      -e "s|__FBOLD__|$FBOLD|g" \
      -e "s|__NUM__|$N|g" \
      -e "s|__OFFSEC__|$offsec|g" \
      -e "s|__OFFFR__|$offfr|g" \
      -e "s|__BGNAME__|$bgname|g" \
      -e "s|__PITCH__|$pitch|g" \
      -e "s|__FPS__|$FPS|g" \
      -e "s|__CRF__|$CRF|g" \
      -e "s|__WxH__|$WH|g" \
      "$TMPL" | tr -d '\n' > "$fg"

  echo "==> [$N/3] encoding $mkv (bg=$bgname pitch=${pitch}Hz)"
  "$FFMPEG_BIN" -hide_banner -y \
    -f lavfi -i "color=c=${bg}:s=${SIZE}:r=${FPS}:d=${DUR}" \
    -f lavfi -i "aevalsrc=exprs='0.6*sin(2*PI*${pitch}*t)*lt(mod(t,1),0.05)':s=48000:d=${DUR}" \
    -/filter_complex "$fg" \
    -map "[v]" -map 1:a \
    -c:v libx264 -preset "$PRESET" -crf "$CRF" -pix_fmt yuv420p \
    -g "$FPS" -keyint_min "$FPS" -x264-params "scenecut=0:open_gop=0" \
    -color_primaries bt709 -color_trc bt709 -colorspace bt709 \
    -c:a aac -b:a 192k -ac 2 -ar 48000 \
    "$mkv"

  if [ "$MAKE_TS" = "1" ]; then
    echo "==> [$N/3] muxing  $ts"
    "$FFMPEG_BIN" -hide_banner -y -i "$mkv" \
      -c copy -f mpegts -mpegts_flags +resend_headers -pcr_period 20 "$ts"
    if [ "$ANALYZE" = "1" ] && command -v tsanalyze >/dev/null 2>&1; then
      echo "--- tsanalyze $ts (summary) ---"
      tsanalyze "$ts" 2>/dev/null | sed -n '1,40p' || true
    fi
  fi
done

cat <<EOF

==> done -> $OUT
    cal_1.mkv/.ts  navy   1000 Hz   ABS 0:00:00 -> 0:03:00
    cal_2.mkv/.ts  forest 1500 Hz   ABS 0:03:00 -> 0:06:00
    cal_3.mkv/.ts  maroon 2000 Hz   ABS 0:06:00 -> 0:09:00

TSDuck quick checks (set LD path same as this script if you use ffprobe):
  tsanalyze cal_1.ts                                   # PIDs, bitrate, PCR, errors
  tsp -I file cal_1.ts -P continuity -O drop           # continuity-counter / packet-loss errors
  tsp -I file cal_1.ts -P pcrverify --jitter-max 5000000 -O drop   # PCR jitter (ns)
  tsp -I file cal_1.ts -P analyze -O drop              # rolling analysis report
For the LIVE ErsatzRS stream (replace URL with your channel's MPEG-TS):
  tsp -I http http://127.0.0.1:8409/iptv/channel/1.ts -P continuity -P pcrverify -O drop
EOF
