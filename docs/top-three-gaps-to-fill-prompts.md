<!--
generated: 2026-06-14
last_processed_commit: f36244538ac4953650f37ceb9c4a281a5c2f4c50
parity_baseline_commit: 0d301df5e8512fdfccf3a611188d27ab1690ee31
generator: docs/automation/daily-progress-routine.md (ersatzrs-daily-progress)
rules: docs/automation/gap-prompt-rules.md
-->

# Top three gaps to fill - goal prompts

Three paste-and-run goal prompts, each closing **one** ErsatzTV parity gap.
Auto-prioritized from `docs/progress`. Regenerated daily by the
[progress routine](automation/daily-progress-routine.md); see the
[rules](automation/gap-prompt-rules.md). For the live gap list run
`cargo xtask progress gaps`.

**Parity baseline:** gaps are measured against ErsatzTV commit
[`0d301df`](https://github.com/ErsatzTV/legacy/commit/0d301df5e8512fdfccf3a611188d27ab1690ee31)
(`remove external dependencies (bugsnag, trakt)`). Upstream work after that
commit is out of scope until the pin in
[`automation/parity-baseline.md`](automation/parity-baseline.md) is advanced.

Ranking this run (manually updated 2026-06-14 after the Classic/HLS replay
closeout, through working tree state after `f362445`): `channels-admin-ui` is
now `Ported`, so its prompt was removed. `classic-scheduling` no longer owns a
top-three prompt because `duplicate-consecutive-shuffle-item` and
`continue-build-seam-overlap` are resolved, and the separate HLS startup replay
found during calibration was fixed and resolved under
`hls-variant-playlists-segmenter`. `ffmpeg-profile-editor` and
`ffmpeg-filter-graph` are carried over unchanged; the FFmpeg command-generation
live proof remains folded into `ffmpeg-filter-graph` because both require the
same live streaming runtime session. Slot 1 is now
`mpeg-ts-channel-streaming`: the calibration playback gate found that the
legacy MPEG-TS concat/direct branch can keep playing the same media item past a
short `PlayoutItem.OutPoint` while HLS Segmenter and the modern MPEG-TS wrapper
advance correctly.

---

## 1 - MPEG-TS channel streaming: fix legacy concat outpoint boundaries (slug `mpeg-ts-channel-streaming`)

Fix the next ErsatzTV parity gap in ErsatzRS: the legacy MPEG-TS concat/direct
branch can ignore short `PlayoutItem` outpoints and continue the same media item
instead of advancing at the playout boundary (slug `mpeg-ts-channel-streaming`,
debt `legacy-concat-outpoint-boundary`).

Use the repo workflow from `AGENTS.md` plus `skills/parity-slice.md`.
The goal is fully functional ErsatzTV parity without stopgaps.
Log any additional parity gap encountered immediately in the relevant
`docs/progress/parity/*.toml` file so it does not get lost.

Runtime evidence (local calibration, 2026-06-14): the playback gate seeded
channel 1 with a 6-second cyclic calibration timeline, 1->2->3. HLS Segmenter
and the modern MPEG-TS hybrid wrapper advanced through the clip sequence at item
boundaries, but `/iptv/channel/1.ts?mode=ts-legacy` stayed on clip 1 past the
6-second outpoint and showed clip 1 frames at ABS 00:00:04.483,
00:00:07.483, 00:00:10.483, and later. Container checks still passed, so this
is a content-boundary parity gap in the legacy concat/direct branch rather than
a TS integrity failure.

Current `ParityDebt` from `docs/progress/parity/mpeg-ts-channel-streaming.toml`:
- `legacy-concat-outpoint-boundary` (2026-06-14): "The legacy MPEG-TS
  concat/direct branch can ignore short PlayoutItem outpoints and continue the
  same media item instead of advancing at the playout boundary." Evidence:
  `scripts/e2e/tsduck-playback-gate.sh --seconds 24 --item-seconds 6 --frames
  --keep gate` seeded channel 1 as 1->2->3; HLS Segmenter and `mode=ts` frame
  sheets advanced, while `mode=ts-legacy` stayed on clip 1 past 6 seconds.

ErsatzTV source: `$ERSATZTV_SRC/ErsatzTV/Controllers/IptvController.cs`
(`GetConcatProcessByChannelNumber` / legacy `.ts` branch),
`$ERSATZTV_SRC/ErsatzTV.Core/FFmpeg/FFmpegProcessService.cs`
(`ConcatChannel`, `WrapSegmenter`, direct stream process shape), and
`$ERSATZTV_SRC/ErsatzTV.Core/FFmpeg/FFmpegStreamSelector.cs` at the pinned
commit. ErsatzRS target:
`crates/ersatzrs-web/src/transport/iptv/{stream,mod}.rs`,
`crates/ersatzrs-infra/src/playback.rs`, and `crates/ersatzrs-ffmpeg` command
builders/snapshots.

Before editing:
1. Read the pinned ErsatzTV legacy MPEG-TS concat path end to end: controller
   action, stream selector, concat playlist generation, FFmpeg process service,
   in-point/out-point handling, realtime looping, and disposal behavior.
2. Inspect the ErsatzRS legacy concat stack:
   `serve_legacy_concat_stream`, `/ffmpeg/concat/{n}`, `/ffmpeg/stream/{n}`,
   `load_active_media_path`, `ActivePlayoutItem::{content_seek_seconds,
   remaining_duration}`, command argv generation, response body cancellation,
   and any concat playlist or internal stream test coverage.
3. Reproduce the gap with the calibration clips before editing. Use
   `scripts/e2e/tsduck-playback-gate.sh --seconds 24 --item-seconds 6 --frames
   --keep gate` or an equivalent fresh DB/run, then inspect the generated
   `mode=ts-legacy`, `mode=ts`, and HLS contact sheets. A failing legacy stream
   shows the same clip number continuing past its 6-second ABS/outpoint while
   the other pipelines advance.
4. Build a Source Map for the legacy MPEG-TS current-item/outpoint path and
   compare it to ErsatzTV. Identify whether the bug lives in concat playlist
   looping, internal stream current-item lookup, FFmpeg argv duration/seek
   bounds, or response/session lifecycle. Keep any unrelated behavior as
   structured `ParityDebt`.

Implement the fix in the proven layer. The legacy concat/direct branch must
advance at each persisted `PlayoutItem` boundary, honor `InPoint`/`OutPoint` and
short item durations, preserve realtime playback, and keep the modern
`mode=ts` wrapper and HLS segmenter behavior unchanged. Do not weaken existing
streaming/session tests. Add focused regressions for the boundary behavior,
preferably at the direct internal stream/active item selection and command argv
level, plus a runtime-facing guard if the bug only appears through the concat
loop.

Runtime verification: re-seed the calibration clips and confirm the same
1->2->3 timeline advances exactly once per item on all three relevant surfaces:
`/iptv/channel/1.ts?mode=ts-legacy`, `/iptv/channel/1.ts?mode=ts`, and the HLS
segmenter playlist reached from `/iptv/channels.m3u`. The visual proof must show
no repeated clip number/timecode span across item seams. Cross-check container
integrity across the seams with `scripts/e2e/tsduck-playback-gate.sh`
(continuity-counter / PCR / PTS), and include `cvlc` sanity playback where
available.

Verify with focused streaming tests, `cargo xtask smoke`, the calibration-clip
playback check above, and `cargo xtask progress check`. Update
`docs/progress/parity/mpeg-ts-channel-streaming.toml` (close or re-scope
`legacy-concat-outpoint-boundary`), changelog, and this prompt file only when
the evidence supports the new status. Report the Source Map, diagnosis,
behavior implemented, remaining `ParityDebt`, calibration playback evidence,
TSDuck/cvlc results, verification run, and changed files.
Remember that ErsatzTV could have a less than optimal solution. If you have a
better way, please create an ADR, but only after careful review in depth.

---

## 2 - FFmpeg Profile editor: restore missing H.264 profile options and NVENC validation (slug `ffmpeg-profile-editor`)

Fix the next ErsatzTV parity gap in ErsatzRS: FFmpeg Profile editor for the
remaining profile-option and validation-warning behavior (slug
`ffmpeg-profile-editor`, debts `h264-profile-options` and
`nvenc-mpeg2-validation-warning`).

Use the repo workflow from `AGENTS.md` plus `skills/parity-slice.md`.
The goal is fully functional ErsatzTV parity without stopgaps.

Current `ParityDebt` from
`docs/progress/parity/ffmpeg-profile-editor.toml`: "when the `/ffmpeg` profile
editor's format is `h264`, its `profile` field does not expose the upstream
enum options `main`, `high`, and `high444p`; when `Nvenc` hardware
acceleration is selected and the format is changed to `mpeg-2`, the format
field does not show the upstream red validation line/warning that NVENC only
supports `h264`, `hevc`, and `av1`."

ErsatzTV source: `ErsatzTV/Pages/FFmpegProfile*.razor` at the pinned commit,
the matching profile view models, validators, and localization resources.
ErsatzRS target: `crates/ersatzrs-web/src/admin/pages/ffmpeg_profiles/`,
`crates/ersatzrs-web/src/admin/languages/`, and the profile persistence/server
function paths touched by editor save/validation.

Before editing:
1. Read the pinned ErsatzTV FFmpeg profile list/editor Razor sources, view
   model, validator, enum option sources, and localization text for profile
   format/hardware validation.
2. Inspect the current ErsatzRS FFmpeg profile editor islands, SSR fallback,
   option lists, translations, tests, and profile repository save path.
3. Build a Source Map and a Visual Source Map for `/ffmpeg`,
   `/ffmpeg/profiles/add`, and `/ffmpeg/profiles/:id`.
4. Identify remaining `ParityDebt`; it must stay visible, not hidden in prose.

Implement the coherent end-to-end slice: expose ErsatzTV's `h264` profile
options `main`, `high`, and `high444p` in the editor when the selected video
format is H.264, persist/load the selected value correctly, and show the
upstream unsupported-format validation warning when `Nvenc` is selected with
`mpeg-2`. Preserve hydrated and no-JS fallback behavior, localize any changed
visible strings in every supported culture, and keep existing startup-report
hardware availability behavior intact.

Add focused unit/server-function regressions for option generation,
validation, save/load, and any raw HTML fallback path affected by the change.
Add browser coverage for the editor state transitions that show the H.264
profile options and the NVENC MPEG-2 validation message.

Verify with focused `ersatzrs-web` FFmpeg profile tests,
`cargo xtask hydration-lint -p ersatzrs-web`, the added browser spec, and
`cargo xtask progress check`. Update
`docs/progress/parity/ffmpeg-profile-editor.toml` only when the evidence
supports the new status. Report the Source Map, Visual Source Map, behavior
implemented, remaining `ParityDebt`, verification run, and changed files.
Remember that ErsatzTV could have a less than optimal solution. If you have a
better way, please create an ADR, but only after careful review in depth.
update changelog, docs and /docs/top-three-gaps-to-fill-prompts.md when done.

---

## 3 - FFmpeg filter graph: live-prove the opacity-expression clock bindings in a real stream (slug `ffmpeg-filter-graph`)

Fix the next ErsatzTV parity gap in ErsatzRS: FFmpeg filter graph live proof for
the watermark opacity-expression clock semantics (slug `ffmpeg-filter-graph`,
debt `opacity-expression-time-variable-semantics`).

Use the repo workflow from `AGENTS.md` plus `skills/parity-slice.md`.
The goal is fully functional ErsatzTV parity without stopgaps.

Current `ParityDebt` from
`docs/progress/parity/ffmpeg-filter-graph.toml`: "Snapshot-covered clock
semantics are now implemented in native FFmpeg eval (`content_seconds =
in-point/seek + T`, `channel_seconds = channel uptime at spawn + T`,
`time_of_day_seconds = mod(seconds-since-midnight-at-spawn + T, 86400)`,
`content_total_seconds = probed MediaVersion.Duration`) and threaded through
direct IPTV `.ts`, HLS segmenter, and troubleshooting preview builders.
Remaining parity debt: no live IPTV/HLS runtime proof has yet captured these
exact wall-clock/in-point/duration bindings in an actual stream, so the feature
must stay `In progress`."

ErsatzTV source: `$ERSATZTV_SRC/ErsatzTV.FFmpeg/Filter/ComplexFilter.cs` at the
pinned commit, plus `ErsatzTV.Infrastructure/Streaming/Graphics/
OpacityExpressionHelper.cs` `GetOpacity` and `GraphicsEngine.cs` (the four
per-frame clock bindings). ErsatzRS target: `crates/ersatzrs-ffmpeg/src/filter/
simple.rs` (`WatermarkOpacityClock` identifier translation),
`crates/ersatzrs-web/src/transport/iptv/stream.rs`,
`crates/ersatzrs-infra/src/hls_worker.rs`, and the filter-graph debt file.

Before editing:
1. Read the pinned ErsatzTV opacity-expression helper, graphics engine, and
   complex-filter sources for how the four clocks are bound per frame before
   touching Rust.
2. Inspect the current ErsatzRS opacity-expression translation, the in-point /
   `MediaVersion.Duration` wiring in the IPTV `.ts` and HLS builders, existing
   `cargo test -p ersatzrs-ffmpeg` identifier/in-point/midnight-wrap coverage,
   and the prior live HLS proof captured on 2026-06-11.
3. Build a Source Map for the opacity-expression clock path across the IPTV
   `.ts`, HLS segmenter, and troubleshooting preview builders, marking where
   tests already cover behavior and where live runtime proof is absent.
4. Identify remaining `ParityDebt`; it must stay visible, not hidden in prose.

Implement only what closing the debt requires: do not weaken the existing
snapshot/unit gates. Capture live runtime proof that an actual IPTV `.ts` and
HLS stream binds `content_seconds` (in-point/seek + T), `channel_seconds`
(channel uptime at spawn + T), `time_of_day_seconds` (midnight-wrapped), and
`content_total_seconds` (probed duration) to the watermark opacity expression —
record the real spawned FFmpeg argv, the resolved expression, and observed
opacity behavior over time. If a binding diverges from ErsatzTV at runtime,
fix it; if the runtime cannot be exercised on this machine, build a documented,
reproducible proof harness that drives the same path without faking success and
keep any shortfall as structured `ParityDebt`. Decide whether any intentional
divergence from ErsatzTV (see ADR 0005 native-filter divergence) needs an ADR
update.

Fold the hardware-bound `ffmpeg-command-generation` debt
`nvenc-filter-gate-live-proof` into this same live streaming session: while a
real `.ts`/HLS transcode is running, attempt to reproduce the NVENC
encoder-with-`cuda`-hwaccel-but-no-CUDA-filter decode gate and the direct `.ts`
learned-denial retry-in-response. If no rejecting / filter-less FFmpeg build is
available, capture the strongest available local command and leave
`nvenc-filter-gate-live-proof` open as structured `ParityDebt` — never weaken
the gate to make a local build reproduce it.

Verify with focused `ersatzrs-ffmpeg` snapshots/tests, `cargo xtask smoke`, any
live IPTV/HLS commands used to capture the clock bindings, and
`cargo xtask progress check`. Update
`docs/progress/parity/ffmpeg-filter-graph.toml` (and, if its debt state
changed, `docs/progress/parity/ffmpeg-command-generation.toml`) only when the
evidence supports the new status. Report the Source Map, behavior implemented,
exact FFmpeg argv / resolved-expression proof, remaining `ParityDebt`,
verification run, and changed files.
Remember that ErsatzTV could have a less than optimal solution. If you have a
better way, please create an ADR, but only after careful review in depth.
update changelog, docs and /docs/top-three-gaps-to-fill-prompts.md when done.
