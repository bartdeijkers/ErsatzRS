---
name: rust-inner-loop
description: Select and run the cheapest faithful ErsatzRS Rust edit loop. Use when implementing or debugging Rust, choosing affected crates/tests, investigating slow builds, or preparing one final closeout; do not use it to weaken milestone acceptance.
---

# Rust Inner Loop

## Select the loop

1. Read the inner-loop section of repository-root `BUILD.md`.
2. Run `cargo xtask affected-checks` for the working tree, or pass explicit
   repository-relative paths when the intended slice is narrower.
3. Inspect `.cargo/config.toml` and `CARGO_TARGET_DIR`. Use the configured target
   directory; never create a faster-looking alternate build path.
4. Choose one feature/target/profile graph for the whole edit loop.

Use the platform wrapper printed by `affected-checks`:

- Logic: `./run.sh check -p <crate>` or `.\run.cmd check -p <crate>`.
- Focused regression: the matching `test -p <crate> <filter>` command, which
  uses nextest.
- Admin UI: `dev`/`serve` so SSR and hydration assets stay from one build.

Do not alternate native, all-targets, WASM, and release graphs after every
edit. Do not run direct `cargo test`. Do not clean a warm target merely
because a build is slow; use `cargo xtask target-size` and the documented
40-GiB policy.

## Escalate deliberately

- Add consumer-crate checks when a public type, feature, serialized contract,
  SQL shape, or crate boundary changed.
- Use `affected-checks` to identify hydration, migration, playback or architecture
  surfaces. Select checks appropriate to the changed behavior; its printed
  recommendations do not require repeated broad gates during iteration.
- Suspect the stale-binary trap when runtime behavior disagrees with source:
  identify the executable under the configured target/package before debugging
  code further.
- Select broad verification from the delivery claim in `CONTRIBUTING.md` and its
  existing requirements. A passing focused loop does not itself require every
  broad gate. Reuse valid results and run outstanding required gates before
  claiming the corresponding acceptance.

Use the Session Handoff format in `AGENTS.md`; reference the selected Cargo graph
and results in canonical evidence, and identify required gates still due.
Never describe an unrun closeout gate as passed.
