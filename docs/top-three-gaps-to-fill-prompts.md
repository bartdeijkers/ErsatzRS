<!--
generated: 2026-07-24
last_processed_commit: 83ead50ae772e0bad3c45709a30364ec596d39e4
parity_baseline_commit: 0d301df5e8512fdfccf3a611188d27ab1690ee31
generator: docs/automation/daily-progress-routine.md (ersatzrs-daily-progress)
rules: docs/automation/gap-prompt-rules.md
-->

# Top three chores to do - goal prompts

Actionable paste-and-run goal prompts selected from the current progress files,
ADR index, active plans, and phase state. With p7b-02 complete, the two
remaining runnable Phase 7B vertical slices lead in milestone order; the
maintainer-requested watermark-preview closeout is the highest-priority
remaining runnable parity chore.

---

## 1. Build the next ErsatzRS spec chore: scan-operation application vertical slice (slug `application-use-case-layer`)

Use the repo workflow from AGENTS.md. Close milestone **p7b-03** under
[ADR 0051](adr/0051-application-boundary-and-composition-root.md) and
[`plans/application-layer-and-video-jukebox.md`](../plans/application-layer-and-video-jukebox.md).
Move the complete library/show scan lifecycle behind application services while
preserving provider-specific ErsatzTV behavior, FIFO ordering, public contracts,
and observable progress. Do not leave web-owned queue or progress paths, and
record any genuine remaining gap as structured `ParityDebt`.

Current debt:

> `operation-coordination-is-split` — web still owns scan queue and progress
> lifecycle even though `ApplicationServices` now owns the shared operation
> locker.

Before editing:

1. Read ADR 0051, the plan's P7B-03 section, `ARCHITECTURE.md`, `STYLE.md`,
   `PORTING.md`, and `cargo xtask progress milestones --id p7b-03 --full`.
2. Read ErsatzTV's queue-library-scan and queue-show-scan handlers plus their
   consumers, then inspect every ErsatzRS scan producer, queue worker, lock,
   progress registry, scanner-companion call, and admin/API observer.
3. Build a Source Map covering admission, FIFO queueing, library/show identity,
   provider validation, start/progress/completion/failure state, retry or
   duplicate behavior, and transport mapping.

Implement the slice:

- Add typed library/show scan application commands, outcomes, and queries.
- Move admission, FIFO queue ownership, operation locking, progress, completion,
  and failure cleanup into application services shared by all transports and
  background workers.
- Remove the parallel web-owned scan coordination path while keeping scanner
  invocation, provider semantics, endpoint names, DTOs, polling behavior, and
  localization stable.
- Add focused regressions for ordering, duplicate requests, both scan kinds,
  progress visibility, worker failure, and cleanup after success or failure.
- Update parity debt, p7b-03 evidence, ADR implementation note, plan, changelog,
  phase state, top-gap prompts, and session log.

Verification:

- `cargo fmt --all -- --check`
- `cargo xtask architecture-lint`
- focused checks and tests through `./run.sh`
- both workspace clippy configurations
- `./run.sh test-closeout`
- `cargo xtask parity-check`
- `cargo xtask progress check`
- `git diff --check`

Report the Source Map, lifecycle ownership, typed outcomes and progress model,
preserved behavior, closed and remaining debt, verification results, changed
files, and the live next milestone. Do not commit or push unless explicitly
requested.

---

## 2. Fix the next ErsatzTV parity gap in ErsatzRS: real block playout preview (slug `block-scheduling-ui`)

Use AGENTS.md plus `skills/parity-slice.md`. Close milestone **p7b-04** with
the pinned ErsatzTV block-preview behavior and the application boundary from
[ADR 0051](adr/0051-application-boundary-and-composition-root.md). Replace the
simplified preview completely; do not keep fabricated rows, collection-type
shortcuts, or empty-on-error behavior. Keep any shortfall as visible
`ParityDebt`.

Current debt:

> `block-preview-bypasses-real-scheduling-engine` — unsaved block preview
> manually enumerates a subset of collections and durations instead of invoking
> the real builder with upstream-shaped temporary scheduling state.

Before editing:

1. Read `$ERSATZTV_SRC/ErsatzTV.Application/Scheduling/Commands/PreviewBlockPlayoutHandler.cs`
   and the called block-preview builder/query chain at the pinned baseline,
   along with the parity file and p7b-04 acceptance criteria.
2. Inspect the ErsatzRS block editor/server function, scheduler block engine,
   temporary template/playout/reference-data types, enrichment queries,
   localization, error mapping, and existing browser coverage.
3. Build a Source Map and Visual Source Map covering draft input, temporary
   state, Reset-mode invocation, collection kinds, resulting enriched rows,
   error states, loading/empty states, and responsive preview presentation.

Implement the slice:

- Introduce an application query that constructs the upstream-shaped temporary
  Template, Playout, and reference data from the unsaved draft and invokes the
  real block preview builder in Reset mode.
- Return the real enriched playout rows for every supported block item kind;
  delete collection-specific preview SQL and fabricated duration accumulation.
- Surface typed failures through a localized, hydration-stable error state
  rather than turning failures into an empty preview.
- Preserve draft editing, endpoint/DTO contracts where externally observable,
  scheduling semantics, and stable SSR/hydration DOM shape.
- Add focused engine/application regressions and desktop/mobile Playwright
  coverage, then update parity, milestone, ADR/plan, changelog, phase, prompts,
  and session records.

Visual verification:

- Build and serve a matched SSR/WASM bundle with `./run.sh serve`; compare the
  populated, empty, loading, and failed preview at matching desktop and mobile
  viewports with the pinned ErsatzTV source.

Verification:

- `cargo fmt --all -- --check`
- `cargo xtask architecture-lint`
- focused checks and tests through `./run.sh`
- `cargo xtask parity-check web`
- focused Chromium and Firefox Playwright specs
- both workspace clippy configurations
- `./run.sh test-closeout`
- `cargo xtask progress check`
- `git diff --check`

Report both source maps, real-engine invocation and temporary state shape,
behavior and visual differences, closed and remaining debt, verification
results, changed files, and the live next milestone. Do not commit or push
unless explicitly requested.

---

## 3. Fix the next maintainer-requested parity chore: truthful watermark preview and comparison (slug `watermark-editor`)

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
2. Inspect the ErsatzRS watermark server functions, page/island boundary,
   selected-asset metadata, upload/save model, drawvg preview geometry,
   localization modules, and existing desktop/mobile browser coverage.
3. Build a Source Map and Visual Source Map covering saved-watermark ordering,
   draft state, asset dimensions, size modes, preview coordinate system,
   4:3/16:9 framing, Previous/Next end behavior, dirty-edit protection, SSR
   shape, and responsive layout.

Implement the complete slice:

- Render Actual Size from the selected asset's real dimensions; if reliable
  dimensions cannot be obtained for a supported source, expose an explicitly
  localized schematic state instead of presenting placeholder geometry as
  actual.
- Add clearly distinguishable and labeled 4:3 and 16:9 content-boundary guides
  without obscuring the watermark placement or changing persisted coordinates.
- Add localized Previous/Next controls over one deterministic saved-watermark
  order, with explicit list-end behavior and protection against silently
  discarding unsaved edits.
- Preserve public server-function/DTO contracts where observable, existing
  upload/save validation, drawvg output, translations for every supported
  culture, and a hydration-stable DOM shape.
- Add focused geometry/navigation regressions plus Chromium and Firefox
  desktop/mobile coverage for actual/schematic size, both framing guides,
  ordering, list ends, and dirty-edit navigation.
- Resolve all three parity debts and update p7c-02 evidence, plan, changelog,
  phase state, top-gap prompts, and session log.

Visual verification:

- Build and serve a matched SSR/WASM bundle with `./run.sh serve`; compare the
  list and editor at matching desktop and mobile viewports with the pinned
  ErsatzTV pages and capture any intentional difference.
- Exercise Actual Size with known landscape and portrait assets, verify both
  aspect-ratio guides remain legible, and prove Previous/Next never loses a
  dirty draft without an explicit user decision.

Verification:

- `cargo fmt --all -- --check`
- `cargo xtask architecture-lint`
- focused tests through `./run.sh`
- `cargo xtask hydration-lint -p ersatzrs-web`
- `cargo xtask parity-check web`
- focused Chromium and Firefox Playwright specs at desktop/mobile viewports
- both workspace clippy configurations
- `./run.sh test-closeout`
- `cargo xtask progress check`
- `git diff --check`

Report both source maps, measured geometry and navigation contract, upstream
and intentional visual differences, closed and remaining debt, verification
results, changed files, and the live next milestone. Do not commit or push
unless explicitly requested.
