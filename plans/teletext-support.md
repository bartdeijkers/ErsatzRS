# Teletext support plan

## Status

Active and not implemented. Phase 8's required milestones completed on
2026-08-07, so this is now the live phase; it starts with the blocking `p9a-01`
product/standards ADR. The authoritative execution state lives in
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
- Embedded subtitle extraction recognizes SubRip, ASS, WebVTT, MOV text, and
  selected bitmap codecs, but does not classify or decode `dvb_teletext`.
- The first-party FFmpeg 9 runtime contract does not currently require or probe
  `libzvbi_teletextdec` or the `--enable-libzvbi` build flag.
- FFmpeg documents DVB teletext as demux/decode support supplied by the optional
  libzvbi dependency. The decoder can select subtitle pages or explicit page
  numbers and can produce bitmap or text/ASS output. FFmpeg does not document a
  DVB teletext encoder, so decoding support must not be presented as proof that
  ErsatzRS can author a teletext service.
- A 2026-08-07 workspace search found no pinned ErsatzTV teletext policy. Phase
  9A must repeat that baseline check before deciding whether each slice is
  parity or an ErsatzRS extension.

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

### P9A-03: Scan and persistence

Persist a typed teletext stream/page model rather than overloading the existing
text/image-subtitle heuristic. At minimum carry the ADR-approved stream index,
codec, service/page number, subtitle flag/type, language, region/character set,
forced/default disposition, and source provenance.

Scanner work covers local and supported remote providers, exact media-version
selection, rescan reconciliation, unavailable files, SQLite/MySQL parity, and
search/detail projections. Preserve alternate-audio and ordinary subtitle
behavior while adding teletext; a codec that cannot be decoded must remain
visible rather than disappear from selection.

## Phase 9B: Delivery and runtime proof

### P9B-01: Selection and output

Thread exact stream/page selection through collection/schedule entries and both
IPTV pipeline families. Implement only the output combinations accepted by the
ADR:

- burn-in uses explicit decoder page/format/region options and preserves
  transparency, timing, seek/restart, scaling, and HDR/SDR behavior;
- extraction creates a typed cached derivative with atomic failure fallback;
- MPEG-TS passthrough or carriage is enabled only after PMT/PID and client proof;
- HLS/browser playback uses the accepted burn-in or sidecar fallback and never
  advertises native teletext controls that do not exist; and
- full-page browsing or authored pages, if accepted, use a separate stable
  application/API boundary rather than scraping FFmpeg logs or cached files.

### P9B-02: Admin, health, and end-to-end acceptance

Localize teletext type, page/service, language/region, selection, capability,
and error presentation. Add health checks for selected-but-undecodable streams,
invalid page metadata, missing runtime support, extraction/decode failures, and
unsupported output combinations.

Use licensed or redistributable fixtures for subtitle pages and, if accepted,
full-page services. Runtime acceptance covers scan/rescan, exact selection,
seek and chunk restart, mixed subtitle types, direct MPEG-TS, HLS fallback,
HDHomeRun clients where applicable, malformed input, runtime refresh, and
decoded visual/text comparison. TSDuck should verify PSI/PID structure whenever
the output carries a separate teletext or DVB-subtitle stream.

## Delivery order

1. `p9a-01`: accept the complete product/standards/runtime contract.
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
- SQLite/MySQL scanner and persistence tests pass;
- FFmpeg argv snapshots and decoded subtitle/page fixtures pass;
- changed admin surfaces are localized and hydration/browser-proven;
- `./run.sh test-closeout`, both workspace Clippy configurations,
  `cargo leptos build`, and `cargo xtask parity-check` pass;
- `cargo xtask smoke` and the TSDuck playback gate cover every changed output
  mode; and
- `cargo xtask progress check` reports no tracker drift.

## References

- [FFmpeg codec documentation: libzvbi teletext](https://ffmpeg.org/ffmpeg-codecs.html#libzvbi_002dteletext)
- [FFmpeg supported subtitle formats](https://ffmpeg.org/general.html#Subtitle-Formats)
- [FFmpeg 9 HDR, Dolby Vision, and pipeline hardening plan](ffmpeg-9-hdr-dolby-vision-and-pipeline-hardening.md)
- [ADR 0003: Invoke FFmpeg through the CLI](../docs/adr/0003-ffmpeg-cli-spawn.md)
- [ADR 0052: Global subtitle language preferences](../docs/adr/0052-global-subtitle-language-preferences.md)
- [ADR 0056: Exact media-version and stream selections](../docs/adr/0056-exact-media-version-and-stream-selection.md)
