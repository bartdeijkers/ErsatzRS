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
