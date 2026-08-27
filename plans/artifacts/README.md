# Add-on metadata editor artifacts

These review artifacts preserve the UX decisions that guide the shared
add-on-list implementation. They are design references, not a second UI
implementation; the production surface remains the localized Leptos
**Add-on Lists** page.

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
