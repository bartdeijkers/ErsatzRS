# Deliberate Deviations from ErsatzTV

NasTv preserves ErsatzTV behavior by default. Deliberate deviations must be
listed here and backed by ADRs.

## drawvg graphics instead of external renderer

NasTv uses FFmpeg 8.1 `drawvg` with SVG templates instead of recreating the
external graphics-renderer process.

ADR: [0005](../adr/0005-drawvg-graphics-engine.md)

## Collapsed initial schema

NasTv starts from one initial sqlx schema based on the final ErsatzTV model
instead of porting every historic EF migration.

ADR: [0004](../adr/0004-collapsed-initial-schema.md)

## sqlx instead of EF Core

NasTv uses explicit SQL through sqlx, with SQLite first and MySQL later behind a
feature flag.

ADR: [0006](../adr/0006-sqlx-over-sea-orm.md)

## Leptos instead of Blazor

NasTv uses Leptos for the admin UI because the project is Rust-native.

ADR: [0002](../adr/0002-ui-framework-leptos.md)

## Plain async handlers instead of MediatR

NasTv uses plain async use-case handlers and constructor-injected traits instead
of recreating MediatR.

ADR: [0001](../adr/0001-record-architecture-decisions.md)
