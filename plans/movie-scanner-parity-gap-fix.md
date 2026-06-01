# Movie metadata & artwork scanning parity gap fix

## Context

Movies show no cover/fanart images and no detailed description in the admin UI,
while TV shows do — even though both were scanned by the same scanner. The
cause is **not** the display layer. The web UI (`movies.rs`, `get_movie_details`
in `media_browser.rs`, `artwork.rs`) already fully supports movie poster, fanart,
and plot, querying the `Artwork` table by `MovieMetadataId` exactly as shows do.
The DB schema (`MovieMetadata`, `Artwork.MovieMetadataId`) also supports it.

The gap is in the **scanner**:

- TV shows are scanned by a dedicated folder scanner,
  `crates/ersatzrs-scanner/src/local_television.rs`, which parses `tvshow.nfo`
  and locates `poster`/`fanart`/`thumb` artwork, then writes full
  `ShowMetadata` + `Artwork` + genres/tags/studios/actors/guids.
- Movies go through the **generic flat-file path**
  `upsert_local_file()` → `insert_fallback_metadata()` in
  `crates/ersatzrs-scanner/src/lib.rs` (around lines 1052–1267). That path only
  inserts a **title-only fallback** `MovieMetadata` row. It never reads an NFO,
  never looks for artwork, and never populates plot/year/genres/etc.

ErsatzTV's authoritative behavior lives in
`$ERSATZTV_SRC/ErsatzTV.Scanner/Core/Metadata/MovieFolderScanner.cs` and
`.../Nfo/MovieNfo.cs` + `MovieNfoReader.cs`. NFO lookup is `<file>.nfo` then
`movie.nfo`; poster candidates are `["poster", "<file>-poster", "folder"]`,
fanart candidates are `["fanart", "<file>-fanart", "backdrop"]`.

**Decisions (confirmed with user):** Full parity (artwork + plot/outline/year/
content-rating + genres/tags/studios/actors/directors/writers/GUIDs). Enrichment
runs on every scan so already-scanned movie libraries upgrade from fallback to
sidecar in place on the next normal scan.

## Approach

Bring the movie flat-file scan path up to parity with the TV show scanner,
reusing the shared persistence helpers that already exist in
`ersatzrs_infra::metadata` (`replace_artwork`, `replace_strings`,
`replace_people`, `replace_guids`, `artwork_kind`, `ArtworkRef`, `PersonRef`) —
the same ones `local_television.rs` uses. These take a parent-column-name
argument and are already media-type agnostic, so they work with
`"MovieMetadataId"` unchanged.

### 1. Add a `MovieNfo` reader — `crates/ersatzrs-scanner/src/nfo.rs`

Mirror the existing `TvShowNfo` struct + `parse_tvshow_nfo` / `parse_tvshow_nfo_str`
(nfo.rs:80–107, 219–237). Add:

- `pub struct MovieNfo` with: `title`, `sort_title`, `outline`, `plot`, `year`,
  `content_rating`, `premiered`, `genres`, `tags`, `studios`, `actors`
  (`Vec<NfoActor>`), `writers` (`Vec<String>`), `directors` (`Vec<String>`),
  `guids` (`Vec<NfoGuid>`). Field set per `MovieNfo.cs`. (`tagline` is commented
  out in ErsatzTV's MovieNfo — omit it, leave `MovieMetadata.Tagline` NULL.)
- `pub fn parse_movie_nfo(path)` / `pub fn parse_movie_nfo_str(input)` with root
  element `movie`, using the existing `parse_file` / `parse_str` +
  `sanitize_nfo_text` plumbing and a serde `RawMovieNfo` deserialize struct
  shaped like the existing raw tvshow type (follow whatever raw/intermediate
  pattern `parse_tvshow_nfo_str` already uses in this file).

Add a unit test alongside the existing NFO tests parsing a representative
`<movie>` document (plot/outline/year/genre/actor/uniqueid).

### 2. Share the artwork-location helpers

`locate_artwork` (local_television.rs:292), `artwork_ref` (337), and
`IMAGE_EXTENSIONS` (43) are currently private to `local_television.rs`. The movie
path needs them too. Promote them to a shared spot the movie code can call —
simplest is to make them `pub(crate)` and add a small
`locate_movie_artwork(video_path, &candidates)` helper, OR move
`locate_artwork`/`artwork_ref`/`IMAGE_EXTENSIONS` into a new
`crates/ersatzrs-scanner/src/artwork.rs` (or into `nfo.rs`) and re-import from
`local_television.rs`. Prefer `pub(crate)` promotion to keep the diff small; only
move to a new module if that creates awkward coupling.

Movie artwork lookup differs from shows: candidates are folder-relative to the
**video file's** directory and include the file stem, e.g. poster =
`["poster", "<stem>-poster", "folder"]`, fanart =
`["fanart", "<stem>-fanart", "backdrop"]` (per `MovieFolderScanner.LocateArtwork`).
Reuse `locate_artwork(video_path.parent(), &candidates)`.

### 3. Enrich movies in the flat scan path — `crates/ersatzrs-scanner/src/lib.rs`

In `upsert_local_file()` (lib.rs:1052), after the file/version upsert and before
`tx.commit()`, when `library_path.media_kind == LibraryMediaKind::Movies`, call a
new `enrich_movie_metadata(&mut tx, item_id, path)`:

- **Locate NFO:** `<path>.with_extension("nfo")`, then `<dir>/movie.nfo`. Parse
  with `parse_movie_nfo`; tolerate parse errors by logging `warn!` and treating
  as no NFO (match the `nfo::parse_tvshow_nfo` error handling at
  local_television.rs:623).
- **Upsert MovieMetadata — mirror the show path's stable-id update, do NOT use
  `REPLACE INTO`.** `REPLACE INTO` is delete+insert: it churns the
  `MovieMetadata.Id` (= `MovieMetadataId`) on every re-scan, which (a)
  cascade-deletes all child `Artwork`/`Genre`/`Tag`/`Studio`/`Director`/
  `Writer`/`Actor`/`MetadataGuid` rows via their `ON DELETE CASCADE` FKs, and
  (b) invalidates any `metadata_id` captured beforehand. Because enrichment runs
  on every scan (the chosen backfill behavior), this would fire constantly.
  Instead follow `local_television.rs:678–745` exactly:
  1. `SELECT "Id" FROM "MovieMetadata" WHERE "MovieId" = ?` (the fallback row,
     if any).
  2. If present, `UPDATE "MovieMetadata" SET … WHERE "Id" = ?` **in place** so
     the `Id` and all child FK rows stay stable.
  3. If absent, `INSERT` a new row and use the new id.
  4. Capture the resulting `metadata_id` and run all child `replace_*` calls
     (below) against it.
  When an NFO is present, write `MetadataKind = 1` (sidecar) with `Title`,
  `SortTitle`, `OriginalTitle`, `Year`, `ReleaseDate` (from `premiered`),
  `ContentRating`, `Plot`, `Outline`. When no NFO is present, keep current
  fallback behavior (`MetadataKind = 0`, title-only) but still attach any
  artwork found. Reuse the `MovieMetadata` column shape already in
  `upsert_movie_metadata` (`crates/ersatzrs-infra/src/repositories/metadata.rs:47`).
- **Artwork:** locate poster + fanart via the shared helper; build
  `Vec<ArtworkRef>` with `artwork_kind::POSTER` / `artwork_kind::FAN_ART`; call
  `replace_artwork(&mut tx, "MovieMetadataId", metadata_id, &[POSTER, FAN_ART], &artwork)`
  exactly as shows do (local_television.rs:797). This also removes stale artwork
  when a file disappears, matching ErsatzTV's `RemoveArtworkWithKind`.
- **Related collections (full parity):** the schema is confirmed — the `Genre`,
  `Tag`, `Studio`, `Actor`, `Director`, `Writer`, and `MetadataGuid` tables each
  carry a nullable `"MovieMetadataId"` FK with `ON DELETE CASCADE`
  (`migrations/0001_initial.sql`; mirrored in `migrations/mysql/0001_initial.sql`),
  so the existing media-type-agnostic helpers work unchanged against
  `"MovieMetadataId"`. When NFO present, call:
  - `replace_strings` for `"Genre"`, `"Tag"`, `"Studio"`;
  - `replace_people` for `"Actor"` (`actors_from_nfo`), and **also** for
    `"Director"` and `"Writer"` — these are separate tables, not roles in the
    `Actor` table (the show path only uses `"Actor"` because `TvShowNfo` has no
    director/writer lists). Map `MovieNfo.directors` / `.writers` (`Vec<String>`)
    to `Vec<PersonRef>` with the existing `directors_from` helper
    (local_television.rs:362).
  - `replace_guids` for the `uniqueid` rows.
  All keyed on `"MovieMetadataId"`, mirroring local_television.rs:747–779 (and,
  like the show path, call each `replace_*` with an empty slice when no NFO is
  present so stale child rows are cleared on re-scan). The `actors_from_nfo` /
  `directors_from` helpers (local_television.rs:349–364) should be promoted to
  `pub(crate)` or duplicated for reuse.

Note: the existing fallback insert hardcodes `MetadataKind = 1` for the
title-only movie row (lib.rs:1258), which is inconsistent with sidecar=1; the new
code should use `0` for fallback and `1` for sidecar to match ErsatzTV's
`MetadataKind` enum and `local_television.rs`'s `METADATA_KIND_FALLBACK`/`_SIDECAR`.
Fix this as part of the change.

`get_movie_details` already maps content rating by splitting on `/` and reads
`ReleaseDate`/`Tagline`, so no web-side change is required.

## Critical files

- `crates/ersatzrs-scanner/src/nfo.rs` — add `MovieNfo` + `parse_movie_nfo[_str]`.
- `crates/ersatzrs-scanner/src/lib.rs` — `upsert_local_file` (~1052) + new
  `enrich_movie_metadata`; fix fallback `MetadataKind`.
- `crates/ersatzrs-scanner/src/local_television.rs` — promote
  `locate_artwork`/`artwork_ref`/`IMAGE_EXTENSIONS`/`actors_from_nfo`/`directors_from`
  to `pub(crate)` (or relocate to a shared module).
- Reused as-is: `ersatzrs_infra::metadata` (`replace_artwork`, `replace_strings`,
  `replace_people`, `replace_guids`, `artwork_kind`, `ArtworkRef`, `PersonRef`).

## i18n / Language Contract

Per CLAUDE.md's Language Contract, user-facing admin UX must be localized in
every supported culture (`crates/ersatzrs-web/src/admin/languages/`: `en_us`,
`nl`, `pl`, `pt_br`) whenever it is built or changed. This change is **already
compliant with no new translation work**, verified as follows:

- The movie detail page (`movies.rs:190–217`) already renders every full-parity
  section — Studios, Directors, Writers, Genres, Tags, Content Ratings,
  Languages, Actors, plus the FanArt `alt` — through localized
  `tr(TextKey::…)` labels. The sections are present today but empty because the
  scanner never populates the data this plan adds.
- All nine `TextKey`s used by those sections (`Studios`, `Directors`, `Writers`,
  `Genres`, `Tags`, `ContentRatings`, `Languages`, `Actors`, `FanArt`) are
  already translated in **all four** language files. Confirmed present in
  `en_us.rs`, `nl.rs`, `pl.rs`, `pt_br.rs`.
- Full parity therefore surfaces **no new user-facing string**, so no
  `i18n.rs` / `languages/*` edits are required. This is why the "no web-side
  change is required" claim above holds: the data path *and* the localized
  chrome already exist.
- The metadata **values** (genre names, actor names, plot/outline text) come
  from the NFO sidecar and are data, not chrome — they are intentionally not
  translated, matching show/season behavior.
- Scanner-side strings introduced by this change (e.g. `warn!` on NFO parse
  failure) are repository-bound English per the Language Contract and need no
  localization.

If the implementation diverges from the plan and adds a *new* visible label
(for example a "Countries" fact row, since `MovieNfo` carries `countries`),
that label must get a new `TextKey` translated in all four cultures before
merge. The current plan does not add such a row.

## Health-check note

This changes movie metadata-persistence parity. Per CLAUDE.md's health-check
contract, check whether a movie metadata/artwork health check exists in
`ersatzrs-core::health` / `ersatzrs-infra::health` and update/add one if the
existing checks cover show metadata but not movies.

## Verification

1. `cargo build --workspace`.
2. `cargo fmt --all -- --check` and clippy matching CI:
   `cargo clippy --workspace --all-targets -- -D warnings` **and** a separate
   `--features mysql` pass.
3. Unit test: new `parse_movie_nfo_str` test in `nfo.rs`.
4. Scanner test: add/extend a scanner integration test (alongside the existing
   television test) with a temp movie folder containing `Film (2020).mkv`,
   `Film (2020).nfo` (plot/genre/actor/director/writer/uniqueid), `poster.jpg`,
   `fanart.jpg`; run the local scan and assert `MovieMetadata.Plot`/`Year` and
   two `Artwork` rows (`MovieMetadataId`, kinds 0 and 3) plus genre/actor/
   director/writer rows. Mind the `LD_LIBRARY_PATH`/ffmpeg-8.1 requirement noted
   for ignored scanner integration tests. Add three more cases:
   - **No NFO, artwork only:** folder with `poster.jpg`/`fanart.jpg` but no NFO
     — assert artwork attaches and the row stays `MetadataKind = 0` (fallback).
   - **Malformed NFO:** assert the scan logs a `warn!` and completes (treated as
     no NFO) rather than failing.
   - **Re-scan idempotency (guards the no-`REPLACE` requirement):** scan the
     NFO+artwork folder twice and assert exactly one `MovieMetadata` row with a
     stable `Id`, and no duplicated or orphaned `Artwork`/`Genre`/`Actor`/
     `Director`/`Writer` rows. This is the regression that would catch a
     `REPLACE INTO` cascade bug.
5. End-to-end: re-scan the real movie library
   (`cargo run --bin ersatzrs-scanner -- --provider local --library-id <movies>`),
   start `ersatzrs-web`, open a movie detail page, confirm poster, fanart, and
   description now render.
