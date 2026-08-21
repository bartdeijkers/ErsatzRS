# Application layer and video jukebox plan

## Status

Queued for Phase 10. The Phase 7 application-boundary work is complete; this
plan now owns the dependency-gated future "The Box" music-video jukebox and its
add-ons. Phase 10 begins after Phase 9 teletext support closes through
milestone `p9b-02`. The earlier
[`drawtext-overlay-leg.md`](drawtext-overlay-leg.md) program establishes a
dynamic, reloadable text overlay that Phase 10 can reuse for changing request,
now/next, or countdown presentation without making it part of the jukebox's
durable queue contract.

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
- Existing structural split plan:
  [`web-server-wasm-crate-split.md`](web-server-wasm-crate-split.md). The
  module-split selection rule closed with `p7c-04` and now lives as the
  `module-split-selection-rule` entry in
  `docs/progress/parity/application-use-case-layer.toml`.
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

- Introduce one localized, hydration-stable admin resource wrapper with
  `data-state="loading|ready|empty|error"`; data-access failures continue to use
  the Phase 7A `RepositoryError` vocabulary rather than a UI-only error type.
- Migrate the complete `/settings/*` and `/media/libraries` route families,
  remove `Default` from their write-back form DTOs, and prove each family with
  a closed-pool regression that renders an error state without a submittable
  form.
- Move those families' row decoding into typed infrastructure read models and
  ratchet direct SQL, awaited-result fallbacks, silent `if let Ok` branches,
  form-rendering `Suspense` fallbacks, and `try_get(...).unwrap_or*` debt.
- Reduce the architecture baseline exactly from 535 to 519 direct admin SQL
  calls, 204 to 184 server-error default loaders, and 851 to 843 row-decode
  defaults; cover one injected error page plus the stable navigation-badge
  markers in the whole-app island hydration gate.

### P7A-04 — Development and tracking surfaces

- Complete. The component gallery and sample stream now require the non-default
  `development-routes` feature plus explicit runtime opt-in.
- `docs/progress/hydration.toml` is the canonical browser-verification source;
  `TEST.md` links to its generated route report, and validation rejects a
  reintroduced route table or either mismatch direction.
- Detailed playout browser coverage now proves the list/detail and destructive
  dialog workflows, and the scanner/show notes describe the implemented
  provider-specific behavior accurately.

## Phase 7B — Application-layer extraction

### P7B-01 — Application foundation

- Complete (2026-07-22). `ersatzrs-app` now provides bounded command/query
  modules, typed application errors, injected ports, and shared operation
  coordination.
- Core repository errors are storage-neutral and infrastructure explicitly
  maps concrete SQLx failures at the adapter boundary.
- The native web composition root constructs `ApplicationServices`; web and
  scheduler share its entity locker and playout activity registry.
- Scan queue/progress ownership intentionally remains for the complete p7b-03
  vertical slice.

### P7B-02 — Playout-build vertical slice

- Complete (2026-07-23). One `RequestPlayoutBuild` application command owns
  validation, coalescing, queue admission, and typed accepted, already queued,
  already building, unavailable, and rejected outcomes.
- Every web/API producer now uses injected `ApplicationServices`; the native
  composition-root adapter retains the existing unbounded FIFO channel without
  exposing its sender through `AppState`.
- The scheduler remains the sole consumer. Refresh/Reset requests, persisted
  build results, public endpoint behavior, and queued/building feedback remain
  intact. Queued modes coalesce to `Reset` over `Refresh` over `Continue`;
  requests received during an active build share one upgradable follow-up
  token, and a lifecycle guard clears only completed activity after success,
  failure, or cancellation.

### P7B-03 — Scan-operation vertical slice

- Complete (2026-07-24). Library/show scan admission, persisted validation,
  distinct FIFO queueing, per-library exclusion, progress, and retained
  terminal failures now live in application services.
- Web and periodic producers use the same commands; polling and the additive
  diagnostic endpoint read one operation registry. Concrete provider/local
  execution and deep-show subtitle extraction remain native adapters.
- External-collection synchronization remains a separate keyed operation in
  the shared application coordinator because upstream gives it a distinct
  media-source lifecycle rather than the library/show scanner queue.

### P7B-04 — Real block-preview vertical slice

- Complete (2026-07-24). The application query constructs upstream-shaped
  temporary Template/Playout/reference state, invokes the production block
  builder in Reset mode, and returns its actual enriched rows.
- Collection-specific preview SQL, fabricated search rows, and empty-on-error
  behavior are removed; the unsaved editor exposes stable localized resource
  states.

### P7B-05 — Remaining admin use cases and queries

- Complete (2026-07-24). Collection, playlist, channel, schedule, settings,
  Trakt, media, and supporting admin/API handlers are thin adapters over
  application commands and queries.
- Core ports and records are storage-neutral; infrastructure owns SQL, typed
  row decoding, transaction boundaries, and SQLite/MySQL differences. The
  composition root is the only web layer that constructs concrete adapters.
- Architecture lint reports zero direct admin SQL, raw admin-pool consumers,
  server-error defaults, and row-decode defaults. Leptos server-function
  names, serialized DTOs, validation, persistence, and side effects remain
  stable.

## Phase 7C — Secondary stopgap closeout and scaling refactors

### P7C-01 — Trakt responsibility split

- Complete (2026-07-25). Client-free local maintenance now owns matching,
  generated playlists, settings, deletion, and hourly due-match work.
- Remote import and daily refresh require `TraktApiClient`, persist fetched
  list state before matching, and surface typed configuration/API failures.
  `NoopTraktApiClient` is removed, and unavailable remote configuration cannot
  prevent the independent local maintenance pass.

### P7C-02 — Watermark preview truthfulness

- Render watermark Actual Size using the selected asset dimensions, or rename
  the result as a schematic preview.
- Add a clearly distinguishable centered 4:3 content-boundary helper line
  without marking the outer 16:9 preview edge.
- Add localized Previous/Next controls that navigate saved watermarks in a
  deterministic order for rapid comparison, handle list ends explicitly, and
  never silently discard unsaved edits.

### P7C-03 — Deferred search and browser behavior

- Complete the deferred search thumbnail-size plumbing.
- Source-check the deferred per-kind See All details/library filter and either
  implement upstream behavior or record an explicit ErsatzRS non-goal.

### P7C-04 — Module splits after extraction

- Split the reduced media-browser transport module after use cases and queries
  leave it.
- Continue YAML instruction dispatch, scripted registrars, and provider dedup
  as independent behavior-preserving slices with completeness gates.
- Coordinate with the web server/WASM split so native application code cannot
  leak back into the hydration crate.

## Phase 10A — Durable video-jukebox domain

### P10A-01 — Product and architecture decisions

Before schema or API work, accept an ADR covering:

- channel scope and eligible music-video sources;
- requester identity, moderation, authorization, and privacy;
- FIFO/priority policy, duplicate and artist/video cooldowns, expiry, and
  per-source rate limits;
- current-item behavior, safe insertion boundaries, cancellation, retries,
  filler interaction, and failure recovery;
- stable add-on transport/versioning and whether adapters run out of process or
  are compiled into the binary.

### P10A-02 — Durable request aggregate and ports

- Add request identity, channel/media identity, source/requester identity,
  idempotency key, queue position, timestamps, and rejection/failure detail.
- Use explicit states: Pending, Accepted, Rejected, Queued, Playing, Played,
  Cancelled, Expired, and Failed.
- Add storage-neutral repository/catalog/policy/clock/event ports and SQLite +
  MySQL implementations.
- Make submit, idempotency, claim, and state transition operations atomic.

### P10A-03 — Application use cases and scheduler integration

- Add catalog search, submit, list/status, cancel/moderate, claim, start,
  complete, fail, and recovery use cases.
- Let scheduling claim an accepted request only at the ADR-defined safe
  boundary, attach its request id to the materialized playout item, and preserve
  the currently playing item during refresh.
- Recover accepted/playing work after restart without duplicate scheduling.

## Phase 10B — Add-on ingress and runtime proof

### P10B-01 — Stable add-on transport

- Expose a versioned authenticated HTTP command/query API and an SSE or
  equivalent status/event feed.
- Apply authorization, rate limits, idempotency, and audit logging at the
  application boundary.
- Keep adapter SDK/examples protocol-only; they must not expose SQL, Leptos
  server-function URLs, or scheduler internals.

### P10B-02 — Operations and end-to-end acceptance

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

Delete this file from the active-plan index after the Phase 10 contract has
moved into accepted ADRs and feature
parity records. Preserve completion evidence in milestone/parity TOMLs,
session entries, ADRs, the changelog, and git history.
