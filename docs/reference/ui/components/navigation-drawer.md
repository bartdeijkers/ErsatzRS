# Navigation drawer

**Status:** Implemented
**Adoption:** Global shell

## Sources

- [`layout/mod.rs`](../../../../crates/ersatzrs-web/src/admin/layout/mod.rs)
- [`layout.module.css`](../../../../crates/ersatzrs-web/src/admin/layout/layout.module.css)
- [`nav.rs`](../../../../crates/ersatzrs-web/src/admin/layout/nav.rs)

## Purpose and responsive contract

The drawer contains the primary admin navigation. It is persistent beside the
main region at desktop widths and becomes an off-canvas disclosure below 768
CSS pixels. The top-bar control is a native button with `aria-controls` and an
`aria-expanded` state. The drawer retains its navigation landmark and localized
groups at both widths.

The closed mobile drawer is `inert` and `aria-hidden`, so translated-off-canvas
links cannot receive keyboard or assistive-technology focus. Opening it focuses
the first available navigation control. Selecting a link or pressing Escape
closes it; Escape and an explicit close restore focus to the disclosure button.
Viewport changes synchronize the desktop and mobile defaults. Session storage
retains user state when available but is not required for the interaction.

## Implementation boundary

The shell and navigation are server-rendered Rust/Leptos markup. A small
page-owned vanilla script provides disclosure state, focus management, inert
state, Escape handling, and session persistence before or independently of
island hydration. CSS owns placement, off-canvas movement, focus visibility,
and the reduced-motion variant. This is not a reusable modal dialog and does
not trap focus.

## Evidence and limitations

- `tests/e2e/addon-metadata-editor.spec.ts` proves keyboard ordering while
  closed, Enter/Space activation, focus inside the open drawer, Escape close,
  focus return, 320/390-pixel reflow, and 200% zoom in Chromium and Firefox.
- The disclosure target is at least 44 by 44 CSS pixels and has a visible focus
  indicator.
- `prefers-reduced-motion: reduce` removes the drawer transition.
- The browser proof covers the shell through the add-on-list workflow. A
  dedicated forced-colors assertion remains future evidence; no forced-colors
  conformance claim is made here.
