#!/usr/bin/env bash
# Generate playout-calibration video clips for ErsatzRS streaming tests.
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
#   * peak-to-black and saturated RGBYCM chips for HDR -> SDR tone mapping
#     review. In HDR modes these are intentionally obvious stress points:
#     clipped chips, unreadable peak text, hue shifts, crushed dark grid lines,
#     or changing saturation between clips are tone-mapping regressions.
#   * audio: a 50 ms pip on every whole second, distinct pitch per clip
#     (1000 / 1500 / 2000 Hz)   -> A/V sync + audible skips + a 2nd TS PID
#
# Outputs, per clip N:
#   <out>/cal_N.mkv   high-bitrate H.264 source for ErsatzRS library ingest
#   <out>/cal_N.ts    clean MPEG-TS mux for direct TSDuck analysis
# Optional HDR modes write mode-suffixed files, for example:
#   <out>/cal_hdr10-pq_N.mkv
#   <out>/cal_hlg_N.mkv
# Optional SDR references write:
#   <out>/cal_hdr10-pq_to_sdr_hable_N.mkv
#
# Requires FFmpeg 8.1 (drawtext/drawbox/drawgrid; libfreetype). HDR SDR
# reference renders require zscale + tonemap. The local ~/.local/ffmpeg-8.1
# build fails silently without its lib dir on
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
#   CAL_COLOR_MODES=sdr           modes to build: sdr hdr10-pq hlg
#   CAL_HDR_MODES=                shorthand: keep sdr and add these HDR modes
#   CAL_HDR_ENCODER=libx265       encoder for HDR 10-bit masters
#   CAL_HDR_CRF=12                HDR encoder quality
#   CAL_HDR_PRESET=medium         HDR encoder preset
#   CAL_HDR_MAX_CLL=1000,400      HDR10 MaxCLL,MaxFALL SEI for libx265
#   CAL_HDR_MASTER_DISPLAY=...    HDR10 mastering-display SEI for libx265
#   CAL_MAKE_SDR_REFS=0           render SDR tonemap references for HDR modes
#   CAL_TONEMAP_ALGORITHMS=hable  tonemap algorithms for SDR references
#   CAL_TONEMAP_NPL=100           nominal peak luminance for zscale
#   CAL_TONEMAP_PEAK=1000         HDR signal peak override for tonemap
#   CAL_TONEMAP_DESAT=2           tonemap desaturation helper

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
if [ -n "${CAL_HDR_MODES:-}" ]; then
  COLOR_MODES="sdr ${CAL_HDR_MODES}"
else
  COLOR_MODES="${CAL_COLOR_MODES:-sdr}"
fi
HDR_ENCODER="${CAL_HDR_ENCODER:-libx265}"
HDR_CRF="${CAL_HDR_CRF:-12}"
HDR_PRESET="${CAL_HDR_PRESET:-medium}"
HDR_MAX_CLL="${CAL_HDR_MAX_CLL:-1000,400}"
HDR_MASTER_DISPLAY="${CAL_HDR_MASTER_DISPLAY:-G(13250,34500)B(7500,3000)R(34000,16000)WP(15635,16450)L(10000000,50)}"
MAKE_SDR_REFS="${CAL_MAKE_SDR_REFS:-0}"
TONEMAP_ALGORITHMS="${CAL_TONEMAP_ALGORITHMS:-hable}"
TONEMAP_NPL="${CAL_TONEMAP_NPL:-100}"
TONEMAP_PEAK="${CAL_TONEMAP_PEAK:-1000}"
TONEMAP_DESAT="${CAL_TONEMAP_DESAT:-2}"

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
echo "==> modes:  $COLOR_MODES"

mode_key() {
  case "$1" in
    sdr|bt709) echo "sdr" ;;
    hdr10|hdr10-pq|pq) echo "hdr10-pq" ;;
    hlg|bt2020-hlg) echo "hlg" ;;
    *) echo "ERROR: unsupported CAL_COLOR_MODES entry '$1' (use sdr, hdr10-pq, hlg)" >&2; exit 2 ;;
  esac
}

mode_label() {
  case "$1" in
    sdr) echo "MODE SDR BT.709" ;;
    hdr10-pq) echo "MODE HDR10 PQ BT.2020 10-bit" ;;
    hlg) echo "MODE HLG BT.2020 10-bit" ;;
  esac
}

video_note() {
  case "$1" in
    sdr) echo "SDR baseline timing crop scale chroma" ;;
    hdr10-pq) echo "Tone-map preserve timers chips dark grid no peak clipping" ;;
    hlg) echo "Tone-map HLG preserve chips hue dark grid text edges" ;;
  esac
}

footer_note() {
  case "$1" in
    sdr) echo "src CRF${CRF} yuv420p bt709" ;;
    hdr10-pq) echo "src CRF${HDR_CRF} yuv420p10le bt2020 pq maxcll ${HDR_MAX_CLL//,/+}" ;;
    hlg) echo "src CRF${HDR_CRF} yuv420p10le bt2020 hlg" ;;
  esac
}

mode_slug() {
  local value="$1"
  value="${value//[^A-Za-z0-9_-]/-}"
  printf '%s\n' "$value"
}

mode_stem() {
  local mode="$1" n="$2"
  if [ "$mode" = "sdr" ]; then
    printf 'cal_%s\n' "$n"
  else
    printf 'cal_%s_%s\n' "$(mode_slug "$mode")" "$n"
  fi
}

require_filter() {
  local filter="$1"
  "$FFMPEG_BIN" -hide_banner -filters 2>&1 | grep -w "$filter" >/dev/null || {
    echo "ERROR: ffmpeg filter '$filter' missing; HDR SDR references require zscale + tonemap" >&2
    exit 1
  }
}

require_encoder() {
  local encoder="$1"
  "$FFMPEG_BIN" -hide_banner -encoders 2>&1 | grep -w "$encoder" >/dev/null || {
    echo "ERROR: ffmpeg encoder '$encoder' missing; set CAL_HDR_ENCODER or rebuild FFmpeg" >&2
    exit 1
  }
}

needs_hdr=0
for raw_mode in $COLOR_MODES; do
  [ "$(mode_key "$raw_mode")" = "sdr" ] || needs_hdr=1
done
[ "$needs_hdr" = 0 ] || require_encoder "$HDR_ENCODER"
if [ "$MAKE_SDR_REFS" = "1" ] && [ "$needs_hdr" = 1 ]; then
  require_filter zscale
  require_filter tonemap
fi

render_filter_graph() {
  local n="$1" mode="$2" fg="$3"
  local bgname="${BGNAME[$n]}" pitch="${PITCH[$n]}"
  local offsec=$(( (n - 1) * DUR ))
  local offfr=$(( (n - 1) * DUR * FPS ))
  local pix_fmt cp trc cs
  case "$mode" in
    sdr) pix_fmt="yuv420p"; cp="bt709"; trc="bt709"; cs="bt709" ;;
    hdr10-pq) pix_fmt="yuv420p10le"; cp="bt2020"; trc="smpte2084"; cs="bt2020nc" ;;
    hlg) pix_fmt="yuv420p10le"; cp="bt2020"; trc="arib-std-b67"; cs="bt2020nc" ;;
  esac

  # Render template -> one-line filtergraph (the parser needs a single line).
  sed -e "s|__FMONO__|$FMONO|g" \
      -e "s|__FBOLD__|$FBOLD|g" \
      -e "s|__NUM__|$n|g" \
      -e "s|__OFFSEC__|$offsec|g" \
      -e "s|__OFFFR__|$offfr|g" \
      -e "s|__BGNAME__|$bgname|g" \
      -e "s|__PITCH__|$pitch|g" \
      -e "s|__FPS__|$FPS|g" \
      -e "s|__CRF__|$CRF|g" \
      -e "s|__WxH__|$WH|g" \
      -e "s|__MODE_LABEL__|$(mode_label "$mode")|g" \
      -e "s|__VIDEO_NOTE__|$(video_note "$mode")|g" \
      -e "s|__FOOTER_NOTE__|$(footer_note "$mode")|g" \
      -e "s|__PIX_FMT__|$pix_fmt|g" \
      -e "s|__COLOR_PRIMARIES__|$cp|g" \
      -e "s|__COLOR_TRC__|$trc|g" \
      -e "s|__COLORSPACE__|$cs|g" \
      "$TMPL" | tr -d '\n' > "$fg"
}

encode_video() {
  local n="$1" mode="$2" fg="$3" out="$4"
  local bg="${BG[$n]}" pitch="${PITCH[$n]}"
  local label; label="$(mode_label "$mode")"

  echo "==> [$n] encoding $out (${label#MODE } bg=${BGNAME[$n]} pitch=${pitch}Hz)"
  if [ "$mode" = "sdr" ]; then
    "$FFMPEG_BIN" -hide_banner -y \
      -f lavfi -i "color=c=${bg}:s=${SIZE}:r=${FPS}:d=${DUR}" \
      -f lavfi -i "aevalsrc=exprs='0.6*sin(2*PI*${pitch}*t)*lt(mod(t,1),0.05)':s=48000:d=${DUR}" \
      -/filter_complex "$fg" \
      -map "[v]" -map 1:a \
      -c:v libx264 -preset "$PRESET" -crf "$CRF" -pix_fmt yuv420p \
      -g "$FPS" -keyint_min "$FPS" -x264-params "scenecut=0:open_gop=0" \
      -color_primaries bt709 -color_trc bt709 -colorspace bt709 \
      -c:a aac -b:a 192k -ac 2 -ar 48000 \
      "$out"
    return
  fi

  local trc="smpte2084" x265_params=()
  [ "$mode" = "hlg" ] && trc="arib-std-b67"
  if [ "$HDR_ENCODER" = "libx265" ]; then
    if [ "$mode" = "hdr10-pq" ]; then
      x265_params=(-x265-params "repeat-headers=1:keyint=${FPS}:min-keyint=${FPS}:scenecut=0:open-gop=0:colorprim=bt2020:transfer=smpte2084:colormatrix=bt2020nc:master-display=${HDR_MASTER_DISPLAY}:max-cll=${HDR_MAX_CLL}")
    else
      x265_params=(-x265-params "repeat-headers=1:keyint=${FPS}:min-keyint=${FPS}:scenecut=0:open-gop=0:colorprim=bt2020:transfer=arib-std-b67:colormatrix=bt2020nc")
    fi
  fi

  "$FFMPEG_BIN" -hide_banner -y \
    -f lavfi -i "color=c=${bg}:s=${SIZE}:r=${FPS}:d=${DUR}" \
    -f lavfi -i "aevalsrc=exprs='0.6*sin(2*PI*${pitch}*t)*lt(mod(t,1),0.05)':s=48000:d=${DUR}" \
    -/filter_complex "$fg" \
    -map "[v]" -map 1:a \
    -c:v "$HDR_ENCODER" -preset "$HDR_PRESET" -crf "$HDR_CRF" -pix_fmt yuv420p10le \
    -g "$FPS" -keyint_min "$FPS" "${x265_params[@]}" \
    -color_primaries bt2020 -color_trc "$trc" -colorspace bt2020nc \
    -c:a aac -b:a 192k -ac 2 -ar 48000 \
    "$out"
}

mux_ts() {
  local n="$1" mkv="$2" ts="$3"
  [ "$MAKE_TS" = "1" ] || return 0
  echo "==> [$n] muxing  $ts"
  "$FFMPEG_BIN" -hide_banner -y -i "$mkv" \
    -c copy -f mpegts -mpegts_flags +resend_headers -pcr_period 20 "$ts"
  if [ "$ANALYZE" = "1" ] && command -v tsanalyze >/dev/null 2>&1; then
    echo "--- tsanalyze $ts (summary) ---"
    tsanalyze "$ts" 2>/dev/null | sed -n '1,40p' || true
  fi
}

make_sdr_refs() {
  local n="$1" mode="$2" src="$3"
  [ "$mode" != "sdr" ] || return 0
  [ "$MAKE_SDR_REFS" = "1" ] || return 0
  local slug; slug="$(mode_slug "$mode")"
  local alg ref graph
  for alg in $TONEMAP_ALGORITHMS; do
    ref="$OUT/cal_${slug}_to_sdr_${alg}_${n}.mkv"
    graph="zscale=t=linear:npl=${TONEMAP_NPL},format=gbrpf32le,tonemap=tonemap=${alg}:desat=${TONEMAP_DESAT}:peak=${TONEMAP_PEAK},zscale=p=bt709:t=bt709:m=bt709:r=tv,format=yuv420p,setparams=range=tv:color_primaries=bt709:color_trc=bt709:colorspace=bt709,sidedata=mode=delete:type=MASTERING_DISPLAY_METADATA,sidedata=mode=delete:type=CONTENT_LIGHT_LEVEL,sidedata=mode=delete:type=DYNAMIC_HDR_PLUS,sidedata=mode=delete:type=DYNAMIC_HDR_VIVID"
    echo "==> [$n] SDR reference $ref (zscale+tonemap=${alg}, npl=${TONEMAP_NPL}, peak=${TONEMAP_PEAK})"
    "$FFMPEG_BIN" -hide_banner -y -i "$src" \
      -vf "$graph" \
      -c:v libx264 -preset "$PRESET" -crf "$CRF" -pix_fmt yuv420p \
      -g "$FPS" -keyint_min "$FPS" -x264-params "scenecut=0:open_gop=0" \
      -color_primaries bt709 -color_trc bt709 -colorspace bt709 \
      -map 0:v:0 -map 0:a? -c:a aac -b:a 192k -ac 2 -ar 48000 \
      "$ref"
  done
}

for N in $CLIPS; do
  for raw_mode in $COLOR_MODES; do
    mode="$(mode_key "$raw_mode")"
    stem="$(mode_stem "$mode" "$N")"
    fg="$OUT/.${stem}.fg"
    mkv="$OUT/${stem}.mkv"
    ts="$OUT/${stem}.ts"

    render_filter_graph "$N" "$mode" "$fg"
    encode_video "$N" "$mode" "$fg" "$mkv"
    mux_ts "$N" "$mkv" "$ts"
    make_sdr_refs "$N" "$mode" "$mkv"
  done
done

cat <<EOF

==> done -> $OUT
    default SDR outputs keep the legacy names:
      cal_1.mkv/.ts  navy   1000 Hz   ABS 0:00:00 -> 0:03:00
      cal_2.mkv/.ts  forest 1500 Hz   ABS 0:03:00 -> 0:06:00
      cal_3.mkv/.ts  maroon 2000 Hz   ABS 0:06:00 -> 0:09:00
    HDR modes write cal_<mode>_<N>.mkv/.ts.
    Each video labels its own color mode and tone-mapping inspection cues.
    Use CAL_MAKE_SDR_REFS=1 to produce zscale+tonemap SDR reference files:
      CAL_HDR_MODES="hdr10-pq hlg" CAL_MAKE_SDR_REFS=1 \\
        CAL_TONEMAP_ALGORITHMS="hable mobius reinhard" \\
        scripts/calibration/gen-calibration-clips.sh

HDR -> SDR tone-mapping review:
  TSDuck can prove the transport, not the decoded tone-map pixels.
  Use this split for automated calibration:
    * ffprobe HDR masters: hevc/yuv420p10le, bt2020, PQ or HLG metadata.
    * TSDuck static/live TS: continuity, PCR/PTS/DTS, PAT/PMT, PIDs, bitrate.
    * ffprobe SDR output: yuv420p bt709 unless intentionally passthrough.
    * ffmpeg crops/metrics: compare live frames to *_to_sdr_<alg> references.
  Inspect the in-video helpers: mode label, peak-to-black chips, RGBYCM chips,
  dark grid, white timer boxes, centered square, and sweep bar.
  Fail for clipped peak chips, crushed dark grid lines, hue shifts, unreadable
  timer text, geometry changes, or timing/order regressions.

TSDuck quick checks (set LD path same as this script if you use ffprobe):
  tsanalyze cal_1.ts                                   # PIDs, bitrate, PCR, errors
  tsp -I file cal_1.ts -P continuity -O drop           # continuity-counter / packet-loss errors
  tsp -I file cal_1.ts -P pcrverify --jitter-max 5000000 -O drop   # PCR jitter (ns)
  tsp -I file cal_1.ts -P analyze -O drop              # rolling analysis report
For the LIVE ErsatzRS stream (replace URL with your channel's MPEG-TS):
  tsp -I http http://127.0.0.1:8409/iptv/channel/1.ts -P continuity -P pcrverify -O drop
EOF
