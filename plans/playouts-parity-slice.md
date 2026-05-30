# `/playouts` Parity Slice

Closes the ErsatzTV parity gap for the `/playouts` admin page. The previous
ErsatzRS implementation was an ErsatzRS-only read-only SVG timeline hub; this
slice replaces it with a faithful port of `Pages/Playouts.razor`: an action bar
(Add Playout menu + Reset All Playouts), a searchable paged playouts table with
per-kind row actions, a lock spinner, build-failure highlighting, and a paged
playout-detail table with a Show Filler toggle and Troubleshoot dialog.

## Source Map

- **ErsatzTV UI/pages:** `ErsatzTV/Pages/Playouts.razor` (list + detail +
  action bar), `ErsatzTV/Pages/PlayoutEditor.razor`
  (`playouts/add[/{Kind}]`), `ErsatzTV/Pages/ClassicPlayoutEditor.razor`,
  `ErsatzTV/Pages/BlockPlayoutEditor.razor`, and
  `ErsatzTV/Pages/ScriptedPlayoutEditor.razor`,
  `ErsatzTV/Pages/PlayoutAlternateSchedulesEditor.razor`, and
  `ErsatzTV/Pages/PlayoutTemplatesEditor.razor`.
- **ErsatzTV components/view models:**
  - `Application/Playouts/PlayoutNameViewModel.cs`,
    `PlayoutItemViewModel.cs`, `PagedPlayoutsViewModel.cs`,
    `PagedPlayoutItemsViewModel.cs`, `Mapper.cs` (`GetDisplayTitle`,
    `PlayoutItem.GetDisplayDuration`).
  - Dialogs: `Shared/DeleteDialog.razor`, `Shared/SchedulePlayoutReset.razor`,
    `Shared/EditExternalJsonFileDialog.razor`, `Shared/SchedulingContextDialog.razor`.
- **ErsatzTV validators:** none for the list page itself (editors validate).
- **ErsatzTV handlers/services:**
  - `Application/Playouts/Queries/GetPagedPlayoutsHandler.cs`
    (count is **unfiltered total**; page filters `Channel.Name LIKE %q%`,
    orders by `Channel.SortNumber`).
  - `Application/Playouts/Queries/GetFuturePlayoutItemsByIdHandler.cs`
    (items + gaps, `Finish >= now`, ShowFiller filter, combined, ordered by
    `Start`, paged; gaps render as `UNSCHEDULED`; `Mapper.GetDisplayTitle`
    fallback/chapter rules drive media-item title projection).
  - `Application/Playouts/Commands/DeletePlayoutHandler.cs`,
    `ResetAllPlayoutsHandler.cs`, `BuildPlayout.cs` (`Classic → Refresh`,
    `Block/Sequential/Scripted → Reset`, `ExternalJson/None → skip`),
    `UpdateExternalJsonPlayoutHandler.cs`, `UpdatePlayoutHandler.cs`
    (DailyRebuildTime), and the `Create*PlayoutHandler.cs` family (channel
    ownership, program-schedule/file validation, insert, initial reset build).
  - `GetPlayoutAlternateSchedulesHandler.cs` appends the current default
    `ProgramSchedule` as the final catch-all row; `ReplacePlayoutAlternateScheduleItemsHandler.cs`
    saves all but the final row as `ProgramScheduleAlternate`, saves the final
    row to `Playout.ProgramScheduleId`, and enqueues a Refresh build when an
    existing future playout window would switch schedules.
  - `Scheduling/Queries/GetPlayoutTemplatesHandler.cs` loads
    `PlayoutTemplate` rows with Template/TemplateGroup and optional
    DecoTemplate/DecoTemplateGroup; `ReplacePlayoutTemplateItemsHandler.cs`
    add/update/removes `PlayoutTemplate` rows and stamps `DateUpdated`.
  - `Application/ProgramSchedules/Queries/ProcessSchedulingContextHandler.cs`
    (enrich Classic/Block scheduling context JSON with names).
- **ErsatzTV repository/query paths:** EF `TvContext` `Playouts`,
  `PlayoutItems`, `PlayoutGaps`, `ProgramSchedules`, `ProgramScheduleItems`,
  `Blocks`, `BlockItems`, `FillerPresets`, `ConfigElements`.
- **Side effects:** delete cascades child rows + refreshes channel list +
  deletes channel-guide cache; reset/refresh enqueue a background build through
  the worker channel under the shared `IEntityLocker`; external-json update
  refreshes channel data; page sizes / show-filler persisted to
  `ConfigElement` (`pages.playouts.*`).
- **ErsatzRS target modules:**
  - `crates/ersatzrs-web/src/admin/pages/scheduling/playouts.rs` (page + island)
  - `crates/ersatzrs-web/src/admin/pages/scheduling/playout_editor.rs`
    (`/playouts/add[/{Kind}]`)
  - `crates/ersatzrs-web/src/admin/pages/scheduling/playout_kind_editor.rs`
    (`/playouts/{classic,block,scripted}/{id}`)
  - `crates/ersatzrs-web/src/admin/pages/scheduling/playout_nested_editor.rs`
    (`/playouts/{id}/alternate-schedules`, `/playouts/{id}/templates`)
  - `crates/ersatzrs-web/src/api/playouts.rs` (HTML POST fallback)
  - `crates/ersatzrs-web/src/admin/pages/scheduling/playouts.module.css`
  - `crates/ersatzrs-web/src/admin/pages/scheduling/playout_editor.module.css`
  - `crates/ersatzrs-web/src/admin/server_fns/playouts.rs` (server fns)
  - `crates/ersatzrs-core/src/locking.rs` (`locked_playouts()`)
  - `crates/ersatzrs-scheduler/src/runner.rs` (`SchedulerOptions.locker`)
  - `crates/ersatzrs-web/src/host/{bootstrap,tasks,state}.rs` (shared locker)
  - `crates/ersatzrs-web/src/admin/i18n.rs` + `languages/*` (i18n)
- **Intentional deviations:**
  - Reset/refresh enqueue a `PlayoutBuildRequest` over the in-process
    `playout_build_tx` channel (the scheduler is the sole builder, mirroring
    ErsatzTV's `WorkerService` model) rather than ErsatzTV's MediatR worker
    channel. Same single shared `EntityLocker`.
  - ErsatzRS serves `channels.m3u`/XMLTV on-the-fly with no per-channel
    guide-cache file, so delete/update-json have no cache file to remove and no
    `RefreshChannelList`/`RefreshChannelData` cache rewrite — the DB change is
    the whole side effect. Consistent with the existing YAML-save divergence
    documented in `server_fns/playouts.rs`.
  - The read-only ErsatzRS-only SVG timeline (`scheduling/timeline.rs`) is
    **removed**; ErsatzTV has no timeline. The parity detail is a table.
  - `PagedTable` has no rows-per-page selector across the admin UI, so playout
    list/detail page sizes are fixed at ErsatzTV's default (10). The
    `pages.playouts.detail_show_filler` preference *is* persisted (it is
    user-toggleable). `pages.playouts.page_size` /
    `pages.playouts.detail_page_size` are read for forward-compat but not
    user-changeable. Consistent with every other ErsatzRS list page.

## Visual Source Map

Reference: `ErsatzTV/Pages/Playouts.razor` plus the Classic/Block/Scripted
playout editor Razor pages. ErsatzTV viewport for comparison 1280×800 (admin
default). ErsatzRS browser evidence:
`plans/references/playouts-1280x800.png` and
`plans/references/playouts-1280x800-snapshot.md`; post-fix hydration/cache
proof is `plans/references/playouts-1280x800-after-hydration-fix.png`.
Nested editor smoke evidence is
`plans/references/playouts-alternate-schedules-1280x800.png` and
`plans/references/playouts-templates-1280x800.png`; Block preview/calendar
and mobile action proof are
`plans/references/playouts-templates-preview-1280x800.png` and
`plans/references/playouts-templates-mobile-actions-390x844.png`.

- **Layout containers:**
  - Top `MudPaper Square` action bar, fixed 64px height, full width,
    `align-items: center`, content padded `ml-6 mr-6`. Right-aligned cluster:
    `Add Playout` `MudMenu` (filled primary button, `Add` start icon) +
    `Reset All Playouts` filled **warning** button (`Refresh` start icon,
    `ml-3`). Mobile (`d-md-none`) collapses both into a `MoreVert` menu.
  - Body: `MudContainer MaxWidth.ExtraLarge` (`pt-8`), `h-100vh` scroll column.
  - `MudText h5` "Playouts" + `MudDivider mb-6`.
  - List: dense hover `MudTable`, server data, `ColGroup` last col width
    `225px`. Toolbar = search `MudTextField` (FilterList start adornment,
    clearable, placeholder "Search for playouts"). Headers: Channel /
    Default Schedule (`d-none d-md-table-cell`) / Schedule Kind
    (`d-none d-md-table-cell`) / (actions). `MudTablePager` footer.
  - Detail (when selected): `MudText h5 mt-10 mb-2` "Playout Detail" +
    `MudDivider mb-6`; either a `MudAlert Warning` (build message) or a dense
    hover `MudTable` with toolbar `MudSwitch` "Show Filler" (secondary) and
    headers Start / Finish / Media Item / Duration / (actions). Pager footer.
- **Action bars/buttons:** row action cell is a flex row, 48px-wide icon
  buttons; a leading 48px slot holds the lock `MudProgressCircular` (small,
  indeterminate) when the playout is locked; empty 48px spacers keep the Delete
  button column-aligned across kinds. Tooltips per icon.
- **Row classes:** `selected` / `playout-build-failure` /
  `playout-build-failure-selected` highlight selected and failed-build rows;
  detail rows get `playout-filler-{kind}` colouring.
- **ErsatzRS shared primitives/CSS to reuse:** `dialog::{Modal, DeleteDialog}`,
  `icon::{icon_style, IconName}`, toast store, the established list-island
  pattern from `channels/list.rs`, and `scheduling/playouts.module.css`
  patterns (`.page`, `.page-title`, `.cell-muted`, table styles). Custom table
  markup (not `PagedTable`) is used so rows are click-selectable with the
  selected / build-failure row classes and per-kind action cells.
- **Known visual deviations:** lock indicator is a CSS spinner (parity with the
  libraries-page `ScanIndicator`) rather than `MudProgressCircular`. Mobile
  `MoreVert` collapse is rendered with the same dropdown the desktop Add menu
  uses, shown via CSS at small widths.
- **Editor pages:** Classic has an empty 64px top bar and the Alternate
  Schedules + Maintenance sections; Block has the Save Default Deco top bar,
  Playout Templates link row, Default Deco toggle/group/deco selects, and
  Maintenance erase rows; Scripted has Save Scripted Schedule, a single schedule
  file text field, and Maintenance erase. All three use the same ExtraLarge
  container and form-field stack spacing as ErsatzTV.
- **Nested editor pages:** both nested routes now render a 64px top action bar,
  ExtraLarge-width content, an ordered selectable table with 180px action
  column, and selected-row editor sections for Schedule/Template/Deco Template,
  date range, start/end date fields, day-of-week, day-of-month, and month
  filters. Desktop action bars use Save/Add/Preview buttons with icons; mobile
  action bars collapse into MoreVert menus. Row actions are icon-only Up/Down/
  Delete buttons with ErsatzTV-style disabled states. Day and month displays
  use localized names/abbreviations, the date selectors disable when date-range
  filtering is off, Weekdays/Weekends helper buttons update the day filter,
  and the Block editor renders the ErsatzTV Preview Calendar branch when no
  row is selected. Persistence mirrors the upstream handlers: Classic treats
  the final row as the default `ProgramScheduleId`, while Block replaces
  `PlayoutTemplate` rows and stamps `DateUpdated`.

## Acceptance

- [x] UI fields/actions/routes/back-links match ErsatzTV.
- [x] Validation and error surfaces match ErsatzTV (build-failure alert + row).
- [x] Persistence and side effects match ErsatzTV (delete cascade, reset/refresh
      build dispatch, schedule-reset DailyRebuildTime, external-json update,
      show-filler persistence).
- [x] Critical save/action paths run inside the hydrated island; SSR renders the
      table + links without JS; add-playout form also has an HTML POST fallback.
- [x] UI layout has screenshot/bounding-box evidence against the Razor contract.
- [x] Repeated defect classes covered: card/row action handlers attach,
      `DeleteDialog`/`Modal` always rendered, build-dispatch side effects.
- [x] Parity docs, status, and this plan updated.

## ParityDebt

- None for `/playouts`.

## Verification

- [x] `cargo xtask parity-check web`
- [x] Focused `ersatzrs-web` server-fn unit tests (duration fmt, reset mode
      mapping, scheduling-context enrichment, `GetDisplayTitle` fallback and
      chapter/artist projection, add-playout create validation/insert).
- [x] Browser smoke for hydrated list selection, search, reset/delete dialogs,
      detail Show Filler, and Troubleshoot dialog.
- [x] Browser smoke after the hydration fix: `/playouts` at 1280×800 with
      fresh `bundle.css`/WASM resources, no page overflow, action bar 64px,
      content/table/pager aligned inside the ExtraLarge container; `/playouts/add`
      loaded channel/program-schedule selects without hydration console errors.
- [x] Browser smoke for `/playouts/{classic,block,scripted}/:id` at 1280×800
      using temporary local SQLite rows: all three real editor forms rendered
      the expected ErsatzTV headings/actions/fields with zero current console
      warnings/errors; Scripted route-param parsing was moved out of the island
      after browser smoke caught the initial hydration panic. Temporary rows
      were deleted after the smoke.
- [x] Browser smoke for `/playouts/{id}/alternate-schedules` and
      `/playouts/{id}/templates` at 1280×800 using temporary local SQLite rows:
      both nested editor routes rendered with zero current console
      warnings/errors; temporary rows were deleted after the smoke.
- [x] Browser smoke for final nested-editor visual controls: desktop
      alternate-schedule and template editors at 1280×800, populated Block
      Preview Calendar at 1280×800, and mobile MoreVert action menus at
      390×844; zero current console warnings/errors.
