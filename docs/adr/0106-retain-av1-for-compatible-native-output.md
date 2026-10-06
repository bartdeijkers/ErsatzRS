# ADR 0106: Retain AV1 for compatible native output

## Status

Accepted 2026-10-01

## Implementation

Implemented (UCU-09 row 183, 2026-10-01) — native AV1 selects fMP4 HLS;
unsupported MPEG-TS and Next output is rejected without rewriting profiles.
Decoded Linux application playback, Windows runtime encoding, localized
browser guidance and the required regression gates pass. Exact results are
owned by [UCU-09](../progress/milestones.toml).

## Context

Upstream commit `960421e8` removes AV1 from profile choices, rejects it in
validation, coerces create/update requests to HEVC and migrates every persisted
AV1 profile to HEVC. Its stated reason is that ErsatzTV no longer produces the
fMP4 segments required for AV1 HLS playback.

ErsatzRS still exposes native `segmenter-fmp4` HLS, emits initialization maps
and retains fMP4 segments across playlist trimming. It also has native AV1
encoder selection. These existing code paths do not by themselves prove working
AV1 application playback. The optional Next adapter explicitly rejects AV1.
Automatically rewriting stored profiles would discard settings that may remain
usable through the native engine.

## Decision

Retain AV1 profile settings and native AV1 output for compatible
containers, without the upstream blanket AV1-to-HEVC migration. Native
transcoded HLS selects fMP4 for AV1 profiles. Preserve
explicit Next rejection. Reject unsupported codec/transport combinations before
starting a stream, with actionable validation or health feedback; do not silently
change the requested codec. Verify native fMP4 HLS through the application with
the exact published FFmpeg runtime before accepting the upstream row.

Software AV1 uses SVT target-bitrate VBR without the generic maximum-bitrate
option: FFmpeg selects CBR when target and maximum are equal, and the published
SVT runtime rejects that mode for random-access output. Hardware encoder rate
control is unchanged.

This decision does not add a new streaming engine, expand hardware acceptance,
or authorize another FFmpeg build or publication. Existing source AV1 decoding
and selection of subtitles and alternate audio must remain intact.

## Consequences

Existing profile intent is preserved. ErsatzRS owns the compatibility checks
and decoded application proof that justify this upstream deviation. That proof permits accepting row 183 and advancing the baseline. Hardware and ARM/macOS debt retain their existing scope.

## Alternatives Considered

- Port upstream removal and blanket migration: simplest parity behavior, but
  discards settings despite the retained native fMP4 implementation.
- Retain AV1 without transport checks or playback proof: insufficient evidence
  for a dependable viewing experience.

## References

- [Upstream change](https://github.com/ErsatzTV/legacy/commit/960421e8a3cb832864fe096d5f0cfa833dbb1837)
- [Catch-up source map and ledger](../../plans/upstream-catch-up.md)
- [Optional Next engine](0095-native-pipeline-instead-of-ersatztv-next-engine.md)
- [Native HLS routing](../../crates/ersatzrs-web/src/transport/iptv/hls.rs)
- [FFmpeg 9 SVT adapter](https://github.com/FFmpeg/FFmpeg/blob/n9.0/libavcodec/libsvtav1.c)
- [Native HLS worker](../../crates/ersatzrs-infra/src/hls_worker.rs)
