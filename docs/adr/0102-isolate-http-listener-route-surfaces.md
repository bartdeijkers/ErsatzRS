# ADR 0102: Isolate HTTP listener route surfaces

## Status

Accepted — maintainer approval on 2026-09-27.

## Implementation

Implemented (UCU-07 row 110, 2026-09-27) — listener and route separation,
trusted callback origins and direct/HLS lifetime isolation have focused,
decoded-runtime, TSDuck and matched-browser proof. Milestone window gates and
cross-platform CI remain separate acceptance obligations.

## Context

Upstream commit `14372575160a58672a44c9ff52034904b5ace156` removes a
client-controlled Host exception that exposed management routes through a
separate streaming port. It also restricts internal media callbacks to actual
loopback peers before processing forwarded headers.

ErsatzRS currently combines management, IPTV and internal callbacks on one
listener. Next callbacks already require capabilities and exact media/session
bindings (ADR 0095), but native legacy concat callbacks lack an access boundary.
The public listener can bind a specific network interface, so a local callback
cannot assume that the same port accepts connections on a loopback address.

## Decision

Partition the existing axum host's routes by listener, within the same process:

- The existing `LEPTOS_SITE_ADDR` listener keeps management and public streaming
  routes, preserving the default shared-listener deployment.
- An optional `ERSATZRS_STREAMING_ADDR` listener exposes only public IPTV,
  XMLTV, channel logos and HDHomeRun HTTP routes. An identical nonzero address
  shares the existing listener. Management routes and assets are absent from
  a distinct streaming router, independently of Host or forwarded headers.
- A dynamically allocated loopback listener exposes only internal FFmpeg and
  Next callbacks. An actual-peer loopback check provides a second boundary;
  missing peer information fails closed. Keep the existing capability/ticket
  and persisted binding checks on Next callbacks.
- Generate callback origins from the bound internal address, never request
  headers. Keep callback paths private and consistent across native/Next
  generators, including legacy concat playlists. Existing callback path names
  need no public compatibility alias: they are internal implementation details.
- Hybrid MPEG-TS wrappers read public HLS from the actual bound streaming
  address, mapping wildcard binds to loopback and preserving access tokens.
  Request and forwarded headers cannot redirect these server-side requests.
- Bind every required listener before launching host background work. Startup
  failures release already-bound sockets. All listeners share the host shutdown
  token and failures cancel the other listeners; no server task is detached.

Direct HTTP response processes remain separate from playlist-producing workers,
as in upstream `IptvController` and `FFmpegSegmenterService`. The existing Rust
registry tracks direct responses separately for channel activity and Stop, with
cleanup by handle identity. HLS lookup and admission cannot reuse a direct
response that is still being torn down. This corrects the native transition
failure found during listener acceptance without adding a service or dependency.

The loopback listener belongs to the existing host; no additional executable,
dependency, sidecar or database is introduced. This preserves the authenticated
Next transport decision and the direct native producer decision (ADR 0057).

## Consequences

Forwarded headers continue to describe public URLs but cannot select a router
or authorize internal work. Separate streaming publication cannot expose admin
forms, APIs, Leptos server functions, or development routes. Native and Next
callbacks remain local even when public listeners use specific interfaces.

Operators may publish the optional streaming address without publishing the
management port. The default combined listener remains suitable for trusted
networks; this row does not introduce management login or API-key policy.

Acceptance requires production-router denial tests with crafted headers,
IPv4/IPv6 and shared/separate binding tests, startup/shutdown lifecycle checks,
and decoded native/Next playback preserving audio and subtitle selections.
Milestone UCU-07 owns results; the decision alone establishes no acceptance.

## Alternatives considered

- Trust a localhost Host or forwarded address: client-controlled data cannot
  establish the connection's origin or listener.
- Filter management paths on a shared streaming router: registering only the
  public routes makes future admin additions private by construction.
- Reuse the public listener for loopback callbacks: specific-interface binds
  do not guarantee a loopback socket; relaxing peer restrictions would weaken
  the upstream security invariant.
- Add a separate server process: unnecessary for independently bound axum
  listeners with shared application state and coordinated lifetime.

## References

- [Catch-up source map](../../plans/upstream-catch-up.md)
- [Next integration](0095-native-pipeline-instead-of-ersatztv-next-engine.md)
- [Native direct producers](0057-direct-remote-stream-pipelines-and-playlist-actions.md)
- `$ERSATZTV_SRC/ErsatzTV/Startup.cs`
- `$ERSATZTV_SRC/ErsatzTV/Controllers/InternalController.cs`
