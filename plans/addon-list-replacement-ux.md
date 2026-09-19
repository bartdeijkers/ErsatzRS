# Local replacement workflow improvements

Future generic UX work. The completed `amm-g-06` enumeration blocker is recorded
in [milestones](../docs/progress/milestones.toml); it does not complete this
workflow. ADRs 0068 and 0091 own existing replacement and task-design behavior.
Recheck each proposal against the current item workspace before implementing it;
retain completed behavior and implement only the remaining gaps.

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


## Owners

The provider-neutral contract and history belong to
[Remote Stream playlists](../docs/progress/parity/remote-stream-playlists.toml).
Keep bulk matching and reverse-entry placement as explicit future product
choices; this cleanup does not approve their implementation.
