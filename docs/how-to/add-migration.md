# Add a migration

Use this guide to add a schema change for SQLite and, where supported, MySQL.
Migrations live under `migrations/`.

Read the SQL and migration rules in [STYLE.md](../../STYLE.md#sql) before
editing schema files. They are the canonical owner for identifier, constraint,
index, and destructive-change policy.

## Add the paired migrations

1. Choose one UTC timestamp and snake-case description.
2. Add `migrations/<timestamp>_<description>.sql` for SQLite.
3. Add the matching file under `migrations/mysql/` when the schema surface is
   supported on both backends.
4. Keep table, column, constraint, and index names equivalent across both
   files; vary only backend syntax.
5. Add or update a repository or bootstrap regression that observes the new
   schema after all migrations run.

Use this filename shape:

```text
YYYYMMDDHHMM_<snake_description>.sql
```

Example:

```text
202608111230_add_channel_notes.sql
```

## Verify both backends

Run focused SQLite migration coverage:

```bash
./run.sh test -p ersatzrs-infra migration
```

For a paired backend change, run the MySQL integration feature as well. Local
environment-gated cases use `ERSATZRS_MYSQL_TEST_URL` as described in
[TEST.md](../../TEST.md#what-runs-where).

```bash
./run.sh test -p ersatzrs-infra --features mysql-integration migration
```

The task is complete when both migration trees compile, a fresh database
reaches the expected schema, and focused tests observe the new column, table,
index, or constraint. Run `cargo xtask progress check` if the schema change also
updates parity or milestone evidence.
