<!--
generated: 2026-07-22
last_processed_commit: a4977f0630a21530207a2ccd621a285f4e943098
parity_baseline_commit: 0d301df5e8512fdfccf3a611188d27ab1690ee31
generator: docs/automation/daily-progress-routine.md (ersatzrs-daily-progress)
rules: docs/automation/gap-prompt-rules.md
-->

# Top three chores to do - goal prompts

Actionable paste-and-run goal prompts selected from the current progress files,
ADR index, active plans, and phase state. Phase 7A is complete, so p7b-01 leads
as the new phase focus. The two runnable Phase 7C browser closeouts follow
because they carry explicit tracked debt and are no longer dependency-blocked.

---

## 1. Build the next ErsatzRS spec chore: application service foundation (slug `application-use-case-layer`)

Use the repo workflow from AGENTS.md. This ADR-backed architectural chore is
governed by [ADR 0051](adr/0051-application-boundary-and-composition-root.md)
and [`plans/application-layer-and-video-jukebox.md`](../plans/application-layer-and-video-jukebox.md),
closing milestone **p7b-01**. Read ErsatzTV only where its application command
and query boundary informs the port; this is a deliberate ErsatzRS structural
extraction, not a UI parity slice. Deliver the complete foundation with no
stopgaps, and keep every remaining shortfall as structured `ParityDebt`.

Current primary debt:

> `core-leaks-concrete-storage-error` — "ersatzrs-core::repositories::
> RepositoryError contains sqlx::Error, so the core trait boundary depends on
> the concrete persistence implementation."

Before editing:
1. Read ADR 0051, the plan's Phase 7B section, `ARCHITECTURE.md`, and
   `cargo xtask progress milestones --id p7b-01 --full`.
2. Inspect `crates/ersatzrs-app`, `crates/ersatzrs-core/src/repositories`,
   `crates/ersatzrs-infra`, and the native web composition root.
3. Build a Source Map of the core error type, infra mappings, application
   modules, injected ports, and construction of `AppState`.

Implement the slice:
- Add documented command, query, and application-error modules to
  `ersatzrs-app`.
- Remove `sqlx::Error` from core; map concrete storage failures in infra into
  the storage-neutral core/application error vocabulary.
- Construct injected `ApplicationServices` in the native composition root.
- Advance operation coordination only as needed for this foundation; keep
  p7b-02/p7b-03 migration debt explicit.
- Preserve endpoint names, persistence shapes, side effects, and existing
  ErsatzTV behavior. Add focused error-mapping and composition tests.
- Update the parity file, milestone evidence, plan, changelog, top-gap prompts,
  phase state, and session log.

Verification:
- `cargo xtask architecture-lint`
- `./run.sh check -p ersatzrs-app -p ersatzrs-core -p ersatzrs-infra`
- focused crate tests for error mapping and the composition root
- `cargo xtask progress check`

Report the Source Map, behavior implemented, closed and remaining debt,
verification commands and outcomes, and changed files.

---

## 2. Fix the next ErsatzTV parity gap in ErsatzRS: truthful watermark preview and navigation (slug `watermark-editor`)

Use AGENTS.md plus `skills/parity-slice.md`. Close milestone **p7c-02** with
fully functional ErsatzTV parity plus the tracked ErsatzRS preview requirements,
without stopgaps. Keep any shortfall as visible `ParityDebt`.

Current debt:
- `actual-size-preview-uses-fixed-placeholder`: Actual Size hard-codes a
  32-pixel height instead of using selected-asset dimensions.
- `preview-lacks-aspect-ratio-positioning-guides`: the preview lacks clearly
  distinguishable 4:3 and 16:9 content guides.
- `preview-lacks-previous-next-watermark-navigation`: the editor lacks
  deterministic Previous/Next comparison controls that preserve edit safety.

Before editing:
1. Read `$ERSATZTV_SRC/ErsatzTV/Pages/Watermark*.razor` at the pinned baseline,
   the watermark parity file, and the p7c-02 acceptance criteria.
2. Inspect `crates/ersatzrs-web/src/admin/pages/watermarks/`, its server
   functions, persistence paths, localization files, and existing browser tests.
3. Build a Source Map and a Visual Source Map covering preview geometry,
   selected-asset metadata, editor navigation, dirty-state behavior, and the
   upstream layout.

Implement the slice:
- Make Actual Size use the selected watermark's real dimensions, or use an
  explicitly localized schematic mode if real dimensions cannot be known.
- Render visually distinct 4:3 and 16:9 safe/content boundary guides.
- Add localized Previous/Next controls with deterministic ordering and an
  explicit save/discard/cancel path so navigation never drops edits silently.
- Preserve SSR and hydration DOM shape and all current upload, validation,
  persistence, and drawvg behavior.
- Add focused desktop and mobile Playwright coverage for geometry and
  comparison navigation, then update parity/milestone/progress records.

Visual verification:
- Rebuild and serve with `./run.sh serve`; capture the editor at matching
  desktop and mobile viewports and compare geometry with the ErsatzTV source.

Verification:
- `cargo xtask parity-check web`
- `npx playwright test` for the focused watermark spec
- `cargo xtask progress check`

Report both source maps, behavior and visual differences, closed and remaining
debt, verification outcomes, and changed files.

---

## 3. Fix the next ErsatzTV parity gap in ErsatzRS: deferred search and media-browser behavior (slug `media-browsers`)

Use AGENTS.md plus `skills/parity-slice.md`. Close milestone **p7c-03** with
fully functional pinned-baseline parity and no stopgaps. If upstream evidence
shows a behavior is deliberately out of scope, record that disposition in an
ADR or the parity contract instead of leaving an ambiguous code comment.

Current debt:
- `search-thumbnail-size-plumbing-deferred`: Thumbnail cards still build their
  artwork URLs with the poster-size path.
- `see-all-detail-and-library-filter-disposition`: per-kind See All routes
  defer richer detail behavior and the library-filter combobox without a
  recorded upstream/product disposition.

Before editing:
1. Read the feature's recorded `$ERSATZTV_SRC/ErsatzTV/Pages/Movies.razor` and
   corresponding browser/search pages at the pinned baseline, plus ADR 0048
   where playback overrides touch these routes.
2. Inspect `admin/server_fns/search.rs`,
   `admin/pages/media/see_all_lists.rs`, shared browser primitives, route data,
   artwork URL construction, and existing Playwright coverage.
3. Build a Source Map and Visual Source Map for search cards, each See All
   route kind, detail navigation, filter state, and responsive layout.

Implement the slice:
- Wire Thumbnail search results through the correct artwork-size path and
  prove the URL dimensions/variant selected for every affected card kind.
- Implement upstream-backed See All detail and library-filter behavior, or
  record a deliberate non-goal with focused evidence and an ADR when required.
- Preserve stay-in-context collection/playlist actions, media-info and
  troubleshooting links, localization, SSR fallbacks, and hydration shape.
- Add focused route/search browser coverage and update the parity file,
  milestone, changelog, top-gap prompts, phase state, and session log.

Visual verification:
- Rebuild and serve with `./run.sh serve`; compare the affected search and See
  All pages against ErsatzTV at matched desktop and mobile viewports.

Verification:
- `cargo xtask parity-check web`
- `npx playwright test` for the focused search/browser spec
- `cargo xtask progress check`

Report both source maps, implemented or intentionally rejected behavior,
closed and remaining debt, verification outcomes, and changed files.
