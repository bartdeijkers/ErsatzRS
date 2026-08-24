# ADR 0080: Standardize on SQLite as the sole database backend

## Status

Accepted

## Implementation

Implemented (2026-08-24). Runtime and Cargo are SQLite-only: the MySQL
features, dependencies, migration tree, dialect branches, integration tests,
CI service job, dual-feature check graphs, dependency-advisory exception, and
current support documentation are removed. `mysql://` and `mariadb://` startup
URLs fail with a permanent SQLite-only diagnostic. The maintainer confirmed
that no ErsatzRS user deployment ever used the optional MySQL/MariaDB backend,
so there is no deployed operator data to convert and no migration utility or
runbook is required.

## Context

ErsatzRS currently uses SQLite by default and also carries MySQL and MariaDB
support behind Cargo feature flags. Supporting both backends requires paired
migration trees, backend-aware SQL generation, two compile and test graphs,
live MySQL integration infrastructure, and MySQL-specific authentication and
cryptography dependencies. Every persistence change must either be proved on
both backends or leave backend-specific release debt.

The runtime architecture does not require a client-server database. ErsatzRS
is deployed as one media-server host, with a scanner companion that shares the
same persistence layer. SQLite already provides the required transactional
storage, foreign-key enforcement, write serialization, busy handling, and
concurrent reads through WAL mode. Active-active ErsatzRS servers sharing one
database are not a supported topology.

Official release archives are built without the opt-in `mysql` feature. MySQL
therefore increases development and release complexity without expanding the
capabilities of the published binaries. Retaining it also makes SQLite
correctness work compete with a second backend whose runtime acceptance
regularly requires separate infrastructure.

ErsatzTV supports MySQL, so removing this backend is a deliberate parity
deviation and requires an explicit decision rather than an incidental cleanup.

## Decision

SQLite will be the sole supported ErsatzRS database backend.

After this ADR is accepted, implementation will:

- remove the `mysql` and `mysql-integration` Cargo features and MySQL-specific
  dependencies;
- remove the MySQL migration tree, dialect branches, integration jobs, and
  backend-specific documentation;
- reject `mysql://` and `mariadb://` database URLs with a clear diagnostic;
- keep the existing SQLite default location and SQLite URL configuration;
- keep WAL mode, foreign-key enforcement, the busy timeout, immediate write
  transactions, bounded write retries, and database health checks as supported
  runtime contracts; and
- simplify repository internals when an abstraction exists only to select a
  SQL dialect, while retaining domain-facing repository boundaries.

The web host and scanner may access the same SQLite database on one host. The
database file must live on a filesystem with correct local locking semantics;
network shares and multiple active ErsatzRS server instances are unsupported.

The maintainer confirmed that no ErsatzRS user deployment ever used the
optional source-built MySQL/MariaDB backend. There is therefore no operator
migration population and no conversion utility or release runbook will be
published. SQLite-only binaries still reject historical MySQL/MariaDB URLs
explicitly instead of silently creating or selecting another database.

Adding another production database backend in the future requires a new ADR
with a demonstrated deployment need, an ownership model for migrations and
live integration tests, and evidence that SQLite is the actual limiting
component.

## Consequences

- Persistence changes need one migration, one SQL dialect, and one primary
  runtime acceptance path.
- CI, release closeout, dependency review, and local development become
  smaller and less dependent on external database infrastructure.
- SQLite reliability and performance work directly benefits every supported
  installation instead of one backend receiving weaker proof.
- MySQL-specific dependencies and their security and maintenance surface leave
  the production graph.
- ErsatzRS deliberately loses MySQL/MariaDB parity with ErsatzTV.
- No operator migration is required because the optional MySQL/MariaDB backend
  had no user deployments before its removal.
- SQLite's single-writer model remains a design constraint. New write-heavy
  features must use short transactions, bounded concurrency, and idempotent
  recovery rather than assuming a client-server database will absorb
  unbounded parallel writes.
- Active-active server deployments and database files on network filesystems
  remain outside the support contract.

## Alternatives Considered

- Keep MySQL as an optional backend: rejected because optional support still
  doubles migrations and meaningful persistence proof, while official release
  artifacts do not expose it.
- Freeze MySQL without testing new migrations: rejected because a nominally
  supported backend that can silently drift is more dangerous than an explicit
  removal.
- Keep only the database abstraction for a hypothetical future backend:
  rejected where the abstraction exists solely for SQL dialect selection;
  domain repository boundaries remain sufficient for a later redesign.
- Replace MySQL with PostgreSQL: rejected because changing the client-server
  backend does not establish a product requirement for one and preserves most
  of the same maintenance cost.
- Support active-active ErsatzRS instances through MySQL: rejected because
  database connectivity alone does not make scheduler ownership, background
  jobs, filesystem state, FFmpeg processes, or stream sessions safe for
  multiple active servers.

## References

- [ADR 0004: Collapse the initial database schema](0004-collapsed-initial-schema.md)
- [ADR 0006: Use sqlx for persistence](0006-sqlx-over-sea-orm.md)
- [Initial schema and database backend parity](../progress/parity/initial-schema-database-backends.toml)
- [Configuration reference](../reference/configuration.md)
- [System requirements](../reference/system-requirements.md)
- [Architecture contract](../../ARCHITECTURE.md)
- [ErsatzTV porting contract](../../PORTING.md)
