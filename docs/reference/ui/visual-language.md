# Admin UI visual language

This page defines the design target for new and changed ErsatzRS admin UI.
Existing routes may predate the contract and must not be assumed to conform.

## Character

The admin UI uses a media-centric, Tunarr-inspired character without copying
Tunarr's implementation or brand:

- dark, layered surfaces as the primary visual expression;
- restrained chrome that keeps media, schedules, and operator data prominent;
- compact but scannable information density;
- clear separation between page, section, card, overlay, and selected states;
- one dominant action per decision region, with destructive actions visually
  distinct from constructive actions;
- purposeful imagery and icons, never decoration that competes with status or
  task information.

The supported light theme preserves the same semantic roles and hierarchy. It
is not a simple color inversion.

Tunarr is an aesthetic reference only. ErsatzRS retains its own information
architecture, accessibility contract, localization, Rust/Leptos runtime, and
CSS tokens. No TypeScript, React, or MUI implementation is part of this design
system.

## Color and surfaces

Semantic CSS custom properties in
[`globals.module.css`](../../../crates/ersatzrs-web/src/admin/style/globals.module.css)
are the implemented token source.

- Use semantic roles such as app background, surface, border, primary action,
  text, muted text, warning, and error instead of page-local literal colors.
- Elevation comes first from surface contrast and borders; shadows are reserved
  for overlays and elements that actually float above the page.
- Color is never the only indicator of selection, validity, status, or risk.
- Text contrast targets at least 4.5:1 for normal text and 3:1 for large text.
  Essential non-text controls and focus indicators target at least 3:1 against
  adjacent colors.

## Type, spacing, and density

- Use the global type stack and a consistent heading hierarchy. Page titles,
  section titles, labels, body copy, metadata, and helper text have visibly
  different roles.
- Prefer a small set of repeated spacing steps. Alignment between neighboring
  regions is more important than adding decorative whitespace.
- Dense tables and schedules may be compact, but labels, status, and row actions
  remain distinguishable at 200% zoom.
- Long localized labels may wrap. Fixed-width controls must not clip meaning or
  push essential actions outside the viewport.

## Semantic interaction

- Use links for navigation and buttons for actions. Icon-only controls require
  a programmatic accessible name.
- Every form control has a persistent label. Placeholder text is supplementary,
  not the label.
- Keyboard order follows the visual and task order. Focus is visible and is not
  hidden by sticky headers, overlays, or transient messages.
- Loading, empty, error, disabled, success, destructive, and recovery states are
  visually and programmatically distinct.
- Animation communicates spatial or state change, remains restrained, and
  respects `prefers-reduced-motion`.

## Responsive behavior

- Layout responds to available content space rather than named device models.
- At 320 CSS pixels, content reflows without horizontal page scrolling or loss
  of information. A bounded table, guide, or timeline may scroll on one axis
  when reformatting would destroy its relationships.
- Primary actions remain visible or reachable in the same task context on
  narrow and wide layouts. Secondary actions may move into a clearly labelled
  disclosure or menu.
- Pointer targets meet the WCAG 2.2 AA minimum of 24 by 24 CSS pixels or the
  spacing exception. Prefer 44 by 44 CSS pixels for primary touch actions.
- Responsive review covers narrow, intermediate, and wide layouts, long
  translations, keyboard focus, touch targets, 200% zoom, and both themes.

## Implementation boundary

Design artifacts use semantic HTML and CSS. Production interactivity maps to
Rust and the narrowest suitable Leptos hydration island, while retaining useful
SSR and no-JavaScript behavior. The [component catalog](components/README.md)
owns reusable component contracts; page CSS owns only genuinely page-specific
composition.

## Standards and visual reference

- [WCAG 2.2](https://www.w3.org/TR/WCAG22/)
- [WAI-ARIA Authoring Practices](https://www.w3.org/WAI/ARIA/apg/)
- [Tunarr source project](https://github.com/chrisbenincasa/tunarr)
