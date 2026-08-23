# ADR 0078: Separate the Remote Stream type from live-source semantics

## Status

Accepted (2026-08-23)

## Implementation

Implemented (2026-08-23)

## Context

Remote Stream is an existing umbrella media type for direct HTTP(S), HLS, and
RTSP sources plus add-on-produced media. It deliberately covers both finite
content such as YouTube VOD and archived programmes and genuinely live sources.

The reviewed Add-on Lists path does not currently preserve that distinction.
`media-list.list.v4` has no liveness field, staged publication reconstructs a
playlist entry with the legacy default `is_live = false`, and generated YAML
therefore marks a provider-discovered live source as finite. The playback
pipeline can then seek, pace, or admit work-ahead as if the source were VOD.

Renaming the media type to Live Stream would not fix the missing state. It
would also mislabel finite Remote Streams and require compatibility work across
persisted enums, routes, YAML, add-on capability names, translations, and
schedules.

## Decision

Keep **Remote Stream** as the canonical collection type, page name, persisted
identity, and add-on capability family. Do not rename `remote_stream` or its
user-facing type to Live Stream. Help text may clarify that Remote Streams can
be live or on-demand.

Model live behavior as a separate provider-neutral per-item value with exactly
three states: `unknown`, `finite`, and `live`. The field is playback state, not
descriptive Kodi metadata, but it follows the AMM provider-snapshot,
operator-override, conflict, review, and published-snapshot ownership model.
An override wins; an untouched provider value refreshes; a published change
requires explicit Publish.

Version the wire contracts rather than changing frozen shapes.
`media-list.list.v5` and `remote-stream.list.v4` carry the three-state value.
Legacy `is_live = true` adapts to `live`, a positive duration adapts to
`finite`, and false/omitted liveness without a positive duration adapts to
`unknown`. Unknown must never silently become finite.

New generated YAML carries `liveness: unknown|finite|live`. Hand-authored YAML
keeps compatibility: explicit legacy `is_live: false` means finite, explicit
true means live, and omission means unknown. Supplying contradictory legacy and
new values is invalid.

The official yt-dlp add-on maps provider-specific status to the shared value;
ErsatzRS contains no YouTube vocabulary or URL branch. Import and explicit
refresh are the provider contact points. Playout does not perform an implicit
provider probe.

Publication and scanning preserve effective liveness into generated YAML and
an authoritative three-state Remote Stream persistence field. The existing
`RemoteStream.IsLive` boolean remains only as a migration compatibility mirror,
true exclusively for live; all runtime readers move to the authoritative
value. Unknown items remain reviewable and library-visible but unavailable for
playback or scheduling. Finite items require a positive duration for ordinary
schedule membership. Live items may omit duration and may run only in a
schedule mode with a deterministic end boundary; Classic/Flood is the required
baseline. Every unsupported placement is rejected before playout with a
localized diagnostic.

The shared inline item accordion exposes **Playback behavior** with provider,
finite, and live choices. Unknown participates in the existing need-review
count. The UI uses text plus an icon rather than a new collection type,
popover, color-only state, or decorative pill set.

## Consequences

- Remote Stream remains accurate and backward compatible for live and finite
  provider-backed media.
- A true live source reaches IPTV/HLS with zero seek, realtime admission, no
  finite pacing, and no early work-ahead.
- The schema, contract, editor, publication snapshot, scanner, schedulers,
  health checks, and official add-ons all gain one explicit field and tests.
- The legacy database boolean remains temporarily duplicated as a checked
  compatibility mirror until a later schema cleanup can remove it safely.
- Existing durationless false/omitted entries become visible review debt
  instead of being guessed finite.
- Upcoming/ended provider vocabulary stays inside the yt-dlp package, while
  operators retain a durable correction path.

## Alternatives Considered

- Rename Remote Stream to Live Stream: rejected because the type also contains
  finite remote media and the rename would not add missing liveness state.
- Keep a boolean `is_live`: rejected because false cannot distinguish confirmed
  finite media from missing provider information.
- Infer live solely from missing duration: rejected because VOD can lack a
  duration and live DVR sources can expose one.
- Probe yt-dlp automatically at playout start: rejected for the initial slice
  because it adds provider latency, rate-limit pressure, and nondeterministic
  schedule behavior. Explicit refresh and operator override are observable.
- Treat every unknown source as live: rejected because it would hide metadata
  defects and make finite playback non-seekable without operator intent.

## References

- [ADR 0057: Direct Remote Stream pipelines and playlist actions](0057-direct-remote-stream-pipelines-and-playlist-actions.md)
- [ADR 0065: Managed Remote Stream playlists](0065-managed-remote-stream-playlists.md)
- [ADR 0075: Review add-on metadata before library publication](0075-review-addon-metadata-before-publication.md)
- [AMM implementation evidence](../progress/parity/addon-metadata-management.toml)
- [Remote Stream playlist manifest reference](../reference/remote-stream-playlist-manifests.md)
