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

Reset and Refresh previously reproduced the identical shuffle order for a
playout's entire life: `Playout.seed` was regenerated only at playout
creation (never on a later Reset), and Refresh could never retain
per-collection or schedule-item shuffle state because the only anchors the
engine wrote were undated while retention required a dated checkpoint. This
was a bug in the port, not an intended deviation — ErsatzTV's `ResetPlayout()`
regenerates `playout.Seed` on every Reset, and its Refresh is designed to
continue the existing shuffle sequence. Both are now fixed; see the
`classic-scheduling` parity file for the full account and regression list.

`PlaybackOrder::RandomRotation` is a deliberate, spirit-preserving adaptation
rather than a literal port of ErsatzTV's live
`RandomizedRotatingMediaCollectionEnumerator`. Upstream tracks per-draw group
state and bumps to a different group only when a live draw repeats the
previous one; ErsatzRS's `MediaEnumerator` instead materializes a full
arranged order once per cycle (the same pattern `Shuffle`/`ShuffleInOrder`
already use), so `RandomRotation` bucket-and-walks a random remaining group at
materialization time, preferring a group other than the previous pick. The
observable property — no two adjacent items from the same show/artist when
avoidable — matches upstream even though the mechanism differs.

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
