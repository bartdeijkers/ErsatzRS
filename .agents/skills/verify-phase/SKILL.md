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

- Identify the requested delivery claim using `CONTRIBUTING.md`. For feature
  acceptance, prove every clause with the required command result, persisted
  state or browser/runtime observation. Record unavailable or failing proof as
  a blocker, not acceptance.
- Use `./run.sh test -p <crate> <filter>` on POSIX and
  `.\run.cmd test -p <crate> <filter>` on Windows. Never replace the required
  nextest wrapper with direct `cargo test`.
- Keep one feature/target/profile configuration during iteration. Select broad
  verification from the claim being finalized and its existing requirements.
  Reuse valid results; run outstanding required gates before claiming the
  corresponding acceptance. A passing focused loop does not itself require
  every broad gate.
- Do not mark a milestone Complete without concrete evidence in
  `docs/progress/milestones.toml` and a passing
  `cargo xtask progress check`.

An evidence string does not prove acceptance, and an agent's completion claim
is not independent verification. A validator cannot reliably infer maintainer
approval from arbitrary prose.

## Output

Use the Session Handoff format in `AGENTS.md`, naming the requested milestone
or phase and linking to its canonical evidence. Follow `skills/update-parity.md`
for evidence ownership; distinguish verified acceptance from partial progress.
