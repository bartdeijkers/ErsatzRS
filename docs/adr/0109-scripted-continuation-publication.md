# ADR 0109: Scripted continuation and timeline publication

## Status

Accepted (2026-10-02) — implements the maintainer-approved finding 10 solution.

## Implementation

Implemented (v111-findings-10, 2026-10-02). The milestone records scheduler,
SQLite restart/publication, Health, browser and playback acceptance evidence.

## Context

An automatic Continue receives the current request clock even when the published
timeline extends days into the future. Scripted scheduling initialized both its
Lua cursor and history cutoff from that clock, appending overlapping programmes
and replaying series positions. Existing restart tests supplied the correct tail
as the request clock and therefore did not reproduce the production failure.

ErsatzTV's `SchedulingEngine.RestoreOrReset` restores `anchor.NextStart` and its
script launcher exposes the build mode as an argument. ErsatzRS retains its
in-process Lua API and user-supplied arguments.

## Decision

- Continue derives its cursor from persisted timeline state, falling back to the
  anchor when no rows remain. Future published rows are authoritative; conflicting
  anchor state must fail observably. Expired timelines resume at the
  request clock. History restoration uses this effective cursor; cleanup retains
  its existing request-clock and mirror-offset semantics.
- Lua exposes `build_mode()` as `continue`, `refresh`, or `reset`, and
  `start_time()` as the immutable effective cursor. `current_time()` remains the
  mutable cursor; `finish_time()` remains the requested horizon. Existing `arg`
  values remain compatible.
- Validate positive item intervals, emitted ordering and non-overlap before a
  Scripted result is returned. Validate again against the retained published
  timeline inside the SQLite write transaction, before deleting or inserting
  rows. A failure leaves items, history, anchors and attachments unchanged.
  Intentional forward waits remain valid gaps; backward emission is rejected.
- Reuse persisted build failures for Activity and playout detail feedback, and
  add a matching Health check. A rejected candidate never replaces the published
  timeline or becomes a successful build.
- Continue remains append-only. Continuity-safe explicit Refresh, source
  dependency invalidation and generation fencing remain owned by finding 06.

## Consequences

Automatic continuation no longer needs channel-specific database queries in Lua.
Validation may reject scripts that previously appeared successful despite an
invalid timeline. Operators must correct the script; the previous broadcast
timeline remains intact. No database migration or external renderer is needed.

## Alternatives Considered

- Supply the tail from each caller: rejected because it leaves automatic and
  future callers vulnerable to the same defect.
- Sort emitted rows before validating: rejected because it hides backward script
  execution and can disconnect history from programme order.
- Repair overlaps by deleting published rows: rejected because Continue must
  preserve programmes and history already published.

## References

- [Rebuild semantics](0020-playout-rebuild-on-impacting-changes.md)
- [Smart Shuffle state](0019-similar-video-cooldown.md)
- [Scripted parity](../progress/parity/scripted-scheduling.toml)
- `$ERSATZTV_SRC/ErsatzTV.Core/Scheduling/Engine/SchedulingEngine.cs`
- `$ERSATZTV_SRC/ErsatzTV.Core/Scheduling/ScriptedScheduling/ScriptedPlayoutBuilder.cs`
