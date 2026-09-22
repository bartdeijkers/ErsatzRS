# ADR 0088: Prefer native yt-dlp transfer and capabilities

## Status

Accepted 2026-09-06. Supersedes only ADR 0087's custom HTTP Range adapter
decision; its product requirements and acceptance gates remain unchanged.

## Implementation

Implemented for M-01A — the unpublished resolver/Range adapter is removed and
replaced by the v4 add-on-owned retained-media operation. The agreed scheduled
MPEG-TS/HLS gate, bounded buffering/cancellation and source-cut correspondence
are verified after the NUT identifier and retained-video timestamp fixes.
Canonical acceptance and evidence are in the
[M-01A tracker](../progress/milestones.toml). This does not claim later delivery
modes, subtitle acquisition or release/platform integration are complete.

## Context

The official add-on already relies on regularly updated yt-dlp provider and
transfer support. Returning signed CDN URLs to a second downloader duplicates
that responsibility. Prior adapter work is not a reason to retain it.

A plain stdout pipe is not seekable. Transferring every excluded byte before
discarding it failed the long-exclusion bandwidth experiment. Independently
encoding each retained section introduced audio boundary errors.

## Decision

Across all remaining AMM slices, prefer native capabilities of the shipped
yt-dlp executable for the yt-dlp add-on. Keep package code focused on invocation,
contract mapping and process orchestration. Before adding a custom substitute,
demonstrate the native limitation against the approved requirement and document
the smallest necessary exception. The Rust host remains provider-neutral.

Remove the unpublished resolve-only operation and custom HTTP Range adapter,
including its private routes, registrations and retained-range concat files.
Do not retain an adapter fallback. Preserve frozen interval plans, edited
durations, publication, scheduling, bounded output reserve and cancellation,
along with unrelated download-ahead groundwork.

The replacement v4 operation accepts bounded frozen source intervals over
stdin. The add-on uses native yt-dlp extraction once and native section
transfers with its FFmpeg downloader. FFmpeg decodes each retained section into
raw video and PCM in NUT with cumulative edited timestamps; the host performs
the final encoding once. A scheduled join is mapped before launching this
operation, never implemented as seeking an ordinary pipe.

One bounded container-handoff exception is necessary: native FFmpeg writes a
25-byte NUT file identifier at the start of every section and offers no option
to omit it. At a later section boundary, the continuous demuxer interprets that
identifier as frame data. A same-parameter local reproduction produces bogus
timestamps and fails; removing only the repeated identifier restores the exact
frame/sample counts. The add-on therefore validates the fixed identifier at
each known process boundary, preserves the first and drops subsequent copies,
then forwards all remaining bytes unchanged with stream backpressure. This is
not a provider transfer implementation, arbitrary byte scanner or NUT parser.
Independent sections must still have compatible native stream parameters;
identifier removal does not establish support for parameter changes.

Probe a bounded header prefix from the same running producer, replay it once
into the host encoder and retain managed process ownership through cancellation.
Private extracted metadata is attempt-owned, not persisted playback state.

## Consequences

Provider redirects and transfer behavior stay with yt-dlp and its downloader.
No mandatory full-media download, custom CDN transport, or second lossy
encoding layer is introduced. Raw intermediate media requires more local pipe
bandwidth and decoding resources; startup and sustained throughput need proof.
The current SDR path refuses known HDR and unmapped selected subtitle clocks;
it must not silently flatten or discard them. Those refusals are not a claim of
HDR or subtitle support; later slices retain their own acceptance requirements.

The ten-second playable-start target, revised twenty-second maximum and
uninterrupted cuts apply. M-01A cannot complete without the specified real add-on
video in scheduled MPEG-TS and HLS. Synthetic section accuracy is not that gate.

## Alternatives Considered

- Custom Range adapter: rejected by the maintainer; duplicates transfer work.
- Continuous-span transfer: exact cuts, but long discarded spans missed the
  approved buffering bound under the measured bandwidth fixture.
- Independently encoded section stdout: not accepted as seamless audio proof.
- Download everything first: reserved for optional M-01B, not streaming.

## References

- [ADR 0087](0087-stream-sponsorblock-intervals-without-playback-underruns.md)
- [AMM delivery acceptance and evidence](../progress/milestones.toml)
- [yt-dlp downloader implementation](https://github.com/yt-dlp/yt-dlp/blob/2026.08.19/yt_dlp/downloader/external.py)
- [FFprobe documentation](https://ffmpeg.org/ffprobe.html)
- [FFmpeg NUT demuxer](https://ffmpeg.org/doxygen/trunk/nutdec_8c_source.html)
- [FFmpeg NUT muxer](https://ffmpeg.org/doxygen/trunk/nutenc_8c_source.html)
