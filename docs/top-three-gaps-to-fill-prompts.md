<!--
generated: 2026-06-13
last_processed_commit: e30b6db4e3ddf1a33bd52d16031fca8f13eed0ab
parity_baseline_commit: 0d301df5e8512fdfccf3a611188d27ab1690ee31
generator: manual maintainer regeneration from docs/automation/gap-prompt-rules.md
rules: docs/automation/gap-prompt-rules.md
-->

# Top three gaps to fill - goal prompts

Three paste-and-run goal prompts, each closing **one** ErsatzTV parity gap.
Auto-prioritized from `docs/progress` with this manual regeneration biased by
the maintainer request toward **full Lucene search parity next** after the
smart-collection admin UI fixes. Regenerated daily by the
[progress routine](automation/daily-progress-routine.md); see the
[rules](automation/gap-prompt-rules.md). For the live gap list run
`cargo xtask progress gaps`.

**Parity baseline:** gaps are measured against ErsatzTV commit
[`0d301df`](https://github.com/ErsatzTV/legacy/commit/0d301df5e8512fdfccf3a611188d27ab1690ee31)
(`remove external dependencies (bugsnag, trakt)`). Upstream work after that
commit is out of scope until the pin in
[`automation/parity-baseline.md`](automation/parity-baseline.md) is advanced.

Ranking this run: the collections prompt is removed because the
smart-collection admin UI fixes are already in the active collection parity
slice; the remaining smart-collection blocker is the shared search-index
Lucene contract. Full Lucene smart-collection query parity is now prompt 1,
followed by `/media/remote/streams` because it is an advertised drawer route
that still 404s. The third slot returns to the FFmpeg profile editor's
remaining pinned-baseline option/validation debts.

---

## 1 - Full Lucene smart-collection search parity (slug `search-index`)

Fix the next ErsatzTV parity gap in ErsatzRS: full documented Lucene
smart-collection search parity (slug
`search-index`, debt `lucene-smart-collection-query-language`).

Use the repo workflow from `AGENTS.md` plus `skills/parity-slice.md`.
The goal is fully functional ErsatzTV parity without stopgaps.

Current `ParityDebt` from `docs/progress/parity/search-index.toml`: "Search
does not yet support the full documented ErsatzTV Lucene field/query contract
used by smart collections." The official contract is
`https://ersatztv.org/docs/search/`; the pinned implementation source is
`$ERSATZTV_SRC/ErsatzTV.Infrastructure/Search/{LuceneSearchIndex.cs,SearchQueryParser.cs,CustomMultiFieldQueryParser.cs}`.
Live profile proof on 2026-06-13: the `Music Videos` smart collection query
with `library_id:3`, `type:"music_video"`, and `NOT artist:"..."` clauses
should return 377 pruned library-3 music videos by direct SQL, but `/search`
returned 699 and included excluded artists such as Alice in Chains.

ErsatzTV source: `$ERSATZTV_SRC/ErsatzTV.Infrastructure/Search/`,
the search docs above, and the smart-collection query call sites. ErsatzRS
target: `crates/ersatzrs-core/src/search.rs`,
`crates/ersatzrs-infra/src/repositories/scheduling.rs`, search-related admin
route/server-function paths, and focused tests for parser/evaluator/document
hydration.

Before editing:
1. Read the official ErsatzTV search docs and pinned C# search parser/index
   sources before changing Rust.
2. Inspect the current ErsatzRS parser/evaluator, search document hydration,
   scheduling repository search source, smart-collection expansion behavior,
   provider-guid fallback handling, and tests.
3. Build a Source Map for every documented search field and operator, marking
   supported, unsupported, and partially hydrated fields.
4. Identify remaining `ParityDebt`; it must stay visible, not hidden in prose.

Implement the coherent end-to-end slice: support all documented ErsatzTV
fields and query helpers used by smart collections, including
`library_name`, `library_folder_id`, `collection`, `content_rating`,
`network`, `show_network`, `show_content_rating`, `style`, `mood`, `album`,
`artist`, `album_artist`, `video_bit_depth`, `video_dynamic_range`,
`added_date`, relative date helpers (`released_onthisday`,
`released_inthelast`, `released_notinthelast`, `added_inthelast`,
`added_notinthelast`), and nested `smart_collection:"name"` expansion/cycle
handling. Preserve ErsatzTV's provider-guid source semantics and boolean,
phrase, wildcard, fuzzy, numeric, and range behavior.

Add focused parser/evaluator tests, document-hydration tests, smart-collection
expansion/cycle tests, and profile-DB regression proof for `Music Videos`
returning the expected 377 results while excluding the listed artists. Use a
copy of the maintainer profile database for runtime proof; do not mutate the
real profile DB.

Verify with focused core/infra search tests, profile-copy runtime proof,
Chromium and Firefox browser proof for `/media/smart-collections` Search and
`/search?query=...`, relevant clippy passes, and `cargo xtask progress check`.
Update `docs/progress/parity/search-index.toml` and
`docs/progress/parity/collections-smart-collections.toml` only when the
evidence supports the new status. Report the Source Map, behavior implemented,
remaining `ParityDebt`, verification run, profile-result counts, and changed
files.
Remember that ErsatzTV could have a less than optimal solution. If you have a
better way, please create an ADR, but only after careful review in depth.
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

Verify with focused `ersatzrs-web` remote-stream tests,
`cargo xtask hydration-lint -p ersatzrs-web`, the added or updated Playwright
spec via `npx playwright test ...`, `cargo xtask parity-check web`, and
`cargo xtask progress check`. Update
`docs/progress/parity/remote-streams-browser.toml`, `docs/progress/routes.toml`,
and `docs/progress/hydration.toml` only when the evidence supports the new
status. Report the Source Map, Visual Source Map, behavior implemented,
remaining `ParityDebt`, verification run, and changed files.
Remember that ErsatzTV could have a less than optimal solution. If you have a
better way, please create an ADR, but only after careful review in depth.
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
Remember that ErsatzTV could have a less than optimal solution. If you have a
better way, please create an ADR, but only after careful review in depth.
update changelog, docs and /docs/top-three-gaps-to-fill-prompts.md when done.
