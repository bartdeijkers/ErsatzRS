---
name: verify-phase
description: Run or plan the verification checks for a phase from the ErsatzRS implementation plan. Use when asked to validate phase 0, phase 1, or later roadmap progress.
---

# Verify Phase

## Workflow

1. Open [plans/sequential-dancing-valiant.md](../../plans/sequential-dancing-valiant.md).
2. Find the requested phase and its verification section.
3. Translate the phase checks into concrete local commands.
4. Run checks that are available in the current repository state.
5. Report unavailable checks explicitly when the workspace or feature does not
   exist yet.

## Output

Use this shape:

```text
Phase
Checks run
Checks unavailable
Failures
Next fixes
```
