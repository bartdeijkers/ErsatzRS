# ADR 0015: HLS MPEG-TS continuity and PCR normalization

## Status

Accepted (2026-06-13).

## Implementation

Implemented — MPEG-TS HLS segmenter continuity normalization (2026-06-13),
extended with PCR normalization and wrapper PCR stabilization (2026-06-24),
then broadened to shared MPEG-TS mux timing and adaptation-only counter
normalization (2026-06-29).

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
3. Even after counters are normalized, separate MPEG-TS outputs can restart or
   jump PCR values at appended FFmpeg generation boundaries. FFmpeg's MPEG-TS
   muxer can also emit PCR with enough packet-position jitter to upset strict
   clients unless it is given a constant mux rate and explicit PCR period.
4. CBR MPEG-TS muxing can insert PCR-only adaptation packets on the PCR PID.
   Those packets do not carry payload and therefore do not advance the
   continuity counter; normalization must preserve that rule when rewriting
   retained segment files.

## Decision

Keep the ErsatzTV playlist contract and add an ErsatzRS transport hardening
step for MPEG-TS HLS output:

- The worker still appends FFmpeg-generated HLS playlists and preserves
  `#EXT-X-DISCONTINUITY` / discontinuity-sequence semantics.
- The worker rewrites served `#EXT-X-PROGRAM-DATE-TIME` values from its own
  monotonic playlist clock, matching ErsatzTV's `HlsPlaylistFilter`.
- Before a trimmed MPEG-TS HLS playlist is returned to clients, the worker
  normalizes continuity counters and PCR values in all retained `.ts` segment
  files in media sequence order. The first retained segment keeps its starting
  counters and PCR clock; later packets are rewritten per PID so PAT, SDT/BAT,
  PMT, video, audio counters, and PCR remain continuous across appended FFmpeg
  process boundaries. Adaptation-only packets keep the previous payload
  counter while preserving the next payload counter advance.
- Direct MPEG-TS output, legacy concat, MPEG-TS HLS segments, the built-in
  MPEG-TS hybrid wrapper, and the bundled default wrapper script use a
  channel-profile-derived constant mux rate plus a 20 ms PCR period for their
  MPEG-TS muxers. This intentionally keeps ErsatzTV's command shape while
  stabilizing PCR emission for strict MPEG-TS clients.
- fMP4 HLS output is not touched by this rule because it does not use MPEG-TS
  continuity counters or PCR.

This is a deliberate improvement over the pinned ErsatzTV baseline where raw
MPEG-TS continuity can reset at playlist discontinuities.

## Consequences

Parallel live HLS playback no longer depends on each client tolerating
transport-counter or PCR resets at appended process boundaries. TSDuck's HLS
reader can analyze the full concatenated live stream without continuity misses
or PCR leaps on the table, audio, or video PIDs, and players that are stricter
than the playlist contract get a stable payload stream.

The worker now mutates generated `.ts` segment files before advertising them
through the served playlist. This adds a small file rewrite cost for MPEG-TS
HLS segments and requires the continuity/PCR-normalization state to stay scoped
to the live session. The operation is intentionally tied to playlist trimming so
clients do not fetch newly retained segments before they are normalized.
Profile-derived CBR muxing adds null-packet stuffing relative to VBR output;
the mux rate uses the configured channel profile bitrate with the same 1.2
overhead used for the HLS variant playlist bandwidth, capped by the shared
MPEG-TS muxing helper.

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
- Normalize counters/PCR only in verification tooling: rejected because it
  would hide the transport defect from tests while leaving strict clients
  exposed.
- Leave the wrapper remux VBR because ErsatzTV does: rejected because live
  TSDuck proof on FFmpeg 8.1 showed wrapper PCR leaps while the segmenter HLS
  capture was already clean.

## References

- [`docs/progress/parity/hls-variant-playlists-segmenter.toml`](../progress/parity/hls-variant-playlists-segmenter.toml)
- [ADR 0003: Invoke FFmpeg through the CLI](0003-ffmpeg-cli-spawn.md)
- [ADR 0013: TSDuck MPEG troubleshooting](0013-tsduck-mpeg-troubleshooting.md)
- ErsatzTV `HlsPlaylistFilter`, `HlsSessionWorker`, and `OutputFormatHls` at
  parity baseline `0d301df5e8512fdfccf3a611188d27ab1690ee31`
