# Blocks Parity Slice

## Source Map

- ErsatzTV UI/pages:
  - `$ERSATZTV_SRC/ErsatzTV/Pages/Blocks.razor`
  - `$ERSATZTV_SRC/ErsatzTV/Pages/BlockEditor.razor`
  - `$ERSATZTV_SRC/ErsatzTV/Shared/CopyBlockDialog.razor`
- ErsatzTV components/view models:
  - `$ERSATZTV_SRC/ErsatzTV/ViewModels/BlockItemEditViewModel.cs`
  - `$ERSATZTV_SRC/ErsatzTV/ViewModels/BlockItemsEditViewModel.cs`
  - `$ERSATZTV_SRC/ErsatzTV.Application/Scheduling/BlockGroupViewModel.cs`
  - `$ERSATZTV_SRC/ErsatzTV.Application/Scheduling/BlockViewModel.cs`
  - `$ERSATZTV_SRC/ErsatzTV.Application/Scheduling/BlockItemViewModel.cs`
  - `$ERSATZTV_SRC/ErsatzTV.Application/Scheduling/Mapper.cs`
- ErsatzTV validators:
  - `$ERSATZTV_SRC/ErsatzTV.Application/Scheduling/Commands/CreateBlockGroupHandler.cs`
  - `$ERSATZTV_SRC/ErsatzTV.Application/Scheduling/Commands/CreateBlockHandler.cs`
  - `$ERSATZTV_SRC/ErsatzTV.Application/Scheduling/Commands/CopyBlockHandler.cs`
  - `$ERSATZTV_SRC/ErsatzTV.Application/Scheduling/Commands/ReplaceBlockItemsHandler.cs`
- ErsatzTV handlers/services:
  - `CreateBlockGroupHandler`, `DeleteBlockGroupHandler`, `CreateBlockHandler`, `DeleteBlockHandler`, `CopyBlockHandler`, `ReplaceBlockItemsHandler`, `PreviewBlockPlayoutHandler`
  - `BlockPlayoutPreviewBuilder` and `BlockPlayoutBuilder` for preview/build behavior.
- ErsatzTV repository/query paths:
  - `GetAllBlockGroupsHandler`, `GetAllBlocksHandler`, `GetBlocksByBlockGroupIdHandler`, `GetBlockByIdHandler`, `GetBlockItemsHandler`
  - EF tables `BlockGroup`, `Block`, `BlockItem`, `BlockItemWatermark`, `BlockItemGraphicsElement`
- Side effects:
  - Create/delete group and block mutate the block tables.
  - Copy block clones child `BlockItem` rows plus watermark/graphics joins.
  - Save replaces block metadata and all child items, updates `DateUpdated`, and preserves overlay joins.
  - ErsatzTV leaves playout refresh after block save as a TODO; ErsatzRS currently follows that.
- ErsatzRS target modules:
  - `crates/ersatzrs-web/src/admin/pages/scheduling/blocks.rs`
  - `crates/ersatzrs-web/src/admin/pages/scheduling/block_editor.rs`
  - `crates/ersatzrs-web/src/admin/server_fns/blocks.rs`
  - `crates/ersatzrs-web/src/admin/pages/scheduling/*.module.css`
  - `docs/reference/parity-matrix.md`, `TEST.md`, `STATUS.md`
- Intentional deviations:
  - None accepted. Any remaining gap stays in `ParityDebt`.

## Visual Source Map

- ErsatzTV layout containers:
  - `Blocks.razor`: top-level `MudForm`, scrollable full-height flex column, `MudContainer MaxWidth.ExtraLarge Class="pt-8"`.
  - `BlockEditor.razor`: `MudForm`; fixed-height `MudPaper` action bar at 64px; scrollable flex body with `MudContainer MaxWidth.ExtraLarge Class="pt-8"`.
- ErsatzTV action bars/buttons:
  - Blocks list: add group button below `Block Group Name`; add block button below `Block Group` and `Block Name`; table row actions are icon buttons for Edit, Copy, Delete; group header has Delete Block Group icon aligned right after two 48px spacer columns.
  - Block editor: desktop action bar buttons `Save Block`, `Add Block Item`, `Preview Block Playout`; mobile `MudMenu` with the same actions.
- ErsatzTV tables/forms/pagers/dialogs:
  - Blocks table: `MudTable Dense Hover`, grouped by group name, search toolbar placeholder `Search for blocks`, columns `Name` plus 180px action column on non-xs viewports.
  - Block editor table: dense `MudTable`, columns `Collection`, `Playback Order`, `Show In EPG`, `Disable Watermarks`, 240px actions column. Selected row uses stronger typography.
  - Dialogs: `DeleteDialog` for block/group delete, `CopyBlockDialog` for block copy, preview table rendered below the editor when no item is selected.
- Width/alignment/spacing rules:
  - Upstream uses ExtraLarge max-width containers, 32px top padding, `mb-6` dividers, `form-field-stack gap-md-8 mb-5` two-column label/input rows that collapse at small viewports.
  - Table rows are dense MudBlazor rows; icon actions reserve 48px button slots.
- Reference screenshots and viewport sizes:
  - Existing ErsatzRS screenshot: `plans/references/blocks_ersatzrs_2026-05-26.png`, 3840x1918.
  - Existing ErsatzTV same-viewport screenshot: `plans/references/ersatztv_screenshots/0051_scheduling_blocks.png`, 3840x1918.
  - User-provided ErsatzTV reference screenshot: `plans/references/blocks_ErsatzTV_2026-05-25.png`, 3838x1689.
  - Updated ErsatzRS screenshot: `plans/references/blocks_ersatzrs_updated_2026-05-26.png`, 3838x1689.
- ErsatzRS shared primitives/CSS modules to reuse:
  - `DeleteDialog`, toast primitive, existing scheduling page CSS modules, table/action conventions from `/schedules`.
- Known visual deviations:
  - ErsatzRS uses the repo-native shell, logo, and support nav styling instead of ErsatzTV's MudBlazor shell chrome.
  - List-page form/table geometry has been brought in line with the captured ErsatzTV page: separated `Block Groups`/`Blocks` sections, underline inputs, toolbar search band, and reserved row action affordances.

## Acceptance

- [x] UI fields/actions/routes/back-links match the main ErsatzTV surfaces for list, group/block create/delete, copy, editor metadata, item target/playback, EPG/watermark flags, overlay selectors, and preview.
- [x] Validation and error surfaces match the important ErsatzTV handler rules for name uniqueness/length, duration bounds, and item target requirements.
- [x] Persistence and side effects match current ErsatzTV block table mutations, including copied overlay joins and full item replacement.
- [x] Critical save/action paths have SSR or browser-safe fallback.
- [x] UI layout has screenshot/bounding-box evidence against ErsatzTV or an attached reference screenshot.
- [x] Focused regression tests cover duration, item target validation, and labels.
- [x] Parity docs, status, and active plan files are updated after final verification.

## ParityDebt

- None open for the `/blocks` list/editor parity slice. Repo-native shell chrome remains an intentional application-wide ErsatzRS difference, not page-specific `ParityDebt`.

## Verification

- [x] `cargo check -p ersatzrs-web`
- [x] `cargo test -p ersatzrs-web blocks::tests -- --nocapture`
- [x] `cargo xtask parity-check web`
- [x] Browser smoke for hydrated list copy flow: `/blocks` copy modal opened, copy persisted, redirected to `/blocks/3`, and console warnings/errors were zero on the clean rebuilt run.
- [x] Browser smoke for hydrated editor flows: fresh Playwright/Chromium probe verified `+ Add Item` mutates the table without hangs or leaked DOM tokens; valid existing-item preview opens; `Save Block` redirects to `/blocks`; console warnings/errors and failed responses were zero after the serde-default fix.
- [x] Browser-safe fallback smoke: POST `/api/blocks/items/save` with `action=add` returned `303 /blocks/3`; SQLite showed one persisted `BlockItem` for block 3 with collection 1, chronological playback, and EPG enabled.
- [x] Same-viewport visual verification for `/blocks`: compared user-provided ErsatzTV `plans/references/blocks_ErsatzTV_2026-05-25.png` and ErsatzRS `plans/references/blocks_ersatzrs_updated_2026-05-26.png`, both 3838x1689; list section labels, label/control columns, vertical form rhythm, empty-state behavior, filter icon, and search toolbar geometry were corrected after comparison.
