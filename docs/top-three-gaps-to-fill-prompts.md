<!--
generated: 2026-06-19
last_processed_commit: 8327709ec1a316332fba661bb947135483f523a0
parity_baseline_commit: 0d301df5e8512fdfccf3a611188d27ab1690ee31
generator: docs/automation/daily-progress-routine.md (ersatzrs-daily-progress)
rules: docs/automation/gap-prompt-rules.md
-->

# Top three gaps to fill - goal prompts

Actionable paste-and-run goal prompts, each closing **one** ErsatzTV parity gap
or ADR-backed parity-critical runtime hardening gap. Auto-prioritized from
`docs/progress`, with manual maintainer promotions called out inline.
Regenerated daily by the [progress routine](automation/daily-progress-routine.md);
see the [rules](automation/gap-prompt-rules.md). For the live gap list run
`cargo xtask progress gaps`.

**Parity baseline:** gaps are measured against ErsatzTV commit
[`0d301df`](https://github.com/ErsatzTV/legacy/commit/0d301df5e8512fdfccf3a611188d27ab1690ee31)
(`remove external dependencies (bugsnag, trakt)`). Upstream work after that
commit is out of scope until the pin in
[`automation/parity-baseline.md`](automation/parity-baseline.md) is advanced.

Current reconcile (2026-06-19, through `8327709`): `cargo xtask progress gaps`
lists **2** remaining features, so this file intentionally contains only two
actionable prompts:

- `ffmpeg-command-generation`
- `hls-variant-playlists-segmenter`

The completed media collection route-family prompt was removed per
`docs/automation/gap-prompt-rules.md`: completed scope is replaced, never kept
with a completed banner. ADR 0008 FFmpeg runtime acquisition (ship without
FFmpeg; download/update from the `bartdeijkers/ErsatzRS-ffmpeg` releases at
startup; surface update availability on `/system/health`) is still
parity-critical infrastructure, but it is not currently emitted by
`cargo xtask progress gaps`; refresh this file from progress data before
promoting it over either open feature.

---

## 1 - FFmpeg command generation: default profiles, AC-3 segmenter delay, and HDR live proof (slug `ffmpeg-command-generation`)

Fix the next ErsatzRS runtime-hardening gap: finish the ADR-backed FFmpeg
command-generation debts for default profile seeding, multichannel AC-3
segmenter delay, and HDR tonemap live proof (slug
`ffmpeg-command-generation`, debts `default-profile-1080p60-retune`,
`ac3-segmenter-max-delay`, and `hdr-tonemap-calibration-live-proof`). Fold
`nvenc-filter-gate-live-proof` into the validation run only if encoder-capable
hardware/builds are available.

Use the repo workflow from `AGENTS.md`. This is ADR-backed FFmpeg/HLS runtime
hardening, not a pinned-baseline ErsatzTV behavior gap, so do not force it
through `skills/parity-slice.md`. Read ADR 0017, ADR 0014, ADR 0003,
`docs/progress/parity/ffmpeg-command-generation.toml`,
`plans/hls-transcode-capacity-and-admission.md` only for HLS interactions, and
the pinned ErsatzTV FFmpeg source where it defines the command/filter baseline.

Current `ParityDebt` from
`docs/progress/parity/ffmpeg-command-generation.toml`:
- `default-profile-1080p60-retune` (ADR 0017): fresh databases still seed one
  conservative `1920x1080 x264 aac` profile instead of five ready-to-use
  720p/1080p/1080p HDR/4K/4K HDR AC-3 profiles with seed-time hardware
  acceleration detection and profile #2 as the default.
- `ac3-segmenter-max-delay` (ADR 0017): HLS segmenter output does not emit
  `-max_delay 500000` for multichannel AC-3, so 6-channel AC-3 can pop or drift
  at segment boundaries.
- `hdr-tonemap-calibration-live-proof`: HDR->SDR command/filter shape is
  snapshot-covered and calibration clips exist, but no live ErsatzRS stream
  capture/pixel comparison has proven the path.

Before editing:
1. Inspect `crates/ersatzrs-infra/src/db.rs`,
   `crates/ersatzrs-domain/src/ffmpeg.rs`,
   `crates/ersatzrs-ffmpeg/src/pipeline/`,
   `crates/ersatzrs-ffmpeg/src/filter/`, calibration scripts, and current
   FFmpeg profile tests/snapshots.
2. Build a Source Map for fresh DB defaults -> hardware capability detection ->
   seeded profile rows -> command generation -> HLS segmenter output -> live
   HDR calibration proof.
3. Verify which parts are ADR 0017 deviations from ErsatzTV rather than
   pinned-baseline parity, and keep that distinction visible.
4. Keep hardware-bound or platform-bound shortfalls as structured `ParityDebt`,
   not prose.

Implement the coherent hardening slice: seed the five ADR 0017 profiles on
fresh databases without overwriting existing user profiles; set the default
profile pointer to the 1080p SDR profile; detect and apply the preferred
available hardware acceleration at seed time while preserving ADR 0014 runtime
fallback; emit `-max_delay 500000` only for multichannel AC-3 HLS segmenter
outputs; and run or script the HDR calibration live proof that streams HDR10 PQ
and HLG clips through ErsatzRS, decodes the output, and compares helper regions
against SDR references. Do not weaken existing command-generation snapshots or
replace proof with argv-only assertions.

Add focused tests/snapshots for the seeded profile rows, hardware-acceleration
selection fallback, AC-3 `-max_delay` gating, and HDR tonemap command paths. If
live HDR proof cannot run on the current machine, keep
`hdr-tonemap-calibration-live-proof` open with exact missing commands and
runtime prerequisites.

Verify with focused `ersatzrs-infra` DB seed tests, `ersatzrs-ffmpeg` snapshot
tests, the HDR calibration/live-stream proof when available, `cargo xtask smoke`
if streaming behavior changes, `cargo xtask progress check`, and
`cargo xtask progress gaps`. Update ADR 0017,
`docs/progress/parity/ffmpeg-command-generation.toml`, the changelog, and this
prompt file only when evidence supports the new state. Report the Source Map,
behavior closed, remaining `ParityDebt`, verification run, and changed files.

---

## 2 - HLS variant playlists / segmenter: hard active transcode capacity (slug `hls-variant-playlists-segmenter`)

Fix the next ErsatzRS streaming runtime-hardening gap: make
`ffmpeg.segmenter.active_transcode_limit` a real ceiling for actively watched
HLS segmenter sessions (slug `hls-variant-playlists-segmenter`, debt
`active-transcode-limit-not-a-real-ceiling`).

Use the repo workflow from `AGENTS.md`. This is ADR-backed HLS runtime hardening
under ADR 0016, not a pinned-baseline ErsatzTV behavior gap, so do not force it
through `skills/parity-slice.md`. Read ADR 0016,
`plans/hls-transcode-capacity-and-admission.md`,
`docs/progress/parity/hls-variant-playlists-segmenter.toml`, and the pinned
ErsatzTV HLS source only where it informs the existing segmenter/session
baseline.

Current `ParityDebt` from
`docs/progress/parity/hls-variant-playlists-segmenter.toml`:
- `active-transcode-limit-not-a-real-ceiling` (ADR 0016): the current priority
  bypass increments the active-transcode counter without an upper bound for
  recently accessed sessions, and every playlist/segment request calls
  `touch()`. With K viewers on K channels and limit N, K-N sessions can
  force-reserve, yielding K concurrent transcodes regardless of N.

Before editing:
1. Inspect `crates/ersatzrs-infra/src/hls_worker.rs`,
   `crates/ersatzrs-infra/src/hls.rs`, `crates/ersatzrs-core/src/hls.rs`,
   `crates/ersatzrs-web/src/transport/iptv/hls.rs`, the `/channels` active
   transcode UI/status path, and existing HLS worker tests.
2. Build a Source Map for playlist/segment touch -> watched/cooling state ->
   capacity reservation -> work-ahead reclamation -> preemption/admission ->
   error/slate response -> active-session reporting.
3. Preserve the already-ported HLS segmenter and StopOnDisconnect lifecycle
   behavior; do not regress `cargo xtask smoke` or TSDuck playback assumptions.
4. Keep MPEG-TS pipeline-agnostic counting or hardware-aware default tuning as
   visible future debt unless this slice proves it end-to-end.

Implement the coherent ADR 0016 slice: replace the unbounded priority bypass
with a hard capacity ceiling; decouple the watch window from idle retention;
stop work-ahead first, reclaim cooling sessions next, preempt the stalest
watched session only when configured behavior allows it, and otherwise serve an
"all streams in use" slate/error path that holds no active transcode slot. Keep
the active count accurate during cancellation, process exit, temp-dir cleanup,
playlist polling, and segment fetches.

Add focused tests for limit N with K>N watched channels, playlist/segment touch
not bypassing the cap, work-ahead reclamation before watched-session preemption,
slot release on cancellation/drop, and active-session/status reporting. Add or
update browser/UI coverage only if the visible `/channels` readout or controls
change.

Verify with focused HLS worker tests, `cargo xtask smoke`, the TSDuck playback
gate if segmenter lifecycle behavior changes, `cargo xtask progress check`, and
`cargo xtask progress gaps`. Update ADR 0016,
`plans/hls-transcode-capacity-and-admission.md`,
`docs/progress/parity/hls-variant-playlists-segmenter.toml`, the changelog, and
this prompt file only when evidence supports the new state. Report the Source
Map, admission/preemption behavior, remaining `ParityDebt`, verification run,
and changed files.
