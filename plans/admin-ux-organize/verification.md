# Revised artifact verification

The current revision uses the local synthetic preview server with browser
JavaScript disabled. No production server, provider or database mutation was
performed. The prior audit remains in [review-evidence.json](review-evidence.json).

## Visual hierarchy and Save/Publish flow

The current UX checks cover the new Save labels, explicit edit-selection wording,
and one native default/override selector per playback option. They prove that an
explicit value equal to the add-on default remains an override, invalid settings
do not partially change saved origins, and publishing saved changes retains newer
unsaved row inputs. Existing numbering, recovery, metadata and page-size scenarios
continue to use their original native submission intents. Earlier screenshots and
reports describe the UI vocabulary and controls that existed at that revision.

## Replacement-readiness corrections

```bash
node plans/admin-ux-organize/verify-replacement-readiness.cjs http://127.0.0.1:8412 chromium
node plans/admin-ux-organize/verify-replacement-readiness.cjs http://127.0.0.1:8412 firefox
```

The `replacement-*-verification.json` reports record native forms with JavaScript
disabled. They cover metadata/chapter draft retention across Previous/Next,
invalid chapter ordering without partially applying metadata, Apply/reload and
Discard. They also cover visible recovery outside collapsed diagnostics,
eligible retry, stale-generation rejection, running/completed state and retained
numbering drafts.

The 165-item scenario uses only synthetic titles and unknown dates/runtimes.
Checks exercise every page size, filters, last-item navigation, off-page
selection, retained edits and Apply. Layout checks cover dark/light themes,
320–1440px, the full 165-row All view and unobscured keyboard focus. Item metadata
editors receive equivalent theme/reflow checks at 320, 390, 720 and 1440px.
Screenshots use the `screenshots/replacement-` prefix. This is prototype evidence;
real providers, recovery, persistence, localization and production performance
remain outside its acceptance claim.

## Access settings on wide displays

`access-stack-verification.json` records 16 layout checks in Chromium and Firefox,
dark/light themes, at 320, 390, 1920 and 3840 CSS pixels. Browser cookies and
SponsorBlock are vertically stacked at every width, with aligned labels and
supporting values, a 48rem maximum group width, 44px select controls and no page
overflow. The current selector-based controls replace the earlier enable/reset checkboxes.
Both explicit choices and aligned supporting text were inspected. The
`screenshots/access-stack-*-4k.png` images are element crops captured at a
3840×2160 viewport; mobile captures use the `-390` suffix.

## Simplified bulk and settings

The current `verify-ux-fixes.cjs` additionally checks that opening the bulk editor
reveals numbering, classification and provider controls without nested task
accordions. It verifies visible genres/tags/source tools in List settings,
visible reset/storage controls and no disclosures in Playback and access,
metadata/options preview, package-default reset, and validation with visible
storage input. Both themes and 320/390/720/1440px are covered; screenshots use
`screenshots/simplified-`. Existing disclosure keyboard checks now cover only
retained disclosures; storage validation and source draft-retention checks remain.

## UX review corrections

Run the focused browser checks against the synthetic preview server:

```bash
node plans/admin-ux-organize/verify-ux-fixes.cjs http://127.0.0.1:8412 chromium
node plans/admin-ux-organize/verify-ux-fixes.cjs http://127.0.0.1:8412 firefox
```

The generated `ux-fixes-*-verification.json` reports cover both themes at
320, 390, 720, 1024 and 1440px with JavaScript disabled. They check mobile drawer
defaults and keyboard toggling, top-of-list Apply, compact rows with 44px number
inputs, error-summary and field focus, off-page/filter correction with retained
drafts and selection, stable Previous/Next slots, and section disclosures with
invalid-input recovery. Screenshots use the `screenshots/ux-fixes-` prefix.

These reports supersede earlier evidence for the modified shell, item rows,
numbering errors, navigation, disclosure grouping and pagination. Page-size
checks exercise 10/25/50/100/All items, forward/back navigation, pending edits and
checked-selection retention. These checks do not constitute a
full WCAG audit or production acceptance. The 720px viewport is a reflow check,
not a claim of actual browser zoom testing.

The action-hierarchy revision additionally checks exactly one Apply/Discard pair,
absence of redundant jump links, a persistent bar that does not obscure focused
fields, contextual Undo and Publish availability, and pending-draft retention
through the Source tools disclosure. Routine mobile rows remain under 160px with
44px input targets; exceptional and long-content rows may grow.

## Earlier shared-grid layout

The current artifact adds [alignment.css](alignment.css): shared Grid/subgrid
tracks for repeated rows, consistent form and media columns, and wrapping flex
action groups. It preserves the distinct Help, settings and section disclosures.

[grid-verification.json](grid-verification.json) records 498 passing checks and
144 layouts in Chromium/Firefox with JavaScript disabled, both themes, and
320/768/1440 widths. It checks shared column coordinates, stable expanded Help
and bulk triggers, grouped Apply, and expanded semantics/form ownership across
all 85 documents. [grid-polish-verification.json](grid-polish-verification.json)
adds 88 passing checks for multiline checkbox alignment, comparison-button
baselines, responsive artwork and toggling the original drawer.

Earlier screenshots: [list](screenshots/grid-index.png),
[playback](screenshots/grid-options.png), [settings](screenshots/grid-settings.png),
[artwork](screenshots/grid-artwork.png), [activity](screenshots/grid-activity.png),
[narrow open help](screenshots/grid-options-mobile.png).
Original header/drawer markup remains unchanged; all help and form references
resolve. The grid evidence supersedes earlier layout measurements; previous
workflow evidence below remains scoped to the revision it exercised.

## Prior disclosure presentation

The triangle-only hints were superseded by compact Help controls, labeled
plus/minus expanders for settings, and full-width section disclosures.
[hint-controls-verification.json](hint-controls-verification.json) records 444
passing checks across Chromium/Firefox, dark/light themes and 320/768/1440 widths
with JavaScript disabled. Six representative views cover every disclosure family.
Checks exercise Enter/Space operation, visible targets, stable help-trigger
placement, native-marker replacement and expanded reflow.

Current screenshots: [playback controls](screenshots/hints-options.png),
[open help](screenshots/hints-options-open-1440.png),
[narrow open help](screenshots/hints-options-open-320.png),
[list controls](screenshots/hints-index.png).

## Prior workflow revision evidence

[revision-verification.json](revision-verification.json) records 338 checks with
zero failures in Chromium and Firefox, including:

- All 85 HTML documents checked in each browser for scripts, duplicate IDs,
  unlabeled inputs and nested forms.
- 144 layouts: twelve representative views, both themes and widths 320, 768 and
  1440. No horizontal document overflow; the fixed header remains 56px.
- One grouped Apply for cookies/SponsorBlock, and pending default-reset behavior
  that preserves the other option.
- Navigation away from and back to settings retains a typed metadata draft.
- Canceling a numbering context retains another row's draft and checked IDs;
  explicit Discard restores numbering values.
- A read-only numbering preview with one Apply, exact selected scope, optional
  min/max length bounds, unknown-length inclusion and unchanged bloopers.
- Applying that review returns the expected S/E assignments.
- A 36-item list with ten-row pages and retained selection across page navigation.

At 1440×900 the first overview row starts around y=505px and five complete rows
are visible, versus y=1,004px and zero rows before this revision. At 320×900 the
first row starts around y=655px and one complete row is visible, versus y=1,532px
before. Measurements include the prototype notice and preserve the application
header; the drawer is closed at 320px. At 768×900 with the drawer open, the first
row starts around y=555px and one complete row remains visible.

The previous 284-check baseline and 58-check options revision describe superseded
UI arrangements. Their results are retained as history, not claimed as current
workflow acceptance.

[final-design-verification.json](final-design-verification.json) adds 246 checks
with zero failures: all 85 expanded documents at 320px in both browsers, valid
help references and form ownership, stable publication disclosure in six
width/theme combinations per browser, keyboard menus, long labels, Undo/draft
preservation, shared item numbering, duplicate rejection, filtered scope,
conditional artwork validation and originating-item replacement navigation.

[final-integrity.json](final-integrity.json) records artifact hashes, zero broken
local content links, unchanged original header/drawer markup and shell CSS. A
visible-text contrast spot check found no failures across eight task views in
both themes; it is not a complete WCAG evaluation.

The [closure review](review.md#follow-up-closure-review) has no open demonstrated
P1/P2 findings within this artifact scope. The 584 browser checks support that
review; their count does not prove usability.

## Prior workflow revision screenshots

- [Expanded publication details](screenshots/publication-expanded.png)
- [Items and numbering](screenshots/revised-index.png)
- [Items, narrow light theme](screenshots/revised-index-mobile.png)
- [List settings](screenshots/revised-settings.png)
- [Playback and access](screenshots/revised-options.png)
- [Playback, narrow light theme](screenshots/revised-options-mobile.png)
- [Activity](screenshots/revised-activity.png)
- [Numbering context](screenshots/revised-numbering-6.png)
- [Read-only numbering review](screenshots/revised-numbering-preview.png)
- [Item review](screenshots/revised-item-6.png)
- [Artwork](screenshots/revised-artwork.png)
- [Media tracks and ownership](screenshots/revised-media-details.png)
- [Replacement selection](screenshots/revised-replacement.png)
- [Reset impact](screenshots/revised-reset-impact.png)
- [Recovery](screenshots/revised-recovery.png)

## Limits

These checks verify browser behavior and the synthetic in-memory renderer. They
do not verify the authoritative Rust numbering algorithm, actual grouped database
updates, provider access, publication, reconciliation, recovery or uploaded image
processing. Some consequential states are explicitly fixed examples.

Both themes were visually inspected; a complete contrast/assistive-technology
conformance audit and an observed human task study remain separate acceptance.
The skill-creator validator checks the draft skill package shape, not expertise
or usability. No production build or milestone closeout is claimed.
