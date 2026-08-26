# Button and action link

**Status:** Implemented
**Adoption:** Limited

## Sources

- [`button.rs`](../../../../crates/ersatzrs-web/src/admin/ui/primitives/button.rs)
- [`button.module.css`](../../../../crates/ersatzrs-web/src/admin/ui/primitives/button.module.css)
- [`dev.rs`](../../../../crates/ersatzrs-web/src/admin/pages/dev.rs) contains the
  component showcase.

## Surfaces

| Surface | Element | Purpose | Variants |
|---|---|---|---|
| `Button` | `<button type="button">` | Invoke an action | Primary, Secondary, Text, Danger |
| `LinkButton` | `<a href>` | Navigate while using action styling | Primary, Secondary, Text, Danger |
| `IconButton` | `<button type="button">` | Invoke a compact icon-only action | Primary, Secondary, Text, Danger |

`Button` accepts a reactive disabled state and an optional callback.
`LinkButton` requires a destination. `IconButton` accepts an icon, optional
`title`, disabled state, and callback.

## Semantic contract

- Navigation remains an anchor; an action remains a button.
- Primary is the dominant constructive action in one decision region.
- Secondary and Text reduce emphasis without changing availability.
- Danger identifies a destructive or high-consequence action; it does not by
  itself supply confirmation or recovery.
- Disabled controls remain visible and expose the native disabled state.

## Current evidence and limitations

- Production use of the shared `Button` family is currently concentrated in
  `Modal`; most admin routes still render local raw buttons.
- Focus-visible styling exists for every shared variant.
- `IconButton` renders a 2rem square target. This exceeds the WCAG 2.2 AA
  24-pixel minimum but is below the preferred 44-pixel touch target.
- `IconButton` passes its optional text only through `title`; the component does
  not currently render an explicit `aria-label`. An accessible-name audit is
  required before claiming conformance.
- The developer showcase is not browser or WCAG proof.
