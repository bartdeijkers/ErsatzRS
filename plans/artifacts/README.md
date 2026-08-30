# Admin UX artifacts

These review artifacts preserve UX decisions and compare implementation
directions before production work begins. They are design references, not a
second UI implementation; production surfaces remain localized Rust/Leptos
pages with SSR and focused hydration.

The [settings desktop layout proposals](settings-desktop-layout-proposals.html)
compare three compositions for Settings and Logs built from one standardized
component kit. The artifact keeps page composition content-specific while
standardizing headers, sections, fields, toolbars, data surfaces, pagination,
actions, and resource states.

The [search-to-collection design variants](search-collection-design-variants.html)
retain the selected Variant 4 as a baseline and compare five focused ways to
improve it: a quick-only drawer, focus-mode takeover, boolean group cards,
guided intent, and a dedicated collection composer. The revised recommendation
keeps global `/search` results-first, then carries one canonical query into a
dedicated smart-collection composer using group cards and focused editing only
when nesting requires it.

The [production search UX/UI visual review](search-ux-ui-visual-review.md)
records the final Chromium/Firefox desktop and mobile assessment, the resolved
hierarchy, contrast, control-label, and drawer-state findings, and the remaining
visual-proof limits.

## Interactive artifacts

- [Add list and import metadata](addon-list-add-layout.html) covers the compact
  source controls, collection-specific iconography, the two Import/Publish
  steps, prominent review count, and the combined list/item review flow.
- [Option D metadata editor](addon-metadata-editor-layouts.html) covers the
  collection-aware overview, inline accordion editor, review filtering,
  pagination, movie primary/extra roles, dense large-tag editing, bounded
  artwork uploads, and provider-versus-operator refresh conflicts.
- [Publication readiness layout proposals](addon-list-readiness-layout-proposals.html)
  compares three responsive alternatives to the page-wide sticky readiness
  aside: a collection-first status band, a review workspace rail, and a
  contextual publication footer. Proposal B was selected for the production
  add-on-list review workspace.

## Browser evidence

The screenshots under [screenshots](screenshots/) capture the desktop and
mobile iterations used to validate hierarchy, spacing, accordion behavior,
Beeld & Geluid series metadata, and the yt-dlp playlist flow. Files containing
`final` are the final artifact states; the remaining captures document the
implementation comparison that led to them.

The final desktop/mobile captures are named `addon-list-import-final-*` and
`addon-metadata-option-d-final-*`. The implementation acceptance evidence is
recorded in `docs/progress/milestones.toml` under `amm-c-01`, `amm-c-02`, and
`amm-e-02`; the artifact files do not replace the production Chromium/Firefox
and hydration regression gates.

The publication-readiness comparison renders are named
`addon-list-readiness-proposal-{a,b,c}-desktop.png`; Proposal A also has the
responsive capture `addon-list-readiness-proposal-a-mobile.png`.
