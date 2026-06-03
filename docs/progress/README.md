# Progress tracking

This directory is the **source of truth** for ErsatzRS porting progress toward
a feature-complete ErsatzTV port. It replaces the large narrative tables that
used to live in `STATUS.md` and `docs/reference/parity-matrix.md` so that
agents and humans can retrieve exactly the state they need cheaply.

The data lives in small, diffable, git-tracked text files. A queryable SQLite
index (`progress.db` at the repo root) is **derived** from these files,
**gitignored**, and rebuilt on demand — it is a read accelerator, never the
source of truth, and is unrelated to the application's runtime database.

## Files

| File | Holds |
| --- | --- |
| `phase.toml` | Current phase, focus, checklist, known TODOs, blocking decisions. |
| `parity/<slug>.toml` | One file per ErsatzTV feature: source, target, status, ADR refs, summary. |
| `routes.toml` | Admin UI route coverage (`routed=false` = advertised-but-404 placeholder). |
| `hydration.toml` | Per-page Leptos islands hydration parity. |
| `sessions.ndjson` | Append-only session/handoff log, one JSON object per line. |

The ADR matrix is **derived** by scanning `docs/adr/*.md` and reverse-linking
features through each parity file's `adr` field; there is no separate ADR file
to maintain.

`status` values for `parity/*.toml`: `Not started`, `In progress`, `Ported`,
`Deferred`, `Dropped`.

## Querying (cheap reads)

```bash
cargo xtask progress next                 # current phase + focus + in-progress + latest session
cargo xtask progress gaps                 # everything not yet Ported (the "what's left" view)
cargo xtask progress parity --status 'In progress'
cargo xtask progress parity --feature ffmpeg-filter-graph --full
cargo xtask progress routes --unrouted
cargo xtask progress adr list
cargo xtask progress sessions --limit 10

# Ad-hoc SQL is fine too (DB is rebuilt automatically if stale):
sqlite3 progress.db "SELECT feature, status FROM parity WHERE status != 'Ported'"
```

## Updating (writes go to the text, not the DB)

- Parity change: edit `parity/<slug>.toml` (create a new file for a new
  feature). See the `update-parity` skill.
- New session/handoff entry: `cargo xtask progress add-session --agent "…"
  --summary "…"` (appends one NDJSON line), or edit `sessions.ndjson` directly.
- Validate before committing: `cargo xtask progress check`.

The DB refreshes itself when any source file is newer than `progress.db`, so an
explicit `cargo xtask progress sync` is rarely needed.
