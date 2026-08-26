---
name: admin-ux-audit
description: Audit ErsatzRS admin UX/UI for deceptive patterns, unnecessary friction, inconsistent or inaccessible components, weak responsive behavior, and visual-system drift. Use for read-only UX reviews, dark-pattern checks, WCAG or responsive audits, or repeated-component discovery; do not implement fixes unless the user asks.
---

# Admin UX Audit

Audit a route or task flow with evidence. Diagnose first; an audit does not
authorize production fixes. When the user explicitly includes component-library
maintenance, update `docs/reference/ui/components/` and the inventory as part of
the audit. Otherwise report proposed catalog changes without editing them.

## Required context

Read these before auditing:

- `docs/reference/ui/README.md`, `docs/reference/ui/visual-language.md`, and
  `docs/reference/ui/component-inventory.md`.
- The relevant entries in `docs/reference/ui/components/`.
- The route, nearby routes, CSS modules, shared primitives, localization keys,
  and browser tests that cover the task.
- `TEST.md` when interactive or hydration behavior is in scope.

Inspect both rendered behavior and source when available. Cover narrow and wide
viewports, keyboard-only use, pointer/touch use, loading and error states, long
localized strings, and both supported themes where relevant.

## Audit passes

1. **Task path:** state the user's goal, count required decisions and context
   switches, and identify avoidable navigation, duplicate entry, dead ends,
   missing undo, or premature confirmation.
2. **Deceptive patterns:** check for visual choice asymmetry, hidden material
   information, preselected consent, forced action, nagging, obstruction,
   confirmshaming, trick wording, disguised controls, false urgency or scarcity,
   and difficult cancellation or deletion. Report product risk; do not present
   the result as a legal conclusion.
3. **Usability heuristics:** check system status, real-world language, user
   control, consistency, error prevention, recognition over recall, efficiency,
   restrained presentation, recoverable errors, and contextual help.
4. **Accessibility:** evaluate applicable WCAG 2.2 AA behavior, including
   landmarks and headings, accessible names, labels and instructions, keyboard
   operation, focus order and visibility, status/error announcements, contrast,
   target size, zoom, reflow, and motion.
5. **Responsive behavior:** test 320 CSS pixels through wide desktop. Flag page
   overflow, hidden actions, content loss, unstable layout, undersized targets,
   unreadable density, and tables that lack a deliberate narrow-screen model.
6. **Visual system:** compare hierarchy, spacing, alignment, type, surface and
   color roles, control shapes, density, icon use, and interaction states with
   the documented visual language and neighboring routes.
7. **Runtime boundaries:** check localization, SSR/no-JavaScript fallbacks,
   stable hydration DOM, loading, empty, error, disabled, success, destructive,
   and recovery states.

## Evidence and severity

For every finding record:

- ID, route and user task.
- Reproduction steps and concrete evidence.
- Violated criterion or catalog contract.
- Severity: critical, high, medium, low, or note.
- Confidence: proven, likely, or hypothesis.
- User impact and affected input/viewports.
- Smallest credible recommendation and acceptance test.

Use **critical** for likely irreversible harm, loss of informed choice, or a
blocked essential task; **high** for probable task failure, deception, or a
major accessibility barrier; **medium** for substantial friction or recurring
inconsistency; and **low** for localized friction or polish. Frequency,
persistence, and recoverability can move a finding up or down. A heuristic
violation is evidence to investigate, not proof by itself.

## Repeated-component discovery

1. Locate existing primitives and all call sites before grouping raw markup.
2. Scan repeated semantic elements and CSS selector families, especially
   buttons, links styled as buttons, form fields, tables, pagination controls,
   dialogs, cards, filters, tabs, alerts, and loading/empty/error blocks.
3. Compare purpose, HTML semantics, labels, state model, keyboard behavior,
   responsive behavior, tokens, localization, and hydration/fallback behavior.
4. Classify each cluster as the same component, a supported variant, a
   composite pattern, or coincidental visual similarity.
5. Record source evidence and consumers. Use the catalog statuses `candidate`,
   `implemented`, and `deprecated`; describe adoption separately.
6. Do not abstract solely because two instances look alike. Conversely, one
   security-, accessibility-, or data-loss-sensitive control may justify a
   shared primitive even before it is repeated.

The catalog describes actual repository state. Do not claim broad adoption
because a primitive exists, and do not claim WCAG conformance without focused
evidence. Recommendations and prototypes must use semantic HTML and CSS, not
TypeScript or a JavaScript component framework.

## Deliverable

Report in this order:

1. Scope, task, environments, and evidence inspected.
2. Findings ordered by severity, with confidence and acceptance tests.
3. Dark-pattern and unnecessary-friction result, including explicit negatives.
4. Responsive and WCAG coverage matrix.
5. Repeated-component clusters and catalog changes.
6. Unknowns, user-research questions, and residual risk.

## Primary references

- WCAG 2.2: <https://www.w3.org/TR/WCAG22/>
- Nielsen's usability heuristics: <https://www.nngroup.com/articles/ten-usability-heuristics/>
- FTC dark-pattern report: <https://www.ftc.gov/reports/bringing-dark-patterns-light>
- EU Digital Services Act, Article 25: <https://eur-lex.europa.eu/eli/reg/2022/2065/oj>
