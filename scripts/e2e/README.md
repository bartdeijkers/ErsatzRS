# TSDuck playback gate (E2E)

## Next copy-mode transport acceptance

Use the existing calibration playout with the accepted Next executable and
FFmpeg runtime staged locally:

```bash
ETV_NEXT_FOLDER=/path/to/staged-next \
ERSATZRS_FFMPEG__PATH=/path/to/ffmpeg \
ERSATZRS_FFPROBE__PATH=/path/to/ffprobe \
ERSATZRS_SMOKE_PORT=18409 \
scripts/e2e/tsduck-playback-gate.sh gate --next-copy --keep
```

This selects copy for both video and audio and captures Next Hybrid MPEG-TS and
HLS across eight-second programme boundaries. It retains the existing
continuity, clock, PSI, audio/video structure and audio-pip checks. Next does
not expose the native legacy/direct transports, so those remain in the default
native gate. The server uses an isolated database and application data/state
directories. The gate refuses an occupied port and leaves existing servers
alone. Select a free `ERSATZRS_SMOKE_PORT` when the default 8409 is in use.

Next HLS captures retain the served playlist and unchanged segment bytes,
grouped only at explicit `EXT-X-DISCONTINUITY` markers. Each group runs the same
zero-error TSDuck checks, including across every unmarked segment boundary.
This follows [RFC 8216 section 3](https://www.rfc-editor.org/rfc/rfc8216.html#section-3):
counter resets are allowed at a declared discontinuity. Raw concatenation loses
that information and reports valid programme transitions as packet loss.
The joined capture still checks decoded audio-pip cadence across transitions.
Run `python3 scripts/e2e/test-capture-hls-groups.py` with TSDuck installed to
prove that unmarked resets and packet loss inside a declared group still fail.

## Next RTSP input fixture

The separate Next CI job stages a pinned, checksum-verified MediaMTX v1.21.1
Linux x64 binary using `stage-rtsp-test-server.py`. It is a temporary RTSP source
for input-protocol acceptance, not a server dependency shipped with ErsatzRS.
The test starts its own loopback listener with synthetic media and stops it on
completion. TSDuck remains the transport-stream output validator; its HTTP/HLS
and UDP/RTP output plugins do not supply an RTSP control server.

For the opt-in Next runtime tests, supply the staged binary as
`ERSATZRS_NEXT_TEST_RTSP_SERVER` alongside `ERSATZRS_NEXT_TEST_BIN`,
`ERSATZRS_NEXT_TEST_FFMPEG` and `ERSATZRS_NEXT_TEST_FFPROBE`. Local tests on other
platforms can point at a compatible official MediaMTX binary; this Linux x64
staging helper serves the existing Linux x64 Next CI job only. The archive hash,
binary hash and upstream license are retained in the staging directory, and CI
uploads the staging provenance with its Next diagnostics. Tests require no
camera, external stream, personal library or permanent service.

References: [MediaMTX v1.21.1 configuration](https://github.com/bluenviron/mediamtx/blob/v1.21.1/mediamtx.yml)
and [TSDuck plugins](https://tsduck.io/commands/).

## Real SponsorBlock publication and transport probe

`tests/e2e/addon-sponsorblock-streaming.spec.ts` is opt-in and never runs in
default CI. It installs the actual yt-dlp add-on through the admin trust flow,
imports the required public fixture, prepares interval metadata, publishes,
and schedules that scanner-owned media for serial MPEG-TS and HLS captures.
It requires explicit FFmpeg 9 paths and does not use browser cookies.

Run against an isolated host/database with an existing matched SSR/WASM build
and installed Chromium. Keep its log available for the test; do not point these
settings at a production database or browser profile:

```bash
E2E_SPONSORBLOCK_ADDON_CHECKOUT=/path/to/addons-checkout \
ERSATZRS_BASE_URL=http://127.0.0.1:18409 \
E2E_DB_PATH=/path/to/isolated/ErsatzRS.db \
E2E_APP_STATE_DIR=/path/to/isolated/state \
E2E_SPONSORBLOCK_HOST_LOG=/path/to/isolated/host.log \
ErsatzRS_FFMPEG__PATH=/path/to/ffmpeg-9/bin/ffmpeg \
ErsatzRS_FFPROBE__PATH=/path/to/ffmpeg-9/bin/ffprobe \
npx playwright test tests/e2e/addon-sponsorblock-streaming.spec.ts \
  --project=chromium --workers=1
```

Captures and timing evidence are retained even when a transport fails. This
probe establishes actual publication and transport. Its H.264/AAC browser HLS
check records first presented frame, post-start stalls/seeks, frame gaps, buffer
samples and conservative temporal cut coverage while capturing those same
segments. MPEG-TS feeds the same HTTP response to one bounded FFmpeg decoder and
records first decoded audio/video, delivery gaps and decoded spans. A bounded
host-log window rejects concealed inner playback failures; only a timestamped
normal cancellation at or after the deliberate TS cutoff is exempt. Only static
failure categories enter the evidence, never raw log lines. Neither check establishes
exact source-content removal, audible continuity or injected network jitter.
Run without concurrent builds or playback checks. For transport diagnosis,
explicitly restart the retained
isolated host/database and pass `E2E_SPONSORBLOCK_PUBLISHED_LIST_ID` with its
existing E2E environment directly to Playwright; the fixture must already have
a completed publication. Configure the isolated host with `ERSATZRS_APP_STATE_DIR`
and `ERSATZRS_APP_DATA_DIR`; do not override `HOME` to isolate this probe.

## SponsorBlock streaming feasibility experiment

`sponsorblock-streaming-spike.mjs` is an isolated diagnostic, not a product or
player acceptance gate. It generates a synthetic source, serves paced HTTP
ranges with delayed opens, and runs one FFmpeg encoder into HLS. Its simulated
consumer measures completed-segment reserve without pausing its playback clock
to hide deficits. No real provider, credentials, application database or Rust
build is involved. All temporary evidence is retained.

```bash
FFMPEG_PATH=/path/to/ffmpeg-9/bin/ffmpeg \
FFPROBE_PATH=/path/to/ffmpeg-9/bin/ffprobe \
node scripts/e2e/sponsorblock-streaming-spike.mjs
```

The approved startup budget is ten seconds target, fifteen seconds maximum.
Other defaults remain experimental: eight seconds of startup reserve, 250 ms
open delay plus repeating 0/150/300 ms jitter, and 128 KiB/s per-response body
pacing. The earlier five-second cap and its results remain historical. The
script header lists overrides. Set both
`OPEN_DELAY_MS=0 OPEN_JITTER_MS=0` for the control. A missed bound exits nonzero;
do not increase bounds merely to obtain a pass. A simulation-only pass proves neither correct
removed content, A/V continuity, cancellation, nor uninterrupted real-player
playback. Set `BROWSER_PROOF=1` to additionally run the existing Chromium and
vendored HLS player against the growing output, checking startup and post-start
video/buffer continuity. This does not boot ErsatzRS or prove audible continuity
or actual-host integration. No browser or dependency is installed by the script.
Run serially, without another build or playback gate.

## Transport playback gate

A **required manual gate for confirming correct playback** — *not* part of CI.

CI proves the code compiles and the units pass; it does **not** prove that the
MPEG‑TS ErsatzRS actually serves is intact for a real player. This harness does:
it captures the served stream and uses [TSDuck](https://tsduck.io/) to prove the
*container* is correct — no packet loss, valid timing, the right structure —
across all streaming pipelines and across playout‑item boundaries (the concat
seams where ErsatzTV‑style stitching is most likely to break).

It is deliberately kept out of CI because it needs FFmpeg 9.0, real‑time
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

`tsduck` (`tsanalyze`, `tsp`), `jq`, `curl`, and FFmpeg 9 on `PATH`
(the gate auto‑exports `LD_LIBRARY_PATH` for the local `~/.local/ffmpeg-9.0`
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

The `url` mode also compares first-to-last PCR media duration with the requested
wall-clock capture window. `MIN_DELIVERY_RATIO_PERCENT` defaults to `95`, and
`MAX_STREAM_STARTUP_SECONDS` defaults to `30`. This makes a structurally clean
stream fail when it cannot deliver media fast enough for a realtime player.
The calculation deliberately does not divide packet count by PCR-derived
bitrate because that assumes constant bitrate and misclassifies VBR HLS.
TSDuck's rolling `bitrate_monitor` uses 50 percent of the capture's wall-clock
average as a severe-stall floor by default; input query strings are omitted
from gate output because they can contain access tokens.

Run the deterministic delivery-classification regression without a server or
provider request:

```bash
scripts/e2e/test-tsduck-delivery.sh
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
| PCR media duration / wall time ≥ configured ratio | content keeps pace without assuming CBR |
| rolling bitrate above the capture-relative severe-stall floor | no empty/starved delivery window |
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
| `MIN_DELIVERY_RATIO_PERCENT` | `95` | minimum PCR media-time / wall-time percentage |
| `MAX_STREAM_STARTUP_SECONDS` | `30` | maximum input startup latency |
| `MIN_ROLLING_DELIVERY_RATIO_PERCENT` | `50` | interval floor relative to capture average |
| `MAX_LOW_DELIVERY_INTERVALS` | `0` | intervals allowed below the rolling floor |
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
--locked`) and `npx`. An FFmpeg override at `~/.local/ffmpeg-9.0` (like
`run.sh`) is used when present so startup skips the managed runtime download.

| Env var | Default | Effect |
| --- | --- | --- |
| `ERSATZRS_SKIP_LEPTOS_BUILD` | `0` | `1` = reuse already-built hydration assets |
| `ERSATZRS_E2E_KEEP_TEMP` | `0` | `1` = keep the temp DB/log dir for debugging |
| `ERSATZRS_E2E_PORT` | `8409` | server port |
| `ERSATZRS_FFMPEG_DIR` | `~/.local/ffmpeg-9.0` | FFmpeg override root |
| `ERSATZRS_SKIP_PW_INSTALL` | `0` | `1` = skip `playwright install chromium` |

Per-route pass criteria: `window.__ersatzrs_hydrated === true` (set right after
`hydrate_islands()` in `crates/ersatzrs-web/src/lib.rs`), no uncaught page
errors, no hydration-signature console errors, and no fall-through to the 404
catch-all. Add new island routes to the `ISLAND_ROUTES` table in the spec.
Detail/editor routes that need a seeded `:id` are out of scope here — cover them
in dedicated seeded specs.

## Teletext resource decoder calibration

Run the explicitly ignored native diagnostic on Linux x64 with Python 3 and
installed libzvbi 0.2.42. Other platforms/versions require their own calibrated
oracle; this helper is not a runtime dependency or a platform acceptance gate.

```bash
fixture_dir="$(mktemp -d)"
ERSATZRS_TELETEXT_TEST_RESOURCE_FIXTURE="$fixture_dir/cases.json" \
  ./run.sh test -p ersatzrs-ffmpeg resource_runtime_tests --lib \
  --run-ignored ignored-only
python3 scripts/e2e/teletext-resource-decoder.py < "$fixture_dir/cases.json"
```

`ERSATZRS_TELETEXT_TEST_PYTHON` optionally selects Python through the existing
locator. The optional export contains only synthetic encoded units and expected
pixels; replay returns compact JSON with the actual decoder version and passed
case count. Inputs are bounded and strictly validated. Missing/mismatched native
libraries or failed cases exit nonzero. The existing runner enforces process
and output limits during the test.

Nine MOT-based cases check exact global/normal DRCS identities, all glyph pixels,
native PAL8 cell rendering, defined blank versus fallback, and independently
isolated missing-row, SPACE-half, parity and undefined-mode failures. They also
retain separate base-text checks. Resource units and local display-invocation
bodies come from production encoders; the nine-case fixture remains byte-identical
to the earlier synthetic calibration. The declared MOT extent is fully transmitted,
including empty companion subtables and correct counts.
Own-magazine closing headers avoid a version-specific 0.2.42 cache behavior.

The [interchange reference](../../docs/reference/teletext-design-interchange.md#pinned-independent-resource-decoding)
records source pins, coverage and the separately reproduced X27 limitation.
This diagnostic does not establish client display/navigation, transport integrity,
complete Level 2.5 compilation or the design/editor/publication workflow.

## Teletext object decoder calibration

The separate explicitly ignored diagnostic uses the same Linux x64, Python 3
and pinned libzvbi 0.2.42 public ABI. It imports the frozen resource helper's
version/layout guards, without changing the DRCS oracle.

```bash
fixture_dir="$(mktemp -d)"
ERSATZRS_TELETEXT_TEST_OBJECT_FIXTURE="$fixture_dir/objects.json" \
  ./run.sh test -p ersatzrs-ffmpeg object_runtime_tests --lib \
  --run-ignored ignored-only
python3 scripts/e2e/teletext-object-decoder.py < "$fixture_dir/objects.json"
```

Eight synthetic cases use complete production POP/GPOP pages, exact allocated
invocations and MOT declarations, including legal empty companions for S1=15.
They cover public/global active/adaptive/passive objects and two nested chains.
The fresh-decoder oracle checks exact glyphs, foreground/background colours,
relative origins, active propagation versus adaptive/passive containment and
unchanged base cells. It does not compare font pixels.

The helper bounds input to 2 MiB, twelve cases, 256 units and sixteen checked
cells per case. It rejects unknown/duplicate members and invalid dimensions,
numbers and bytes before native decoding. Replay emits compact version/count
JSON; the existing test runner bounds process time, stdout and stderr.

The [object-decoding reference](../../docs/reference/teletext-design-interchange.md#pinned-independent-object-decoding)
records exact fixture/helper hashes and coverage. Native proof covers the first
pointer bank only; byte tests cover all pointer banks and capacities. Higher-bank
external decoding, local objects, complete inheritance/service composition,
actual client display/navigation, transport and editor/publication remain open.
This helper is neither a production dependency nor platform acceptance.

## Local-object decoder limitation diagnostic

The explicitly ignored Linux x64 diagnostic records an actual libzvbi 0.2.42
limitation for correctly assigned local definition markers. It uses the unchanged
object helper and complete production local-program bodies.

```bash
fixture_dir="$(mktemp -d)"
ERSATZRS_TELETEXT_TEST_LOCAL_OBJECT_FIXTURE="$fixture_dir/diagnostic.json" \
ERSATZRS_TELETEXT_TEST_LOCAL_OBJECT_CONFORMANCE_FIXTURE="$fixture_dir/conformance.json" \
  ./run.sh test -p ersatzrs-ffmpeg \
  pinned_libzvbi_0_2_42_local_definition_marker_limitation_is_reproduced --lib \
  --run-ignored ignored-only
python3 scripts/e2e/teletext-object-decoder.py < "$fixture_dir/diagnostic.json"
# Expected to fail on this pinned decoder: correct authored glyph/colour oracle.
python3 scripts/e2e/teletext-object-decoder.py < "$fixture_dir/conformance.json"
```

The diagnostic requires exact base T/white/black fallback for all three local
object types, alongside the independent B calibration. The separate conformance
fixture retains the correct A/red/blue expectations. Their production units are
byte-identical; only expected decoder cells differ. A green diagnostic reproduces
the limitation and cannot establish local-object rendering or Level 2.5 acceptance.
The [local-decoder reference](../../docs/reference/teletext-design-interchange.md#local-object-decoder-limitation)
records hashes, source findings and the still-required decoder/client evidence.

## Teletext second local-object decoder calibration

The separate opt-in cell oracle uses unmodified teletextimager 0.2.0 at
`2b0125b778c45ffdfa2df0b196870c093361f317`. Supply an external checkout explicitly;
the helper checks its commit and imported source hashes before executing it.
It does not install packages or fetch sources. Python's standard library suffices
for these cell checks; this workflow does not invoke the Pillow renderer.

```bash
decoder_dir="$(mktemp -d)"
git clone https://github.com/gkthemac/teletextimager.git "$decoder_dir"
git -C "$decoder_dir" checkout --detach 2b0125b778c45ffdfa2df0b196870c093361f317
fixture_dir="$(mktemp -d)"
ERSATZRS_TELETEXT_TEST_IMAGER_DIR="$decoder_dir" \
ERSATZRS_TELETEXT_TEST_LOCAL_OBJECT_CONFORMANCE_FIXTURE="$fixture_dir/conformance.json" \
  ./run.sh test -p ersatzrs-ffmpeg \
  authored_local_objects_decode_with_pinned_independent_teletextimager --lib \
  --run-ignored ignored-only
ERSATZRS_TELETEXT_TEST_IMAGER_DIR="$decoder_dir" \
  python3 scripts/e2e/teletext-imager-decoder.py < "$fixture_dir/conformance.json"
```

The original correct conformance request retains SHA256
`11bfc143b8ad25d66433b7e8235cdede3fc39ab4cddae685547eb2c6c09bfe2c`.
Its `expected_version` field identifies its original libzvbi provenance; the
second helper guards and reports its own pinned decoder/version. All three local
types must render A/red/blue, with exact type-specific adjacent T attributes and
unchanged B. The decoder reads original production DVB bytes through its own
Hamming reader, after framing removal and bit reversal only. Wrong expected cells
or modified upstream sources fail rather than relaxing the oracle.

This proves representative direct local-object decoding. It is neither full
Level 2.5 acceptance nor actual interactive client/navigation or bitmap evidence.
Non-Latin adaptive/passive G0/G2, side-panel edges, automatic MOT/X27 selection,
complete local coverage and resource pointer extensions remain separate
proof obligations. See the
[pinned source and acceptance boundaries](../../docs/reference/teletext-design-interchange.md#second-independent-local-object-decoder).

The advanced opt-in case group adds production-encoded local definitions at
designation 8/slot 0 and designation 15/slot 0, plus a full-ID nested
Active-to-Adaptive-to-Passive chain. Reuse the same detached decoder checkout:

```bash
ERSATZRS_TELETEXT_TEST_IMAGER_DIR="$decoder_dir" \
ERSATZRS_TELETEXT_TEST_ADVANCED_LOCAL_OBJECT_FIXTURE="$fixture_dir/advanced.json" \
  ./run.sh test -p ersatzrs-ffmpeg \
  advanced_local_objects_decode_with_pinned_independent_teletextimager --lib \
  --run-ignored ignored-only
ERSATZRS_TELETEXT_TEST_IMAGER_DIR="$decoder_dir" \
  python3 scripts/e2e/teletext-imager-decoder.py < "$fixture_dir/advanced.json"
```

The advanced fixture SHA256 is
`57bf567c2dd894bf884d2435a9932738ca972c7fb3cfd16a77509801eceaece3`.
This is additional independent local formatting/origin proof, with unchanged
original fixtures. It does not certify all object interactions, pixels, transport
or client navigation. The only replaced base X26 in these synthetic ASCII cases
is its known empty sequence; generic base/enhancement composition remains open.

## Teletext ordered-main composition decoder calibration

The opt-in composition oracle reuses the unchanged pinned teletextimager loader
and its eight source guards. It checks literal code/repertoire/diacritic/colour
tuples at both Level 1.5 and 2.5 from production-composed and lowered DVB units.
Reuse the explicit detached `decoder_dir` and temporary `fixture_dir` above:

```bash
ERSATZRS_TELETEXT_TEST_IMAGER_DIR="$decoder_dir" \
ERSATZRS_TELETEXT_TEST_MAIN_COMPOSITION_FIXTURE="$fixture_dir/main.json" \
  ./run.sh test -p ersatzrs-ffmpeg \
  composed_main_program_decodes_with_pinned_independent_teletextimager --lib \
  --run-ignored ignored-only
ERSATZRS_TELETEXT_TEST_IMAGER_DIR="$decoder_dir" \
  python3 scripts/e2e/teletext-main-composition-decoder.py < "$fixture_dir/main.json"
```

The fixed cases cover derived pound/acute glyphs, compatible authored replacement,
Level-2.5 G0/G1 with Level-1.5 fallback, and an anchored full-ID local Passive
invocation with its definition pointer shifted by inserted base commands. All
original base X26 bodies are replaced by the one compiled sequence; header,
presentation and fallback bytes remain unchanged. The separate schema reports
the decoder's own 0.2.0 version and rejects duplicate/unknown fields and unbounded
units/cells. It adds no runtime dependency or source download. Non-Latin restore
decoding, Active overlap, header/dual-row lowering, complete display geometry,
transport and actual client navigation remain separate acceptance obligations.

The four-case fixture SHA256 is
`bf4584486120137270f3a5ed62a7830ee5b1af81923445c0b87013b8ae91c603`;
the dedicated helper SHA256 is
`e1cb2ad2b16e78a0f56baaae14434aa6d78f146d7e0173c7ade9f19db97228eb`.
The pages explicitly select German national option one: pound therefore requires
the intended derived G2 replacement, while ordinary ASCII retains repertoire 15
in the pinned decoder. The original local/resource fixture hashes stay unchanged.
