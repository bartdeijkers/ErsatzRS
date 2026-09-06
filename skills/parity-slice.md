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

## Slice agreement

Use the existing milestone's acceptance and evidence, following the field
convention in `skills/update-parity.md`. Keep implementation sequence in the
linked live plan and source maps there when needed; reuse existing maps and
update them when the mapped behavior changes. Do not copy acceptance or
detailed current evidence into another status report.

For large admin pages, split commits around natural boundaries: data/server
functions, list page, editor/detail page, actions and side effects, then
hydration/browser verification. Apply the delivery claims in `CONTRIBUTING.md`;
a checkpoint does not declare full feature acceptance.

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
6. Record incomplete behavior in the owning structured parity debt, following
   `skills/update-parity.md`; link to that record from the plan. Parity cannot
   be claimed while required behavior for the page or feature remains open.

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
- Structured progress reflects the actual state and
  `cargo xtask progress check` passes. Follow `skills/update-parity.md` for
  canonical milestone evidence and debt. Update `TEST.md` or a plan only when
  its own testing guidance or intended work changes.

## Verification

Use focused checks during implementation, guided by `skills/rust-inner-loop.md`.
Run the required web parity gate before claiming admin UI parity complete, and
repeat it when relevant changes invalidate its evidence, as described in
`PORTING.md` under Parity verification. Scope labels do not narrow the gate.
Preserve all agreed acceptance requirements.

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
6. Record remaining mismatches in the owning parity debt; do not report UI
   parity complete.

## Checkpoint report

Use the Session Handoff format in `AGENTS.md`. Link to source maps, canonical
milestone evidence and owning parity debt instead of repeating them.
