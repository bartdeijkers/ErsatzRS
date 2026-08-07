# ADR 0064: HDR and Dolby Vision input and output contract

## Status

Accepted (2026-08-07).

## Implementation

Partial — the SDR/HDR10 output contract and Dolby Vision input policy are
implemented on the software and NVENC paths (2026-08-07, Phase 8); the other
hardware backends are not.

Implemented: `FFmpegProfile` persists a validated, localized `OutputDynamicRange`
with `SdrBt709` preserving the previous behavior exactly; HDR10 output works on
the software and NVENC paths for PQ, HLG, and SDR sources, with stable signalling
across programme transitions proven from decoded frames; source colour, rotation,
mastering-display, MaxCLL/MaxFALL, and the Dolby Vision configuration record are
persisted by the scanner; HDR10 static metadata is emitted at the FFmpeg 9
per-stream input boundary, preferring source values and falling back to
documented defaults; backends with no characterized HDR10 graph demote to
software rather than emitting SDR; and unproven Dolby Vision profiles are
rejected with health evidence instead of played with wrong colour. Dolby Vision
output is not advertised.

Dolby Vision profile 8.1 is characterized against a real file: its HDR10
base layer plays to both SDR and HDR10 targets with the Dolby Vision
configuration record correctly absent from the output. That work also uncovered
and fixed a defect in the pre-existing SDR path, where mastering-display and
content-light metadata survived the tone-map and was written onto BT.709 frames
— the exact "never copy source HDR metadata onto transformed SDR frames" rule
this ADR states.

Not implemented, tracked as parity debt in
`docs/progress/parity/hdr-dolby-vision-metadata.toml` and
`docs/progress/parity/ffmpeg-9-performance-and-support.toml`: HDR10 is
uncharacterized on QSV/VAAPI/AMF/VideoToolbox/RKMPP/V4L2M2M; the seeded HDR10
metadata defaults are constants rather than operator-editable profile fields;
Dolby Vision profiles 5 and 7 remain rejected because no fixture of either was
available; and Dolby Vision overlay, subtitle, and seek behavior was not
re-exercised with Dolby Vision input specifically.

## Context

ADR 0017 defines two fresh-database profiles as HEVC Main 10 HDR10 outputs, but
the current pipeline has no persisted output dynamic-range field. Bit depth and
codec profile are insufficient to distinguish 10-bit SDR from HDR10. Whenever
an HDR source is transcoded, the current filter builder tone-maps it to BT.709,
including when one of the profiles named `hdr` is selected. Adding HDR10 static
metadata to that output would label SDR pictures as HDR and would be incorrect.

FFmpeg 9 adds per-stream `-mastering_display` and `-content_light` input-side
metadata options and the `dovi_split` bitstream filter for multilayer Dolby
Vision HEVC. The existing libplacebo path can also consume embedded Dolby
Vision metadata while processing Vulkan frames. These capabilities make a
better-than-ErsatzTV implementation possible, but they do not define product
behavior by themselves.

ErsatzTV has no equivalent Dolby Vision policy and does not emit HDR10 static
metadata. This decision is therefore an intentional beyond-parity extension.
It must keep linear output signaling stable, must not invent Dolby Vision
support from the presence of one bitstream filter, and must fail visibly rather
than produce plausible but incorrectly coloured video.

## Decision

### Output dynamic range is a profile contract

Add a persisted `OutputDynamicRange` enum to `FFmpegProfile` and expose it as a
localized **Output Dynamic Range** select in the FFmpeg profile editor. The
initial values are:

- `SdrBt709`: produce BT.709 SDR. HDR10, HLG, and supported Dolby Vision inputs
  are tone-mapped to SDR and all HDR/Dolby Vision signaling is removed.
- `Hdr10Bt2020Pq`: produce a stable BT.2020/PQ HDR10 output. HDR input is
  preserved or normalized, while SDR input is mapped into the HDR10 signal
  without inventing highlight detail. The implementation must select and
  document a fixed SDR reference-white policy and prove programme transitions
  do not change the output signaling contract.

The enum applies only when video is transcoded.
`FFmpegProfileVideoFormat::Copy` preserves the source elementary stream and
ignores the selected output dynamic range; the editor must communicate that
distinction. Bit depth stays a separate field. Selecting HDR10 requires a
compatible codec, 10-bit pixel format, container, and output path, and invalid
combinations are rejected during profile validation rather than silently
downgraded.

The tonemap-algorithm field is meaningful for conversions that compress a
higher dynamic range. It does not select the output color space and must not be
used as a proxy for this enum.

### Keep one stable output contract per stream

An FFmpeg profile describes the output stream, not merely a preference for
individual source items. An HDR10 profile therefore remains HDR10 when an SDR
item follows an HDR item. A future source-preserving mode that switches
signaling between items requires separate client, HLS, MPEG-TS, and
discontinuity evidence and is not part of this decision.

### Metadata follows the picture transformation

The scanner persists source color primaries, transfer characteristics, matrix,
range, mastering-display metadata, MaxCLL/MaxFALL, and the Dolby Vision
configuration record when ffprobe reports them. Pipeline code carries this
typed metadata with the selected video stream.

For HDR10 output:

- Preserve valid source mastering and content-light metadata when the picture
  transformation preserves their meaning.
- Use explicit, validated profile metadata when a transform changes their
  meaning or the source lacks them. Seeded defaults must be documented and
  editable; hidden guessed values are forbidden.
- Emit FFmpeg 9 `-mastering_display` and `-content_light` at their per-stream
  input-option boundary, or use an encoder/filter-specific equivalent when
  required. They are not generic output options.

For SDR output, strip HDR10 and Dolby Vision metadata after tonemapping. A
pipeline must never copy source HDR metadata onto transformed SDR frames.

### Dolby Vision is an input policy first

Dolby Vision support is introduced from characterized fixtures, not inferred
from codec names:

- Profile 7 and compatible profile 8.1 sources may use their HDR10-compatible
  base layer for HDR10 output. Enhancement-layer and RPU handling must be
  explicit; HDR10 output must not retain Dolby Vision signaling that the muxed
  stream no longer satisfies.
- Profile 5 has no assumed HDR10 fallback. It may be converted to SDR or HDR10
  only after a fixture proves the selected libplacebo/FFmpeg path produces
  correct color. Until then it is rejected with troubleshooting and health
  evidence rather than played with wrong colors.
- `dovi_split` is treated as a profile-7 multilayer tool, not as a general
  Dolby Vision decoder or encoder.
- Dolby Vision output is not supported by this ADR. Passthrough may be enabled
  later only for a proven source/codec/container/output/client matrix; Dolby
  Vision transcoding or a general output mode requires a separate decision.

### Hardware processing remains capability driven

Software, QSV, NVENC, VAAPI, AMF, VideoToolbox, RKMPP, and V4L2M2M paths must
produce the same selected output contract or report that the combination is
unsupported. Hardware acceleration may change performance, not color
semantics.

The existing NVENC Vulkan/libplacebo bridge is retained, but selection requires
the operation-specific runtime capability and fallback rules in ADR 0014.
Whether Vulkan becomes a first-class cross-vendor encoder is outside this ADR
and requires its own decision after benchmarks and release-matrix proof.

### Verification is output based

Argv snapshots are necessary but insufficient. Acceptance requires generated
HDR10, HLG, SDR, and licensed or redistributable Dolby Vision profile 5/7/8.1
fixtures where available, plus ffprobe/frame-side-data assertions and decoded
image comparisons. Direct MPEG-TS and HLS paths must prove stable signaling,
programme transitions, stream copy, transcode, restart/fallback, overlays, and
scaling. Unsupported combinations must appear in health and troubleshooting.

## Consequences

HDR versus SDR becomes an explicit operator choice instead of being inferred
from a profile name, codec, or bit depth. Fresh HDR seeds can finally implement
ADR 0017 truthfully, while existing profiles can retain the current effective
SDR behavior through an additive migration default.

The scanner, domain model, SQLite/MySQL schema, application validation, admin
UI and every pipeline backend must change together. The implementation needs
localized labels for every supported culture and output-based runtime tests;
metadata-only snapshots cannot prove correct color.

Stable HDR10 output for mixed SDR/HDR schedules requires an SDR-in-HDR mapping
policy and consumes more processing than passing source signaling through.
Dolby Vision profile 5 may remain visibly unsupported until suitable evidence
exists. Those costs are preferable to silent wrong-color playback.

## Alternatives Considered

- Infer HDR from HEVC Main 10 or a 10-bit profile: rejected because 10-bit SDR
  is valid and the current code demonstrates that bit depth does not define
  output colorimetry.
- Add only `mastering_display` and `content_light`: rejected because the current
  transcode path produces BT.709 SDR; attaching HDR metadata would be false.
- Use an `HDR` checkbox: rejected because it cannot describe the fixed output
  contract, copy behavior, future modes, or invalid codec combinations.
- Preserve each source's dynamic range automatically: deferred because a
  linear stream could change signaling at every item boundary and client
  compatibility is unproven.
- Advertise Dolby Vision output immediately: rejected because FFmpeg 9's
  `dovi_split` is not a Dolby Vision encoder and container/client behavior is
  not characterized.
- Make Vulkan the universal processing and encoding backend: deferred to a
  separate ADR after cross-platform capability and quality benchmarks.

## References

- [ADR 0003: Invoke FFmpeg through the CLI](0003-ffmpeg-cli-spawn.md)
- [ADR 0014: Runtime-learned hardware-capability fallback](0014-runtime-learned-hardware-capability-fallback.md)
- [ADR 0017: Default FFmpeg profiles for multi-resolution streaming](0017-default-ffmpeg-profile-1080p60.md)
- [ADR 0063: FFmpeg 9 runtime and profile options](0063-ffmpeg-9-runtime-and-profile-options.md)
- [FFmpeg 9 HDR, Dolby Vision, and pipeline hardening plan](../../plans/ffmpeg-9-hdr-dolby-vision-and-pipeline-hardening.md)
- [HDR and Dolby Vision progress](../progress/parity/hdr-dolby-vision-metadata.toml)
- [FFmpeg 9 performance and support progress](../progress/parity/ffmpeg-9-performance-and-support.toml)
- [FFmpeg 9 bitstream-filter documentation](https://ffmpeg.org/ffmpeg-bitstream-filters.html)
- [FFmpeg libplacebo filter documentation](https://ffmpeg.org/ffmpeg-filters.html#libplacebo)
