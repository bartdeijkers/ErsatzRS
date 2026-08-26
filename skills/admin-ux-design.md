---
name: admin-ux-design
description: Convert ErsatzRS admin UX/UI ideas into implementation-ready, responsive, WCAG 2.2 AA designs using semantic HTML and CSS. Use for new or redesigned admin flows, layouts, interaction models, visual hierarchy, responsive behavior, or static HTML/CSS prototypes; do not use for a read-only usability or deceptive-pattern audit.
---

# Admin UX Design

Turn a product intent into a concrete interaction and presentation contract for
the ErsatzRS admin UI. The normal output is a design specification or static
prototype, not production code. Implement the design only when the user asks
for implementation.

## Required context

Read these before designing:

- `docs/reference/ui/README.md` and `docs/reference/ui/visual-language.md`.
- `docs/reference/ui/components/README.md` and the relevant component entries.
- The current route, nearby working routes, and their CSS modules.
- The ErsatzTV source and parity record when an existing feature is being
  redesigned; preserve behavior unless an ADR records a deviation.

Before production implementation, also read `TEST.md`, `ARCHITECTURE.md`, and
the `hydration-islands` skill. Read `BUILD.md` when the SSR/WASM bundle or
release assets are in scope.

## Design constraints

- Start from native HTML semantics: landmarks, headings, links, buttons,
  labels, fieldsets, tables, details, and dialogs where their semantics fit.
- Express visual design in plain CSS or a repository CSS module. Do not produce
  TypeScript, React, MUI, or another client-side design-system dependency.
- A static prototype may use multiple HTML documents or explicit state blocks
  to show interactions. Do not use checkbox hacks or hidden JavaScript to fake
  behavior.
- Map necessary client state to Rust and a narrow Leptos hydration island only
  when implementation is requested. Preserve a useful SSR/no-JavaScript path.
- Route every user-facing production string through the ErsatzRS localization
  boundary.
- Treat Tunarr as visual inspiration: media-centric composition, restrained
  chrome, clear hierarchy, and layered surfaces. Do not copy its framework,
  source implementation, brand assets, or exact screen layouts.
- Target WCAG 2.2 AA. Design keyboard order and visible focus, programmatic
  names and relationships, status/error announcements, contrast, zoom, and
  target size as part of the interaction rather than as later polish.
- Define desktop and mobile behavior. Content must reflow at 320 CSS pixels
  without two-dimensional page scrolling; a bounded data table, timeline, or
  guide may scroll on one axis when its meaning cannot be preserved otherwise.

## Workflow

1. State the primary user, job, success condition, frequency, risk, and parity
   constraints. Separate confirmed facts from assumptions.
2. Trace the shortest reasonable task path. Remove steps that do not protect
   user intent, data, security, or a required system constraint.
3. Inventory the component catalog and source before drawing new controls.
   Reuse an implemented component when its semantic and interaction contract
   matches. Record a variant instead of duplicating a near-identical pattern.
4. Define information hierarchy and interaction states: initial, loading,
   ready, empty, validation error, system error, disabled, success, and any
   destructive confirmation or recovery state that applies.
5. Write the semantic HTML outline or static prototype. Use CSS for layout,
   density, responsive changes, visual states, and motion preferences.
6. Specify responsive behavior by content pressure rather than device names.
   Cover narrow, intermediate, and wide layouts; touch and pointer targets;
   long translations; dense tables; and action placement.
7. Run an accessibility pass against the applicable WCAG 2.2 AA criteria.
   Include keyboard-only operation, focus visibility, screen-reader naming,
   contrast, 200% zoom, 320-pixel reflow, and reduced motion.
8. Produce an implementation map: semantic elements, CSS module/tokens,
   shared primitives, Rust state transitions, island boundary, server/no-JS
   fallback, localization keys, and focused verification.
9. When the design introduces or changes a reusable component, update
   `docs/reference/ui/components/` and the evidence inventory. Do not mark a
   component implemented or broadly adopted without source and usage evidence.

## Decision rules

- Two visually similar instances are a discovery signal, not automatic proof
  that they are one component. Compare purpose, semantics, states, responsive
  behavior, and keyboard behavior first.
- Prefer one obvious primary action per decision region. Visual prominence must
  follow user value and risk, not business pressure.
- Destructive actions need a clear label, proportional confirmation, and
  recovery where recovery is feasible. Do not add confirmation to routine,
  reversible actions.
- Keep advanced controls discoverable but subordinate. Do not hide information
  required to make an informed choice.
- If evidence from real users is missing, label the point as a research question
  instead of presenting taste as fact.

## Deliverable

Report in this order:

1. User outcome and shortest task flow.
2. Semantic HTML structure or prototype paths.
3. Responsive and state behavior.
4. Accessibility acceptance criteria.
5. Reused, extended, and proposed components.
6. Rust/Leptos and CSS implementation map.
7. Verification plan, assumptions, and research gaps.

## Primary references

- WCAG 2.2: <https://www.w3.org/TR/WCAG22/>
- WAI-ARIA Authoring Practices: <https://www.w3.org/WAI/ARIA/apg/>
- Tunarr project: <https://github.com/chrisbenincasa/tunarr>
