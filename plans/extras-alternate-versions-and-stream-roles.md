# Special Features, Alternate Versions, and Stream Roles

> **Status: Ported for the local and provider scanner/search/UI/playback/scheduling scope.** Implements
> [ADR 0018](../docs/adr/0018-extras-alternate-versions-and-stream-roles.md).
> Deliberate, parity-safe enhancement with no ErsatzTV equivalent. Live state
> lives in `docs/progress/parity/content-variants.toml`. The 2026-07-01
> movie-detail follow-up restored `/media/movies/:id` extras rows, alternate
> movie edition cards, and commentary audio/subtitle playback options.

## Goal

Let extras (special features), alternate versions, and commentary-bearing items
travel through the existing **search → smart collection → playlist →
channel/filler** pipeline, while leaving every existing channel, collection, and
browse page unchanged by default.

Three maintainer use cases are the acceptance yardstick:

- **U1 — "MTV Unplugged" collection.** A smart collection that selects the
  unplugged renditions instead of the studio originals.
- **U2 — Commentary-only movie channel.** A channel that plays only commentary
  audio, plus commentary subtitles where present.
- **U3 — Featurette before the movie.** A schedule that plays one or more
  featurettes ahead of the main movie.

## Source Map

- **ErsatzTV discard lists (today's behavior to reverse):**
  `$ERSATZTV_SRC/ErsatzTV.Scanner/Core/Metadata/LocalFolderScanner.cs`
  (`ExtraFiles`, `ExtraDirectories`),
  `MovieFolderScanner.cs` (filter call site ~line 133),
  `TelevisionFolderScanner.cs`, `OtherVideoFolderScanner.cs`,
  `MusicVideoFolderScanner.cs`.
- **ErsatzTV folder→tag precedent:**
  `LocalMetadataProvider.cs` `RefreshSidecarMetadata(OtherVideo, …)`.
- **ErsatzTV stream selection (playback half of U2, already parity-tracked):**
  `$ERSATZTV_SRC/ErsatzTV.Core/FFmpeg/CustomStreamSelector.cs`,
  `FFmpegStreamSelector.cs`, `Domain/Channel.cs`
  (`PreferredAudioTitle`, `StreamSelector`, `StreamSelectorMode`,
  `SubtitleMode`), `Domain/ChannelStreamSelectorMode.cs`,
  `Domain/ChannelSubtitleMode.cs`.
- **ErsatzTV search field conventions:**
  `$ERSATZTV_SRC/ErsatzTV.Infrastructure/Search/LuceneSearchIndex.cs`.
- **ErsatzRS targets:**
  - Schema: `migrations/0004_content_variants.sql` (+ `migrations/mysql/`).
  - Domain/search: `crates/ersatzrs-core/src/search.rs` (`SearchField`,
    `SearchDocument`, parser, matcher).
  - Projection: `crates/ersatzrs-infra/src/repositories/scheduling.rs`
    (`SearchDocument` build), plus stream-title joins.
  - Scanner: `crates/ersatzrs-scanner/src/lib.rs`,
    `local_television.rs`, `library_folder.rs`, `nfo.rs`.
  - Provider import: `crates/ersatzrs-domain/src/content_variants.rs`,
    `crates/ersatzrs-infra/src/media_server.rs`, `plex.rs`, `jellyfin.rs`,
    `emby.rs`.
  - UI: `crates/ersatzrs-web/src/admin/pages/media/*`
    (`movies.rs`, `shows.rs`, `season_detail.rs`, music video / artist /
    songs / other_videos pages, `smart_collections.rs`),
    `crates/ersatzrs-web/src/admin/server_fns/media_browser.rs`,
    `admin/routes.rs`, `admin/languages/*` (four cultures), `admin/i18n.rs`.
  - Health: `ersatzrs-core::health`, `ersatzrs-infra::health`.
  - Channel stream selector (U2 render half): `crates/ersatzrs-infra/src/playback.rs`,
    `condition.rs`, `repositories/channel.rs`.
  - Phase 5 examples: `crates/ersatzrs-core/src/content_variants.rs`,
    `crates/ersatzrs-infra/tests/scheduling_repository.rs`, and
    `docs/how-to/schedule-content-variants.md`.

## Data Model

Additive columns on `MediaItem` (defaults preserve every existing row):

```sql
ALTER TABLE "MediaItem" ADD COLUMN "ContentKind" INTEGER NOT NULL DEFAULT 0; -- 0 Primary, 1 Extra, 2 AlternateVersion
ALTER TABLE "MediaItem" ADD COLUMN "SubKind" TEXT NULL;                      -- normalized lowercase token
ALTER TABLE "MediaItem" ADD COLUMN "ParentMediaItemId" INTEGER NULL;         -- self-FK, ON DELETE SET NULL
-- + IX_MediaItem_ParentMediaItemId, IX_MediaItem_ContentKind
```

- `ContentKind` enum (ErsatzRS-original; closed): `Primary`/`Extra`/`AlternateVersion`.
- `SubKind` token vocabulary (open; normalized): extras →
  `trailer`/`behind_the_scenes`/`deleted_scene`/`featurette`/`interview`/
  `scene`/`short`/`other`; versions →
  `official_video`/`live`/`unplugged`/`acoustic`/`remix`/`remaster`/`extended`/
  `directors_cut`/`other`.
- Storage kind reuses existing tables: film/show extras → `OtherVideo` rows;
  music alternate versions → `MusicVideo`/`Song` rows. No new media-kind table.
- Stream roles add **no** columns: index existing `MediaStream.Title` /
  `Subtitle.Title`; derive `commentary` by case-insensitive substring.

## Search Contract

New tokens (parser + `SearchField` + projection + matcher + tests):

- `content_kind` → `primary` | `extra` | `alternate_version`
- `sub_kind` → open token
- `parent_title` → parent primary's title
- `audio_title` → audio `MediaStream.Title`
- `subtitle_title` → `Subtitle.Title`
- derived `has_commentary` (true when an audio/subtitle title classifies as
  commentary)

**Default-exclusion keystone:** query evaluation and `/media/*` listings add an
implicit `content_kind:primary` unless the query references `content_kind`,
`sub_kind`, or `parent_title`. Implement once, centrally, so every consumer
(smart collections, browse, scheduling enumerators) inherits it. Manual
collections (id-referenced) are untouched.

## Phases

Each phase is independently shippable and leaves the tree green.

### Phase 0 — Foundation (schema + search contract)
**Status: complete.**

- Migration `0004_content_variants.sql` (+ MySQL variant); confirm collapsed
  schema parity rules and EF-style names.
- Core domain: `ContentKind` enum, `SubKind` normalizer (canonical token map),
  search `SearchField` variants + parser tokens + matcher.
- Infra projection: populate `content_kind`, `sub_kind`, `parent_title`,
  `audio_title`, `subtitle_title` in `SearchDocument`; add the derived
  commentary classifier.
- Central default-exclusion (`content_kind:primary`) in query evaluation.
- Tests: `insta`/unit coverage for parser, matcher, default-exclusion, and
  projection; round-trip repository test.
- No UI, no scanner change yet. Existing behavior provably unchanged.

### Phase 1 — `/media/*` UI surfacing
**Status: complete.**

- Browse pages default to primaries; persisted "Show special features /
  versions" toggle (pattern: playouts `Show Filler`); variant badge on cards.
- Detail pages: **Extras / Special Features** section (movie/show/season,
  grouped by `SubKind`) and **Versions** section (music video/song/artist),
  with standard add-to-collection/playlist actions.
- Smart-collection / playlist search-field reference documents new tokens.
- i18n strings for all four cultures; hydration rules per TEST.md.

### Phase 2 — Local scanner: import + classify extras (reverse the discard)
**Status: complete for local libraries.**

- Replace the `ExtraFiles`/`ExtraDirectories` *skip* with *import-and-classify*
  for movie/show libraries; store as `OtherVideo` with `ContentKind=Extra`,
  `SubKind`, and `ParentMediaItemId` resolved by folder ownership.
- Library-level/orphan extras keep `ParentMediaItemId = NULL` (still typed,
  still searchable).
- Tests against representative folder layouts (sibling `-trailer` files and
  `Featurettes/` subfolders). Verify default-exclusion keeps existing channels
  unchanged after a rescan.

### Phase 3 — Alternate-version linking (music videos / songs) → **U1**
**Status: complete for local libraries and provider movie libraries.**

- Detect version tokens from title/folder (`(Live)`, `- Live`, `/Unplugged/`,
  `(Acoustic)`, `(Official Video)`, "MTV Unplugged", …); normalize to `SubKind`.
- Link renditions of the same work (same artist + matched base title) to a
  canonical primary (official/studio, else first-seen) via `ParentMediaItemId`.
- Provider movie-library sync for Plex/Jellyfin/Emby reuses the same classifier
  after the normal sync page, including ETag-skipped rows, so provider alternates
  become hidden/searchable `AlternateVersion` rows without requiring a full
  rewrite of every media row.
- Acceptance: an "MTV Unplugged" smart collection
  (`content_kind:alternate_version AND sub_kind:unplugged`, or
  `album:"MTV Unplugged"` for songs) resolves to unplugged renditions only.

### Phase 4 — Stream roles (commentary) → **U2**
**Status: complete.**

- Index audio/subtitle titles (Phase 0 projection) and finalize the
  `commentary` classifier; expose `audio_title`/`subtitle_title`/
  `has_commentary`.
- Verify/complete the channel render half: `Channel.PreferredAudioTitle` and
  `CustomStreamSelector` `AudioTitleAllowlist`/`SubtitleTitleAllowlist`
  (+ `SubtitleMode`) — record any remaining selector-port gap as ParityDebt.
- Acceptance: a `type:movie AND audio_title:commentary` smart collection feeds a
  channel whose stream selector pins the commentary audio + subtitles.

### Phase 5 — Scheduling consumers → **U3** + examples
**Status: complete.**

- Featurette-before-movie: a pre-roll filler preset pointed at an extras smart
  collection (`content_kind:extra AND sub_kind:featurette`, optionally
  `parent_title:"…"`), attached as `PreRoll` filler on the movie schedule item.
- Ship example/templated smart collections for U1–U3 and document them.
- Evidence: `ersatzrs-core::content_variants` ships examples and builders for
  unplugged renditions, commentary movies, featurette pre-roll filler, and
  primary movie schedule items. `scheduling_repository_content_variant_examples_resolve_and_drive_featurette_preroll`
  proves the saved example smart collections resolve intended rows, keep
  extras out of primary movie schedules by default, persist the pre-roll
  filler, and feed Classic playout inputs.

### Cross-cutting
- Plex/Jellyfin/Emby provider movie-library sync now classifies server-reported
  extras from title/path/tag signals and writes `ContentKind=Extra` plus
  normalized `SubKind`.
- Health check extension (search projection includes variant + stream-role
  fields).
- `docs/progress/parity/content-variants.toml` created at Phase 0; updated each
  slice; record the deliberate scanner divergence as visible work.
- CHANGELOG `[Unreleased]` (Added) entries at the phase that makes each
  capability user-visible.
- Platform parity: no OS/arch assumptions; scanner path handling stays
  cross-platform.

## Acceptance

- [x] U1: unplugged smart collection selects only alternate `unplugged`
      renditions; originals excluded.
- [x] U2: commentary smart collection selects items with a commentary audio
      track; paired channel plays commentary audio (+ subtitles where present).
- [x] U3: a featurette plays before its movie via pre-roll filler.
- [x] Default-exclusion proven: existing channels/collections/browse pages are
      unchanged after importing extras/versions (regression test + rescan).
- [x] New search tokens parse, match, project, and snapshot-test cleanly.
- [x] `/media/*` browse + detail surfaces match the toggle/section design; i18n
      complete for four cultures; hydration-clean.
- [x] Health check reports variant/stream-role index readiness.
- [x] Schema migration applies on SQLite and MySQL; existing rows default to
      `Primary`/no parent.

## ParityDebt

- **Deliberate divergence (scanner):** ErsatzRS imports extras that ErsatzTV
  discards. Tracked in `content-variants` parity slice; ADR 0018 is the
  deviation record.
- **Version-detection heuristics:** title/folder token matching is best-effort;
  ambiguous cases fall back to `Primary` (no false subordination).

## Verification

- [x] `cargo test -p ersatzrs-core search` (parser/matcher/default-exclusion)
- [x] `cargo test -p ersatzrs-infra scheduling_repository` (projection + scheduling examples)
- [ ] `cargo test -p ersatzrs-infra` (projection + migration round-trip, both
      backends: default + `--features mysql`)
- [x] `cargo test -p ersatzrs-infra plex_movie_sync_classifies_provider_extras_and_alternate_versions`
- [x] `cargo test -p ersatzrs-infra jellyfin_movie_sync_classifies_provider_extras_and_alternate_versions`
- [x] `cargo test -p ersatzrs-infra emby_movie_sync_classifies_provider_extras_and_alternate_versions`
- [ ] `cargo test -p ersatzrs-scanner` (extras import + version linking)
- [x] `cargo test -p ersatzrs-web admin::server_fns::media_browser` (media browse/detail + smart
      collection server fns)
- [x] `cargo xtask hydration-lint -p ersatzrs-web`
- [x] `cargo fmt --all -- --check`
- [x] `cargo clippy --workspace --all-targets -- -D warnings` and
      `--features mysql` (mirror CI: two passes, neither `--all-features`)
- [ ] `cargo xtask parity-check web` (before claiming UI parity)
- [ ] `cargo xtask smoke` (playback-sensitive: Phase 4/5)
- [x] `cargo test --workspace --all-features`
- [x] `cargo deny check`
- [x] `cargo xtask progress check`
