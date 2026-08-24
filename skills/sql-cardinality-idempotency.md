---
name: sql-cardinality-idempotency
description: Audit and implement SQL repository reads or projection writes whose joins, related rows, refreshes, or repairs can duplicate logical media. Use for metadata/artwork projection, list queries, replace operations, deduplication, and cardinality or query-performance regressions.
---

# SQL Cardinality and Idempotency

Read [STYLE.md](../STYLE.md#sql), the repository trait, both backend queries,
and [add-migration](add-migration.md) when persisted repair is required.

## Define invariants first

- Name the logical row key and expected cardinality for every public query.
- Name the write identity for every related row, including normalized path/URL,
  kind, owner, and any ordering/language dimension that makes a variant real.
- State what a second identical Publish, scan, refresh, or retry must change.

## Protect both sides

1. Make the producer idempotent before insertion/replacement.
2. Make the consumer cardinality-safe with a deterministic preferred related
   row. Do not use broad `DISTINCT` to hide an unexplained one-to-many join.
3. Add fixtures containing zero, one, duplicate, and genuinely different
   related rows. Assert logical count, page count, links, and selected artwork.
4. Repeat the write and restart the repository; row counts and identities must
   remain stable.
5. Inspect the SQLite query plan and the equivalent MySQL indexes when a join,
   correlated selection, or grouping changes. Correctness must not introduce a
   table scan on a hot media-browser path.

For existing data, offer an aggregate dry-run and a transactional repair that
deletes only exact extras while preserving distinct variants and operator-owned
rows. Test a fresh database, a legacy duplicate fixture, rollback/failure, and
both backends. Extend health when duplicate state affects visible media or
port-completeness readiness.
