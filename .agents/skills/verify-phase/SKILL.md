---
name: verify-phase
description: Verify an ErsatzRS phase or milestone against the current structured progress acceptance. Use for phase closeout, milestone evidence, release-gate selection, or roadmap validation; do not use an obsolete standalone plan as the source of truth.
---

# Verify Phase

## Source of truth

1. Run `cargo xtask progress next`.
2. Query each requested milestone with
   `cargo xtask progress milestones --id <id> --full`.
3. Read the linked live plan and parity file only for details not present in
   milestone acceptance. `docs/progress/` remains authoritative.
4. Run `cargo xtask affected-checks` or pass the changed paths explicitly to
   select the smallest faithful inner-loop checks.

## Verification

- Prove every acceptance clause with a command result, persisted state,
  browser/runtime observation, or an explicit unavailable/blocking reason.
- Use `./run.sh test -p <crate> <filter>` on POSIX and
  `.\run.cmd test -p <crate> <filter>` on Windows. Never replace the required
  nextest wrapper with direct `cargo test`.
- Keep one feature/target/profile configuration during iteration. Run the two
  Clippy configurations, test closeout, Leptos/parity gates, and other broad
  milestone gates once after focused checks pass.
- Do not mark a milestone Complete without concrete evidence in
  `docs/progress/milestones.toml` and a passing
  `cargo xtask progress check`.

## Output

Use this shape:

```text
Phase
Milestones and acceptance
Checks run
Checks unavailable
Failures
Evidence recorded
Next fixes
```
