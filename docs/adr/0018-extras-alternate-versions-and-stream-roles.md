# ADR 0018: Special features, alternate versions, and stream roles

## Status

Accepted 2026-06-28

<!--
Status records the DECISION lifecycle only: Proposed | Accepted | Superseded
by ADR NNNN | Deprecated. It must NOT be read as "the code exists".
-->

## Implementation

Implemented — the local/provider scanner/search/UI/playback/scheduling scope
tracked by `docs/progress/parity/content-variants.toml` is implemented through
Phase 5 plus provider-side Plex/Jellyfin/Emby content-variant import, and the
2026-07-01 movie-detail follow-up now surfaces alternate-edition cards plus
commentary audio/subtitle playback options on `/media/movies/:id`. A
2026-06-27 live admin dogfood pass also proved the documented
`content_kind:alternate_version AND sub_kind:unplugged` selector against
admin-triggered Music Videos scans after the scanner learned to retry SQLite
busy/locked write contention at the per-write import/link/finalization boundary.
That pass created an MTV Unplugged smart collection, channel, Classic schedule,
and playout from the admin UI, with the existing MTV watermark and a bumper
after every three unplugged videos.

<!--
Tracked by `plans/extras-alternate-versions-and-stream-roles.md`. Parity slug
to be created at first implementation slice: `content-variants`.
-->

## Context

ErsatzTV's library model recognizes a fixed set of *primary* media kinds
(movie, show/season/episode, music video, other video, song, image, remote
stream). It has no concept of a media item being **subordinate to** or an
**alternate rendition of** another item. Three real capabilities fall through
that gap, and a maintainer request asks ErsatzRS to close them:

1. **Special features (extras).** Trailers, behind-the-scenes, deleted scenes,
   featurettes, interviews, scenes, and shorts that belong to a movie or show.
   ErsatzTV does not merely lack management for these — it **actively discards
   them at scan time**. `LocalFolderScanner` defines `ExtraFiles`
   (filename suffixes: `behindthescenes`, `deleted`, `featurette`, `interview`,
   `scene`, `short`, `trailer`, `other`) and `ExtraDirectories` (`behind the
   scenes/`, `deleted scenes/`, `featurettes/`, `interviews/`, `scenes/`,
   `shorts/`, `trailers/`, `other/`, `extras/`, `specials/`), and
   `MovieFolderScanner` filters those files out so they never become media
   items (`$ERSATZTV_SRC/ErsatzTV.Scanner/Core/Metadata/LocalFolderScanner.cs`,
   `MovieFolderScanner.cs`). No Plex/Jellyfin/Emby scanner imports them either.
   Extras are therefore invisible: not browsable, not searchable, not
   schedulable.

2. **Alternate versions.** A music video or song frequently exists in several
   renditions of the same work — official video, live, acoustic, "MTV
   Unplugged", remix, remaster — and a movie may exist as several cuts. ErsatzTV
   imports each rendition as an independent `MusicVideo`/`Movie` with **no
   relationship** to the others; the only discriminator is free text in the
   title or folder-derived tags. A maintainer cannot, for example, build an "MTV
   Unplugged" collection that selects the unplugged rendition instead of the
   studio one without hand-curating it.

3. **Stream roles (commentary).** A movie file often carries a secondary audio
   stream (director's commentary) and matching commentary subtitles. ErsatzTV
   already handles this on the **playback** side: `Channel.PreferredAudioTitle`
   and the `CustomStreamSelector` `AudioTitleAllowlist` / `AudioTitleBlocklist`
   / `SubtitleTitleAllowlist` / `SubtitleTitleBlocklist` select an audio or
   subtitle stream by title substring at transcode time
   (`$ERSATZTV_SRC/ErsatzTV.Core/FFmpeg/CustomStreamSelector.cs`,
   `Domain/Channel.cs`). ErsatzRS already carries the channel columns and a
   partial selector port. What is missing is **selection**: stream titles are
   not indexed, so there is no way to build a smart collection of "items that
   *have* a commentary track" to point such a channel at.

Worked maintainer use cases that motivate this ADR:

- An "MTV Unplugged" smart collection that selects the unplugged renditions
  rather than the originals.
- A movie channel that plays only commentary audio (and commentary subtitles
  where present).
- A schedule that plays one or more featurettes before the main movie.

The common thread is that all three are **non-default renditions** that today
cannot travel through the existing *search → smart collection → playlist →
channel/filler* pipeline. The porting rule (ARCHITECTURE/INDEX locked decision:
"ErsatzTV behavior by default; deliberate deviations require ADRs") means this
enhancement, which has no ErsatzTV equivalent, must be recorded as a deliberate,
parity-safe divergence — this ADR.

Constraints:

- **Parity-safe and additive.** Existing ErsatzTV JSON shapes, table/column
  casing, `PK_/FK_/IX_` names, and the established search field names must not
  change. New persisted state must default existing rows to today's behavior.
- **Default-invisible outside media management.** Importing extras/versions must
  not silently change what existing channels, playlists, manual/smart
  collections, searches, or schedules select. ErsatzTV behaves as if only
  primaries exist there; that must remain the default. Admin media browse pages
  are the management exception and intentionally show variants so users can see
  what was imported.
- **Single-binary, in-process search** (ADR 0009): the selection surface is the
  existing SQL-backed Lucene-like search, not a new service.

## Decision

Introduce a single unifying concept — **content variants and stream roles** —
across three cooperating layers, all selectable through the existing search
contract and all default-excluded from primary browsing and scheduling.

### 1. Item variants — structured, on `MediaItem`

Add three additive columns to the shared `MediaItem` base table (the same place
ErsatzTV keeps the cross-cutting `State`):

- `ContentKind INTEGER NOT NULL DEFAULT 0` — a closed ErsatzRS enum:
  `Primary = 0`, `Extra = 1`, `AlternateVersion = 2`.
- `SubKind TEXT NULL` — an **open, normalized lowercase token** describing the
  subtype, scoped by `ContentKind`. Canonical (non-exhaustive) vocabulary:
  - Extra: `trailer`, `behind_the_scenes`, `deleted_scene`, `featurette`,
    `interview`, `scene`, `short`, `other` (mirrors ErsatzTV's
    `ExtraFiles`/`ExtraDirectories` taxonomy, which is the Kodi/Plex/Jellyfin
    convention).
  - AlternateVersion: `official_video`, `live`, `unplugged`, `acoustic`,
    `remix`, `remaster`, `extended`, `directors_cut`, `other`.
- `ParentMediaItemId INTEGER NULL` — self-referential FK to `MediaItem(Id)`
  with `ON DELETE SET NULL`, pointing an extra/version at the primary item it
  belongs to (a featurette → its movie; an unplugged rendition → the canonical
  music video/song).

A closed integer enum for `ContentKind` keeps the structural dimension typed and
cheap to filter; an open string token for `SubKind` lets new subtypes
(`unplugged`, `acoustic`, …) arrive from folder/NFO conventions without a
migration. Because `ContentKind` defaults to `Primary` and `ParentMediaItemId`
defaults to `NULL`, **every existing row is unchanged** and the schema stays
ErsatzTV-shape-compatible.

**Storage kind of an extra/version is its natural media kind**, not a new table:
movie/show extras are stored as `OtherVideo` rows (they are, literally, other
video files) marked `ContentKind = Extra` with a parent; music alternate
versions are stored as `MusicVideo`/`Song` rows marked `AlternateVersion` with a
parent. No new media-kind table is introduced.

### 2. Stream roles — index existing stream titles

No new stream storage is required (`MediaStream.Title` and `Subtitle.Title`
already exist and are populated by ffprobe/provider sync). Add a **projection
and search** layer:

- Index audio stream titles and subtitle titles into the search document.
- Derive a conservative **role classification** from those titles (initially
  `commentary`, matched by case-insensitive substring as ErsatzTV's selector
  does), exposed as a convenience predicate.

This is the *selection* half. The *render* half already exists in ErsatzTV
parity: `Channel.PreferredAudioTitle` and the custom stream selector
allow/blocklists pick the commentary stream at transcode time. The two halves
compose: a smart collection finds the items, the channel's stream selector
plays the right track.

### 3. Search contract additions

Add to the core search field set (`ersatzrs-core::search`), the field-name
parser, the infra projection, and the matcher — with snapshot/unit coverage:

| Token | Backed by | Example |
| --- | --- | --- |
| `content_kind` | `MediaItem.ContentKind` (`primary`/`extra`/`alternate_version`) | `content_kind:extra` |
| `sub_kind` | `MediaItem.SubKind` token | `sub_kind:featurette`, `sub_kind:unplugged` |
| `parent_title` | parent item's title | `parent_title:"The Matrix"` |
| `audio_title` | `MediaStream.Title` (audio) | `audio_title:commentary` |
| `subtitle_title` | `Subtitle.Title` | `subtitle_title:commentary` |

`commentary` is additionally exposed as a derived convenience (`has_commentary`,
true when an audio or subtitle title classifies as commentary) so queries are
robust to title spelling variations.

### 4. Default-exclusion keystone

Search-query evaluation implicitly adds `content_kind:primary` **unless** the
query already references `content_kind`, `sub_kind`, or `parent_title`. This
rule is the parity-safety mechanism:
extras and alternate versions stay invisible to every existing primary-only
collection and channel, exactly as if ErsatzTV had skipped them, while an
author who *wants* them opts in explicitly. Manual collections (which reference
items by id) are unaffected. Media browse server functions retain an explicit
variant-inclusive flag for callers that need management visibility; admin media
browse pages pass that flag and always show variants.

### 5. UI and scheduling consumers

- `/media/*` browse pages always include special features / versions and show a
  badge on variant cards.
- Movie/show/season detail pages gain an **Extras / Special Features** section
  grouped by `SubKind`; TV show/season specials render as episode-style rows,
  while music-video/song/artist detail pages gain a **Versions** section. Each
  item supports the usual add-to-collection/playlist actions.
- The smart-collection / playlist search field reference (and any in-UI help)
  documents the new tokens.
- Extras become schedulable as **pre-roll filler** (featurette before a movie)
  by pointing a filler preset at an extras smart collection; commentary channels
  combine an `audio_title:commentary` smart collection with the channel stream
  selector.

### 6. Scanner

Replace ErsatzTV's *discard* with *import-and-classify* (local first;
Plex/Jellyfin/Emby later where the provider exposes extras/version data). Files
matching `ExtraFiles`/`ExtraDirectories` are imported, classified into
`ContentKind`/`SubKind`, and linked to the owning movie/show via folder
ownership; music alternate versions are detected from title/folder tokens and
linked to a canonical primary. Default-exclusion guarantees this does not alter
existing channels.

### 7. Health check

Extend the search/index readiness check (Health Check Parity contract) to assert
the projection includes the variant and stream-role fields, so the report stays
an accurate port-completeness indicator.

## Consequences

- **New capability ErsatzTV lacks**: extras, alternate versions, and
  commentary-bearing items become browsable, searchable, and schedulable through
  one consistent pipeline; the three worked use cases become expressible as smart
  collections plus existing channel/filler features.
- **Parity preserved by default**: additive columns default existing rows to
  `Primary`/no-parent; the implicit `content_kind:primary` filter keeps existing
  collections, channels, searches, schedules, and playouts byte-for-byte
  unchanged until an author opts in. Admin media browse pages intentionally show
  variants for management visibility.
- **Bounded schema change**: one migration (`MediaItem` + indexes; SQLite and
  MySQL variants) under the "collapse then evolve" rule (ADR 0004). No new
  media-kind table; extras/versions reuse `OtherVideo`/`MusicVideo`/`Song`.
- **Scanner divergence is documented**: the local scanner stops discarding
  extras. This is a deliberate behavior change from ErsatzTV, recorded here and
  tracked as visible work in the `content-variants` parity slice.
- **Costs / follow-ups**: scanner parent-resolution and version-detection
  heuristics need tests against real folder layouts; provider (Plex/Jellyfin/
  Emby) extras import is deferred until the API surface is mapped; the derived
  `commentary` classifier starts deliberately conservative; new UI strings must
  be localized across all four cultures (Language Contract); the stream-role
  *playback* half depends on completing the channel custom-stream-selector port.
- **Search cost**: a few more indexed multi-valued fields per document; within
  the in-process SQL-backed model (ADR 0009), no new service.

## Alternatives Considered

- **Tag-only (no schema change).** Import extras/versions as ordinary items and
  mark them with reserved tags (`extra`, `live`, …), reusing the existing
  folder→tag precedent and `tag:` search. Rejected as the primary model: tags
  are free text and collide with genuine user tags, give no typed structural
  parent link, and force every existing query to remember `NOT tag:extra` to
  stay correct — the opposite of a safe default. (Canonical tokens are still
  emitted as a convenience, but the source of truth is the structured field.)
- **Dedicated `Extra` / `Version` media-kind tables.** A first-class table per
  concept. Rejected as too invasive for the collapsed schema and redundant:
  `OtherVideo`/`MusicVideo`/`Song` already model the file; only the
  kind/parent/sub-kind triple is new, and it belongs on `MediaItem`.
- **Put the triple on metadata rows instead of `MediaItem`.** Rejected:
  variant identity and parent linkage are intrinsic to the item and cross-cut
  metadata revisions; the search projection and scheduling enumerators key off
  `MediaItem`, mirroring how `State` is modeled.
- **Model commentary as an item variant.** Rejected: commentary is an alternate
  *stream within the same file*, not a separate item. The correct lever is
  stream-title indexing for selection plus ErsatzTV's existing
  `PreferredAudioTitle`/custom-stream-selector for playback.
- **No default-exclusion (extras visible everywhere).** Rejected: it would
  silently inject trailers/featurettes into every existing "all movies"
  collection and channel, breaking parity the moment a library is rescanned.

## References

- `plans/extras-alternate-versions-and-stream-roles.md` — implementation plan.
- `docs/reference/content-variant-scheduling-examples.md` — operator examples
  for unplugged renditions, commentary channels, and featurette pre-roll.
- [ADR 0004: collapsed initial schema](0004-collapsed-initial-schema.md) —
  "collapse then evolve" migration rule this change follows.
- [ADR 0009: SQL-backed search](0009-sql-backed-search-and-no-elasticsearch.md)
  — the in-process search surface being extended.
- `crates/ersatzrs-core/src/search.rs` — search field set, parser, matcher.
- `crates/ersatzrs-infra/src/repositories/scheduling.rs` — `SearchDocument`
  projection.
- `$ERSATZTV_SRC/ErsatzTV.Scanner/Core/Metadata/LocalFolderScanner.cs` —
  `ExtraFiles` / `ExtraDirectories` (today's discard lists).
- `$ERSATZTV_SRC/ErsatzTV.Scanner/Core/Metadata/MovieFolderScanner.cs` —
  extras-filter call site.
- `$ERSATZTV_SRC/ErsatzTV.Scanner/Core/Metadata/LocalMetadataProvider.cs` —
  folder→tag precedent (`RefreshSidecarMetadata(OtherVideo, …)`).
- `$ERSATZTV_SRC/ErsatzTV.Core/FFmpeg/CustomStreamSelector.cs`,
  `$ERSATZTV_SRC/ErsatzTV.Core/Domain/Channel.cs` — existing stream-title
  selection (the playback half of the commentary use case).
- `$ERSATZTV_SRC/ErsatzTV.Infrastructure/Search/LuceneSearchIndex.cs` — field
  conventions the new tokens follow.
