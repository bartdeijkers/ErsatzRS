# ADR 0051: Enforce the application boundary and native composition root

## Status

Accepted (2026-07-21)

## Implementation

Partial — the Phase 7B foundation in `ersatzrs-app` now exposes documented command, query,
application-error, port, and operation-coordination modules. Core repository
errors are storage-neutral, infrastructure maps concrete SQLx failures at its
boundary, and the native web composition root constructs injected
`ApplicationServices` with one process-wide operation coordinator. The
baseline-aware `cargo xtask architecture-lint` gate also prevents concrete
storage dependencies from re-entering core. Phase 7B still has to migrate the
playout-build, scan, block-preview, and remaining admin vertical slices.

## Context

`ARCHITECTURE.md` assigns use-case coordination to `ersatzrs-app`, but the
crate was a one-line placeholder while Leptos server functions and Axum APIs
performed validation, SQL, row decoding, queue admission, and side effects
directly. This makes those workflows unavailable to other adapters and would
force the future video-jukebox API to duplicate web-owned orchestration.

ErsatzTV organizes commands, queries, and handlers under
`$ERSATZTV_SRC/ErsatzTV.Application/`, and its web host consumes that project.
The upstream application project also references its concrete infrastructure
project. ErsatzRS deliberately strengthens this dependency boundary: core
ports replace concrete infrastructure references so application use cases can
be tested and reused without SQL or a web runtime.

Existing direct admin SQL and error-to-default resource loaders are too
numerous to remove safely in one horizontal rewrite. The boundary therefore
needs a ratchet that blocks new debt while complete vertical slices migrate.

## Decision

Use these ownership and dependency rules:

- `ersatzrs-domain` owns entities and value objects and has no ErsatzRS crate
  dependencies.
- `ersatzrs-core` owns business policies and storage/provider ports. Ports use
  storage-neutral errors and types.
- `ersatzrs-app` owns commands, queries, use-case validation, transaction and
  side-effect ordering, and cross-adapter operation coordination. It may
  depend on `ersatzrs-core`, `ersatzrs-domain`, and general-purpose libraries,
  but not concrete infrastructure, transports, schedulers, binaries, FFmpeg,
  or platform probes.
- `ersatzrs-infra` implements core ports and owns SQL, persisted row decoding,
  and concrete provider clients.
- `ersatzrs-web` owns Leptos/Axum/IPTV/HDHomeRun transport mapping and is the
  native composition root. It constructs infrastructure adapters, injects
  them into `ApplicationServices`, and supplies those application services to
  HTTP handlers and native background workers.
- `ersatzrs-scheduler` consumes durable scheduling work and core policies. It
  may call injected application services but application code cannot depend on
  the scheduler implementation.
- Hydration/WASM code may share serializable transport DTOs but cannot depend
  on application services, infrastructure, or other native-only code.

Transport adapters convert protocol input into application requests and map
typed outcomes back to transport responses. They do not own business
validation, raw SQL, row decoding, queues, or operation locks. Application
code does not return `sqlx::Error`, HTTP responses, Leptos types, or concrete
repository implementations.

Enforce the decision with `cargo xtask architecture-lint`, run directly in CI
and as part of `cargo xtask parity-check`. The gate:

- rejects an `ersatzrs-app` crate without a public application item;
- rejects direct or renamed dependencies from `ersatzrs-app` to adapter and
  runtime crates;
- counts production SQLx query constructors, including imported, renamed, and
  glob-imported `query*`, `raw_sql`, and `QueryBuilder` forms, plus raw-SQL
  calls through `sqlx::Executor`, under the Leptos and plain Axum admin
  adapters; and
- counts production `.await.unwrap_or_default()` loaders that turn server
  failure into apparently valid data.

The two source-pattern rules use exact per-file counts in
`.architecture-lint-baseline.toml`. A new file, an increase, or a stale count
fails. When a vertical slice removes debt, its change must lower or delete the
matching baseline entry. Only modules and functions whose `cfg` expression
cannot compile when `test` is false are excluded; production-capable forms
such as `cfg(not(test))` and `cfg(any(test, ...))` remain audited.

## Consequences

New work cannot deepen the audited application-boundary debt. Phase 7B can
reduce the baseline one vertical slice at a time without destabilizing
unrelated behavior, and future HTTP/add-on adapters can reuse the resulting
use cases.

The lexical source rules intentionally enforce narrow, reviewable invariants;
they do not prove that every orchestration concern has moved. Parity files and
milestone acceptance evidence remain responsible for that broader audit.

This boundary intentionally differs from ErsatzTV's application-to-
infrastructure project reference. The added port indirection is the cost of a
storage-neutral application layer and is consistent with ErsatzRS's existing
crate architecture.

## Alternatives Considered

- Preserve ErsatzTV's direct application-to-infrastructure dependency.
  Rejected because it would make application handlers concrete-storage
  services and defeat the existing core-port boundary.
- Move existing admin SQL horizontally before adding the gate. Rejected
  because a broad rewrite would mix many parity-sensitive route families and
  still allow new debt during the migration.
- Baseline whole files without exact counts. Rejected because fixed file
  exemptions would allow debt to grow and would not force the allowlist to
  shrink.
- Put the composition root in `ersatzrs-app`. Rejected because constructing
  concrete adapters would reverse the application-to-infrastructure boundary.

## References

- [../../ARCHITECTURE.md](../../ARCHITECTURE.md)
- [../../plans/application-layer-and-video-jukebox.md](../../plans/application-layer-and-video-jukebox.md)
- `$ERSATZTV_SRC/ErsatzTV.Application/ErsatzTV.Application.csproj`
- `$ERSATZTV_SRC/ErsatzTV/ErsatzTV.csproj`
