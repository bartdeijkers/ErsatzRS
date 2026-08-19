# Add-on list artwork and full metadata projection

Two slices, found by inspecting a real imported list rather than the code.
Nothing implemented yet. Both are follow-ups to ADR 0073.

## Evidence

Looked at `~/.ersatzrs/beeldengeluid_media/Teleac/streams/` after a real
Schatkamer import: 37 `.nfo`, 37 `.yml`, 4 state `.json`, and **no image files
at all**.

One of those NFO documents is rich — plot, `showtitle`, season, episode, year,
`aired`, `mpaa`, one `genre`, four `tag`, two `studio`, three `actor` with
roles, a `director`, broadcasters, collection, a `thumb` URL, and two
`uniqueid` values. So the add-ons are scraping plenty; the loss happens after.

## Slice 1 — write artwork to disk

The chain stops one step short:

| Step | State |
|---|---|
| Add-on emits `additional_image_urls` | works |
| Host stores them in `AddonMediaListItem.AdditionalImageUrls` | works (`addon_media_list_sync.rs:389`, `:1321`) |
| Written as `<thumb>` into the NFO and the generated YAML | works (`remote_stream_playlist.rs:1232`) |
| Scanner turns those URLs into `Artwork` rows | works (`ersatzrs-scanner/src/lib.rs:2145`) |
| **Something fetches the image to disk** | **missing** |

The irony is that a complete sidecar path already exists and nobody ever puts a
file in it: `locate_remote_stream_artwork` (`ersatzrs-scanner/src/lib.rs:2499`)
looks for `<stem>.jpg|jpeg|png|webp` next to each definition and uses it as the
thumbnail. The local scanner goes further and already distinguishes
poster/thumb/fanart by suffix for artists, music videos, and songs
(`lib.rs:1236`, `:1554`, `:1677`).

**The work:** fetch the first artwork URL when a list is imported or refreshed
and write it beside the `.yml`, using the naming the scanner already looks for.
Then the existing chain does the rest.

Decisions to make while implementing:

- **Naming.** `<stem>.jpg` satisfies the remote-stream lookup. Kodi-style
  `<stem>-poster.jpg` / `-fanart.jpg` / `-thumb.jpg` matches what the local
  scanner distinguishes, and is what a Kodi-compatible folder should look like.
  Probably: first image → `-thumb`, additional → `-fanart`, list-level → folder
  `poster.jpg`.
- **Bounds.** This is an unattended fetch of an operator-supplied URL. It needs
  the same discipline as the add-on download path: size cap, timeout, content
  type check, and refusal to write outside the managed folder.
- **Idempotence.** A refresh must not re-download unchanged images. The item
  already has a stable provider id; a content hash or a conditional request is
  the cheap option.
- **Ownership.** ADR 0068 gives the operator the right to replace managed files.
  An artwork file the operator swapped must not be overwritten on refresh, the
  same way an adopted NFO is not.
- **Failure.** One unreachable image must not fail an import of 37 items.
  Record and continue, and surface it the way other per-item problems are.

## Slice 2 — project the metadata that is already captured

Every projection writes exactly one metadata row and nothing else. Across the
whole projection module:

```
INSERT INTO "Artwork"  → 0
INSERT INTO "Genre"    → 0
INSERT INTO "Tag"      → 0
INSERT INTO "Studio"   → 0
INSERT INTO "Actor"    → 0
```

So genre, tags, studios, actors, directors, and artwork sit in the NFO and never
reach `EpisodeMetadata` / `MovieMetadata` / `MusicVideoMetadata` / `SongMetadata`
/ `ImageMetadata`. Searching by genre, filtering by actor, and artwork in the UI
therefore do not work for add-on lists even though the data exists on disk.

**This omission was inherited and then multiplied.** The television projection
(ADR 0071) never wrote those tables; the four projections added on 2026-08-19
were modelled on it and copied the gap four more times without anyone noticing.

**The work:** after writing the metadata row, write the related rows from the
same NFO metadata, for every presentation. `AddonMediaListNfoMetadata` already
carries genres, tags, studios, and people — `crates/ersatzrs-core/src/repositories/addon_media_list.rs`
holds the shape, and `addon_media_nfo.rs` already parses them, since they round
trip through the file.

Points to get right:

- **One implementation, not seven.** `FlatProjection` already describes the
  parentless kinds; the related-row writer should hang off the same description
  plus the two parented kinds, or the same omission will be copied again.
- **Column reality per kind.** The metadata tables differ, exactly as they did
  for the description columns: check which of Genre/Tag/Studio/Actor actually
  have a foreign key for each metadata kind before assuming symmetry.
- **Replace, do not accumulate.** The projection already deletes and re-inserts
  its metadata row; related rows need the same treatment or a refresh will
  duplicate every genre.
- **Artwork rows depend on slice 1** for a local path, but can be written from
  the URL in the meantime — the scanner already does this for remote streams, so
  check whether projecting artwork duplicates what the scan already produced
  rather than adding it twice.

## Verification

For both: import a real Schatkamer list, then assert against the database rather
than the log. A projected episode should end up with its genres, tags, studios,
and actors, and an artwork row whose path points at a file that exists. The
existing projection tests in `crates/ersatzrs-infra/tests/addon_media_list_tv_projection.rs`
are the right place; they already seed a managed list with on-disk NFO files.

Full gate as in AGENTS.md, including the second nextest configuration.

## Pending work in the tree when this plan was written

Uncommitted in ErsatzRS, all gated except the last:

- `adopt_derived_presentation` — a newly imported list now adopts the
  presentation its entries imply instead of being hard-set to Remote Streams; a
  projection the import cannot finish is recorded as `PresentationError` rather
  than failing the import. Plus its test in `tests/addon_media_list_sync.rs`.
- Removal of the `addon-list-classification-not-filled-by-every-add-on-path`
  debt. It was wrong: both runtimes derive `content_kind` identically
  (`beeldengeluid.sh:467`, `beeldengeluid-list.ps1:275`) and both emit `auto`
  only on the same fallback path, where the metadata request failed and there is
  no HTML to classify.
- `plans/channel-packages.md` and its INDEX entry.

Uncommitted in the sibling `ErsatzRS-addons` repository:

- Beeld & Geluid → manifest v4, package 0.6.0,
  `default_source_hosts = ["schatkamer.beeldengeluid.nl"]`. The CDN host is
  deliberately not claimed: it is contacted for media, never pasted as a source.
- yt-dlp → manifest v4, package 0.5.0,
  `default_source_hosts = ["youtube.com", "*.youtube.com", "youtu.be"]`.
  Playback stays generic, but enumeration rebuilds each flat-playlist entry as a
  youtube.com watch URL (`addon.sh:101`), so YouTube is all it can honestly
  claim.
- Both validate with `ersatzrs-addon-validator` (exit 0). That repository's own
  `tests/test_repository.py` and `tools/validator` have **not** been run yet.
