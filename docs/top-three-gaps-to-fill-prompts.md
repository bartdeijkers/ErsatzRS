<!--
generated: 2026-06-13
last_processed_commit: e30b6db4e3ddf1a33bd52d16031fca8f13eed0ab
parity_baseline_commit: 0d301df5e8512fdfccf3a611188d27ab1690ee31
generator: manual maintainer regeneration from docs/automation/gap-prompt-rules.md
rules: docs/automation/gap-prompt-rules.md
-->

# Top three gaps to fill - goal prompts

Three paste-and-run goal prompts, each closing **one** ErsatzTV parity gap.
Auto-prioritized from `docs/progress` with this manual regeneration replacing
the completed Lucene smart-collection search prompt. Regenerated daily by the
[progress routine](automation/daily-progress-routine.md); see the
[rules](automation/gap-prompt-rules.md). For the live gap list run
`cargo xtask progress gaps`.

**Parity baseline:** gaps are measured against ErsatzTV commit
[`0d301df`](https://github.com/ErsatzTV/legacy/commit/0d301df5e8512fdfccf3a611188d27ab1690ee31)
(`remove external dependencies (bugsnag, trakt)`). Upstream work after that
commit is out of scope until the pin in
[`automation/parity-baseline.md`](automation/parity-baseline.md) is advanced.

Ranking this run: the completed Lucene smart-collection search prompt is
removed because `search-index` and `collections-smart-collections` are now
`Ported`. The two still-actionable carry-over prompts move up, and the refill
slot goes to the Phase 7 FFmpeg command-generation live-proof debt.

---

## 1 - Remote streams browser: replace the advertised 404 with the ErsatzTV admin UI (slug `remote-streams-browser`)

Fix the next ErsatzTV parity gap in ErsatzRS: Remote streams browser for
`/media/remote/streams` (slug `remote-streams-browser`).

Use the repo workflow from `AGENTS.md` plus `skills/parity-slice.md`.
The goal is fully functional ErsatzTV parity without stopgaps.

Current gap from `docs/progress/parity/remote-streams-browser.toml`: "Not
started: nav link advertised in the drawer 404s today. Remote-stream domain
entities exist; no UI yet." `docs/progress/routes.toml` also records
`/media/remote/streams` as `routed = false` with backing "Remote-stream
entities exist; no UI yet."

ErsatzTV source: `ErsatzTV/Pages/RemoteStreams*.razor`. ErsatzRS target:
`crates/ersatzrs-web/src/admin/pages/media/`, the matching server functions/API
fallbacks, route registration, route/hydration progress docs, and
`crates/ersatzrs-web/src/admin/languages/`.

Before editing:
1. Read the pinned ErsatzTV remote-stream list/editor Razor sources and any
   handlers/validators they call for URL, name, grouping, and save/delete
   behavior.
2. Inspect the current ErsatzRS remote-stream domain/repository shape, media
   scanner persistence, admin nav route registration, media-page conventions,
   raw POST fallback patterns, translations, and tests.
3. Build a Source Map and a Visual Source Map for ErsatzTV's remote-stream
   list/editor routes and the local `/media/remote/streams` route family.
4. Identify remaining `ParityDebt`; it must stay visible, not hidden in prose.

Implement the coherent end-to-end slice: route `/media/remote/streams` and the
needed add/edit/delete surfaces, backed by real persisted remote-stream rows
and ErsatzTV-equivalent fields, validation, labels, actions, sorting/search or
paging behavior, and side effects. Preserve SSR/browser-safe fallbacks for
create/save/delete, wire the route into admin navigation without 404s, and add
translations for every supported culture. If the current domain model lacks a
field ErsatzTV exposes, port the persistence/server-function path or record a
structured `ParityDebt`; do not silently omit it.

Add focused server-function/API tests plus browser coverage for list, add,
edit, delete/cancel, validation failure, and no-JS fallback behavior.

Visual verification: serve via `./run.sh serve` after rebuilding Leptos assets,
capture same-viewport screenshots against the ErsatzTV reference for list and
editor states, and compare action-bar geometry, table/form layout, validation
messages, empty state, and mobile behavior.

Verify with focused `ersatzrs-web` remote-stream tests,
`cargo xtask hydration-lint -p ersatzrs-web`, the added or updated Playwright
spec via `npx playwright test ...`, `cargo xtask parity-check web`, and
`cargo xtask progress check`. Update
`docs/progress/parity/remote-streams-browser.toml`, `docs/progress/routes.toml`,
and `docs/progress/hydration.toml` only when the evidence supports the new
status. Report the Source Map, Visual Source Map, behavior implemented,
remaining `ParityDebt`, verification run, and changed files.
Remember that ErsatzTV could have a less than optimal solution. If you have a
better way, please create an ADR, but only after careful review in depth.
update changelog, docs and /docs/top-three-gaps-to-fill-prompts.md when done.

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

ErsatzTV source: `ErsatzTV/Pages/FFmpegProfile*.razor`, the matching profile
view models, validators, and localization resources. ErsatzRS target:
`crates/ersatzrs-web/src/admin/pages/ffmpeg_profiles/`,
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

## 3 - FFmpeg command generation: live-proof the CUDA-filter gate and direct TS retry path (slug `ffmpeg-command-generation`)

Fix the next ErsatzTV parity gap in ErsatzRS: FFmpeg command-generation live
proof for the remaining hardware-decode debt (slug
`ffmpeg-command-generation`, debt `nvenc-filter-gate-live-proof`).

Use the repo workflow from `AGENTS.md` plus `skills/parity-slice.md`.
The goal is fully functional ErsatzTV parity without stopgaps.

Current `ParityDebt` from
`docs/progress/parity/ffmpeg-command-generation.toml`: "The CUDA-filter decode
gate and the encoder-only software-graph demotion are verified by unit tests
and argv snapshots only; live proof requires an FFmpeg build with NVENC
encoders + `cuda` hwaccel but no CUDA filter kernels, and no such build was
available on 2026-06-12 (both local builds ship all three CUDA filters). The
IPTV .ts learned-denial retry likewise has unit/snapshot coverage plus a
passing `cargo xtask smoke` happy path, but its retry-in-response has not been
observed live against real rejecting hardware the way the HLS slice was on
2026-06-11."

ErsatzTV source: `$ERSATZTV_SRC/ErsatzTV.FFmpeg/CommandGenerator.cs` and the
hardware capability / FFmpeg process paths it calls. ErsatzRS target:
`crates/ersatzrs-ffmpeg`, `crates/ersatzrs-infra/src/ffmpeg_capabilities.rs`,
`crates/ersatzrs-web/src/transport/iptv/stream.rs`, health/troubleshooting
surfaces that expose learned denials, and the progress debt file.

Before editing:
1. Read the pinned ErsatzTV command generator and hardware-capability sources
   before changing Rust.
2. Inspect the current ErsatzRS command builders, CUDA/NVENC filter gates,
   learned-denial persistence, direct IPTV `.ts` retry path, snapshots, and
   runtime logs from the prior HLS proof.
3. Build a Source Map for the NVENC/CUDA capability gates and direct `.ts`
   denial retry path, including where tests already cover behavior and where
   live proof is still absent.
4. Identify remaining `ParityDebt`; it must stay visible, not hidden in prose.

Implement the coherent end-to-end slice only if live proof exposes a real
runtime mismatch. Do not weaken the existing unit/snapshot gates just to make a
local FFmpeg build reproduce the missing-filter path. If the local machine
still cannot provide an FFmpeg build with NVENC encoders plus `cuda` hwaccel
but without CUDA filters, create a documented, reproducible proof harness or
fixture that exercises the same runtime classification path without faking
success, and keep any hardware-only shortfall as structured `ParityDebt`.

For direct IPTV `.ts`, capture the real spawned command, FFmpeg stderr
classification, persisted denial row, retry command, HTTP response status, and
stream bytes when hardware rejects decode at runtime. If no rejecting hardware
is available, prove the boundary with the strongest available local command
and leave the live-hardware gap open.

Verify with focused `ersatzrs-ffmpeg` snapshots/tests, focused infra/web tests
for denial persistence and direct `.ts` retry, `cargo xtask smoke`, any
hardware-specific live commands used, and `cargo xtask progress check`. Update
`docs/progress/parity/ffmpeg-command-generation.toml` only when the evidence
supports the new status. Report the Source Map, behavior implemented, exact
FFmpeg stderr/argv proof, remaining `ParityDebt`, verification run, and
changed files.
Remember that ErsatzTV could have a less than optimal solution. If you have a
better way, please create an ADR, but only after careful review in depth.
update changelog, docs and /docs/top-three-gaps-to-fill-prompts.md when done.
