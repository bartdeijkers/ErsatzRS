# Decos and Deco Templates Parity Slice

## Source Map

- ErsatzTV UI/pages:
  - `$ERSATZTV_SRC/ErsatzTV/Pages/Decos.razor`
  - `$ERSATZTV_SRC/ErsatzTV/Pages/DecoEditor.razor`
  - `$ERSATZTV_SRC/ErsatzTV/Pages/DecoTemplates.razor`
  - `$ERSATZTV_SRC/ErsatzTV/Pages/DecoTemplateEditor.razor`
- ErsatzTV components/view models:
  - `$ERSATZTV_SRC/ErsatzTV/ViewModels/DecoTreeItemViewModel.cs`
  - `$ERSATZTV_SRC/ErsatzTV/ViewModels/DecoTemplateTreeItemViewModel.cs`
  - `$ERSATZTV_SRC/ErsatzTV/ViewModels/DecoEditViewModel.cs`
  - `$ERSATZTV_SRC/ErsatzTV/ViewModels/DecoBreakContentEditViewModel.cs`
  - `$ERSATZTV_SRC/ErsatzTV/ViewModels/DecoTemplateItemsEditViewModel.cs`
  - `$ERSATZTV_SRC/ErsatzTV/ViewModels/DecoTemplateItemEditViewModel.cs`
  - `$ERSATZTV_SRC/ErsatzTV.Application/Scheduling/DecoGroupViewModel.cs`
  - `$ERSATZTV_SRC/ErsatzTV.Application/Scheduling/DecoViewModel.cs`
  - `$ERSATZTV_SRC/ErsatzTV.Application/Scheduling/DecoTemplateGroupViewModel.cs`
  - `$ERSATZTV_SRC/ErsatzTV.Application/Scheduling/DecoTemplateViewModel.cs`
  - `$ERSATZTV_SRC/ErsatzTV.Application/Scheduling/DecoTemplateItemViewModel.cs`
- ErsatzTV validators:
  - `CreateDecoGroupHandler`: non-empty, max 50 chars, globally unique group name.
  - `CreateDecoHandler`: max 50 chars, unique deco name within group.
  - `CreateDecoTemplateGroupHandler`: non-empty, max 50 chars, globally unique group name.
  - `CreateDecoTemplateHandler`: max 50 chars, unique deco template name within group.
  - `UpdateDecoHandler`: validates deco exists, group exists, name length/uniqueness, selected watermark/graphics/filler/dead-air references, and break-content rows.
  - `ReplaceDecoTemplateItemsHandler`: validates template/group/name and prevents overlapping template items.
- ErsatzTV handlers/services:
  - `GetAllDecoGroups`, `GetDecoTree`, `CreateDecoGroup`, `CreateDeco`, `DeleteDecoGroup`, `DeleteDeco`.
  - `GetDecoById`, `UpdateDeco`, `GetDecosByDecoGroupId`.
  - `GetAllDecoTemplateGroups`, `GetDecoTemplateTree`, `CreateDecoTemplateGroup`, `CreateDecoTemplate`, `DeleteDecoTemplateGroup`, `DeleteDecoTemplate`.
  - `GetDecoTemplateById`, `GetDecoTemplateItems`, `GetDecoTemplatesByDecoTemplateGroupId`, `ReplaceDecoTemplateItems`.
- ErsatzTV repository/query paths:
  - EF `TvContext.DecoGroups`, `Decos`, `DecoBreakContent`, `DecoWatermarks`, `DecoGraphicsElements`.
  - EF `TvContext.DecoTemplateGroups`, `DecoTemplates`, `DecoTemplateItems`.
  - Tree query handlers project grouped tree view models ordered by group/name.
- Side effects:
  - Add/delete group and item refreshes the tree and select options.
  - Delete group cascades owned decos/templates through EF relationship configuration.
  - Save deco replaces watermarks, graphics elements, break content, filler/dead-air selections.
  - Save deco template replaces all template items and updates template metadata.
- ErsatzRS target modules:
  - `crates/ersatzrs-web/src/admin/server_fns/decos.rs`
  - `crates/ersatzrs-web/src/admin/pages/scheduling/decos.rs`
  - `crates/ersatzrs-web/src/admin/pages/scheduling/templates.module.css` reused for scheduling page geometry and icon buttons.
  - `crates/ersatzrs-web/src/api/decos.rs`
  - `crates/ersatzrs-web/src/host/router.rs`
  - `crates/ersatzrs-web/src/admin/routes.rs`
  - `crates/ersatzrs-web/src/admin/server_fns/mod.rs`
  - `crates/ersatzrs-web/src/admin/pages/scheduling/mod.rs`
  - Existing persistence in `crates/ersatzrs-infra/src/repositories/deco.rs`, scheduling read model, and initial schema.
- Intentional deviations:
  - ErsatzRS keeps browser-safe HTML form `method="post"` fallbacks for critical add/delete/save paths. This is an implementation requirement for SSR/hydration resilience, not an ErsatzTV behavior divergence.

## Visual Source Map

- ErsatzTV layout containers:
  - List pages wrap a `MudForm` around a full-height `div.d-flex.flex-column` with `height: 100vh; overflow-x: auto`.
  - Main content uses `MudContainer MaxWidth.ExtraLarge Class="pt-8"`.
  - The two list-page sections are `MudText Typo.h5`, `MudDivider Class="mb-6"`, then form rows.
- ErsatzTV action bars/buttons:
  - Add group and add item buttons are primary filled MudButtons with leading add icon, placed in the second form column under their associated input.
  - Tree rows place edit/delete icon buttons at the far right; edit is present only for leaf deco/template rows.
  - Delete opens `DeleteDialog` with `MaxWidth.ExtraSmall`.
- ErsatzTV tables/forms/pagers/dialogs:
  - List pages use `MudTreeView` full width, with a two-column row grid (`1fr auto`) and row hover behavior.
  - Deco editor uses stacked form-field rows, mode selects, conditional tables, and selected break-content editing controls.
  - Deco template editor uses a save action row, grouped deco selects, add/remove controls, and a day timeline/calendar surface.
- Width/alignment/spacing rules:
  - ExtraLarge container width; field rows use `form-field-stack gap-md-8 mb-5`.
  - Labels occupy the left form column, inputs/buttons/tree occupy the right column.
  - Section spacing: `mb-2` headings, `mb-6` dividers, `mt-10` between group and item sections.
  - Tree action column is right-aligned and does not resize the name column.
- Reference screenshots and viewport sizes:
  - No live ErsatzTV screenshot captured yet in this slice.
  - ErsatzRS screenshots captured at 1200x900 after `./run.sh serve` rebuild:
    - `plans/references/decos-tree-visual.png`
    - `plans/references/deco-template-timeline-visual.png`
    - `plans/references/deco-editor-break-content-visual.png`
- ErsatzRS shared primitives/CSS modules to reuse:
  - `templates.module.css` geometry and icon-button patterns for grouped scheduling list/editor pages.
  - `templates.module.css` timeline geometry from `/templates/:id` for the deco-template day calendar surface.
  - `DeleteDialog` and toast primitives from `admin::ui::primitives`.
  - Existing `Resource::new_blocking` pattern for SSR-stable editor content.
  - Synchronous `<noscript>` list/editor fallbacks for critical raw POST paths, because Leptos streamed resources are serialized through scripts that JavaScript-disabled browsers cannot apply.
- Known visual deviations:
  - None. Live ErsatzTV screenshots were not available in this slice; the verified visual contract is derived from the Razor/MudBlazor structure above plus same-viewport ErsatzRS screenshots and browser measurements.

## Acceptance

- [x] UI fields/actions/routes/back-links match ErsatzTV.
- [x] Validation and error surfaces match ErsatzTV.
- [x] Persistence and side effects match ErsatzTV.
- [x] Critical save/action paths have SSR or browser-safe fallback.
- [x] UI layout has screenshot/bounding-box evidence against the Razor-derived visual contract.
- [x] Repeated defect classes have reusable tests or playbook coverage.
- [x] Parity docs, status, and active plan files are updated.

## ParityDebt

- None.

## Verification

- [x] `cargo xtask parity-check web`
- [x] Focused server function/repository tests for add/delete/save behavior.
  - `cargo test -p ersatzrs-web decos --all-features` (4 passed)
- [x] Compile/format checks for landed code.
  - `cargo fmt --all -- --check`
  - `cargo check -p ersatzrs-web --all-features`
- [x] Browser smoke for hydrated add/delete/save and SSR fallback paths.
  - Hydrated browser smoke passed for `/decos` and `/deco-templates` add/edit/save against `DATABASE_URL=sqlite:/tmp/ersatzrs-decos-smoke-20260527*.db` with screenshots in `plans/references/`.
  - Hydrated visual smoke passed at 1200x900 against `DATABASE_URL=sqlite:/tmp/ersatzrs-decos-visual-20260527c.db`: `/decos` visible tree rows measured 44px row heights with right-aligned actions and `/deco-templates/:id` rendered a day timeline with a 01:00-01:30 deco item at the expected offset. Screenshots: `plans/references/decos-tree-visual.png`, `plans/references/deco-template-timeline-visual.png`.
  - Hydrated `/decos/:id` editor visual smoke passed at 1200x900 against `DATABASE_URL=sqlite:/tmp/ersatzrs-decos-editor-20260527.db`: action bar measured 64px high, the first form grid measured two columns (`352px 532px`), break-content table headers were Type/Name/Placement/action, break row measured 45px high, inline delete was present, selected row styling applied, and selected break-content Placement/Collection Type controls were disabled like ErsatzTV. Screenshot: `plans/references/deco-editor-break-content-visual.png`.
  - Verified JS-enabled editor pages hide the direct SSR fallback form; only one visible `Add Deco To Template` control remained in the hydrated editor.
  - JavaScript-disabled browser smoke passed at 1200x900 against `DATABASE_URL=sqlite:/tmp/ersatzrs-decos-noscript-20260527f.db`: created deco/template groups, created deco/template records, opened both editor routes, saved renamed records through raw POST forms, redirected to the list routes, and saw updated rows as visible HTML.
  - JavaScript-disabled browser smoke re-passed at 1200x900 after the tree/timeline changes against `DATABASE_URL=sqlite:/tmp/ersatzrs-decos-visual-20260527c.db`.
  - JavaScript-disabled browser smoke re-passed at 1200x900 after `/decos/:id` editor table changes against `DATABASE_URL=sqlite:/tmp/ersatzrs-decos-editor-20260527.db`.
  - Added a raw form parser regression for repeated hidden media-item fields produced by browser form submission.
