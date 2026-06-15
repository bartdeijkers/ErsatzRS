<!--
generated: 2026-06-16
last_processed_commit: 5787fba2be790da6dc0f544b0f80ff7af5653e14
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

Daily reconcile (2026-06-16, window `ec6d14c..5787fba`): two commits merged.
`feat(web): prove settings i18n parity` (9914a76) already flipped `settings`
to `Ported` and removed it from the gap list; `fix(ffmpeg): use persisted
playback stream indexes` (5787fba) was recorded in
`ffmpeg-command-generation.toml` (still `In progress` — its sole open debt
`nvenc-filter-gate-live-proof` is hardware-verification-only and stays folded
into slot 1 rather than getting its own prompt). `cargo xtask progress gaps`
now lists 7 remaining features, all `In progress`. No prior pick saw scope-
closing work this window, so all three prompts carry over unchanged: slot 1
`ffmpeg-filter-graph` (phase-focus, foundational pipeline, carries the folded
`nvenc-filter-gate-live-proof` hardware proof), slot 2
`multi-collections-rerun-collections-playlists-filler-presets` (phase-focus
route-family closeout), slot 3 `scanner` (phase-focus hardening, two blocking
runtime debts).

Manual playback-continuity refresh (2026-06-15): channel 5 `calibration`
proved an open HLS Segmenter content-timeline deviation even though TSDuck
transport checks were clean. The HLS parity tracker is reopened with debt
`segmenter-content-timeline-deviations`, and the root-cause / reproduction
note lives at
[`explanation/hls-segmenter-playback-deviation-root-cause.md`](explanation/hls-segmenter-playback-deviation-root-cause.md).
This user-visible playback gap takes slot 1. The previous FFmpeg filter graph
prompt moves to slot 2, the multi-collections route-family prompt moves to
slot 3, and the Scanner prompt is deferred out of the top three.

---

## 1 - HLS Segmenter: fix unplanned playback deviations at respawn and playout seams (slug `hls-variant-playlists-segmenter`)

Fix the next ErsatzTV parity gap in ErsatzRS: HLS Segmenter decoded-content
continuity across work-ahead/realtime FFmpeg respawns and real `PlayoutItem`
seams (slug `hls-variant-playlists-segmenter`, debt
`segmenter-content-timeline-deviations`).

Use the repo workflow from `AGENTS.md` plus `skills/parity-slice.md`.
The goal is fully functional ErsatzTV parity without stopgaps.

Current `ParityDebt` from
`docs/progress/parity/hls-variant-playlists-segmenter.toml`: "HLS Segmenter
appends independently spawned FFmpeg generations by scheduled playout time and
output PTS continuity, but does not preserve or verify decoded content-time
continuity across work-ahead respawns and real PlayoutItem seams. This can
start the incoming item early or move audio pip phase while TSDuck transport
checks remain clean."

Root-cause / reproduction note:
[`docs/explanation/hls-segmenter-playback-deviation-root-cause.md`](explanation/hls-segmenter-playback-deviation-root-cause.md).
Observed evidence from channel 5 `calibration` on 2026-06-15:
`/iptv/channel/5.m3u8?mode=segmenter` produced zero TSDuck transport errors,
sync losses, continuity-counter discontinuities, PTS/DTS/PCR leaps, missing
video/audio PIDs, or scrambling errors, but audio pips moved +32 ms near a
segmenter respawn, -488 ms at the clip 3 -> clip 1 rollover, and +32 ms at the
clip 1 -> clip 2 rollover. Decoded frame 18,000 showed
`ABS 00:03:03.517` instead of expected `ABS 00:03:03.000`; rollover probes
showed clip 1 already at `ABS 00:00:00.517` by decoded second 117. The server
log had no WARN/ERROR/FFmpeg failure/no-active-playout lines during capture.

ErsatzTV source: `$ERSATZTV_SRC/ErsatzTV.Application/Streaming/
HlsSessionWorker.cs`, `$ERSATZTV_SRC/ErsatzTV.FFmpeg/
FFmpegLibraryProcessService.cs`, `$ERSATZTV_SRC/ErsatzTV.FFmpeg/
CommandGenerator.cs`, and `$ERSATZTV_SRC/ErsatzTV.FFmpeg/OutputFormat/
OutputFormatHls.cs` at the pinned commit. ErsatzRS target:
`crates/ersatzrs-infra/src/hls_worker.rs`,
`crates/ersatzrs-ffmpeg/src/{pipeline,command,options}.rs`,
`crates/ersatzrs-web/src/transport/iptv/{hls,stream,variant}.rs`,
`scripts/e2e/tsduck-playback-gate.sh`, and the HLS parity file.

Before editing:
1. Read the pinned ErsatzTV segmenter worker and FFmpeg command/HLS output
   sources for how request time, seek, segment duration, append-list state, and
   item transitions are represented.
2. Inspect the current ErsatzRS HLS worker state machine, especially
   `hls_transcode_decision`, `hls_playout_item_duration`,
   `content_seek_seconds`, `probe_output_ts_offset`, playlist trimming, and the
   generated FFmpeg argv for HLS Segmenter.
3. Build a Source Map from persisted `PlayoutItem.Start` / `Finish` ->
   `ActiveHlsItem` -> FFmpeg `-ss` / `-t` / `-output_ts_offset` ->
   appended HLS segments -> served playlist, marking where sub-second precision
   is lost and where output PTS continuity is currently treated as content
   continuity.
4. Reproduce the channel 5 calibration evidence from the root-cause note, or
   create an equivalent temporary calibration channel if the local database
   changed. Keep the resulting audio-pip and ABS/frame-counter evidence.

Implement only the coherent playback-continuity fix: preserve sub-second
request/seek positions through the worker and command builders where needed,
reconcile appended generation boundaries against decoded media time rather than
only scheduled finish or PTS offset, and make the segmenter avoid early
incoming-item starts, outgoing-item truncation, and audio-pip phase jumps at
work-ahead respawns and playout seams. Do not weaken MPEG-TS continuity
normalization, fMP4 init-cache behavior, StopOnDisconnect lifecycle handling,
or the existing TSDuck structural checks.

Add focused tests or a deterministic harness that catches this class of bug:
one-second audio pip cadence and visible ABS/frame-counter continuity must be
checked across at least one HLS respawn and at least one real `PlayoutItem`
rollover. TSDuck zero-error results are necessary but not sufficient; the test
must fail if content advances or skips by hundreds of milliseconds while the TS
container remains clean.

Verify with focused `ersatzrs-infra` / `ersatzrs-ffmpeg` tests,
`cargo xtask smoke`, the TSDuck playback gate, the calibration reproduction
steps in the root-cause note, and `cargo xtask progress check`. Update
`docs/progress/parity/hls-variant-playlists-segmenter.toml`, changelog, and
this prompt file only when the evidence supports the new status. Report the
Source Map, root cause fixed, exact FFmpeg argv before/after if it changed,
audio-pip offsets, ABS/frame-counter evidence at the five-minute mark and
rollovers, remaining `ParityDebt`, verification run, and changed files.
Remember that ErsatzTV could have a less than optimal solution. If you have a
better way, please create an ADR, but only after careful review in depth.
update changelog, docs and /docs/top-three-gaps-to-fill-prompts.md when done.

## 2 - FFmpeg filter graph: live-prove the opacity-expression clock bindings in a real stream (slug `ffmpeg-filter-graph`)

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

## 3 - Multi-collections / rerun collections / playlists / filler presets: close route-family parity (slug `multi-collections-rerun-collections-playlists-filler-presets`)

Fix the next ErsatzTV parity gap in ErsatzRS: finish and prove the media
collection route-family parity closeout for `/media/multi-collections`,
`/media/rerun-collections`, `/media/playlists`, and `/media/filler/presets`
(slug `multi-collections-rerun-collections-playlists-filler-presets`).

Use the repo workflow from `AGENTS.md` plus `skills/parity-slice.md`.
The goal is fully functional ErsatzTV parity without stopgaps.

Current status from
`docs/progress/parity/multi-collections-rerun-collections-playlists-filler-presets.toml`:
"In progress: `/media/multi-collections` now ports the ErsatzTV
multi-collection list plus add/edit/delete editor surface ... `/media/playlists`
now ports the playlist group/tree and playlist editor route ... Generated/system
playlists intentionally diverge from ErsatzTV's disabled-editor presentation:
ADR 0011 keeps scheduling semantics but renders generated rows as read-only
projections with clearer Count and Play All labels."

ErsatzTV source: `ErsatzTV/Pages/MultiCollections*.razor`,
`ErsatzTV/Pages/RerunCollections.razor`,
`ErsatzTV/Pages/RerunCollectionEditor.razor`,
`ErsatzTV/Pages/Playlists.razor`, `ErsatzTV/Pages/PlaylistEditor.razor`,
`ErsatzTV/Pages/FillerPresets.razor`, and
`ErsatzTV/Pages/FillerPresetEditor.razor` at the pinned commit. ErsatzRS
target: `crates/ersatzrs-web/src/admin/pages/media/`,
`crates/ersatzrs-web/src/admin/server_fns/media_browser.rs`,
`crates/ersatzrs-web/src/admin/server_fns/filler.rs`,
`crates/ersatzrs-web/src/api/{collections,playlists,rerun_collections,filler}.rs`,
and `crates/ersatzrs-web/src/admin/languages/`.

Before editing:
1. Read the pinned ErsatzTV Razor pages, editor models, validation/save
   handlers, delete/copy actions, and localization resources for all four
   route families.
2. Inspect the current ErsatzRS pages, server functions, raw HTML POST
   fallbacks, API handlers, language modules, existing browser smoke notes, and
   ADR 0011 generated-playlist deviation.
3. Build a Source Map and Visual Source Map for the list and editor routes,
   including add/delete/copy actions, page-size persistence, validation,
   generated/system playlist behavior, smart-collection preview, and playout
   refresh side effects.
4. Identify remaining `ParityDebt`; if the route family is already complete,
   prove it and close the tracker instead of inventing new scope.

Implement only the coherent closeout slice: fix any mismatch found in source
review or browser proof, preserve existing SSR/raw POST fallbacks and
localized text, keep ADR 0011's intentional generated-playlist deviation
explicit, and avoid unrelated media-browser refactors. Add focused tests for
any fixed persistence, validation, preview, delete/copy, or playout-refresh
behavior.

Add browser coverage that exercises the high-risk workflows across the route
family: create/edit/delete multi-collections, rerun collections, playlists and
playlist groups/items, and filler presets; verify visible geometry, hydrated
actions or fallback forms, localized labels, and no console errors. If one route
still cannot be proven, keep the exact missing workflow as structured
`ParityDebt`.

Verify with focused `ersatzrs-web` tests, `cargo xtask hydration-lint -p
ersatzrs-web`, the added browser spec or documented Playwright run,
`cargo xtask parity-check web`, and `cargo xtask progress check`. Update
`docs/progress/parity/multi-collections-rerun-collections-playlists-filler-presets.toml`,
changelog, and this prompt file only when the evidence supports the new status.
Report the Source Map, Visual Source Map, behavior closed, remaining
`ParityDebt`, verification run, and changed files.
Remember that ErsatzTV could have a less than optimal solution. If you have a
better way, please create an ADR, but only after careful review in depth.
update changelog, docs and /docs/top-three-gaps-to-fill-prompts.md when done.

---
