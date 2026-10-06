# ADR 0116: Temporarily bound Next live RTSP buffering

## Status

Accepted 2026-10-06. The maintainer approved the scoped patch and rebuilt
packages, with mandatory removal when upstream provides an equivalent fix.

## Implementation

Partial — the temporary correction is accepted and integrated under UCU-09
row 184, with six verified packages published as `next-e88b2d7-ersatzrs.1`.
Linux/Windows application and transport acceptance pass; approved platform
debt remains unchanged. Mandatory upstream-equivalent review and patch
retirement remain open in the owning parity record.

## Context

Next e88b2d7 adds copy-mode support required by the latest upstream catch-up.
Its audio-transcoding pipelines use `-shortest` to stop padded audio when video
ends. The default shortest buffer makes the existing live RTSP regression
exceed its unchanged 30-second startup deadline. The previous accepted runtime
passes the same input. Diagnostic runs retaining `-shortest` with a 0.5-second
buffer restore startup and selected-audio/source-clock proof.

ADR 0100 normally requires official artifacts and unmodified upstream source.
Changing the startup deadline or skipping RTSP acceptance would hide the
regression. Removing `-shortest` globally would lose its intended EOF behavior.

## Decision

Permit one temporary Next source patch, based on an exact upstream revision,
that bounds shortest buffering to 0.5 seconds for live RTSP video inputs.
Preserve `-shortest`, existing copy-mode decisions and the upstream QSV-specific
buffer rule. Local files, finite RTSP and unrelated source types retain their
current behavior. Do not modify FFmpeg or introduce a runtime command wrapper.

Build matching Next artifacts for all six supported platforms, record source,
lockfile, patch and payload hashes, and identify the packages as ErsatzRS
builds. Preserve licenses and platform companions. Require source regressions,
decoded startup/EOF/handoff, audio/subtitle, Linux/Windows application and CI
evidence before adopting or publishing the distribution. Existing approved
hardware and ARM/macOS application acceptance debt remains explicit.

### Mandatory removal

Every Next revision update must first inspect upstream changes and run the
unchanged RTSP startup and EOF/handoff regressions against the unpatched
candidate. Equivalent behavior matters; matching our implementation is not
required. When upstream passes those checks and the compatibility gates,
remove the patch from the active manifest and build, return to verified
official artifacts where available, and resolve the retirement debt. Retain
the regression tests. Never carry this patch forward solely because it still
applies cleanly. The source/patch manifest must refuse an unreviewed revision.

### Package retention

The maintainer also authorized removal of obsolete Next packages from GitHub
and local storage. Before deletion, establish that current builds, retained
release/recovery workflows and the chosen rollback candidate do not require
them. Remove duplicate temporary downloads once verified evidence and the
required artifacts are retained. Do not remove an active dependency merely
because a newer candidate has been built.

## Consequences

ErsatzRS temporarily owns a small Next patch and six-platform packaging.
Reduced buffering can affect stream endings or delayed tracks, so diagnostic
startup success alone does not establish acceptance. Removal is mandatory
once equivalent upstream behavior is verified. ADR 0100 remains applicable
outside this narrowly scoped packaging exception.

## Alternatives Considered

- Wait for upstream: avoids patch maintenance but leaves ordered catch-up open.
- Increase startup deadlines: conceals the regression and weakens acceptance.
- Remove `-shortest`: loses the intended padded-audio EOF behavior.
- Rewrite FFmpeg commands in a wrapper: hides the runtime identity and bypasses
  the source-level pipeline contract.

## References

- [ADR 0100](0100-next-alpha-embedded-text-subtitle-limits.md)
- [Catch-up source map and evidence](../../plans/upstream-catch-up.md)
- [Owning parity record](../progress/parity/upstream-baseline-catch-up.toml)
