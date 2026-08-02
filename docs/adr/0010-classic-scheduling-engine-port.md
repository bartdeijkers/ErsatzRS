# ADR 0010: Classic scheduling engine port

## Status

Accepted

## Implementation

Implemented — the Classic scheduling engine is ported (feature `classic-scheduling`).

## Context

ErsatzTV's Classic scheduling engine remains part of the non-web parity target
because imported ErsatzTV databases commonly contain `ProgramSchedule` /
`ProgramScheduleItem` rows. ErsatzRS must build those playouts without requiring
operators to rewrite schedules into Block, Sequential YAML, or Scripted Lua.

The Phase 7 Slice 15 work ported the Classic engine behavior currently needed
by the persisted schedule model: One, Multiple, Duration, and Flood items;
fixed-start behavior; schedule alternates; collection/media enumerator state;
program-schedule anchors; fill-group indices; guide groups; and compatible
`ClassicSchedulingContext` JSON.

## Decision

Keep Classic scheduling as a first-class in-process scheduling engine behind
the shared `PlayoutSchedulingEngine` trait. `PlayoutBuilder` dispatches
`PlayoutScheduleKind::Classic` to `ClassicSchedulingEngine`, while
`ersatzrs-infra` owns SQL-backed Classic reference-data loading from the
ErsatzTV-compatible schema.

Classic remains a compatibility engine. New Rust-only scheduling behavior
should be added to the Block, Sequential YAML, or Scripted engines unless it is
needed to preserve imported ErsatzTV Classic schedule behavior.

## Consequences

Imported ErsatzTV databases with Classic schedules can build playout items
through the same maintenance path as the other engines.

Classic-specific persisted state remains part of the parity contract:
`PlayoutProgramScheduleAnchor`, `PlayoutScheduleItemFillGroupIndex`,
`FillGroupEnumeratorState`, and Classic scheduling context JSON must not drift
from the ErsatzTV shape without a new ADR.

The remaining Classic follow-up is narrow: schedule-item filler preset fields
and generated fake group schedule items should be revisited only when the
corresponding ErsatzTV behavior is needed by real imported databases or the
final Slice 21 import verification.

### 2026-07-27 addendum: seed/anchor semantics fix and `RandomRotation`

`Playout.seed` was regenerated only at playout creation, so every later Reset
reproduced the same randomized order. Reset now mirrors ErsatzTV's
`ResetPlayout()` by generating and persisting a new seed.

Refresh continuation must be time-aligned. The playout anchor and undated
collection anchors describe the end of the previously generated horizon;
reusing them after clearing that horizon skips schedule items and media at the
new rebuild start. Refresh therefore resets schedule-item state and accepts
only same-day dated collection checkpoints. Without such a checkpoint it
starts from the playout seed, matching ErsatzTV's anchor filtering rather than
pretending an end-of-horizon position belongs at the rebuild start.

`PlaybackOrder::RandomRotation` is a deliberate, spirit-preserving adaptation
rather than a literal port of ErsatzTV's live
`RandomizedRotatingMediaCollectionEnumerator`. Upstream tracks per-draw group
state and bumps to a different group only when a live draw repeats the
previous one; ErsatzRS's `MediaEnumerator` instead materializes a full
arranged order once per cycle (the same pattern `Shuffle`/`ShuffleInOrder`
already use). The adaptation canonicalizes each cycle by ADR 0048 scheduling
item identity — media ID plus audio/subtitle overrides — randomizes items
within groups, and uses a max-heap to interleave repeated groups without
adjacent matches when avoidable. Including the override tuple is necessary
because one media item may appear multiple times with different track
selection; media ID alone leaves their relative order dependent on the prior
shuffle and cannot reconstruct the same persisted cycle. After O(n log n)
canonicalization, singleton groups need only one linear shuffle and a
constant-time seam adjustment; grouped collections add O(n log g) heap work,
avoiding the prior quadratic scan.

Classic anchor context version 2 stores the predecessor media ID used to
materialize each active Random Rotation cycle. A Continue build resolves that
ID back to its show/artist group before reconstructing the seeded cycle, so
persisted build seams produce the same next item as an uninterrupted build.
Older contexts default this additive map to empty.

`PlaybackOrder::Random` now follows ErsatzTV's draw-with-replacement behavior
instead of sharing Shuffle's exhaustive permutation. ErsatzRS retains its
existing deterministic LCG rather than reproducing .NET `Random` byte-for-byte:
the collection state persists the seed and draw count, runtime draws advance
in O(1), and anchor restoration jumps to the required draw in O(log n).

### 2026-08-02 addendum: database-owned runtime-state identities

Classic runtime-state identities are global database primary keys, not
playout-local sequence numbers. ErsatzTV leaves new owned EF entities at their
default identity and lets the database assign the key. ErsatzRS previously
derived a new key from the maximum state ID visible to one playout and then
persisted it with `REPLACE`. Two playouts could therefore choose the same key;
the later write deleted the first playout's enumerator state and caused future
Continue builds to restart shuffled collections, producing consecutive
duplicate movies.

New Classic anchors and fill-group indices now use identity zero as the
in-process "not persisted" marker. The repository omits the key on insert,
captures the SQLite/MySQL generated identity, and uses that identity for the
owned enumerator row. Existing positive identities are retained with strict
`INSERT` semantics; a collision fails and rolls back instead of replacing
another playout. Multiple schedule items that share one content source also
persist exactly one collection enumerator, matching ErsatzTV's source-keyed
state.

Upgraded installations audit Classic runtime state before the normal scheduler
cadence. Only a playout with generated items and incomplete, duplicated, or
orphaned enumerator state is rebuilt with Refresh. Every later Continue build
repeats the invariant check as defense in depth, and the health engine reports
any inconsistent playout that remains. No schema migration is required because
both existing schemas already define these identities as auto-incrementing
global primary keys.

## Alternatives Considered

- Drop Classic scheduling and require migration to YAML or Block: rejected
  because it breaks imported ErsatzTV database parity.
- Translate Classic schedules into another engine at import time: rejected
  because it would add lossy conversion rules and make stateful resume behavior
  harder to compare with ErsatzTV.
- Keep Classic only as an admin UI compatibility shell: rejected because
  non-web maintenance must build valid playout rows without web intervention.

## References

- [Project contracts and status](../../plans/INDEX.md)
- [Current phase status](../../STATUS.md)
- [Parity matrix](../reference/parity-matrix.md)
- `$ERSATZTV_SRC/ErsatzTV.Core/Scheduling/`
- `$ERSATZTV_SRC/ErsatzTV.Core/Scheduling/PlayoutBuilder.cs`
