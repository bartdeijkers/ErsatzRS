# ADR 0084: Adapt live add-on quality after HLS under-delivery

## Status

Accepted

## Implementation

Partial — the provider-neutral play v2 contract, bounded 1080p-to-720p HLS
recovery, verified 24-hour fallback cache, health reporting, and official
yt-dlp add-on policy are implemented; exact Windows live acceptance remains
tracked by milestone `hls-live-01`.

## Context

A live add-on producer can remain alive and emit structurally valid MPEG-TS
while advancing media time too slowly for uninterrupted HLS playback. Process
exit supervision cannot detect that failure, and increasing the player buffer
does not correct sustained under-delivery. Provider-specific rendition syntax
must remain outside the host.

ErsatzTV does not automatically change the quality of a script-backed source
from HLS delivery telemetry. This recovery is therefore an intentional
reliability deviation.

## Decision

Add `remote-stream.play.v2`. The host supplies
`ERSATZRS_REMOTE_STREAM_MAX_VIDEO_HEIGHT` as a provider-neutral upper bound;
the add-on translates it into provider-specific rendition selection. Version
1 remains supported without automatic quality recovery.

For live HLS v2 sources, the worker starts with a 1080p ceiling and measures
completed playlist duration against monotonic wall time. Severe under-delivery
may trigger exactly one restart of the same live item with a 720p ceiling. The
restart uses the existing HLS append, discontinuity, timestamp-normalization,
process-group, and Windows Job ownership paths.

A 720p fallback is remembered only after one full healthy delivery window.
The provider-neutral record contains stable media identity, timestamps, and
the ceiling, expires after 24 hours, and is pruned to 256 entries. Expiry
retries 1080p. A 720p under-run clears the learned record and never creates a
restart loop.

## Consequences

Healthy sources retain 1080p. A source that cannot sustain it can recover
without an operator changing the channel profile, and later tuning sessions
avoid repeatedly reproducing the initial stall. The host does not learn
provider URLs or format identifiers.

The first downgrade introduces one controlled HLS discontinuity and may skip
live content while the new producer starts. Play v1 add-ons continue to expose
health warnings but cannot receive the generic quality ceiling.

## Alternatives Considered

- Permanently cap the channel at 720p: rejected because healthy sessions would
  lose quality and the choice belongs to the source, not the output profile.
- Increase the HLS startup buffer: rejected because it only delays sustained
  starvation.
- Put delivery monitoring and restart loops inside yt-dlp: rejected because
  HLS publication timing, cancellation, and process-tree ownership belong to
  the host.
- Use `--concurrent-fragments`: rejected because the external FFmpeg downloader
  path merges selected inputs and does not use yt-dlp's native fragment
  concurrency.

## References

- [../plans/hls-live-delivery-starvation.md](../../plans/hls-live-delivery-starvation.md)
- [0057-direct-remote-stream-pipelines-and-playlist-actions.md](0057-direct-remote-stream-pipelines-and-playlist-actions.md)
- [0066-repository-backed-add-ons.md](0066-repository-backed-add-ons.md)
- [../progress/parity/remote-stream-playback.toml](../progress/parity/remote-stream-playback.toml)
