# Trakt Lists Parity Slice

## Source Map

- ErsatzTV UI/pages:
  - `$ERSATZTV_SRC/ErsatzTV/Pages/TraktLists.razor`
  - `$ERSATZTV_SRC/ErsatzTV/Pages/TraktListEditor.razor`
  - `$ERSATZTV_SRC/ErsatzTV/Shared/AddTraktListDialog.razor`
- ErsatzTV components/view models:
  - `$ERSATZTV_SRC/ErsatzTV.Application/MediaCollections/TraktListViewModel.cs`
  - `$ERSATZTV_SRC/ErsatzTV.Application/MediaCollections/PagedTraktListsViewModel.cs`
  - `$ERSATZTV_SRC/ErsatzTV/ViewModels/TraktListEditViewModel.cs`
- ErsatzTV validators:
  - Add list validation is in `AddTraktListHandler.ValidateUrl`; editor booleans have no separate validator.
- ErsatzTV handlers/services:
  - `$ERSATZTV_SRC/ErsatzTV.Application/MediaCollections/Commands/AddTraktListHandler.cs`
  - `$ERSATZTV_SRC/ErsatzTV.Application/MediaCollections/Commands/TraktCommandBase.cs`
  - `$ERSATZTV_SRC/ErsatzTV.Application/MediaCollections/Commands/MatchTraktListItemsHandler.cs`
  - `$ERSATZTV_SRC/ErsatzTV.Application/MediaCollections/Commands/UpdateTraktListHandler.cs`
  - `$ERSATZTV_SRC/ErsatzTV.Application/MediaCollections/Commands/DeleteTraktListHandler.cs`
  - `$ERSATZTV_SRC/ErsatzTV.Infrastructure/Trakt/TraktApiClient.cs`
- ErsatzTV repository/query paths:
  - `$ERSATZTV_SRC/ErsatzTV.Application/MediaCollections/Queries/GetPagedTraktListsHandler.cs`
  - `$ERSATZTV_SRC/ErsatzTV.Application/MediaCollections/Queries/GetTraktListByIdHandler.cs`
  - `$ERSATZTV_SRC/ErsatzTV.Application/MediaCollections/Mapper.cs`
  - `$ERSATZTV_SRC/ErsatzTV.Infrastructure/Data/Configurations/Collection/TraktListConfiguration.cs`
- Side effects:
  - Add downloads Trakt list/items, saves `TraktList` and item GUIDs, matches local media, and updates match timestamps.
  - Match refreshes local `MediaItemId` matches and generated playlist items when enabled.
  - Update toggles daily refresh and generated playlist; enabling generated playlist creates the system playlist and schedules matching.
  - Delete removes the Trakt list and generated playlist linkage.
  - List page persists `pages.trakt.lists_page_size`.
- ErsatzRS target modules:
  - `crates/ersatzrs-core/src/trakt.rs`
  - `crates/ersatzrs-infra/src/trakt.rs`
  - `crates/ersatzrs-infra/src/trakt_sync.rs`
  - `crates/ersatzrs-web/src/admin/server_fns/trakt.rs`
  - `crates/ersatzrs-web/src/admin/pages/media/trakt_lists.rs`
  - `crates/ersatzrs-web/src/api/trakt.rs`
  - `crates/ersatzrs-web/src/host/router.rs`
  - `docs/reference/parity-matrix.md`, `STATUS.md`, `TEST.md`
- Intentional deviations:
  - ErsatzRS executes list actions directly instead of pushing queued background work through ErsatzTV's `ChannelWriter<IBackgroundServiceRequest>`; the existing synchronizer preserves the same persistence and generated playlist side effects.
  - ErsatzTV's Lucene search-index rebuild side effect maps to ErsatzRS SQL-backed scheduling search projection, which reads Trakt joins directly.

## Visual Source Map

- ErsatzTV layout containers:
  - `MudForm Style="max-height: 100%"`.
  - Square `MudPaper` action bar, 64px high, full width, centered actions, z-index 100.
  - Scrollable `div.d-flex.flex-column` with `height: 100vh; overflow-x: auto`.
  - `MudContainer MaxWidth.ExtraLarge Class="pt-8"` for both list and editor content.
- ErsatzTV action bars/buttons:
  - List action bar has a primary filled `Add Trakt List` button with add icon and left margin.
  - Editor action bar has a primary filled `Save Trakt List` button with save icon and left margin.
  - Row actions are icon buttons with tooltips: edit, search, refresh/match, delete.
- ErsatzTV tables/forms/pagers/dialogs:
  - List uses dense hover `MudTable`, columns `Id`, `Name`, `Match Status`, and a 240px action column.
  - Pager is `MudTablePager`, default page size loaded from `pages.trakt.lists_page_size`, with rows-per-page persistence.
  - Add dialog has title `Add Trakt List`, body label `Enter Trakt List URL`, a 400px minimum URL field, and `Cancel` / primary `Add` actions.
  - Editor form has three horizontal `MudStack` rows: `Id`, `Automatic Refresh`, `Generate Playlist`; checkbox captions are shown under the labels.
- Width/alignment/spacing rules:
  - Content width follows MudBlazor ExtraLarge, effectively full available admin width up to the XL container.
  - Table rows are dense, approximately 38px high; action column is fixed and right aligned.
  - Editor label column is fixed-width with input/control column to the right, collapsing vertically on small screens.
- Reference screenshots and viewport sizes:
  - No user screenshot is attached in this turn. Browser verification must capture ErsatzRS at a recorded viewport after `./run.sh serve`; live ErsatzTV screenshots are used when practical.
- ErsatzRS shared primitives/CSS modules to reuse:
  - `DeleteDialog`, `Modal`, `IconName`, `icon_style`, toast primitives, and the completed `filler_presets.module.css` layout pattern.
- Known visual deviations:
  - None accepted for the final state.

## Acceptance

- [x] UI fields/actions/routes/back-links match ErsatzTV.
- [x] Validation and error surfaces match ErsatzTV.
- [x] Persistence and side effects match ErsatzTV.
- [x] Critical save/action paths have SSR or browser-safe fallback.
- [x] UI layout has screenshot/bounding-box evidence against ErsatzTV or a Razor-derived visual contract.
- [x] Repeated defect classes have reusable tests or playbook coverage.
- [x] Parity docs, status, and active plan files are updated.

## ParityDebt

- None.

## Verification

- [x] `cargo xtask parity-check web`
- [x] Focused Trakt synchronizer/web tests.
- [x] Browser smoke for hydrated and fallback UI.
- [x] Visual check after `./run.sh serve`.

## Verification Evidence

- `cargo test -p ersatzrs-web trakt --lib` passed.
- `cargo test -p ersatzrs-infra --test trakt_sync` passed.
- `cargo xtask hydration-lint -p ersatzrs-web` passed.
- `cargo xtask parity-check web` passed.
- Live Trakt list URL check returned HTTP 200 on 2026-05-27; the URL was not saved in this repository.
- The real add/refresh path was exercised through `/api/trakt-lists/add` on `sqlite:///tmp/ersatzrs-trakt-live2.db` with `TRAKT__CLIENTID` configured. The route returned `303 /media/trakt/lists`; the temp database saved the supplied list with 1 `TraktListItem`, 3 `TraktListItemGuid` rows, and both `LastUpdate` and `LastMatch` populated.
- `./run.sh serve` rebuilt Leptos assets and served the route from `sqlite:///tmp/ersatzrs-trakt-lists-visual.db`.
- Browser visual smoke used a 1365x768 viewport:
  - `plans/references/trakt-lists-ersatzrs-list-2026-05-27.png`: list action bar 64px high, content/table width 1081px, fixed right action column with four 36px icon buttons, pager visible, no horizontal overflow.
  - `plans/references/trakt-lists-ersatzrs-add-dialog-2026-05-27.png`: Add dialog open with URL field and primary Add/Cancel actions.
  - `plans/references/trakt-lists-ersatzrs-editor-2026-05-27.png`: editor action bar 64px high, save button visible, three form rows aligned in a two-column layout, no horizontal overflow.
  - Add dialog opened through hydrated click with a 430px URL field and no console warnings/errors.
  - Delete dialog opened through hydrated click; Search href, Match POST form, and Delete POST form were present.
