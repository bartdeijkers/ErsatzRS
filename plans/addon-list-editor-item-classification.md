# Add-on list editor: per-item classification and library projections

Second slice after the source-resolver work (ADR 0072).

Status: the editor rework, the per-item classification, and the Movies
projection are implemented; four library projections remain (Music Videos,
Other Videos, Songs, Images). Sections below marked **done** are in the
working tree.

## Context

The add-on list editor was built around one provider snapshot with list-wide
settings. Operator feedback (2026-08-19) reshapes it around the items:

- The list-wide **Media kind** is the wrong scope. Each entry has its own kind,
  and the add-on already tells the host which kind it enumerated
  (`media-list.list.v3` emits typed movie / show / season / episode / remote
  stream records).
- **Collection type** currently offers only the two projections ADR 0071 built.
  The operator wants the full library-kind set, matching `LibraryMediaKind`:
  TV Shows, Movies, Music (music videos), Other Videos, Songs, Images, Remote
  Streams — plus an `Auto` default derived from item metadata.
- One flat **Local Replacement** table hides what matters. The operator needs to
  see what cannot be played, separately from what can, with counts and paging.

**Locked decisions (maintainer, 2026-08-19):**
1. Build all seven projections now, not a selector over two.
2. Per-item media kind, *and* keep the list-wide value as a bulk default.
3. Three item sections: non-playable, unknown, playable — `Unknown` availability
   gets its own group rather than being folded into either side.
4. Ships after the ADR 0072 resolver slice, as its own reviewable change.

## Layout the editor must end up with

Field order, top to bottom:

1. `Add-on list URL` — first input field.
2. `Add-on` — resolved from the URL (ADR 0072), changeable afterwards.
3. Everything else, populated by the resolved add-on: title, summary,
   collection type, refresh and playlist settings, storage folder.

`Media kind` leaves this form entirely and appears on every item row instead,
defaulting to `Auto`. The list-wide control remains only as an explicit bulk
default ("apply to all items"), never as the per-item truth.

Item sections, each with its count in the heading and a rows-per-page control
offering 10 / 25 / 50 / 100 / all:

| Section | Availability | Default state |
|---|---|---|
| `non-playable items (n)` | `Unavailable` | expanded |
| `unknown items (n)` | `Unknown` | collapsed |
| `playable items (n)` | `Available` | collapsed |

All three behave identically: per-item media kind, and local-replacement
search / set / clear (today only the flat table offers this).

## Add-on side (sibling repository)

Both official packages already classify each entry: `beeldengeluid.sh` derives
television_episode / movie / music_video / other_video from Schatkamer genre and
series metadata, and the yt-dlp package derives the same set through
`--parse-metadata` / `--replace-in-metadata`. Two gaps remain, tracked as
`addon-list-classification-not-filled-by-every-add-on-path`:

- `libexec/beeldengeluid-list.ps1` hardcodes `content_kind = auto` on its list
  and search record paths where the POSIX runtime derives a value, so the same
  source classifies differently per platform.
- Neither package can express `song` or `image`; the wire contract has no such
  record classification. Leave it that way unless a provider needs it — a wire
  enum change is refused by every older host.

## Work

### 1. Domain and schema — done

- **Correction found while implementing:** `AddonMediaListItem.ContentKind`
  already exists (migration `202608151200`) and `save_list_items` already
  persists whatever each add-on enumerated. No migration is needed. The gap is
  that `list_items` never read the column, the administration surface exposes
  only the list-wide `ContentKindOverride`, and `save_item_nfo` picked the NFO
  root from that list-wide value — so a mixed list wrote every entry under one
  root. The item value now wins for its own NFO root; the editor surface is the
  remaining work.
- `AddonMediaListContentKind` gains `Song` and `Image` so an item can be
  classified for the Songs and Images projections. The frozen add-on wire
  contract has neither, so those two map to `Auto` when a classification is sent
  back to a package.
- `AddonMediaListPresentationKind` grows from `{RemoteStreams, TelevisionShow}`
  to `Auto` plus one variant per `LibraryMediaKind`. Keep the existing two
  discriminants stable so stored rows survive; append the new ones.
- Auto resolution: derive the list presentation from the item kinds when every
  item agrees, and record the derived value rather than leaving `Auto` in the
  projection tables.

### 2. Projections (the bulk of this slice)

ADR 0071 built the Television Show projection: a managed Show/Season/Episode
hierarchy sharing the scanned Remote Stream `MediaVersion`, `tvshow.nfo` plus
per-item NFO ownership, requested-versus-active state, and a revert path that
refuses when a projected item is referenced elsewhere. Five more projections
must follow that same shape — Movies, Music Videos, Other Videos, Songs, Images
— each needing:

- its media tables and metadata rows (`Movie`/`MovieMetadata`,
  `MusicVideo`, `OtherVideo`, `Song`, `Image` and their metadata),
- a link table mirroring `AddonMediaListItemEpisode`,
- NFO ownership for the kinds ErsatzTV writes sidecars for,
- reconciliation, revert guards, and health reporting per kind.

Sequence them one kind per commit; Movies landed first (closest to the Episode
shape) and `reconcile_projection` now dispatches per presentation with a revert
per projection, so each remaining kind adds one apply/revert pair and one link
table. Images last: no duration and no playback version.

### 3. Editor rebuild — done

- Reorder the form; move `Media kind` to items; add the bulk-default control.
- Replace the single table with three collapsible sections sharing one item-table
  component, each with its own page/page-size signals and a persisted page-size
  preference (mirroring the existing `addon_media_lists.page_size`
  `ConfigElement` pattern).
- Server fns take availability + page + page size instead of returning every
  item; add per-section counts so a heading can render without loading rows.
- Keep the hydration rules from TEST.md: no `<Show>` inside an island, prop-
  bearing islands only behind `Resource::new_blocking`.

### 4. Contract, docs, progress

- Record the presentation set and the per-item kind in a new ADR (0073),
  referencing ADR 0071 for the projection mechanics it generalizes.
- Update `docs/reference/addon-media-lists.md`, the parity TOMLs
  (`addon-platform`, `remote-stream-playlists`), `routes.toml` backing for
  `/media/addon-lists/:id`, `hydration.toml`, and `CHANGELOG.md`.
- Resolve the ADR 0072 debt line only when the corresponding projection lands.

## Verification

Per projection: a repository test that projects and reverts, an infra test that
refuses a revert with an outside reference, and a health-check case. For the
editor: page-render assertions for section order, counts, and default collapse
state, plus a Playwright pass that pages each section and sets a replacement
from the playable list. Full gate at closeout as in AGENTS.md.
