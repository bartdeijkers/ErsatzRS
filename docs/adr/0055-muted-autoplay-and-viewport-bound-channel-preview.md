# ADR 0055: Autoplay channel previews muted within the viewport

## Status

Accepted (2026-07-27)

Partially supersedes
[ADR 0053](0053-dedicated-channel-watch-route-and-reusable-preview-modal.md)
and [ADR 0054](0054-aggregate-epg-preview-and-isolated-browser-preview-pipeline.md):
their explicit click-to-play policy no longer applies.

## Implementation

Implemented (2026-07-27). The shared player performs one muted autoplay attempt
per active playable channel, retains explicit reload/stop controls, and uses
viewport- and container-bounded responsive geometry. Chromium and Firefox
coverage proves lifecycle behavior, real playback, and desktop plus
portrait/landscape-mobile containment.

## Context

The channel-list modal and dedicated watch route required an explicit Start
Stream action before issuing preview authorization or starting HLS playback.
Repeatedly starting previews during EPG and channel administration made the
workflow unnecessarily cumbersome. The shared 16:9 player also expanded to the
full content width, which could make the video or modal larger than desktop and
mobile viewports.

ErsatzTV starts its channel preview immediately. Modern browsers permit
autoplay when the media is muted, and both ErsatzRS preview surfaces already
share lifecycle, authorization, failure, and teardown behavior.

## Decision

Both preview surfaces request their channel-scoped URL and begin playback
muted as soon as the active channel projection becomes playable. Each
activation makes at most one automatic attempt; periodic projection refreshes
must not restart playback. Reload remains an explicit retry, Stop Preview
remains local browser teardown, and native controls let the administrator
pause or unmute.

Closing the modal, changing channels, or navigating away invalidates pending
authorization and player work as before. Reopening the preview creates a new
activation and may start it again.

The full-page player remains a centered, uncropped 16:9 frame bounded by the
dynamic viewport. The modal reserves space for its title, status, controls,
programme context, and footer, then scales the frame into the remaining width
and height. Desktop, portrait-mobile, and landscape-mobile geometry are
browser-tested.

## Consequences

Opening either preview surface can immediately issue a scoped credential and
consume a slot from the existing shared transcode-capacity limit. Capacity and
playback failures remain visible and recoverable through Reload. Closing the
surface still releases the browser player, while the isolated server preview
worker follows its existing idle timeout.

The isolated H.264 eight-bit/AAC derivative, normal IPTV behavior, token
format, preview registry, and persistence contracts are unchanged.

## Alternatives Considered

- Keep click-to-play. Rejected after administrator testing showed that the
  repeated extra action obstructed the intended rapid-preview workflow.
- Autoplay with audio. Rejected because browsers commonly block audible
  autoplay and unexpected audio is inappropriate in an administration UI.
- Fill the viewport by cropping. Rejected because previewing the complete
  channel composition is more important than eliminating letterboxing.

## References

- [ADR 0053](0053-dedicated-channel-watch-route-and-reusable-preview-modal.md)
- [ADR 0054](0054-aggregate-epg-preview-and-isolated-browser-preview-pipeline.md)
- `$ERSATZTV_SRC/ErsatzTV/Shared/ChannelPreviewDialog.razor`
