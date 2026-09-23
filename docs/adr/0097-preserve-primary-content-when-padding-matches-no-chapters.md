# ADR 0097: Preserve primary content when padding matches no chapters

## Status

Accepted 2026-09-23

## Implementation

Partial — the existing Classic unsplit-content path and
`chapter_expression_selects_breaks_and_preserves_unsplit_content_without_matches`
regression preserve the programme when a mid-roll padding expression matches no
chapter points. The maintainer accepted this deviation on 2026-09-23 after the
newer-upstream history audit. UCU-02 row 22 remains pending its other expression,
placement and playback acceptance evidence.

## Context

UCU-02 row 22 reviews ErsatzTV commit
`7e38297d6ed240f784d388696e687f781e550a71` (`#2867`). Inspection of
`ErsatzTV.Core/Scheduling/PlayoutModeSchedulerBase.cs` at that commit identifies a
primary-content omission when all of these conditions hold:

- The programme has more than one effective chapter.
- Its mid-roll filler uses `FillerMode.Pad`.
- The filler expression matches no chapter points.

The source follows this sequence:

1. `AddFiller` only appends the unsplit primary item when there are at most one
   effective chapters (lines 453–456). The following mid-roll loop excludes Pad
   presets, so it also does not append this programme (lines 460–462).
2. `FillerExpression.FilterChapters` returns an empty list when no points match.
3. The padding stage changes the effective placement to PostRoll when the
   filtered list has at most one entry and the original list has more than one
   (lines 597–604).
4. Because no primary row exists, the padding calculation adds the durations of
   the filtered chapters, which sum to zero (lines 606–610). The PostRoll branch
   appends padding and fallback without restoring the primary item
   (lines 736–766).

This conclusion is derived from the pinned source; it is not a claim of an
executed upstream runtime reproduction. Non-pad mid-roll has a distinct explicit
fallback that appends the original programme when no points match
(lines 467–471).

For a concrete example, use a 45-minute programme starting at 06:00, multiple
chapters, expression `false`, `PadToNearestMinute = 60`, no other pre/post-roll
items, and sufficient filler or fallback material. In the upstream code the
empty result has duration zero. The initial 06:00 target is advanced by 60
minutes because it is not later than the current finish (lines 613–634). The
result therefore contains padding from 06:00 to 07:00 and omits the programme.

A fresh upstream fetch on 2026-09-23 confirmed `origin/main` at
`ee038ba460a237e277c2ed2fbb413bfac13795f8`. The complete `AddFiller` method is
byte-identical between row 22 and that tip. The only subsequent changes to its
file are `a7ecc5f9` (flexible midnight handling) and `b0f313ee` (nonpositive-duration
termination in tail/gap/fallback loops). `FillerExpression.cs` and its tests have
no changes in this range. No later upstream fix was found; adopting such a fix
would avoid this deliberate deviation if one becomes available. The maintainer
requested this history check before considering the ADR, then explicitly accepted
ADR 0097 on 2026-09-23.

## Decision

Preserve the complete primary programme when a valid mid-roll Pad
expression matches no chapter points. Treat that outcome as no internal breaks:
append the unsplit primary item, retain its source interval and playback
settings, and apply the configured padding after it using the existing clock
rounding calculation.

For the example above, the result is the programme from 06:00 to 06:45,
followed by 15 minutes of padding to 07:00. Padding uses the existing preset and
fallback rules; this decision does not guarantee a filled interval when no
usable filler source exists.

This decision does not change expression evaluation, successful chapter-point
selection, non-pad mid-roll behavior, malformed-expression errors, or clock
rounding. It does not authorize ignoring pre/post-roll padding expressions or
changing their upstream placement rules. Preserving the primary item must also
preserve alternate-audio, subtitle, exact-version and source-seek settings, and
count the programme as one scheduling selection.

## Consequences

A valid break-selection expression cannot silently remove the scheduled
programme merely because it selects no breaks. The result intentionally differs
from the pinned upstream omission and remains visible as a parity deviation.

Acceptance requires a focused regression proving the complete primary source
interval precedes post-roll padding, with the expected clock finish and no
internal filler breaks. Relevant persistence and playback acceptance remain
owned by UCU-02; this ADR does not replace those gates or claim the milestone is
complete. The associated implementation debt remains open until the required row acceptance
evidence is complete.

## Alternatives Considered

- **Strictly reproduce the omission.** Drop the primary programme and pad from
  the start using zero primary duration. This matches the inspected upstream
  branch but makes a valid no-match expression remove intended television
  content. It is not the chosen behavior.
- **Preserve the programme and omit padding.** Avoids content loss but abandons
  the configured clock-boundary padding requirement. It is not the chosen
  behavior.
- **Reject every valid no-match expression.** Prevents omission but fails a
  schedule whose expression legitimately chooses no internal breaks. Existing
  non-pad behavior already preserves the primary programme in that case.

## References

- [ADR 0001: Record architecture decisions](0001-record-architecture-decisions.md)
- [Upstream catch-up plan](../../plans/upstream-catch-up.md)
- [Classic scheduling parity](../progress/parity/classic-scheduling.toml)
- [Pinned upstream AddFiller implementation](https://github.com/ErsatzTV/legacy/blob/7e38297d6ed240f784d388696e687f781e550a71/ErsatzTV.Core/Scheduling/PlayoutModeSchedulerBase.cs#L453-L766)
- [Pinned upstream FillerExpression implementation](https://github.com/ErsatzTV/legacy/blob/7e38297d6ed240f784d388696e687f781e550a71/ErsatzTV.Core/Scheduling/FillerExpression.cs)
- [Classic block construction](../../crates/ersatzrs-core/src/scheduling/classic/mod.rs)
- [Classic filler regressions](../../crates/ersatzrs-core/src/scheduling/classic/fallback_tests.rs)
