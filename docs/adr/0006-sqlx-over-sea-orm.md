# ADR 0006: Use sqlx for persistence

## Status

Accepted

## Context

ErsatzRS needs SQLite first and MySQL later. The project also wants explicit SQL
and predictable migrations instead of an ORM model port.

## Decision

Use `sqlx` for persistence. Start with SQLite. Add MySQL behind a Cargo feature
flag when the schema and repository traits are stable enough.

## Consequences

SQL remains explicit and testable. Repository traits must protect upper layers
from database details.

## Alternatives Considered

- SeaORM: rejected because ErsatzRS prefers explicit SQL and compile-time query
  checking where practical.
- Diesel: rejected because async support and the planned multi-backend model are
  less aligned with the rest of the stack.

## References

- [../../ARCHITECTURE.md](../../ARCHITECTURE.md)
