# ADR 0015: HLS MPEG-TS continuity normalization

## Status

Accepted (2026-06-13).

## Implementation

Implemented — MPEG-TS HLS segmenter continuity normalization (2026-06-13).

## Context

ErsatzTV appends successive HLS segmenter FFmpeg processes into one live media
playlist using `append_list`, `discont_start`, and MPEG-TS segment options such
as `mpegts_flags=+initial_discontinuity`. Its playlist filter trims old
segments and inserts `#EXT-X-DISCONTINUITY` between appended generations.

That shape is good HLS behavior, but it is not enough for every raw
transport-stream client. In ErsatzRS live proof on 2026-06-13, parallel clients
on MTV channel 2 showed a client-visible freeze: VLC stopped progressing while
the server playlist continued to advance. TSDuck reading the same
`/iptv/channel/2.m3u8?mode=segmenter` stream reported continuity-counter
discontinuities on PAT, SDT/BAT, PMT, video, and audio PIDs at the exact first
packets after appended HLS discontinuity boundaries.

Deep review showed two independent runtime issues:

1. Program-date-time values must be synthesized from the worker playlist start
   and accumulated `#EXTINF` durations, matching ErsatzTV's playlist filter,
   instead of trusting FFmpeg's per-process wall-clock values.
2. Separate FFmpeg HLS processes reset MPEG-TS continuity counters. ErsatzTV's
   playlist discontinuity markers do not make the raw TS counter stream
   continuous for tools and players that validate the concatenated segment
   payloads.

## Decision

Keep the ErsatzTV playlist contract and add an ErsatzRS transport hardening
step for MPEG-TS HLS output:

- The worker still appends FFmpeg-generated HLS playlists and preserves
  `#EXT-X-DISCONTINUITY` / discontinuity-sequence semantics.
- The worker rewrites served `#EXT-X-PROGRAM-DATE-TIME` values from its own
  monotonic playlist clock, matching ErsatzTV's `HlsPlaylistFilter`.
- Before a trimmed MPEG-TS HLS playlist is returned to clients, the worker
  normalizes continuity counters in all retained `.ts` segment files in media
  sequence order. The first retained segment keeps its starting counters; later
  packets are rewritten per PID so PAT, SDT/BAT, PMT, video, and audio counters
  remain continuous across appended FFmpeg process boundaries.
- fMP4 HLS output is not touched by this rule because it does not use MPEG-TS
  continuity counters.

This is a deliberate improvement over the pinned ErsatzTV baseline where raw
MPEG-TS continuity can reset at playlist discontinuities.

## Consequences

Parallel live HLS playback no longer depends on each client tolerating
transport-counter resets at appended process boundaries. TSDuck's HLS reader
can analyze the full concatenated live stream without continuity misses on the
table, audio, or video PIDs, and players that are stricter than the playlist
contract get a stable payload stream.

The worker now mutates generated `.ts` segment files before advertising them
through the served playlist. This adds a small file rewrite cost for MPEG-TS
HLS segments and requires the continuity-normalization state to stay scoped to
the live session. The operation is intentionally tied to playlist trimming so
clients do not fetch newly retained segments before they are normalized.

## Alternatives Considered

- Match ErsatzTV exactly and rely only on `#EXT-X-DISCONTINUITY`: rejected
  because the live proof reproduced client-visible freeze risk and TSDuck
  continuity misses at the exact appended-process boundaries.
- Remove discontinuity markers after normalizing counters: rejected because
  timestamps, codec state, and media item boundaries still legitimately change
  across appended generations.
- Keep one long-running FFmpeg process for the whole channel: rejected because
  the ErsatzTV-compatible segmenter state machine intentionally starts bounded
  FFmpeg processes per item, slug, or fallback generation.
- Normalize counters only in verification tooling: rejected because it would
  hide the transport defect from tests while leaving strict clients exposed.

## References

- [`docs/progress/parity/hls-variant-playlists-segmenter.toml`](../progress/parity/hls-variant-playlists-segmenter.toml)
- [ADR 0003: Invoke FFmpeg through the CLI](0003-ffmpeg-cli-spawn.md)
- [ADR 0013: TSDuck MPEG troubleshooting](0013-tsduck-mpeg-troubleshooting.md)
- ErsatzTV `HlsPlaylistFilter`, `HlsSessionWorker`, and `OutputFormatHls` at
  parity baseline `0d301df5e8512fdfccf3a611188d27ab1690ee31`
