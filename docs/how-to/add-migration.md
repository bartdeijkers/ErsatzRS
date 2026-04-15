# Add a Migration

Migrations live under `migrations/` once the workspace exists.

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

Test SQLite migrations first. Add MySQL coverage behind the MySQL integration
feature once that backend exists.
