---
name: port-ersatztv
description: Plan a NasTv port from an ErsatzTV C# source file or feature. Use when asked to map an ErsatzTV file, directory, class, or behavior to Rust targets before implementation.
---

# Port ErsatzTV

Produce a plan only. Do not write code.

## Inputs

- A C# file, directory, class, or feature under `/home/bart/ErsatzTV/`.
- Optional target phase or crate hint.

## Workflow

1. Read [PORTING.md](../../PORTING.md).
2. Read [ARCHITECTURE.md](../../ARCHITECTURE.md).
3. Read the requested ErsatzTV source.
4. Identify the NasTv target crate and module.
5. Map important C# concepts to Rust idioms.
6. Identify traits, structs, tests, snapshots, and ADR needs.
7. Point to the parity-matrix row that should be added or updated.

## Output

Use this shape:

```text
Source
Target
Behavior summary
Rust mapping
Implementation steps
Tests and snapshots
ADR/parity updates
Open questions
```
