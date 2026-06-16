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

Manual closeout refresh (2026-06-16): the HLS slot,
`hls-variant-playlists-segmenter` debt
`segmenter-content-timeline-deviations`, was closed with sub-second seek
propagation through the worker and FFmpeg command builders plus an updated
TSDuck gate that checks decoded audio-pip cadence. `cargo xtask progress gaps`
now lists 7 remaining features and no longer lists HLS variant playlists /
segmenter, so the completed prompt was removed per
`docs/automation/gap-prompt-rules.md`. `ffmpeg-filter-graph` moves back to
slot 1, `multi-collections-rerun-collections-playlists-filler-presets` moves
to slot 2, and slot 3 is refilled with the structured Scanner runtime debts.

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

## 2 - Multi-collections / rerun collections / playlists / filler presets: close route-family parity (slug `multi-collections-rerun-collections-playlists-filler-presets`)

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

## 3 - Scanner: close real-world music-video NFO and SQLite scan-locking parity debts (slug `scanner`)

Fix the next ErsatzTV parity gap in ErsatzRS: Scanner hardening for real-world
music-video NFO leniency and concurrent SQLite scan locking (slug `scanner`,
debts `music-video-nfo-fragment-leniency` and `sqlite-scan-locking`).

Use the repo workflow from `AGENTS.md` plus `skills/parity-slice.md`.
The goal is fully functional ErsatzTV parity without stopgaps.

Current `ParityDebt` from `docs/progress/parity/scanner.toml`:
`music-video-nfo-fragment-leniency` says "Local music-video NFO parsing rejects
real-world files that ErsatzTV accepts, falling back to filename metadata."
Evidence: "ErsatzRS.log: hundreds of NFO did not contain a <musicvideo> root
element errors during music-video scans." `sqlite-scan-locking` says
"Concurrent scan and web activity can abort library scans with SQLite database
locked errors instead of serializing work like ErsatzTV." Evidence:
"ErsatzRS.log: library scan failed library_id=3/5 with SQLite code 5 database
is locked."

ErsatzTV source: `$ERSATZTV_SRC/ErsatzTV.Scanner/` at the pinned commit,
especially the local music-video scanner/NFO parsing path and the scan
coordination behavior around library scans. ErsatzRS target:
`crates/ersatzrs-scanner`, `crates/ersatzrs-infra/src/repositories/`, scanner
tests under `crates/ersatzrs-scanner/tests/`, and the scanner parity file.

Before editing:
1. Read the pinned ErsatzTV scanner source for local music-video NFO parsing,
   fallback behavior, scan transaction boundaries, and scan concurrency /
   serialization.
2. Inspect the current ErsatzRS local music-video scanner, NFO parser, provider
   sync write paths, SQLite pool/transaction usage, existing scanner tests, and
   any logs or fixtures referenced by the debt evidence.
3. Build a Source Map for music-video scan input -> NFO parse -> metadata row
   writes -> fallback metadata, and a second Source Map for web/scanner
   concurrent database access and lock handling.
4. Identify remaining `ParityDebt`; it must stay visible, not hidden in prose.

Implement only the coherent scanner hardening slice: make local music-video NFO
parsing accept the real-world fragments ErsatzTV accepts while preserving the
existing strict row-shape and fallback semantics, and make concurrent scan plus
web activity serialize/retry consistently enough to avoid aborting with SQLite
database-locked errors. Do not weaken scanner metadata parity, path hashing, or
provider enum values to make fixtures pass. If a production log sample is
needed but unavailable, create a minimal fixture from the observed failure
shape and keep any unproven runtime breadth as structured `ParityDebt`.

Add focused scanner/infra regressions for the accepted NFO fragment shape, the
metadata written from that NFO, the fallback behavior for truly invalid NFO,
and the SQLite locking scenario. If the locking fix changes shared DB behavior,
add the narrow repository or integration coverage that proves it does not hide
real errors or break MySQL feature builds.

Verify with focused `ersatzrs-scanner` and affected `ersatzrs-infra` tests,
`cargo test --workspace --all-features` if shared DB behavior changed broadly,
`cargo xtask progress check`, and `cargo xtask progress gaps`. Update
`docs/progress/parity/scanner.toml`, changelog, and this prompt file only when
the evidence supports the new status. Report the Source Map, behavior closed,
remaining `ParityDebt`, verification run, and changed files.
Remember that ErsatzTV could have a less than optimal solution. If you have a
better way, please create an ADR, but only after careful review in depth.
update changelog, docs and /docs/top-three-gaps-to-fill-prompts.md when done.
