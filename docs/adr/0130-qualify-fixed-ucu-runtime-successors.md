# ADR 0130: Qualify fixed UCU runtime successors

## Status

Accepted 2026-11-07. The maintainer authorizes local implementation, builds
and verification. Commit/push, GitHub CI, publication, production installation
and application releases require their separate authorization.

Runtime delivery amendment, 2026-11-07: the maintainer approves the prepared
Next025 recipe checkpoint and FFmpeg2.0.10 source commit/push, the existing four
FFmpeg package-verification jobs without rebuilding, and verified FFmpeg source
integration with Docker publication suppressed. Exact staged runtime publication
is authorized after verification and row271 security applicability are resolved,
followed by local managed pins and disposable download tests. Application main
integration, application CI/release and operator installation remain separately
gated. The maintainer has no additional advisory details; investigate the fixed
ErsatzTV source and published Windows packages without inferring non-applicability.

Later amendment, 2026-11-07: the maintainer approves retaining the unresolved
row271 applicability obligation as explicit unverified later-work debt and
resumes the goal. This removes that prerequisite from the already-authorized
runtime publication sequence. Verification, exact assets, all other required
acceptance and application/operator authorization boundaries remain unchanged.
The owning record is `ucu14-windows-advisory-applicability` in the baseline
catch-up parity file; no advisory fix or non-applicability is accepted.

## Implementation

Partial overall; the local FFmpeg2.0.10 source/build/package/software component
is complete on the retained FFmpeg9 base. Four packages bind exact dependency
images, source deltas, applied patches, payloads and notices. Required software,
ICC/EXIF, SVG, teletext, pacing and unchanged buffering/caption regressions pass
within their recorded platform scopes. Available native Windows CUDA, source
format and low-frame-rate generated commands also pass independent decoding.
`tmp/ucu14/ffmpeg210/acceptance.json` owns the complete scoped proof; its package
manifest is `f08f3b3eb95fbfe78e9acdf0169fffcb149ff2037a567f9d498d073dd08368d2`.
Unavailable hardware/native ARM/macOS remain unverified. Publication, managed
adoption and application/window acceptance remain separate in UCU14/15.

The local Next025 bare-worker source/build/package/software component is complete
within its explicit scope. Twelve exact patches reproduce292 prepared files;
455 selected worker/reader and13 core/schema cases, strict affected lint and
660 serial software playback cases pass. All four required archives match exact
source, payload and companion hashes. Actual clean Bookworm, native Windows
software/CUDA H264/CUDA HEVC and both ARM emulators each decode200 frames with
selected Dutch audio and independent subtitle-reference checks. ARM64 proves
whole-channel software fallback with the pinned companion but no drivers.

Independent stock-source/security, actual copied-caption, no-device MPP,
finite-cut and single-use contract controls still fail their unchanged
regressions; all ten corrections remain required. The current finite-cut and
single-use negatives stop at missing negotiated contracts, without a new stock
physical timing claim. Both strict2164/2143 source-frame/caption/audio/cleanup
routes, grouped single-use delivery and ten compatibility playback checks pass
on the successor. The metadata-only twelfth patch declares the existing MIT
license and existing025 path versions, with the security lock unchanged; the
complete four-target dependency policy passes. Exact evidence and limitations:
`tmp/ucu14/next025/qualification.json`.

The indexed reader's demonstrated overlay-ownership correction now passes
33 affected reader/config cases, strict lint and four revised package/playback
checks. Linux and native Windows hosts preserve clock, selected audio and file
write economy through active edits, with separate normal/browser-preview outputs
and finite troubleshooting. Windows publication/recovery and row261 background
proof also pass. Corrected recipe/packages and retained prior negative:
`tmp/ucu14/next025/indexed-integration/qualification.json`.

Applicable final application/platform/window gates, published acquisition and
managed adoption remain open. No runtime pin,
publication, production installation or baseline acceptance is changed.

## Fixed release-tail scope amendment (2026-11-07)

The maintainer extends UCU-15 through v26.12.0 at
`53b3b42a02533ad8e72af09565b494f2b19e40fc` and accepts this amended decision.
Final qualification must account for released Next0.2.5 at
`c5cb065f9f35dbbcb7c05845f0f92c70f576a480` and upstream FFmpeg8.1.2-8 at
`83b9746e7e121b7a6bc4d7220bac94ec8b911ce5`. Complete source comparisons show
no crate implementation changes from audited Next92e93d72, and only release
notes/revision changes from audited FFmpeg4089c130. Retain earlier proof with
its original identity and qualify one consolidated successor, including the
accepted ADR0131 extension, against the final fixed source. Keep FFmpeg9 and
Native. Local builds/tests are authorized by acceptance; runtime acceptance,
publication and adoption still require their separate evidence and authority.

## Context

The initial audit used the fixed ErsatzTV/legacy snapshot
`06c008b49964cb9d8232aa2804cd8533eb5576e1`. Its terminal Next pin resolves
to `92e93d72c0e3694417a22207374b9d12cbf38087`; its FFmpeg pin resolves to
`4089c1301508a503d277ff94adc8ec2957946198` in
ErsatzTV/ErsatzTV-ffmpeg. The latter is an FFmpeg 8.1.2 patch set, not a
supported ErsatzRS runtime. ErsatzRS retains FFmpeg 9 and drawvg.

Published ErsatzRS Next `0.2.3-54c6cb9-ersatzrs.7` and FFmpeg `2.0.9` remain
the current production inputs. The new contract and software defects prevent
claiming that those inputs cover all fixed-window requirements:

- Upstream row261 adds channel schema `0.1.3`, `fallback.background_image`
  and an offline callback bounded by `x-etv-until`. ErsatzRS still emits
  schema `0.1.2` and has no configured background-image field. Its existing
  session authentication and gap admission remain required.
- Fixed Next adds VAAPI/libplacebo and low-power selection, safer QSV/AMF
  selection and segmentation, correct selected embedded text subtitles and
  square-pixel output. The terminal revision also supplies describe/probe,
  callback negotiation, low-fps NVENC, watermark graph-rebuild continuity and
  software decode for unsupported chroma/depth. These are behavior changes,
  not a reason to remove Native or accept a newer binary by version alone.
- The exact published FFmpeg payload `50ee90f724b22c4706e782fa834670cb14539d798e2693f3466c248b724b3eab`
  fails upstream's unchanged separate-input sync-queue reproducer: 301 video
  frames held versus the limit of 30; a shared input holds 1. This software
  reproducer establishes the scheduling defect without QSV hardware.
- The same payload fails the unchanged MPEG-2 software caption reproducer:
  1,680 bytes on a frame versus the 93-byte limit. This affects software
  decoding too, independently of the upstream VAAPI symptom.
- The retained FFmpeg source matches the published base revision
  `d32b387f2b0a484599d4587d651891f0c63c4238` and full source-delta SHA-256
  `6654fb23a15a5a5b3fb0083ca21e4dec0b5bad4a20768bb3e31153cb310b8dde`.
  Source comparisons establish missing fixed-window corrections. They do not
  establish native hardware behavior. The 2.0.9 teletext/concat additions
  concern different source files and must remain intact.

ADRs 0117 and 0128 do not grant blanket authority for a later runtime window.
One bounded decision is preferable to separate approvals for every necessary
port or context adjustment. Milestones `ucu-14` and `ucu-15` own acceptance;
`tmp/ucu14/ffmpeg-source-audit/` retains exact comparisons and original negatives.

## Decision

### Preserve the two-engine application

Keep ErsatzRS Native supported and selectable alongside Next. Preserve its
routes, controls, saved profiles and channel selections. Never carry over
upstream's Native removal or global hiding of Native fields. Shared FFmpeg
changes require affected Native as well as Next playback proof. Keep
international track/caption selection, teletext, Health, callback isolation,
bounded ownership and persistence requirements unchanged.

### Prepare one fixed Next successor

Prepare a local candidate based exactly on Next
`c5cb065f9f35dbbcb7c05845f0f92c70f576a480` (released0.2.5), covering the cumulative fixed
UCU-14/15 runtime requirements without rebuilding each intermediate bump.
Its identity must disclose every retained correction and the ADR0131 extension,
rather than claim equivalence to an unpatched upstream release. Record exact source archive,
compiler, original/prepared lock, patch and archive/payload hashes.

Review all ten current temporary corrections independently against unpatched
source and their retained regressions. Retire a correction only when equivalent
unpatched behavior passes; preserve the tests. Rebase only still-required
minimal corrections and their tests. A source-context conflict alone neither
proves retirement nor authorizes a different workaround. Review the actual
worker dependency graph, advisory/license policy and private ARM64 MPP
companion rather than carrying an obsolete lock patch mechanically.

Integrate the required channel/playout schemas, background selection and
authenticated callbacks, describe/contract validation and generation-fenced
hardware observations through existing managers, workers and Health surfaces.
Do not introduce a second runtime service, new upload destination or broaden
the fixed UCU scope. Keep the accepted local Download Results deviation.

### Prepare ErsatzRS-ffmpeg 2.0.10 on FFmpeg 9

Use the existing FFmpeg 9 base, qualified features, dependency inputs and
recipe/package machinery. Prepare four local successor packages for Linux
x64, Linux ARM64, Linux ARMv7 and Windows x64. Preserve teletext original PTS,
concat clocks, SVG/drawvg/text, sparse streams and existing runtime fixes.

Audit and adapt the following fixed upstream amendments where the verified
FFmpeg 9 source lacks equivalent behavior:

| Upstream amendment | Required behavior |
| --- | --- |
| 0019 | Synchronize Vulkan-to-VAAPI reverse mapping. |
| 0021 | Avoid duplicate QSV AV1 HDR side data. |
| 0022, 0024, 0025, 0026 | Preserve QSV decode clocks, avoid later-packet DTS and corrupted leading output, and handle bare VC-1 start codes. |
| 0023 | Drop captions belonging to skipped MPEG-2 pictures. |
| 0027–0030 | Order AMF HEVC headers, handle leading frames, export correct single HDR metadata and match the decoded frame format. |
| 0031 | Schedule separate-input encoder queues by their limiting stream. |
| 0032 | Permit hardware filter-graph rebuilds when source colour tags change. |
| 0033 | Safely adopt a replacement compatible QSV frame pool. |

These are fourteen non-macOS patch amendments, not fourteen verified hardware
fixes. Initial isolated applicability checks found nine clean applications. Ordered
application subsequently proves the QSV VC-1 patch applies after its prerequisites.
Four AMF patches require reviewed context adaptation; compilation additionally
requires FFmpeg9's explicit UNCHOKE_ALL scheduler argument. Preserve exact
unmodified-upstream comparisons and require compile, focused regression and
affected software/runtime proof after adaptation. Do not claim a patch is
needed solely because its number is absent.

Also reconcile the existing AMF patch's removal of its unusable `vc1_amf`
wrapper: VC-1 viewing must remain available through safe software decoding,
with truthful hardware capability reporting. Upstream's Rockchip patch change
is configure-context rebasing after that removal, not new Rockchip behavior;
preserve the already-qualified private MPP/RGA contract and its tests.
Additional fixes qualify only if necessary to preserve an existing accepted
invariant while applying these exact changes. A later upstream revision, new
feature, dependency architecture or new acceptance deferral needs a new decision.

### Acceptance and delivery boundaries

Both unchanged software negatives must pass on the exact successor payloads.
Run affected upstream/source tests and available Linux/Windows GPU trials,
plus the existing Native/Next decoded, international audio/caption, teletext,
transport, ownership and managed-acquisition gates. ARM build/package/software
emulation remains mandatory. Use exact-source CI only when separately authorized.
Publication and ordinary public managed adoption remain distinct gates.

macOS-specific patch0020, builds, packages, toolchains, trials and maintenance
remain deferred. Preserve existing macOS identities/code/evidence without
relabeling old binaries. Existing unavailable native ARM, QSV, AMD and other
approved hardware debt remains explicitly unverified. Do not credit source
review, emulation, another GPU or upstream's results as native acceptance.

Reuse valid dependency images, build caches and evidence after identity checks;
build the necessary candidate once per configuration. Use Docker directly
from the host if needed; never mount its socket inside a container. Preserve
unrelated containers/processes and installations. An unavailable Docker engine
is an environment prerequisite, not permission to waive ARM acceptance or
start a broad alternative build service.

## Consequences

The fixed catch-up needs new dependency candidates rather than only application
edits. Builds and affected acceptance have a real cost. Keeping exact inputs
and one combined runtime update avoids repeated intermediate builds. Native
remains a first-class alternative; accepting this decision does not complete
any UCU row, waive a gate or authorize remote delivery.

## Alternatives Considered

- Retain the current runtimes without amendments: the two reproduced software
  failures and missing fallback contract contradict required acceptance.
- Adopt ErsatzTV-ffmpeg 8 or remove Native: conflicts with accepted FFmpeg 9,
  drawvg and two-engine requirements.
- Carry every old patch blindly or remove them after clean application:
  neither establishes equivalent behavior; regression-led retirement is required.
- Build every intermediate runtime: adds cost without proving the final fixed
  combination. Keep ordered row dispositions and test the cumulative candidate.

## References

- [Fixed scope and source maps](../../plans/upstream-catch-up.md)
- [Canonical milestone evidence](../progress/milestones.toml)
- [Temporary correction policy](0117-temporary-next-compatibility-corrections.md)
- [FFmpeg 9 and private MPP contract](0128-complete-ffmpeg-9-and-private-rockchip-runtime.md)
- [Local diagnostic results](0129-retain-local-troubleshooting-results.md)
- [Fixed Next release source](https://github.com/ErsatzTV/next/commit/c5cb065f9f35dbbcb7c05845f0f92c70f576a480)
- [Fixed upstream FFmpeg release source](https://github.com/ErsatzTV/ErsatzTV-ffmpeg/commit/83b9746e7e121b7a6bc4d7220bac94ec8b911ce5)
