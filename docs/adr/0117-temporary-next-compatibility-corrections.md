# ADR 0117: Temporary Next compatibility corrections

## Status

Accepted 2026-10-06. The maintainer accepts the consolidated correction policy,
including implementation, commit/push, CI and verified package publication.
Individual qualifying fixes do not require another ADR or approval. UCU-09 is
bounded by upstream v26.11.1, not later releases.

The maintainer also accepted the consolidated FFmpeg extension on 2026-10-06,
conditional on updating the separate ErsatzRS-ffmpeg repository to **v2.0.6**.
This authorizes its implementation, four package builds, testing, commit/push,
CI and verified publication. It does not claim completed runtime acceptance.

## Implementation

Partial — accepted corrections are implemented; approved platform/hardware
obligations and independent upstream patch retirement remain open.

UCU-10 update (2026-10-14): the maintainer separately authorizes the v26.11.2
local catch-up, the source-only macOS candidate push/CI and verified Next021
dependency publication. Both unchanged unpatched regressions still fail, so
both minimal patches are retained with independent removal criteria. The exact
six-RID `next-7f75012-ersatzrs.1` release is published; all20 assets pass anonymous
download/checksum checks. Linux/Windows bundled startup, cold acquisition,
managed restart and decoded lifecycle proof pass. This does not publish an
application or FFmpeg release, push main or verify the approved platform/hardware
debt. UCU-10 owns source identities and detailed acceptance.

Earlier row186 corrections are implemented and accepted. The runtime
distinguishes static WebP from animated WebP and repeats
complete GIF/WebP cycles. All six static formats and finite/infinite animated
WebP pass decoded Linux application checks, including local/HTTP half-second
seeking, six-second handoff to static WebP and exact eight-second termination.
The exact native Windows release executable also passes 19 runtime cases plus
one interval/silence unit, including copy/AVI, Burn/Convert/Off subtitles,
alternate audio and RTSP source-clock/EOF/handoff. Live inputs retain their EOF
behavior. The reviewed upstream0362692 and both local patches are now published for all
six platforms as `next-0362692-ersatzrs.1`. Both macOS package jobs passed at
checkpoint `4b5b1523`; Linux and Windows optimized executables each passed 20
focused checks. Production staging pins all source, patch and archive hashes;
all six real archives pass its guards, and authenticated release staging
matches the verified executable. Earlier evidence described as a public download
used the authenticated GitHub CLI and did not establish anonymous availability.
On 2026-10-07 the maintainer approved a public, package-only
`bartdeijkers/ErsatzRS-next` destination. All19 unchanged assets are published
there, and actual anonymous downloads of all six archives match their accepted
hashes. No Next or FFmpeg rebuild was performed. Frozen row200 source08 pins
that public destination and passes actual Linux cold acquisition before
listening, decoded TS/HLS playback and all six standalone offline/override
startup cases. Matched Chromium/Firefox checks also pass; available native
Windows cold and bundled decoded playback and all six read-only/offline/override
startup cases now pass with independent reviews. Row200 is accepted under the
conditional availability decision in ADR0095; final window acceptance remains open. Production HTTP smoke passes normal/preview
playback, selected Dutch audio and rendered captions, reconnect and idle cleanup.
Row186 is accepted; later ordered rows, final window CI and ARM/macOS application
debt remain open. UCU-09 owns the canonical acceptance evidence.

All four v2.0.6 FFmpeg packages are published with verified archive, executable,
source and patch checksums. Package CI
[37539516801](https://github.com/bartdeijkers/ErsatzRS-ffmpeg/actions/runs/37539516801)
passed all four jobs at release commit `b221a3d32d3169fc563d2443af7f897d2259b1e8`.
Clean Bookworm and emulated ARM software checks pass; native Windows graphics,
AV1, animation, SVG, sparse subtitles and selected alternate audio also pass.
The compiled Linux demuxer preserves complete ICC/EXIF and removes short-read
side data. Native Windows NVIDIA interop and Linux software Vulkan pass.
All ten published asset digests match the frozen verified files. The manifest
SHA-256 is `af6132181dd191b5eb68503debcc6d7e699d6a6047ab85eaa93c74f64e123f0f`.
[The stable runtime release](https://github.com/bartdeijkers/ErsatzRS-ffmpeg/releases/tag/v2.0.6)
supplies the tested runtime for row186 acceptance; package publication alone
does not complete UCU-09 application acceptance.

## Context

UCU-09 row 186 supplies image metadata duration and correct container hints to
Next. Published `next-e88b2d7-ersatzrs.1` renders PNG, JPEG, BMP and TIFF for the
scheduled duration. Single-frame WebP and GIF instead reach EOF and Next serves
its "playout item ended early" error card. FFmpeg decodes the source files;
the failure is in Next's input lifetime, not the ErsatzRS hint or FFmpeg build.
The runtime was built from e88b2d7. A later 2026-10-06 source audit found
upstream 0362692, which corrects VAAPI padding and color-state handling. It
does not change image repeat recognition or live RTSP buffering. The reviewed successor package adoption is recorded above; native VAAPI
padding now has independently reviewed native graph corrections and software
range/depth proof. Ordered row207 is accepted as Covered; VAAPI hardware
execution remains explicitly unverified acceptance debt. No additional FFmpeg
binary amendment is established.

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

The runtime evidence and source audit of pending rows 186-208 identify two
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
identity control that uncertainty. Row186 runtime acceptance is complete;
later ordered catch-up and final application gates remain open. Acceptance authorizes the work; it does
not claim that replacement packages or their verification are complete.

### Accepted FFmpeg extension

The maintainer accepted this amendment after the consolidated audit, explicitly
requiring ErsatzRS-ffmpeg v2.0.6. This extends the earlier Next-only authority
to the bounded FFmpeg changes and package scope below.

Finite animated WebP exposes a defect in the existing FFmpeg demuxer at source
revision `d32b387f2b0a484599d4587d651891f0c63c4238`. Its packet positions point
eight bytes beyond each ANMF chunk header, into the payload. Generic rewind
seeks there, although the demuxer expects a chunk header; bounded repeat fails.
The native infinite-loop path seeks to the correct header and decodes all frames.
Evidence and a four-line candidate correction are retained in
`tmp/ucu09-row186/animation-audit/webp-packet-offsets.json` and
`tmp/ucu09-row186/ffmpeg-review/0001-correct-animated-webp-seek-position.patch`.
The initial proposal was not built or runtime-verified; the implementation
checkpoint above records its subsequent verified publication. Audited FFmpeg upstream
`9008db55d29631ae4ec62d46edea77c840ce0663` still has this seek-position defect.

#### Consolidated audit through v26.11.1

The 2026-10-06 audit covers all 23 pending application rows, the current Next
revision `03626929089a438245cfc77cbedda6e92d98dc9f`, all 15 patches in
ErsatzTV-ffmpeg `8.1.2-4`, and the exact four published first-party RID payloads.
It establishes one additional FFmpeg source amendment:

- Backport upstream FFmpeg
  [`cfaaf0062c085433ccb500689f137849794aeb63`](https://github.com/FFmpeg/FFmpeg/commit/cfaaf0062c085433ccb500689f137849794aeb63).
  The same WebP demuxer allocates ICC/EXIF side data for the declared chunk size
  but accepts a positive short read, leaving unread bytes uninitialized. The
  upstream fix requires exact-length reads and removes incomplete side data.
  The published source lacks it. The original truncated EXIF fixture finished probing without an error.
  The compiled corrected demuxer subsequently passed complete and truncated
  ICC/EXIF tests, preserving complete data and removing incomplete side data.
  This does not establish an exploit in the previous runtime.

| Pending rows / runtime surface | Audit result and required owner |
| --- | --- |
| 186: still and animated images | Next lifetime correction plus FFmpeg WebP rewind and ICC/EXIF fixes. |
| 187: block scheduling | Application scheduling/history changes; no new FFmpeg primitive. |
| 189: songs | Application separate audio/video and song-background integration; existing PNG, lavfi, drawtext and audio filters suffice. |
| 190/192: macOS acquisition and unsigned packaging | Adapt through ADRs 0008/0063; keep unbundled FFmpeg 9 acquisition and explicit operator overrides. No FFmpeg 8 downgrade or one-time override of saved paths. |
| 193/201: seek and troubleshooting | Application/Next integration with a bounded troubleshooting process; no new FFmpeg library. |
| 194/195: duration and deep scan | Application scanner/parser changes; existing ffprobe fields suffice, with the Matroska duration guard below. |
| 197/198: subtitles and cached URLs | Application eligibility and URL preparation; preserve ADR 0100 refusals. |
| 199/200: output rate and engine selection | Application/Next schema integration using existing rational frame-rate support; preserve ADR 0095's conditional Next default and stored native choices. |
| 204: Windows playout folders | Application filesystem lifecycle; no FFmpeg amendment. |
| 207: VAAPI padding | Adopt the upstream Next correction and correct the matching native Rust pipeline path; no FFmpeg binary patch established. |
| 188/191/196/202/203/205/206/208: build or release documentation | Source/workflow review; no additional codec or library requirement established. |

All 15 upstream FFmpeg patches are already present, have an FFmpeg 9 equivalent,
or are inapplicable with the existing dependency pins. In particular, the fixed
QSV decoder pool, D3D11 handle index and child-context transfer corrections are
already in v2.0.4; they must not be added again. The pinned libva already enables
the surface-alignment query. Existing x64 OpenAPV predates its changed 1.1 API;
ARM packages carry the matching compatibility patch. Reassess these conditions
only if their dependencies change. Compiled hardware support remains distinct
from the already unverified hardware acceptance.

The four downloaded packages already include Cairo, librsvg and text shaping.
None uses `--enable-small`; the upstream container metadata fix does not require
another managed-package amendment. A bounded Linux probe produces H.264/AAC
with a PNG background at `30000/1001`, confirming the existing primitives.
These probes establish capabilities, not acceptance of the pending song/rate rows.

Two application corrections remain visible alongside the binary amendments:

- The native `apply_pad_filter` selects `pad_vaapi` without driver eligibility
  or BT.2020 handling. Next's upstream fix limits hardware padding to compatible
  Intel iHD, restores source color tags around its padding workaround, and
  provides fallback for other drivers. Correct the native capability/filter
  selection too; preserve frame depth, HDR signaling, range and encoder uploads.
  Driver-specific runtime proof remains subject to the existing hardware debt.
- FFmpeg 9 may synthesize a Matroska stream's numeric duration from the container.
  A one-second video plus a twenty-second subtitle reports video `duration=20`
  but video `DURATION=00:00:01.000000000`; the two video packets end at one second.
  Copying upstream's numeric-first parser would retain the overrun. Prefer valid
  content-stream Matroska duration tags over that estimate, preserving upstream's
  content-bound outcome, stream selection and incomplete-audio fallback. Scope
  this preference to the demonstrated Matroska conflict with a container-derived
  numeric estimate; retain numeric precedence for other containers and fallback
  for missing/invalid tags. Conflicting/stale tags need regression coverage to
  prevent undercounting. This requires a scanner regression and decoded handoff,
  not an FFmpeg source patch.

Retained audit evidence is in `tmp/ucu09-ffmpeg-amendment-audit/`: the upstream
source window, `patch-review/patch-equivalence.json`, `arm-package-audit.json`,
`existing-runtime-probes.json`, `content-duration-packets.json` and malformed
WebP probes. The owning milestone records the result; no pending row is accepted
by this audit alone. Subsequent row186 runtime acceptance is recorded in UCU-09.

#### Authority and package scope

Extend this same decision to the two necessary WebP corrections, their regression
tests, implementation, commit/push, CI and verified v2.0.6 package publication.
Also cover a subsequently demonstrated minimal FFmpeg correction necessary for
an existing requirement through the fixed row-208 boundary, under the same entry,
identity, compatibility and independent-retirement conditions as the Next policy.
Source proof of a matching upstream memory-safety fix qualifies; speculative
changes, unrelated features, general dependency upgrades and acceptance waivers
do not. Prefer an equivalent upstream correction when available.

Build corrected packages for **all four currently supplied RIDs**: Linux x64,
Windows x64, Linux ARM64 and Linux ARMv7. Published v2.0.5 reuses v2.0.4 x64
archives; both ARM archives also use the affected `d32b387` source. Carrying the
old ARM bytes into the replacement release would knowingly preserve the defect.
This supersedes the earlier proposal to replace only Linux/Windows x64 and reuse
the ARM archives. Keep the older release available for existing explicit pins;
each replacement asset needs its own source, patch, payload and archive hashes.

Reuse verified dependency inputs and available build caches. Keep the existing
FFmpeg 9 source pin with these narrow corrections unless source review establishes
why another exact revision is necessary. Preserve graphics, SVG, subtitle,
alternate-audio, codec and portability capabilities. Use host Docker for build
containers, without a Docker socket mount. Application containers consume the
managed packages; this proposal does not require publication of another separate
FFmpeg runtime image or use a mutable image tag as package evidence.

No macOS first-party asset currently exists. ADR 0063 permits an operator FFmpeg
9 runtime on PATH in that case; row 190 does not overturn that accepted contract.
This extension does not promise macOS packages or verified macOS animation.
An unpatched operator runtime can share the WebP defect; keep that limitation
and the existing approved macOS application acceptance debt explicit.

Require decoded finite/infinite animation, fractional seeking, exact scheduled
termination/handoff and process cleanup on Linux and Windows, plus emulated
software checks of both corrected ARM payloads. Verify malformed ICC/EXIF error
handling and removal of incomplete side data. Preserve GIF/static-image, SVG
alpha/text, drawvg/drawtext, sparse-subtitle, alternate-audio, RTSP and existing
available NVIDIA/Vulkan regressions; verify clean portability baselines and all
download checksums. Existing native ARM/macOS and hardware acceptance debt remains
unchanged; emulation is not native application or GPU proof.

Remove each local correction independently when equivalent upstream behavior
passes its required regressions. A package build/publication does not accept
row 186, another pending row, UCU-09 or the stable application release. The full
closeout and terminal green exact-SHA CI gates remain required.

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
- [ADR 0008](0008-ffmpeg-8-1-runtime-and-multi-arch-images.md)
- [ADR 0063](0063-ffmpeg-9-runtime-and-profile-options.md)
- [Upstream FFmpeg package changes](https://github.com/ErsatzTV/ErsatzTV-ffmpeg/blob/8.1.2-4/release-notes/8.1.2-4.md)
- [Upstream VAAPI padding correction](https://github.com/ErsatzTV/next/commit/03626929089a438245cfc77cbedda6e92d98dc9f)
- [Catch-up source map](../../plans/upstream-catch-up.md)
- [Owning UCU-09 milestone](../progress/milestones.toml)
