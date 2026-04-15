# GitHub Copilot Instructions

This is a Rust porting project. The authoritative agent instructions live in
[AGENTS.md](../../AGENTS.md) at the repository root. Read that file for the full
project context, architecture, porting workflow, and contract index.

## Quick Reference

- **Project:** NasTv — Rust reimplementation of ErsatzTV
- **Status:** Phase 0 bootstrap, partially complete. No Cargo.toml yet. See
  [STATUS.md](../../STATUS.md) for the checklist and session log.
- **Plan:** [plans/sequential-dancing-valiant.md](../../plans/sequential-dancing-valiant.md)
- **ErsatzTV source:** `/home/bart/ErsatzTV/`

## Key Commands (once Cargo workspace exists)

```bash
cargo build --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo deny check
```

## Rules

- Read the ErsatzTV source before writing Rust ports.
- Preserve ErsatzTV behavior unless an ADR records a deviation.
- Snapshot-test generated FFmpeg argv with `insta`.
- Follow [STYLE.md](../../STYLE.md), [ARCHITECTURE.md](../../ARCHITECTURE.md),
  and [PORTING.md](../../PORTING.md).
- Update [STATUS.md](../../STATUS.md) session log when ending work.
