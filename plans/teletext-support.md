# Teletext support plan

## Status

Queued and not implemented. The 2026-10-07 planning refresh retains the five
teletext milestones and the blocking `p9a-01` product/standards ADR. Implementation
starts after `ucu-09` and the other recorded prerequisites are Complete. UCU-09
has a fixed upstream v26.11.1 boundary; later upstream releases do not extend
that gate. This refresh updates planning, not product scope, schemas, APIs, or
runtime capability. The authoritative execution state lives in
[`docs/progress/milestones.toml`](../docs/progress/milestones.toml) and
[`docs/progress/parity/teletext-support.toml`](../docs/progress/parity/teletext-support.toml);
this plan owns future sequencing and acceptance boundaries.

## Goal

Add deliberate, accessible teletext support without conflating three different
products: DVB teletext subtitle pages embedded in source transport streams,
interactive full-page teletext browsing, and an authored teletext service
carried in ErsatzRS output. Scan and persist the accepted source metadata,
provide predictable selection and playback behavior, and expose capability or
failure through health and troubleshooting on every supported platform.

## Current evidence

- ErsatzRS currently has no teletext-specific domain model, scanner branch,
  selection policy, FFmpeg capability requirement, admin surface, or runtime
  proof.
- Embedded subtitle extraction recognizes SubRip, ASS, WebVTT and MOV text and
  distinguishes selected bitmap codecs, but does not classify or decode
  `dvb_teletext`.
- The first-party FFmpeg 9 runtime contract does not currently require or probe
  `libzvbi_teletextdec` or the `--enable-libzvbi` build flag.
- FFmpeg documents DVB teletext as demux/decode support supplied by the optional
  libzvbi dependency. The decoder can select subtitle pages or explicit page
  numbers and can produce bitmap or text/ASS output. FFmpeg does not document a
  DVB teletext encoder, so decoding support must not be presented as proof that
  ErsatzRS can author a teletext service.
- Native FFmpeg and Next are now integrated engines. Next publishes selected
  tracks and subtitle probe hints, but its current host wire types carry no
  teletext page or decoder options. Native support cannot establish Next support.
- [ADR 0095](../docs/adr/0095-native-pipeline-instead-of-ersatztv-next-engine.md)
  conditionally accepts Next as the new-channel and standalone-preview default
  only with automatic verified runtime availability. That acquisition/default
  implementation is still a UCU-09 candidate at this planning checkpoint.
- Existing selection and preparation include exact media versions, provider
  stream identity, bounded Next subtitle artifacts, and edited-source subtitle
  timing. Reuse those boundaries rather than adding a parallel extraction path.
- The current managed FFmpeg pin is v2.0.6. P9A-02 must inspect the actual pinned
  FFmpeg and Next artifacts at implementation time; version or package presence
  alone does not prove teletext capability.
- The 2026-10-07 search of the local ErsatzTV checkout at `df6ae228` found no
  teletext-specific policy, as in the initial 2026-08-07 search. P9A-01 must
  repeat the source audit at the completed UCU baseline and classify each
  accepted behavior as parity or an ErsatzRS extension.

## Non-goals before the Phase 9 ADR

- Do not assume that “teletext support” means subtitles only.
- Do not promise full-page authoring or MPEG-TS teletext carriage from decoder
  availability.
- Do not treat DVB bitmap subtitles, closed captions, ordinary text subtitles,
  and DVB teletext as interchangeable codecs.
- Do not make an optional external FFmpeg library a startup requirement for
  users who have no teletext media until the packaging decision is accepted.
- Do not expose raw page bytes or undecoded character-set data through public
  APIs as a temporary contract.

## Phase 9A: Contract and ingest

### P9A-01: Product and standards decision

Accept an ADR before schema, runtime-package, or pipeline work. It must decide:

- whether Phase 9 supports subtitle pages, full-page browsing, authored channel
  pages, or a staged subset;
- page-number, magazine/service, subtitle flag, language, national-option/
  default-region, character-set, transparency, and duration semantics;
- whether source teletext is burned into video, extracted to a cached text/ASS
  derivative, carried or stream-copied in MPEG-TS, exposed as a browser sidecar,
  or rejected per output mode;
- how channel, selected-item, global language, forced/default, and accessibility
  preferences interact with multiple teletext pages;
- HLS behavior, because browser HLS clients do not gain teletext-page support
  merely because an MPEG-TS input contains teletext;
- whether full-page browsing needs a dedicated authenticated endpoint and UI,
  and whether generated pages need a separate authoring/data model;
- the six-RID libzvbi packaging, licensing, capability, and operator-runtime
  fallback contract; and
- health behavior for unsupported pages, missing decoder capability, malformed
  streams, and character-set failures.

Use ETS 300 706/EN 300 706 terminology where it applies and ground output
carriage decisions in MPEG-TS/client evidence. Record unsupported combinations
explicitly instead of allowing FFmpeg's automatic stream mapping to decide.

The ADR must fill an engine/source/output matrix using these independent axes:

| Axis | Cases requiring a disposition |
|---|---|
| Engine | Native FFmpeg; Next, including its Burn and Convert modes |
| Source | Reopenable local files; authenticated provider/finite remote sources; prepared add-on files; one-shot producers; live HTTP/RTSP |
| Output | Native MPEG-TS and HLS modes; Next HLS and hybrid MPEG-TS; browser/client delivery |
| Video processing | Transcode/burn-in; video-copy policy and its effect on selected captions |
| Teletext behavior | Subtitle pages; full-page browsing; authored services, as selected by the ADR |

For each accepted combination, identify page selection, decoder/preparation
ownership, delivery, failure behavior and required runtime proof; explicitly
reject or mark inapplicable the remaining combinations. Determine whether the
pinned Next protocol can express the selection or needs an accepted adapter or
protocol change before promising support. Preserve [ADR 0100](../docs/adr/0100-next-alpha-embedded-text-subtitle-limits.md):
never silently discard an exact selection, switch engines, consume a one-shot
producer twice, or await whole-source extraction from an endless stream. Existing
alpha limitations are constraints to evaluate, not proof of teletext behavior.

### P9A-02: Runtime packaging and capability

Build or acquire a small redistributable teletext fixture before changing the
managed runtime. For every first-party RID:

1. establish whether libzvbi can be built, linked, licensed, and packaged;
2. verify `ffmpeg -decoders` exposes `libzvbi_teletextdec` and the accepted
   decoder options;
3. add a bounded functional decode probe rather than checking the name alone;
4. record operator-runtime capability without rejecting unrelated playback;
5. surface a missing required capability when a saved channel actually selects
   teletext; and
6. verify runtime archives and container images contain any required shared
   libraries or remain genuinely static.

Use the existing managed-runtime acquisition and capability-reporting paths,
recording exact artifact identity separately from application decode evidence.
UCU's scoped ARM/macOS and hardware acceptance exceptions do not waive Phase 9
requirements; any teletext platform exception needs its own accepted decision.

### P9A-03: Scan and persistence

Persist a typed teletext stream/page model rather than overloading the existing
text/image-subtitle heuristic. At minimum carry the ADR-approved stream index,
codec, service/page number, subtitle flag/type, language, region/character set,
forced/default disposition, and source provenance.

Bind page identity to the exact source revision, media version and stream. Pages
within one stream must remain independently selectable and must not collide in
cache identity. Preserve provider subtitle ownership under
[ADR 0101](../docs/adr/0101-provider-subtitle-stream-identity.md); a provider path
is metadata, not sufficient identity. Extend existing reconciliation and
preparation contracts to invalidate stale page selections and artifacts without
replacing unrelated tracks.

Scanner work covers local and supported remote providers, exact media-version
selection, rescan reconciliation, unavailable files, SQLite persistence, and
search/detail projections. Preserve alternate-audio and ordinary subtitle
behavior while adding teletext; a codec that cannot be decoded must remain
visible rather than disappear from selection.

## Phase 9B: Delivery and runtime proof

### P9B-01: Selection and output

Thread exact stream/page selection through collection/schedule entries and the
native and Next adapters. Implement the P9A-01 matrix through the existing
application ports, selection rules and source owners:

- burn-in uses explicit decoder page/format/region options and preserves
  transparency, timing, seek/restart, scaling, and HDR/SDR behavior;
- extraction reuses bounded, source-bound preparation with page-aware cache keys,
  deadlines, cancellation and atomic publication; failure follows the accepted
  selection policy without admitting partial or stale artifacts;
- MPEG-TS passthrough or carriage is enabled only after PMT/PID and client proof;
- HLS/browser playback uses the accepted burn-in or sidecar fallback and never
  advertises native teletext controls that do not exist; and
- full-page browsing or authored pages, if accepted, use a separate stable
  application/API boundary rather than scraping FFmpeg logs or cached files.

Preserve subtitle timing through retained source intervals, mid-cue seeks,
programme handoffs and chunk restarts. Define video-copy behavior explicitly;
copying video must not accidentally promise burned-in captions or carry a
teletext stream solely through automatic mapping.

If authored pages are accepted, reuse the read-only programme occurrence
projection from [ADR 0112](../docs/adr/0112-programme-aware-graphics-context.md)
for current/next/continuing programmes and Later Today, including filler and
DST boundaries. Reuse the native preview boundary from
[ADR 0114](../docs/adr/0114-author-graphics-with-the-native-renderer.md) where
appropriate. A rendered page preview does not prove an encoded teletext service;
the ADR must still select that service's authoring and transport contract.

### P9B-02: Admin, health, and end-to-end acceptance

Localize teletext type, page/service, language/region, selection, capability,
and error presentation in all supported cultures (currently nine). Extend the
existing troubleshooting and health surfaces for selected-but-undecodable streams,
invalid page metadata, missing runtime support, extraction/decode failures, and
unsupported output combinations.

Keep browser actions on the management surface, machine operations under the
existing API policy, and decoder/media callbacks on the internal listener under
[ADRs 0102](../docs/adr/0102-isolate-http-listener-route-surfaces.md) and
[0103](../docs/adr/0103-separate-machine-api-from-browser-actions.md). Any browsing
or sidecar endpoint needs an explicit route and access-policy classification;
do not expose management through the streaming-only listener.

Use licensed or redistributable fixtures for subtitle pages and, if accepted,
full-page services. Runtime acceptance covers scan/rescan, exact selection,
seek and chunk restart, mixed subtitle types, direct MPEG-TS, HLS fallback,
HDHomeRun clients where applicable, malformed input, runtime refresh, and
decoded visual/text comparison. Include multiple pages in one stream,
multilingual character sets, preserved alternate audio, source replacement,
stale cache rejection, cancellation, edited timelines, and native/Next copy-mode
and unsupported-selection outcomes from the accepted matrix. Prove the served
output, not only serialized selection or successful process startup. TSDuck
must verify PSI/PID structure whenever the output carries a separate teletext
or DVB-subtitle stream.

## Delivery order

1. `p9a-01`: after UCU-09 closes, revalidate the source baseline and accept the
   complete product/standards/runtime contract and engine/source/output matrix.
2. `p9a-02`: package and functionally probe the decoder.
3. `p9a-03`: persist source services/pages and expose exact metadata.
4. `p9b-01`: implement only the accepted selection/output matrix.
5. `p9b-02`: close localized management, health, and end-to-end proof.

Phase 10 video-jukebox work remains dependency-gated behind `p9b-02`.

## Closeout gates

Each slice updates the owning parity debt, health contract, changelog, and
session log. Phase 9 closes only after:

- runtime packaging/probing is verified for all six RIDs or an ADR records an
  intentional supported-platform exception;
- SQLite scanner and persistence tests pass;
- FFmpeg argv snapshots and decoded subtitle/page fixtures pass;
- changed admin surfaces are localized and hydration/browser-proven;
- `./run.sh test-closeout`, the workspace Clippy configuration,
  `cargo leptos build`, and `cargo xtask parity-check` pass;
- `cargo xtask smoke` and the TSDuck playback gate cover every changed output
  mode; and
- `cargo xtask progress check` reports no tracker drift.

## References

- [FFmpeg codec documentation: libzvbi teletext](https://ffmpeg.org/ffmpeg-codecs.html#libzvbi_002dteletext)
- [FFmpeg supported subtitle formats](https://ffmpeg.org/general.html#Subtitle-Formats)
- [FFmpeg 9 performance and support progress](../docs/progress/parity/ffmpeg-9-performance-and-support.toml)
- [ADR 0003: Invoke FFmpeg through the CLI](../docs/adr/0003-ffmpeg-cli-spawn.md)
- [ADR 0052: Global subtitle language preferences](../docs/adr/0052-global-subtitle-language-preferences.md)
- [ADR 0056: Exact media-version and stream selections](../docs/adr/0056-exact-media-version-and-stream-selection.md)
