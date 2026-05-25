# Rerun Collections Parity Slice

## Source Map

- ErsatzTV UI/pages: `$ERSATZTV_SRC/ErsatzTV/Pages/RerunCollections.razor`, `$ERSATZTV_SRC/ErsatzTV/Pages/RerunCollectionEditor.razor`.
- ErsatzTV components/view models: `$ERSATZTV_SRC/ErsatzTV/ViewModels/RerunCollectionEditViewModel.cs`, `$ERSATZTV_SRC/ErsatzTV.Application/MediaCollections/RerunCollectionViewModel.cs`, `PagedRerunCollectionsViewModel.cs`, `MediaCollections/Mapper.cs`, and `MediaItems/Mapper.cs`.
- ErsatzTV validators: `$ERSATZTV_SRC/ErsatzTV.Application/MediaCollections/Commands/CreateRerunCollectionHandler.cs`, `UpdateRerunCollectionHandler.cs`, and `$ERSATZTV_SRC/ErsatzTV.Application/Validators/StringValidation.cs`.
- ErsatzTV handlers/services: `GetPagedRerunCollectionsHandler`, `GetRerunCollectionByIdHandler`, `SearchRerunCollectionsHandler`, `CreateRerunCollectionHandler`, `UpdateRerunCollectionHandler`, and `DeleteRerunCollectionHandler`.
- ErsatzTV repository/query paths: `$ERSATZTV_SRC/ErsatzTV.Infrastructure/Data/Configurations/Collection/RerunCollectionConfiguration.cs`, `$ERSATZTV_SRC/ErsatzTV.Infrastructure/Data/Repositories/MediaCollectionRepository.cs` `GetRerunCollectionItems` and `PlayoutIdsUsingRerunCollection`.
- Side effects: list page stores `pages.rerun_collections.page_size`; update enqueues `BuildPlayout(playoutId, Refresh)` for playouts referenced by `PlayoutProgramScheduleAnchor.RerunCollectionId`; delete relies on database cascades and does not explicitly enqueue.
- ErsatzRS target modules: `crates/ersatzrs-web/src/admin/pages/media/rerun_collections.rs`, `rerun_collections.module.css`, `admin/server_fns/media_browser.rs`, `api/rerun_collections.rs`, `api/mod.rs`, `admin/routes.rs`, `host/router.rs`, `admin/ui/primitives/icon.rs`, `docs/reference/parity-matrix.md`, `docs/reference/persistence-gap-matrix.md`, `STATUS.md`, and `TEST.md`.
- Intentional deviations: ErsatzRS uses native HTML selects for browser-safe fallback instead of MudBlazor autocomplete popovers. The persisted ids, labels, conditional fields, routes, validation, and playout-refresh side effect match the ErsatzTV contract.

## Visual Source Map

- ErsatzTV layout containers: outer `MudForm`; square `MudPaper` action bar at 64 px height; scroll region with `height: 100vh; overflow-x: auto`; `MudContainer MaxWidth.ExtraLarge` with top padding; `MudText Typo.h5`; `MudDivider`.
- ErsatzTV action bars/buttons: list has a primary `Add Rerun Collection` button with add icon on the left; editor has a primary save/add button with save icon in the same 64 px top action bar.
- ErsatzTV tables/forms/pagers/dialogs: list uses a dense hover `MudTable`, toolbar search field with filter icon, two columns with a 120 px action column, edit/delete icon buttons, `MudTablePager`, and a `DeleteDialog` titled `Delete Rerun Collection`. Editor uses two-column `form-field-stack` rows for `Name`, `Collection Type`, type-specific target selector, `First Run Playback Order`, and `Rerun Playback Order`.
- Width/alignment/spacing rules: reference screenshot shows a 3835x1435 capture with left drawer about 352 px wide, top app bar about 96 px high, list action bar about 64 px high, content starting around x=692, and the table band centered inside the extra-large content width. The ErsatzRS CSS mirrors the 64 px action bar, 1536 px centered content, 1.5rem title, dense 38 px rows, 120 px action column, and right-aligned pager.
- Reference screenshots and viewport sizes: `plans/references/media_rerun_collections_ErsatzTV_2026-05-25.png` (`3835x1435`) and `plans/references/media_rerun_collections_ErsatzRS_2026-05-25.png` (`2048x768`, proportional capture after rebuild).
- ErsatzRS shared primitives/CSS modules to reuse: shared `DeleteDialog`, toast and icon primitives, plus the filler-preset table/action-bar geometry adapted into a dedicated rerun-collection CSS module.
- Known visual deviations: native select controls replace MudAutocomplete popovers in the editor while preserving the same labels, persisted ids, and browser fallback contract.

## Acceptance

- [x] UI fields/actions/routes/back-links match ErsatzTV.
- [x] Validation and error surfaces match ErsatzTV.
- [x] Persistence and side effects match ErsatzTV.
- [x] Critical save/action paths have SSR or browser-safe fallback.
- [x] UI layout has screenshot/bounding-box evidence against the ErsatzTV reference screenshot.
- [x] Repeated defect classes have reusable tests or playbook coverage.
- [x] Parity docs, status, and active plan files are updated.

## ParityDebt

None.

## Visual Evidence

- Browser capture: `plans/references/media_rerun_collections_ErsatzRS_2026-05-25.png` at `2048x768`.
- Geometry: drawer `224x712` at `x=0`; add button `221x36` at `x=248`; content/table band `1536x177` at `x=368`; title `1536x32` at `x=368`; pager `1536x52` at `x=368`; body scroll width equals viewport width (`2048`).
- Smoke result: add/save/delete flow created `Codex Rerun Smoke ...`, deleted it through the hydrated delete dialog, and left `0` matching table rows with no console warnings, console errors, or page errors.

## Verification

- [x] `cargo xtask parity-check web`
- [x] Focused repository/app/scanner tests, when non-web behavior changed.
- [x] Browser smoke for hydrated or fallback UI.
