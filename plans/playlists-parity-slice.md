# Playlists Parity Slice

## Source Map

- ErsatzTV UI/pages:
  - `$ERSATZTV_SRC/ErsatzTV/Pages/Playlists.razor`
  - `$ERSATZTV_SRC/ErsatzTV/Pages/PlaylistEditor.razor`
- ErsatzTV components/view models:
  - `$ERSATZTV_SRC/ErsatzTV/ViewModels/PlaylistItemsEditViewModel.cs`
  - `$ERSATZTV_SRC/ErsatzTV/ViewModels/PlaylistItemEditViewModel.cs`
  - `$ERSATZTV_SRC/ErsatzTV/ViewModels/PlaylistTreeItemViewModel.cs`
  - `$ERSATZTV_SRC/ErsatzTV.Application/MediaCollections/PlaylistGroupViewModel.cs`
  - `$ERSATZTV_SRC/ErsatzTV.Application/MediaCollections/PlaylistViewModel.cs`
  - `$ERSATZTV_SRC/ErsatzTV.Application/MediaCollections/PlaylistItemViewModel.cs`
  - `$ERSATZTV_SRC/ErsatzTV.Application/MediaCollections/Mapper.cs`
- ErsatzTV validators:
  - `$ERSATZTV_SRC/ErsatzTV.Application/MediaCollections/Commands/CreatePlaylistGroupHandler.cs`
  - `$ERSATZTV_SRC/ErsatzTV.Application/MediaCollections/Commands/CreatePlaylistHandler.cs`
  - `$ERSATZTV_SRC/ErsatzTV.Application/MediaCollections/Commands/ReplacePlaylistItemsHandler.cs`
- ErsatzTV handlers/services:
  - `$ERSATZTV_SRC/ErsatzTV.Application/MediaCollections/Commands/CreatePlaylistGroupHandler.cs`
  - `$ERSATZTV_SRC/ErsatzTV.Application/MediaCollections/Commands/CreatePlaylistHandler.cs`
  - `$ERSATZTV_SRC/ErsatzTV.Application/MediaCollections/Commands/DeletePlaylistGroupHandler.cs`
  - `$ERSATZTV_SRC/ErsatzTV.Application/MediaCollections/Commands/DeletePlaylistHandler.cs`
  - `$ERSATZTV_SRC/ErsatzTV.Application/MediaCollections/Commands/ReplacePlaylistItemsHandler.cs`
  - `$ERSATZTV_SRC/ErsatzTV.Application/MediaCollections/Commands/PreviewPlaylistPlayoutHandler.cs`
  - `$ERSATZTV_SRC/ErsatzTV.Application/MediaCollections/Queries/GetAllPlaylistGroupsHandler.cs`
  - `$ERSATZTV_SRC/ErsatzTV.Application/MediaCollections/Queries/GetPlaylistTreeHandler.cs`
  - `$ERSATZTV_SRC/ErsatzTV.Application/MediaCollections/Queries/GetPlaylistByIdHandler.cs`
  - `$ERSATZTV_SRC/ErsatzTV.Application/MediaCollections/Queries/GetPlaylistItemsHandler.cs`
- ErsatzTV repository/query paths:
  - `$ERSATZTV_SRC/ErsatzTV.Infrastructure/Data/TvContext.cs`
  - `$ERSATZTV_SRC/ErsatzTV.Infrastructure/Data/Configurations/Collection/PlaylistConfiguration.cs`
  - `$ERSATZTV_SRC/ErsatzTV.Infrastructure/Data/Configurations/Collection/PlaylistGroupConfiguration.cs`
  - `$ERSATZTV_SRC/ErsatzTV.Infrastructure/Data/Configurations/Collection/PlaylistItemConfiguration.cs`
  - `$ERSATZTV_SRC/ErsatzTV.Infrastructure/Data/Repositories/MediaCollectionRepository.cs`
- Side effects:
  - Playlist group and playlist delete are blocked for `IsSystem`.
  - Save replaces all `PlaylistItem` rows for the playlist transactionally and updates the playlist name.
  - Preview builds an unsaved playlist-shaped program schedule and limits output to one playlist pass.
  - ErsatzTV does not enqueue playout refreshes directly from playlist editor saves.
- ErsatzRS target modules:
  - `crates/ersatzrs-web/src/admin/server_fns/media_browser.rs`
  - `crates/ersatzrs-web/src/admin/pages/media/playlists.rs`
  - `crates/ersatzrs-web/src/api/playlists.rs`
  - `crates/ersatzrs-web/src/admin/pages/media/collections.module.css`
  - `crates/ersatzrs-web/src/admin/routes.rs`
  - `crates/ersatzrs-web/src/admin/pages/media/mod.rs`
  - `docs/reference/parity-matrix.md`
  - `docs/reference/persistence-gap-matrix.md`
  - `STATUS.md`
  - `TEST.md`
- Intentional deviations:
  - The Rust editor uses native Leptos controls and explicit item selectors rather than MudBlazor `MudAutocomplete`, while preserving fields, labels, validation, persistence, and browser-safe save/action fallbacks.

## Acceptance

- [x] UI fields/actions/routes/back-links match ErsatzTV.
- [x] Validation and error surfaces match ErsatzTV.
- [x] Persistence and side effects match ErsatzTV.
- [x] Critical save/action paths have SSR or browser-safe fallback.
- [x] Repeated defect classes have reusable tests or playbook coverage.
- [x] Parity docs, status, and active plan files are updated.

## ParityDebt

- None.

## Verification

- [x] `cargo xtask parity-check web`
- [x] Focused server-function tests for playlist validation and replace planning.
- [x] Focused Classic scheduling test proving playlist preview reference data uses `PlaylistEnumerator` through `PlayoutBuilder`.
- [x] Focused API parser test for HTML POST playlist save fields.
- [x] Browser smoke for `/media/playlists` and `/media/playlists/:id`: add group, add playlist, edit/save redirect, preview empty state, delete playlist, delete group, and console warning/error check on an isolated SQLite database with `./run.sh serve`.
