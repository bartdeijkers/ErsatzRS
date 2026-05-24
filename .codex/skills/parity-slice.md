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
5. Do not hide incomplete behavior in prose. Add a visible `ParityDebt`
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
- Validation behavior and error surfaces match.
- Persistence plus delete/copy/edit/add side effects match, including playout
  refresh, generated playlists, media actions, and health/troubleshooting
  impact when relevant.
- SSR fallback works for critical save/action flows, or hydration is explicitly
  required and browser-tested.
- Repeated historical defect class has a reusable regression test or playbook:
  card action menus, hydrated islands attaching, SSR links/fallback modals, or
  post-add/remove/edit side effects.
- `docs/reference/parity-matrix.md`, `STATUS.md`, `TEST.md`, and any active
  `/plans` file are updated when their status changed.

## Fast Gate Ladder

Use the narrow ladder while iterating:

```bash
cargo xtask parity-check web
```

Add scopes as useful (`media`, `scanner`, `infra`, etc.) for the final command
log even when they only document intent. Use full workspace gates only at the
end or when the slice changes shared behavior.

## Final Report

Report:

```text
Source map
Parity behavior implemented
ParityDebt
Verification run
Full gates left, if any
Changed files
```
