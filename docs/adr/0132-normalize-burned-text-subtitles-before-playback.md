# ADR 0132: Normalize burned text subtitles before native playback

## Status

Accepted (2026-11-08)

## Implementation

Implemented (PR #129, 2026-11-08) — local decoding/playback proof and exact-head
CI are complete; the maintainer merged the change as `d275f1b9`. Milestone
`subtitle-encoding` owns the evidence and explicit source/PR boundary. Production
installation and release acceptance are excluded.

## Context

A reported native HLS incident stopped video because a legacy SubRip source
contained a non-UTF-8 accented byte. ADR 0061's Unicode reader fell back to
FFmpeg stream copy, which retained the invalid bytes. The later subtitle burn
decoder failed while initializing the video graph. A same-path seek-cache key
could also retain output after a source or encoding correction.

Source review found no character-set declaration or conversion in
`$ERSATZTV_SRC/ErsatzTV.FFmpeg/Pipeline/PipelineBuilderBase.cs::Seek` or
`$ERSATZTV_SRC/ErsatzTV.Application/Subtitles/Commands/ExtractEmbeddedSubtitlesHandlerBase.cs`.
This extends ADR 0061's intentional native cue-aware normalization boundary.

## Decision

Normalize ordinary native burned SRT, ASS/SSA and WebVTT before every playback
start, including position zero. HLS, MPEG-TS and troubleshooting share admission.
Keep publication-verified/retained subtitle decoding strict and separate; this
slice does not change Next's independently bound subtitle delivery or extraction.

Use `encoding_rs` for strict conversion and the assessed API in the compatible
`chardetng` 0.1 line for legacy detection. BOM declarations are authoritative;
conflicting explicit declarations fail. Otherwise an explicit source charset
precedes strict UTF-8, followed by assessed legacy detection. Fewer than four
non-ASCII bytes or an unassessed guess require an explicit declaration. Detection
is probabilistic, not proof of the author's intended charset. A local `.charset`
companion or HTTP `Content-Type` charset resolves uncertain/wrong guesses.
Reject malformed Unicode, decoder errors and binary control data; never
introduce a lossy replacement character. Preserve literal glyphs already
present in valid Unicode source text.

Hash the source bytes and canonical resolved charset with a normalization-version
prefix. Include format and seek in derived artifact names. Read the source before
using cached output, and verify cached text against expected output. Promote
complete files atomically and preserve source files. Rebased cues retain ADR
0061's straddling-cue clamp. FFmpeg is a syntax-only compatibility fallback over
normalized UTF-8; failed preparation is an explicit error, never subtitle-free
success or a return to the original undecodable source.

Preflight upcoming local sidecars through the same decoder in Health, without
network retrieval or mutation. Limit work and disclose unchecked sources.
Localize operator-facing health messages in every supported culture. Remote
sources remain checked at native playback admission and troubleshooting.
The operator reference owns declaration syntax and limits.

## Consequences

Legacy Western, Cyrillic and Japanese fixtures retain their text without making
Windows-1252 a global fallback. Cache entries cannot outlive source/charset
corrections. Sources must be read at each admission to establish byte identity;
bounded reads and deadlines constrain that cost. Short legacy subtitles may need
an explicit charset, and previously tolerated malformed text now fails visibly.
Health is preflight evidence for its stated local window, not proof for the whole
library or remote provider availability.

## Alternatives considered

- Convert only the incident file: does not repair recurrence in playback.
- Always assume Windows-1252: corrupts international subtitle text.
- Add only `-sub_charenc`: does not fix Rust cue rewriting or stale cache reuse.
- Ignore undecodable subtitles: silently changes the selected viewing experience.
- Add per-item database schema and admin forms: unnecessary for a source-level
  charset declaration; reuse companion files and HTTP metadata.

## References

- [ADR 0061](0061-cue-aware-subtitle-seek-rebase.md)
- [Subtitle text reference](../reference/subtitle-text.md)
- [Progress records](../progress/README.md)
