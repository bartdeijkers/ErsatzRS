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

HDR and tone-mapping calibration clips are opt-in so the default TSDuck gate
stays fast and compatible with its legacy `cal_{1,2,3}` inputs:

```bash
CAL_HDR_MODES="hdr10-pq hlg" \
CAL_MAKE_SDR_REFS=1 \
CAL_TONEMAP_ALGORITHMS="hable mobius reinhard" \
scripts/calibration/gen-calibration-clips.sh
```

The HDR masters are written as `cal_hdr10-pq_{1,2,3}.mkv` and
`cal_hlg_{1,2,3}.mkv`. SDR reference renders are written as
`cal_<mode>_to_sdr_<algorithm>_<N>.mkv`. Every generated video labels its own
color mode and embeds the inspection helpers: peak-to-black chips, saturated
RGBYCM chips, dark grid lines, white timer boxes, the centered geometry square,
and the sweep bar.

## HDR / Tone-Mapping Calibration Spec

TSDuck can automatically verify the **transport** part of HDR playback: TS
sync, continuity counters, PCR/PTS/DTS continuity, PAT/PMT/service structure,
video and audio PID presence, HLS playlist structure, and whether the stream
keeps flowing. It cannot decide whether HDR -> SDR tone mapping is visually
correct, because that requires decoded pixels and color-management semantics.
Use this split:

| Layer | Tool | Gate |
|---|---|---|
| HDR source signaling | `ffprobe` | HDR masters must be 10-bit BT.2020 with `smpte2084` for HDR10 PQ or `arib-std-b67` for HLG. Treat mastering-display and MaxCLL/MaxFALL side data as an additional libx265 capability check when it is present in the generated file. |
| Static TS integrity | TSDuck | Optional HDR `.ts` muxes must pass the same baseline checks as SDR: no TEI/sync loss, no continuity errors, PCR present, video/audio PIDs present, sane bitrate, and no timestamp leaps. |
| Live stream integrity | TSDuck | Captured ErsatzRS HDR-source playback must pass the existing URL/gate checks for every transport mode under test. |
| SDR output signaling | `ffprobe` | A tone-mapped output must be `bt709` primaries, `bt709` transfer, `bt709` matrix, 8-bit `yuv420p` unless the profile intentionally requests HDR passthrough. |
| SDR tone-map pixels | `ffmpeg` frame crops + metrics | Extract matching frames from the live stream and the generated `*_to_sdr_<algorithm>_*.mkv` reference; compare the embedded helper regions, not full-frame compression noise. |

Recommended automated pixel checks:

- Crop the right-side helper panel and compare it against the selected SDR
  reference with `ssim` or `psnr`. Use strict thresholds for software paths
  using the same algorithm, looser thresholds for hardware encoders.
- Run `signalstats` on the peak-to-black strip. It should retain multiple
  distinct luma bands after tone mapping; all-white peak chips, all-black dark
  chips, or collapsed adjacent bands are failures.
- Run `signalstats` on the RGBYCM strip. The saturated chips should remain
  chroma-distinct; a gray or swapped-hue strip is a failure.
- Crop the timer boxes and centered square. They should stay readable and
  geometrically stable relative to the SDR reference; unreadable labels indicate
  highlight clipping or range mistakes, while a distorted square indicates
  scaling/pixel-aspect regression.
- Keep the existing audio pip cadence and frame-order review. Tone mapping must
  not mask playback-order, A/V sync, or dropped-frame regressions.

Minimum manual review when pixel metrics are not wired yet:

1. Generate HDR masters and SDR references with the command above.
2. Ingest the HDR masters into ErsatzRS and stream through the target FFmpeg
   profile with SDR output enabled.
3. Capture the live output with the `url` mode and keep frame evidence:

   ```bash
   scripts/e2e/tsduck-playback-gate.sh --frames url \
     'http://127.0.0.1:8409/iptv/channel/1.ts' 20
   ```

4. Compare captured frames to the matching `*_to_sdr_<algorithm>_*.mkv`
   reference. Fail for clipped peak chips, crushed dark grid lines, hue shifts,
   unreadable timer text, geometry changes, or timing/order regressions.

The automated live content gate is:

```bash
scripts/e2e/hdr-tone-map-proof.sh
```

It generates short HDR10 PQ and HLG calibration masters plus SDR references,
boots `ersatzrs-web` on a throwaway SQLite database, seeds each HDR master as
the active channel source, captures the live MPEG-TS output, asserts 8-bit
BT.709 SDR signaling with `ffprobe`, and compares the decoded static helper-chip
crop against the generated SDR reference with PSNR and SSIM. Set
`HDR_PROOF_KEEP=1` to keep the throwaway DB, server log, live captures, and
metric files. The same gate honors `HDR_PROOF_MODES`, `HDR_PROOF_CLIP_SECONDS`,
`HDR_PROOF_CAPTURE_SECONDS`, `HDR_PROOF_SIZE`, `HDR_PROOF_MIN_PSNR`, and
`HDR_PROOF_MIN_SSIM` for local proof tuning.

## NVENC Encoder-Only Filter Gate

The automated live proof for ADR 0014's NVENC encoder-only path is:

```bash
NVENC_PROOF_FFMPEG_DIR=/path/to/ffmpeg-package/ffmpeg \
  scripts/e2e/nvenc-filter-gate-proof.sh
```

This gate is intentionally narrower than the normal playback gates. It only
runs when the selected FFmpeg build reports `cuda` hwaccel and `h264_nvenc`
while omitting `scale_cuda`, `yadif_cuda`, and `overlay_cuda`. A full
CUDA-filter build is rejected before boot because it cannot prove the
encoder-only software-graph demotion path. On a matching build, the script
boots `ersatzrs-web` on a throwaway SQLite database, seeds a short SDR
calibration clip, forces the channel profile to NVENC H.264, captures the live
MPEG-TS stream, verifies H.264 BT.709 output with `ffprobe`, and scans the
server log for CUDA-filter and playback failure signatures.

Set `NVENC_PROOF_KEEP=1` to keep the throwaway DB, server log, and capture.
The same gate honors `NVENC_PROOF_CLIP_SECONDS`,
`NVENC_PROOF_CAPTURE_SECONDS`, `NVENC_PROOF_SIZE`, and `NVENC_PROOF_FPS`.

## Scale And Pad Profile Proof

The live Scale and Pad proof reproduces synthetic 1920x800 scope-ratio source
geometry without using private library media:

```bash
scripts/e2e/scale-pad-profile-proof.sh
```

The gate generates a calibration source with
`CAL_SIZE_PRESET=cinema-1920x800` (`1920x800`, SAR `1:1`, DAR `12:5`), boots
`ersatzrs-web` on a throwaway SQLite database, seeds that source as active
channel content, forces a software H.264 `1920x1080` Scale and Pad profile,
captures the live MPEG-TS stream, and samples decoded pixels to prove the
output is `1920x1080` with black top/bottom padding and visible source
content. Set `SCALE_PAD_PROOF_KEEP=1` to keep the throwaway DB, logs, and
capture.

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

---

# Island hydration smoke (E2E)

A **required manual gate for confirming the admin UI is interactive** — *not*
part of CI. Where the TSDuck gate proves the served container is sound, this gate
proves every Leptos `#[island]` actually hydrates, so the click handlers that
SSR alone cannot exercise (stop/delete on `/channels`, refresh and row-select on
`/playouts`, every dialog and form) really attach in the browser.

It exists because we keep regressing one failure class: a change to a
shell-level island, a shared UI primitive, the router, or a Cargo dependency
aborts island hydration and silently disables the interactive controls on
otherwise-unrelated pages. Plain `<a>` links keep working, so it is easy to ship
without noticing.

```bash
scripts/e2e/hydration-smoke.sh
```

The runner is self-contained: it builds the Leptos hydration assets
(`cargo leptos build`), boots `ersatzrs-web` on a throwaway SQLite database with
an isolated `HOME` (never touches `~/.ersatzrs`), waits for `/healthz`, runs
`tests/e2e/island-hydration.spec.ts` against every island route, then tears the
server and temp files down. Requires `cargo-leptos` (`cargo install cargo-leptos
--locked`) and `npx`. An FFmpeg override at `~/.local/ffmpeg-8.1` (like
`run.sh`) is used when present so startup skips the managed runtime download.

| Env var | Default | Effect |
| --- | --- | --- |
| `ERSATZRS_SKIP_LEPTOS_BUILD` | `0` | `1` = reuse already-built hydration assets |
| `ERSATZRS_E2E_KEEP_TEMP` | `0` | `1` = keep the temp DB/log dir for debugging |
| `ERSATZRS_E2E_PORT` | `8409` | server port |
| `ERSATZRS_FFMPEG_DIR` | `~/.local/ffmpeg-8.1` | FFmpeg override root |
| `ERSATZRS_SKIP_PW_INSTALL` | `0` | `1` = skip `playwright install chromium` |

Per-route pass criteria: `window.__ersatzrs_hydrated === true` (set right after
`hydrate_islands()` in `crates/ersatzrs-web/src/lib.rs`), no uncaught page
errors, no hydration-signature console errors, and no fall-through to the 404
catch-all. Add new island routes to the `ISLAND_ROUTES` table in the spec.
Detail/editor routes that need a seeded `:id` are out of scope here — cover them
in dedicated seeded specs.
