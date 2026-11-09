# ADR 0061: Cue-aware burned text-subtitle seek rebase

## Status

Accepted (2026-08-03)

## Implementation

Implemented (PR #129, 2026-11-08) — the original cue-aware rewrite and the
ADR 0132 legacy encoding/cache correction are complete. Milestone
`subtitle-encoding` records local proof, exact-head CI and maintainer merge;
production installation and release acceptance are excluded.

Original component (2026-08-03) — source normalization plus in-process cue rewrite
for SRT/WebVTT/ASS, a narrow ffmpeg compatibility fallback, versioned
subtitle-seek cache directory, and focused Rust regression coverage
(straddling-cue cases per format, including a fixture directly modeling the
reported bug).

## Context

ErsatzRS's HLS on-demand segmenter (`crates/ersatzrs-infra/src/hls_worker.rs`,
`HLS_WORK_AHEAD_TRANSCODE_SECONDS`) restarts its ffmpeg transcode process
roughly every 44 seconds, re-seeking every input — video, audio, and any
burned-in subtitle — to the new absolute content position on each restart.

Before this ADR, burned **text** subtitles (SRT/ASS/WebVTT) were re-based for
each restart by shelling out to
`ffmpeg -i <subtitle> -ss <seek_seconds> -c:s copy -f <format> <out>`
(`crates/ersatzrs-infra/src/subtitles.rs`, `rebase_text_subtitle`). This is an
output-side seek combined with stream copy. FFmpeg decides whether to keep a
subtitle cue packet by comparing its **start** timestamp to the seek point; it
does not split a packet or retroactively include one that started before the
seek point even when its display duration extends past it. Net effect: any
cue that was already on screen when a ~44-second chunk boundary landed
mid-cue was **silently dropped entirely** from that chunk's rebased file,
instead of being clipped to show for its remaining visible duration.

A wholly synthetic external SubRip fixture reproduces the report at a
44-second HLS work-ahead boundary. The reproduction uses
`HttpLiveStreamingSegmenter` and confirms the drop happens in the text rebase
path itself, independently of track selection and PGS/image-subtitle handling.

This is **not** an ErsatzRS regression. ErsatzTV's own
`PipelineBuilderBase.Seek` (`$ERSATZTV_SRC/ErsatzTV.FFmpeg/Pipeline/PipelineBuilderBase.cs`)
uses the identical mechanism: `StreamSeekFilterOption`
(`$ERSATZTV_SRC/ErsatzTV.FFmpeg/GlobalOption/StreamSeekFilterOption.cs`) emits
`-ss` as an output/filter option (not an input option), combined with
`EncoderCopySubtitle` (`$ERSATZTV_SRC/ErsatzTV.FFmpeg/Encoder/EncoderCopySubtitle.cs`,
`-c:s copy`). ErsatzTV therefore very likely has the same limitation, and
there is no upstream fix to port.

## Decision

ADR 0132 extends this decision: encoding failures now refuse preparation;
FFmpeg syntax compatibility only receives normalized UTF-8, including at
position zero. Its content-bound cache replaces the historical path-only key.
The original decision below records the delivered cue-rewrite component.

Replace the ffmpeg copy-based rebase with an in-process, cue-aware rewrite in
`crates/ersatzrs-infra/src/subtitles.rs`:

- Fetch HTTP(S) sources and decode UTF-8 (with or without BOM) or BOM-marked
  UTF-16 in either byte order before parsing. Local sources use the same
  decoding path.
- Every cue's `start`/`end` is shifted by `-seek_seconds` (in the format's
  native sub-second unit — milliseconds for SRT/WebVTT, centiseconds for ASS,
  to avoid float rounding).
- A cue whose shifted `end <= 0` is dropped (fully in the past).
- A cue whose shifted `start < 0` but `end > 0` is **clamped** — start forced
  to `0`, kept — instead of dropped. This is the fix: the cue keeps
  displaying for its remaining visible duration in the new chunk.
- Otherwise the cue is kept with both timestamps shifted.

The rewrite is a surgical, line/field-oriented text transformation, not a
full parse-into-struct-and-reformat: SRT/WebVTT cue blocks recognize the
format's ASCII whitespace around `-->` (including tabs, while `WEBVTT` headers
and `NOTE`/`STYLE`/`REGION` blocks pass through untouched); ASS `Dialogue:`
lines are split with `splitn(field_count, ',')` using the section's own
`Format:` line to locate the `Start`/`End` fields (never a fixed column), so
the `Text` field's embedded commas (e.g. inside `\move(x1,y1,x2,y2)` override
tags) survive intact. No new crate dependency was added — `regex` is not used
anywhere else in `ersatzrs-infra`, and this crate's existing convention for
fixed-grammar timestamp parsing is hand-rolled `split`/`parse`
(`playback.rs::parse_time_of_day_seconds`,
`graphics_template.rs::parse_duration_seconds`); a full subtitle-parsing
crate was rejected because round-tripping ASS override tags and WebVTT cue
settings (`align:`/`position:`) byte-for-byte is safer via targeted rewriting
than parse-and-reformat.

Any line the rewriter cannot parse, and any input in which no cue can be
recognized, fails the whole file. Normalization or
parse failures fall back to the previous ffmpeg seek-and-copy rebase, retaining
support for FFmpeg-compatible syntax and encodings without handing absolute
cue timestamps to a zero-based playback chunk. If that compatibility pass also
fails, the original path is returned unchanged. A partial rewrite is never
emitted.

The subtitle-seek cache directory is versioned
(`subtitle_cache_folder().join("seek-v2")`, was `"seek"`) because the cache
key is a content offset, not wall-clock time: a looping channel would
otherwise keep re-serving pre-fix, cue-dropping cached files indefinitely.
The old `seek/` folder is simply orphaned, not actively cleaned up.

## Consequences

Fixes the intermittent missing-subtitle-line bug for burned text subtitles on
the HLS segmenter path. Canonical and normalized sources avoid one ffmpeg
subprocess spawn per ~44-second chunk per burned-text-subtitle session, while
unrecognized FFmpeg-compatible inputs retain the subprocess fallback. Callers
(`rebase_burned_text_subtitle_input` and its three call sites in
`hls_worker.rs`, `transport/iptv/stream.rs`, and `host/troubleshooting.rs`)
are unchanged — the function signature is identical.

Image-based (PGS/DVD/VOBSUB) subtitle burn-in has a related but distinct
mechanism (the decoder rides the same re-seeked video input and has no cross-
chunk packet history) and is **not** addressed by this ADR; it remains
undocumented/open behavior.

## Alternatives Considered

- Keep the ffmpeg copy pass but pre-widen the input-side seek and post-trim
  in a second pass. Rejected: still shells out twice per chunk, still fragile
  around packet-boundary semantics, and doesn't cleanly solve clamping a
  straddling cue's start.
- Adopt a Rust subtitle-parsing crate. Rejected: round-trip fidelity risk to
  ASS override tags and WebVTT cue settings on content this code doesn't
  need to touch.
- Do nothing / accept as an ErsatzTV parity limitation. Rejected — this is a
  confirmed, reproducible, user-facing correctness bug the maintainer asked
  to have fixed, and international subtitle correctness is an explicit
  high-priority parity surface for this project.

## References

- `crates/ersatzrs-infra/src/subtitles.rs`
- `crates/ersatzrs-infra/src/hls_worker.rs`
- `crates/ersatzrs-infra/src/file_system_layout.rs`
- `$ERSATZTV_SRC/ErsatzTV.FFmpeg/Pipeline/PipelineBuilderBase.cs`
- `$ERSATZTV_SRC/ErsatzTV.FFmpeg/GlobalOption/StreamSeekFilterOption.cs`
- `$ERSATZTV_SRC/ErsatzTV.FFmpeg/Encoder/EncoderCopySubtitle.cs`
