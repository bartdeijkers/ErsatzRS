---
name: rust-inner-loop
description: Select and run the cheapest faithful ErsatzRS Rust edit loop. Use when implementing or debugging Rust, choosing affected crates/tests, investigating slow builds, or preparing one final closeout; do not use it to weaken milestone acceptance.
---

# Rust Inner Loop

## Select the loop

1. Read the inner-loop section of [BUILD.md](../BUILD.md).
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

Do not alternate default, MySQL, all-targets, WASM, and release graphs after
every edit. Do not run direct `cargo test`. Do not clean a warm target merely
because a build is slow; use `cargo xtask target-size` and the documented
40-GiB policy.

## Escalate deliberately

- Add consumer-crate checks when a public type, feature, serialized contract,
  SQL shape, or crate boundary changed.
- Add hydration, migration, playback, or architecture gates when
  `affected-checks` identifies that surface.
- Suspect the stale-binary trap when runtime behavior disagrees with source:
  identify the executable under the configured target/package before debugging
  code further.
- Run the two Clippy configurations, `test-closeout`, Leptos/parity gates, and
  other broad milestone requirements once after the focused loop is green.

Report the selected Cargo graph, focused commands and outcomes, and the broad
gates still due. Never describe an unrun closeout gate as passed.
