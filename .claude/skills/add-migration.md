---
name: add-migration
description: Add a sqlx migration following NasTv naming and review rules. Use when creating or changing database schema migrations.
---

# Add Migration

## Workflow

1. Read [docs/how-to/add-migration.md](../../docs/how-to/add-migration.md).
2. Check whether the change is additive or destructive.
3. Create a timestamped migration under `migrations/`.
4. Use `snake_case` names.
5. Add comments for destructive or compatibility-sensitive changes.
6. Add or update tests.

## ADR Trigger

Create an ADR before dropping tables, dropping columns, changing identity
semantics, or changing the collapsed-initial-schema strategy.
