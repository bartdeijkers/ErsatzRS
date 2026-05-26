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
  - Editor reference still needs live capture or a route-specific screenshot after seeded data exists.
- ErsatzRS shared primitives/CSS modules to reuse:
  - Scheduling route structure from `blocks.rs`, `block_editor.rs`, and `schedules.rs`.
  - `DeleteDialog` and `toast` primitives.
  - Existing block editor option loading for block groups/blocks where possible, with route-specific template server functions.
- Known visual deviations:
  - None intended. If the Rust day timeline cannot match `MudCalendar` geometry after browser verification, record it below as `ParityDebt`.

## Acceptance

- [ ] UI fields/actions/routes/back-links match ErsatzTV.
- [ ] Validation and error surfaces match ErsatzTV.
- [ ] Persistence and side effects match ErsatzTV.
- [ ] Critical save/action paths have SSR or browser-safe fallback.
- [ ] UI layout has screenshot/bounding-box evidence against ErsatzTV or an attached reference screenshot when this is an admin UI route.
- [ ] Repeated defect classes have reusable tests or playbook coverage.
- [ ] Parity docs, status, and active plan files are updated.

## ParityDebt

- None.

## Verification

- [ ] `cargo xtask parity-check web`
- [ ] Focused repository/app/scanner tests, when non-web behavior changed.
- [ ] Browser smoke for hydrated or fallback UI.
