# Scheduling Route Family Parity Closeout

## Source Map

- ErsatzTV UI/pages: `$ERSATZTV_SRC/ErsatzTV/Pages/Schedules.razor`,
  `Blocks.razor`, `BlockEditor.razor`, `Templates.razor`,
  `TemplateEditor.razor`, `Decos.razor`, `DecoEditor.razor`,
  `DecoTemplates.razor`, and `DecoTemplateEditor.razor`.
- ErsatzTV components/view models: schedule, block, template, deco, and
  deco-template page view models plus the shared delete/copy dialogs used by
  the Razor pages.
- ErsatzTV validators: schedule name/copy validation, block group/block
  duration and target validation, template overlap and midnight validation,
  deco group/deco break-content/default-filler/dead-air validation, and
  deco-template non-empty/overlap/midnight validation.
- ErsatzTV handlers/services: page handlers dispatch add/copy/delete/save
  commands through Mediator and reload page-local rows after mutations.
- ErsatzTV repository/query paths: schedule list/detail/page-size queries,
  block group/block item queries, template group/template item queries, deco
  group/deco item queries, and deco-template item queries.
- Side effects: schedule and schedule-item saves enqueue dependent playout
  refreshes; list/editor page sizes persist through config keys; delete/copy
  actions update the same route-local row surfaces.
- ErsatzRS target modules:
  `crates/ersatzrs-web/src/admin/pages/scheduling/{schedules,blocks,block_editor,templates,decos}.rs`,
  `crates/ersatzrs-web/src/admin/server_fns/{schedules,blocks,templates,decos}.rs`,
  `crates/ersatzrs-web/src/api/{schedules,blocks,templates,decos}.rs`, and
  `crates/ersatzrs-web/src/admin/languages/`.
- Intentional deviations: `DecoTemplateEditor.razor` and
  `TemplateEditor.razor` use `MudCalendar`; ErsatzRS keeps the existing
  timeline grid implementation while preserving the same add/remove/save
  semantics, 5-minute snapping, midnight validation, and visible sections.

## Visual Source Map

- ErsatzTV layout containers: scheduling pages use full-height `MudForm`,
  `MudContainer MaxWidth.ExtraLarge`, h5 section headings, and dividers.
  Schedules and editor pages also use a 64px `MudPaper` action bar.
- ErsatzTV action bars/buttons: add/save buttons use leading Material icons;
  schedule list has Add Schedule in the top bar; schedule item, block,
  template, deco, and deco-template editors keep save/back controls visible
  above the content.
- ErsatzTV tables/forms/pagers/dialogs: schedules use paged `MudTable`
  master/detail rows; blocks/templates use grouped tables with edit/copy/delete
  actions; decos and deco templates use tree-style grouped rows; editor forms
  use label/control rows and delete/copy confirmation dialogs.
- Width/alignment/spacing rules: ExtraLarge-width content, 64px action bars,
  row-stacked add forms on narrow viewports, dense grouped tables, and no
  overlapping action controls.
- Reference screenshots and viewport sizes: existing captures in
  `plans/references/` cover ErsatzTV schedules/blocks/templates/decos/deco
  templates and ErsatzRS smoke captures; the durable browser proof now lives in
  `tests/e2e/scheduling-route-family.spec.ts` and emits route screenshots under
  `test-results/`.
- ErsatzRS shared primitives/CSS modules to reuse:
  `schedules.module.css`, `blocks.module.css`, `block_editor.module.css`,
  `templates.module.css`, and the shared dialog/toast primitives.
- Known visual deviations: MudCalendar is represented by a native timeline
  grid for template/deco-template content; no open layout debt remains for this
  route-family closeout.

## Acceptance

- [x] UI fields/actions/routes/back-links match the pinned ErsatzTV route
      family or the timeline-grid deviation above.
- [x] Validation and error surfaces are carried by the same server functions
      and localized error mapping used by the admin pages.
- [x] Persistence and side effects cover add/copy/delete/save, schedule
      page-size persistence, and dependent playout refresh for schedule
      changes.
- [x] Critical save/action paths have SSR or browser-safe HTML POST fallbacks.
- [x] Browser proof covers hydrated schedule, block, template, deco, and
      deco-template route-family workflows with visible geometry screenshots
      and console-error checks.
- [x] Parity docs, route/hydration status, changelog, and the top-gap prompt
      are updated by this closeout.

## ParityDebt

- None.

## Verification

- `E2E_BASE_URL=http://127.0.0.1:8409 npx playwright test tests/e2e/scheduling-route-family.spec.ts --project=chromium`
- `cargo xtask hydration-lint -p ersatzrs-web`
- `cargo test -p ersatzrs-web --all-targets scheduling`
- `cargo xtask parity-check web`
- `cargo xtask progress check`
