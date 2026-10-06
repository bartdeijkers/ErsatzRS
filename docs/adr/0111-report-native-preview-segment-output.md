# ADR 0111: Report native preview segment output

## Status

Accepted (2026-10-05) — implements the approved finding 05 correction.

## Implementation

Implemented — shared drawvg/Drawtext path escaping and native per-segment output
provenance have Windows/portable render, worker/ownership, real Chromium/Firefox
playback and matched-bundle proof. Canonical local acceptance and platform limits
live in `v111-findings-05`.

## Context

An FFmpeg failure can be replaced by a generated error slate. Successful video
decoding and HTTP delivery therefore do not prove that the programme is playing.
ErsatzTV's HlsSessionModel reports process/cursor state, while its HLS worker
transcodes an error message after a failed process. Neither exposes which
buffered segment belongs to that fallback. The encoder may already have moved
on while a browser still plays the slate.

## Decision

Keep the upstream machine session model unchanged. Expose read-only native
preview segment provenance through the existing core handle and application
runtime port. Capture each process's existing playlist URI set and classify only
newly generated URIs. Retain at most 256 reports, independent of process state.
Unknown engines, missing/retired segments and failed baseline reads remain
unknown; reads do not start, touch or cancel a worker.

Use the Hls.js playing fragment in the existing preview component to request
that exact segment's status. Match its URI and programme-date-time against the
served playlist so reused filenames after session replacement remain unknown.
Use the existing saved-anchor projection for that check: public playlists
rewrite FFmpeg's spawn timestamps. Status reads reuse its parser without
trimming files, normalizing media or advancing the anchor.
Show an always-mounted localized warning with the bounded redacted original
failure alongside decoder status. Poll failure is
visible and cannot claim healthy output. Component/player cleanup ends polling;
late responses for a different fragment, channel or worker are ignored. Native
HLS without fragment identity cannot infer successful programme output.

Share Drawtext's two-parser option escaping with generated drawvg paths, without
shell quoting. Normalize Windows separators and preserve literal Unix
backslashes. This extends the path-safety decision in ADR 0082.

## Consequences

The watch page and list preview distinguish a decoded slate from ordinary
output without changing playback ownership, scheduling or upstream session JSON.
The native diagnostic does not establish output provenance for the optional Next
engine. Unknown output remains explicitly unavailable rather than being labelled
successful. Existing capacity slates are also identified as replacement output.

## Alternatives considered

- Infer programme success from HTTP 200, decoder events or process state:
  these also succeed while playing a slate.
- Clear an error on the next successful encoder process: buffered slate segments
  can still be playing, and work-ahead failures can belong to future output.

## References

- [Finding 05 acceptance](../progress/milestones.toml)
- [Subtitle path safety](0082-escape-filtergraph-metacharacters-in-subtitle-paths.md)
- `$ERSATZTV_SRC/ErsatzTV.Application/Streaming/HlsSessionWorker.cs`
- `$ERSATZTV_SRC/ErsatzTV.Core/FFmpeg/HlsSessionModel.cs`
- `$ERSATZTV_SRC/ErsatzTV/Shared/ChannelPreviewDialog.razor`
- [FFmpeg filtergraph escaping](https://ffmpeg.org/ffmpeg-filters.html#Notes-on-filtergraph-escaping)
- [Hls.js playing fragment](https://hlsjs.video-dev.org/api-docs/hls.js.events)
