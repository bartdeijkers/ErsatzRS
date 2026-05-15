**Search Results Parity Plan**

**Authoritative C# sources**
- Page: [$ERSATZTV_SRC/ErsatzTV/Pages/Search.razor](../../ErsatzTV/ErsatzTV/Pages/Search.razor)
- Multi-select base: [$ERSATZTV_SRC/ErsatzTV/Pages/MultiSelectBase.cs](../../ErsatzTV/ErsatzTV/Pages/MultiSelectBase.cs)
- Card component: [$ERSATZTV_SRC/ErsatzTV/Shared/MediaCard.razor](../../ErsatzTV/ErsatzTV/Shared/MediaCard.razor)
- Save-as dialog: [$ERSATZTV_SRC/ErsatzTV/Shared/SaveAsSmartCollectionDialog.razor](../../ErsatzTV/ErsatzTV/Shared/SaveAsSmartCollectionDialog.razor)
- Mapping rules: [$ERSATZTV_SRC/ErsatzTV.Application/MediaCards/Mapper.cs](../../ErsatzTV/ErsatzTV.Application/MediaCards/Mapper.cs)
- Per-kind search handlers: `$ERSATZTV_SRC/ErsatzTV.Application/Search/Queries/QuerySearchIndex*Handler.cs`
- Bulk-ids query: `QuerySearchIndexAllItems` (returns id lists for every kind for "Add All").

**Summary**

The current [crates/ersatzrs-web/src/admin/pages/search.rs](../crates/ersatzrs-web/src/admin/pages/search.rs) shows a flat Title/Kind/Year/Duration table built from the scheduling repository's flattened-leaf `media_items_for_source(Search(..))` projection. ErsatzTV's `Search.razor` instead renders ten typed card grids (Movies, Shows, Seasons, Episodes, Artists, Music Videos, Other Videos, Songs, Images, Remote Streams) with per-kind counts, anchor links in the toolbar, "See All" overflow links at 50, "Add All / Add All To Playlist / Save As Smart Collection" actions, a select-mode toolbar swap, per-card "Add To Collection / Show Media Info" menus, and a top-bar search field. We will port that shape exactly, reusing the existing `search_documents_page` projection in [crates/ersatzrs-infra/src/repositories/scheduling.rs](../crates/ersatzrs-infra/src/repositories/scheduling.rs) (already filters `SearchExpression` and computes the Lucene-style page map) as the index layer.

**What already exists (reuse, do not re-derive)**

- `SearchExpression` parser + matcher + page map: [crates/ersatzrs-core/src/search.rs](../crates/ersatzrs-core/src/search.rs). Supports `type:`, `library_id:`, `trakt_list:`, `tmdb:`, ranges, fuzzy, phrase, wildcards.
- `SchedulingRepository::search_documents_page(query, skip, limit)` → `(Vec<SearchDocument>, total, Option<SearchPageMap>)`. Use this as the per-kind hit source; do not write a parallel projection. See [crates/ersatzrs-infra/src/repositories/scheduling.rs:808](../crates/ersatzrs-infra/src/repositories/scheduling.rs#L808).
- Collection / playlist / smart-collection mutations:
  - `list_all_collections`, `create_collection`, `add_media_items_to_collection` ([media_browser.rs:1771](../crates/ersatzrs-web/src/admin/server_fns/media_browser.rs#L1771), [:1738](../crates/ersatzrs-web/src/admin/server_fns/media_browser.rs#L1738), [:2026](../crates/ersatzrs-web/src/admin/server_fns/media_browser.rs#L2026)).
  - `list_all_playlist_groups`, `list_playlists_by_group`, `add_media_items_to_playlist` ([:1835](../crates/ersatzrs-web/src/admin/server_fns/media_browser.rs#L1835), [:1860](../crates/ersatzrs-web/src/admin/server_fns/media_browser.rs#L1860), [:2077](../crates/ersatzrs-web/src/admin/server_fns/media_browser.rs#L2077)).
  - `save_smart_collection(form)` ([:1589](../crates/ersatzrs-web/src/admin/server_fns/media_browser.rs#L1589)) — already does upsert by `Id` with name + query.
- App shell `TopBar` ([crates/ersatzrs-web/src/admin/layout/mod.rs:30](../crates/ersatzrs-web/src/admin/layout/mod.rs#L30)) — extend in place rather than wrap.
- Artwork URL helper: `crate::api::artwork::{artwork_url, ArtworkUrlKind}` (Poster / Thumbnail) — already used by `get_collection_detail`. Resolve server-side so the wire shape is ready for `<img src>`.
- `MultiSelectBrowserPage` shell at [crates/ersatzrs-web/src/admin/pages/media/multi_select_browser.rs](../crates/ersatzrs-web/src/admin/pages/media/multi_select_browser.rs) — base for the new list-page routes.

**Gaps the current plan must close**

1. **Mixed-kind operations.** `add_media_items_to_playlist` takes a single `MediaBrowserKind`; ErsatzTV's `AddItemsToPlaylistHandler` accepts ten id lists and picks the `CollectionType` per kind. Search results are mixed-kind by nature, so this must be generalized.
2. **Per-kind result projection.** ErsatzTV returns a `*CardResultsViewModel { Cards, TotalCount, PageMap }` per kind. The current `SearchResultRow` flattens episodes to leaves and drops show/season/artist parents — that is wrong for the search grid.
3. **MediaCard placeholder, state overlay, and per-card menu.** The current `MediaCard` primitive only renders "🎬" when artwork is missing and has no state overlay or per-card menu. Search needs the C# `MediaCard.razor` behaviour: first-letter / `#` / season-number placeholder, FileNotFound (error) and Unavailable (warning) corner icons, and an Add-To-Collection / Add-To-Playlist / Show-Media-Info menu.
4. **Shift-click range select.** `MultiSelectBase.SelectClicked` builds the sorted card list across all visible sections and selects the range between `_recentlySelected` and the clicked card on shift-click. Must be preserved — it is the main reason a user expands the search page to a selection.
5. **Save-As Smart Collection.** `save_smart_collection` exists but the dialog flow (existing collection vs new, name field disabled until "(New Collection)" selected, snackbar success) is not yet ported.
6. **`type:` filter routing.** `search_items` in the scheduling repo expands shows/seasons/artists into playable leaves. The search page needs the *index hits*, not the playable expansion. Use `search_documents_page` directly so a show is one card, not N episodes.

**Server-fn surface (new and changed)**

Add to [crates/ersatzrs-web/src/admin/server_fns/search.rs](../crates/ersatzrs-web/src/admin/server_fns/search.rs):

- `pub enum SearchCardKind { Movie, Show, Season, Episode, Artist, MusicVideo, OtherVideo, Song, Image, RemoteStream }` — matches the `MediaKind` strings the index uses (`movie`, `show`, `season`, `episode`, `artist`, `music_video`, `other_video`, `song`, `image`, `remote_stream`).
- `pub struct SearchCard { media_item_id, kind: SearchCardKind, title, sort_title, subtitle: Option<String>, artwork_url: Option<String>, artwork_kind: ArtworkKind, placeholder: String, state: MediaItemState, href: String }` — mirrors `MediaCardViewModel` + the per-section `Href`/`Subtitle` overrides built inline in `Search.razor`.
- `pub struct SearchSection { cards: Vec<SearchCard>, total_count: u32 }` — equals `Cards` + `TotalCount` in `*CardResultsViewModel`. Cards are already sorted by `SortTitle`.
- `pub struct SearchResults { movies, shows, seasons, episodes, artists, music_videos, other_videos, songs, images, remote_streams: SearchSection, query: String }`.
- `#[server] pub async fn search_results(query: String) -> Result<SearchResults, ServerFnError>` — runs `search_documents_page` once per kind with `type:<kind> AND (<user_query>)`, `skip=0`, `limit=50`, projects each `SearchDocument` to `SearchCard`, resolves artwork via `artwork_url`. Empty query → empty sections (do not run ten queries).
- `pub struct SearchAllIds { movie_ids, show_ids, season_ids, episode_ids, artist_ids, music_video_ids, other_video_ids, song_ids, image_ids, remote_stream_ids: Vec<i64> }`.
- `#[server] pub async fn search_all_ids(query: String) -> Result<SearchAllIds, ServerFnError>` — ports `QuerySearchIndexAllItems`, used by Add All / Add All To Playlist. Runs one `search_documents_page` with `limit=0` and buckets ids by `media_kind`.

Generalize in [crates/ersatzrs-web/src/admin/server_fns/media_browser.rs](../crates/ersatzrs-web/src/admin/server_fns/media_browser.rs):

- Add `#[server] pub async fn add_media_items_to_playlist_mixed(playlist_id: i64, ids: SearchAllIds) -> Result<u64, ServerFnError>` — keyed by ErsatzTV `CollectionType` per kind (`Movie=10`, `Show=1`, `Season=2`, `Episode=4`, `Artist=3`, `MusicVideo=12`, `OtherVideo=40`, `Song=15`, `Image=50`, `RemoteStream=60` — verify against `$ERSATZTV_SRC/ErsatzTV.Core/Domain/CollectionType.cs`). Reuses the existing per-kind branch; keep the old single-kind `add_media_items_to_playlist` for the browser pages.
- `add_media_items_to_collection` is media-item-id agnostic (one `MediaItemId` column), so reuse as-is — just pass the flattened ids.

`save_smart_collection` already supports upsert; expose `list_smart_collections_for_select() -> Vec<CollectionOption>` (name + id, sorted by name, case-insensitive) for the dialog's dropdown and reuse the existing handler for create/update.

**MediaCard primitive — extensions in [crates/ersatzrs-web/src/admin/ui/primitives/card.rs](../crates/ersatzrs-web/src/admin/ui/primitives/card.rs)**

Currently `MediaCard` renders one fixed emoji placeholder and exposes a barebones `selected` checkbox. To reach `MediaCard.razor` parity, change the signature to:

```rust
#[component]
pub fn MediaCard(
    title: String,
    subtitle: Option<String>,
    href: Option<String>,
    artwork_url: Option<String>,
    placeholder: String,           // "#" / first letter / "S" / season number
    state: MediaItemState,         // Normal | FileNotFound | Unavailable
    select_mode: Signal<bool>,
    selected: Option<RwSignal<bool>>,
    on_select: Option<Callback<MouseEvent>>,     // receives shift-key flag
    on_add_to_collection: Option<Callback<()>>,
    on_add_to_playlist: Option<Callback<()>>,
    on_show_media_info: Option<Callback<()>>,
) -> impl IntoView
```

- Placeholder typography size scales with `placeholder.len()` (`>12 → h3`, `>2 → h2`, else `h1`) to match the C# typo switch.
- State overlay: corner warning icon, red for `FileNotFound`, amber for `Unavailable`, hidden for `Normal`.
- Per-card "MoreVert" menu: only renders when not in select-mode and at least one callback is set.
- Overlay click: in select-mode, fires `on_select`; otherwise navigates to `href`. Matches `OnOverlayClick`.
- `on_select` must forward `event.shift_key()` so the page can do shift-range selection.

**Search page rewrite — [crates/ersatzrs-web/src/admin/pages/search.rs](../crates/ersatzrs-web/src/admin/pages/search.rs)**

Replace the flat-table island with a section grid:

- Page wrapper reads `?query=...` from `use_query_map`, then renders a single `SearchIsland`.
- Island state: `RwSignal<HashSet<(SearchCardKind, i64)>>` for selection, `RwSignal<Option<MouseEvent>>` for shift-click anchor.
- Resource: `Resource::new_blocking(|| query, |q| search_results(q))`. Render inside `<Suspense>` — toolbar lives outside the suspense boundary per `TEST.md`.
- Toolbar — two visual states, picked by `select_mode = selected.len() > 0`:
  - **Default**: query echo on the left; section count links (`{N} Movies`, `{N} Shows`, …) only when section non-empty, each a `<a href="#movies">` etc.; right side `Add All` (collection), `Add All` (playlist), `Save As`.
  - **Select**: `"{N} Items Selected"`; `Add To Collection`, `Add To Playlist`, `Clear Selection`.
- Sections render in the ErsatzTV order with the matching anchor IDs (`movies`, `shows`, `seasons`, `episodes`, `artists`, `music_videos`, `other_videos`, `songs`, `images`, `remote_streams`). Heading is `<h2 id=...>{Label}</h2>`; when `total_count > 50`, add a `See All >>` link to the matching list route with the query preserved (`?query=<encoded>`).
- Card grid uses the extended `MediaCard`. `select_clicked` builds the sorted cross-section card list (movies → shows → … → remote_streams, each ordered by `sort_title`) and applies the shift-range logic from `MultiSelectBase.SelectClicked`.
- Add All / Add All To Playlist: call `search_all_ids(query)` then `add_media_items_to_collection(collection_id, ids.flatten())` or `add_media_items_to_playlist_mixed(playlist_id, ids)`. Snackbar via the existing `use_toast`.
- Save As: open a new `SaveAsSmartCollectionDialog` (port of the C# dialog) that lists existing smart collections plus a `(New Collection)` sentinel; on submit, call `save_smart_collection` with the current query and selected id or new name.

Per-section subtitle / href rules (lifted from `Search.razor`):

| Kind | Href | Subtitle | Artwork |
|---|---|---|---|
| Movie | `/media/movies/{id}` | `Year` | Poster |
| Show | `/media/tv/shows/{id}` | `Year` | Poster |
| Season | `/media/tv/seasons/{id}` | `Season {n}` (or "Specials" for 0) | Poster (season → show fallback) |
| Episode | `/media/tv/seasons/{season_id}#episode-{id}` | `{ShowTitle} - S{season} E{episode}` | Episode → season → show poster fallback |
| Artist | `/media/music/artists/{id}` | `Disambiguation` | Thumbnail |
| Music video | `""` | Artist name | Thumbnail |
| Other video | `""` | Original title | Thumbnail → Poster fallback |
| Song | `""` | Artists + ` - {Album}` | Thumbnail |
| Image | `""` | Original title | Thumbnail |
| Remote stream | `""` | Original title | Thumbnail |

The placeholder string follows `MediaCard.razor` `GetPlaceholder`: if `sort_title` first char is a letter, use uppercase letter; if digit/non-letter, use `#`; for seasons, use `"S"` for season 0, else `season_number.to_string()`.

**Top-bar search**

Extend `TopBar` in [crates/ersatzrs-web/src/admin/layout/mod.rs:30](../crates/ersatzrs-web/src/admin/layout/mod.rs#L30) with a `<form method="get" action="/search">` containing a single `<input name="query">` before the existing `topbar_nav`. Server-rendered, no hydration cost. Submit goes through standard navigation and lands on `/search?query=...`. Match the existing CSS conventions in `layout.module.css`.

**List-route gaps (See All targets)**

`/media/movies`, `/media/tv/shows`, `/media/music/artists`, `/media/other/videos`, `/media/music/songs`, `/media/images` already exist via `MultiSelectBrowserPage` or dedicated pages. Missing: `/media/tv/seasons`, `/media/tv/episodes`, `/media/music/videos`, `/media/remote/streams`.

Slice these in two passes to keep this PR scoped:

- **In this slice**: add the four routes wired to `MultiSelectBrowserPage` with new `BrowserKindConfig` entries (`SEASONS`, `EPISODES`, `MUSIC_VIDEOS`, `REMOTE_STREAMS`). Extend `MediaBrowserKind` and `MediaBrowserKind::tables`/`collection_type` accordingly. The list-page query can fall back to a thin `library_id`-less filter on `MediaItem`-joined metadata, matching the SQL shape used by `list_movies` etc. The "See All" link is the only required entry path, so the basic letter-bar + paginated grid is sufficient — full filter UI parity for these pages can be deferred.
- **Deferred (separate plan)**: per-item detail pages for season/episode/music-video/remote-stream and the right-side "library filter" combobox these pages have in ErsatzTV. The search "See All" link's contract is "land on a page that shows all hits of this kind for this query" — letter-bar + grid + query echo satisfies that.

**Routes**

Add to [crates/ersatzrs-web/src/admin/routes.rs](../crates/ersatzrs-web/src/admin/routes.rs):

```rust
<Route path=(StaticSegment("media"), StaticSegment("tv"), StaticSegment("seasons")) view=SeasonsListPage/>
<Route path=(StaticSegment("media"), StaticSegment("tv"), StaticSegment("episodes")) view=EpisodesListPage/>
<Route path=(StaticSegment("media"), StaticSegment("music"), StaticSegment("videos")) view=MusicVideosListPage/>
<Route path=(StaticSegment("media"), StaticSegment("remote"), StaticSegment("streams")) view=RemoteStreamsListPage/>
```

The existing `<Route path=StaticSegment("search") view=SearchPage/>` keeps its current shape.

**Order of implementation**

Sequenced so each step compiles and is independently reviewable. Earlier steps unblock later ones; tests land alongside the step that introduces the behaviour they cover.

1. [x] **Extend `MediaCard` primitive.** Update the signature in [crates/ersatzrs-web/src/admin/ui/primitives/card.rs](../crates/ersatzrs-web/src/admin/ui/primitives/card.rs) and the matching CSS to add `placeholder: String`, `state: MediaItemState`, the per-card `MoreVert` menu, and a shift-aware `on_select`. Add an enum `MediaItemState { Normal, FileNotFound, Unavailable }` in core if one does not already exist. Audit existing call sites (`multi_select_browser.rs`, `collection_detail.rs`, `trash.rs`, browser pages) and pass the new props with `state: Normal` + first-letter placeholder so behaviour is unchanged. No new server fns yet.
2. [x] **Generalize playlist insertion.** In [crates/ersatzrs-web/src/admin/server_fns/media_browser.rs](../crates/ersatzrs-web/src/admin/server_fns/media_browser.rs) add `SearchAllIds` and `add_media_items_to_playlist_mixed(playlist_id, ids: SearchAllIds)`. Verify each `CollectionType` value against `$ERSATZTV_SRC/ErsatzTV.Core/Domain/CollectionType.cs`. Keep `add_media_items_to_playlist` (single-kind) untouched — the browser pages still use it. Add a unit test that confirms `Index` is contiguous across kinds and the right `CollectionType` is stamped per row.
3. [x] **`SearchSection` projection layer.** Rewrite [crates/ersatzrs-web/src/admin/server_fns/search.rs](../crates/ersatzrs-web/src/admin/server_fns/search.rs): add `SearchCardKind`, `SearchCard`, `SearchSection`, `SearchResults`; implement `search_results` by calling `search_documents_page("type:<kind> AND (<q>)", 0, 50)` ten times in parallel via `futures::future::try_join_all` and projecting per the subtitle/href/artwork table. Resolve artwork URLs server-side via `artwork_url(...)`. Delete the old `SearchResultRow` + `search_media`. Add a focused integration test in [crates/ersatzrs-infra/tests](../crates/ersatzrs-infra/tests) that seeds a small library and asserts section counts and the season-0 → `"S"` / episode → season → show poster fallbacks.
4. [x] **`search_all_ids` for bulk actions.** Add `#[server] pub async fn search_all_ids(query: String) -> Result<SearchAllIds, ServerFnError>`. One pass through `search_documents_page` with `limit=0`, bucket by `media_kind`. Tests can be unit-level over a fixed `Vec<SearchDocument>`.
5. [ ] **`list_smart_collections_for_select` helper.** In [media_browser.rs](../crates/ersatzrs-web/src/admin/server_fns/media_browser.rs) add a small `#[server]` that returns `Vec<CollectionOption>` sorted by name (case-insensitive). Used only by the Save-As dialog.
6. [ ] **`SaveAsSmartCollectionDialog` component.** New module under [crates/ersatzrs-web/src/admin/ui/dialogs/](../crates/ersatzrs-web/src/admin/ui/dialogs/) (or co-locate next to the search page if no dialogs folder exists yet) using the existing `Modal` primitive. Driven by `list_smart_collections_for_select` + `save_smart_collection`. Mirror the `(New Collection)` sentinel and the "name field disabled until New is selected" rule from the C# dialog.
7. [ ] **Rewrite the search page.** Replace [crates/ersatzrs-web/src/admin/pages/search.rs](../crates/ersatzrs-web/src/admin/pages/search.rs) with the section grid: page wrapper reads `?query=`, `SearchIsland` owns selection state and renders the toolbar (both default and select-mode variants), the ten sections with stable anchor IDs, See-All overflow links, the Add All / Add All To Playlist / Save As bulk actions, and the per-card menus. Implement the cross-section sorted-card builder used by shift-range select. The existing module-CSS file at `collections.module.css` is reused only for the toolbar shell; introduce a dedicated `search.module.css` for the section layout and anchor scroll padding.
8. [ ] **Top-bar search field.** Extend `TopBar` in [crates/ersatzrs-web/src/admin/layout/mod.rs:30](../crates/ersatzrs-web/src/admin/layout/mod.rs#L30) with a server-rendered `<form method="get" action="/search">`. Add the matching CSS to `layout.module.css`. Confirm the form does not require hydration (no JS) and that pressing Enter on the input navigates correctly.
9. [ ] **See-All list-route pages.** Extend `MediaBrowserKind` with `Season`, `Episode`, `MusicVideo`, `RemoteStream` variants; add the `tables()` + `collection_type()` entries; add `BrowserKindConfig` constants. Implement `list_seasons`, `list_episodes`, `list_music_videos`, `list_remote_streams` mirroring the shape of `list_movies`. Register the four routes in [crates/ersatzrs-web/src/admin/routes.rs](../crates/ersatzrs-web/src/admin/routes.rs). Smoke-test that the See-All links from the search page land on a populated grid.
10. [ ] **Playwright + final passes.** Add a per-page playbook entry under `crates/ersatzrs-web/tests/` (see `TEST.md`) covering top-bar submit, section anchors, placeholder rendering, Add All dialog, Save As dialog. Run `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo clippy --workspace --all-targets --features mysql -- -D warnings`, and `cargo test --workspace`.

Steps 1–6 are server- and primitive-only and can land as a single PR or split if review pressure demands. Step 7 is the visible behaviour change. Steps 8–9 are independent of each other once 1–7 land and can be parallelized between contributors if needed.

**Tests**

- Unit tests in [crates/ersatzrs-infra/tests/scheduling_repository.rs](../crates/ersatzrs-infra/tests/scheduling_repository.rs) cover `search_documents_page` already; add a new file (or section) that exercises `search_results` end-to-end: seed a small library with one movie, one show + 2 seasons + 3 episodes, one artist + 2 music videos, one song, one image, one remote stream — assert section counts, that show-without-poster falls back to placeholder `"S"`, and that episode-without-poster uses season poster.
- Card-projection mapping tests against fixtures for the three trickiest cases:
  - Season poster fallback when `SeasonMetadata` is absent → show poster.
  - Episode poster fallback chain → season → show.
  - Season-0 placeholder is `"S"`.
- `add_media_items_to_playlist_mixed` test that confirms `CollectionType` is selected per id list and that `Index` is contiguous across the resulting `PlaylistItem` rows.
- Playwright smoke at `/search?query=library_id%3a3`: top-bar search field present and submits; sections appear in order; counts in toolbar match section card counts; placeholder letters render when artwork is missing; Add All opens the dialog with the existing collections list; Save As opens the smart-collection dialog and persists on submit.
- `cargo fmt --all -- --check` and clippy on the no-default-features and `--features mysql` passes (see `feedback_clippy_command` memory).

**Assumptions and deferred work**

- "Full search UI" = parity for the page shape, toolbar, sections, selection, bulk actions, and See-All entry points. Detail pages for kinds other than Movie are deferred.
- ErsatzRS does not run Lucene; `SearchExpression` over a SQL-backed projection is the index. We accept that fielded queries (`type:`, `library_id:`, `tmdb:`, ranges) work today and proximity/boost grammar is loose.
- The current "See All" list pages reuse the existing query-string pagination; we do not port the `/page/N` URL form ErsatzTV uses. Search results' "See All" links pass `?query=…&page=1`; the list pages already accept that.
- `PersistentComponentState` (the C# anti-flicker cache) has no Leptos analogue and is not needed for the SSR path.
