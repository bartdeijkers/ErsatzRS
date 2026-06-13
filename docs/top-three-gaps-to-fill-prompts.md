<!--
generated: 2026-06-13
last_processed_commit: 80e9524220c6f134f05703bf49882cc4ea42eaed
parity_baseline_commit: 0d301df5e8512fdfccf3a611188d27ab1690ee31
generator: manual maintainer regeneration from docs/automation/gap-prompt-rules.md
rules: docs/automation/gap-prompt-rules.md
-->

# Top three gaps to fill - goal prompts

Three paste-and-run goal prompts, each closing **one** ErsatzTV parity gap.
Auto-prioritized from `docs/progress` with this manual regeneration biased by
the maintainer request toward **admin UI problems** after the 2026-06-13 HLS
segmenter runtime debt was closed. Regenerated daily by the
[progress routine](automation/daily-progress-routine.md); see the
[rules](automation/gap-prompt-rules.md). For the live gap list run
`cargo xtask progress gaps`.

**Parity baseline:** gaps are measured against ErsatzTV commit
[`0d301df`](https://github.com/ErsatzTV/legacy/commit/0d301df5e8512fdfccf3a611188d27ab1690ee31)
(`remove external dependencies (bugsnag, trakt)`). Upstream work after that
commit is out of scope until the pin in
[`automation/parity-baseline.md`](automation/parity-baseline.md) is advanced.

Ranking this run: the previous HLS segmenter prompt is removed because
`parallel-hls-playback-client-freeze` is closed by ADR 0015 and the 2026-06-13
TSDuck proof. The next slots stay admin-focused: `/media/collections/add`
does not create a collection successfully, `/media/remote/streams` is an
advertised drawer route that still 404s, and the FFmpeg profile editor is
missing two pinned-baseline option/validation behaviors.

---

## 1 - Collections / Smart Collections: make `/media/collections/add` work and restore visual parity (slug `collections-smart-collections`)

Fix the next ErsatzTV parity gap in ErsatzRS: Collections / Smart Collections
for `/media/collections/add` and the manual/smart collection pages (slug
`collections-smart-collections`, debts `collections-add-page-save` and
`collections-visual-parity`).

Use the repo workflow from `AGENTS.md` plus `skills/parity-slice.md`.
The goal is fully functional ErsatzTV parity without stopgaps.

Current `ParityDebt` from
`docs/progress/parity/collections-smart-collections.toml`: "the
`/media/collections/add` page does not function (creating a new collection via
the add page fails), and the collections pages' HTML/CSS does not match
ErsatzTV's markup/styling (no visual parity with the MudBlazor layout at the
pinned baseline)."

ErsatzTV source: `ErsatzTV/Pages/Collections*.razor`,
`ErsatzTV/Pages/SmartCollections.razor`, and
`ErsatzTV/Pages/SmartCollectionEditor.razor`. ErsatzRS target:
`crates/ersatzrs-web/src/admin/pages/media/{collections,collection_detail,smart_collections}.rs`,
the matching server functions/API fallbacks, and
`crates/ersatzrs-web/src/admin/languages/`.

Before editing:
1. Read the pinned ErsatzTV manual-collection list, add/edit, collection-items,
   smart-collection list, and smart-collection editor sources.
2. Inspect current ErsatzRS routes, server functions, raw HTML POST fallbacks,
   translations, tests, CSS modules, and `docs/progress/routes.toml` /
   `docs/progress/hydration.toml`.
3. Build a Source Map and a Visual Source Map for `/media/collections`,
   `/media/collections/add`, `/media/collections/:id`,
   `/media/collections/:id/edit`, and `/media/smart-collections`.
4. Identify remaining `ParityDebt`; it must stay visible, not hidden in prose.

Implement the coherent end-to-end slice: make `/media/collections/add` create
a new collection successfully through both hydrated interaction and no-JS HTML
POST fallback, preserve ErsatzTV validation and redirect behavior, and keep
collection mutation side effects aligned with the existing `BuildPlayout`
refresh rules. Then bring the manual/smart collection markup and CSS back to
the pinned MudBlazor layout: action bars, containers, headings/dividers,
search/table/pager geometry, icon action columns, editor rows, detail-page
bulk actions, custom playback-order controls, and responsive/mobile behavior.
Localize any new or changed visible strings in every supported culture.

Add focused server-function/API regressions for the add/save path and browser
coverage for the add route plus at least one representative list/detail visual
contract. Preserve SSR/browser-safe fallbacks and follow `TEST.md` hydration
rules.

Visual verification: serve via `./run.sh serve` after rebuilding Leptos assets,
capture same-viewport screenshots against the ErsatzTV reference for the
listed routes, and compare geometry, action visibility, validation states, and
mobile layout rather than DOM presence alone.

Verify with focused `ersatzrs-web` collection tests, `cargo xtask hydration-lint -p ersatzrs-web`,
the added or updated Playwright spec via `npx playwright test ...`,
`cargo xtask parity-check web`, and `cargo xtask progress check`. Report the
Source Map, Visual Source Map, behavior implemented, remaining `ParityDebt`,
verification run, and changed files.
update changelog, docs and /docs/top-three-gaps-to-fill-prompts.md when done.

---

## 2 - Remote streams browser: replace the advertised 404 with the ErsatzTV admin UI (slug `remote-streams-browser`)

Fix the next ErsatzTV parity gap in ErsatzRS: Remote streams browser for
`/media/remote/streams` (slug `remote-streams-browser`).

Use the repo workflow from `AGENTS.md` plus `skills/parity-slice.md`.
The goal is fully functional ErsatzTV parity without stopgaps.

Current gap from `docs/progress/parity/remote-streams-browser.toml`: "Not
started: nav link advertised in the drawer 404s today. Remote-stream domain
entities exist; no UI yet." `docs/progress/routes.toml` also records
`/media/remote/streams` as `routed = false` with backing "Remote-stream
entities exist; no UI yet."

ErsatzTV source: `ErsatzTV/Pages/RemoteStreams*.razor`. ErsatzRS target:
`crates/ersatzrs-web/src/admin/pages/media/`, the matching server functions/API
fallbacks, route registration, route/hydration progress docs, and
`crates/ersatzrs-web/src/admin/languages/`.

Before editing:
1. Read the pinned ErsatzTV remote-stream list/editor Razor sources and any
   handlers/validators they call for URL, name, grouping, and save/delete
   behavior.
2. Inspect the current ErsatzRS remote-stream domain/repository shape, media
   scanner persistence, admin nav route registration, media-page conventions,
   raw POST fallback patterns, translations, and tests.
3. Build a Source Map and a Visual Source Map for ErsatzTV's remote-stream
   list/editor routes and the local `/media/remote/streams` route family.
4. Identify remaining `ParityDebt`; it must stay visible, not hidden in prose.

Implement the coherent end-to-end slice: route `/media/remote/streams` and the
needed add/edit/delete surfaces, backed by real persisted remote-stream rows
and ErsatzTV-equivalent fields, validation, labels, actions, sorting/search or
paging behavior, and side effects. Preserve SSR/browser-safe fallbacks for
create/save/delete, wire the route into admin navigation without 404s, and add
translations for every supported culture. If the current domain model lacks a
field ErsatzTV exposes, port the persistence/server-function path or record a
structured `ParityDebt`; do not silently omit it.

Add focused server-function/API tests plus browser coverage for list, add,
edit, delete/cancel, validation failure, and no-JS fallback behavior.

Visual verification: serve via `./run.sh serve` after rebuilding Leptos assets,
capture same-viewport screenshots against the ErsatzTV reference for list and
editor states, and compare action-bar geometry, table/form layout, validation
messages, empty state, and mobile behavior.

Verify with focused `ersatzrs-web` remote-stream tests, `cargo xtask hydration-lint -p ersatzrs-web`,
the added or updated Playwright spec via `npx playwright test ...`,
`cargo xtask parity-check web`, and `cargo xtask progress check`. Update
`docs/progress/parity/remote-streams-browser.toml`, `docs/progress/routes.toml`,
and `docs/progress/hydration.toml` only when the evidence supports the new
status. Report the Source Map, Visual Source Map, behavior implemented,
remaining `ParityDebt`, verification run, and changed files.
update changelog, docs and /docs/top-three-gaps-to-fill-prompts.md when done.

---

## 3 - FFmpeg Profile editor: restore missing H.264 profile options and NVENC validation (slug `ffmpeg-profile-editor`)

Fix the next ErsatzTV parity gap in ErsatzRS: FFmpeg Profile editor for the
remaining profile-option and validation-warning behavior (slug
`ffmpeg-profile-editor`, debts `h264-profile-options` and
`nvenc-mpeg2-validation-warning`).

Use the repo workflow from `AGENTS.md` plus `skills/parity-slice.md`.
The goal is fully functional ErsatzTV parity without stopgaps.

Current `ParityDebt` from
`docs/progress/parity/ffmpeg-profile-editor.toml`: "when the `/ffmpeg` profile
editor's format is `h264`, its `profile` field does not expose the upstream
enum options `main`, `high`, and `high444p`; when `Nvenc` hardware
acceleration is selected and the format is changed to `mpeg-2`, the format
field does not show the upstream red validation line/warning that NVENC only
supports `h264`, `hevc`, and `av1`."

ErsatzTV source: `ErsatzTV/Pages/FFmpegProfile*.razor`, the matching profile
view models, validators, and localization resources. ErsatzRS target:
`crates/ersatzrs-web/src/admin/pages/ffmpeg_profiles/`,
`crates/ersatzrs-web/src/admin/languages/`, and the profile persistence/server
function paths touched by editor save/validation.

Before editing:
1. Read the pinned ErsatzTV FFmpeg profile list/editor Razor sources, view
   model, validator, enum option sources, and localization text for profile
   format/hardware validation.
2. Inspect the current ErsatzRS FFmpeg profile editor islands, SSR fallback,
   option lists, translations, tests, and profile repository save path.
3. Build a Source Map and a Visual Source Map for `/ffmpeg`,
   `/ffmpeg/profiles/add`, and `/ffmpeg/profiles/:id`.
4. Identify remaining `ParityDebt`; it must stay visible, not hidden in prose.

Implement the coherent end-to-end slice: expose ErsatzTV's `h264` profile
options `main`, `high`, and `high444p` in the editor when the selected video
format is H.264, persist/load the selected value correctly, and show the
upstream unsupported-format validation warning when `Nvenc` is selected with
`mpeg-2`. Preserve hydrated and no-JS fallback behavior, localize any changed
visible strings in every supported culture, and keep existing startup-report
hardware availability behavior intact.

Add focused unit/server-function regressions for option generation,
validation, save/load, and any raw HTML fallback path affected by the change.
Add browser coverage for the editor state transitions that show the H.264
profile options and the NVENC MPEG-2 validation message.

Verify with focused `ersatzrs-web` FFmpeg profile tests,
`cargo xtask hydration-lint -p ersatzrs-web`, the added browser spec, and
`cargo xtask progress check`. Update
`docs/progress/parity/ffmpeg-profile-editor.toml` only when the evidence
supports the new status. Report the Source Map, Visual Source Map, behavior
implemented, remaining `ParityDebt`, verification run, and changed files.
update changelog, docs and /docs/top-three-gaps-to-fill-prompts.md when done.
