# ADR 0054: Add an aggregate EPG preview and isolated browser preview pipeline

## Status

Accepted (2026-07-26)

Partially supersedes
[ADR 0053](0053-dedicated-channel-watch-route-and-reusable-preview-modal.md):
the modal, scoped-token model, and dedicated channel route remain accepted,
but browser preview no longer uses the configured channel codecs unchanged.
The inherited click-to-play lifecycle is superseded by
[ADR 0055](0055-muted-autoplay-and-viewport-bound-channel-preview.md).

## Implementation

Implemented (2026-07-26; zoom interaction refined 2026-07-27). The aggregate repository/application query, shared
lane grid, `/epg` route, isolated preview registry and directories,
browser-compatible profile override, preview-only routes and token scope, and
Hls.js-first player are in place. Focused Rust and browser coverage exercise
the decision. Correctness hardening keeps segment paths inside validated
channel-number directories, gives normal and preview workers distinct
admission identities within the shared capacity map, batches large title
lookups, matches live-playback playout tie-breaking, renders schedule labels in
local time, invalidates stale asynchronous modal responses, and withholds HLS
playlists until the worker has prepared its session-owned directory.

## Context

The first channel-watch slice rendered a compressed single-channel 24-hour
strip. It could not provide the multi-channel lane comparison administrators
expect from an EPG preview, and loading one watch projection per channel would
turn the guide into an N+1 query.

The first player also treated Chromium's `canPlayType` response as proof that
native HLS would work and preserved the configured channel codecs. Chromium
then selected a native path it could not demux, while Firefox/Hls.js correctly
rejected an H.264/AC-3 variant as incompatible. Transport validation of the
real HLS backend passed independently, so changing normal IPTV output would
hide the browser compatibility problem in the wrong layer.

## Decision

ErsatzRS adds `/epg` as a top-level, localized administration route. An
application query requests one aggregate projection from the channel
repository. Infrastructure batch-loads enabled, `ShowInEpg` channels in
`SortNumber`, number, and ID order, then their selected playouts, overlapping
items and gaps, and media titles. The projection spans two hours before
generation time through 24 hours after it.

The global route and `/channels/:id/watch` share one lane-grid component. It
renders the complete loaded range at six pixels per minute at 100% zoom,
defaults to an approximately three-hour desktop viewport beginning one hour
before now, and keeps channel labels and the time ruler sticky. A 50–200% Zoom
control rescales the horizontal timeline while preserving its temporal center;
the native horizontal scrollbar remains the navigation control. Programme, complete filler-kind,
gap, active-session, current-time, scrolling, keyboard, and responsive
semantics belong to this shared presentation. The global route refreshes its
projection every 30 seconds without resetting the user's scroll position.

Administration playback uses a second `HlsSessionRegistry`, separate working
directories, and a browser-preview registry key. The preview worker clones the
configured channel playback profile and overrides only the compatibility
fields: H.264 video, eight-bit output, and AAC audio. Resolution, frame rate,
stream selection, subtitles, graphics, watermarks, loudness processing, and
the rest of the channel behavior remain configured-channel behavior. Normal
IPTV/HLS workers and persisted FFmpeg profiles are unchanged.

Preview playlists and segments live under `/iptv/preview/{channel}/...`.
Short-lived preview tokens authorize only the matching preview path. Preview
workers use the configured segmenter idle timeout regardless of the channel's
keep-running setting, share the normal global transcode-capacity admission,
and use a preview-specific capacity key so a normal and preview worker for the
same channel may coexist when the shared limit has room. Channel route captures
are validated before resolving session-owned files. Both worker kinds are
cancelled together by the explicit Stop Transcode Session action.
Workers do not publish a playlist through their registry handle until working
directory preparation has removed any output left by an interrupted prior
process.

The browser selects Hls.js whenever it is supported. Native HLS is a fallback
only when Hls.js is unavailable and the browser reports native support. One
Hls.js media-error recovery attempt is retained, fatal details are reported,
and modal close, route navigation, reload, and Stop Preview destroy browser
player state.

## Consequences

Administrators can compare published schedules in a readable multi-channel
guide without per-lane repository calls. The dedicated watch route retains its
stable URL and operational context while presenting the same guide semantics.

Browser preview is intentionally a derivative, not a byte-for-byte statement
of the configured channel codec profile. The UI discloses that distinction.
Normal clients still receive the persisted channel profile, and the two
registries prevent an admin preview from replacing or reusing their worker.
Both worker kinds still compete for the same bounded transcode capacity.

The additional route and session namespace increase test surface: repository
aggregation, grid geometry, token scope, registry isolation, profile overrides,
and real Chromium/Firefox playback require dedicated coverage.

## Alternatives Considered

- Call the single-channel watch query once per EPG lane. Rejected because its
  query count grows with the channel count and cannot provide one coherent
  generated window.
- Change every channel's HLS profile to H.264/AAC. Rejected because it mutates
  persisted operator intent and normal IPTV behavior.
- Reuse the normal channel HLS registry with a different profile. Rejected
  because registry-key collision could replace or reuse a normal worker.
- Prefer native HLS whenever `canPlayType` returns `maybe`. Rejected because
  Chromium's response did not establish demuxer compatibility.
- Transmux configured codecs in JavaScript. Rejected because incompatible
  audio/video codecs still require transcoding and would duplicate pipeline
  policy in the browser.

## References

- [ADR 0053](0053-dedicated-channel-watch-route-and-reusable-preview-modal.md)
- `$ERSATZTV_SRC/ErsatzTV/Pages/Channels.razor`
- `$ERSATZTV_SRC/ErsatzTV/Shared/ChannelPreviewDialog.razor`
- [hls.js](https://github.com/video-dev/hls.js)
