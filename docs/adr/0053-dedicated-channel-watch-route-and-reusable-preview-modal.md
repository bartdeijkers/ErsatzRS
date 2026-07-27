# ADR 0053: Add a dedicated channel watch route and reusable preview modal

## Status

Accepted (2026-07-26); partially superseded by
[ADR 0054](0054-aggregate-epg-preview-and-isolated-browser-preview-pipeline.md)
and [ADR 0055](0055-muted-autoplay-and-viewport-bound-channel-preview.md)

## Implementation

Implemented (2026-07-26). The application-owned watch projection, channel-scoped
preview JWT, shared native-HLS/Hls.js player, quick modal, dedicated route,
rolling 24-hour guide, and no-JavaScript route identity fallback are in place.
Focused Rust tests, hydration lint, matched SSR/WASM builds, and the Chromium
and Firefox browser flow cover the decision.

ADR 0054 supersedes the configured-codec preview decision and replaces the
compressed guide presentation. ADR 0055 supersedes click-to-play with muted
autoplay and bounds the shared player to desktop and mobile viewports. The
quick modal, scoped tokens, dedicated watch route, and shared browser lifecycle
remain in force.

## Context

ErsatzTV previews a channel in a modal backed by Hls.js. The modal immediately
opens the segmenter stream, exposes only basic media controls, and is tied to
the channels table. It does not provide a stable URL for opening or sharing an
admin watch view, programme context, schedule visibility, or operational
controls beyond closing the dialog.

Tunarr demonstrates a more complete administrative watch experience with a
dedicated page, channel context, and a nearby guide. ErsatzRS also needs a
lightweight preview while working through the channel list, so replacing the
modal outright would make routine administration slower.

The existing ErsatzRS preview is only a raw playlist link. It does not manage
Hls.js lifecycle, JWT-enabled installations, browser errors, or local cleanup.

## Decision

ErsatzRS deliberately extends ErsatzTV with two complementary surfaces backed
by one shared player component:

- The Preview action on `/channels` opens a quick modal. It shows channel and
  current-programme context, explicit Start, Reload, Stop Preview, and a link to
  the full player.
- `/channels/:id/watch` is the durable full experience. It adds channel
  identity, now/next information, a rolling 24-hour guide, edit and
  troubleshooting links, and an explicit shared transcode-session stop action.

The original decision made both surfaces click-to-play. ADR 0055 supersedes
that policy: an active playable projection now starts muted automatically.
Playback always requests `mode=segmenter`.

The preceding configured-codec requirement is superseded by ADR 0054. Preview
now uses an isolated H.264 eight-bit/AAC derivative while preserving the other
configured playback behavior; normal IPTV output remains unchanged.

The browser uses native HLS where available and a repository-bundled, pinned
Hls.js module otherwise. Closing the modal, stopping the preview, or navigating
away destroys only that browser player. Shared server workers retain the
existing channel idle policy and are cancelled only by the explicit Stop
Transcode Session action.

When IPTV JWT enforcement is active, the admin server issues a short-lived
token scoped to one channel. The token authorizes only that channel's master,
session playlist, and segment paths. Existing unscoped IPTV tokens keep their
current behavior.

The application layer owns the bounded channel-watch query. Infrastructure
owns SQL, playout selection, row decoding, and media-title enrichment. The web
layer owns localization, URL issuance, player lifecycle, and presentation.

## Consequences

Administrators gain a fast modal and a bookmarkable full player without two
independent playback implementations. The dedicated route makes schedule and
troubleshooting context available without crowding the channel list.

The preview remains operationally truthful: a codec or profile the browser
cannot decode is reported as such instead of being hidden by a different
transcode profile. Visiting either surface remains resource-conscious.

Bundling Hls.js adds a licensed web asset and a dependency synchronization
gate. Scoped query tokens can appear in browser network tools, so server logs
and rendered HTML must not expose them.

## Alternatives Considered

- Keep only the ErsatzTV-style modal. Rejected because it cannot provide a
  durable watch URL or fit the full guide and operational context.
- Replace the modal with only a dedicated page. Rejected because opening a
  quick preview from a channel row is a valuable administrative workflow.
- Duplicate the full watch page inside the modal. Rejected because it crowds
  small viewports and creates two complex layouts to maintain.
- Autoplay on open. Originally rejected because merely inspecting a channel
  would consume a transcode slot and issue credentials; ADR 0055 supersedes
  that tradeoff after administrator testing.
- Use a special browser-safe FFmpeg profile. Rejected because the preview must
  expose the real configured channel output, including graphics, subtitles,
  audio selection, and codec compatibility.

## References

- `$ERSATZTV_SRC/ErsatzTV/Pages/Channels.razor`
- `$ERSATZTV_SRC/ErsatzTV/Shared/ChannelPreviewDialog.razor`
- [Tunarr ChannelWatchPage](https://github.com/chrisbenincasa/tunarr/blob/main/web/src/pages/watch/ChannelWatchPage.tsx)
- [hls.js](https://github.com/video-dev/hls.js)
