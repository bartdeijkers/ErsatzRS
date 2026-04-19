# Phase 6 — Leptos Admin UI

## Context

Phase 6 ports ErsatzTV's 122-file Blazor/MudBlazor admin UI to Leptos 0.7 SSR
with selective islands hydration. The acceptance criterion from the master plan
is: *"A user can create a channel, configure a media source, build a playout,
and start a stream from the UI. Critical UI paths have no browser-console
errors."*

Priority order: channels → media sources → library browser/collections → block
editor → YAML playout editor → FFmpeg profiles → graphics/watermark → playout
timeline.

## CSS Stack

**Stylance** — scoped CSS modules compiled by the `stylance-cli` Rust binary.
Zero npm dependency; the binary is a Rust crate installed with
`cargo install stylance-cli`. Styles ship as `public/bundle.css` linked from
the HTML shell. Design tokens are CSS custom properties in
`src/style/globals.module.css`, sourced from ErsatzTV's `PaletteDark` in
[`ErsatzTvTheme.cs`](/home/bart/ErsatzTV/ErsatzTV/Shared/MainLayout.razor).

Each component owns a co-located `*.module.css` file. The `stylance` crate
generates compile-time `&'static str` constants for every class, replacing
inline Tailwind strings. The `import_crate_style!` macro path is relative to
the crate root (i.e. `src/ui/primitives/button.module.css`).

Key tokens (from PaletteDark):

| Token | Value | Maps to |
|---|---|---|
| `--color-bg` | `#272727` | page background |
| `--color-surface` | `#1f1f1f` | card / input background |
| `--color-appbar` | `#121212` | top bar + nav drawer |
| `--color-primary` | `#009000` | primary action, active nav |
| `--color-secondary` | `#009090` | secondary action |
| `--color-error` | `#ef4444` | validation, delete |
| `--color-muted` | `rgba(255,255,255,0.40)` | disabled / helper text |

## Completed Sub-Phases

### 6.0 — CSS Infrastructure + Primitive Components ✓

Implemented hand-rolled Leptos components mapping to the MudBlazor surface used
by ErsatzTV. All initially built with Tailwind v4 + npm, then migrated to
Stylance (sub-phase 6.0.M).

**Primitive files** (`crates/ersatzrs-web/src/ui/primitives/`):

| File | Maps to |
|---|---|
| `button.rs` / `button.module.css` | `MudButton`, `MudIconButton` |
| `form.rs` / `form.module.css` | `MudTextField`, `MudSelect`, `MudCheckBox` |
| `icon.rs` | Material/Lucide SVG dispatcher; `size` prop sets inline width/height |
| `badge.rs` / `badge.module.css` | `MudBadge` |
| `table.rs` / `table.module.css` | `MudTable` + server data; `PagedTable<T>` |
| `dialog.rs` / `dialog.module.css` | `MudDialog`; `Modal` + `DeleteDialog` |
| `toast.rs` / `toast.module.css` | `ISnackbar`; `ToastStore` + `ToastContainer` |
| `card.rs` / `card.module.css` | `MudCard`, `MediaCard`, `MediaCardPager` |

**Layout** (`src/ui/layout.rs` / `layout.module.css`):
- `AppShell` — root shell wrapping nav drawer + top bar + `<main>`
- `TopBar` — logo, M3U/XMLTV quick links
- `NavDrawer` — collapsible groups matching ErsatzTV's 74-link sidebar
- `NavLink` — active-state detection via `use_location()`

### 6.1 — Channel List and Editor ✓

Server functions (direct SQLite via `expect_context::<AppState>()`):

| Function | Signature |
|---|---|
| `list_channel_rows` | `(show_disabled, page, page_size) → Vec<ChannelListRow>` |
| `count_channels` | `(show_disabled) → u64` |
| `get_channel_form` | `(id) → ChannelForm` |
| `save_channel` | `(ChannelForm) → i64` — id=0 inserts, id>0 updates |
| `delete_channel` | `(id) → ()` |
| `list_ffmpeg_profiles_for_select` | `() → Vec<(i64, String)>` |
| `list_filler_presets_for_select` | `() → Vec<(i64, String)>` |

Pages:
- `/channels` — `ChannelsListPage`: `PagedTable` + show-disabled toggle + `DeleteDialog`
- `/channels/add` — `ChannelEditorPage`: create form
- `/channels/:id` — `ChannelEditorPage`: edit form; navigates to `/channels` on save

Routes registered in `app.rs`; `AppState` injected via
`leptos_routes_with_context` in `main.rs`.

### 6.2 — Layout + Navigation ✓

Wired `AppShell` and `NavDrawer` across the routed UI. Added the mobile drawer
toggle and the scheduling/support badges so the main shell now matches the
ported navigation structure.

Nav structure mirrors `MainLayout.razor` (74 links, 7 groups):
- Channels, FFmpeg Profiles, Watermarks
- Media Sources: Local, Emby, Jellyfin, Plex
- Media: Libraries, Trash, TV Shows, Movies, Music, Images, Remote Streams
- Lists: Collections, Smart Collections, Multi-Collections, Playlists, Trakt,
  Filler Presets
- Scheduling (+ warning badge): Schedules, Blocks, Templates, Decos, Playouts
- Settings: FFmpeg, Logging, HDHomeRun, Scanner, Playout, UI, XMLTV
- Support (+ error/warning badge): Health Checks, Logs, Troubleshooting

**Verification:** all nav links route; drawer collapses on mobile; badge counts
refresh on their configured polling interval.

### 6.3 — Media Source Configuration ✓

Completed `server/media_sources.rs` plus the provider pages under
`pages/media_sources/{local,plex,jellyfin,emby}.rs`.

Each provider now has the expected list page, connection editor, libraries
editor, and path replacements editor. The interactive parts stay island-based
where needed, including scan actions and the Plex OAuth PIN flow.

Plex OAuth: island with `use_interval` polling `/api/plex/pin` every 2 seconds
until confirmed. Reference:
[`PlexLibraryEditor.razor`](/home/bart/ErsatzTV/ErsatzTV/Pages/PlexLibraryEditor.razor).

Shared component `ui/primitives/remote_media_source.rs`: mirrors
`RemoteMediaSourceEditor.razor`, `RemoteMediaSourceLibrariesEditor.razor`,
`RemoteMediaSourcePathReplacementsEditor.razor`.

**Verification:** local library save works; Jellyfin libraries load from saved
credentials; the Plex OAuth PIN flow completes.

### 6.4 — Library Browser and Collections ✓

Completed the library browser and collection pages under `pages/libraries/` and
`pages/media/{movies,shows,images,collections,smart_collections}.rs`.

The media browsers use `MediaCard` / `MediaCardPager`, plus `LetterBar` and
`FragmentLetterAnchor` equivalents for alphabetic quick navigation.

Collection editors include the debounced autocomplete island used for adding
media items.

**Verification:** browse populated movie library, paginate; create manual
collection, add items.

### 6.5 — Block Editor ✓

Ported `BlockEditor.razor` as a fully hydrated island with
`RwSignal<Vec<BlockItemForm>>` local state and the expected item editing flow.

"Preview Block Playout" calls `preview_block(id)` and renders the result in a
read-only `Modal`.

**Verification:** load existing block, reorder items, add item from collection,
save; preview returns playout table.

### 6.6 — YAML Playout Editor ✓

Ported `ScriptedPlayoutEditor.razor` as an island with a raw YAML editor,
debounced validation, inline errors, save, and playout rebuild wiring.

**Verification:** introduce syntax error → banner appears; fix → banner
clears; save → playout rebuilds.

### 6.7 — FFmpeg Profile Editor ✓

Ported `FFmpegSettingsEditor.razor` as a fully hydrated island. Conditional
sections react to the selected video format and "Copy Profile" opens the
expected `Modal` with a single `TextField` input.

**Verification:** add H264 profile; hardware sections react; copy profile
creates named copy.

### 6.8 — Graphics / Watermark Editor ✓

Ported `WatermarkEditor.razor` as an island with debounced live preview. Field
changes fetch `preview_watermark(form)`, render a drawvg-backed PNG preview,
and update the editor reactively.

**Verification:** move position sliders → preview updates within 500 ms; save.

### 6.9 — Playout Timeline Visualiser ✓

Implemented the SVG timeline island. `Resource` fetches
`list_playout_items(id, from, to)`, renders a proportional 24-hour time axis,
supports Prev/Next paging, and shows item detail on selection.

**Verification:** 24-hour window renders; items are proportionally placed;
clicking an item shows its title and time details.

## Islands Hydration Decision Matrix

| Feature | Strategy | Reason |
|---|---|---|
| Channel list, media grids, settings pages | SSR-only | Content fully knowable on server |
| Delete confirmation dialogs | Island | `.showModal()` is a DOM API |
| Channel editor conditional fields | Island | `StreamingMode` drives field visibility |
| Block item drag-and-drop | Island | Client-state reorder |
| Watermark live preview | Island | Debounced fetch + reactive image |
| YAML editor validation | Island | Keystroke debounce + error banner |
| Nav drawer mobile toggle | Island | Hamburger open/close is DOM state |
| Toast container | Island | Client-side timed dismiss |
| Search autocomplete | Island | Typeahead debounce + popover |
| Plex OAuth PIN polling | Island | `use_interval` 2-second polling |
| Playout timeline | Island | SVG time scrubbing |

## Server Function Pattern

```rust
// Read
#[server]
pub async fn list_channel_rows(
    show_disabled: bool, page: u32, page_size: u32,
) -> Result<Vec<ChannelListRow>, ServerFnError> {
    let state = expect_context::<AppState>();
    // sqlx query directly on state.pool
}

// Write
#[server]
pub async fn delete_channel(id: i64) -> Result<(), ServerFnError> {
    let state = expect_context::<AppState>();
    sqlx::query(r#"DELETE FROM "Channel" WHERE "Id" = ?"#)
        .bind(id).execute(&state.pool).await?;
    Ok(())
}
```

`AppState` is injected via `leptos_routes_with_context` in `main.rs`. Server
functions use `?` directly on `sqlx::Error` — it implements `std::error::Error`
and converts to `ServerFnError` via the blanket `From` impl.

## Porting References

| ErsatzTV file | Phase 6 target |
|---|---|
| `ErsatzTV/Shared/MainLayout.razor` | `ui/layout.rs` — nav structure and theme tokens |
| `ErsatzTV/Pages/Channels.razor` | `pages/channels/list.rs` |
| `ErsatzTV/Pages/ChannelEditor.razor` | `pages/channels/editor.rs` |
| `ErsatzTV/Pages/PlexLibraryEditor.razor` | `pages/media_sources/plex.rs` |
| `ErsatzTV/Pages/BlockEditor.razor` | `pages/scheduling/block_editor.rs` |
| `ErsatzTV/Pages/ScriptedPlayoutEditor.razor` | `pages/scheduling/yaml_editor.rs` |
| `ErsatzTV/Pages/WatermarkEditor.razor` | `pages/watermarks/editor.rs` |

## Critical Files

| File | Role |
|---|---|
| `crates/ersatzrs-web/src/app.rs` | Root routing + shell; add routes here each sub-phase |
| `crates/ersatzrs-web/src/main.rs` | `leptos_routes_with_context` injects `AppState` |
| `crates/ersatzrs-web/src/style/globals.module.css` | Design tokens (CSS custom properties) |
| `crates/ersatzrs-web/src/ui/primitives/table.rs` | `PagedTable<T>`; required by every list page |
| `crates/ersatzrs-web/src/server/channels.rs` | Reference implementation for server function pattern |
