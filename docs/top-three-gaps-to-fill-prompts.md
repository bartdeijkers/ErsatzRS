<!--
generated: 2026-07-22
last_processed_commit: a4977f0630a21530207a2ccd621a285f4e943098
parity_baseline_commit: 0d301df5e8512fdfccf3a611188d27ab1690ee31
generator: docs/automation/daily-progress-routine.md (ersatzrs-daily-progress)
rules: docs/automation/gap-prompt-rules.md
-->

# Top three chores to do - goal prompts

Actionable paste-and-run goal prompts selected from the current progress files,
ADR index, active plans, and phase state. With the Phase 7B foundation complete,
the three newly runnable vertical slices lead in milestone order.

---

## 1. Build the next ErsatzRS spec chore: playout-build application vertical slice (slug `application-use-case-layer`)

Use the repo workflow from AGENTS.md. Close milestone **p7b-02** under
[ADR 0051](adr/0051-application-boundary-and-composition-root.md) and
[`plans/application-layer-and-video-jukebox.md`](../plans/application-layer-and-video-jukebox.md).
This is an orchestration extraction: preserve ErsatzTV Refresh/Reset semantics,
all public endpoints, and current queued/building feedback while making the
application layer the single admission boundary. Deliver the complete vertical
slice with no transport-owned fallback path, and keep any remaining shortfall as
structured `ParityDebt`.

Current debt:

> `operation-coordination-is-split` — web still owns playout-build sending even
> though `ApplicationServices` now owns the shared entity locker and activity
> registry.

Before editing:

1. Read ADR 0051, the plan's P7B-02 section, `ARCHITECTURE.md`, `STYLE.md`, and
   `cargo xtask progress milestones --id p7b-02 --full`.
2. Inspect every `PlayoutBuildRequest` producer, the scheduler consumer,
   `ApplicationServices`, `OperationCoordinator`, AppState construction, and
   playout admin/API feedback paths.
3. Read the matching ErsatzTV application handlers and build a Source Map of
   request admission, Refresh/Reset selection, queue ownership, duplicate
   detection, worker consumption, activity state, persistence, and transport
   responses before editing.

Implement the slice:

- Add one documented `RequestPlayoutBuild` application command/use case with
  typed outcomes for accepted, already queued or building, unavailable, and
  rejected requests.
- Move sender ownership and all admission decisions behind injected
  `ApplicationServices`; replace every direct web/API sender without leaving a
  second queue or lock registry.
- Keep the scheduler as the durable work consumer and preserve ordering,
  Refresh/Reset semantics, persisted build outcomes, and current admin/API
  response shapes.
- Prove duplicate, queued, building, closed-worker, Refresh, Reset, and failure
  lifecycles with focused app, scheduler, and transport regressions.
- Update the application parity file, p7b-02 evidence, ADR implementation note,
  plan, changelog, phase state, top-gap prompts, and session log.

Verification:

- `cargo fmt --all -- --check`
- `cargo xtask architecture-lint`
- focused checks and tests through `./run.sh`
- both workspace clippy configurations
- `./run.sh test-closeout`
- `cargo xtask parity-check`
- `cargo xtask progress check`
- `git diff --check`

Report the Source Map, exact orchestration boundary, typed outcomes, preserved
behavior, closed and remaining debt, verification results, changed files, and
the live next milestone. Do not commit or push unless explicitly requested.

---

## 2. Build the next ErsatzRS spec chore: scan-operation application vertical slice (slug `application-use-case-layer`)

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

## 3. Fix the next ErsatzTV parity gap in ErsatzRS: real block playout preview (slug `block-scheduling-ui`)

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
