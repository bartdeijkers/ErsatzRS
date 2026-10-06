# ADR 0100: Preserve official Next with explicit embedded text subtitle limits

## Status

Accepted 2026-09-23

The maintainer selected the official Next build and explicitly accepted the
embedded text subtitle cases described below as temporary alpha limitations.

Packaging amendment accepted 2026-10-01: build ARMv7 Next from the pinned,
unmodified upstream source because no official ARMv7 executable is published.

## Implementation

Partial — the refusal contract is implemented: unavailable selections are
refused during Next source projection or producer preparation. Row 39 owns
explicit failure evidence, supported subtitle paths, routed lifecycle proof and
Next/native smoke acceptance. The underlying embedded text capabilities remain
open parity debt; full window and supported-platform acceptance are separate.

## Context

ADR 0095 adds Next as an optional second engine while preserving operator
selection, source ownership and subtitle/audio behavior. The bundled official
Next revision is `f2a981b`. A 2026-09-23 comparison with official main
`f6350a0ad9077290c0fbad7ea3feacaef71f0ca3` finds only a canvas overlay conversion
change, with no correction to the following text subtitle contracts:

- Next Burn omits embedded text from a multiplexed input and requires a
  scheduler-extracted sidecar. Finite reopenable sources can use the existing
  source-bound extraction path; a live source cannot use whole-source extraction.
- Next Convert rejects RTSP inputs and waits for HTTP subtitle extraction to
  reach EOF before starting playback. An endless source cannot satisfy that
  preparation contract.
- An ordinary installed producer is prepared once and exposes one claimed byte
  stream. Giving that URI to independent subtitle extraction and then video
  playback would consume it twice. Reopening the producer or treating a legacy
  item/index cache as source/version-bound evidence would violate existing
  playback ownership and selection guarantees.

These limitations differ from image subtitles, for which Next has an overlay
path, and from finite external text sidecars, which can be prepared independently
of the one-shot audio/video producer.

### Upstream correction audit

Before committing this ADR, the maintainer requested a check for an existing
upstream correction. Both live remote main refs were verified on 2026-09-23:
Next `f6350a0ad9077290c0fbad7ea3feacaef71f0ca3` and ErsatzTV legacy
`27f9d2174f377e27baa553872c73fcac863a52d7`. The inspected corrections are:

- `0036fc5c` (#2938) adds selected subtitle codec/index probe hints. It does not
  add a live text decoder or extract from a one-shot producer.
- `4ffd775a` (#2960) passes already-extracted caption paths to Next, preserves
  sidecar in/out points, resynchronizes playout after extraction, and corrects
  subtitle-relative extraction indexes. Its converter explicitly skips embedded
  text that has not been extracted. These are relevant fixes for finite sidecar
  playback, not a solution for the unavailable cases in this decision.
- `c673082e` (#3014) fixes image selection without audio, sidecar routing,
  availability and timing. It still requires extracted embedded text. At the
  verified legacy tip, the extraction handler's subtitle dispatch covers
  Episode/Movie/MusicVideo/OtherVideo, with no RemoteStream extraction branch.
- `06ee45ff` (#2994) repairs a subtitle seek query delimiter in the native
  FFmpeg service. `62b7a5e9` (#3037) optimizes the legacy graphics subtitle
  renderer; neither changes Next's embedded text preparation.
- Next's sole commit after the bundled revision, `f6350a0` (#250), changes
  canvas overlay conversion. At that exact tip, embedded text Burn still
  removes the selection, RTSP Convert still returns an error, and HTTP Convert
  still waits for the extraction process without a source-duration bound.

No inspected correction removes the specific limitations accepted here. The
other subtitle improvements remain subject to normal ordered catch-up review;
this audit does not process their ledger rows or advance the upstream pin.

## Decision

Keep the official Next artifact on platforms where it is published. Accept the
following unresolved capabilities as visible upstream alpha limitations for
row 39:

1. Embedded text subtitles in live Next inputs where Burn or Convert cannot
   honor the selected captions under the official engine contract.
2. Embedded text selected from an ordinary one-shot producer without an
   independently verified, source-bound prepared subtitle artifact.

Refuse these unavailable selections explicitly. Do not silently remove the
selected subtitle, select another track, switch engines, reopen a producer, or
claim that wire-level selection proves rendered captions. Keep the limitation
visible in parity debt and playback diagnostics. Operators can explicitly select
the native engine or provide a supported prepared artifact/sidecar where their
source supports it; there is no automatic fallback.

This exception does not waive image subtitle support, external sidecar timing,
prepared artifact captions, retained-interval subtitle mapping, alternate audio,
source cancellation, HTTP lifecycle checks or supported-platform acceptance.
Those remain part of the existing requirements.

### ARMv7 packaging exception

For ARMv7, build upstream revision
`4ad282fae7f49d488aa40e5b6bb092dbe9c4018f` without source or lockfile changes.
Verify the source archive and lockfile checksums before building with Rust
1.97.1 and a Debian Bookworm ARM hard-float compiler. Require the executable
to remain compatible with GLIBC 2.36. Retain its license, source revision,
compiler identity, build command and payload hashes in `origin.json`.

The recipe produces the accepted ARMv7 artifact; release archives and Docker
images reuse that checksum-pinned binary. Packaging
must identify this as a source build rather than an official downloaded asset.
Other platforms continue using their verified official artifacts. A new source
revision requires the existing source review, runtime acceptance and CI gates.
Emulated startup and decoded playback checks are distinct from the approved,
still-unverified native ARM application and hardware acceptance.

### Preserving accepted dependency artifacts

The maintainer approved the immutable `next-4ad282f` dependency snapshot on
2026-10-01 after upstream deleted the accepted `develop` downloads. Preserve
all five official archives byte-for-byte from upstream CI 36832767332, alongside
the accepted ARMv7 build, checksums, license and provenance. Pin archive and
license hashes in the staging code. Private-repository access uses the existing
host GitHub CLI login or CI token; Docker consumes host-staged files without
credentials. Do not replace this snapshot when upstream advances.

## Consequences

The official engine remains reproducible across packaging, Docker and CI without
a private patch series or an additional subtitle tee/extraction architecture.
Affected selections remain unavailable instead of producing misleading success.
Keep the debt open until an upstream correction or a separately accepted adapter
provides rendered, timed captions while preserving the existing ownership
contract. Reevaluate the limitation when adopting relevant newer Next behavior.

## Alternatives Considered

- Patch Next or add a bounded subtitle adapter now: not selected by the
  maintainer for these alpha limitations.
- Consume the prepared stream for extraction and reopen it for playback:
  rejected because the registered transfer and producer are single-use.
- Reuse legacy extracted captions solely by item/index: rejected because those
  keys do not prove the current source/version/publication identity.
- Silently omit captions or switch to the native engine: rejected because it
  conceals a failed operator selection.

## References

- [ADR 0095](0095-native-pipeline-instead-of-ersatztv-next-engine.md)
- [Upstream catch-up plan](../../plans/upstream-catch-up.md)
- [Owning parity record](../progress/parity/upstream-baseline-catch-up.toml)
- [Official Next comparison](https://github.com/ErsatzTV/next/compare/f2a981b...f6350a0ad9077290c0fbad7ea3feacaef71f0ca3)
- [Official session subtitle handling](https://github.com/ErsatzTV/next/blob/f2a981b/crates/ersatztv-channel/src/channel_session.rs)
- [Official text conversion](https://github.com/ErsatzTV/next/blob/f2a981b/crates/ffpipeline/src/web_vtt.rs)
- [Subtitle probe hints, #2938](https://github.com/ErsatzTV/legacy/commit/0036fc5cc81429008ef7136efc14b88ec018e561)
- [Extracted text sidecars, #2960](https://github.com/ErsatzTV/legacy/commit/4ffd775ae79ffd319d29bf170b96c2dcf404222f)
- [Subtitle converter corrections, #3014](https://github.com/ErsatzTV/legacy/commit/c673082ed844d46ed9adf73fac52db4fba953883)
- [Current legacy subtitle extraction](https://github.com/ErsatzTV/legacy/blob/27f9d2174f377e27baa553872c73fcac863a52d7/ErsatzTV.Application/Subtitles/Commands/ExtractEmbeddedSubtitlesHandlerBase.cs)
