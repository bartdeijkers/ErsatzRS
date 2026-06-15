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

Manual closeout refresh (2026-06-15): the previous HLS slot,
`hls-variant-playlists-segmenter` debt
`vlc-stop-on-disconnect-session-lifecycle`, was closed with focused tests and
`cargo xtask smoke --hls-lifecycle --timeout 240`. `cargo xtask progress gaps`
now lists 8 remaining features and no longer lists HLS variant playlists /
segmenter, so the completed prompt was removed per
`docs/automation/gap-prompt-rules.md`. `ffmpeg-filter-graph` carries over as
slot 1, `settings` moves to slot 2, and slot 3 is refilled from the current
phase-focus gap list with
`multi-collections-rerun-collections-playlists-filler-presets`.

---

## 1 - FFmpeg filter graph: live-prove the opacity-expression clock bindings in a real stream (slug `ffmpeg-filter-graph`)

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

## 2 - Settings: finish admin text coverage and four-language browser proof (slug `settings`)

Fix the next ErsatzTV parity gap in ErsatzRS: Settings admin i18n coverage and
browser proof across the settings route family (slug `settings`, debt
`settings-i18n-browser-proof`).

Use the repo workflow from `AGENTS.md` plus `skills/parity-slice.md`.
The goal is fully functional ErsatzTV parity without stopgaps.

Current `ParityDebt` from `docs/progress/parity/settings.toml`: "Full
admin-route text coverage and four-language browser proof remain incomplete."
The current summary says `/settings/{ffmpeg,logging,hdhr,scanner,playout,ui,xmltv}`
are routed with ErsatzTV-shaped forms and raw HTML POST fallbacks backed by
`ConfigElement`; the persisted `pages.language` value drives the SSR shell
language and many route families, but settings still needs full text coverage
and four-language browser proof. See `plans/settings-parity-slice.md`.

ErsatzTV source: `ErsatzTV/Pages/Settings*.razor` and
`$ERSATZTV_SRC/ErsatzTV.Core/Domain/ConfigElementKey.cs` at the pinned commit.
ErsatzRS target:
`crates/ersatzrs-web/src/admin/pages/settings.rs`,
`crates/ersatzrs-web/src/admin/server_fns/settings.rs`,
`crates/ersatzrs-web/src/admin/i18n.rs`,
`crates/ersatzrs-web/src/admin/languages/`,
`crates/ersatzrs-domain/src/config.rs`,
`crates/ersatzrs-infra/src/{db,logging,xmltv}.rs`, and
`crates/ersatzrs-core/src/playout_builder.rs`.

Before editing:
1. Read the pinned ErsatzTV settings Razor sources, config keys, validators /
   save handlers, and localization resources for each settings route.
2. Inspect the current ErsatzRS settings page, server functions, language
   modules, fallback POST paths, existing settings tests, and
   `plans/settings-parity-slice.md`.
3. Build a Source Map and Visual Source Map for `/settings/ffmpeg`,
   `/settings/logging`, `/settings/hdhr`, `/settings/scanner`,
   `/settings/playout`, `/settings/ui`, and `/settings/xmltv`.
4. Identify remaining `ParityDebt`; it must stay visible, not hidden in prose.

Implement only the coherent i18n/browser-proof slice: replace any remaining
hard-coded visible settings text with typed translations in every supported
culture (`en-US`, `pl`, `pt-BR`, `nl`), keep `i18n.rs` as routing/helper glue,
preserve the existing raw HTML POST fallbacks and settings save side effects,
and keep ADR 0012's intentional Dutch-extension deviation intact. Do not
change config persistence semantics unless source review proves a real
ErsatzTV parity mismatch; if it does, document the behavior and decide whether
an ADR update is required.

Add focused tests for missing translation keys, fallback POST paths touched by
the change, and language-specific rendering where practical. Add browser
coverage that switches `pages.language` through every supported culture and
proves the settings route family renders translated labels/help/errors without
hydration errors or English fallback leakage outside intentional values such as
FFmpeg option names.

Verify with focused `ersatzrs-web` settings/i18n tests,
`cargo xtask hydration-lint -p ersatzrs-web`, the added browser spec,
`cargo xtask parity-check web`, and `cargo xtask progress check`. Update
`docs/progress/parity/settings.toml`, `plans/settings-parity-slice.md`,
changelog, and this prompt file only when the evidence supports the new status.
Report the Source Map, Visual Source Map, text coverage closed, remaining
`ParityDebt`, verification run, and changed files.
Remember that ErsatzTV could have a less than optimal solution. If you have a
better way, please create an ADR, but only after careful review in depth.
update changelog, docs and /docs/top-three-gaps-to-fill-prompts.md when done.

---

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
