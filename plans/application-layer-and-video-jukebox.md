# Application layer and video jukebox plan

## Status

Active. This plan sequences the structural debt found by the 2026-07-21
stub/stopgap audit and prepares a stable application boundary for a future
"The Box" music-video jukebox and its add-ons.

The authoritative execution state is in
[`docs/progress/milestones.toml`](../docs/progress/milestones.toml). Query it
with `cargo xtask progress milestones --runnable`; update this plan only when
sequencing, scope, or an architectural premise changes.

## Goal

Restore the crate boundary already promised by
[`ARCHITECTURE.md`](../ARCHITECTURE.md): transport adapters call
`ersatzrs-app` use cases, application code depends on storage-neutral core
ports, and infrastructure implements those ports. Use that boundary to add a
durable, restart-safe video request queue without coupling add-ons to Leptos,
Axum handlers, SQL, or scheduler internals.

## Non-goals

- Do not introduce a native dynamic-plugin ABI. Rust ABI stability and the six
  supported release targets make an authenticated HTTP/event boundary the
  safer first add-on contract.
- Do not rewrite working features horizontally. Migrate one complete vertical
  slice at a time while preserving endpoint names, persisted shapes, side
  effects, and ErsatzTV behavior.
- Do not move SQL from web into app. SQL belongs in infrastructure adapters.
- Do not make an in-memory channel or queue the durable source of truth for
  jukebox requests.
- Do not mechanically split the largest web modules before their application
  logic and queries have been extracted.

## Source map

- ErsatzTV application handlers: `$ERSATZTV_SRC/ErsatzTV.Application/`
- Block preview handler:
  `$ERSATZTV_SRC/ErsatzTV.Application/Scheduling/Commands/PreviewBlockPlayoutHandler.cs`
- ErsatzTV application locking and background-service patterns:
  `$ERSATZTV_SRC/ErsatzTV.Application/`
- ErsatzRS application target: `crates/ersatzrs-app`
- Core ports and rules: `crates/ersatzrs-core`
- Infrastructure adapters: `crates/ersatzrs-infra`
- Scheduler consumer: `crates/ersatzrs-scheduler`
- HTTP, Leptos, IPTV, HLS, and add-on transport adapters:
  `crates/ersatzrs-web`
- Existing structural split plans:
  [`module-split-survey.md`](module-split-survey.md) and
  [`web-server-wasm-crate-split.md`](web-server-wasm-crate-split.md)
- Intentional deviations: the future video-jukebox add-on API is an ErsatzRS
  extension and requires an ADR before its public contract is implemented.

## Findings that drive the sequence

1. `ersatzrs-app` is a one-line placeholder and has no consumers.
2. Web server functions and APIs perform application orchestration through raw
   SQL, concrete repositories, process-local queues, and duplicated lock
   registries.
3. Playback/HLS loaders conflate database failure with missing data, while many
   admin resources turn server failures into valid-looking empty/default state.
4. Block preview bypasses the real block scheduler and silently drops query
   failures.
5. Development routes are registered in the normal production router.
6. Local-only Trakt operations require a fake API client, and watermark
   "Actual Size" preview uses a fixed placeholder size.
7. A future jukebox needs durable request identity, policy, state transitions,
   and scheduler integration. Adding it to the current web-owned orchestration
   would deepen the boundary problem.

## Target dependency and runtime flow

```text
Leptos/Axum/admin API/add-on HTTP adapters
                    |
                    v
              ersatzrs-app
       commands, queries, orchestration
                    |
                    v
  ersatzrs-core ports and business policies
                    |
                    v
              ersatzrs-domain

ersatzrs-infra implements core ports
ersatzrs-scheduler consumes durable scheduling work
ersatzrs-web is the native composition root and transport boundary
```

The future web/WASM split must keep app, core repository ports, infra, and
scheduler native-only. Shared hydration code may own request/response DTOs but
must not depend on concrete application services or storage.

## Tracking contract

- `docs/progress/phase.toml` names the active program increment and immediate
  focus; it does not carry a historical backlog.
- `docs/progress/parity/*.toml` owns feature status and structured open debt.
- `docs/progress/milestones.toml` owns fine-grained status, dependency order,
  acceptance criteria, and completion evidence.
- This plan owns architectural sequencing, scope, and cross-milestone rules.
- ADRs own accepted decisions. Plans and milestone entries must not be treated
  as decisions before the relevant ADR is accepted.
- A milestone may be marked `Complete` only after its `evidence` field names
  the focused tests and applicable repository gates that passed.

## Phase 7A — Guardrails and failure semantics

### P7A-01 — Application-boundary decision and guardrails

- Write an ADR for the application boundary, composition root, and allowed
  dependency directions.
- Add architecture checks that prevent a placeholder app crate, app-to-web or
  app-to-infra dependencies, new direct admin SQL outside a shrinking
  allowlist, and new server-error-to-default loader patterns.
- Keep checks baseline-aware so existing debt can be reduced slice by slice.

### P7A-02 — Playback and HLS data-access semantics

- Return `Result<Option<T>>` or `Result<Vec<T>>` from playback metadata,
  transcode-capacity, HLS configuration, and active-item loaders.
- Distinguish absence, invalid persisted values, row decode failures, and
  database failures.
- Propagate operational failures into correlated logs and health reporting.
- Prove that a storage failure cannot silently select different FFmpeg input,
  audio, subtitle, deinterlace, tonemap, or capacity behavior.

### P7A-03 — Admin failure states and typed query rows

- Introduce a shared localized `Loading / Ready / Empty / Error` presentation
  model and error boundary for server-backed admin resources.
- Replace query-level `unwrap_or_default` behavior in coherent route families.
- Move row decoding into typed infrastructure read models and return typed
  errors instead of default identifiers or empty titles.

### P7A-04 — Development and tracking surfaces

- Remove development-only routes from normal production registration, or gate
  them behind a non-default feature plus explicit runtime opt-in.
- Reconcile `TEST.md` with structured hydration data and add detailed playout
  workflow coverage where it is genuinely missing.
- Correct misleading scanner/show documentation without converting intentional
  SSR/WASM no-op counterparts or manual FFmpeg proofs into false debt.

## Phase 7B — Application-layer extraction

### P7B-01 — Application foundation

- Populate `ersatzrs-app` with bounded command/query modules and typed
  application errors.
- Keep repository/provider traits in core; remove `sqlx::Error` from core and
  map concrete errors in infra.
- Build `ApplicationServices` in the native composition root from injected
  port implementations.
- Consolidate operation coordination so web and scheduler do not maintain
  competing lock/queue abstractions for one logical operation.

### P7B-02 — Playout-build vertical slice

- Add one `RequestPlayoutBuild` use case with typed outcomes such as accepted,
  already queued/building, unavailable, and rejected.
- Replace every direct `PlayoutBuildRequest` sender in web/API code while
  preserving ErsatzTV Refresh/Reset semantics and current admin feedback.
- Keep the scheduler as the consumer and preserve existing public endpoints.

### P7B-03 — Scan-operation vertical slice

- Move library/show scan admission, FIFO queueing, locking, and progress
  lifecycle into application services.
- Ensure all transport adapters observe one operation registry and the same
  accepted/running/failed state.

### P7B-04 — Real block-preview vertical slice

- Port the upstream temporary Template/Playout/reference-data construction.
- Invoke the real block preview builder in Reset mode and enrich its actual
  playout rows.
- Remove collection-type-specific preview SQL, fabricated search rows, and
  empty-on-error behavior.

### P7B-05 — Remaining admin use cases and queries

- Migrate collection, playlist, channel, schedule, settings, Trakt, and media
  mutation slices with all validation, persistence, and side effects intact.
- Migrate read models until admin transport adapters no longer depend on a raw
  database pool.
- Keep Leptos server-function names and serialized DTOs stable during moves.

## Phase 7C — Secondary stopgap closeout and scaling refactors

### P7C-01 — Trakt responsibility split

- Separate local matching/generated-playlist maintenance from remote API
  refresh operations.
- Delete `NoopTraktApiClient`; operations that do not need a remote capability
  must not accept one.

### P7C-02 — Watermark preview truthfulness

- Render watermark Actual Size using the selected asset dimensions, or rename
  the result as a schematic preview.
- Add clearly distinguishable 4:3 and 16:9 content-boundary helper lines so
  watermark placement can be aligned across content aspect ratios.
- Add localized Previous/Next controls that navigate saved watermarks in a
  deterministic order for rapid comparison, handle list ends explicitly, and
  never silently discard unsaved edits.

### P7C-03 — Deferred search and browser behavior

- Complete the deferred search thumbnail-size plumbing.
- Source-check the deferred per-kind See All details/library filter and either
  implement upstream behavior or record an explicit ErsatzRS non-goal.

### P7C-04 — Module splits after extraction

- Refresh file-size/count evidence in `module-split-survey.md`.
- Split the reduced media-browser transport module after use cases and queries
  leave it.
- Continue YAML instruction dispatch, scripted registrars, and provider dedup
  as independent behavior-preserving slices with completeness gates.
- Coordinate with the web server/WASM split so native application code cannot
  leak back into the hydration crate.

## Phase 8A — Durable video-jukebox domain

### P8A-01 — Product and architecture decisions

Before schema or API work, accept an ADR covering:

- channel scope and eligible music-video sources;
- requester identity, moderation, authorization, and privacy;
- FIFO/priority policy, duplicate and artist/video cooldowns, expiry, and
  per-source rate limits;
- current-item behavior, safe insertion boundaries, cancellation, retries,
  filler interaction, and failure recovery;
- stable add-on transport/versioning and whether adapters run out of process or
  are compiled into the binary.

### P8A-02 — Durable request aggregate and ports

- Add request identity, channel/media identity, source/requester identity,
  idempotency key, queue position, timestamps, and rejection/failure detail.
- Use explicit states: Pending, Accepted, Rejected, Queued, Playing, Played,
  Cancelled, Expired, and Failed.
- Add storage-neutral repository/catalog/policy/clock/event ports and SQLite +
  MySQL implementations.
- Make submit, idempotency, claim, and state transition operations atomic.

### P8A-03 — Application use cases and scheduler integration

- Add catalog search, submit, list/status, cancel/moderate, claim, start,
  complete, fail, and recovery use cases.
- Let scheduling claim an accepted request only at the ADR-defined safe
  boundary, attach its request id to the materialized playout item, and preserve
  the currently playing item during refresh.
- Recover accepted/playing work after restart without duplicate scheduling.

## Phase 8B — Add-on ingress and runtime proof

### P8B-01 — Stable add-on transport

- Expose a versioned authenticated HTTP command/query API and an SSE or
  equivalent status/event feed.
- Apply authorization, rate limits, idempotency, and audit logging at the
  application boundary.
- Keep adapter SDK/examples protocol-only; they must not expose SQL, Leptos
  server-function URLs, or scheduler internals.

### P8B-02 — Operations and end-to-end acceptance

- Add health checks for stalled claims, invalid requested media, failed
  scheduler handoff, and add-on configuration.
- Add request/queue/state-transition metrics without logging tokens or private
  media paths.
- Prove duplicate-idempotency handling, rejection policy, restart survival,
  exactly-once scheduling, playing/played transitions, and multiple independent
  add-on sources in a real end-to-end run.

## Vertical-slice completion rule

Every implementation milestone must close with:

- source-vs-port comparison when ErsatzTV behavior exists;
- application unit tests with fake ports;
- infrastructure database tests for SQLite and MySQL-sensitive shapes;
- focused web/scheduler tests for adapter and side-effect behavior;
- updated parity debt and milestone evidence;
- `cargo xtask progress check`;
- the narrow wrapper-based Rust test command for changed crates;
- the relevant hydration, Playwright, smoke, or TSDuck gate when the slice
  changes admin interaction or playback behavior.

Run the full multi-configuration repository gate once at the closeout of a
phase, not once per milestone.

## Plan retirement

Delete this file from the active-plan index after all Phase 7 milestones are
complete and the Phase 8 contract has moved into accepted ADRs and feature
parity records. Preserve completion evidence in milestone/parity TOMLs,
session entries, ADRs, the changelog, and git history.
