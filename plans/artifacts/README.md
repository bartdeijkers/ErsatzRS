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
  pagination, movie primary/extra roles, and dense large-tag editing.

## Browser evidence

The screenshots under [screenshots](screenshots/) capture the desktop and
mobile iterations used to validate hierarchy, spacing, accordion behavior,
Beeld & Geluid series metadata, and the yt-dlp playlist flow. Files containing
`final` are the final artifact states; the remaining captures document the
implementation comparison that led to them.

The implementation acceptance evidence is recorded in
`docs/progress/milestones.toml` under `amm-c-01`; the artifact files do not
replace the Chromium/browser and hydration regression gates.
