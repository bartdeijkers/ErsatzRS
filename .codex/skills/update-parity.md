---
name: update-parity
description: Update the ErsatzRS ErsatzTV parity matrix. Use when a porting task starts, progresses, completes, is deferred, or is deliberately dropped.
---

# Update Parity

## Workflow

1. Open [docs/reference/parity-matrix.md](../../docs/reference/parity-matrix.md).
2. Find or add the ErsatzTV feature row.
3. Include the ErsatzTV source path.
4. Include the ErsatzRS target crate or module.
5. Set status to one of:
   - `Not started`
   - `In progress`
   - `Ported`
   - `Deferred`
   - `Dropped`
6. Link any ADR that explains a deviation or dropped behavior.

## Rule

Every PR that completes or intentionally changes parity updates this matrix.
