# Code Style

This is the single, normative style contract for ErsatzRS. It covers:

1. **Rust** — how we write Rust in this repository.
2. **Rust ↔ C# parity** — when ErsatzTV C# patterns win over idiomatic Rust
   because persisted data or public behavior depends on them.
3. **SQL** — how we write SQLite (and MySQL-behind-feature-flag) schema and
   migrations so they remain compatible with the ErsatzTV reference model.

If you find style guidance anywhere else in the tree, treat it as a pointer to
this file. Do not create a second style document.

## Baseline

- Use stable Rust through [rust-toolchain.toml](rust-toolchain.toml).
- Keep the MSRV policy at "current stable minus one" once releases begin.
- Use Rust 2021 until an ADR upgrades the edition.
- Prefer explicit modules and small files over large catch-all modules.
- Keep public APIs documented in library crates.

## Naming (Rust)

- Crates use lowercase `ersatzrs-*` (for example `ersatzrs-core`,
  `ersatzrs-web`).
- Modules, functions, variables, and fields use `snake_case`.
- Types, traits, and enum variants use `PascalCase`.
- Constants use `SCREAMING_SNAKE_CASE`.

Database identifiers follow the SQL rules below, not Rust `snake_case`. When
mapping a Rust struct field to a database column, use `#[sqlx(rename = "...")]`
to bridge the two conventions rather than renaming either side.

## Rust ↔ C# Parity

ErsatzRS is primarily a port. Where persisted data or public behavior crosses
the Rust/C# boundary, the C# shape wins by default.

- Treat ErsatzTV-generated strings stored in the database or returned through
  scheduling APIs as compatibility contracts. This includes JSON payloads such
  as `BlockKey`, `CollectionKey`, `SchedulingContext`, playout history keys,
  and history details.
- Match ErsatzTV serializer behavior for those contracts instead of applying
  Rust naming conventions by default. Preserve property casing, omitted/null
  fields, enum representation, and deterministic property order when tests or
  persisted data depend on them.
- When porting C# code that calls `JsonConvert.SerializeObject` or
  `System.Text.Json.JsonSerializer.Serialize`, read the serializer settings and
  add a fixture or snapshot that captures the produced string before relying on
  derived `serde` output.
- If a Rust representation intentionally differs from ErsatzTV serialized
  output, document the reason in a code comment that references the C# source
  path and line, and add or update an ADR if the difference affects persisted
  data or public behavior.
- LanguageExt `Option`/`Either` map to native `Option`/`Result`. Use `anyhow`
  for binary-level errors and `thiserror` for typed library errors (see
  [PORTING.md](PORTING.md) for the full mapping table).

## Errors

- Library crates use `thiserror` for typed errors.
- Binaries and top-level entry points may use `anyhow`.
- Use `Result<T, E>` for recoverable failures.
- Avoid panics in library code.
- `unwrap` and `expect` are allowed only in tests, doctests, and startup code
  where failure is unrecoverable and the message explains the invariant.

## Async

- Use Tokio for async runtime behavior.
- Do not block inside async code. Use `spawn_blocking` for blocking filesystem or
  CPU-heavy work when needed.
- Pass cancellation-aware handles where long-running FFmpeg, scanner, or
  scheduling operations are involved.

## Logging

- Use `tracing`, not `println!`, in library code.
- Log operational state with fields, not string interpolation alone.
- Never log secrets, bearer tokens, cookies, database passwords, or full config
  dumps.
- Follow [docs/how-to/logging.md](docs/how-to/logging.md).

## Public API Documentation

Public items in `ersatzrs-core`, `ersatzrs-ffmpeg`, and `ersatzrs-app` must include
rustdoc. Prefer examples for stable APIs and explain invariants for builders,
state machines, and generated FFmpeg arguments.

## Unsafe Code

Unsafe Rust is denied by default. Any future exception requires an ADR and must
be isolated behind a safe API.

## Dependencies

- Prefer small, maintained crates with clear licenses.
- Add cross-crate dependencies only when the owning crate boundary still matches
  [ARCHITECTURE.md](ARCHITECTURE.md).
- Use [deny.toml](deny.toml) to enforce license and advisory policy once the
  workspace exists.

## SQL

ErsatzRS ships a collapsed initial schema derived from the final ErsatzTV EF
Core model (see [ADR 0004](docs/adr/0004-collapsed-initial-schema.md)). That
schema is a **porting parity contract**: table names, column names, constraint
names, and index names are observed by migrations, sqlx queries, and any
tooling that compares against an ErsatzTV database. The canonical reference is
[migrations/0001_initial.sql](migrations/0001_initial.sql).

### Identifiers

- Quote every identifier with double quotes: `"Channel"`, `"PlayoutItem"`,
  `"Id"`. This is the EF Core default and what the collapsed schema uses.
- Table and column names use `PascalCase` to match ErsatzTV
  (`ArtistMetadata`, `ReleaseDate`, `PreferredAudioLanguageCode`).
- Do **not** rename existing identifiers to `snake_case`, even when a new
  migration touches them. Renames break parity with the ErsatzTV reference
  schema and invalidate any comparison tooling.
- New tables and columns that have no ErsatzTV equivalent still use
  `PascalCase` for consistency; note the deviation from upstream in the
  migration's header comment.

### Keywords and formatting

- SQL keywords are uppercase: `CREATE TABLE`, `CONSTRAINT`, `FOREIGN KEY`,
  `REFERENCES`, `NOT NULL`, `NULL`, `PRIMARY KEY`, `AUTOINCREMENT`,
  `ON DELETE CASCADE`, `ON DELETE SET NULL`, `COLLATE NOCASE`, `DEFAULT`.
- Column and constraint lists inside `CREATE TABLE` are indented four spaces,
  one entry per line, with a trailing comma on every entry except the last.
- Place all column definitions before any `CONSTRAINT` definitions inside the
  table body.

### Column types

- Prefer SQLite native affinities: `INTEGER`, `TEXT`, `REAL`, `BLOB`.
- `varchar(n)` appears in the collapsed schema because EF generated it. Leave
  existing `varchar(n)` columns as-is; do not introduce new ones — use `TEXT`
  instead.
- Apply `COLLATE NOCASE` to user-facing name columns that need case-insensitive
  search and uniqueness (the rule ErsatzTV follows for `Name`-style columns).
- Represent booleans as `INTEGER NOT NULL` with `DEFAULT 0` or `DEFAULT 1`.
- Represent `TimeSpan` and `DateTimeOffset` as `TEXT` using the ISO formats
  EF Core writes (`'HH:MM:SS'`, ISO 8601). Match the existing column's format
  when adding a new one of the same logical type.

### Constraint and index names

These patterns come from EF Core and are used throughout the collapsed schema.
Preserve them so parity tooling and `cargo insta` snapshots stay stable.

- Primary key: `PK_<TableName>`.
- Foreign key: `FK_<TableName>_<ReferencedTable>_<ColumnName>`.
  Example: `FK_Actor_ArtistMetadata_ArtistMetadataId`.
- Non-unique index: `IX_<TableName>_<ColumnName>`; composite indexes append
  more columns with underscores (`IX_PlayoutItem_Start_Finish`).
- Unique index: same `IX_` prefix, created via `CREATE UNIQUE INDEX`
  (`IX_Channel_Number`, `IX_Block_BlockGroupId_Name`).
- Declare the primary key inline on the `Id` column using a named constraint:
  `"Id" INTEGER NOT NULL CONSTRAINT "PK_<TableName>" PRIMARY KEY AUTOINCREMENT`.
- Declare composite primary keys as a table-level `CONSTRAINT "PK_..." PRIMARY KEY (...)`.

### Referential actions

Match the ErsatzTV `OnDelete` choice for the relationship being ported:

- Use `ON DELETE CASCADE` for rows owned by a parent (metadata rows, join
  tables, per-parent configuration).
- Use `ON DELETE SET NULL` for optional references where the child should
  survive the parent's deletion (for example `Channel.WatermarkId`,
  `Channel.FallbackFillerId`, `Channel.MirrorSourceChannelId`).
- Do not use `ON DELETE RESTRICT` or `ON DELETE NO ACTION` unless the
  ErsatzTV source uses it; document any deviation in the migration.

### Generated and computed columns

Use SQLite's inline `AS (...)` syntax on the same line as the column, mirroring
the ErsatzTV form. Example from the initial schema:

```sql
"SourcePath" TEXT NULL,
"IsMetadataOrphan" AS (CASE WHEN COALESCE(...) IS NULL THEN 1 ELSE NULL END),
```

### Migrations

- Write each migration as a single `.sql` file under `migrations/` using the
  naming rule in [docs/how-to/add-migration.md](docs/how-to/add-migration.md).
- Prefer additive changes. Do not drop columns or tables without an ADR.
- When porting an ErsatzTV EF migration, reference the C# migration file in a
  header comment so reviewers can compare line-for-line.
- Add a fixture or `insta` snapshot whenever a migration changes a shape that
  sqlx queries depend on, so the query regression surfaces with the schema
  change.

### MySQL backend

The MySQL backend is behind a feature flag. When that backend gains its own
migrations:

- Keep identifier casing identical to the SQLite schema. Use backticks for
  quoting (`` `Channel` ``) instead of double quotes.
- Map types as EF Core does: `INTEGER` → `INT`, `TEXT` → `LONGTEXT` or
  `VARCHAR(n)` where EF used `varchar(n)`, `REAL` → `DOUBLE`.
- Mirror the same constraint and index names.
