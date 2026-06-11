---
name: port-ersatztv
description: Plan an ErsatzRS port from an ErsatzTV C# source file or feature. Use when asked to map an ErsatzTV file, directory, class, or behavior to Rust targets before implementation.
---

# Port ErsatzTV

Produce a plan only. Do not write code. For implementation, switch to the
`parity-slice` skill so the no-stopgap acceptance gate is applied.

## Inputs

- A C# file, directory, class, or feature under `$ERSATZTV_SRC/`.
- Optional target phase or crate hint.

## Workflow

1. Read [PORTING.md](../PORTING.md).
2. Read [ARCHITECTURE.md](../ARCHITECTURE.md).
3. Read the requested ErsatzTV source.
4. Build a source map that covers UI/pages, components/view models,
   validators, handlers/services, repository/query paths, and side effects.
5. Identify the ErsatzRS target crate and module.
6. Map important C# concepts to Rust idioms.
7. Identify traits, structs, tests, snapshots, health/troubleshooting impact,
   and ADR needs.
8. Point to the `docs/progress/parity/<slug>.toml` feature file that should be
   added or updated (see the `update-parity` skill).
9. List any known `ParityDebt`; use `None` only when there is no hidden
   deferred behavior.

## Output

Use this shape:

```text
Source
Source map
Target
Behavior summary
Rust mapping
Implementation steps
Tests and snapshots
ADR/parity updates
ParityDebt
Open questions
```
