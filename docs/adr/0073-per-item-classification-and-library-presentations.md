# ADR 0073: Classify add-on list entries per item

## Status

Accepted (2026-08-19)

## Implementation

Partial — the per-item classification, the presentation set, the grouped editor,
and the Movies and Other Videos projections are implemented (working tree,
2026-08-19). Three of the seven presentations have no projection behind them yet
and are refused on request; they are tracked as open parity debt.

## Context

The add-on list editor was built around one provider snapshot with list-wide
settings, and two of those settings had outgrown their scope.

`AddonMediaList.ContentKindOverride` classified an entire list. One imported
list can mix episodes, a trailer, and an archive fragment, so a single value
cannot be right for all of them. The data model already knew this:
`AddonMediaListItem.ContentKind` has existed since migration `202608151200`,
and `save_list_items` has always persisted whatever each add-on enumerated —
both official packages derive a kind from provider metadata. The host simply
never read the column. `list_items` did not select it, administration never
showed it, and `save_item_nfo` chose each item's NFO root from the list-wide
value, so a mixed list wrote every entry under one wrong root.

`AddonMediaListPresentationKind` offered only the two projections ADR 0071
built. An operator importing a list of films had no way to say so.

The item table had the opposite problem: one flat list headed *Local
Replacement* showed every entry at once, so what cannot be played — the only
part that needs action — was indistinguishable from what can.

## Decision

### The entry owns its classification

`AddonMediaListItem.ContentKind` becomes the effective kind: it is read, shown
and editable per row, and it selects that entry's NFO root. The list-wide value
survives only as an explicit bulk default — an **Apply to all items** action
that overwrites every entry after confirming, and the value new entries inherit.

`AddonMediaListContentKind` gains `Song` and `Image` so an entry can be
classified for the Songs and Images presentations. The add-on wire contract is
frozen and has neither, so a host-side `Song` or `Image` is sent to a package as
`Auto` rather than as a value the contract would reject.

### The presentation set mirrors the library kinds

`AddonMediaListPresentationKind` grows to `Auto` plus one variant per
`LibraryMediaKind`: Television Show, Movies, Music Videos, Other Videos, Songs,
Images, and Remote Streams. Discriminants `0` and `1` are frozen so rows written
before the set grew still decode to the same two projections. `Auto` derives the
presentation from the entries themselves.

Auto, Remote Streams, Television Show, Movies, and Other Videos have a
projection.
`is_projected` names that boundary in one place, and requesting a kind without
one is refused rather than silently stored, so a list cannot claim a
presentation it does not have. The remaining three follow the shape ADR 0071
established, one media kind at a time.

Movies and Other Videos are that shape without the Show and Season levels: each
entry becomes one media item whose metadata comes from its own NFO, over the same
scanned Remote Stream `MediaVersion`. One `FlatProjection` description drives
both, so the remaining parentless kinds add a table pair rather than a copy.
Reverting detaches the version rather than deleting it, and is refused while a
projected item is referenced from a collection, another playlist, or a playout.
Applying one projection reverts the others first, so two hierarchies can never
live over the same provider items.

`Auto` resolves from the entry classifications: a list auto-projects only when
every entry agrees and none is still unclassified, and never into a kind whose
projection does not exist. Guessing a hierarchy for a mixed list would be worse
than leaving it as Remote Streams.

### Entries are grouped by availability

The editor renders three sections — non-playable (`Unavailable`), unknown
(`Unknown`), and playable (`Available`) — each headed by its own count and paged
independently. Unknown is deliberately its own group rather than folded into
either neighbour: the provider could not determine availability, which is not
the same as knowing it is unplayable. Non-playable is the one section expanded by
default, because it is the one that needs action.

All three share one row renderer, so local replacement and the per-item kind are
available everywhere; a playable entry can still be worth replacing with a local
copy. Counts come from a separate aggregate so a heading is correct without
loading any rows, and paging is server-side with an explicit "all" choice.

## Consequences

- A mixed list classifies correctly, and the classification an add-on already
  derived is finally visible and used.
- The bulk default is destructive by nature, so it confirms first and says what
  it overwrites.
- Choosing an unprojected collection type fails loudly at the moment of choice
  instead of leaving a list in a presentation nothing implements.
- Long lists no longer render every entry at once, and the operator sees what is
  unplayable without reading the whole list.
- Three projections remain outstanding; until each lands, its collection type is
  offered but refused.

## Alternatives Considered

- Keep the media kind list-wide and add a per-item override: rejected because
  two controls that look identical would disagree, and the item value already
  existed as the more specific truth.
- Fold `Unknown` into the non-playable group: rejected because the heading would
  then assert something the provider explicitly did not determine.
- Offer only the projections that exist: rejected because the operator asked for
  the full library-kind set, and a visibly refused choice is more honest than a
  silently missing one.
- Page the sections client-side from one full fetch: rejected because it keeps
  the unbounded query the grouping was meant to remove.

## References

- [ADR 0067: Move provider lists into repository-installed add-ons](0067-move-provider-lists-into-addons.md)
- [ADR 0068: Keep add-on metadata provider-neutral in the host](0068-addon-metadata-sidecars-and-local-replacements.md)
- [ADR 0071: Project add-on media lists as television shows](0071-project-addon-lists-as-television-shows.md)
- [ADR 0072: Route operator source URLs to add-ons by declaration](0072-declared-source-hosts-and-default-remote-stream-addon.md)
