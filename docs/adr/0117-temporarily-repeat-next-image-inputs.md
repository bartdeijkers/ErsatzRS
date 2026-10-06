# ADR 0117: Temporarily repeat Next image inputs

## Status

Proposed

## Implementation

Not implemented

## Context

UCU-09 row 186 supplies image metadata duration and correct container hints to
Next. Published `next-e88b2d7-ersatzrs.1` renders PNG, JPEG, BMP and TIFF for the
scheduled duration. Single-frame WebP and GIF instead reach EOF and Next serves
its "playout item ended early" error card. FFmpeg decodes the source files;
the failure is in Next's input lifetime, not the ErsatzRS hint or FFmpeg build.
The 2026-10-06 upstream refresh still points to e88b2d7.

Next's still-image loop recognizes PNG, JPEG, BMP and TIFF. WebP and GIF may
contain animation, so adding them to the single-frame loop would risk freezing
animation. Existing graphics inputs already have bounded repeat behavior, but
ordinary programme inputs do not expose that behavior through the wire schema.
AVI correctly follows Next's documented ContainerWithoutPts transcode fallback;
its decoded picture and selected audio pass and need no runtime correction.

ADR 0100 requires official artifacts. ADR 0116 makes one narrow exception for
live RTSP buffering. This proposal requests a separate, explicit exception;
RTSP approval alone does not authorize this change.

## Decision

Permit a minimal, temporary Next source correction for finite WebP/GIF image
programme inputs identified by their codec and image container. Repeat the
input as needed through the existing bounded programme interval, preserving
all animation frames and frame timing. Preserve seeks, cancellation, separate
silence/audio inputs and the existing duration/EOF contract for ordinary video,
live sources and RTSP. Reuse Next's input-option machinery; do not add a command
wrapper, disguise the container, freeze animated input to its first frame, or
replace the native renderer.

The exact patch remains subject to source review and executable evidence before
adoption. Extend the reproducible Next source manifest and publish a separately
identified snapshot for all six supported platforms only after the required
gates pass. Reuse the existing verified ErsatzRS-ffmpeg packages; this proposal
authorizes no FFmpeg rebuild or FFmpeg release.

Require source-level argv regressions and decoded single-frame and animated
WebP/GIF acceptance across duration boundaries, seeks and cleanup. Preserve
PNG/JPEG/BMP/TIFF, AVI transcode fallback, audio/subtitles, RTSP startup and
EOF/handoff acceptance. Available Linux/Windows software checks remain required;
only the already accepted hardware/ARM/macOS application debt remains unverified.

At every upstream Next update, test the unpatched candidate first. Remove this
patch as soon as equivalent upstream behavior passes these regressions, update
source/package identity, and retain the tests. Retirement is independent of the
RTSP patch: remove either correction as soon as its own upstream fix qualifies.

## Consequences

A second small patch and replacement Next snapshot become maintenance work.
Animation timing, input seeking and bounded cancellation must be proven; a
successful static frame alone is insufficient. Row 186 and ordered catch-up
remain open until the decision and runtime acceptance are complete. No new
package build or publication has started under this proposal.

## Alternatives Considered

- Wait for upstream: avoids another temporary patch but keeps row 186 open.
- Accept an error card or omit the format: fails the user-visible image outcome.
- Classify animated-capable formats as single-frame stills: can freeze animation.
- Convert every such image through a new application producer: expands runtime
  ownership for a gap in the already used Next input machinery.

## References

- [ADR 0100](0100-next-alpha-embedded-text-subtitle-limits.md)
- [ADR 0116](0116-temporarily-bound-next-live-rtsp-buffering.md)
- [Catch-up source map](../../plans/upstream-catch-up.md)
- [Owning UCU-09 milestone](../progress/milestones.toml)
