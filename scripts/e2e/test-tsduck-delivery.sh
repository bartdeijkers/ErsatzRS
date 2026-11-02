#!/usr/bin/env bash

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/e2e/tsduck-checks.sh
source "$SCRIPT_DIR/tsduck-checks.sh"

REQUESTED=130

expect_pass() {
  local name="$1" media_seconds="$2" total_seconds="$3"
  CHECKS_FAILED=0
  check_delivery_timing \
    "$((media_seconds * 1000))" "$REQUESTED" "$total_seconds" "$name" >/dev/null \
    || { printf 'expected pass: %s\n' "$name" >&2; exit 1; }
}

expect_fail() {
  local name="$1" media_seconds="$2" total_seconds="$3"
  CHECKS_FAILED=0
  if check_delivery_timing \
      "$((media_seconds * 1000))" "$REQUESTED" "$total_seconds" "$name" >/dev/null; then
    printf 'expected failure: %s\n' "$name" >&2
    exit 1
  fi
}

expect_pass healthy 130 130
expect_pass bursty_within_budget 125 135
expect_fail structurally_clean_starvation 60 130
expect_fail excessive_startup_latency 130 161

monitor_log=$(mktemp)
trap 'rm -f "$monitor_log"' EXIT

# Event-driven DVB cues may be more than3s apart while their PTS track PCR.
# Missing, stale, reordered or shifted observations must never clear raw flags.
printf '%s\n' \
  '258,1230,0,PTS,1,33601,0,-11' \
  '258,30039,28,PTS,2,820831,787230,-3' \
  '258,30040,29,PTS,3,820832,787231,-30' \
  '258,79444,57,PTS,4,2170831,2137230,-24' \
  >"$monitor_log"
_sparse_subtitle_pts_clock_matches "$monitor_log" 258 4 2 33601 2170831 \
  || { printf 'expected clock-bound sparse cues to qualify\n' >&2; exit 1; }
for metadata in '258 5 2 33601 2170831' '258 4 1 33601 2170831' \
    '258 4 2 33602 2170831' '258 4 2 33601 2170832' '259 4 2 33601 2170831'; do
  read -r pid count flags first final <<<"$metadata"
  if _sparse_subtitle_pts_clock_matches "$monitor_log" "$pid" "$count" "$flags" "$first" "$final"; then
    printf 'unexpected sparse cue qualification: %s\n' "$metadata" >&2; exit 1
  fi
done
for bad in '258,30040,29,PTS,3,820832,787231,' \
    '258,30040,29,PTS,3,820832,787231,900000' \
    '258,30040,29,PTS,3,12345,787231,-30' \
    '258,30040,29,PTS,3,invalid,787231,-30'; do
  printf '%s\n' '258,1230,0,PTS,1,33601,0,-11' \
    '258,30039,28,PTS,2,820831,787230,-3' "$bad" \
    '258,79444,57,PTS,4,2170831,2137230,-24' >"$monitor_log"
  if _sparse_subtitle_pts_clock_matches "$monitor_log" 258 4 2 33601 2170831; then
    printf 'unexpected invalid sparse cue qualification: %s\n' "$bad" >&2; exit 1
  fi
done
printf 'tsduck sparse subtitle clock regressions passed\n'

printf '%s\n' \
  'bitrate_monitor: ERSATZRS_DELIVERY{ "bitrate": 10368000, "status": "normal" }' \
  'bitrate_monitor: ERSATZRS_DELIVERY{ "bitrate": 10000000, "status": "normal" }' \
  >"$monitor_log"
CHECKS_FAILED=0
check_delivery_monitor "$monitor_log" 10184000 healthy_monitor >/dev/null \
  || { printf 'expected rolling monitor pass\n' >&2; exit 1; }

printf '%s\n' \
  'bitrate_monitor: ERSATZRS_DELIVERY{ "bitrate": 10368000, "status": "normal" }' \
  'bitrate_monitor: ERSATZRS_DELIVERY{ "bitrate": 0, "status": "lower" }' \
  >"$monitor_log"
CHECKS_FAILED=0
if check_delivery_monitor "$monitor_log" 5184000 starving_monitor >/dev/null; then
  printf 'expected rolling monitor failure\n' >&2
  exit 1
fi

printf 'tsduck delivery regressions passed\n'

# A failed input must retain its partial bytes and diagnostics with --keep.
# Stub only the gate's tools, so this regression needs no server or TSDuck.
capture_root=$(mktemp -d)
trap 'rm -f "$monitor_log"; rm -rf "$capture_root"' EXIT
mkdir "$capture_root/bin" "$capture_root/keep" "$capture_root/remove"
cat >"$capture_root/bin/tsp" <<'STUB'
#!/usr/bin/env bash
printf 'partial transport' >"${@: -1}"
printf 'synthetic capture failure\n' >&2
exit 1
STUB
chmod +x "$capture_root/bin/tsp"
for tool in tsanalyze jq curl; do
  ln -s tsp "$capture_root/bin/$tool"
done
for retention in keep remove; do
  args=()
  [ "$retention" = keep ] && args=(--keep)
  if PATH="$capture_root/bin:$PATH" TMPDIR="$capture_root/$retention" \
      bash "$SCRIPT_DIR/tsduck-playback-gate.sh" "${args[@]}" \
        url http://example.invalid/capture 1 >"$capture_root/$retention.log" 2>&1; then
    printf 'expected failed capture: %s\n' "$retention" >&2
    exit 1
  fi
done
kept=("$capture_root/keep"/tsduck-gate-url.*)
[ "${#kept[@]}" = 1 ] && [ -d "${kept[0]}" ] \
  && [ "$(cat "${kept[0]}/capture.ts")" = 'partial transport' ] \
  && grep -qF 'synthetic capture failure' "${kept[0]}/capture.ts.delivery.log" \
  || { printf 'failed --keep capture lost evidence\n' >&2; exit 1; }
removed=("$capture_root/remove"/tsduck-gate-url.*)
[ ! -e "${removed[0]}" ] \
  || { printf 'default failed capture was not cleaned\n' >&2; exit 1; }
printf 'tsduck failed-capture retention regressions passed\n'

# Match the real gate's strict shell: no diagnostic matches must not abort it.
# A separate shell retains errexit even though its result is checked here.
bash -euo pipefail -c '
  source "$1"
  tsp() { return 0; }
  check_continuity_plugin unused clean >/dev/null
  [ "$CHECKS_RUN" -eq 1 ] && [ "$CHECKS_FAILED" -eq 0 ]
  checks_summary >/dev/null
' bash "$SCRIPT_DIR/tsduck-checks.sh" \
  || { printf 'zero-error continuity check aborted under errexit\n' >&2; exit 1; }

CHECKS_RUN=0 CHECKS_FAILED=0
tsp() { printf 'synthetic missing packet\n' >&2; return 0; }
check_continuity_plugin unused discontinuity >/dev/null
[ "$CHECKS_RUN" -eq 1 ] && [ "$CHECKS_FAILED" -eq 1 ] \
  || { printf 'continuity diagnostics were not counted\n' >&2; exit 1; }
if checks_summary >/dev/null; then
  printf 'continuity diagnostic failure was hidden\n' >&2; exit 1
fi

# Even a silent processor failure must fail, independently of the line count.
CHECKS_RUN=0 CHECKS_FAILED=0
tsp() { return 7; }
if check_continuity_plugin unused silent_processor_failure >/dev/null; then
  printf 'silent continuity processor failure was hidden\n' >&2; exit 1
fi
[ "$CHECKS_RUN" -eq 1 ] && [ "$CHECKS_FAILED" -eq 1 ] \
  || { printf 'processor failure was not recorded\n' >&2; exit 1; }
unset -f tsp
printf 'tsduck strict-shell continuity regressions passed\n'
