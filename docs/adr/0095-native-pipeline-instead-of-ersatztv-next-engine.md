# ADR 0095: Native pipeline instead of the ErsatzTV Next streaming engine

## Status

Proposed

## Implementation

Not implemented

## Context

The upstream catch-up backlog introduces ErsatzTV Next, a separately
packaged `ersatztv-channel` process fed channel configuration and playout JSON.
ErsatzRS already owns scheduling, FFmpeg command generation and process
supervision. Integrating a second streaming engine would duplicate those
responsibilities and complicate deployment and cross-platform acceptance.
Subtitle, alternate-audio, graphics and playback improvements inside Next
commits still require individual behavioral review.

## Decision

Proposed for maintainer acceptance: retain the native ErsatzRS pipeline.
Do not add a per-channel `StreamingEngine` column, spawn or bundle
`ersatztv-channel`, or export Next playout/channel-config JSON. Next-only
version displays and troubleshooting UI are outside the native product.

Extract useful behavior from each upstream commit and implement missing
behavior in `ersatzrs-ffmpeg`, the scheduler or the application layer under
existing crate boundaries. Use Covered only with code/test evidence. Once
this ADR is accepted, pure Next plumbing can be N/A under this decision;
missing playback behavior cannot be excluded merely because Next introduced it.
Native adaptations use Adapt (ADR 0095), with owning-feature debt and evidence.

Preserve direct FFmpeg invocation (ADR 0003), in-process graphics through
FFmpeg drawvg/drawtext (ADRs 0005 and 0077), FFmpeg 9 (ADR 0063), SQLite
(ADR 0080), and the application boundary (ADR 0051).

## Consequences

Deployment retains one native streaming implementation without another engine
binary or channel configuration protocol. ErsatzRS must implement and test
applicable behavior itself, especially subtitles, alternate audio, watermarks,
media-server inputs and on-demand continuity. This proposal is not a blanket
parity exemption and does not prove any behavior implemented.

Acceptance of this ADR and per-row reconciliation remain open. Behavioral gaps
belong in their owning parity records; the catch-up record tracks this decision
and links the ledger without collecting unrelated feature debt.

## Alternatives Considered

- Bundle Next alongside the native engine: duplicates lifecycle, packaging,
  configuration and acceptance responsibilities.
- Exclude every Next commit: loses useful user-visible behavior and correctness
  fixes that can apply to the native pipeline.
- Replace the native pipeline wholesale: discards the accepted FFmpeg and
  graphics architecture without evidence that replacement is necessary.

## References

- [Catch-up ledger](../../plans/upstream-catch-up.md)
- [Catch-up progress](../progress/parity/upstream-baseline-catch-up.toml)
- [Architecture](../../ARCHITECTURE.md)
- [ADR 0003](0003-ffmpeg-cli-spawn.md)
- [ADR 0005](0005-drawvg-graphics-engine.md)
