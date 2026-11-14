# ADR 0133: Qualify native bitmap caption clock rescaling

## Status

Proposed

## Implementation

Implemented (PR #155, 2026-11-13). Twelve focused bitmap-carriage regressions and
the unchanged real-source native regression pass. Whole recovery acceptance
remains in `teletext-admin-recovery-flow` under
[teletext parity](../progress/parity/teletext-support.toml).

## Context

ADR 0121 requires source-bound bitmap captions, unchanged source bytes, actual
clock qualification and owned native-process cleanup. Complete-source decoding
now reaches every packet of the qualified 37 MB source. Its next refusal is the
old requirement that decoded microsecond endpoints minus the caption origin be
exact multiples of one millisecond. That restriction does not model FFmpeg's
native rescaling into Matroska's millisecond clock.

The actual n9.0 debug trace shows the first source timestamp at 189056/90000 s,
a native input offset of -1500678 us (rescaled to -135061 ticks), a shifted
subtitle timestamp of 53995/90000 s (599944 us), and a Matroska packet at 600 ms.
The later source timestamp 2397674/90000 s similarly becomes 25140144 us and
25140 ms. Both source bitmaps and their explicit clear packets remain present.
No source remux, timestamp option or fixture change is needed to explain this.

## Decision

For this existing FFmpeg9 MPEG-TS bitmap-carriage path, compare exact coordinates
produced by the audited native integer operations, in their actual order:

1. Require the selected source subtitle clock to be exactly 1/90000. Require
   each reported source bitmap microsecond timestamp and the established native
   caption origin to round-trip through one unique 90 kHz tick. Missing values,
   sentinel timestamps and unrepresentable observations remain unavailable.
2. Rescale the established input offset from microseconds into source ticks and
   apply it in that domain, as the native demuxer does. Reject overflow or a
   negative normalized endpoint.
3. Rescale the shifted source tick into subtitle-decoder microseconds, then
   into the actual Matroska millisecond clock, using nearest rounding with ties
   away from zero (`AV_ROUND_NEAR_INF`). Compare that exact result to the observed
   artifact endpoint; do not accept a plus/minus tolerance interval.

The existing source/PSI/PES/hash checks, native origin and input-policy checks,
bitmap count, complete clear tail, packet/frame pairing, bounded work and actual
cleanup remain mandatory. This mapping changes neither generated FFmpeg argv
nor the source's timestamp policy. It does not grant a different clock, seek,
consumer, live-source or platform capability.

## Consequences

Valid native submillisecond source endpoints can be qualified without pretending
that the artifact retains finer precision than its real millisecond clock.
Integer arithmetic reproduces native quantization instead of guessing a rounding
window. Unsupported or ambiguous observations still fail closed.

Focused arithmetic/parser negatives and the unchanged real-source native
carriage regression must pass. Joined cold/late-cue/clear/programme-boundary
pixels, transport, audio, guide refresh and isolation remain mandatory part6
acceptance; component completion cannot substitute for those proofs.

## Alternatives Considered

- Require exact millisecond divisibility: preserves an artificial source-format
  restriction and rejects the observed valid native output.
- Accept nearby artifact timestamps: cannot identify the actual native result.
- Override teletext timestamp repair, retimestamp the source or replace sparse
  captions: changes the input/policy instead of qualifying the requested one.

## References

- [ADR 0121](0121-teletext-pages-transport-and-addon-contract.md).
- [Native bitmap parser](../../crates/ersatzrs-infra/src/ffprobe/teletext_decode_bitmap_carriage/parser.rs).
- [FFmpeg n9.0 input shift](https://github.com/FFmpeg/FFmpeg/blob/n9.0/fftools/ffmpeg_demux.c#L385).
- [FFmpeg n9.0 decoder timestamp conversion](https://github.com/FFmpeg/FFmpeg/blob/n9.0/libavcodec/decode.c#L891).
- [FFmpeg n9.0 subtitle packets](https://github.com/FFmpeg/FFmpeg/blob/n9.0/fftools/ffmpeg_enc.c#L343).
- [FFmpeg n9.0 mux rescaling](https://github.com/FFmpeg/FFmpeg/blob/n9.0/fftools/ffmpeg_mux.c#L128).
- [FFmpeg n9.0 integer rescaling](https://github.com/FFmpeg/FFmpeg/blob/n9.0/libavutil/mathematics.c).
