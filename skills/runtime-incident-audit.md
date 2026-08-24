---
name: runtime-incident-audit
description: Diagnose an ErsatzRS runtime incident by correlating the launched binary, bounded logs, health, read-only database aggregates, and source queries. Use for repeats, missing/duplicate media, unexpected builds, stale binaries, or behavior that differs from the admin UI; diagnosis does not authorize a fix or live-data mutation.
---

# Runtime Incident Audit

## Safety boundary

Treat the running installation and its database as user data. Start read-only,
do not restart services or edit rows unless the user explicitly requests that
action, and never commit titles, URLs, credentials, media paths, or database
copies. Prefer aggregate counts and stable schema relationships.

## Evidence chain

1. Record the observation window, server version/commit when available, process
   executable path, executable hash, database backend, and health result.
2. Confirm the executable is the configured-target or packaged binary rather
   than a stale `target/debug` artifact.
3. Read only the bounded log window surrounding the event. Correlate scan,
   build, scheduling, and publication timestamps before inferring causality.
4. For SQLite, run
   `cargo xtask runtime-audit --db <database> --redact`. It opens the file
   read-only and reports integrity, Classic anchor state, playout timeline
   anomalies, and episode/artwork cardinality without titles or paths. For
   MySQL, use equivalent read-only aggregate queries; do not convert or mutate a
   database merely to use this helper.
5. Count at each layer: provider identity, `MediaItem`, subtype, metadata,
   related rows, repository query rows, and rendered rows. This distinguishes a
   duplicate import from a one-to-many join multiplying one logical item.
6. Map every confirmed transition to the owning source query/function and the
   accepted ADR/parity contract.

## Result

Report confirmed facts, excluded explanations, remaining hypotheses, root
cause with confidence, the smallest failing regression scenario, and a repair
plan. Keep investigation and implementation separate unless the user requested
both.
