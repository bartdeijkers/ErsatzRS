# Templates Parity Slice

## Source Map

- ErsatzTV UI/pages:
  - `$ERSATZTV_SRC/ErsatzTV/Pages/Templates.razor`
  - `$ERSATZTV_SRC/ErsatzTV/Pages/TemplateEditor.razor`
  - `$ERSATZTV_SRC/ErsatzTV/Shared/CopyTemplateDialog.razor`
- ErsatzTV components/view models:
  - `$ERSATZTV_SRC/ErsatzTV.Application/Scheduling/TemplateGroupViewModel.cs`
  - `$ERSATZTV_SRC/ErsatzTV.Application/Scheduling/TemplateViewModel.cs`
  - `$ERSATZTV_SRC/ErsatzTV.Application/Scheduling/TemplateItemViewModel.cs`
  - `$ERSATZTV_SRC/ErsatzTV/ViewModels/TemplateItemsEditViewModel.cs`
  - `$ERSATZTV_SRC/ErsatzTV/ViewModels/TemplateItemEditViewModel.cs`
  - `$ERSATZTV_SRC/ErsatzTV.Application/Scheduling/Mapper.cs`
- ErsatzTV validators:
  - `CreateTemplateGroupHandler`: name required, max 50 chars, unique group name.
  - `CreateTemplateHandler`: template name max 50 chars and unique within group.
  - `CopyTemplateHandler`: new name required, max 50 chars, unique within target group.
  - `ReplaceTemplateItemsHandler`: template exists, name max 50 chars, unique within group, item blocks must not overlap.
- ErsatzTV handlers/services:
  - `GetAllTemplateGroupsHandler`, `GetAllTemplatesHandler`, `GetTemplateByIdHandler`, `GetTemplateItemsHandler`
  - `CreateTemplateGroupHandler`, `CreateTemplateHandler`, `CopyTemplateHandler`, `DeleteTemplateGroupHandler`, `DeleteTemplateHandler`, `ReplaceTemplateItemsHandler`
- ErsatzTV repository/query paths:
  - EF `TvContext.TemplateGroups`, `Templates`, `TemplateItems`, `Blocks`, `BlockGroups`
  - `GetAllTemplatesHandler` adds dummy `(none)` rows for groups with no templates so group delete remains visible.
  - `GetTemplateItemsHandler` joins `Block` and drops invalid items where start/end ranges cross incorrectly.
- Side effects:
  - Add/copy/delete template group and template mutate scheduling tables.
  - Save replaces all `TemplateItem` rows and updates `Template.DateUpdated`.
  - ErsatzTV contains a TODO to refresh playouts using the changed template; no refresh side effect exists upstream.
- ErsatzRS target modules:
  - `crates/ersatzrs-web/src/admin/pages/scheduling/templates.rs`
  - `crates/ersatzrs-web/src/admin/pages/scheduling/templates.module.css`
  - `crates/ersatzrs-web/src/admin/server_fns/templates.rs`
  - `crates/ersatzrs-web/src/api/templates.rs`
  - `crates/ersatzrs-web/src/admin/routes.rs`
  - `crates/ersatzrs-web/src/host/router.rs`
  - `crates/ersatzrs-web/src/api/mod.rs`
  - `crates/ersatzrs-web/src/admin/server_fns/mod.rs`
  - `crates/ersatzrs-web/src/admin/pages/scheduling/mod.rs`
- Intentional deviations: None.

## Visual Source Map

- ErsatzTV layout containers:
  - `/templates`: `MudForm` wrapping a full-height scrolling `div`; `MudContainer MaxWidth.ExtraLarge Class="pt-8"`.
  - `/templates/{id}`: top `MudPaper Square=true` action bar, 64px high, full width, save button with left margin; then full-height scrolling `div` and `MudContainer MaxWidth.ExtraLarge Class="pt-8"`.
- ErsatzTV action bars/buttons:
  - List page has inline filled primary buttons: `Add Template Group`, `Add Template`.
  - Table row actions are icon buttons: edit, copy, delete; group header has delete group icon aligned after two 48px spacers.
  - Editor action bar has `Save Template`; editor content add/remove buttons are filled primary buttons.
  - Copy dialog uses `Cancel` and filled primary `Copy Template`.
- ErsatzTV tables/forms/pagers/dialogs:
  - Form rows are `MudStack Row=true Breakpoint=SmAndDown Class="form-field-stack gap-md-8 mb-5"` with a label column and input/select column.
  - List table is `MudTable Hover=true Dense=true Class="mt-8"`, grouped by template group, with search toolbar placeholder `Search for templates`.
  - Columns are effectively name plus 180px actions on non-xs breakpoints; group header background is appbar background and row background is background-gray.
  - Editor content uses sections `General`, `Add Content`, `Remove Content`, `Content`, each with `Typo.h5` heading and `MudDivider`.
  - The upstream content calendar is a Heron `MudCalendar` day view with 10-minute intervals, draggable items, no month/week/date/today controls.
- Width/alignment/spacing rules:
  - Max content width follows MudBlazor `MaxWidth.ExtraLarge`; existing ErsatzRS scheduling pages use `max-width: 1920px` to mirror this.
  - Section headings are h5 scale, normal weight, with divider margin below.
  - Form label/input grid should preserve the label column, 2rem-ish input height, underline inputs/selects, and responsive one-column collapse.
  - The editor action bar must not clip buttons and must remain full width above content.
- Reference screenshots and viewport sizes:
  - `plans/references/ersatztv_screenshots/0052_scheduling_templates.png`, 3840x1918.
  - `plans/references/templates_ersatzrs_list_2026-05-26.png`, 3840x1918, captured from `./run.sh serve` with a seeded template group/template.
  - `plans/references/templates_ersatzrs_editor_2026-05-26.png`, 3840x1918, captured from `./run.sh serve` after editing a seeded template item.
  - `plans/references/templates_console_2026-05-26.txt`, final browser console capture with 0 warnings/errors.
- ErsatzRS shared primitives/CSS modules to reuse:
  - Scheduling route structure from `blocks.rs`, `block_editor.rs`, and `schedules.rs`.
  - `DeleteDialog` and `toast` primitives.
  - Existing block editor option loading for block groups/blocks where possible, with route-specific template server functions.
- Known visual deviations:
  - None intended. The Rust day timeline maps the upstream `MudCalendar` day-view geometry and supports drag repositioning, overlap rejection, and midnight rejection without the disabled month/week/date/today controls.

## Acceptance

- [x] UI fields/actions/routes/back-links match ErsatzTV.
- [x] Validation and error surfaces match ErsatzTV.
- [x] Persistence and side effects match ErsatzTV.
- [x] Critical save/action paths have SSR or browser-safe fallback.
- [x] UI layout has screenshot/bounding-box evidence against ErsatzTV or an attached reference screenshot when this is an admin UI route.
- [x] Repeated defect classes have reusable tests or playbook coverage.
- [x] Parity docs, status, and active plan files are updated.

## ParityDebt

- None.

## Verification

- [x] `cargo xtask parity-check web`
- [x] Focused repository/app/scanner tests, when non-web behavior changed: `cargo test -p ersatzrs-web templates -- --nocapture`.
- [x] Browser smoke for hydrated or fallback UI: `./run.sh serve`; fresh 3840x1918 browser context; list screenshot, editor screenshot, delete template/group confirmation dialogs, copy/edit navigation, block select/add, timeline drag, save POST fallback redirect to `/templates`, and final console warnings/errors = 0.
