# Add an FFmpeg Pipeline

Use this guide when adding a new software or hardware FFmpeg path.

## Steps

1. Read the matching ErsatzTV `*PipelineBuilder.cs` source.
2. Identify the ErsatzRS target module in `ersatzrs-ffmpeg`.
3. Preserve the command-generation structure unless an ADR records a deviation.
4. Add unit tests for builder invariants.
5. Add `insta` snapshots for generated argv and filter graphs.
6. Add an end-to-end manual verification note if the path requires hardware.
7. Update the parity matrix.

## Required Snapshot Coverage

Every pipeline must include snapshots for:

- A minimal single video input.
- HLS output.
- Watermark or graphics overlay when supported.
- Subtitle handling when supported.

## Hardware Paths

Hardware-specific paths such as NVENC, VAAPI, QSV, AMF, VideoToolbox, RKMPP, and
V4L2M2M must document the required FFmpeg build capabilities.
