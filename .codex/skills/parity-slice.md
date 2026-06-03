---
name: parity-slice
description: Drive an ErsatzTV parity implementation slice from source audit through no-stopgap acceptance. Use for admin/media pages, provider/scanner ports, health/troubleshooting changes, or any request that says parity, no stopgaps, or fully functional parity.
---

# Parity Slice

This is the default workflow for implementation work that claims ErsatzTV
parity. Do not use it for discussion-only source mapping; use
`port-ersatztv` for a plan-only answer.

## Required Inputs

- Slice name, route, feature, or plan file.
- ErsatzTV source area under `$ERSATZTV_SRC/`.
- ErsatzRS target crate/module, if already known.

## Source Map

Create or update a short source map before editing code. Include all layers
that define behavior:

```md
## Source Map

- ErsatzTV UI/pages:
- ErsatzTV components/view models:
- ErsatzTV validators:
- ErsatzTV handlers/services:
- ErsatzTV repository/query paths:
- Side effects:
- ErsatzRS target modules:
- Intentional deviations: None, or ADR/deviation link.
```

For admin UI routes, add a visual source map before editing. This is required
whenever the page has an ErsatzTV Razor/MudBlazor reference or the user supplies
screenshots.

```md
## Visual Source Map

- ErsatzTV layout containers:
- ErsatzTV action bars/buttons:
- ErsatzTV tables/forms/pagers/dialogs:
- Width/alignment/spacing rules:
- Reference screenshots and viewport sizes:
- ErsatzRS shared primitives/CSS modules to reuse:
- Known visual deviations:
```

Read the Razor markup for actual MudBlazor structure (`MudPaper`,
`MudContainer`, `MudTable`, `MudStack`, `MudForm`, `MudDivider`,
`MudTablePager`, dialogs, and toolbars). Do not reduce this to "match the
aesthetics"; capture the geometry that can regress: container max-width,
button placement, form label/input columns, row heights, table column widths,
pagers, dividers, text clipping, and overflow behavior.

For large ports, add a concise top-level Rust comment or module doc mapping the
main sections of the Rust file back to the ErsatzTV source files. Do not paste
large source excerpts.

## Slice Scaffold

Start new parity plans or status notes with this scaffold:

```md
# <Feature> Parity Slice

## Source Map

- ErsatzTV UI/pages:
- ErsatzTV components/view models:
- ErsatzTV validators:
- ErsatzTV handlers/services:
- ErsatzTV repository/query paths:
- Side effects:
- ErsatzRS target modules:
- Intentional deviations:

## Acceptance

- [ ] UI fields/actions/routes/back-links match ErsatzTV.
- [ ] Validation and error surfaces match ErsatzTV.
- [ ] Persistence and side effects match ErsatzTV.
- [ ] Critical save/action paths have SSR or browser-safe fallback.
- [ ] UI layout has screenshot/bounding-box evidence against ErsatzTV or an
      attached reference screenshot when this is an admin UI route.
- [ ] Repeated defect classes have reusable tests or playbook coverage.
- [ ] Parity docs, status, and active plan files are updated.

## ParityDebt

- None.

## Verification

- [ ] `cargo xtask parity-check web`
- [ ] Focused repository/app/scanner tests, when non-web behavior changed.
- [ ] Browser smoke for hydrated or fallback UI.
```

For large admin pages, split commits around natural boundaries: data/server
functions, list page, editor/detail page, actions and side effects, then
hydration/browser verification.

## Implementation Rules

1. Read the plan file first when one exists, then `PORTING.md`,
   `STYLE.md`, `ARCHITECTURE.md`, and the relevant ErsatzTV source.
2. Preserve behavior across UI fields, validation, route shape, persistence,
   delete/copy/edit side effects, generated playlists, playout refreshes,
   media actions, and health/troubleshooting impact.
3. Prefer existing ErsatzRS primitives and repository patterns. Add shared
   abstractions only when they remove repeated parity defects.
4. For hydrated admin UI, follow `TEST.md`: stable DOM shape, no `<Show>`
   inside islands, no prop-bearing islands across `Suspense`, and SSR or
   browser-safe fallback paths for critical saves/actions.
5. For admin UI layout, prefer existing ErsatzRS primitives and CSS-module
   patterns, but preserve ErsatzTV's visible geometry unless a documented
   project-level design decision says otherwise.
6. Do not hide incomplete behavior in prose. Add a visible `ParityDebt`
   section to the plan/status note when anything remains.

## ParityDebt

Use this shape for any behavior that cannot land in the slice:

```md
## ParityDebt

- Behavior:
  - Status: blocked by missing schema/data | intentionally divergent | next-slice required
  - Evidence:
  - Owner/follow-up:
  - ADR/deviation link:
```

Parity cannot be claimed while `ParityDebt` contains a `next-slice required`
item for the page or feature under review.

## No-Stopgap Acceptance

Before marking parity complete, prove each item with code, docs, or command
evidence:

- ErsatzTV source map is present and covers UI, validators, handlers,
  repository/query paths, and side effects.
- Visible UI fields, actions, labels, and route/back-link behavior match the
  ErsatzTV source or an ADR records the deviation.
- Admin UI visual geometry is verified in a browser against an ErsatzTV or
  attached reference screenshot at the same viewport. The check must cover
  action bars/buttons, content width/alignment, form/table/pager placement,
  clipped text, and overflow. If CSS modules changed, verify the running app is
  serving the rebuilt `/public/bundle.css` or use a fresh browser context.
- Validation behavior and error surfaces match.
- Persistence plus delete/copy/edit/add side effects match, including playout
  refresh, generated playlists, media actions, and health/troubleshooting
  impact when relevant.
- SSR fallback works for critical save/action flows, or hydration is explicitly
  required and browser-tested.
- Repeated historical defect class has a reusable regression test or playbook:
  card action menus, hydrated islands attaching, SSR links/fallback modals, or
  post-add/remove/edit side effects.
- The feature's `docs/progress/parity/<slug>.toml` (plus `routes.toml` /
  `hydration.toml` / `phase.toml` as relevant), `TEST.md`, and any active
  `/plans` file are updated when their status changed, and
  `cargo xtask progress check` passes.

## Fast Gate Ladder

Use the narrow ladder while iterating:

```bash
cargo xtask parity-check web
```

Add scopes as useful (`media`, `scanner`, `infra`, etc.) for the final command
log even when they only document intent. Use full workspace gates only at the
end or when the slice changes shared behavior.

## Visual Browser Loop

For admin UI routes, run this before claiming parity:

1. Start the app with `./run.sh serve` so Leptos assets and `/public/bundle.css`
   are rebuilt and served.
2. Open the route in a fresh browser context, or otherwise bypass stale
   stylesheet cache.
3. Capture ErsatzRS screenshots at the same viewport as the ErsatzTV reference
   or user attachment. If no reference screenshot exists, create one from the
   live ErsatzTV page when practical; otherwise document the Razor-derived
   visual contract used.
4. Compare visible geometry. Look for clipped controls, incorrect content
   width, misplaced action buttons, stretched tables/forms, text overflow,
   wrong dividers, and pager/action alignment.
5. Use browser snapshots, bounding boxes, or computed styles to prove or debug
   mismatches. CSS changes are not proven until the running page reflects them.
6. Add remaining mismatch as `ParityDebt`; do not report UI parity complete.

## Final Report

Report:

```text
Source map
Visual source map, for UI routes
Parity behavior implemented
ParityDebt
Verification run
Full gates left, if any
Changed files
```
