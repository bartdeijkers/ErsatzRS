# Help disclosure

**Status:** Implemented
**Adoption:** Teletext upload tips, add-on permissions and graphics preview context

## Sources

- `crates/ersatzrs-web/src/admin/ui/primitives/help_disclosure.rs`
- `crates/ersatzrs-web/src/admin/ui/primitives/help_disclosure.module.css`
- `tests/e2e/help-disclosure.spec.ts`

## Contract

`HelpDisclosure` groups an optional tip or supporting explanation in native
`details` / `summary` markup. The whole summary is clickable and keyboard
operable. A decorative arrow points right when closed and down when open;
localized Show explanation / Hide explanation labels make the action explicit.
The browser exposes the expanded state through native semantics. No script,
extra tab stop or custom ARIA state is needed.

The summary has a visible focus outline and a minimum height of 44 CSS pixels.
The expanded body has 0.75rem padding on every side, including above its text.
Long titles and action labels wrap, including at 320 CSS pixels. Styling uses
shared semantic theme tokens. Keep information needed to complete the main
task visible outside the disclosure. Task sections and action menus retain
their own components.

## Rendering boundaries

- `/teletext-elements/add`: `TeletextElementAddPage` renders the tip inside
  the native upload form. No island or endpoint changes.
- `/settings/addons/:id`: the existing add-on detail renderer uses
  `permissions_view` for installed and available package permissions. Its
  existing island retains data ownership; this primitive adds no handlers.
- `/graphics-elements/:id`: `GraphicsElementEditorPage` renders the native
  preview result through `saved_preview_html`; `GraphicsElementEditorIsland`
  renders `GraphicsContextPreview`. Both paths use the same primitive and
  preserve their existing preview data and server function.

The focused browser check verifies the upload tip with pointer and keyboard,
all supported cultures, both themes, narrow and wide viewports, and JavaScript
enabled and disabled. Existing add-on and graphics specs cover their owning
flows separately. This does not establish general WCAG conformance.
