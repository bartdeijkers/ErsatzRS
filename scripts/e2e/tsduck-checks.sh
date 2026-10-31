#!/usr/bin/env bash
# TSDuck check library — sourced by tsduck-playback-gate.sh.
#
# Provides extensive transport-stream validation of a captured (or served)
# MPEG-TS, with pass/fail accounting so the caller can use the exit status as a
# gate. Every check is a single PASS/FAIL line; counters live in
# CHECKS_RUN / CHECKS_FAILED. Call checks_summary at the end.
#
# Authoritative structural data comes from `tsanalyze --json` (parsed with jq);
# PCR jitter comes from the `pcrverify` processor; continuity is cross-checked
# with the `continuity` processor. Requires: tsduck (tsanalyze, tsp), jq.

# --- thresholds (override via environment) ----------------------------------
: "${MAX_CC_ERRORS:=0}"              # continuity-counter errors == real packet loss
: "${MAX_PTS_LEAPS:=0}"             # unexpected PTS jumps == visible skips
: "${MAX_DTS_LEAPS:=0}"
: "${MAX_PCR_LEAPS:=0}"
: "${MAX_PCR_JITTER_US:=8000}"      # per-PCR jitter ceiling (microseconds)
: "${MAX_PCR_JITTER_VIOLATIONS:=0}" # how many PCRs may exceed that ceiling
: "${MIN_BITRATE_BPS:=100000}"
: "${MIN_PACKETS:=2000}"
: "${STRICT_PCR_JITTER:=0}"          # 1 = fail on pcrverify jitter (only sound for CBR channels)
: "${MAX_DUP_PACKETS:=0}"           # duplicated packets (warn-only unless >0 here)
: "${EXPECT_VIDEO_PIDS:=1}"
: "${EXPECT_AUDIO_PIDS:=1}"
: "${MIN_HLS_SEGMENTS:=1}"
: "${MIN_DELIVERY_RATIO_PERCENT:=95}" # captured media time / requested wall time
: "${MAX_STREAM_STARTUP_SECONDS:=30}" # request start until tsp receives its first packet
: "${MIN_ROLLING_DELIVERY_RATIO_PERCENT:=50}" # rolling bitrate floor vs capture average
: "${MAX_LOW_DELIVERY_INTERVALS:=0}"   # rolling intervals below the floor

CHECKS_RUN=0
CHECKS_FAILED=0
LAST_TS_PACKETS=0
LAST_TS_BITRATE=0
LAST_TS_MEDIA_MS=0

if [ -t 1 ]; then
  C_PASS=$'\033[32m'; C_FAIL=$'\033[31m'; C_WARN=$'\033[33m'; C_DIM=$'\033[2m'; C_OFF=$'\033[0m'
else
  C_PASS=""; C_FAIL=""; C_WARN=""; C_DIM=""; C_OFF=""
fi

pass() { CHECKS_RUN=$((CHECKS_RUN + 1)); printf '%s[ PASS ]%s %s\n' "$C_PASS" "$C_OFF" "$1"; }
fail() {
  CHECKS_RUN=$((CHECKS_RUN + 1)); CHECKS_FAILED=$((CHECKS_FAILED + 1))
  printf '%s[ FAIL ]%s %s%s\n' "$C_FAIL" "$C_OFF" "$1" "${2:+ — $2}"
}
warn() { printf '%s[ WARN ]%s %s\n' "$C_WARN" "$C_OFF" "$1"; }
info() { printf '%s[ info ] %s%s\n' "$C_DIM" "$1" "$C_OFF"; }

# assert_<op> ACTUAL BOUND LABEL  → PASS/FAIL with the values shown
assert_le() { if [ "$1" -le "$2" ] 2>/dev/null; then pass "$3 ($1 <= $2)"; else fail "$3" "got $1, allowed <= $2"; fi; }
assert_ge() { if [ "$1" -ge "$2" ] 2>/dev/null; then pass "$3 ($1 >= $2)"; else fail "$3" "got $1, need >= $2"; fi; }
assert_gt() { if [ "$1" -gt "$2" ] 2>/dev/null; then pass "$3 ($1 > $2)"; else fail "$3" "got $1, need > $2"; fi; }
assert_eq() { if [ "$1" = "$2" ]; then pass "$3 ($1)"; else fail "$3" "got $1, expected $2"; fi; }

# _jn 'jq-expr' file  → numeric scalar (null/empty → 0)
_jn() { jq -r "($1) // 0" "$2" 2>/dev/null || echo 0; }

# analyze_ts TSFILE LABEL [JSON_OUT]
# Runs the full structural battery against a transport-stream file.
analyze_ts() {
  local ts="$1" label="$2" json="${3:-${WORKDIR:-/tmp}/${2//[^A-Za-z0-9]/_}.analyze.json}"

  if [ ! -s "$ts" ]; then fail "$label · capture present" "empty or missing: $ts"; return 1; fi
  if ! tsanalyze --json "$ts" >"$json" 2>/dev/null || [ ! -s "$json" ]; then
    fail "$label · tsanalyze" "could not parse $ts"; return 1
  fi

  local packets terr isync bitrate cc ptsleap dtsleap pcrleap scrpids scrpkt invscr vid aud pcr dup pat pmt sdt svc
  packets=$(_jn '.ts.packets.total' "$json")
  terr=$(_jn '.ts.packets["transport-errors"]' "$json")
  isync=$(_jn '.ts.packets["invalid-syncs"]' "$json")
  bitrate=$(_jn '.ts.bitrate' "$json")
  cc=$(_jn '[.pids[].packets.discontinuities]|add' "$json")
  ptsleap=$(_jn '[.pids[].packets["pts-leap"]]|add' "$json")
  dtsleap=$(_jn '[.pids[].packets["dts-leap"]]|add' "$json")
  pcrleap=$(_jn '[.pids[].packets["pcr-leap"]]|add' "$json")
  scrpids=$(_jn '[.pids[]|select(.["is-scrambled"]==true)]|length' "$json")
  scrpkt=$(_jn '[.pids[].packets.scrambled]|add' "$json")
  invscr=$(_jn '[.pids[].packets["invalid-scrambling"]]|add' "$json")
  vid=$(_jn '[.pids[]|select(.video==true)]|length' "$json")
  aud=$(_jn '[.pids[]|select(.audio==true)]|length' "$json")
  pcr=$(_jn '[.pids[].packets.pcr]|add' "$json")
  dup=$(_jn '[.pids[].packets.duplicated]|add' "$json")
  pat=$(_jn '[.pids[]|select(.id==0)]|length' "$json")
  pmt=$(_jn '[.pids[]|select(.pmt==true)]|length' "$json")
  sdt=$(_jn '[.pids[]|select(.id==17)]|length' "$json")
  svc=$(_jn '.services|length' "$json")
  LAST_TS_PACKETS="$packets"
  LAST_TS_BITRATE="$bitrate"
  local pcr_csv="${json}.pcr.csv"
  if tsp -I file "$ts" -P pcrextract --pcr --csv --noheader -o "$pcr_csv" -O drop \
      >/dev/null 2>&1; then
    LAST_TS_MEDIA_MS=$(awk -F, '
      $4 == "PCR" && $7 ~ /^[0-9]+$/ && $7 > maximum { maximum=$7 }
      END { printf "%.0f", maximum * 1000 / 27000000 }
    ' "$pcr_csv")
  else
    LAST_TS_MEDIA_MS=0
  fi

  info "$label — packets=$packets bitrate=${bitrate}bps services=$svc video_pids=$vid audio_pids=$aud TEI=$terr sync_errs=$isync"

  # --- packet / bitrate sanity (stream actually flowed, undamaged) ---
  assert_ge "$packets" "$MIN_PACKETS" "$label · packet count"
  assert_ge "$bitrate" "$MIN_BITRATE_BPS" "$label · TS bitrate"
  assert_eq "$terr" 0 "$label · transport-error (TEI) packets"
  assert_eq "$isync" 0 "$label · TS sync losses"

  # --- PSI/structure (a player can actually find the program) ---
  assert_gt "$pat" 0 "$label · PAT present"
  assert_gt "$pmt" 0 "$label · PMT present"
  assert_gt "$svc" 0 "$label · service declared"
  if [ "$sdt" -gt 0 ]; then pass "$label · SDT/service-info present"; else warn "$label · no SDT (service name table) — players still work, EPG-bound clients may not"; fi

  # --- elementary streams present ---
  assert_ge "$vid" "$EXPECT_VIDEO_PIDS" "$label · video PID present"
  assert_ge "$aud" "$EXPECT_AUDIO_PIDS" "$label · audio PID present"

  # --- timing model present (PCR is what keeps playback paced) ---
  assert_gt "$pcr" 0 "$label · PCR samples present"

  # --- the playback-correctness signals ---
  assert_le "$cc" "$MAX_CC_ERRORS" "$label · continuity-counter errors (packet loss)"
  if [ "$cc" -gt "$MAX_CC_ERRORS" ]; then
    jq -r '.pids[]|select(.packets.discontinuities>0)|"           ↳ PID \(.id) (\(.description)): \(.packets.discontinuities) discontinuities"' "$json" 2>/dev/null
  fi
  assert_le "$ptsleap" "$MAX_PTS_LEAPS" "$label · PTS leaps (visible skips/jumps)"
  assert_le "$dtsleap" "$MAX_DTS_LEAPS" "$label · DTS leaps"
  assert_le "$pcrleap" "$MAX_PCR_LEAPS" "$label · PCR leaps (clock breaks)"

  # --- must be clear (unscrambled) ---
  assert_eq "$scrpids" 0 "$label · scrambled PIDs"
  assert_eq "$scrpkt" 0 "$label · scrambled packets"
  assert_eq "$invscr" 0 "$label · invalid-scrambling packets"

  # --- duplicated packets: warn-only by default ---
  if [ "$dup" -gt "$MAX_DUP_PACKETS" ]; then
    if [ "$MAX_DUP_PACKETS" -eq 0 ]; then warn "$label · $dup duplicated packets (allowed by MPEG, watch if growing)"
    else fail "$label · duplicated packets" "got $dup, allowed <= $MAX_DUP_PACKETS"; fi
  fi
}

# check_pcr_jitter TSFILE LABEL
# NOTE: pcrverify measures PCR value vs the average-bitrate position model, so
# it is only a meaningful pass/fail signal for CONSTANT-bitrate streams. ffmpeg
# VBR output legitimately reports large jitter here, so this is informational by
# default; set STRICT_PCR_JITTER=1 to gate on it (CBR channels only). Real PCR
# clock breaks are caught by the pcr-leap check in analyze_ts.
check_pcr_jitter() {
  local ts="$1" label="$2" out viol
  out=$(tsp -I file "$ts" -P pcrverify --jitter-max "$MAX_PCR_JITTER_US" -O drop 2>&1 | grep -i 'pcrverify' | tail -1)
  if [ -z "$out" ]; then warn "$label · pcrverify produced no report (no PCR?)"; return; fi
  info "$label — ${out#\* }"
  viol=$(printf '%s' "$out" | sed -nE 's/.*OK, ([0-9,]+) with jitter.*/\1/p' | tr -d ',')
  viol="${viol:-0}"
  if [ "$STRICT_PCR_JITTER" = 1 ]; then
    assert_le "$viol" "$MAX_PCR_JITTER_VIOLATIONS" "$label · PCR jitter > ${MAX_PCR_JITTER_US}us [strict/CBR]"
  else
    info "$label — PCR jitter informational (VBR); set STRICT_PCR_JITTER=1 to gate"
  fi
}

# check_continuity_plugin TSFILE LABEL  (independent cross-check of CC errors)
check_continuity_plugin() {
  local ts="$1" label="$2" errs report
  if ! report=$(tsp -I file "$ts" -P continuity -O drop 2>&1); then
    fail "$label · continuity processor" "$report"
    return 1
  fi
  # Zero matching lines is a successful clean stream, including with errexit.
  # Keep the processor exit independent of the diagnostic count.
  errs=$(printf '%s\n' "$report" | awk '
    tolower($0) ~ /discontinuity|broken|missing|error/ { count++ }
    END { print count + 0 }')
  assert_le "$errs" "$MAX_CC_ERRORS" "$label · continuity processor errors"
}

# check_delivery_timing MEDIA_MS REQUESTED_SECONDS TOTAL_SECONDS LABEL
#
# TSDuck's `until --seconds` uses wall-clock time after the first packet, while
# pcrextract reports the media clock elapsed between the first and last PCR.
# Comparing those independent clocks exposes a stream that remains structurally
# valid but delivers too little media for a realtime player. Packet count divided
# by a PCR-derived bitrate is intentionally not used: that calculation assumes
# constant bitrate and misclassifies valid variable-rate HLS. TOTAL_SECONDS
# starts before the input plugin opens, so startup remains a separate signal.
check_delivery_timing() {
  local media_ms="$1" requested="$2" total="$3" label="$4"
  local before="$CHECKS_FAILED"
  if [ "$media_ms" -le 0 ] 2>/dev/null || [ "$requested" -le 0 ] 2>/dev/null \
      || [ "$total" -le 0 ] 2>/dev/null; then
    fail "$label · realtime delivery" "missing PCR media-time or elapsed-time evidence"
    return 1
  fi

  local requested_ms media_ratio startup_seconds
  requested_ms=$((requested * 1000))
  media_ratio=$((media_ms * 100 / requested_ms))
  startup_seconds=$((total > requested ? total - requested : 0))

  info "$label — pcr_media_ms=$media_ms requested_ms=$requested_ms delivery_ratio=${media_ratio}% startup_seconds=$startup_seconds"
  assert_ge "$media_ratio" "$MIN_DELIVERY_RATIO_PERCENT" \
    "$label · media-time / wall-time delivery ratio (%)"
  assert_le "$startup_seconds" "$MAX_STREAM_STARTUP_SECONDS" \
    "$label · startup latency (seconds)"

  [ "$CHECKS_FAILED" -eq "$before" ]
}

# check_delivery_monitor LOG CAPTURE_AVERAGE_BITRATE LABEL
#
# bitrate_monitor writes prefixed JSON lines. The reference is the capture's
# wall-clock average, not tsanalyze's PCR-derived bitrate, so VBR HLS is not
# mistaken for under-delivery. The deliberately low rolling floor catches an
# empty or severely starved interval while tolerating ordinary segment bursts.
check_delivery_monitor() {
  local log="$1" average_bitrate="$2" label="$3"
  local before="$CHECKS_FAILED"
  if [ ! -s "$log" ] || [ "$average_bitrate" -le 0 ] 2>/dev/null; then
    warn "$label · rolling delivery monitor unavailable"
    return 0
  fi

  local threshold samples low minimum stats
  threshold=$((average_bitrate * MIN_ROLLING_DELIVERY_RATIO_PERCENT / 100))
  stats=$(sed -nE 's/^[^{]*(\{.*\})[^}]*$/\1/p' "$log" \
    | jq -r 'select(.status != "summary") | .bitrate' 2>/dev/null \
    | awk -v threshold="$threshold" '
        BEGIN { samples=0; low=0; minimum=-1 }
        /^[0-9]+$/ {
          samples += 1
          if ($1 < threshold) low += 1
          if (minimum < 0 || $1 < minimum) minimum=$1
        }
        END { printf "%d %d %d", samples, low, minimum }
      ')
  read -r samples low minimum <<<"${stats:-0 0 -1}"
  if [ "$samples" -eq 0 ]; then
    warn "$label · rolling delivery monitor produced no periodic sample"
    return 0
  fi

  info "$label — rolling_samples=$samples low_delivery_intervals=$low minimum_interval_bitrate=${minimum}bps threshold=${threshold}bps"
  assert_le "$low" "$MAX_LOW_DELIVERY_INTERVALS" \
    "$label · rolling low-delivery intervals"
  [ "$CHECKS_FAILED" -eq "$before" ]
}

# check_hls_playlist URL_OR_FILE LABEL  (structure of an .m3u8)
check_hls_playlist() {
  local src="$1" label="$2" body segs variants display_src
  display_src="${src%%\?*}"
  if [[ "$src" == http* ]]; then body=$(curl -fsS --max-time 15 "$src" 2>/dev/null); else body=$(cat "$src" 2>/dev/null); fi
  if [ -z "$body" ]; then fail "$label · HLS playlist fetch" "empty/unreachable: $display_src"; return 1; fi
  if printf '%s' "$body" | grep -q '#EXTM3U'; then pass "$label · #EXTM3U header"; else fail "$label · #EXTM3U header" "not an HLS playlist"; fi
  segs=$(printf '%s' "$body" | grep -c '#EXTINF')
  variants=$(printf '%s' "$body" | grep -c '#EXT-X-STREAM-INF')
  if [ "$segs" -ge "$MIN_HLS_SEGMENTS" ]; then
    assert_ge "$segs" "$MIN_HLS_SEGMENTS" "$label · media segments"
    if printf '%s' "$body" | grep -q '#EXT-X-TARGETDURATION'; then pass "$label · TARGETDURATION tag"; else warn "$label · no #EXT-X-TARGETDURATION"; fi
  elif [ "$variants" -gt 0 ]; then
    assert_gt "$variants" 0 "$label · master playlist variants"
  else
    fail "$label · playable entries" "no media segments or master-playlist variants"
  fi
  local disc; disc=$(printf '%s' "$body" | grep -c '#EXT-X-DISCONTINUITY')
  [ "$disc" -gt 0 ] && info "$label — playlist declares $disc discontinuity marker(s) (expected at playout-item boundaries)"
}

# checks_summary  → prints totals, returns nonzero if any check failed
checks_summary() {
  printf '\n────────────────────────────────────────────\n'
  if [ "$CHECKS_FAILED" -eq 0 ]; then
    printf '%sGATE PASS%s — %d checks, 0 failures\n' "$C_PASS" "$C_OFF" "$CHECKS_RUN"
    return 0
  fi
  printf '%sGATE FAIL%s — %d/%d checks failed\n' "$C_FAIL" "$C_OFF" "$CHECKS_FAILED" "$CHECKS_RUN"
  return 1
}
