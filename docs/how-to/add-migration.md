# Add a migration

Use this guide to add a schema change for SQLite and, where supported, MySQL.
Migrations live under `migrations/`.

## Naming

Use UTC timestamps:

```text
YYYYMMDDHHMM_<snake_description>.sql
```

Example:

```text
202604151030_create_initial_schema.sql
```

## Policy

- The initial schema is collapsed from the final ErsatzTV model.
- Later migrations are normal incremental ErsatzRS migrations.
- Prefer additive changes.
- Do not drop columns or tables without an ADR.
- Destructive changes need a manual rollback note in the pull request.

## Testing

Test SQLite migrations first. Add the matching migration under
`migrations/mysql/` and run MySQL-feature coverage for changes supported by both
backends.
