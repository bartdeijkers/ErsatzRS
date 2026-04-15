---
name: new-crate
description: Scaffold a new ErsatzRS workspace crate according to the architecture contract. Use when adding crates under crates/ and wiring them into the Cargo workspace.
---

# New Crate

Use this only after the root Cargo workspace exists.

## Workflow

1. Read [ARCHITECTURE.md](../../ARCHITECTURE.md).
2. Confirm the crate belongs in the planned workspace layout.
3. Create `crates/<name>/Cargo.toml`.
4. Create `crates/<name>/src/lib.rs`.
5. Add `crates/<name>/src/error.rs` when the crate has typed errors.
6. Add `crates/<name>/tests/.gitkeep`.
7. Add the crate to workspace `members`.
8. Apply workspace lints.
9. Run `cargo fmt --all -- --check`.

## Constraints

- Do not create dependency cycles.
- Do not put infrastructure concerns in `ErsatzRS-core`.
- Do not put domain types outside `ErsatzRS-domain` without a clear reason.
