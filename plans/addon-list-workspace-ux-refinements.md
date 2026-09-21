# Add-on workspace refinement acceptance

Phase 4 is entering testing, per the maintainer. Historical phase 1-4 test
reports and the publication-island correction are recorded under `amm-n-01b`
in [milestones](../docs/progress/milestones.toml); they do not establish final
acceptance. ADR 0091 owns the design and ADR 0094 owns automatic publication.

## Remaining checks

- Verify active-filter chips and Clear filters reset category/query/page while
  retaining all unrelated draft fields. Pointer-picked category/page size must
  submit the native form and restore focus; keyboard selection must remain
  usable without reloading on every arrow key.
- Verify First/Last and Page X of Y, top-of-item Save, replacement-panel Save,
  and Numbered/Unnumbered behavior with real persisted state in hydrated/no-JS
  Chromium and Firefox. No direct page-number input is required.
- Recheck publication-island console errors after the page hydration flag using
  `tests/e2e/addon-list-publication-island.spec.ts`. Use a matched SSR/WASM build;
  a separately rebuilt server is not valid hydration evidence.
- Verify the accepted 2026-09-21 ADR 0091 amendment: rows remain editable below
  the numbering review, and Apply saves the current Items task including later
  row edits. Explicitly exercise post-proposal edits in hydrated/no-JS Chromium
  and Firefox. Preserve Cancel's pre-preview draft and unchanged database.
  The decision and four native browser cases now pass; the milestone owns
  evidence. Keep the original frozen reference unchanged.
- Check item visibility, filter/tool geometry, focus, contrast and principal
  controls at 320px and desktop, both themes and long localized text. Preserve
  paging/filter draft retention, off-page selections, independent season
  detection and automatic saved-change publication.

## Verification and recording

Reuse the already recorded 44/48/52/62/80/94-case runs only where their code,
inputs and assumptions remain valid. For changed behavior use focused workspace,
item-form, metadata-editor, preparation-feedback and publication-island specs,
then the owning milestone's remaining acceptance. Inspect real persistence and
browser errors rather than only the page-ready flag. Keep temporary profiles
and captures outside the reference bundle.

Record new results and any failures under `amm-n-01b`, preserving N-03's
integrated platform/release gates. Prune this plan only after the remaining
work is either accepted or explicitly transferred to its owner.

Reference: [locked task design](../docs/reference/ui/design-artifacts/admin-ux-organize/LOCK.md).
