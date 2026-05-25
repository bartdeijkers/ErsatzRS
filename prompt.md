Fix the next ErsatzTV parity gap in ErsatzRS: <FEATURE/ROUTE/GAP>.

  Use the repo workflow from AGENTS.md plus `.codex/skills/parity-slice.md`. The goal is fully functional ErsatzTV parity without stopgaps.

  Before editing:
  1. Read the relevant ErsatzTV source under `$ERSATZTV_SRC/`.
  2. Inspect the current ErsatzRS implementation, routes, docs, and tests.
  3. Build a Source Map covering UI/pages, components/view models, validators, handlers/services, repository/query paths, side effects, ErsatzRS target modules, and intentional deviations.
  4. Identify any `ParityDebt`; incomplete behavior must be visible and cannot be hidden in prose.

  Implement the coherent end-to-end slice that genuinely closes the gap:
  - Match ErsatzTV UI fields, actions, labels, validation, routes, persistence, and side effects.
  - Preserve SSR or browser-safe fallbacks for critical save/action flows.
  - Follow TEST.md hydration rules for interactive admin UI.
  - Add focused tests or reusable regressions for the behavior being closed.
  - Update parity/status docs when the route or feature status changes.

  Verification:
  - Run the narrow useful gates, normally `cargo xtask parity-check web` for admin UI work.
  - Add focused crate tests when non-web behavior changes.
  - Browser-smoke any hydrated or fallback UI behavior.
  - Do not claim parity complete until the no-stopgap checklist is proven.
  - ui/pages match the ErsatzTV aesthetics 

  When done, report the Source Map, behavior implemented, any remaining `ParityDebt`, verification run, and changed files.