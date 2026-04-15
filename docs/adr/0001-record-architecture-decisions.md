# ADR 0001: Record architecture decisions

## Status

Accepted

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
