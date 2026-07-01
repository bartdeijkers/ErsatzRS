# ADR 0048: Collection Item Playback Overrides

## Status

Accepted

## Implementation

Implemented

## Context

ErsatzTV stores preferred audio and subtitle settings on channels, program
schedule items, and generated playout items, but `CollectionItem` and
`PlaylistItem` only identify the media item to enumerate. That means a movie can
be added to a collection or playlist, but an admin cannot save that this specific
entry should use a named commentary or alternate audio track.

ADR 0018 added ErsatzRS content variant and commentary-track surfacing. The
movie detail page can identify named audio tracks and can preview them through
playback troubleshooting, but the add-to-collection/add-to-playlist flows need a
persisted model before the selected audio track can survive scheduling.

## Decision

Add ErsatzRS-owned playback override columns to `CollectionItem` and
`PlaylistItem`:

- `PreferredAudioLanguageCode`
- `PreferredAudioTitle`
- `PreferredSubtitleLanguageCode`
- `SubtitleMode`

When a collection or playlist item is expanded for scheduling, copy those
preferences to each resulting scheduled media item. The scheduling engines then
copy them into generated `PlayoutItem` rows, where the existing playback path
already honors preferred audio title/language, preferred subtitle language, and
subtitle mode.

Keep the fields optional. A row with no override behaves exactly like the
ErsatzTV-compatible row shape.

## Consequences

Admins can add the same movie to different ErsatzRS collections or playlists
with different named audio-track preferences. Collection detail pages must make
saved overrides visible so the item no longer looks identical to a default
playback entry.

This intentionally extends the ErsatzTV schema. Import/export or parity tooling
that compares collection tables must treat these fields as ErsatzRS-only
metadata and ignore them when comparing against an ErsatzTV database.

Subtitle selection remains aligned with the current playback engine: saved
subtitle preferences are language/mode based, not a hard subtitle row id.

## Alternatives Considered

- Store playback options only on program schedule items. Rejected because it
  does not help admins who build collections and playlists as reusable content
  groups.
- Encode commentary variants as duplicate media items. Rejected because it
  hides the real source media relationship and complicates scanner/provider
  parity.
- Add a separate override table. Rejected for now because `CollectionItem` and
  `PlaylistItem` are the exact rows the admin is editing, and additive nullable
  columns are simpler to preserve and display.

## References

- [ADR 0018: Extras, Alternate Versions, and Stream Roles](0018-extras-alternate-versions-and-stream-roles.md)
- `$ERSATZTV_SRC/ErsatzTV.Core/Domain/Collection/CollectionItem.cs`
- `$ERSATZTV_SRC/ErsatzTV.Core/Domain/Collection/PlaylistItem.cs`
