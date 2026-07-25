<!--
generated: 2026-07-25
last_processed_commit: 436a94029d9ce37217ae310d930e665dea8ee238
parity_baseline_commit: 0d301df5e8512fdfccf3a611188d27ab1690ee31
generator: docs/automation/daily-progress-routine.md (ersatzrs-daily-progress)
rules: docs/automation/gap-prompt-rules.md
-->

# Top three chores to do - goal prompts

Goal prompts selected from the current progress files, ADR index, active plans,
and Phase 7C state. P7C-01 is complete; p7c-02 and p7c-03 are runnable, while
the third prompt records the dependent p7c-04 closeout that follows them.

---

## 1. Fix the next maintainer-requested parity chore: truthful watermark preview and comparison (slug `watermark-editor`)

Use AGENTS.md plus `skills/parity-slice.md`. Close milestone **p7c-02** and all
three open debts in
[`docs/progress/parity/watermark-editor.toml`](progress/parity/watermark-editor.toml).
Preserve the ported ErsatzTV watermark list/editor, validation, upload, and
drawvg placement behavior while making the preview's size and framing claims
truthful and adding a safe comparison workflow.

Current debt:

> `actual-size-preview-uses-fixed-placeholder`,
> `preview-lacks-aspect-ratio-positioning-guides`, and
> `preview-lacks-previous-next-watermark-navigation`.

Before editing:

1. Read the pinned `$ERSATZTV_SRC/ErsatzTV/Pages/Watermark*.razor` list,
   editor, preview, and navigation chain; the watermark parity file; the plan's
   P7C-02 section; and the full p7c-02 milestone acceptance criteria.
2. Inspect the ErsatzRS watermark application/server-function boundary,
   page/island boundary, selected-asset metadata, upload/save model, drawvg
   preview geometry, localization modules, and desktop/mobile browser coverage.
3. Build a Source Map and Visual Source Map covering saved-watermark ordering,
   draft state, asset dimensions, size modes, preview coordinates, 4:3/16:9
   framing, navigation ends, dirty-edit protection, and responsive layout.

Implement the complete slice:

- Render Actual Size from the selected asset's real dimensions; if reliable
  dimensions cannot be obtained for a supported source, expose an explicitly
  localized schematic state instead of presenting placeholder geometry as
  actual.
- Add clearly distinguishable and labeled 4:3 and 16:9 content-boundary guides
  without obscuring placement or changing persisted coordinates.
- Add localized Previous/Next controls over one deterministic saved-watermark
  order, with explicit list-end behavior and protection against silently
  discarding unsaved edits.
- Preserve public server-function/DTO contracts where observable, existing
  upload/save validation, drawvg output, all supported translations, and a
  hydration-stable DOM shape.
- Add focused geometry/navigation regressions plus Chromium and Firefox
  desktop/mobile coverage for size, guides, ordering, list ends, and dirty
  navigation.
- Resolve all three debts and update p7c-02 evidence, plan, changelog, phase,
  prompts, and session records.

Visual verification:

- Build and serve a matched SSR/WASM bundle with `./run.sh serve`; compare the
  list and editor at matching desktop and mobile viewports with the pinned
  ErsatzTV pages and capture any intentional difference.
- Exercise Actual Size with known landscape and portrait assets, verify both
  guides remain legible, and prove Previous/Next never loses a dirty draft
  without an explicit user decision.

Verification:

- `cargo fmt --all -- --check`
- `cargo xtask architecture-lint`
- focused tests through `./run.sh`
- `cargo xtask hydration-lint -p ersatzrs-web`
- `cargo xtask parity-check web`
- focused Chromium and Firefox Playwright specs
- both workspace clippy configurations
- `./run.sh test-closeout`
- `cargo xtask progress check`
- `git diff --check`

Report both source maps, measured geometry and navigation contract, upstream
and intentional visual differences, closed and remaining debt, verification
results, changed files, and the live next milestone. Do not commit or push
unless explicitly requested.

---

## 2. Fix the next ErsatzTV parity gap in ErsatzRS: deferred search and See All behavior (slug `media-browsers`)

Use AGENTS.md plus `skills/parity-slice.md`. Close milestone **p7c-03** and the
two open debts in
[`docs/progress/parity/media-browsers.toml`](progress/parity/media-browsers.toml).
Wire the known thumbnail-size omission and settle the See All detail/filter
contract from pinned upstream evidence without leaving another undocumented
deferral.

Current debt:

> `search-thumbnail-size-plumbing-deferred` and
> `see-all-detail-and-library-filter-disposition`.

Before editing:

1. Read the pinned ErsatzTV search and per-kind browser pages beginning with
   `$ERSATZTV_SRC/ErsatzTV/Pages/Movies.razor` and the equivalent show, image,
   music, song, and other-video routes named by the parity source map.
2. Read the media-browser parity file, ADR 0048 where playback overrides
   intersect card actions, the plan's P7C-03 section, and
   `cargo xtask progress milestones --id p7c-03 --full`.
3. Inspect search result classification, artwork URL sizing, See All routing,
   per-kind detail navigation, library-filter data/state, shared card
   primitives, localization, SSR/island boundaries, and existing browser specs.
4. Build a Source Map and Visual Source Map for query input through typed
   result/card rendering, artwork geometry, See All navigation, detail actions,
   library filtering, empty/error states, and responsive presentation.

Implement the complete slice:

- Route every Thumbnail-classified search card through the thumbnail artwork
  size path while preserving poster cards and artwork precedence.
- Establish the pinned ErsatzTV contract for each See All kind and its library
  filter. Implement required detail/filter behavior, or record a deliberate
  ErsatzRS non-goal in an ADR when the product intentionally differs.
- Remove the deferred code comments only when the behavior and durable
  disposition are complete.
- Preserve typed application query failures, stable route/server-function
  contracts, localized labels, shared card actions, and hydration-stable DOM.
- Add focused query/render regressions and Chromium/Firefox route coverage for
  thumbnail URLs, poster non-regression, See All navigation, filters,
  ready/empty/error states, and desktop/mobile geometry.
- Resolve both debts and update p7c-03 evidence, parity, plan, changelog, phase,
  prompts, and session records.

Visual verification:

- Build and serve a matched SSR/WASM bundle with `./run.sh serve`; compare
  search and every affected See All route against pinned ErsatzTV at matching
  desktop and mobile viewports.
- Verify thumbnail and poster aspect ratios from rendered image geometry, not
  only URL text, and exercise filter selection plus zero-result recovery.

Verification:

- `cargo fmt --all -- --check`
- `cargo xtask architecture-lint`
- focused tests through `./run.sh`
- `cargo xtask hydration-lint -p ersatzrs-web`
- `cargo xtask parity-check web`
- focused Chromium and Firefox Playwright specs
- both workspace clippy configurations
- `./run.sh test-closeout`
- `cargo xtask progress check`
- `git diff --check`

Report both source maps, pinned See All/filter disposition, artwork behavior,
intentional deviations and ADR decision, closed and remaining debt,
verification results, changed files, and the live next milestone. Do not
commit or push unless explicitly requested.

---

## 3. Build the next ErsatzRS spec chore: split large modules after boundary extraction (slug `application-use-case-layer`)

Start only after milestones **p7c-02** and **p7c-03** are complete. Follow
AGENTS.md and the P7C-04 contract in
[`plans/application-layer-and-video-jukebox.md`](../plans/application-layer-and-video-jukebox.md).
Close p7c-04 without changing public behavior or weakening completeness gates.

Before editing:

1. Reconfirm that `cargo xtask progress milestones --id p7c-04 --full` is
   runnable, then refresh the measurements in `plans/module-split-survey.md`.
2. Read the web/WASM split plan and inspect the reduced media-browser
   transport, YAML/scripted registries, and provider adapters.
3. Build a Source Map of module ownership, dependency directions, exhaustive
   dispatch, tests, and public transport contracts.

Implement the complete slice:

- Split the reduced modules along application, infrastructure, scheduler, and
  transport ownership boundaries.
- Keep native application code out of hydration/WASM code and preserve all
  public DTOs, routes, persisted shapes, dispatch behavior, and provider
  semantics.
- Retain or strengthen exhaustive dispatch/completeness gates so file moves
  cannot silently drop behavior.
- Update p7c-04 evidence, the active plans, changelog, phase, prompts, and
  session records.

Verification:

- focused crate tests through `./run.sh`
- `cargo xtask architecture-lint`
- both workspace clippy configurations
- `./run.sh test-closeout`
- `cargo xtask parity-check web`
- `cargo xtask progress check`
- `git diff --check`

Report the refreshed measurements, Source Map, final module ownership,
preserved contracts, verification results, changed files, and the live next
milestone. Do not commit or push unless explicitly requested.
