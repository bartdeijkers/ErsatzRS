# ADR 0011: Playlist Editor Semantics and Generated Playlist Read-Only UI

## Status

Accepted

## Context

ErsatzTV generated Trakt playlists are system playlists. The scheduler behavior
is parity: `TraktCommandBase` creates generated `PlaylistItem` rows with
`IncludeInProgramGuide = true` and chronological playback, while `Count`
remains unset and `PlayAll` defaults to false. `PlaylistEnumerator` interprets
that shape as one child item per playlist pass. For a generated show row, that
means the show enumerator advances across repeated playlist visits; it does not
mean the first episode is played forever.

The ErsatzTV playlist editor renders this state as a disabled editor: Count
appears as `1`, Play All is an unchecked disabled checkbox, and row actions are
visible but disabled. That is technically consistent with the persisted shape,
but it is confusing for generated playlists because the user sees editable
controls that cannot be changed and a Count value that reads like a hard
one-episode limit.

The same Count and Play All labels can also be unclear for user-created
playlists. In the summary grid, Count is not an editor control; it describes how
many child items are consumed each time the playlist visits that row. Showing a
bare `1` next to playback order does not explain the scheduler behavior.

ErsatzRS ports ErsatzTV behavior by default, but can intentionally diverge in
admin UI presentation when the persisted and scheduling behavior remains
unchanged and the divergence reduces operator confusion.

## Decision

Preserve the generated playlist scheduling and persistence semantics. Do not set
`PlayAll = true` or a concrete `Count` when Trakt list sync generates playlist
items.

Render system/generated playlist items as a read-only projection instead of a
disabled editable table:

- Display unset Count with `PlayAll = false` as `1 per pass`.
- Display Play All and Show In EPG as read-only Yes/No text.
- Omit copy, move, and delete row actions for generated playlist rows.
- Keep the playout preview available.

Render user-created playlist summary rows with the same Count semantics:

- Display Count values as `{n} per pass`.
- Display Play All rows as `All` in the Count column.
- Keep Play All, Show In EPG, and item detail fields editable.
- Open the item detail editor only when an item row is selected.

## Consequences

Generated Trakt playlists explain their scheduling behavior without changing how
they play. Users no longer need to infer behavior from disabled controls, and
the generated playlist grid fits the default editor viewport more reliably.

Manual playlists use the same scheduler language in their table summary while
keeping the full editor available on selection. The list-first view also keeps
manual and generated playlist pages visually consistent until the user chooses a
specific item to edit.

Future playlist parity work must preserve this as an intentional UI divergence:
system playlists remain source-managed, and edits should happen through the
owning integration rather than the playlist item editor.

## Alternatives Considered

- Set `PlayAll = true` for generated show and season rows. Rejected because it
  changes scheduling semantics and Trakt-rank traversal.
- Keep strict ErsatzTV disabled controls. Rejected because the UI was
  misleading for generated playlists.
- Allow editing generated playlists. Rejected because generated rows are
  source-managed and can be overwritten by refresh or match operations.
- Always show the manual playlist item editor. Rejected because it makes manual
  playlist pages look structurally different from generated playlists before the
  user has selected an item.

## References

- `$ERSATZTV_SRC/ErsatzTV/Pages/PlaylistEditor.razor`
- `$ERSATZTV_SRC/ErsatzTV.Application/MediaCollections/Commands/TraktCommandBase.cs`
- `$ERSATZTV_SRC/ErsatzTV.Core/Scheduling/PlaylistEnumerator.cs`
- [Trakt lists parity slice](../../plans/trakt-lists-parity-slice.md)
