# Plan: make add-on-list import, review, and publication self-explanatory

## Implementation status

Proposed on 2026-08-26 after a read-only UX audit of the live temporary
Windows dogfood instance documented in
[`docs/how-to/glamourland-addon-list-ux-walkthrough.md`](../docs/how-to/glamourland-addon-list-ux-walkthrough.md).
Production behavior is unchanged. Open milestone `amm-i-01` and parity debt
`addon-list-import-review-ux-obscures-state-and-mobile-actions` own delivery.

This is an ErsatzRS product extension. The parity record confirms that
ErsatzTV has no repository-installed add-on draft equivalent, so the existing
provider-neutral decisions in ADRs 0072, 0073, 0075, and 0079 are the behavior
contract. This plan improves how those decisions are presented; it does not
replace them.

## Scope, user outcome, and evidence

The primary user is an administrator importing a provider list, reviewing its
items, and explicitly publishing a stable revision. Success means that the
administrator can discover the flow, understand what the host is doing, reach
every item action at 320 CSS pixels and by keyboard, distinguish advice from a
publication blocker, and know which revision will be published.

The audited path covered add-on installation and enablement, source URL
resolution, overview discovery, draft save, item review, bulk numbering, and
the responsive editor. It stopped before detail enrichment, publication,
projection, playback, or rollback.

Evidence inspected:

- the walkthrough's accepted Windows run at 1278 by 1304 and 390 by 844 CSS
  pixels;
- a read-only Chromium probe against the still-running temporary instance at
  1278 by 1304, 390 by 844, and 320 by 800 CSS pixels, in dark theme plus a
  desktop light-theme visual pass;
- keyboard focus order, DOM geometry, accessible names, visible action state,
  hydration completion, and source markup;
- `addon_lists.rs`, its CSS module, shared navigation and add-on-detail pages,
  localization, server functions, HTML fallbacks, and current browser tests;
- the admin visual language and component catalog;
- ADRs 0072, 0073, 0075, and 0079 plus the add-on media-list reference and
  `addon-metadata-management` parity record.

The live probe did not submit a form, fetch item details, change an item,
publish, or write the temporary database. External thumbnail requests were
blocked by the probe environment and are not treated as product console
failures; the accepted walkthrough recorded zero browser errors and warnings.

## Findings

### AAL-UX-01 — the closed mobile drawer receives invisible keyboard focus

- **Severity / confidence:** high / proven.
- **Route and task:** every admin route at less than 768 CSS pixels; open
  navigation and reach Add-on lists.
- **Evidence:** at both 390 and 320 pixels, Tab moved from the logo through
  approximately fourteen drawer links and summaries at `x = -216` or
  `x = -208` before reaching the first page action. The visible hamburger is a
  `<label>` for a `display: none` checkbox and is not keyboard focusable. The
  closed drawer is only translated off canvas, so its descendants remain in
  the tab order.
- **Criterion:** WCAG 2.2 2.1.1 Keyboard, 2.4.3 Focus Order, and 2.4.7 Focus
  Visible; the repository's semantic-interaction and primary-action contract.
- **Impact:** keyboard and switch users cannot operate the visible menu button,
  receive no visible focus for many Tab presses, and cannot discover the
  add-on-list route through the intended mobile navigation.
- **Smallest credible recommendation:** make the toggle a native button with
  `aria-expanded` and `aria-controls`; make a closed drawer inert and hidden
  from assistive technology; move focus into the opened drawer and restore it
  on Escape or close.
- **Acceptance:** a keyboard-only 320-pixel run reaches the menu button after
  the logo, never focuses closed-drawer descendants, opens the drawer with
  Enter or Space, reaches Add-on lists, closes with Escape, and returns focus
  to the toggle.

### AAL-UX-02 — the item editor has no usable narrow-screen model

- **Severity / confidence:** high / proven.
- **Route and task:** `/media/addon-lists/:id`; inspect, include, classify, or
  edit one imported item.
- **Evidence:** the live table was 1382 pixels wide inside a 342-pixel region
  at 390 pixels and a 272-pixel region at 320 pixels. Its only horizontal
  scrollbar followed about 1882 pixels of table content. First-air date,
  season/episode, kind, playback behavior, status, and row actions could not be
  kept in the same task context. The walkthrough observed the same failure
  with an approximately 1741-pixel table and more than 4000 pixels of vertical
  travel when all rows were selected.
- **Criterion:** WCAG 2.2 1.4.10 Reflow, 1.4.4 Resize Text, 2.4.3 Focus Order,
  and 2.5.8 Target Size; the visual-language requirement that essential
  actions remain reachable at 320 pixels.
- **Impact:** touch, magnification, and narrow-window users can see an item but
  cannot efficiently reach its state or actions. The bounded-table exception
  is not sufficient for this interactive editor because its row fields can be
  represented without preserving a two-dimensional comparison grid.
- **Smallest credible recommendation:** replace the editable table with one
  semantic ordered review list that uses a compact grid at wide widths and
  labelled item cards at narrow widths. Keep provider order, the dedicated
  date and season/episode values, and the existing item-detail route.
- **Acceptance:** at 320 pixels and at 200% zoom, every visible item exposes
  its order, inclusion, title, first-air date, season/episode, kind, playback
  behavior, status, and review action without horizontal scrolling or hidden
  focus.

### AAL-UX-03 — critical create and publish paths do not have faithful no-JS fallbacks

- **Severity / confidence:** high / proven for markup and request shape;
  likely for the resulting state loss because no live mutation was attempted.
- **Route and task:** `/media/addon-lists/add` and
  `/media/addon-lists/:id`; import, save, and publish while hydration is absent
  or has failed.
- **Evidence:** the add form has a hydrated `on:submit` handler but no native
  method/action. The existing add POST does not carry all visible refresh,
  playlist, and presentation choices or redirect to the new editor. The editor
  POST rebuilds omitted folder, presentation, review, revision, and progress
  fields from fallback defaults, while Publish is a hydrated button only.
- **Criterion:** the repository runtime-boundary contract and `PORTING.md`
  requirement that critical save/action flows retain an SSR or browser-safe
  fallback; WCAG 2.1.1 Keyboard when script failure removes the only action.
- **Impact:** a checksum mismatch or hydration failure can turn a complete
  rendered form into a dead or behaviorally different workflow. A fallback
  save must never substitute Remote Streams or empty/default state for the
  current draft.
- **Smallest credible recommendation:** route hydrated and native POST flows
  through the same application commands. Native add must accept every visible
  setting and redirect to the created editor; native save must load current
  state and patch only submitted fields; native Publish must remain explicit.
- **Acceptance:** Playwright with JavaScript disabled imports a deterministic
  list, saves list-level settings without changing unsubmitted state, and
  initiates publication through visible native controls.

### AAL-UX-04 — Ready does not reliably reveal the persisted overview

- **Severity / confidence:** high / proven in the walkthrough.
- **Route and task:** `/media/addon-lists/:id?import=1`; continue from completed
  discovery into review.
- **Evidence:** the progress card reported `Ready to review: 48`, `48 / 48
  Discovered`, while the review section reported `0 Discovered` and rendered no
  rows. Selecting `Save add-on list` made all 48 rows appear. No copy explained
  that Save would materialize provider output. The source already attempts to
  reload rows as polling markers change, so runtime evidence shows that the
  current trigger or read boundary is insufficient.
- **Criterion:** visibility of system status, match between system and the real
  world, recognition over recall, and the ADR 0079 promise that a verified
  overview enters the reviewable draft.
- **Impact:** the main import task appears complete and empty at the same time.
  Administrators must guess that an unrelated-looking write action is the
  recovery path.
- **Smallest credible recommendation:** when discovery becomes Ready, reload
  the committed list header and items and populate review automatically. A
  provider write or operator save must not be required to read committed
  overview rows.
- **Acceptance:** a browser regression observes the Ready transition and the
  exact discovered row count in the review list without reload, navigation,
  Save, or any additional database mutation.

### AAL-UX-05 — Save and Publish can target different perceived revisions

- **Severity / confidence:** high / likely, supported by source and live
  geometry.
- **Route and task:** `/media/addon-lists/:id`; edit list metadata or settings
  and publish the intended revision.
- **Evidence:** Save is at document position 70 while Publish was at about 3194
  on desktop, 4302 at 390 pixels, and 4708 at 320 pixels. Save serializes
  list-level signals, while Publish dispatches only the list ID. There is no
  visible dirty state or explanation that persisted row/bulk edits and
  unsaved list-level edits have different save behavior.
- **Criterion:** error prevention, visibility of system status, user control,
  and one dominant action per decision region.
- **Impact:** an administrator can reasonably believe the current visible form
  will be published when the server may still hold an earlier list-level
  revision.
- **Smallest credible recommendation:** expose a persistent workflow state and
  one sticky primary action. Unsaved list-level changes make `Save changes`
  primary and Publish unavailable with an explanation; a clean review draft
  makes Publish primary. Immediately persisted row/bulk actions must say so and
  retain Undo.
- **Acceptance:** changing one list-level field immediately shows `Unsaved
  changes`; Publish cannot target the stale revision; successful Save changes
  the state to `Draft saved` and exposes Publish without a long scroll.

### AAL-UX-06 — review advice and publication blockers use the same visual language

- **Severity / confidence:** medium / proven for presentation; publication
  consequence beyond preparation remains outside the walkthrough.
- **Route and task:** `/media/addon-lists/:id`; decide whether the draft is safe
  to publish.
- **Evidence:** Publish was enabled with `48 Need review` and `0 / 48 Details
  complete`. The label `Need review` sounds blocking, while ADR 0079 explicitly
  permits a verified overview to publish before optional details. Source shows
  that missing playback-required finite durations instead start durable
  preparation and require a later Publish attempt.
- **Criterion:** informed choice, clear system status, error prevention, and
  restrained presentation.
- **Impact:** administrators cannot tell whether Publish is unsafe, advisory,
  or about to queue prerequisite work instead of publishing.
- **Smallest credible recommendation:** render separate `Blockers` and `Review
  recommendations` counts from one authoritative readiness model. Optional
  details and advisory review do not disable Publish. Incomplete overview,
  validation failures, zero included items, unsaved list settings, or required
  playback preparation do.
- **Acceptance:** a clean 48-item overview with 48 recommendations and zero
  optional details explains those facts and keeps Publish enabled; every
  disabled state exposes a localized reason and a control that moves to the
  affected items or progress state.

### AAL-UX-07 — entry points do not follow the operator's list-oriented mental model

- **Severity / confidence:** medium / likely from one dogfood session; current
  route presence is proven.
- **Route and task:** `/settings/addons`, the shared sidebar, and
  `/media/addon-lists`; find where an enabled list-capable add-on is used.
- **Evidence:** the walkthrough could not find the route and used the address
  bar. Current source and the live instance now expose Add-on lists under
  `Media sources`, so the original missing-link observation is no longer
  reproducible. The link remains absent from the `Lists` group, and the enabled
  add-on detail declares `media-list` capabilities without offering a task
  action.
- **Criterion:** match with user language, recognition over recall,
  consistency, and efficient task paths.
- **Impact:** an administrator who has just enabled a provider must switch
  context, infer that a named list is a media source, or already know the route.
- **Smallest credible recommendation:** place Add-on lists in the `Lists`
  group, with no duplicate sidebar entry, and show `Import a list` on an
  enabled add-on detail when it declares a supported media-list capability.
  The contextual link may preselect that add-on but must keep ADR 0072's
  source-first resolution visible and overridable.
- **Acceptance:** both entry points reach `/media/addon-lists/add`; the add-on
  context is announced and editable; add-ons without list capability do not
  show the action.

### AAL-UX-08 — long provider descriptions displace the review task

- **Severity / confidence:** medium / proven.
- **Route and task:** `/media/addon-lists/:id`; move from list identity to item
  review on a narrow screen.
- **Evidence:** the 739-character series description occupied 519 pixels at a
  390-pixel viewport and 778 pixels at 320 pixels before settings and item
  controls.
- **Criterion:** WCAG 1.4.10 Reflow, efficiency, and restrained presentation.
- **Impact:** the provider description consumes roughly one full mobile screen
  before the operator reaches the import and review state.
- **Smallest credible recommendation:** clamp long descriptions by content
  pressure and provide a localized `Show full description` / `Show less`
  button with `aria-expanded` and a stable target.
- **Acceptance:** the collapsed mobile hero uses no more than four text lines,
  the full description is available by keyboard and touch, and expansion does
  not discard focus or editor state.

### AAL-UX-09 — completed progress renders an empty elapsed-time value

- **Severity / confidence:** low / proven.
- **Evidence:** both the walkthrough and live probe rendered `Elapsed time:`
  with no value after discovery completed.
- **Criterion:** system status and accessible status announcements.
- **Impact:** the field adds noise and cannot distinguish a quick completion,
  missing data, or stalled work.
- **Recommendation and acceptance:** render a valid duration such as
  `Completed in 10 seconds`; if the timestamps cannot produce one, omit the
  complete label/value pair from visual and accessible output.

### AAL-UX-10 — add-on readiness copy leaks command-shell escaping

- **Severity / confidence:** low / proven.
- **Route and task:** `/settings/addons/:id`; confirm that installation and
  enablement succeeded.
- **Evidence:** the live status was `Beeld ^& Geluid is ready.`. The host shows
  the installed add-on's `last_check_message` as opaque text.
- **Criterion:** match with natural language, professionalism, and consistent
  system status.
- **Impact:** the success message looks corrupted and weakens confidence in
  the installation result.
- **Recommendation and acceptance:** fix quoting in the official package's
  Windows entrypoint rather than stripping arbitrary carets in the host; freeze
  the exact message `Beeld & Geluid is ready.` in Windows and POSIX package
  tests.

### AAL-UX-11 — protective confirmations do not state their resulting status

- **Severity / confidence:** low / proven from walkthrough and source.
- **Route and task:** `/settings/addons/:id`; install and enable a verified
  package.
- **Evidence:** Install and Enable each open a generic Confirm modal. The trust
  and permission review is legitimate, but the copy does not say that Install
  leaves the add-on disabled or that Enable activates its declared access.
- **Criterion:** informed choice, error prevention, and task completion status.
- **Impact:** the second confirmation feels repetitive because the state
  transition protected by each step is not explicit.
- **Recommendation and acceptance:** retain proportional trust confirmation;
  title and body must name the add-on, version, trust source, permissions, and
  exact resulting state. Do not combine install and enable until package trust
  research proves that doing so preserves informed consent.

### AAL-UX-12 — the empty-state primary action is visually detached

- **Severity / confidence:** low / proven in the walkthrough.
- **Route and task:** `/media/addon-lists`; create the first list.
- **Evidence:** `Add add-on list` appeared above and left of the page heading,
  reading like a floating back/action control rather than the empty state's
  primary next step.
- **Criterion:** visual hierarchy, consistency, and recognition.
- **Recommendation and acceptance:** render the page heading first, then one
  empty-state message with a nearby primary `Add add-on list` navigation link;
  the non-empty page retains the same action in its normal page header.

## Deceptive-pattern and friction result

No forced action, preselected consent, confirmshaming, false urgency, scarcity,
nagging, disguised advertising, difficult cancellation, or destructive-choice
asymmetry was found. The install and enable confirmations protect a real trust
boundary and should not be removed merely to reduce clicks.

The material risk is weak informed choice rather than a dark pattern: Publish
is visually available while advice and blockers are not distinguished, and the
generic confirmation copy hides the result of each add-on state transition.
Unnecessary friction is proven in route discovery, the unexplained Save needed
to reveal imported rows, the vertical distance between Save and Publish, the
off-screen mobile navigation focus, and the item table's remote scrollbar.

## Responsive and WCAG coverage matrix

| Surface | Audited evidence | Result | Required implementation proof |
| --- | --- | --- | --- |
| Wide desktop, 1278 by 1304 | Dark and light visual pass; keyboard and geometry | Main page has no page-level horizontal overflow; review table still overflows its region | Dark/light, 1280 and wider, keyboard, focus visibility, contrast |
| Narrow, 390 by 844 | Dark visual pass; keyboard and geometry | 1382-pixel table in 342 pixels; 519-pixel description; closed drawer receives focus | No horizontal item-editor scroll; card/grid reflow; drawer focus management |
| Minimum, 320 by 800 | Dark visual pass; keyboard and geometry | 1382-pixel table in 272 pixels; 778-pixel description; closed drawer receives focus | WCAG 1.4.10 reflow, 24-pixel minimum targets, preferred 44-pixel primary targets |
| Keyboard only | Eighteen Tab stops captured without activating controls | Mobile focus order fails; desktop order reaches navigation before page content as expected | Menu open/close/focus return, item edit, pager, Save, Publish, dialogs |
| Hydration | Walkthrough and probe reported `window.__ersatzrs_hydrated = true` | Hydrated interactions available | Matched SSR/WASM browser proof plus JavaScript-disabled fallback |
| Themes | Dark at all sizes; light desktop visual inspection | No add-on-specific theme break found; contrast was not measured | Automated contrast plus manual focus/state review in both themes |
| Localization | All supported culture files and current keys inspected | Runtime long-translation pressure not tested | Every new string translated; German/Polish or equivalent long-string browser pass |
| Screen reader, high contrast, touch hardware, 200% zoom | Not directly exercised | Unknown | Manual assistive-technology pass before claiming WCAG 2.2 AA |
| Loading, retry, empty, error, preparing, success | Walkthrough covered empty and Ready; source covered other branches | Not all live states were mutated | Deterministic state fixtures and one browser scenario per state |

## Target interaction design

### Shortest task flow

After installation, the shortest reasonable path is:

1. Select `Import a list` from the enabled add-on detail, or select
   `Lists > Add-on lists > Add add-on list`.
2. Enter the source URL. Confirm or override the resolved add-on and collection
   presentation, then select `Import overview`.
3. Watch bounded discovery. When Ready, the same page automatically exposes
   every committed row and moves the workflow to Review.
4. Address blockers, optionally follow review recommendations, and save only
   if list-level fields are dirty. Row and bulk actions continue to save
   immediately and expose Undo.
5. Select the one visible primary Publish action for the exact saved revision.

This removes the address-bar context switch and the undocumented materializing
Save without weakening the explicit publication boundary accepted by ADR 0075.

### Workflow state and primary action

| State | Status presentation | Primary action | Publish rule |
| --- | --- | --- | --- |
| Source configuration | Add-on resolution and validation inline | `Import overview` | Not available |
| Queued / discovering / verifying | Native progress, item count, current item when present | None; Pause/Cancel are secondary | Blocked: overview is incomplete |
| Needs attention / cancelled | Error plus retained-checkpoint explanation | `Retry import` or `Resume import` | Blocked with reason |
| Ready, clean draft | Included count, blockers, recommendations, optional-detail count | `Publish list` | Enabled when blockers are zero, even when review is recommended or optional details are zero |
| Ready, dirty list settings | `Unsaved changes`; row/bulk persistence described separately | `Save changes` | Blocked until the exact revision is saved |
| Playback preparation required | Exact finite-item count and durable progress | `Prepare publication` before work; none while running | Publish appears when required details are ready; optional details never block |
| Publishing | Atomic revision and rollback status | Disabled `Publishing…` | Duplicate submission blocked |
| Published | Revision and completion status, with links to created surfaces when available | No primary action until new changes exist | Later safe detail enrichment remains non-blocking |

`Need review` becomes `Review recommended` in user-facing copy unless the
underlying rule is promoted to a real blocker. Internal enum names need not
change. The readiness summary is authoritative and links each non-zero count to
the existing filter or affected progress state.

### Semantic structure

The design uses native landmarks and controls. A representative outline is:

```html
<main>
  <nav aria-label="Add-on list progress">
    <ol>
      <li>Source</li>
      <li aria-current="step">Review</li>
      <li>Publish</li>
    </ol>
  </nav>

  <header>
    <h1>List title</h1>
    <button type="button" aria-expanded="false"
            aria-controls="list-description">Show full description</button>
    <p id="list-description">Provider description</p>
  </header>

  <section aria-labelledby="import-status-heading" aria-live="polite">
    <h2 id="import-status-heading">Ready to review</h2>
    <progress max="48" value="48">48 of 48</progress>
  </section>

  <section aria-labelledby="readiness-heading">
    <h2 id="readiness-heading">Publication readiness</h2>
    <p role="status">0 blockers · 48 review recommendations</p>
  </section>

  <section aria-labelledby="items-heading">
    <h2 id="items-heading">Episodes</h2>
    <form><!-- bulk controls with persistent labels --></form>
    <ol class="review-items" start="1">
      <li>
        <article>
          <h3>Provider item title</h3>
          <label><input type="checkbox"> Include in publication</label>
          <dl><!-- first-air date, season/episode, kind, behavior, status --></dl>
          <a href="/media/addon-lists/1/items/1">Review item</a>
        </article>
      </li>
    </ol>
    <nav aria-label="Items pagination"><!-- labelled pager --></nav>
  </section>

  <div aria-live="polite"><!-- saved / unsaved revision status --></div>
  <div><!-- one sticky state-dependent primary action --></div>
</main>
```

Production markup uses localized strings and real IDs. The numeric paths above
are illustrative placeholders, not fixture identities.

### Responsive behavior

- Make the review collection respond to its container, not a named device.
  At wide widths, each ordered-list item uses a compact grid aligned like a
  row. At intermediate widths, lower-priority artwork and extended status move
  into the expandable item details while title, inclusion, first-air date,
  season/episode, state, and Review remain visible.
- Below the width needed by those essential fields, each item becomes a
  one-column labelled card. It does not retain an internal horizontal
  scrollbar. Bulk controls and pagination remain above and below the same
  collection.
- The existing `/media/addon-lists/:id/items/:item_id` route is the SSR/no-JS
  Review target. Hydration may enhance the same control into the existing
  inline editor, but the link must remain meaningful before hydration.
- Keep the state-dependent primary action sticky within the main scroll
  container on narrow screens. It must not obscure focused content or the last
  row; reserve equivalent block space at the end of the page.
- Clamp a long description to four lines on narrow layouts and a restrained
  measure on intermediate layouts. Expansion is user controlled and respects
  reduced motion.
- Primary actions prefer a 44 by 44 CSS-pixel target. Every target meets WCAG
  2.2's 24 by 24 minimum or documented spacing exception.

## Component and catalog decisions

- Reuse the existing `Modal` only where a modal decision is actually needed;
  do not introduce a drawer solely to hide the row actions. The item-detail
  route and inline editor already provide a better progressive-enhancement
  boundary.
- Reuse `Button`, `LinkButton`, and `IconButton` where their current semantics
  fit. A submit action must remain a native submit button; extend the primitive
  only if it can own submit semantics without page-specific leakage.
- Keep pagination page-local for this slice. The catalog already marks shared
  pagination as a candidate with unresolved variants; consolidation is not a
  prerequisite for fixing this flow.
- Treat the workflow stepper and responsive review collection as page
  composites, not new shared primitives. Reassess only after a second route
  demonstrates the same purpose, state model, responsive behavior, and
  fallback requirements.
- At implementation time, add a `Navigation drawer` catalog entry. It is a
  global, accessibility-sensitive primitive whose keyboard, inert, focus
  return, responsive, and no-JS contracts deserve explicit evidence even
  though it has one shell consumer. Do not mark the corrected contract proven
  until focused browser evidence exists.

No component-catalog files are changed by this audit because component-library
maintenance was not requested.

## Implementation map

### 1. Freeze the state and focus regressions

- Add a focused browser spec for the Ready transition, mobile keyboard drawer,
  dirty revision, readiness summary, and 320-pixel review collection before
  restructuring markup.
- Add a JavaScript-disabled path for create, save, and Publish so the fallback
  contract is executable rather than inferred from source.
- Use deterministic provider/import fixtures; do not depend on mutable public
  content counts.

### 2. Repair navigation and contextual onboarding

- In `admin/layout/nav.rs`, move Add-on lists from `Media sources` to `Lists`.
- In `admin/layout/mod.rs` and `layout.module.css`, replace the checkbox/label
  toggle with a keyboard-operable disclosure, make the closed drawer inert,
  and implement open/close focus management without changing desktop
  navigation behavior.
- In `admin/pages/addons.rs`, expose `Import a list` only for an enabled package
  with a supported `media-list.list.vN` capability. Pass its ID as optional
  context to the add route while keeping URL resolution visible and editable.
- Keep the trust and enable confirmations, but give each a localized,
  state-specific title, result, and permission explanation.

### 3. Make workflow and publication readiness authoritative

- Add one application-owned publish-readiness projection containing included
  count, blocking reasons, review recommendations, required playback-detail
  count/state, optional detail counts, saved revision, and publication state.
  Reuse existing repository aggregates where possible; no schema migration is
  expected.
- Return that projection through `admin/server_fns/addon_media_lists.rs` so
  the button state and explanatory copy cannot disagree with the application
  command.
- In `admin/pages/media/addon_lists.rs`, render the three-step state model,
  reload committed rows when overview Ready is observed, track list-level dirty
  state, and expose one state-dependent primary action. Keep row/bulk edits
  immediately persisted and undoable.
- Replace the toast-only `publish again after details` instruction with a
  persistent Preparing state. Do not let optional extended metadata block
  first publication, as required by ADR 0079.
- In `api/addon_media_lists.rs`, make add/save/publish POST fallbacks call the
  same application commands and preserve current unsubmitted state. Redirect
  native add to the created editor and surface bounded route errors inline.

### 4. Replace the table with the responsive review collection

- Refactor the current `item_row_view` and table composition into one keyed,
  semantic ordered list. Preserve provider-backed identity, provider order,
  direct season/episode editing, inclusion, per-item kind and liveness,
  numbering preview, replacement, detail fetch, metadata editing, filtering,
  paging, and one-step undo.
- Use the CSS module for wide grid, intermediate priority collapse, narrow
  cards, focus visibility, target size, dark/light states, and reduced motion.
- Keep the item route as the non-hydrated Review destination. Do not render two
  active form-control copies merely to switch layouts.
- Collapse long descriptions through a real button and remove empty elapsed
  status fragments from both visible and live-region output.

### 5. Finish copy, package, documentation, and progress

- Add or update every string in all supported culture files. Keep `i18n.rs` as
  routing/helper glue.
- Fix the official Beeld & Geluid package's Windows quoting and add the paired
  POSIX/Windows status-message regression; keep host status messages opaque.
- Align the list empty state, add-on trust copy, progress copy, and
  blocker/recommendation vocabulary.
- If implementation changes a reusable primitive, update
  `docs/reference/ui/components/` and `component-inventory.md` with actual
  source and browser evidence.
- Update the changelog, this parity debt, milestone `amm-i-01`, hydration route
  evidence, and the session log when the work is completed. No new ADR is
  needed if ADR 0079's optional-detail and explicit-publication semantics stay
  unchanged; create one before changing those decisions.

## Verification and acceptance

Focused automated proof must cover:

- pure readiness classification: incomplete overview, no included items,
  advisory review only, optional details pending, required finite details
  preparing, dirty settings, clean draft, publishing, and published;
- a Ready transition that changes the review count from zero to the committed
  item total without Save, navigation, reload, or duplicate rows;
- dirty list settings making Save primary, followed by the exact saved revision
  becoming publishable;
- advisory `Review recommended` and zero optional details leaving Publish
  enabled, while every real blocker supplies a visible reason and destination;
- 320, 390, intermediate, and wide layouts in dark and light themes with no
  horizontal item-editor scrolling, clipped actions, or page overflow;
- keyboard-only drawer open/close/focus return, review filtering, paging,
  numbering preview, item editing, Undo, Save, Publish, and confirmation;
- JavaScript-disabled import, list-level save, item Review navigation, and
  explicit Publish against the same application behavior as the hydrated path;
- long localized labels/descriptions, 200% zoom, reduced motion, target size,
  focus not obscured by the sticky action, and status/error announcements;
- add-on detail capability gating and exact unescaped package ready copy;
- zero hydration marker errors with a matched SSR/WASM bundle.

Use the narrow Rust/web inner loop first, then run the focused Playwright spec,
`cargo xtask hydration-lint -p ersatzrs-web`, and
`cargo xtask progress check`. Run `cargo xtask parity-check web` once before
closing the parity debt. The normal final closeout remains required when the
implementation slice is finalized; it is not required for this plan-only
change.

## Assumptions, research gaps, and residual risk

- ADR 0079 settles the central product question: a complete overview may
  publish while optional details are absent. User-facing review advice is
  therefore non-blocking unless application validation identifies a separate
  required condition.
- The exact rule that currently produces `Need review` still needs a code-level
  inventory before renaming its presentation. Any member that protects
  playback, data integrity, or informed choice belongs in Blockers instead.
- Screen-reader behavior, Windows High Contrast, touch hardware, automated
  contrast ratios, and 200% zoom were not directly tested during this audit.
  They remain acceptance work, not conformance claims.
- The walkthrough did not exercise publication rollback, generated media,
  playlists, or playback. This plan must preserve those ADR 0075 boundaries and
  must not use UX work to weaken atomic publication or health reporting.
- Moving the sidebar link to `Lists` is an evidence-backed design decision from
  one observed operator path and the feature name, not broad user research.
  If future operator research rejects it, keep the contextual add-on CTA and
  record the alternative information-architecture decision explicitly.
- The current live/source route link under `Media sources` means the original
  complete absence is no longer reproducible. Acceptance should test the chosen
  hierarchy, not claim that the current build has no link.

## Related contracts and plans

- [Review add-on metadata before publication](../docs/adr/0075-review-addon-metadata-before-publication.md)
- [Discover add-on lists before deferring item details](../docs/adr/0079-make-addon-list-imports-resumable.md)
- [Add-on media-list contract reference](../docs/reference/addon-media-lists.md)
- [Local replacement UX](addon-list-replacement-ux.md)
- [Per-item classification and projections](addon-list-editor-item-classification.md)
