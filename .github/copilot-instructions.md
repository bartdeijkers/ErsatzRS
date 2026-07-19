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

## Key Commands

```bash
cargo build --workspace
cargo fmt --all -- --check
# Lint mirrors CI: two passes, neither with --all-features.
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy --workspace --all-targets --features mysql -- -D warnings
./run.sh test-closeout
cargo deny check
```

## Rules

- Read the ErsatzTV source before writing Rust ports.
- Preserve ErsatzTV behavior unless an ADR records a deviation.
- Use `./run.sh test -p <crate> [filter]` for focused Rust tests. Do not invoke
  `cargo test` directly; CI is the only direct-Cargo exception.
- Snapshot-test generated FFmpeg argv with `insta`.
- Follow [STYLE.md](../../STYLE.md), [ARCHITECTURE.md](../../ARCHITECTURE.md),
  and [PORTING.md](../../PORTING.md).
- Record sessions with `cargo xtask progress add-session` and update
  `docs/progress/` (parity/phase) when ending work.
