# ADR 0115: Play compact JPEG XL graphics

## Status

Accepted (2026-10-02)

## Implementation

Implemented (2026-10-05) — native regression, typed-editor preview, actual
served-stream alpha/phase/looping and complete native-graph throughput pass.
Milestone `v111-findings-09` owns the unchanged acceptance and canonical evidence,
including native closeout, transport/parity and final matched-bundle gates.

## Context

The channel findings demonstrate that FFmpeg 9 decodes transparent animated
JPEG XL, but its demuxer rejects image2 looping and input seeking. Unqualified
stream looping lacks duration and stalls. Existing Motion also loses programme
phase in regular playback. Replacing compact JPEG XL with GIF or an external
renderer does not satisfy the accepted solution.

ErsatzTV's native Motion renderer separates source phase from programme timing
and seeks looped content after input. ErsatzRS preserves that behavior within
its existing shared FFmpeg graph, without reintroducing the external renderer.

## Decision

Probe actual decoded frame timestamps through the existing bounded FFprobe
process owner. Derive source duration from the last timestamp plus its frame
duration; reject empty, nonmonotonic or invalid timing. Bound decoding to fifteen
seconds and the owner's existing eight-MiB output limit. Use four bounded decoder
threads to avoid serial full-frame probing dominating startup. Await cancellation and
child cleanup before releasing startup ownership.

For a compact animation that repeats, write an owned finite ffconcat manifest
containing references to the original JPEG XL file with explicit durations.
Reopen the compressed file for each cycle: timestamps advance while decoded
frames need not remain buffered for the entire animation. Bound manifests to
100,000 entries and eight MiB; fail explicitly when a requested graph exceeds
those limits. Construct enough cycles for the finite output interval. Normalize
repeat phase modulo the decoded duration, then seek with decoded `trim` and
reset output timestamps at microsecond precision. Retain the in-progress decoded
frame at fractional joins, including 24 fps; millisecond rounding can discard it.
Never use unsupported JPEG XL input seek or loop
options. A still JPEG XL retains one decoded frame.

Numbered JPEG XL image2 sequences retain their existing native input semantics.
For finite previews and canvas chunks, bound manifest repetition by the actual
requested output interval while retaining full programme metadata.

Use the same typed source timing for Image, Motion, regular channel playback,
canvas chunks and real editor previews. Motion start, hold and disappearance
remain programme-relative even when joining an existing programme.

Preserve decoded alpha. Constant opacity uses colorchannelmixer; provably Boolean
opacity gates the compositor once per output frame. Continuous expressions
multiply the existing alpha rather than replacing it. VAAPI composition without
timeline enable support uses the existing software-composition/upload path for
timed or compact alpha animations. Other native rendering and
hardware/software fallback contracts remain in effect. Runtime health requires
both JPEG XL decoders and the corresponding still/animated demuxers.

## Consequences

Production media remains JPEG XL. Additional owned manifests contain references,
not transcoded replacement media. Repeat decoding and the complete graph's
throughput require measured acceptance, including actual served-stream phase
and transparency; an offline decoder benchmark is insufficient.

Motion exact-pixel authoring and independent typography acceptance remain the
separate authorized `v111-findings-09b` milestone.

## Alternatives Considered

- Decode and buffer every RGBA frame in the loop filter: memory grows with the
  complete animation and duplicates across carriers.
- Use GIF or preconverted broadcast media: changes the requested delivery format.
- Trust demuxer looping or input seeking: contradicted by bounded native tests.

## References

- [ADR 0005](0005-drawvg-graphics-engine.md)
- [ADR 0114](0114-author-graphics-with-the-native-renderer.md)
- [Graphics parity](../progress/parity/drawvg-graphics-engine.toml)
- `$ERSATZTV_SRC/ErsatzTV.Infrastructure/Streaming/Graphics/Motion/MotionElement.cs`
