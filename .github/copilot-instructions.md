# GitHub Copilot Instructions

This is a Rust porting project. The authoritative agent instructions live in
[AGENTS.md](../../AGENTS.md) at the repository root. Read that file for the full
project context, architecture, porting workflow, and contract index.

## Quick Reference

- **Project:** ErsatzRS — Rust reimplementation of ErsatzTV
- **Status:** Phase 7 — Hardening and Parity Completion (Phases 0–6 complete).
  Run `cargo xtask progress next` for the live state; structured detail lives in
  [`docs/progress/`](../../docs/progress/README.md).
- **Plan:** [plans/INDEX.md](../../plans/INDEX.md)
- **ErsatzTV source:** `$ERSATZTV_SRC/`

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
- Record sessions with `cargo xtask progress add-session` and update
  `docs/progress/` (parity/phase) when ending work.
