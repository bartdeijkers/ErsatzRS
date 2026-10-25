# ADR 0128: Complete FFmpeg 9 compatibility and private Rockchip runtime

## Status

Accepted 2026-10-24. The maintainer explicitly authorizes local implementation,
builds and tests for ErsatzRS-ffmpeg v2.0.7 and the private pinned Next MPP
companion/readiness guard. Push, GitHub CI and publication remain separate.
One consolidated UCU-12 decision covers the runtime amendments below.
ADR 0117's accepted UCU-09 delivery authority does not extend to this window.

## Implementation

Partial — all four non-macOS FFmpeg v2.0.7 kits pass their software,
archive/provenance and separately authorized package-CI checks. Separately
approved publication distributes those exact kits and all six Next rs.4 kits;
anonymous downloads match every approved asset checksum. Linux and Windows
applications pass ordinary public cold acquisition and managed restart with
the published pins. The private loader and actual ARM64 no-device fallback
preserve decoded video, Dutch audio and captions; empty initialized codec sets
refuse the hardware wrapper instead of selecting RGA.

Available Linux/Windows current-source application gates pass. Native macOS FFmpeg
builds, software acceptance and complete six-RID FFmpeg distribution remain
deferred under the maintainer's instruction to skip macOS builds until explicit
resume. Approved hardware debt stays unverified. Milestone `ucu-12` owns exact
publication, runtime and remaining acceptance evidence; these dependency
approvals do not authorize application delivery or Docker publication.

Maintainer acceptance amendment, 2026-10-25: macOS has no priority and UCU-12
may close without its outstanding builds and acceptance. Keep the macOS
FFmpeg/Next build, software, package, application and distribution gaps as
explicit unverified debt; do not start those builds until instructed. Qualified
non-macOS successors may be adopted independently while macOS retains its exact
older package identity. Old packages must not be relabelled as new builds or
credited with the new caption/audio fixes. This supersedes uniform six-kit
completion as a prerequisite for UCU-12 closure, without removing supported
platforms or changing the separate publication-approval gate.

The separately approved rs.6 publication supplies four unchanged, qualified
non-macOS Next packages and their source/patch/licence materials. All 23 public
downloads match the approved checksums. macOS retains the original rs.4 assets.
Per-RID version, release tag and patch identity are resolved together for
download, cache, provenance and reported runtime version; they do not weaken
archive or payload checks. Application adoption and affected CI remain pending.

## Context

UCU-12 is bounded by ErsatzTV v26.11.4, rows237–254. Row246 adopts upstream
FFmpeg8.1.2-5 at `aed834faa58cc0fd1f620eaf4edaa1a704182b31`; ErsatzRS keeps
FFmpeg9 and its existing CLI architecture. Published ErsatzRS-ffmpeg v2.0.6
does not satisfy three requirements introduced by that upstream release:

- Bitmap cues can move `sub2video.last_pts` backwards. Subsequent audio packets
  redundantly push, scale and overlay the same canvas. The unchanged upstream
  reproducer fails on the published Linux binary:835 versus250 frames,
  ratio3.34 against its2.0 limit. This is separate from the unknown-size PGS
  canvas correction already qualified in Native/Next.
- FFmpeg9 retains VideoToolbox's faulty caption-SEI merging parser. Upstream
  replaces merging with a separate caption SEI before the first slice,
  preserving captions. Both source fixes apply without fuzz to the exact pinned
  FFmpeg9 files. Native macOS execution remains unproven.
- The published Linux ARM64 package lacks the full RKMPP hardware-device,
  codec and RKRGA filter pipeline required by Native and Next0.2.3. Enabling
  FFmpeg9's existing basic codecs alone cannot supply that contract.

Next's ARM64 MPP probe also loads bare `librockchip_mpp.so.1`. Static linkage
inside a separate FFmpeg executable cannot make that probe work. A managed
installation must supply its own reviewed library rather than depend on an
unknown system installation. This private runtime dependency requires a new
decision under ADRs0001/0117; ordinary bug maintenance alone would not.

## Decision

Propose one bounded ErsatzRS-ffmpeg **v2.0.7** amendment, retaining the FFmpeg9
base, existing verified corrections and independent patch-retirement rules:

1. Backport the monotonic bitmap-heartbeat correction to every downloadable RID.
   Reuse the exact upstream regression and its unchanged2.0 threshold.
2. Backport the caption-preserving VideoToolbox correction. Qualify it through
   the native macOS encoder and decoded A53 side data, with captions enabled.
3. Adapt the upstream RKMPP/RKRGA patch only to Linux ARM64. Add pinned MPP
   `a9380ef333102ac318628f83b5f7a460d377749e` and RGA
   `1d330cc28551943bed3380261a5a9c6fbd58ff53`, reusing the existing libdrm.
   Build FFmpeg and ffprobe from the same patched source/libraries. Confine the
   common pixel-format enum additions to that target and verify NV15/NV20
   software conversion; other RID pixel-format layouts remain unchanged.
4. Build a shared MPP companion from that same pin/configuration for the
   Linux ARM64 Next package. FFmpeg can retain static MPP/RGA linkage. Next
   probes compiled formats and owned create/init/destroy contexts in its
   independent library instance; no objects cross the process or
   library-instance boundary.
5. Load the companion by an absolute path derived from `current_exe()` and its
   owned package directory. Do not alter global loader paths or silently use an
   unpinned system library. Extend the existing companion/provenance checks to
   ARM64: exact payload/notice hashes, confined extraction, missing/tampered
   companion refusal, relocation and truthful Health diagnostics.

MPP's `mpp_check_support_format` reports compile-time codec support, not driver
readiness. Next023 currently treats that result as usable hardware. Bundling
MPP must therefore also retain software fallback on ordinary/non-Rockchip
ARM64 machines: establish actual MPP device/driver readiness before selecting
RKMPP. Library presence or compiled-format enumeration cannot grant hardware
capability. Require public MPP context creation and codec initialization for
each advertised decode/encode capability, always destroying the context on
success and failure. Retain only initialized capabilities; zero capabilities
must produce the existing software fallback. The upstream RKMPP hardware-device
creation alone is not such a probe: it only sets allocation flags. Qualify
bundled-library/no-device playback before adoption, and retain the existing
owned worker startup deadline for an unresponsive driver. RGA operation and
positive Rockchip playback remain separate mandatory hardware qualification.

Retain MPP's Apache-2.0 and MIT licence texts and Rockchip notices, plus RGA's
Apache-2.0 COPYING and applicable author notices. Verify actual shared MPP
`DT_NEEDED` after building; the source link graph establishes no mandatory
shared RGA companion. Any additional non-system dependency needs explicit
source, licence, package and loader qualification before adoption.

Use an isolated source candidate and existing recipes/caches. Preserve the
separate FFmpeg checkout, other work, current packages and exact evidence.
Do not overwrite v2.0.6 or rebuild unrelated images. Source/build identity,
archive/payload checksums and the complete affected software/graphics/subtitle/
audio/transport gates remain required. The four existing package RIDs require
corrected candidates; both macOS candidate jobs must qualify their applicable
source/build/VideoToolbox obligations when CI is separately authorized.

The ARM64 companion requires a reviewed Next source/packaging candidate after
the current `.2` candidate; all six Next kits must retain consistent source,
patch, compiler and companion provenance. Existing valid components may be
reused only within their unchanged scope. No stored engine/profile choices,
callback surfaces or international track preferences change.

## Consequences

Managed ARM64 installations can resolve the required MPP probe without a
machine-wide library installation. The larger upstream Rockchip patch and two
new dependencies add source/licence and platform qualification obligations.
Cross-builds, emulation and inventories cannot prove Rockchip hardware behavior.
Real decoding/encoding, RGA scaling/overlay, NV15 colour and loudness checks need
a supported Rockchip board and kernel drivers; the Raspberry Pi3 is Broadcom.

On 2026-10-24 the maintainer separately approved explicit unverified hardware
acceptance debt for Rockchip RKMPP/RGA, Intel QSV HDR, AMD VAAPI and macOS
VideoToolbox captions. No native success is inferred. Build, software, checksum
and all other acceptance requirements remain required. Existing debt remains
unchanged; the feature's `ucu-12-hardware-acceptance` debt owns this new scope.

Maintainer acceptance authorizes the proposed local implementation/candidate
build work. Source pushes, CI dispatches, package publication and application
delivery retain the active goal's separate approval gates. Exact reviewable
commits and tested artifacts must exist before requesting those actions.

## Alternatives considered

- Retain v2.0.6 unchanged: the demonstrated common defect and missing ARM64
  functionality would remain, so row246 could not close.
- Disable captions or silently substitute another accelerator: changes the
  accepted viewing/profile behavior and does not fix the defects.
- Adopt upstream FFmpeg8 or an unrelated mutable image: violates the supported
  FFmpeg9 and exact-artifact contracts.
- Link FFmpeg statically and rely on system MPP for Next: does not provide a
  reproducible managed runtime or a complete installation.

## References

- [ADR 0117](0117-temporary-next-compatibility-corrections.md)
- [UCU-12 ledger and source map](../../plans/upstream-catch-up.md)
- `docs/progress/milestones.toml`, milestone `ucu-12`
- [Pinned upstream release notes](https://github.com/ErsatzTV/ErsatzTV-ffmpeg/blob/aed834faa58cc0fd1f620eaf4edaa1a704182b31/release-notes/8.1.2-5.md)
- [Heartbeat regression and retirement record](https://github.com/ErsatzTV/ErsatzTV-ffmpeg/blob/aed834faa58cc0fd1f620eaf4edaa1a704182b31/patches/manifests/0017-sub2video-heartbeat-burst.yaml)
- [VideoToolbox correction and native proof](https://github.com/ErsatzTV/ErsatzTV-ffmpeg/blob/aed834faa58cc0fd1f620eaf4edaa1a704182b31/patches/manifests/0016-videotoolbox-caption-sei.yaml)
- [RKMPP/RKRGA source and ABI contract](https://github.com/ErsatzTV/ErsatzTV-ffmpeg/blob/aed834faa58cc0fd1f620eaf4edaa1a704182b31/patches/manifests/0018-rkmpp-rkrga.yaml)
- [Pinned MPP link graph](https://github.com/nyanmisaka/rk-mirrors/blob/a9380ef333102ac318628f83b5f7a460d377749e/mpp/CMakeLists.txt)
- [Pinned RGA licence](https://github.com/nyanmisaka/rk-mirrors/blob/1d330cc28551943bed3380261a5a9c6fbd58ff53/COPYING)
