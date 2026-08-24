# Add a migration

Use this guide to add a SQLite schema change. Migrations live under
`migrations/`.

Read the SQL and migration rules in [STYLE.md](../../STYLE.md#sql) before
editing schema files. They are the canonical owner for identifier, constraint,
index, and destructive-change policy.

## Add the migration

1. Choose one UTC timestamp and snake-case description.
2. Add `migrations/<timestamp>_<description>.sql` for SQLite.
3. Preserve the identifier, constraint, index, and destructive-change rules in
   [STYLE.md](../../STYLE.md#sql).
4. Add or update a repository or bootstrap regression that observes the new
   schema after all migrations run.

Use this filename shape:

```text
YYYYMMDDHHMM_<snake_description>.sql
```

Example:

```text
202608111230_add_channel_notes.sql
```

## Verify the migration

Run focused SQLite migration coverage:

```bash
./run.sh test -p ersatzrs-infra migration
```

The task is complete when the migration tree compiles, a fresh database
reaches the expected schema, and focused tests observe the new column, table,
index, or constraint. Run `cargo xtask progress check` if the schema change also
updates parity or milestone evidence.
