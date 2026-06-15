<!--
generated: 2026-06-15
last_processed_commit: 9e0d59e6bf32c5d19aeee35b215fece5813f3ea5
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

Manual closeout refresh (2026-06-15): the previous slot 1,
`mpeg-ts-channel-streaming` debt `legacy-concat-outpoint-boundary`, was closed
with focused stream tests and the calibration playback gate. `cargo xtask
progress gaps` now lists 10 remaining features and no longer lists MPEG-TS
channel streaming, so the completed prompt was removed per
`docs/automation/gap-prompt-rules.md`. `ffmpeg-profile-editor` and
`ffmpeg-filter-graph` carry over as slots 1 and 2. Slot 3 is refilled from the
current gap list with `hls-variant-playlists-segmenter`, debt
`vlc-stop-on-disconnect-session-lifecycle`.

---

## 1 - FFmpeg Profile editor: restore missing H.264 profile options and NVENC validation (slug `ffmpeg-profile-editor`)

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

---

## 3 - HLS variant playlists / segmenter: prove StopOnDisconnect session lifecycle (slug `hls-variant-playlists-segmenter`)

Fix the next ErsatzTV parity gap in ErsatzRS: HLS Segmenter stop-on-disconnect
session lifecycle proof and any required fixes (slug
`hls-variant-playlists-segmenter`, debt
`vlc-stop-on-disconnect-session-lifecycle`).

Use the repo workflow from `AGENTS.md` plus `skills/parity-slice.md`.
The goal is fully functional ErsatzTV parity without stopgaps.

Current `ParityDebt` from
`docs/progress/parity/hls-variant-playlists-segmenter.toml`: "For channels
configured with `IdleBehavior = StopOnDisconnect`, VLC playback shutdown must
drain every channel transcode session after the configured segmenter idle
timeout and must not leave the `/channels` active-session state or FFmpeg
process tree running past that timeout. This must cover HLS segmenter playback
and the MPEG-TS hybrid wrapper path that starts/touches the HLS segmenter,
while preserving the already-ported direct `.ts` response-body disposal
behavior."

ErsatzTV source: `$ERSATZTV_SRC/ErsatzTV.FFmpeg/OutputFormat/OutputFormatHls.cs`,
`$ERSATZTV_SRC/ErsatzTV/Controllers/IptvController.cs`,
`$ERSATZTV_SRC/ErsatzTV.Application/Streaming/HlsSessionWorker.cs`, and
`$ERSATZTV_SRC/ErsatzTV.Core/FFmpeg/FFmpegStreamSelector.cs` at the pinned
commit. ErsatzRS target:
`crates/ersatzrs-web/src/transport/iptv/{hls,variant,streaming_specs,access_token,playlist}.rs`,
`crates/ersatzrs-core/src/hls.rs`,
`crates/ersatzrs-infra/src/{hls,hls_worker,playback}.rs`, and
`crates/ersatzrs-ffmpeg`.

Before editing:
1. Read the pinned ErsatzTV HLS lifecycle path end to end: channel idle
   behavior settings, `StartFFmpegSessionHandler` timeout loading /
   KeepRunning bypass, worker touch/cancel behavior, playlist and segment file
   serving, and direct `.ts` response disposal.
2. Inspect the ErsatzRS HLS registry/worker timeout loop, `touch` calls for
   `/iptv/channel/{n}.m3u8?mode=segmenter`, `/iptv/session/{n}/hls.m3u8`,
   segment file serving, the MPEG-TS hybrid wrapper path, `/channels`
   active-session state, and existing response-body disposal tests.
3. Build a Source Map for session creation, touch, idle timeout, cancellation,
   FFmpeg process cleanup, registry removal, and `/channels` active-session UI
   state. Identify whether the gap is missing proof, missing touch semantics,
   stale registry state, or a leaked FFmpeg process tree.
4. Reproduce with a `StopOnDisconnect` channel and the configured
   `ffmpeg.segmenter.timeout_seconds` value. Include HLS segmenter playback and
   the modern MPEG-TS wrapper (`/iptv/channel/{n}.ts?mode=ts`) because it
   starts/touches the HLS segmenter.

Implement only the lifecycle behavior needed to match ErsatzTV. A disconnected
client must stop touching the session, the worker must cancel after the
configured timeout for `StopOnDisconnect`, `KeepRunning` must still bypass that
timeout, FFmpeg children must be reaped, the registry must remove the channel,
and `/channels` must stop reporting the session active. Preserve the already
covered direct `.ts` response-body disposal behavior and do not weaken
continuity or playlist-window tests.

Add focused regressions around worker idle timeout/touch behavior, registry
removal, and `/channels` active-session state where practical. If live VLC
proof is environment-sensitive, add a deterministic local client harness that
opens the same playlist/segment paths, disconnects, waits the configured
timeout plus a small grace window, and checks registry/process cleanup.

Verify with focused HLS/session tests, `cargo xtask smoke`, a live
StopOnDisconnect playback/disconnect run for HLS Segmenter and `mode=ts`, and
`cargo xtask progress check`. Update
`docs/progress/parity/hls-variant-playlists-segmenter.toml`, changelog, and
this prompt file only when the evidence supports the new status. Report the
Source Map, behavior implemented, timeout value used, process/session cleanup
evidence, remaining `ParityDebt`, verification run, and changed files.
Remember that ErsatzTV could have a less than optimal solution. If you have a
better way, please create an ADR, but only after careful review in depth.
update changelog, docs and /docs/top-three-gaps-to-fill-prompts.md when done.
