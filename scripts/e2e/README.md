# TSDuck playback gate (E2E)

A **required manual gate for confirming correct playback** — *not* part of CI.

CI proves the code compiles and the units pass; it does **not** prove that the
MPEG‑TS ErsatzRS actually serves is intact for a real player. This harness does:
it captures the served stream and uses [TSDuck](https://tsduck.io/) to prove the
*container* is correct — no packet loss, valid timing, the right structure —
across all streaming pipelines and across playout‑item boundaries (the concat
seams where ErsatzTV‑style stitching is most likely to break).

It is deliberately kept out of CI because it needs FFmpeg 8.1, real‑time
encoding, and tens of seconds per pipeline — too heavy and too timing‑sensitive
for a hosted runner. Run it locally before claiming playback‑sensitive work is
done, alongside `cargo xtask smoke`.

## Pairing with the calibration clips

This gate proves the **container**; the calibration clips from
[`scripts/calibration`](../calibration) prove the **content**. Each clip carries
a millisecond timer, a per‑frame counter (`f/60` stepping 0→59 every second),
and a sweep bar, so a human watching the stream confirms *order* and *smoothness*
by eye while TSDuck confirms the stream around it is sound.

The clips are **gitignored and never committed** — they are regenerated on
demand. `baseline` and `gate` auto-run the generator when the clips are missing,
so a fresh clone of `main` works with no manual setup (the first run spends a
few minutes encoding; later runs reuse the local files). To (re)generate
explicitly:

```bash
scripts/calibration/gen-calibration-clips.sh    # -> calibration-clips/cal_{1,2,3}.{mkv,ts}
```

## Requirements

`tsduck` (`tsanalyze`, `tsp`), `jq`, `curl`, and FFmpeg 8.1 on `PATH`
(the gate auto‑exports `LD_LIBRARY_PATH` for the local `~/.local/ffmpeg-8.1`
build, which fails silently without it). `sqlite3` and `cargo` are needed for
`gate` mode only.

## Modes

```bash
# 1. Baseline — validate the static calibration *.ts (no server). Fast self-test
#    of both the checks and the source quality.
scripts/e2e/tsduck-playback-gate.sh baseline

# 2. Live URL — capture a channel you already have running and validate it.
#    Works on the MPEG-TS endpoint or any HLS playlist.
scripts/e2e/tsduck-playback-gate.sh url 'http://127.0.0.1:8409/iptv/channel/1.ts?mode=ts-legacy' 20
scripts/e2e/tsduck-playback-gate.sh url 'http://127.0.0.1:8409/iptv/session/1/hls.m3u8' 20

# 3. Gate — the full E2E proof. Builds + boots ersatzrs-web on a throwaway
#    SQLite DB, seeds the 3 calibration clips as a short 3-item playout, then
#    captures and validates every pipeline and scans the server log.
scripts/e2e/tsduck-playback-gate.sh gate
```

Common flags: `--seconds N` (per‑pipeline capture window), `--item-seconds N`
(seconds of each clip the gate playout uses; total playout = 3×N, so a capture
longer than one item crosses the concat seams), `--frames` (extract PNGs for
visual order evidence), `--keep` (keep the work dir: captures, per‑stream
`*.analyze.json`, server log), `-h`.

**Exit status is the gate**: `0` only if every check passed.

The `gate` server port is hardcoded to **8409**; the gate refuses to start if
something is already serving there (it will not clobber another running server —
e.g. one started by another agent). In that case stop it, or use `url` mode.

## What gets checked (per stream)

| Check | Why it matters |
|---|---|
| packet count / TS bitrate ≥ min | the stream actually flowed |
| transport‑error (TEI) / sync losses = 0 | no corruption in delivery |
| PAT / PMT / service / SDT present | a player can find the program |
| ≥1 video PID, ≥1 audio PID | A/V actually present |
| PCR samples present | the stream carries a clock |
| **continuity‑counter errors = 0** | **real packet loss / skips** (primary signal) |
| **PTS / DTS / PCR leaps = 0** | **no visible skips, no clock breaks** |
| scrambled / invalid‑scrambling = 0 | content is clear |
| `continuity` processor cross‑check | independent confirmation of the CC check |
| HLS: `#EXTM3U`, segment count, target duration, discontinuity markers | playlist is well‑formed |
| `pcrverify` jitter | informational (see below) |

`continuity-counter errors` and `pts/dts/pcr leaps` are the playback‑correctness
core: a dropped or duplicated packet, or an unexpected timestamp jump, is exactly
what shows up as a stutter, skip, or A/V‑sync break in a player.

### A note on PCR jitter

`pcrverify` measures each PCR against the average‑bitrate byte‑position model, so
it is only a meaningful pass/fail for **constant‑bitrate** streams. FFmpeg VBR
output legitimately reports large jitter here, so it is **informational by
default**. Set `STRICT_PCR_JITTER=1` to gate on it (CBR channels only). Real PCR
clock breaks are already gated by the `pcr-leap` check.

## Thresholds (environment overrides)

| Var | Default | Meaning |
|---|---|---|
| `MAX_CC_ERRORS` | `0` | allowed continuity‑counter errors |
| `MAX_PTS_LEAPS` / `MAX_DTS_LEAPS` / `MAX_PCR_LEAPS` | `0` | allowed timestamp leaps |
| `MIN_BITRATE_BPS` | `100000` | minimum TS bitrate |
| `MIN_PACKETS` | `2000` | minimum captured packets |
| `EXPECT_VIDEO_PIDS` / `EXPECT_AUDIO_PIDS` | `1` | required ES counts |
| `MIN_HLS_SEGMENTS` | `1` | minimum `#EXTINF` entries |
| `STRICT_PCR_JITTER` | `0` | `1` = gate on `pcrverify` jitter (CBR only) |
| `MAX_PCR_JITTER_US` | `8000` | jitter ceiling (µs) when strict |
| `CAPTURE_SECONDS` | `20` | per‑pipeline capture window |
| `ITEM_SECONDS` | `8` | gate playout: seconds per clip |

## Ad‑hoc TSDuck

```bash
tsanalyze calibration-clips/cal_1.ts                      # full structural report
tsanalyze --json cal_1.ts | jq '.pids[]|{id,description,cc:.packets.discontinuities}'
tsp -I file cal_1.ts -P continuity -P pcrverify -O drop   # CC + PCR in one pass
tsp -I http 'http://127.0.0.1:8409/iptv/channel/1.ts' -P continuity -O drop  # live
```
