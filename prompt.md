Fix the next ErsatzTV parity gap in ErsatzRS: page `/decos` and related page `/deco-templates`.

Use the repo workflow from AGENTS.md plus `skills/parity-slice.md`.
The goal is fully functional ErsatzTV parity without stopgaps.

Before editing:
1. Read the relevant ErsatzTV source under `$ERSATZTV_SRC/`.
2. Inspect the current ErsatzRS implementation, routes, docs, and tests.
3. Build a Source Map covering UI/pages, components/view models, validators,
   handlers/services, repository/query paths, side effects, ErsatzRS target
   modules, and intentional deviations.
4. For admin UI routes, build a Visual Source Map too:
   - ErsatzTV Razor layout containers (`MudPaper`, `MudContainer`, `MudTable`,
     `MudStack`, `MudForm`, toolbars, pagers, dialogs).
   - Button/action placement, content max-width, grid/form column widths,
     table column widths, row heights, dividers, typography, and overflow rules.
   - Attached reference screenshots or live ErsatzTV screenshots, with viewport
     size recorded.
   - Existing ErsatzRS shared primitives/CSS modules that should be reused.
5. Identify any `ParityDebt`; incomplete behavior or visual mismatch must be
   visible and cannot be hidden in prose.

Implement the coherent end-to-end slice that genuinely closes the gap:
- Match ErsatzTV UI fields, actions, labels, validation, routes, persistence,
  side effects, and visible layout behavior.
- Preserve SSR or browser-safe fallbacks for critical save/action flows.
- Follow TEST.md hydration rules for interactive admin UI.
- For UI work, do not hand-wave "ErsatzTV aesthetics": map the actual
  MudBlazor structure into ErsatzRS CSS and verify the resulting geometry.
- Add focused tests or reusable regressions for the behavior being closed.
- Update parity/status docs when the route or feature status changes.

Visual verification for UI routes:
- Start the app with `./run.sh serve` after rebuilding Leptos assets.
- Open the touched route in a fresh browser context or otherwise bypass stale
  `/public/bundle.css` cache.
- Capture same-viewport screenshots for ErsatzRS and the ErsatzTV reference
  when available; at minimum use the viewport from the user's attached
  screenshot.
- Compare visible geometry, not just DOM presence: action bars not clipped,
  content width/alignment matches the upstream `MudContainer`, labels/inputs
  line up, tables and pagers have the same relative placement, text is not
  cropped, and page-level overflow does not hide controls.
- Inspect bounding boxes or computed styles when a screenshot shows a mismatch.
  CSS-module changes must be verified against the stylesheet actually served by
  the running app.
- Do not claim UI parity complete while a visible screenshot mismatch remains;
  record it as `ParityDebt` with evidence.

Verification:
- Run the narrow useful gates, normally `cargo xtask parity-check web` for admin
  UI work.
- Add focused crate tests when non-web behavior changes.
- Browser-smoke any hydrated or fallback UI behavior.
- For UI routes, include the visual verification evidence above.
- Do not claim parity complete until the no-stopgap checklist is proven.

When done, report the Source Map, Visual Source Map for UI routes, behavior
implemented, any remaining `ParityDebt`, verification run, and changed files.
