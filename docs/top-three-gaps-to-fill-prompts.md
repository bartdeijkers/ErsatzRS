<!--
generated: 2026-06-17
last_processed_commit: cfc502479b32b6b968d8cb237cfa05073e075cc4
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

Daily reconcile (2026-06-17, window `5787fba..cfc5024`): four commits merged,
all in the HLS variant playlists / segmenter family. `fix(hls): preserve
sub-second segmenter seeks` (6694ceb) propagated full-precision request-time /
in-point seek offsets as `Duration` through active-item selection, the worker
FFmpeg command, the shared `StreamSeek` step, direct `.ts`, and troubleshooting
previews, closing the last open content-timeline debt; `fix: limit active hls
transcodes` (cfc5024) added the ErsatzRS-specific
`ffmpeg.segmenter.active_transcode_limit` (ADR 0016, default 2). With every
structured debt `resolved` at `cfc5024`, `hls-variant-playlists-segmenter` was
`Ported` and dropped out of `cargo xtask progress gaps`; its over-budget summary
was trimmed to current-state-only in this pass. (It was reopened the same day by
the ADR 0016 revision — see the 2026-06-17 addendum below.) The two docs commits
(f67a502, 2b98cb0) recorded that work and the prior reconcile. No other parity
feature changed.

`cargo xtask progress gaps` listed 7 remaining features at `cfc5024`, all `In
progress` (8 after the 2026-06-17 addendum below). None of the three
carried-over picks saw any commits this window, so per the carry-over rule they
were kept unchanged in the daily reconcile: slot 1 `ffmpeg-filter-graph`
(phase-focus, foundational pipeline), slot 2
`multi-collections-rerun-collections-playlists-filler-presets`, slot 3
`scanner`. The manual closeout addendum below replaces the now-completed slot
1. The root-cause note for the now-closed HLS deviation remains at
[`explanation/hls-segmenter-playback-deviation-root-cause.md`](explanation/hls-segmenter-playback-deviation-root-cause.md).

**2026-06-17 addendum (manual reconcile; design work not yet committed).** Two
ErsatzRS-specific streaming decisions were recorded, both tracked through
[`plans/hls-transcode-capacity-and-admission.md`](../plans/hls-transcode-capacity-and-admission.md):

- **ADR 0016 revised** from a soft `active_transcode_limit` cap (whose
  recently-accessed bypass was unbounded) into a hard capacity ceiling plus an
  admission/preemption ladder. This reopened `hls-variant-playlists-segmenter`
  (`In progress`) with debt `active-transcode-limit-not-a-real-ceiling`.
- **ADR 0017 added** — the default FFmpeg profile tuned for smooth 1080p60 /
  AC-3 6ch streaming on CPU — adding debts `default-profile-1080p60-retune` and
  `ac3-segmenter-max-delay` to `ffmpeg-command-generation`.

`cargo xtask progress gaps` now lists **8** features. These three debts are
ErsatzRS hardening/deviations carrying their own ADRs and a dedicated plan, not
gaps against the pinned ErsatzTV baseline, so per the one-ErsatzTV-parity-gap
rule they do not consume a goal-prompt slot; the three parity picks below are
unchanged. The next routine run will fold this addendum into the regenerated
narrative.

**2026-06-17 closeout addendum (manual gap #1).** The
`ffmpeg-filter-graph` prompt was completed in this pass: opacity-expression
still images now use looped inputs, direct `.ts` and HLS segmenter live proofs
captured the resolved clock expressions and decoded black -> red -> black
opacity transitions, and `ffmpeg-filter-graph` is now `Ported`. The hardware
`ffmpeg-command-generation` debt `nvenc-filter-gate-live-proof` remains open
because this machine still lacks an encoder-only/no-CUDA-filter FFmpeg build.
Per the completed-scope replacement rule, slot 1 is refilled with
`classic-scheduling`; slots 2 and 3 carry over. `cargo xtask progress gaps`
now lists **7** features after this closeout.

**2026-06-17 closeout addendum (manual gap #1 Classic).** The
`classic-scheduling` prompt was completed in this pass: imported
`ProgramScheduleItem` PreRoll/MidRoll/PostRoll/Tail/Fallback filler IDs now
round-trip through the repository, Classic SQL reference loading resolves those
filler presets and media, and the Classic engine schedules per-item pre/post
filler, no-chapter mid-roll as post-roll, tail filler up to the next fixed item,
fallback filler for the remaining gap, and existing fill-group state. Focused
core and infra tests prove the imported SQL and scheduling paths, and
`classic-scheduling` is now `Ported`. Per the completed-scope replacement rule,
slot 1 is refilled with
`schedules-templates-decorations-deco-templates`; slots 2 and 3 carry over.
`cargo xtask progress gaps` now lists **6** features after this closeout.

**2026-06-17 closeout addendum (manual gap #1 Scheduling admin).** The
`schedules-templates-decorations-deco-templates` prompt was completed in this
pass: the closeout Source Map and Visual Source Map were recorded in
`plans/scheduling-route-family-parity-closeout.md`, durable Chromium browser
coverage now creates and inspects schedules, blocks, templates, decos, and deco
templates through `tests/e2e/scheduling-route-family.spec.ts`, and the tracker
now records no remaining `ParityDebt` for this route family. Per the
completed-scope replacement rule, slot 1 is refilled with `support-pages`;
slots 2 and 3 carry over. `cargo xtask progress gaps` now lists **5** features
after this closeout.

**2026-06-17 closeout addendum (manual gap #1 Support pages).** The
`support-pages` prompt was completed in this pass: the closeout Source Map and
Visual Source Map were recorded in
`plans/support-pages-parity-closeout.md`, hydrated Support nav badges now fetch
and render warning/error counts, `/system/logs` now carries search, Timestamp /
Level sort, persisted page size, and pager state through the server function,
and durable Chromium coverage in `tests/e2e/support-pages.spec.ts` proves the
health, logs, troubleshooting, sequential schedule validation, block playout,
and playback fallback surfaces without console/runtime errors. The
`support-pages` tracker is now `Ported`. `cargo xtask progress gaps` now lists
**4** features after this closeout; the remaining FFmpeg/HLS items are
ErsatzRS-specific hardening/deviation work from the addendum above, so only two
actionable pinned-baseline ErsatzTV parity prompts remain below.

---

## 1 - Multi-collections / rerun collections / playlists / filler presets: close route-family parity (slug `multi-collections-rerun-collections-playlists-filler-presets`)

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

## 2 - Scanner: close real-world music-video NFO and SQLite scan-locking parity debts (slug `scanner`)

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
