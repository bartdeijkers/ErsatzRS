# Parity matrix

> **Moved.** The per-feature parity data that used to live in one large table
> here is now the structured source of truth under
> [`docs/progress/`](../progress/README.md) — one file per feature at
> `docs/progress/parity/<slug>.toml`. This keeps reads cheap and diffs small.
> Query it instead of loading a wall of text:
>
> ```bash
> cargo xtask progress parity                  # compact feature | status list
> cargo xtask progress parity --status 'In progress'
> cargo xtask progress parity --feature <slug> --full   # full summary + ADRs
> cargo xtask progress gaps                    # everything not yet Ported
> cargo xtask progress adr list                # derived ADR index
> ```

This is the living map from ErsatzTV behavior to ErsatzRS implementation.
Phase-level progress comes from `cargo xtask progress next`; [STATUS.md](../../STATUS.md)
is a short pointer to that structured state. Persistence-level table coverage
is tracked separately in
[persistence-gap-matrix.md](persistence-gap-matrix.md).

Each `parity/<slug>.toml` records the ErsatzTV source, the ErsatzRS target, a
status, ADR references, and a summary. The ADR index is derived from
`docs/adr/` and reverse-linked through each feature's `adr` field. Statuses:

- `Not started`
- `In progress`
- `Ported`
- `Deferred`
- `Dropped`

Every PR that completes or intentionally changes parity updates the relevant
`docs/progress/parity/<slug>.toml` (see the `update-parity` skill) and runs
`cargo xtask progress check`.

## Admin UI route coverage

Route coverage (advertised nav link → backing route, plus advertised-but-404
placeholders) now lives in [`docs/progress/routes.toml`](../progress/routes.toml).
Query it with:

```bash
cargo xtask progress routes              # all routes
cargo xtask progress routes --unrouted   # advertised-but-404 placeholders only
```

When closing a route gap, set `routed = true` on its `[[route]]` entry and
update the matching `parity/<slug>.toml` feature file.
