# Schedules Parity Slice

## Source Map

- ErsatzTV UI/pages:
  - `$ERSATZTV_SRC/ErsatzTV/Pages/Schedules.razor`
  - `$ERSATZTV_SRC/ErsatzTV/Pages/ScheduleEditor.razor`
  - `$ERSATZTV_SRC/ErsatzTV/Pages/ScheduleItemsEditor.razor`
  - `$ERSATZTV_SRC/ErsatzTV/Shared/CopyScheduleDialog.razor`
  - `$ERSATZTV_SRC/ErsatzTV/Shared/ScheduleItemsDialog.razor`
- ErsatzTV components/view models:
  - `$ERSATZTV_SRC/ErsatzTV/ViewModels/ProgramScheduleEditViewModel.cs`
  - `$ERSATZTV_SRC/ErsatzTV/ViewModels/ProgramScheduleItemsEditViewModel.cs`
  - `$ERSATZTV_SRC/ErsatzTV/ViewModels/ProgramScheduleItemEditViewModel.cs`
  - `$ERSATZTV_SRC/ErsatzTV.Application/ProgramSchedules/ProgramScheduleViewModel.cs`
  - `$ERSATZTV_SRC/ErsatzTV.Application/ProgramSchedules/ProgramScheduleItem*ViewModel.cs`
- ErsatzTV validators:
  - `$ERSATZTV_SRC/ErsatzTV/Validators/ProgramScheduleItemEditViewModelValidator.cs`
  - handler validation in `$ERSATZTV_SRC/ErsatzTV.Application/ProgramSchedules/Commands/ProgramScheduleItemCommandBase.cs`
- ErsatzTV handlers/services:
  - `$ERSATZTV_SRC/ErsatzTV.Application/ProgramSchedules/Queries/GetPagedProgramSchedulesHandler.cs`
  - `$ERSATZTV_SRC/ErsatzTV.Application/ProgramSchedules/Queries/GetProgramScheduleByIdHandler.cs`
  - `$ERSATZTV_SRC/ErsatzTV.Application/ProgramSchedules/Queries/GetProgramScheduleItemsHandler.cs`
  - `$ERSATZTV_SRC/ErsatzTV.Application/ProgramSchedules/Commands/CreateProgramScheduleHandler.cs`
  - `$ERSATZTV_SRC/ErsatzTV.Application/ProgramSchedules/Commands/UpdateProgramScheduleHandler.cs`
  - `$ERSATZTV_SRC/ErsatzTV.Application/ProgramSchedules/Commands/DeleteProgramScheduleHandler.cs`
  - `$ERSATZTV_SRC/ErsatzTV.Application/ProgramSchedules/Commands/CopyProgramScheduleHandler.cs`
  - `$ERSATZTV_SRC/ErsatzTV.Application/ProgramSchedules/Commands/AddProgramScheduleItemHandler.cs`
  - `$ERSATZTV_SRC/ErsatzTV.Application/ProgramSchedules/Commands/ReplaceProgramScheduleItemsHandler.cs`
  - `$ERSATZTV_SRC/ErsatzTV.Application/MediaItems/Queries/GetAllLanguageCodesHandler.cs`
- ErsatzTV repository/query paths:
  - EF `TvContext.ProgramSchedules` with `Items`, `Playouts`, `ProgramScheduleAlternates`
  - EF `ProgramScheduleItems` includes collection, multi-collection, smart collection, rerun collection, playlist, media item metadata/artwork, filler presets, watermarks, graphics elements
  - `ConfigElementKey.SchedulesPageSize` and `ConfigElementKey.SchedulesDetailPageSize`
  - `MediaItemRepository.GetAllLanguageCodesAndNames()` derives override language options from `MediaStream.Language`, `Subtitle.Language`, and `Channel.PreferredAudioLanguageCode`
- Side effects:
  - schedule item add/replace refreshes every playout that uses the schedule with `BuildPlayout(playout.Id, Refresh)`
  - list page persists schedule list/detail rows-per-page config
  - copy clones schedule items plus watermark/graphics joins, resets playouts and alternates
  - delete removes the schedule and dependent schedule items/alternates through cascade behavior
- ErsatzRS target modules:
  - `crates/ersatzrs-web/src/admin/server_fns/schedules.rs`
  - `crates/ersatzrs-web/src/admin/pages/scheduling/schedules.rs`
  - `crates/ersatzrs-web/src/admin/pages/scheduling/schedules.module.css`
  - `crates/ersatzrs-web/src/admin/routes.rs`
  - `crates/ersatzrs-web/src/admin/server_fns/mod.rs`
  - existing repository support in `crates/ersatzrs-core/src/repositories/program_schedule.rs` and `crates/ersatzrs-infra/src/repositories/program_schedule.rs`
  - existing scheduler refresh queue in `crates/ersatzrs-web/src/admin/server_fns/media_browser.rs`
- Intentional deviations:
  - None planned for the ported surface. Any missing sub-surface remains `ParityDebt` until implemented or covered by an ADR.

## Visual Source Map

- ErsatzTV layout containers:
  - All three routes use a `MudForm` root, a square `MudPaper` top action bar at 64 px height, then a scrollable full-height column with `MudContainer MaxWidth.ExtraLarge` and `pt-8`.
  - Page headings are `MudText Typo.h5` with `mb-2`, followed by `MudDivider mb-6`.
- ErsatzTV action bars/buttons:
  - `/schedules`: top-left filled primary `Add Schedule` button with Add icon.
  - `/schedules/add` and `/schedules/{id}`: top-left filled primary `Add Schedule` or `Save Schedule` button with Add/Save icon.
  - `/schedules/{id}/items`: top bar shows schedule name or selected item summary on the left; desktop buttons `Save Schedule Items` and `Add Schedule Item` align right; mobile collapses to a More menu.
- ErsatzTV tables/forms/pagers/dialogs:
  - `/schedules` main `MudTable` is dense, hoverable, no breakpoint collapse, with search toolbar, name column, 300 px action column, edit/items/copy/troubleshoot/delete icon buttons, and `MudTablePager`.
  - Selected schedule detail table appears below the list on non-small screens, `mt-8`, with Start Time, Collection, Playout Mode columns and its own pager.
  - `/schedules/add` and `/schedules/{id}` editor form uses two-column `form-field-stack` rows: labels at left, controls at right. Fields are Name, Keep Multi-Part Episodes Together, Treat Collections As Shows, Shuffle Schedule Items, Random Start Point, Fixed Start Time Behavior.
  - `/schedules/{id}/items` table has Start Time, Collection, Playout Mode, and 240 px action column; row actions are copy, move up, move down, delete. Selected row reveals a long two-column editor with schedule item, filler, and override sections.
  - Copy dialog asks for `New Schedule Name` and redirects to copied schedule items on success.
  - Delete dialog confirms before destructive schedule deletion.
  - Preferred Audio/Subtitle Language overrides are clearable selects populated from media language codes.
- Width/alignment/spacing rules:
  - Use existing ErsatzRS extra-large admin surfaces at `max-width: 1920px`, 64 px top action bars, 24 px section dividers, dense table rows around 38 px, right-aligned row action icon groups, and two-column editor grids matching the existing filler/channel editor geometry.
- Reference screenshots and viewport sizes:
  - `plans/references/schedules_ErsatzTV_2026-05-25.png` at 3840 x 1918.
  - `plans/references/schedules_add_ErsatzTV_2026-05-25.png` at 3840 x 1918.
  - `plans/references/schedules_items_ErsatzTV_2026-05-25.png` at 3840 x 1918.
  - `plans/references/schedules-copy-dialog-2026-05-25.png` at 1920 x 959.
  - `plans/references/schedules-items-language-selectors-2026-05-25.png` at 1920 x 959.
  - Additional captured ErsatzTV references: `plans/references/ersatztv_screenshots/0045_scheduling_schedules.png`, `0046_scheduling_schedules_add_schedule.png`, `0047_scheduling_schedules_edit.png`, `0048_scheduling_schedules_edit_schedule_items.png`, `0049_scheduling_schedules_edit_schedule_items_add_schedule_item.png`, `0050_scheduling_schedules_edit_schedule_items_edit_schedule_item.png`, `0074_panel_edit_schedule_item_detail.png` all at 3840 x 1918 where present.
- ErsatzRS shared primitives/CSS modules to reuse:
  - Reuse the established admin top action bar/table/form CSS pattern from filler presets and routed media pages.
  - Reuse `IconName::{Add,Save,Edit,Delete,Copy,Info,FilterList,ChevronLeft,ChevronRight}` and `DeleteDialog`.
- Known visual deviations:
  - None accepted as final. Any screenshot mismatch after implementation must remain below in `ParityDebt`.

## Acceptance

- [x] UI fields/actions/routes/back-links match ErsatzTV for `/schedules`, `/schedules/add`, `/schedules/:id`, and the hydrated `/schedules/:id/items` editor surface.
- [x] Validation and error surfaces match ErsatzTV's schedule item command rules for fixed start, multiple count, duration bounds, collection references, playback-order consistency, and filler padding dependencies.
- [x] Persistence and side effects match ErsatzTV's schedule item replacement contract: existing items are deleted, rebuilt with contiguous indexes and subtype rows, watermark/graphics joins are preserved, and referenced playouts receive `BuildPlayout(Refresh)`.
- [x] Critical save/action paths have SSR or browser-safe fallback.
- [x] UI layout has screenshot/bounding-box evidence against ErsatzTV or the captured reference screenshots.
- [x] Repeated defect classes have reusable tests or playbook coverage.
- [x] Parity docs, status, and active plan files are updated.

## ParityDebt

- None.

## Verification

- [x] `cargo xtask parity-check web`
- [x] `cargo fmt --all -- --check`
- [x] `cargo check -p ersatzrs-web`
- [x] `cargo test -p ersatzrs-web admin::server_fns::schedules::tests`
- [x] `cargo test -p ersatzrs-web admin::server_fns::schedules::tests::schedule_item_form_deserializes_default_multi_selects_when_omitted`
- [x] `cargo test -p ersatzrs-web admin::server_fns::schedules::tests::normalization_matches_schedule_item_view_model_dependency_rules`
- [x] `cargo test -p ersatzrs-web admin::server_fns::schedules::tests::load_language_options_matches_media_language_codes --features ssr`
- [x] `cargo clippy -p ersatzrs-web --all-targets -- -D warnings`
- [x] Focused repository/app tests for schedule commands and validation.
- [x] HTTP route smoke after `./run.sh serve`: `/schedules`, `/schedules/add`, and `/schedules/1/items` returned 200 and rendered the expected route headings/actions.
- [x] Browser visual/action smoke for list, add/edit save, copy/delete, and schedule item save: Playwright captured `plans/references/schedules-ersatzrs-2026-05-25.png`, `plans/references/schedules-add-ersatzrs-2026-05-25.png`, and `plans/references/schedules-items-ersatzrs-2026-05-25.png` at 1920 x 959, matching the ErsatzTV reference aspect ratio. The `/schedules`, `/schedules/add`, and `/schedules/:id/items` action bars rendered within bounds with `overflow=0`; the item page rendered without console errors after the raw `<noscript>` fallback fix.
- [x] Hydrated item save regression: Playwright clicked `Save Schedule Items` on `/schedules/3/items` with an item that has no watermarks or graphics elements; the row remained `Dynamic / English Comedy / One`, no visible error rendered, and the previous `missing field watermark_ids` server-function deserialization error did not recur.
- [x] Hydrated dependency-rule smoke: Playwright opened `/schedules/3/items`, selected an existing item, confirmed Multiple Mode options were narrowed to `Count` / `Collection Size` and Fill With Group Mode labels were `(none)` / `Ordered Groups` / `Shuffled Groups`, then clicked `Save Schedule Items`; both `English Comedy` and `scifi` rows remained and no visible error rendered.
- [x] Hydrated copy/delete dialog smoke: Playwright opened `/schedules`, clicked row `Copy Schedule`, confirmed the `Copy Schedule` modal with `Enter a name for the new Schedule` and `New Schedule Name`, verified blank submit keeps the modal open with `New Schedule Name is required.`, created a temporary copied schedule and landed on `/schedules/{id}/items`, then deleted the temporary copy through the `Confirm Delete` dialog. Screenshot: `plans/references/schedules-copy-dialog-2026-05-25.png`.
- [x] Hydrated language selector smoke: Playwright opened `/schedules/3/items`, selected the `English Comedy` row, and verified both Preferred Audio Language and Preferred Subtitle Language render as `<select>` controls with 53 live options derived from media language codes, starting with `(none)`, `eng`/English, `dut`/Dutch, `und`/Undetermined, `fra`/French, and `spa`/Spanish. Screenshot: `plans/references/schedules-items-language-selectors-2026-05-25.png`.
- [x] No-JS item fallback smoke: raw HTML for `/schedules/:id/items` includes `schedule-items-fallback-form` posting to `/api/schedules/items/save`, and a form POST returned `303 See Other` to `/schedules/:id/items`.
