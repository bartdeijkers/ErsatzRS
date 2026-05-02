# ADR 0010: Classic scheduling engine port

## Status

Accepted

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

## Alternatives Considered

- Drop Classic scheduling and require migration to YAML or Block: rejected
  because it breaks imported ErsatzTV database parity.
- Translate Classic schedules into another engine at import time: rejected
  because it would add lossy conversion rules and make stateful resume behavior
  harder to compare with ErsatzTV.
- Keep Classic only as an admin UI compatibility shell: rejected because
  non-web maintenance must build valid playout rows without web intervention.

## References

- [Master plan](../../plans/sequential-dancing-valiant.md)
- [Non-web parity gaps](../../plans/non-web-parity-gaps-2026-04-26.md)
- [Parity matrix](../reference/parity-matrix.md)
- `/home/bart/ErsatzTV/ErsatzTV.Core/Scheduling/`
- `/home/bart/ErsatzTV/ErsatzTV.Core/Scheduling/PlayoutBuilder.cs`
