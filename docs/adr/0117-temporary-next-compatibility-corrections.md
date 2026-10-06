# ADR 0117: Temporary Next compatibility corrections

## Status

Accepted 2026-10-06. The maintainer accepts the consolidated correction policy,
including implementation, commit/push, CI and verified package publication.
Individual qualifying fixes do not require another ADR or approval. UCU-09 is
bounded by upstream v26.11.1, not later releases.

## Implementation

Partial — a candidate patch distinguishes static WebP from animated WebP and
repeats complete GIF cycles. All six static image formats and a finite GIF
animation pass decoded Linux application checks. Production packages are
unchanged; finite animated WebP remains blocked by FFmpeg demuxer seeking.

## Context

UCU-09 row 186 supplies image metadata duration and correct container hints to
Next. Published `next-e88b2d7-ersatzrs.1` renders PNG, JPEG, BMP and TIFF for the
scheduled duration. Single-frame WebP and GIF instead reach EOF and Next serves
its "playout item ended early" error card. FFmpeg decodes the source files;
the failure is in Next's input lifetime, not the ErsatzRS hint or FFmpeg build.
The runtime was built from e88b2d7. A later 2026-10-06 source audit found
upstream 0362692, which corrects VAAPI padding and color-state handling. It
does not change image repeat recognition or live RTSP buffering. Its runtime
acceptance and adoption remain pending.

Next's still-image loop recognizes PNG, JPEG, BMP and TIFF. WebP and GIF may
contain animation, so adding them to the single-frame loop would risk freezing
animation. Existing graphics inputs already have bounded repeat behavior, but
ordinary programme inputs do not expose that behavior through the wire schema.
AVI correctly follows Next's documented ContainerWithoutPts transcode fallback;
its decoded picture and selected audio pass and need no runtime correction.

A separate synthetic animation audit confirms that the existing FFmpeg package
decodes four distinct frames in both GIF and animated WebP. GIF with input
`-stream_loop -1 -ignore_loop 1` and animated WebP with `-ignore_loop 0`
produce the expected 12-frame sequence over six seconds, including a half-second
seek. Generic `-stream_loop -1` on animated WebP instead repeatedly reaches EOF
and times out. The correction must distinguish demuxers; a shared generic loop
option is insufficient. These fixtures have infinite embedded loop counts;
finite embedded loops, static inputs and the actual Next pipeline still require
acceptance. Evidence: `tmp/ucu09-row186/animation-audit/`.

ADR 0100 requires official artifacts. ADR 0116 makes one narrow exception for
live RTSP buffering. Requiring a new architectural decision for each defect
would repeatedly interrupt the same compatibility work. The maintainer asks
for one decision covering necessary Next corrections. This decision extends
the exception without changing UCU-09's product scope or acceptance gates.

The runtime evidence and source audit of pending rows 186-206 identify two
demonstrated local corrections: the accepted RTSP buffer bound and image repetition.
Song playback uses Next's existing separate audio/video inputs; output frame
rate and bounded troubleshooting already exist in e88b2d7. Subtitle eligibility,
cached stream URLs and playout-folder handling are application integration
concerns. These rows still need application acceptance; this audit neither
accepts them nor proves that no further runtime defects will be found.

## Decision

### One approval for the correction policy

Permit minimal temporary Next source corrections necessary to satisfy existing
UCU-09 requirements and preserve already accepted playback behavior. This covers
input lifetime, seeking and handoff, copy/transcode decisions, audio/subtitle
preservation, graphics, output timing, troubleshooting, process cleanup and
supported-platform compatibility when a demonstrated Next defect blocks those
requirements. It also permits adopting equivalent upstream fixes.

Acceptance authorizes implementation, regression tests, source/manifest updates,
commit/push, CI, and verified replacement Next package publication for qualifying
corrections. Such fixes use this ADR and the existing UCU-09 evidence and debt
records; they do not need separate architectural decisions or repeated approval.
This authority applies through UCU-09 closeout and to maintenance/removal of its
remaining patches afterward. It does not establish an indefinitely expanding
fork or authorize unrelated new features.

This policy supersedes ADR 0116's one-patch approval boundary.
Its accepted RTSP behavior, verification, package-retention and mandatory-removal
requirements remain in force. ADR 0116 remains the historical source for that
correction. ADR 0100's subtitle limitations and explicit refusals remain intact.

### Entry conditions for each correction

Before adding a patch:

1. Identify the accepted requirement, affected behavior and reproducible failure
   against an exact unpatched upstream revision. Separate an application
   integration error from a Next defect; fix it in its owning component.
2. Inspect current upstream for an equivalent fix and prefer that implementation
   after compatibility verification. Never retain or introduce a local patch
   merely because it applies cleanly.
3. Record the minimal correction, regression evidence, source/patch identity and
   removal criterion in the existing patch manifest and owning progress records.
   Reuse the current tooling; no additional tracking system is required.
4. Prove the changed behavior and relevant existing invariants before adoption.
   Failed acceptance keeps the patch a candidate, even when its scope is covered
   by this decision.

The initial open correction repeats finite WebP/GIF programme inputs for their
scheduled interval, preserving every animation frame, frame timing, seeks,
cancellation, separate silence/audio and ordinary video/live EOF behavior.
Static inputs, finite and infinite embedded loops, and schedule-boundary handoff
must be tested. Reuse Next's input machinery; no command wrapper, disguised
container, frozen first-frame substitute or replacement renderer is permitted.

### Packaging and acceptance

Every patch remains subject to source review and executable evidence. Extend
the reproducible source manifest and publish distinctly identified packages for
all six supported platforms only after their required gates pass. Preserve
source, lockfile, patch and payload hashes, licenses and companion libraries.
Batch compatible corrections into the same candidate/package build where
practical; preserve the ordered application acceptance baseline. Follow ADR
0116's retention rules for both GitHub packages and local copies. Reuse the
verified ErsatzRS-ffmpeg packages; this decision authorizes no FFmpeg rebuild
or release.

Require source-level argv regressions and decoded single-frame and animated
WebP/GIF acceptance across duration boundaries, seeks and cleanup. Preserve
PNG/JPEG/BMP/TIFF, AVI transcode fallback, audio/subtitles, RTSP startup and
EOF/handoff acceptance. Available Linux/Windows software checks remain required;
only the already accepted hardware/ARM/macOS application debt remains unverified.

Application milestone closeout and the stable release still require their full
gates and terminal green exact-SHA CI. A dependency package publication is not
application acceptance or stable-release approval.

### Mandatory removal and scope boundaries

At every upstream Next update, test the unpatched candidate first. Remove each
local patch as soon as equivalent upstream behavior passes its regressions and
compatibility gates, update source/package identity, and retain the tests.
Retirement is independent per patch; one unresolved defect cannot justify
carrying an already superseded correction. Return to verified official artifacts
when they provide the required behavior without local patches. The source/patch
manifest must refuse an unreviewed revision.

A new decision is needed only if the proposed solution changes an existing
architectural or product contract, introduces a separate runtime/dependency,
changes the native/Next default, relaxes an explicit refusal, expands feature
scope or weakens acceptance. A newly discovered qualifying bug is not such a
decision. Additional hardware/platform deferrals still require maintainer
approval; existing approved debt remains unchanged.

## Consequences

ErsatzRS owns a bounded, evidence-backed patch set with one approval boundary
and independent retirement criteria. The final number of corrections cannot be
known before runtime acceptance. Required tests, transparent scope and source
identity control that uncertainty. Row 186 and ordered catch-up remain open
until runtime acceptance is complete. Acceptance authorizes the work; it does
not claim that replacement packages or their verification are complete.

### Proposed FFmpeg extension

This amendment is proposed, not accepted. The accepted policy above still
excludes FFmpeg rebuilds and releases until the maintainer accepts this extension.

Finite animated WebP exposes a defect in the existing FFmpeg demuxer at source
revision `d32b387f2b0a484599d4587d651891f0c63c4238`. Its packet positions point
eight bytes beyond each ANMF chunk header, into the payload. Generic rewind
seeks there, although the demuxer expects a chunk header; bounded repeat fails.
The native infinite-loop path seeks to the correct header and decodes all frames.
Evidence and a four-line candidate correction are retained in
`tmp/ucu09-row186/animation-audit/webp-packet-offsets.json` and
`tmp/ucu09-row186/ffmpeg-review/0001-correct-animated-webp-seek-position.patch`.
The correction has not been built or runtime-verified.

Propose extending this same decision to the minimal FFmpeg animated-WebP
demuxer correction, its implementation, commit/push, CI and publication of
verified replacement Linux/Windows packages as v2.0.6. Reuse existing verified
inputs and build caches; adopt immutable version/checksum pins only after
acceptance. This exception adds no unrelated FFmpeg changes or platform waiver.

Require decoded finite/infinite animation, fractional seeking, scheduled
termination and cleanup, plus relevant graphics, audio and subtitle regressions
on Linux and Windows. Existing approved platform/hardware debt remains unchanged.
Track source/patch/payload hashes and remove the correction independently once
equivalent upstream behavior passes these checks, following the retirement
policy above. A build or publication alone does not accept row 186.

## Alternatives Considered

- Wait for upstream: avoids another temporary patch but keeps row 186 open.
- A separate ADR and approval for every defect: repeats the same maintenance
  decision without changing architecture or acceptance.
- An unrestricted permanent fork: permits unrelated drift and weakens removal.
- Accept an error card or omit the format: fails the user-visible image outcome.
- Classify animated-capable formats as single-frame stills: can freeze animation.
- Convert every such image through a new application producer: expands runtime
  ownership for a gap in the already used Next input machinery.

## References

- [ADR 0100](0100-next-alpha-embedded-text-subtitle-limits.md)
- [ADR 0116](0116-temporarily-bound-next-live-rtsp-buffering.md)
- [Upstream VAAPI padding correction](https://github.com/ErsatzTV/next/commit/03626929089a438245cfc77cbedda6e92d98dc9f)
- [Catch-up source map](../../plans/upstream-catch-up.md)
- [Owning UCU-09 milestone](../progress/milestones.toml)
