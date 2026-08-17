# Plan: make Local replacement discoverable in both directions

## Implementation status

Not started. Written 2026-08-17 after fixing the defect that made the
replacement picker return no candidates at all, and after walking the flow
end to end against a real unavailable episode.

## Context

An add-on list item can be unavailable at the provider while the operator holds
a local copy. `AddonMediaListItemReplacement` maps that list item to a scanned
local media version, and playback then uses the local file.

The mechanism works. Confirmed on 2026-08-17: Teleac item 49 (rank 6, reported
`unavailable` / `not_playable`) now resolves to a local `.mp4` through the
existing picker, and the row reports the replacement.

Getting there exposed how hard the feature is to actually use.

### What the flow costs an operator today

1. **The picker is detached from the item.** Selecting **Replace** on a row
   opens a search panel at the bottom of the page. Nothing in that panel names
   the item being replaced, so with eight similarly-titled episodes on screen
   there is no way to confirm the right target. Driving this flow required
   tagging DOM nodes to keep track of which row was in play.
2. **The pre-filled query is actively counterproductive.** The panel pre-fills
   the provider title. For an unavailable item that title is often a
   placeholder — here it was `STRIP- EN CARTOONTEKENEN - Deze video bekijken?`
   ("watch this video?"), which matches nothing. The default state of the
   picker is therefore an empty result list, and the operator has to clear the
   field and guess a term.
3. **The operator must already know the answer.** There are no suggested
   candidates. Nothing uses the signals the host already holds: the item's
   duration, its episode number, the list name, or the folder the operator
   just scanned.
4. **The reverse direction does not exist.** With a folder of replacement files
   in hand, the only route is item-by-item from the list page. There is no way
   to start from the file.
5. **Replacing several items is entirely manual** — search, select, repeat.

### What already supports a better design

- One write path covers everything:
  `set_addon_media_list_item_replacement { item_id, media_item_id, media_version_id }`.
  Clearing is the same call with `None`.
- `AddonMediaListItemReplacement` is keyed by `AddonMediaListItemId`, and
  `MediaItemId` is only indexed, not unique. **One local file may already back
  several list items**, so the reverse direction needs no schema change.
- `/media/other/videos/{id}` (and the Movie/Episode equivalents) already exist
  as item pages and are the natural home for the reverse entry point.
- Candidate rows already carry `title`, `path`, `media_item_id` and
  `media_version_id` — enough to rank and group without new queries.

## Decisions to confirm before building

- Whether the reverse entry point belongs on every local item page or only on
  Other Videos, which is the kind operators will realistically use for
  replacements.
- Whether bulk folder matching (§5) is wanted at all, or whether per-item
  assignment with good suggestions is enough.

## Proposed work

### 1. Anchor the picker to the item it replaces

Move the picker into a modal that names its target: item title, provider
availability and reason, episode number where known, and provider duration.
The operator should never have to remember which row they clicked. Reuse the
existing dialog primitive rather than introducing a new pattern.

### 2. Stop pre-filling a query that cannot match

Open with suggestions rather than a failed search. Derive the initial term from
the *host's* view of the item — show title plus episode number — and never from
a provider placeholder. If no term is derivable, leave the field empty and show
suggestions instead of an empty table.

### 3. Suggest candidates before the operator types

Rank by signals already available:

- duration within a tolerance of the provider duration (strongest signal, and
  the one an operator checks by hand today);
- filename or title similarity to the item title or episode number;
- membership of a library path whose name matches the list or show;
- recently added, to surface a folder the operator just scanned.

Show the match reason per row ("duration matches", "same folder"). A ranked
shortlist is what turns this from a search problem into a confirmation.

### 4. Reverse direction: assign from the media item

On the local item page, add **Use as replacement for…**. It opens a picker over
add-on list *items*, defaulting to unavailable ones, grouped by list, searchable,
with multi-select so one file can back several items in one action. Selection
calls the existing command once per target.

The same page should show what the file currently replaces, with a way to
detach — otherwise the relationship is only visible from the other side.

Needs one new read: candidate list items for a given media item, with their
list name, availability and any existing replacement. No new write path.

### 5. Bulk matching from a folder (optional, confirm first)

On the add-on list page, **Match replacements from folder…** proposes pairings
for all unavailable items using the §3 ranking, and shows a preview table with
per-row accept/reject before anything is written. Nothing is applied without
explicit confirmation.

### 6. Make the relationship legible

- Show the replacement's path on the item detail page, not only the list table.
- Report replaced items in the add-on list health check, so a replacement whose
  file disappears is surfaced rather than failing at playback.
- Confirm the item card indicates it plays from a local file.

## Verification

- An operator with a scanned replacement folder can assign a file to an
  unavailable item without typing a search term.
- The picker names its target item at all times.
- From a local item page, one file can be assigned to several list items in a
  single action, and detached again.
- Assignments made in either direction produce the same
  `AddonMediaListItemReplacement` rows.
- Removing the underlying file surfaces a health finding rather than a playback
  failure.
- Focused `ersatzrs-web` and `ersatzrs-infra` tests, then the workspace gate.
- Browser proof for the picker and the reverse assignment, per TEST.md.

## Related

- ADR 0068 — add-on metadata sidecars and Local replacements.
- `docs/progress/parity/remote-stream-playlists.toml` — owns add-on list
  behaviour.
