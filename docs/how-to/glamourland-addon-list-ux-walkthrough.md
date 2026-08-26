# Evaluate the Glamourland add-on-list import UX

This walkthrough records a visual Windows dogfood session of the Beeld &
Geluid add-on and the Glamourland add-on-list import. It is intended as input
for a later UX/UI expert review, not as an automated acceptance test.

Observed on 2026-08-26. The successful scenario used an isolated temporary
SQLite database, temporary application state and data directories, and a
loopback-only web listener. No publication was attempted.

## Outcome

The Beeld & Geluid add-on installed and enabled successfully through the admin
UI. ErsatzRS resolved the public Glamourland URL to that add-on, imported 48
items, and saved them as an add-on list named `GLAMOURLAND`.

The overview import initially reported `Ready to review: 48 / 48`, while the
episode table still reported `0 Discovered` and displayed no rows. Clicking
`Save add-on list` was required before the 48 rows appeared. The UI did not
explain that this save was the step that materialized the imported overview.

All 48 rendered rows were in ascending provider order, from 1990-10-06 through
2000-02-25. No exact duplicate based on the rendered title plus first-air date
was present. Every row remained marked `Need review` and `No replacement`, and
the progress card remained at `0 / 48 Details complete` and `0 Details
skipped`.

## Test identity

| Evidence | Value |
| --- | --- |
| Source checkout | Current uncommitted checkout based on `42c00ac1230076ec63c144d94eae80d414f544c9` |
| Native web build | Consistent SSR and WASM build produced with cargo-leptos 0.3.6 |
| Native executable SHA-256 | `F2C08B9E3F1347DEA21CFA3466DFA611EC517AAEC520CABCA659F6CD4C37668E` |
| Hydration JavaScript SHA-256 | `4CCCDABA06EDA7B322F67ADBC29BEA4773545DDB23D9B0184C16B03A291D0FC4` |
| Hydration WASM SHA-256 | `44860F000344BA0A7D17BF2C9FB412C2D717FCC7BCAFAA2062FB3B2FB617DC35` |
| CSS SHA-256 | `0DC76704B80DB91C139DA0EC1BDDB1EDB1D7CFF945542D58E2176F822594D757` |
| FFmpeg and ffprobe | Version `n9.0-20260805`, loaded from the requested sibling `ErsatzRS-build` directory |
| FFmpeg SHA-256 | `A0BFE4880A62E7777F98A1602245E6E1AE812A86E54C011B0938A6A840FEAF0E` |
| FFprobe SHA-256 | `92238CDD8414F95CA23D8E58DFDC5625DEBF1B8A6C478AF32B6B0585509CC958` |
| Runtime isolation | `<temporary-root>\ErsatzRS.db`, temporary state and data directories, `127.0.0.1:8409` |
| Desktop viewport | 1278 by 1304 CSS pixels |
| Mobile viewport | 390 by 844 CSS pixels; the usable layout width was 375 pixels because of the browser scrollbar |

The successful server bootstrap identified the temporary database, reported
the requested FFmpeg and ffprobe paths with `source=override`, listened only on
`127.0.0.1:8409`, and returned HTTP 200 from `/healthz`.

## Safety incident in a rejected setup attempt

An initial attempt invoked the repository web-bundle gate through WSL while
supplying native Windows environment variables. Those variables did not reach
the native process. Contrary to the intended isolation, that rejected process
opened the default profile database, selected the managed development FFmpeg
instead of the requested build, and listened on the default all-interface
address. Its scheduler rebuilt five existing playouts during the approximately
one-minute attempt.

The gate did not observe `/healthz` from its shell and stopped the process. No
listener or process from that attempt remained afterward. No database repair,
rollback, or deletion was performed, so the five playout rebuilds must be
treated as a possible profile-data mutation. The successful workflow described
below was restarted with native PowerShell environment propagation and used
only the temporary database.

A second rejected candidate combined a newly built SSR executable with stale
WASM. Playwright observed a `failed_to_cast_marker_node` hydration panic on the
add-ons page. No UI mutation was performed with that candidate. It was stopped,
the native cargo-leptos tool was installed, and SSR plus WASM were rebuilt
together before the successful scenario began.

## Hydration and server-boundary map

| Route | Rendering and interaction | Evidence used in this run |
| --- | --- | --- |
| `/settings/addons` | SSR page with hydrated add-on controls | Browse, install, confirmation, enable, and status updates all worked through visible controls |
| `/media/addon-lists/add` | SSR page with a hydrated add-list form | URL input resolved the provider and collection type; `Import` created list 1 |
| `/media/addon-lists/1` | SSR page with the hydrated review editor and import presentation | Progress, save, pagination, bulk controls, numbering preview, and responsive layout were exercised |

For the accepted build, `window.__ersatzrs_hydrated` was `true`; the page had
no not-found marker; and the browser console contained zero errors and zero
warnings. The observed add-on and add-on-list server-function requests returned
HTTP 200. No direct database write or server-function shortcut was used from
Playwright.

## Visual workflow

### 1. Install the add-on

1. Open `/settings/addons` in the admin UI.
2. Select `Browse add-ons`.
3. Open `Beeld & Geluid` version 0.11.1. The catalog marks it `Verified`,
   available, and sourced from the official repository.
4. Select `Install`.
5. Review the trust and permission confirmation, then select `Confirm`.
6. Observe that installation completes in the `Disabled` state.
7. Select `Enable`, review the second confirmation, and select `Confirm`.
8. Observe the final `Verified` and `Enabled` states and the ready status.

The ready message rendered as `Beeld ^& Geluid is ready.`. The caret before the
ampersand looks like an escaping defect and should be evaluated as user-facing
copy.

### 2. Locate the add-on-list page

1. Expand `Lists`, `Media`, and `Settings` in the desktop sidebar.
2. Look for an add-on-list destination.

No `Add-on Lists` navigation item was present. The route could not be
discovered through the visible navigation, even after expanding all plausible
groups. Continue by entering `/media/addon-lists` in the browser address bar.

The empty page displayed `No add-on lists yet.` and an `Add add-on list` action.
The action was positioned above and to the left of the primary page heading,
which made it read more like a floating back/action control than the principal
empty-state action.

### 3. Configure the Glamourland import

1. Select `Add add-on list`.
2. Enter:

   `https://schatkamer.beeldengeluid.nl/serie/2101608030021531731/glamourland`

3. Observe that URL resolution enables the form, chooses `TV Shows`, and later
   identifies `Beeld & Geluid` as the selected add-on.
4. Leave the resolved/default settings unchanged:

   - add-on: `Determine from the URL`;
   - collection type: `TV Shows`;
   - automatic refresh: enabled once each day;
   - storage folder: empty and derived later;
   - create playlist in original order: disabled;
   - load item details automatically: enabled.

5. Select `Import`.

The form explains the two-step `Import` and `Publish` process and says that the
overview is imported first. It does not explain that another save action will
be needed between a completed overview and a populated review table.

### 4. Wait for the overview and save it

1. Observe the progress card transition from `Checking overview: 0` to `Ready
   to review: 48` and `48 / 48 Discovered` in about ten seconds.
2. At this point, observe the contradictory episode section: `0 Discovered`,
   `0 Need review`, and `No items to display.`
3. Select `Save add-on list` at the top of the page.
4. Observe navigation from `/media/addon-lists/1?import=1` to
   `/media/addon-lists/1`.
5. Confirm that the title, description, and derived storage folder
   `GLAMOURLAND` now appear and that the episode table contains 48 rows.

The completed progress card still had an empty value after `Elapsed time:`.
This removes a useful confirmation of whether the provider operation was fast,
stalled, or resumed.

### 5. Inspect the imported rows

1. Set `Rows per page` to `All`.
2. Verify 48 rendered episode rows and a range of `1-48 of 48`.
3. Compare the rendered title and first-air date pairs. No exact pair occurred
   more than once.
4. Observe the initial row state:

   - included in publication: checked;
   - season and episode: empty;
   - media kind: `Leave empty to auto-detect`;
   - playback behavior: `Use provider value · Finite media`;
   - status: `Need review` and `No replacement`;
   - per-row actions: fetch details, replace, edit metadata, and clear.

The page enabled `Publish` even though all 48 rows needed review and no details
were complete. A reviewer should determine whether `Need review` is advisory or
blocking and whether the call to action communicates that distinction.

The table used an internal horizontal scroll area. At the desktop viewport its
content was approximately 1741 pixels wide inside a 963-pixel table wrapper.
The scrollbar was below the 25 visible rows, so the season fields, status, and
right-side row actions required a long vertical trip before horizontal
navigation was available.

### 6. Preview automatic season numbering

1. Select `Number episodes` without applying the result.
2. Observe the default preview `48 Episodes · S01E01–S01E48`.
3. Enable `Auto-detect seasons from first-air dates`.
4. Observe the newly revealed `Minimum gap between seasons (days)` spinbox,
   whose default is 90.
5. Observe that the preview updates the rows immediately. With 90 days, the
   last rendered item previews as `S07E02`.
6. Disable auto-detection and collapse the panel. Do not select `Apply`.

The numbering panel stacked cleanly at mobile width, and its immediate preview
made the effect of the 90-day threshold visible before mutation.

### 7. Inspect the mobile layout

1. Resize the viewport to 390 by 844 CSS pixels.
2. Observe that the sidebar moves off-canvas and the top navigation retains a
   visible menu toggle.
3. Observe that metadata controls, the numbering panel, and bulk-edit controls
   become a single column. `Undo last change` and `Apply (48)` become full-width
   controls.
4. Observe that the episode table remains approximately 1741 pixels wide
   inside a 311-pixel region. With all 48 rows displayed, its horizontal
   scrollbar is after more than 4,000 pixels of table content.
5. Restore `Rows per page` to 25, restore the desktop viewport, and return to
   the top of the page.

The stacked bulk editor is usable on mobile, but the row-level editor is not:
most fields and all right-side actions are outside the initial viewport, while
the horizontal scroll affordance is only reachable at the bottom of the long
table. The full series description also consumed approximately 561 vertical
pixels before the import settings and episode controls.

## UX review prompts

Use the observations above to evaluate these questions:

- Where should `Add-on Lists` live in the sidebar, and should the add-on detail
  page link directly to lists provided by that add-on?
- Should a completed overview populate the table automatically, or should the
  progress card explicitly change its primary action to `Save imported items`?
- Can the distinction between `Save add-on list` and `Publish` be expressed as
  one coherent, persistent stepper with a single primary action per state?
- Should `Publish` remain enabled while all rows need review and technical
  details are still `0 / 48`?
- Can row editing use mobile cards, a selected-row drawer, sticky columns, or a
  top-and-bottom horizontal scrollbar instead of a 1741-pixel table?
- Should the long series description be collapsed on small screens?
- Should elapsed time show a duration, disappear, or report an explicit
  unavailable value when the import is complete?
- Can the add-on installation and enable confirmations state the resulting
  status more directly and remove the stray caret from the ready message?

## Scope limits and retained state

This session stopped after overview import and save. It did not fetch a single
item's details manually, apply season numbering, change bulk values, publish
the list, create media, or test playback. Consequently it does not validate
finite-media durations, multi-part stream concatenation, detail enrichment, or
publication rollback behavior.

At the end of the walkthrough, the accepted ErsatzRS process remained running
on `127.0.0.1:8409` with the temporary database. The browser was left on the
saved Glamourland list at the desktop viewport with 25 rows per page. The
temporary data can be discarded after the UX review; it is not the profile
database affected by the rejected setup attempt described above.
