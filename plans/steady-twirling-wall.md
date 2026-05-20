# Plan: Port `/media/tv/seasons/:id` (TV season detail page)

## Context

`/media/tv/seasons/:id` is ErsatzTV's `ErsatzTV/Pages/TelevisionEpisodeList.razor`
(route `@page "/media/tv/seasons/{SeasonId:int}"`). It is the season detail
page reached from the show-detail page's season cards — `shows.rs:253` already
emits `href="/media/tv/seasons/{id}"` links, but they 404 today:
`routes.rs:186` only maps `/media/tv/seasons` (no id) to the `SeasonsListPage`
placeholder. `docs/reference/parity-matrix.md:97` lists `/media/tv/seasons/`
under "Advertised but 404".

The screenshot `plans/references/Screenshot 2026-05-19 at 23-08-25 ErsatzTV Legacy.jpg` (`/media/tv/seasons/341`, 'Allo 'Allo! Season 1) shows: a
fanart/poster header with the show title + "Season 1" + Add To
Collection/Playlist/Schedule, then a vertical list of episode cards — each with
thumbnail, "N. Title", plot, per-episode "Add To …"/"Show Media Info"/
"Troubleshoot Playback", a file-status line, "Released: <date>", and
Directors/Writers as search links.

Goal: a faithful, non-stopgap port that genuinely advances both matrices.
Per user decisions: (1) data access goes through a real `ersatzrs-infra`
Television repository (ErsatzTV `TelevisionRepository.cs` parity) — the
persistence-gap-matrix only counts `crates/ersatzrs-infra` SQL; (2) per-episode
"Show Media Info"/"Troubleshoot Playback" is implemented now by extending the
media-item-info read to episodes (no hidden controls).

## Authoritative sources

- ErsatzTV page: `/home/bart/ErsatzTV/ErsatzTV/Pages/TelevisionEpisodeList.razor`
- ErsatzTV repo: `/home/bart/ErsatzTV/ErsatzTV.Infrastructure/Data/Repositories/TelevisionRepository.cs`
  (`GetSeason` L119, `GetEpisodeCount` L179, `GetPagedEpisodes` L187)
- ErsatzTV mappers: `ErsatzTV.Application/Television/Mapper.cs` (season VM),
  `ErsatzTV.Application/MediaCards/Mapper.cs` (episode card; `GetThumbnail` L285),
  `ErsatzTV.Core/Domain/Metadata/ArtworkKind.cs` (Poster=0, Thumbnail=1, FanArt=3)
- Episode actions: `AddSeasonToCollectionHandler` (reindex + BuildPlayout Refresh),
  `AddSeasonToPlaylistHandler` (no side effect), `AddProgramScheduleItem.ForMediaItem`
  (BuildPlayout Refresh), `GetMediaItemInfoHandler` (episode branch)
- Pattern to mirror exactly: `crates/ersatzrs-web/src/admin/pages/media/shows.rs`
  (page-wrapper → scalar-prop `#[island]` → `<Suspense>` inside island; per
  `TEST.md` hydration rules)

## Faithful divergences (documented, not stopgaps)

- Post-add navigation: ErsatzTV navigates to the collection/playlist/schedule;
  ErsatzRS keeps the established multi-select-browser behaviour (toast + stay),
  identical to the just-merged show-detail page.
- Search reindex on collection add: globally not wired in ErsatzRS for any add
  path; remains a project-wide deferred parity note (not introduced here).
- Null episode release date: render ErsatzTV's min-date exactly as the
  screenshot shows ("Released: 1-1-0001"); do not hide the row.

## Ordered steps

### 1. Core: Television repository trait
`crates/ersatzrs-core/src/repositories/television.rs` (new) + register in
`crates/ersatzrs-core/src/repositories/mod.rs`.

- `trait TelevisionRepository: Send + Sync` with:
  - `get_season(season_id) -> Result<Option<SeasonDetail>>`
  - `get_episode_count(season_id) -> Result<i64>`
  - `get_paged_episodes(season_id, page_number, page_size) -> Result<Vec<EpisodeCard>>`
  - `get_episode_media_item_info(episode_id) -> Result<Option<EpisodeMediaItemInfo>>`
- Define plain result structs in this module (analogous to `scheduling`'s
  `SchedulingMediaItem`): `SeasonDetail { id, show_id, show_title, show_year,
  season_number, season_poster_path, show_fanart_path }`,
  `EpisodeCard { episode_id, episode_number, title, sort_title, plot,
  release_date, thumbnail_path, state, file_path, directors, writers,
  show_title, season_number }`, `EpisodeMediaItemInfo { … }` (same shape the
  movie media-info query produces: title, library kind/name, server name,
  state, media_version_id + ids needed for streams/chapters/subtitles).
- Repository returns raw artwork **paths** and state ints; URL resolution and
  date formatting stay in the web layer (matches `get_show_details`).

### 2. Infra: SQLite Television repository
`crates/ersatzrs-infra/src/repositories/television.rs` (new) +
`repositories/mod.rs` (`mod television; pub use … SqliteTelevisionRepository;`,
add to `Repositories` struct + `make_repositories`).

- `pub struct SqliteTelevisionRepository(pub Pool<crate::db::Db>)`,
  `#[async_trait] impl TelevisionRepository` — mirror the `metadata.rs` shape.
- `get_season`: Season + SeasonMetadata + Artwork(`ArtworkKind=0` Poster) +
  Show + ShowMetadata + Artwork(`ArtworkKind=3` FanArt); name resolution is
  done in web (`season_number == 0 ? "Specials" : "Season {n}"`).
- `get_episode_count`: `SELECT COUNT(*) FROM "Episode" WHERE "SeasonId" = ?`.
- `get_paged_episodes`: `EpisodeMetadata` filtered by `Episode.SeasonId`,
  `ORDER BY "EpisodeNumber"`, `LIMIT ? OFFSET (page-1)*size`; join
  episode thumbnail (`Artwork.ArtworkKind=1`), `MediaItem.State`, head
  `MediaFile.Path`; `directors`/`writers` via `Director.EpisodeMetadataId` /
  `Writer.EpisodeMetadataId` (`ORDER BY "Id"`).
- `get_episode_media_item_info`: episode analogue of the movie query in
  `media_browser.rs:632` (FROM `Episode`/`EpisodeMetadata`, library/source
  joins, head `MediaVersion`).
- Tests: `crates/ersatzrs-infra/tests/television.rs` (new) — seed full graph,
  assert all four methods (ordering, directors/writers, state/path, episode
  media info).

### 3. Web server fns (thin callers)
`crates/ersatzrs-web/src/admin/server_fns/media_browser.rs`.

- Add serializable DTOs `SeasonDetailRow` + `EpisodeCardRow` (PascalCase not
  required — internal, like `ShowDetailRow`).
- `#[server] get_season_details(id) -> Result<Option<SeasonDetailRow>>`:
  construct `SqliteTelevisionRepository(state.pool.clone())`, call
  `get_season` + `get_paged_episodes(id, 1, 100)` (ErsatzTV `PageNumber=1`,
  `PageSize=100`); map to DTO; resolve `artwork_url`
  (`ArtworkUrlKind::Poster`/`FanArt`/`Thumbnail`) and short-date formatting
  (null → ErsatzTV min-date string).
- Generic `#[server] add_media_item_to_schedule(schedule_id, collection_type,
  media_item_id)`; refactor existing `add_show_to_schedule` to delegate with
  `collection_type = 1` (TelevisionShow). Season uses `2` (TelevisionSeason).
  Keeps `enqueue_schedule_playout_refresh` (matches
  `AddProgramScheduleItemHandler` BuildPlayout Refresh).
- Reuse `add_media_items_to_collection` / `add_media_items_to_playlist` for
  season (`MediaBrowserKind::Season`) and episode (`::Episode`); season→
  collection already enqueues `enqueue_collection_playout_refresh`
  (matches `AddSeasonToCollectionHandler`); playlist add no side effect
  (matches `AddSeasonToPlaylistHandler`).
- Refactor `get_media_item_info_json(id)`: detect Movie vs Episode by subtype
  table; episode → `SqliteTelevisionRepository::get_episode_media_item_info`,
  `kind:"Episode"`, title `"{ShowTitle} - s{season:02}e{episode:02} -
  {Title}"` (ErsatzTV `ShowInfo`); reuse `load_media_info_streams` /
  `load_media_info_chapters`; add `load_episode_subtitle_info` (Subtitle by
  `EpisodeMetadataId`). This also advances `/system/troubleshooting/playback`
  parity to episodes.

### 4. Web UI: season detail page
`crates/ersatzrs-web/src/admin/pages/media/season_detail.rs` (new) +
`pages/media/mod.rs` (`pub mod season_detail;`) +
`pages/media/season_detail.module.css` (new, episode-card row layout only;
reuse `movie_detail.module.css` for hero/fanart and `show_detail.module.css`
for action bar/buttons/section/divider).

- `SeasonPage()` plain `#[component]` reads `:id` → `<SeasonDetailIsland id/>`.
- `SeasonDetailIsland(id)` `#[island]`: `Resource::new_blocking` over
  `get_season_details(id)` + `<Suspense>` (Loading… / "Season not found" +
  back link / `SeasonDetailView`). No prop-bearing island across Suspense.
- `SeasonDetailView`: fanart; hero poster; header = show title `<a
  href="/media/tv/shows/{show_id}">` + subtitle Name; action bar Add To
  Collection/Playlist/Schedule; "Back" → `/media/tv/shows/{show_id}`.
- Episode list = vertical card stack (NOT the poster grid): per episode —
  thumbnail (`ArtworkUrlKind::Thumbnail`) with state-1/2 warning overlay;
  `<h*>{n}. {title}</h*>`; plot; row of "Add To …" (Collection/Playlist for
  the episode, reuse shared modals with `MediaBrowserKind::Episode`,
  selection `{episode_id}`), "Show Media Info" (modal → `get_media_item_info_json(episode_id)`;
  reuse `MediaInfoModal` if usable else local), "Troubleshoot Playback"
  `<a href="/system/troubleshooting/playback?mediaItem={episode_id}">`;
  file-status line (FileNotFound→path / Unavailable→local path);
  "Released: {short date}"; Directors/Writers as
  `/search?query=director:"<lower>"` / `writer:"<lower>"` links (sorted).
- Reuse `AddToCollectionModal`/`AddToPlaylistModal` from
  `multi_select_browser`. Extract the show page's `AddToScheduleModal` into a
  shared, parametrized component (media item id + collection_type) used by
  both `shows.rs` and `season_detail.rs` (calls `add_media_item_to_schedule`).

### 5. Routes
`crates/ersatzrs-web/src/admin/routes.rs`: import `SeasonPage`; add
`(media,tv,seasons,:id) → SeasonPage` and alias `(media,seasons,:id) →
SeasonPage` (mirrors the shows `:id` + `/media/shows/:id` pair). Leave
`(media,tv,seasons) → SeasonsListPage` untouched (ErsatzTV has no season list;
out of scope). No nav change (ErsatzTV reaches seasons via show cards).

### 6. Matrix + status doc updates (genuine)
- `docs/reference/parity-matrix.md`: move `/media/tv/seasons/` from the
  "Advertised but 404" table to "Routed (work today)" (backing route
  `SeasonPage` `/media/tv/seasons/:id`, alias `/media/seasons/:id`); extend
  the main "Media browsers" row (line 36) with one sentence on the season
  detail page + the new `ersatzrs-infra` Television repository + the episode
  `GetMediaItemInfo` extension; keep the divergence notes.
- `docs/reference/persistence-gap-matrix.md`: update `Season`,
  `SeasonMetadata` (drop "has no owner repository" → "Yes: Television
  repository read"), `Episode`, `EpisodeMetadata`, `Director`, `Writer` rows
  to cite the new Television repository read paths; recompute the three
  summary counts honestly only if a bucket actually changes.
- `STATUS.md`: add a session-log row (2026-05-19) and adjust "What an Agent
  Should Do Next". Per the Session Handoff contract this is committed as
  `docs(status): update session log` (commit only when the user asks).

### 7. Web test
`crates/ersatzrs-web` DB-backed test mirroring the show-detail web test
(commit `bfbbe16`): assert `get_season_details` mapping + episode
`get_media_item_info_json` (`kind:"Episode"`, title format).

## Critical files

- New: `crates/ersatzrs-core/src/repositories/television.rs`,
  `crates/ersatzrs-infra/src/repositories/television.rs`,
  `crates/ersatzrs-infra/tests/television.rs`,
  `crates/ersatzrs-web/src/admin/pages/media/season_detail.rs`,
  `crates/ersatzrs-web/src/admin/pages/media/season_detail.module.css`
- Modified: `crates/ersatzrs-core/src/repositories/mod.rs`,
  `crates/ersatzrs-infra/src/repositories/mod.rs`,
  `crates/ersatzrs-web/src/admin/server_fns/media_browser.rs`,
  `crates/ersatzrs-web/src/admin/pages/media/{mod.rs,shows.rs}`,
  `crates/ersatzrs-web/src/admin/routes.rs`,
  `docs/reference/parity-matrix.md`,
  `docs/reference/persistence-gap-matrix.md`, `STATUS.md`
- Reuse: `multi_select_browser.rs` (`AddToCollectionModal`,
  `AddToPlaylistModal`, `MediaInfoModal`, `MediaBrowserKind`),
  `admin/ui/primitives/card.rs` (`MediaCard`/`MediaItemState`),
  `admin/ui/primitives/dialog.rs` (`Modal`), `toast`,
  `crate::api::artwork::{artwork_url, ArtworkUrlKind}`,
  `enqueue_{schedule,collection}_playout_refresh`,
  `load_media_info_streams`/`load_media_info_chapters`,
  `ersatzrs_core::language_codes` (not needed — season page has no Languages
  row in ErsatzTV; explicitly not added)

## Verification

1. `cargo fmt --all -- --check`
2. `cargo clippy --workspace --all-targets -- -D warnings` **and**
   `cargo clippy --workspace --all-targets --features mysql -- -D warnings`
   (CI parity — feature-gated test files escape lint otherwise)
3. `cargo test --workspace --all-features` (new infra + web tests green)
4. `(cd crates/ersatzrs-web && cargo leptos build)`
5. `cargo run --bin ersatzrs-web`; open `/media/tv/shows/<id>`, click a season
   card → `/media/tv/seasons/<season_id>`. Verify against the screenshot:
   header (poster, show-title link, "Season N"), episode rows (thumbnail,
   "N. Title", plot, Add To/Show Media Info/Troubleshoot, file-status line,
   "Released: <date>" incl. "1-1-0001" when null, Directors/Writers search
   links); Add To Collection/Playlist/Schedule modals + toast; per-episode
   Add To/Show Media Info modal; `/system/troubleshooting/playback?mediaItem=
   <episode_id>` now returns episode JSON.
