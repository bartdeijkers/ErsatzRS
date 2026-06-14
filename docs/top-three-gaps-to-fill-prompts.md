<!--
generated: 2026-06-14
last_processed_commit: 55034bd1cf9c31361c9b00c3c3dd63dbf095933e
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

Ranking this run (window `3617358..55034bd`, manually updated 2026-06-14 after
the remote-streams slice): `remote-streams-browser` is now `Ported`, so its
prompt was removed. `ffmpeg-profile-editor` remains a focused UI parity gap,
and the FFmpeg command-generation live proof remains folded into
`ffmpeg-filter-graph` because both require the same live streaming runtime
session. The refill is `channels-admin-ui`, the highest remaining
`cargo xtask progress gaps` entry after remote streams completed; its next task
is to audit why the feature still reports `In progress`, either closing it with
evidence or recording precise remaining `ParityDebt`.

---

## 1 - Channels admin UI: close or precisely debt the remaining in-progress status (slug `channels-admin-ui`)

Fix the next ErsatzTV parity gap in ErsatzRS: Channels admin UI
(`channels-admin-ui`).

Use the repo workflow from `AGENTS.md` plus `skills/parity-slice.md`.
The goal is fully functional ErsatzTV parity without stopgaps.

Current state from `docs/progress/parity/channels-admin-ui.toml`: the list,
delete/show-disabled toggle, generated/stored logos, stop-transcode action,
full SSR multipart add/edit form, persisted labels/options, language selector,
and music-video credits template behavior are already described as ported, but
the feature still reports `In progress` and has no explicit structured debt.

ErsatzTV source: `ErsatzTV/Pages/Channel*.razor`,
`ErsatzTV/ViewModels/ChannelEditViewModel.cs`,
`ErsatzTV/Validators/ChannelEditViewModelValidator.cs`, and
`ErsatzTV/Locals/Pages/Channels*.resx` at the pinned commit. ErsatzRS target:
`crates/ersatzrs-web/src/admin/pages/channels/`,
`crates/ersatzrs-web/src/admin/languages/`,
`crates/ersatzrs-core/src/repositories/channel.rs`, and
`crates/ersatzrs-infra/src/repositories/channel.rs`.

Before editing:
1. Read the pinned ErsatzTV channel list/editor sources, validators,
   localization resources, logo handling, stop-transcode paths, and music-video
   credits template behavior.
2. Inspect the current ErsatzRS channels implementation, repository save/load
   paths, SSR multipart fallback, i18n coverage, tests, and progress docs.
3. Build a Source Map and a Visual Source Map for `/channels`,
   `/channels/add`, and `/channels/:id`.
4. Identify why the feature is still `In progress`. If parity is complete,
   collect evidence and mark it `Ported`; if not, record concrete structured
   `ParityDebt` instead of leaving the status ambiguous.

Implement only the missing behavior proven by the audit. Do not rewrite already
ported channel surfaces. Add focused tests or browser coverage for any missing
list/editor/save/validation/hydration behavior you change, and update
translations for every supported culture when visible text changes.

Verify with focused `ersatzrs-web` channel tests,
`cargo xtask hydration-lint -p ersatzrs-web`, any added or updated Playwright
spec, `cargo xtask parity-check web`, and `cargo xtask progress check`. Update
`docs/progress/parity/channels-admin-ui.toml`, route/hydration docs, changelog,
and this prompt file only when the evidence supports the new status. Report the
Source Map, Visual Source Map, behavior implemented, remaining `ParityDebt`,
verification run, and changed files.
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
