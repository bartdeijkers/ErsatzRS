---
name: stateful-refresh-safety
description: Preserve scheduling continuity when changing Classic scheduling, playout Refresh/Reset, anchors, shuffled enumerators, scan invalidation, or membership reconciliation. Use when persisted state is cleared, restored, re-keyed, or used to select affected playouts.
---

# Stateful Refresh Safety

Read ADRs 0010, 0019, and 0020, the relevant parity debt, the active corrective
plan, and the pinned ErsatzTV scheduling source before editing behavior.

## Continuity contract

Treat an uninterrupted build as the oracle. Compare its future media identities,
timestamps, scheduling contexts, and mutable setting snapshots against:

1. fresh build plus Continue;
2. fresh build, process restart, then Continue;
3. fresh build, restart, then relevant Refresh;
4. the same sequence with an unrelated scan or membership change;
5. relevant candidate additions, removals, replacements, and re-identification;
6. an explicitly confirmed Reset.

Refresh may rewrite and re-stamp the active window, but it must not silently
become Reset, replay consumed shuffled media, or modify unrelated playouts.

## Required evidence

- Generate checkpoints through the production builder and repository path. A
  test that only hand-injects the desired anchor is insufficient.
- Persist generated items, continuation state, and rebuild checkpoints
  transactionally; validate the complete restore set before clearing rows.
- Exercise day seams, duplicate source references, Shuffle, Random,
  RandomRotation, and the relevant collection types.
- Resolve scan impact through schedule/content dependencies, never merely from
  the existence of an anchor. Coalesce to one reasoned request per playout.
- Test existing databases with missing, undated, stale, or version-incompatible
  state. Refuse an unsafe clear observably rather than guessing a fresh Reset.
- Update the Classic Playout State health contract whenever readiness to
  continue or refresh changes.

Run focused core/infra/scanner tests in one Cargo graph first, then SQLite,
MySQL, restart, runtime smoke, and playback-sensitive closeout gates required by
the owning milestone.
