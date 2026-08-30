# Search UX/UI visual review

Date: 2026-08-30
Status: all findings in this review are resolved in the working tree

## Scope and evidence

This focused review covers the production `/search` results page and the
smart-collection composer reached through `Create smart collection`. It checks
the selected Variant 4 plus focused boolean-group design, not the superseded
prototype variants.

Evidence was collected in the dark theme at these viewports:

| Surface | Desktop | Mobile | Browsers |
|---|---:|---:|---|
| Search results | 1280 x 720 | 390 x 844 | Chromium and Firefox |
| Smart-collection composer | 1280 x 720 | 320 x 800 | Chromium and Firefox |

The executable evidence lives in
[`collections.spec.ts`](../../tests/e2e/collections.spec.ts) and
[`search-see-all.spec.ts`](../../tests/e2e/search-see-all.spec.ts). Each run
captures `search-results-*` and `search-composer-*` screenshots under the
gitignored `test-results/` directory. The synthetic bright-green artwork in
those captures is test data and was excluded from the visual assessment.

The review applies the cognitive-load, disclosure, clear-control, filtering,
and reflow references already retained by the
[search-to-collection design artifact](search-collection-design-variants.html),
including WCAG 2.2 Reflow and W3C clear-control guidance.

## Findings and implemented changes

| Severity | Confidence | Finding | Implemented change | Acceptance evidence |
|---|---|---|---|---|
| P2 | High | A desktop-open navigation state could remain open after entering a 320–390 px viewport and obscure most search content. | Mobile now always initializes closed without overwriting the stored desktop preference. The drawer remains `aria-hidden` and inert when closed. | Chromium and Firefox assert `aria-expanded=false`, wait until the transition has moved the drawer fully outside the viewport, and then prove no horizontal overflow. |
| P2 | High | The two result-wide actions had the same visible and accessible name, `Add All`; their collection-versus-playlist purpose existed only in a tooltip. | The buttons now use localized `Add All to Collection` and `Add All to Playlist` labels in all supported cultures. | Both accessible names are asserted before the result actions are exercised. |
| P2 | High | Small green links and 40%-white helper text measured about 3.55:1 and 3.79:1 on the dark surfaces, below the 4.5:1 target for normal text. The green button fill also gave white text about 4.21:1. | Search links use the higher-contrast action color; small helper text uses `gray-400` (about 6.49:1); dark search/composer primary buttons use `#008500` (about 4.82:1 against white). | Source-token calculation plus final Chromium and Firefox captures in both target widths. |
| P3 | High | The main query input looked like unlabelled text placed beside the heading. | The existing semantic label is retained and the input now uses the shared visible search-field boundary and focus treatment. | Desktop and mobile screenshots show a distinct editable field without adding instructional copy. |
| P3 | High | Result counts ran together as a dense green text row, while wrapped actions left almost no separation before the first result heading. | Counts are compact, wrapping navigation chips; actions form a distinct group; the result toolbar has a stable bottom gap. | The 390 px captures retain a clear Filters → counts → actions → results sequence. |
| P3 | High | At 320 px the status badge reduced the composer heading width enough to wrap both the eyebrow and title. | The mobile composer header stacks the status after the full-width title. | Chromium and Firefox show one-line `Smart Collection Composer` and `Smart Collection` labels at 320 px. |
| P3 | High | Expanding saved searches repeated `Saved searches` as both disclosure title and field label. | The field label now describes the immediate choice: `Choose a saved search`. | The label, placeholder option, and saved-search actions remain keyboard and no-JavaScript operable. |

## Dark-pattern and friction check

No deceptive patterns were found. Search does not hide costs or side effects,
preselect destructive choices, manufacture urgency, or make cancellation and
return navigation harder than creation. A saved advanced query remains an
authoring draft with no collection, cache, schedule, or playout side effects.

The remaining friction is intentional and proportional: collection identity
is required only when saving a smart collection, advanced controls start
collapsed, nested logic opens one boolean level at a time, and the canonical
query is available for expert review without dominating the default surface.

## Responsive and accessibility matrix

| Check | Result |
|---|---|
| 320 CSS px composer reflow | Pass: one column, no page-level horizontal scroll, full-width primary save action |
| 390 CSS px search reflow | Pass: closed drawer, wrapping count chips and actions, no page-level horizontal scroll |
| Keyboard disclosure and nested focus | Pass: native `details` and buttons; Enter opens focused nested editing and an explicit Back action returns |
| Focus visibility | Pass by source review: query fields, disclosures, chips, buttons, rule controls, and canonical editor retain visible focus styles |
| Text contrast in changed search surfaces | Pass for reviewed dark-theme normal text; helper text is about 6.49:1 and dark primary-button text about 4.82:1 |
| Hydrated/no-JavaScript parity | Pass: query handoff, smart-collection save, saved-query load/update, and exact return URL have both paths covered |

## Reused component catalog

- App shell: navigation drawer, top bar, search field, action button.
- Search discovery: native filter disclosure, active-filter links, result-count
  links, media cards, selection toolbar, collection/playlist dialogs.
- Collection authoring: action bar, details disclosures, status badge, boolean
  group cards, focused nested-group view, rule dialog, saved-search controls,
  and live preview.

No new one-off interaction primitive was introduced during finetuning. The
count-chip styling is local to search because those chips navigate to result
sections; it is not a second global facet component.

## Residual limits

- The final visual screenshots use English. All new labels have translation
  coverage, and the action group wraps, but long-label screenshots were not
  captured for every culture.
- Light-theme token contrast was checked from source, but this focused pass did
  not add a second screenshot matrix for the light theme.
- The browser captures use deterministic synthetic artwork; real library image
  cropping and perceived image quality are outside this query-composition
  review.
