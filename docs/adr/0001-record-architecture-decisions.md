# ADR 0001: Record architecture decisions

## Status

Accepted

## Implementation

N/A — meta decision: this is the ADR process itself.

## Context

ErsatzRS is a long-running port of ErsatzTV. Without written decisions, crate
boundaries, dependency choices, and deliberate deviations will drift over time.

## Decision

Use ADRs in `docs/adr/` for significant decisions. ADR numbers are four-digit,
zero-padded, and never reused.

An ADR is required when a change affects:

- Crate boundaries.
- Runtime architecture.
- Database strategy.
- Public APIs.
- External dependencies used across multiple crates.
- Behavior that deliberately diverges from ErsatzTV.

Each ADR tracks two **independent** lifecycles in separate sections:

- `## Status` — the decision: `Proposed` → `Accepted` → `Superseded by ADR
  NNNN` / `Deprecated`. It says whether we agreed, never whether code exists.
- `## Implementation` — whether the code exists: one of `Not implemented`,
  `Partial — …`, `Implemented (<ref>, <date>)`, or `N/A — …`. An accepted
  decision can be entirely unbuilt, so `Accepted {date}` must never be read as
  "done".

`cargo xtask progress check` enforces this: every ADR must carry a recognized
`## Implementation` token, and an ADR may not claim `Implemented` while a
parity debt linked to it (`adr = "NNNN"` in
`docs/progress/parity/<slug>.toml`) is still open.

## Consequences

The project pays a small documentation cost up front, but later work can point
to stable decisions instead of rediscovering them.

## Alternatives Considered

- Rely on commit history only: rejected because decisions become difficult to
  find.
- Keep decisions only in the implementation plan: rejected because plans change
  and ADRs need stable numbering.

## References

- [../../ARCHITECTURE.md](../../ARCHITECTURE.md)
- [../../PORTING.md](../../PORTING.md)
