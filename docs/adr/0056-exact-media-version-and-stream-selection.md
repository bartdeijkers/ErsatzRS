# ADR 0056: Persist exact media-version and stream selections

## Status

Accepted (2026-07-27)

Extends [ADR 0048](0048-collection-item-playback-overrides.md) and
[ADR 0018](0018-extras-alternate-versions-and-stream-roles.md).

## Implementation

Implemented (2026-07-27). Movie detail exposes linked Version, Video, Audio,
and Subtitle selectors for Add To Schedule and Add To Playlist. Playlist,
schedule, and materialized playout rows persist the exact tuple, editors
preserve it, scheduling propagates it, and MPEG-TS plus HLS playback enforce
it. The health engine reports stale tuples.

## Context

Language/title preferences from ADR 0048 cannot distinguish multiple files
for one movie or duplicate streams with the same metadata. Jellyfin-style
media-source selection rebuilds the dependent video, audio, and subtitle
choices when a version changes and gives the administrator an exact,
reviewable combination.

## Decision

`PlaylistItem`, `ProgramScheduleItem`, and `PlayoutItem` gain nullable
`MediaVersionId`, `VideoStreamIndex`, `AudioStreamIndex`, and
`SubtitleStreamIndex` columns. A pinned tuple always includes a media version
and video stream; audio and subtitle are optional. No audio means explicit
silence and no subtitle means explicit Off, so tuple presence must remain
distinguishable from an unpinned row whose normal selectors are automatic.
Movie detail defaults to the first media version, its default video/audio
streams, and subtitles Off.

Add To Schedule and Add To Playlist validate that the version belongs to the
movie and every selected stream belongs to that version. Classic scheduling
and playlist expansion preserve the tuple through materialization. Active
MPEG-TS and HLS queries select only the pinned media version, and FFmpeg input
mapping uses pinned stream indexes before channel or global preferences.
Pinned subtitle lookup also scopes embedded metadata to that media version;
missing or ambiguous exact matches fail closed.

The columns intentionally have no foreign keys. Rescanning can remove or
replace source streams, and preserving stale values makes the operator's
choice auditable. Playback fails closed when a pinned version or stream no
longer exists; it never silently selects another file or stream. The Pinned
Stream Selection health check reports those stale references.

## Consequences

Administrators can create schedule and playlist entries for a particular cut,
audio mix, commentary track, or subtitle stream. Existing unpinned rows retain
normal channel/global stream-selection behavior.

The exact tuple is an ErsatzRS extension over ErsatzTV's preference fields and
adds migration, editor-preservation, scheduling, playback, health, and browser
test obligations. A rescan may require recreating a pinned entry when source
indexes legitimately change.

## Alternatives Considered

- Persist only language and title preferences. Rejected because those values
  are not unique and cannot pin a media version.
- Fall back when a pinned stream disappears. Rejected because it can broadcast
  unintended content without operator visibility.
- Add foreign keys with cascading deletion. Rejected because that would erase
  the requested tuple and prevent actionable health reporting.

## References

- [ADR 0018](0018-extras-alternate-versions-and-stream-roles.md)
- [ADR 0048](0048-collection-item-playback-overrides.md)
- `$ERSATZTV_SRC/ErsatzTV/Pages/Movie.razor`
- Jellyfin Web item-details media-source selection
