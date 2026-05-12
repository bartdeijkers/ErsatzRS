---
name: Movies page port — artwork, pagination, details
description: Patterns for porting MudBlazor media-list pages — artwork proxy via /artwork/{kind}/{provider}/..., URL-driven pagination, blocking resource details page.
type: project
---

The movies page port (May 2026) established three reusable patterns for any
MudBlazor media-list port.

**Why:** the page had three bugs (no posters, dead pagination, no details
page). All three are recurring symptoms of porting MudBlazor naively without
considering ErsatzTV's `ArtworkController`, `NavigationManager` usage, and
hydration constraints.

**How to apply:**

1. **Artwork URLs.** `Application/MediaCards/Mapper.cs` (`GetPoster`,
   `GetThumbnail`) and `Movies/Mapper.cs` (`Artwork`) convert provider-prefixed
   DB paths (`jellyfin://Items/.../Primary?tag=X`) into relative URLs of the
   form `jellyfin/.../?fillHeight=N` and prepend `artwork/posters/` /
   `artwork/fanart/`. `ArtworkController` proxies those to the upstream Jellyfin
   / Emby server using the stored API key.

   Port: a `crate::api::artwork` module with `artwork_url(path, kind)` for the
   server fn → client URL mapping and `/artwork/posters/{*path}` /
   `/artwork/fanart/{*path}` axum routes that read first
   `JellyfinConnection`/`EmbyConnection` row, load the API key from
   `ConfigElement["{provider}.current_server_api_key"]`, and stream the upstream
   response with reqwest. Auth header: `Authorization: MediaBrowser Token="..."`
   for Jellyfin; `X-Emby-Token: ...` for Emby.

   `ArtworkKind` enum: 0=Poster, 1=Thumbnail, 2=Logo, 3=FanArt, 4=Watermark.
   The original `list_movies` query used `1` (Thumbnail) which silently dropped
   all `Poster`-kind rows — verify the correct kind whenever copy-pasting
   queries from another media-list port.

2. **Pagination + search via URL, not signals.** ErsatzTV uses
   `NavigationManager.NavigateTo("media/movies/page/N")` — pagination is a
   navigation, not a stateful island. The cleanest port is a plain
   `#[component]` page that reads `use_query_map()` for `?query=…&page=N`,
   owns a `Resource::new_blocking` keyed on those values, and renders a `<form
   method="get">` for search + `<a href=…>` for pager arrows.

   Bonus: this sidesteps the codebase-wide `failed_to_cast_marker_node`
   hydration panic that affects islands using `prop:value` + `<Suspense>`
   together (search.rs and channels/editor.rs reproduce it). When the panic
   triggers, `on:click` handlers in the same island never attach — buttons
   look enabled but do nothing.

3. **Details page.** `Movie.razor` + `GetMovieByIdHandler` → `MoviePage`
   `#[component]` (not an island) wrapping a single `Resource::new_blocking`
   inside one `<Suspense>`. Pass the resolved `MovieDetailRow` to a child
   `#[component] MovieDetailView` by value. No prop-bearing island; all `<a>`
   navigation. Fanart is rendered as a full-width background image with a
   gradient overlay (`.fanart-container` + `.fanart-tint`), poster + metadata
   inside a constrained content column.

Files: `crates/ersatzrs-web/src/api/artwork.rs`,
`crates/ersatzrs-web/src/admin/pages/media/movies.rs`,
`crates/ersatzrs-web/src/admin/pages/media/movie_detail.module.css`,
`crates/ersatzrs-web/src/admin/server_fns/media_browser.rs`
(`get_movie_details`).
