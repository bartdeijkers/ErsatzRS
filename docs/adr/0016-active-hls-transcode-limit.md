# ADR 0016: Active HLS transcode limit

## Status

Accepted (2026-06-16: implemented for HLS Segmenter FFmpeg generations)

## Context

ErsatzTV exposes `ffmpeg.segmenter.work_ahead_limit`, which limits how many HLS
segmenters may transcode ahead at maximum speed. That protects against multiple
channels racing far ahead of realtime, but it is not a total cap on active
FFmpeg transcode processes.

ErsatzRS originally matched that shape. During channel switching, old
`StopOnDisconnect` HLS sessions could remain alive until the configured idle
timeout. When many channels were visited quickly, those sessions could continue
spawning realtime FFmpeg generations even when the work-ahead pool was full.
This kept zapping responsive, but could overload smaller systems, increase fan
noise and heat, and degrade the channel the viewer actually settled on.

The pinned ErsatzTV baseline does not have a separate `ConfigElement` for a
total active HLS transcode limit. Adding one is therefore an intentional
ErsatzRS runtime hardening decision rather than a direct port.

## Decision

Add `ffmpeg.segmenter.active_transcode_limit` with default `2`.

The setting limits the number of HLS Segmenter FFmpeg generations that may run
at the same time, independently from the existing
`ffmpeg.segmenter.work_ahead_limit`:

- `work_ahead_limit` continues to control only maximum-speed work-ahead
  transcodes.
- `active_transcode_limit` controls total active HLS Segmenter FFmpeg
  generations.
- Recently touched sessions may temporarily take a priority slot even when the
  configured active limit is full, so channel switching and the currently
  watched channel stay responsive.
- Priority slots are counted in the global active-transcode counter and use a
  short no-viewer timeout, so abandoned channel switches drain instead of
  waiting for the full segmenter idle timeout.
- Background workers without an active slot wait and trim their playlist
  instead of spawning another realtime FFmpeg process.

The setting is exposed on `/settings/ffmpeg` as "Active HLS Transcode Limit"
and is localized with the rest of the settings route family.

## Consequences

Fast channel changes continue to prefer the viewer's current request, but old
sessions no longer multiply FFmpeg processes solely because their idle timeout
has not expired. Smaller machines get a hard back-pressure point for HLS
Segmenter load, while work-ahead throughput remains separately tunable.

The limit is process-local, matching the in-memory HLS session registry. It is
not a cluster-wide scheduler. A priority viewer can temporarily push the count
above the configured limit; this is deliberate so playback responsiveness wins
over strict admission control during zapping.

## Alternatives Considered

- Reuse `ffmpeg.segmenter.work_ahead_limit` as a total process limit: rejected
  because it would conflate maximum-speed buffering with total active FFmpeg
  load, and would make the existing ErsatzTV-compatible setting misleading.
- Rely only on the idle timeout: rejected because idle timeout bounds lifetime,
  not concurrency; many channel switches inside one timeout window can still
  create too many simultaneous transcodes.
- Strictly reject or block the newest viewed channel when the limit is full:
  rejected because this optimizes system protection at the expense of the
  viewer's active channel and makes zapping feel broken.
- Add a full preemptive scheduler that cancels the oldest idle worker
  immediately: deferred as more complex than needed for this hardening slice;
  the priority slot plus background waiting model gives the active viewer
  priority without adding cross-worker ownership rules.

## References

- [`docs/progress/parity/hls-variant-playlists-segmenter.toml`](../progress/parity/hls-variant-playlists-segmenter.toml)
- [`crates/ersatzrs-infra/src/hls_worker.rs`](../../crates/ersatzrs-infra/src/hls_worker.rs)
- [ADR 0003: Invoke FFmpeg through the CLI](0003-ffmpeg-cli-spawn.md)
