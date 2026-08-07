# Project Status

**Phase 9 — Teletext support** (Phases 0–8 complete).

The "The Box" video-jukebox extension is Phase 10.

> **Progress tracking moved.** The detailed phase narrative, per-feature parity
> matrix, route/hydration coverage, roadmap milestones, and the session log
> are now the structured source of truth under
> [`docs/progress/`](docs/progress/README.md). Query exactly what you need
> instead of loading a large file:
>
> ```bash
> cargo xtask progress next        # phase, focus, in-progress features, latest session
> cargo xtask progress gaps        # everything not yet Ported (what's left)
> cargo xtask progress parity --feature <slug> --full
> cargo xtask progress sessions --limit 10
> cargo xtask progress adr list
> ```

## How progress is tracked

| Need | Source of truth | Query |
| --- | --- | --- |
| Phase, focus, checklist, TODOs | `docs/progress/phase.toml` | `cargo xtask progress next` |
| Per-feature parity + ADR links | `docs/progress/parity/<slug>.toml` | `cargo xtask progress parity` / `gaps` |
| Admin UI route coverage | `docs/progress/routes.toml` | `cargo xtask progress routes` |
| Leptos hydration parity | `docs/progress/hydration.toml` | `cargo xtask progress hydration` |
| Session / handoff log | `docs/progress/sessions.ndjson` | `cargo xtask progress sessions` |

The queryable `progress.db` is gitignored and rebuilt automatically from these
text files; validate edits with `cargo xtask progress check`. See
[`docs/progress/README.md`](docs/progress/README.md) for the full model, and
[plans/INDEX.md](plans/INDEX.md) for locked decisions and the contract index.

## Session Handoff

When ending a work session, record it in the log and commit:

```bash
cargo xtask progress add-session --agent "<name>" --summary "<one line>"
git commit -m "docs(status): update session log"
```

If a porting task changed parity, also update the relevant
`docs/progress/parity/<slug>.toml` (see the `update-parity` skill) and run
`cargo xtask progress check`. Note remaining work for the next agent in
`docs/progress/phase.toml` (`next_steps`).
