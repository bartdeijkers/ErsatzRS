<!--
generated: 2026-06-10
last_processed_commit: 52e102896ce17e0b776b0a8e83763eb19d7a96db
parity_baseline_commit: 0d301df5e8512fdfccf3a611188d27ab1690ee31
generator: docs/automation/daily-progress-routine.md
rules: docs/automation/gap-prompt-rules.md
-->

# Top three gaps to fill — goal prompts

Three paste-and-run goal prompts, each closing **one** ErsatzTV parity gap.
Auto-prioritized from `docs/progress` (phase focus first, then `In progress`
before `Not started`, foundational before leaf UI). Regenerated daily by the
[progress routine](automation/daily-progress-routine.md); see the
[rules](automation/gap-prompt-rules.md). For the live gap list run
`cargo xtask progress gaps`.

**Parity baseline:** gaps are measured against ErsatzTV commit
[`0d301df`](https://github.com/ErsatzTV/ErsatzTV/commit/0d301df5e8512fdfccf3a611188d27ab1690ee31)
(`remove external dependencies (bugsnag, trakt)`). Upstream work after that
commit is out of scope until the pin in
[`automation/parity-baseline.md`](automation/parity-baseline.md) is advanced.

Ranking this run (window `c5162a2..52e1028`): backend/transport work dominated —
the feature commits closed colorspace normalization (a new `Ported`
`hardware-colorspace-normalization` slug) and advanced HLS segmenter / fMP4
init-segment lifecycle, MPEG-TS streaming, IPTV direct-stream cancellation, and
NVENC CUDA overlay/device init. None of those features are named in
`phase.toml` `next_steps`, so the three picks are the **phase-focus** gaps that
are still `In progress` and saw no commits this window:
`support-pages` (health/troubleshooting is the explicit current focus, carried
over from the prior run), `settings` (i18n full coverage is named in
`next_steps`; foundational config consumed app-wide), then `multi-collections…`
(phase-named leaf media UI, alphabetical/leaf tiebreaker, carried over). HLS
(prior #2) re-ranks off after this window's heavy progress.

---

## 1 — Support pages: Health / Logs / Troubleshooting (slug `support-pages`)

Fix the next ErsatzTV parity gap in ErsatzRS: Support pages — routes
`/system/health`, `/system/logs`, `/system/troubleshooting` (slug
`support-pages`). *(Carried over from the prior run — no commits this window.)*

Use the repo workflow from `AGENTS.md` plus `.codex/skills/parity-slice.md`.
The goal is fully functional ErsatzTV parity without stopgaps.

Current remaining ParityDebt (quoted from `docs/progress/parity/support-pages.toml`):
"Dynamic nav badge counts and full log search/sort paging remain future parity
work." ErsatzTV source: `ErsatzTV/Pages/Index.razor`, `ErsatzTV/Pages/Logs.razor`,
`ErsatzTV/Pages/Troubleshooting/`. ErsatzRS target:
`crates/ersatzrs-web/src/admin/pages/system/`,
`crates/ersatzrs-web/src/admin/languages/`, `tests/e2e/support-pages.spec.ts`.

Before editing:
1. Read the ErsatzTV source above under `$ERSATZTV_SRC/` (at pinned commit
   `0d301df5e8512fdfccf3a611188d27ab1690ee31`), focusing on the log table
   paging/sort and the nav badge count sources.
2. Inspect the current ErsatzRS `system/` pages, routes, i18n languages, and
   `tests/e2e/support-pages.spec.ts`.
3. Build a Source Map (UI/pages, view models, handlers, repository/query paths,
   side effects, ErsatzRS target modules, intentional deviations) and a Visual
   Source Map for the touched admin routes.
4. Identify remaining `ParityDebt`; it must stay visible, not hidden in prose.

Implement the coherent end-to-end slice: close log search/sort paging and the
dynamic nav badge counts to match ErsatzTV fields, actions, labels, validation,
persistence, and visible layout. Preserve SSR/browser-safe fallbacks, follow
`TEST.md` hydration rules, localize every new visible string across
`en-us`/`pl`/`pt-br`/`nl`, add focused tests, and update the parity file.

Visual verification: serve via `./run.sh serve` after rebuilding Leptos assets,
open each touched route in a fresh context, and compare same-viewport geometry
against the ErsatzTV reference (table column widths, pager placement, badge
positions) — not just DOM presence.

Verify with `cargo xtask parity-check web` and `cargo xtask progress check`.
Report Source Map, Visual Source Map, behavior implemented, remaining
`ParityDebt`, verification run, and changed files.

---

## 2 — Settings i18n full coverage (slug `settings`)

Fix the next ErsatzTV parity gap in ErsatzRS: Settings (FFmpeg / Logging /
HDHomeRun / Scanner / Playout / UI / XMLTV) — routes
`/settings/{ffmpeg,logging,hdhr,scanner,playout,ui,xmltv}` (slug `settings`).

Use the repo workflow from `AGENTS.md` plus `.codex/skills/parity-slice.md`.
The goal is fully functional ErsatzTV parity without stopgaps.

Current parity state (from `docs/progress/parity/settings.toml`): the settings
routes are ported with ErsatzTV-shaped forms, raw HTML POST fallbacks backed by
`ConfigElement`, the ported save side effects (logging filter reload, FFmpeg
runtime/capability refresh, report-folder creation, subtitle extraction
queueing, playout rebuild queueing, XMLTV refresh), and a `FFmpegLocator` port
that resolves and persists the ffmpeg/ffprobe paths at startup and on save. The
persisted `pages.language` value drives the SSR shell and typed translations
across a large but not yet complete set of admin routes; ErsatzRS intentionally
adds an `nl` culture beyond ErsatzTV's en/pl/pt-br (ADR 0012). Remaining
ParityDebt quoted from the summary: "Full admin-route text coverage and
four-language browser proof remain active i18n `ParityDebt`; see
`plans/settings-parity-slice.md`." Also: "the FFmpeg settings inputs are still
`required`, so empty-means-auto-detect is exercised only by startup/the server
fn, not the form UI." ErsatzTV source: `ErsatzTV/Pages/Settings*.razor`,
`$ERSATZTV_SRC/ErsatzTV.Core/Domain/ConfigElementKey.cs`. ErsatzRS target:
`crates/ersatzrs-web/src/admin/pages/settings.rs`,
`crates/ersatzrs-web/src/admin/server_fns/settings.rs`,
`crates/ersatzrs-web/src/admin/i18n.rs`,
`crates/ersatzrs-web/src/admin/languages/`.

Before editing:
1. Read the ErsatzTV `Settings*.razor` pages and `ConfigElementKey.cs` under
   `$ERSATZTV_SRC/` (at pinned commit
   `0d301df5e8512fdfccf3a611188d27ab1690ee31`), enumerating every visible label,
   helper, option, and validation message on each settings tab.
2. Inspect the current ErsatzRS settings pages, server fns, `i18n.rs` routing
   glue, the per-culture `languages/` modules, and `plans/settings-parity-slice.md`.
3. Build a Source Map and a Visual Source Map for each settings route; list the
   admin routes still missing four-language text coverage.
4. Identify remaining `ParityDebt`; it must stay visible, not hidden in prose.

Implement the coherent end-to-end slice: close the remaining admin-route text
coverage so every visible settings string is localized across
`en-us`/`pl`/`pt-br`/`nl`, keep `i18n.rs` as routing/helper glue (ADR 0012's
translation boundary, not a translation dump), and — if in scope — relax the
`required` FFmpeg inputs so empty-means-auto-detect is exercised from the form.
Preserve SSR/browser-safe fallbacks, follow `TEST.md` hydration rules, add
focused tests, and update the parity file (and `plans/settings-parity-slice.md`).

Visual verification: serve via `./run.sh serve` after rebuilding Leptos assets,
open each touched settings route in each of the four cultures, and compare
same-viewport geometry and translated text against the ErsatzTV reference (field
labels, option lists, helper/validation text, clipped/overflowing strings).

Verify with `cargo xtask parity-check web` and `cargo xtask progress check`.
Report Source Map, Visual Source Map, behavior implemented, remaining
`ParityDebt`, verification run, and changed files.

---

## 3 — Multi-collections / Rerun collections / Playlists / Filler presets (slug `multi-collections-rerun-collections-playlists-filler-presets`)

Fix the next ErsatzTV parity gap in ErsatzRS: Multi-collections / Rerun
collections / Playlists / Filler presets — routes `/media/multi-collections`,
`/media/rerun-collections`, `/media/playlists`, and `/media/filler/presets`
(slug `multi-collections-rerun-collections-playlists-filler-presets`).
*(Carried over from the prior run — no commits this window.)*

Use the repo workflow from `AGENTS.md` plus `.codex/skills/parity-slice.md`.
The goal is fully functional ErsatzTV parity without stopgaps.

Current parity state (from
`docs/progress/parity/multi-collections-rerun-collections-playlists-filler-presets.toml`):
the list/editor surfaces are largely ported, including persisted page sizes,
HTML POST fallback paths, playout refresh side effects, playlist item actions,
canonical Classic scheduler-backed preview data, and persisted-language labels.
Generated/system playlists intentionally diverge from ErsatzTV's disabled-editor
presentation under ADR 0011 (read-only projections with clearer Count and Play
All labels).

ErsatzTV source: `ErsatzTV/Pages/MultiCollections*.razor`,
`ErsatzTV/Pages/RerunCollections.razor`,
`ErsatzTV/Pages/RerunCollectionEditor.razor`,
`ErsatzTV/Pages/Playlists.razor`, `ErsatzTV/Pages/PlaylistEditor.razor`,
`ErsatzTV/Pages/FillerPresets.razor`, and
`ErsatzTV/Pages/FillerPresetEditor.razor`. ErsatzRS target:
`crates/ersatzrs-web/src/admin/pages/media/` and
`crates/ersatzrs-web/src/admin/languages/`.

Before editing:
1. Read the ErsatzTV Razor/editor sources above under `$ERSATZTV_SRC/` (at
   pinned commit `0d301df5e8512fdfccf3a611188d27ab1690ee31`), focusing on any
   remaining route-family gaps, generated playlist behavior, list actions,
   dialogs, preview data, and delete/save side effects.
2. Inspect the current ErsatzRS media pages, server functions, i18n language
   modules, fallback routes, and focused media-page tests.
3. Build a Source Map and Visual Source Map for the touched route(s), including
   tables, action bars, editors, dialogs, pager geometry, and known ADR 0011
   deviation.
4. Identify remaining `ParityDebt`; it must stay visible, not hidden in prose.

Implement the coherent end-to-end slice: close the next remaining route-family
gap while preserving ErsatzTV fields, actions, labels, validation, persistence,
delete/copy/edit side effects, playout refresh behavior, and visible layout.
Keep SSR/browser-safe fallback paths working, follow `TEST.md` hydration rules,
localize every new visible string across `en-us`/`pl`/`pt-br`/`nl`, add focused
tests, and update the parity file.

Visual verification: serve via `./run.sh serve` after rebuilding Leptos assets,
open each touched route in a fresh context, and compare same-viewport geometry
against ErsatzTV references or the Razor-derived contract. Cover action bars,
tables/forms, dialogs, pagers, clipped text, and overflow.

Verify with focused media-page tests, `cargo xtask parity-check web`, and
`cargo xtask progress check`. Report Source Map, Visual Source Map, behavior
implemented, remaining `ParityDebt`, verification run, and changed files.
