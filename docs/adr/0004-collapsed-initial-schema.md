# ADR 0004: Collapse the initial database schema

## Status

Accepted

## Context

ErsatzTV has hundreds of Entity Framework migrations. ErsatzRS starts from an empty
Rust project and does not need to replay historic .NET schema evolution.

## Decision

Create one initial sqlx schema derived from the final ErsatzTV model, then add
normal ErsatzRS migrations from that point forward.

## Consequences

The initial migration is larger, but the migration history is understandable and
starts from the intended ErsatzRS baseline.

## Alternatives Considered

- Port every EF migration: rejected because it imports obsolete history and
  .NET-specific migration mechanics.

## References

- `/home/bart/ErsatzTV/ErsatzTV.Infrastructure/Data/TvContext.cs`
- [Migration guide](../how-to/add-migration.md)
