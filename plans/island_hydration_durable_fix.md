# Durable fix: island hydration marker-mismatch regression

## Context

ErsatzRS's admin UI repeatedly regressed the same class of bug: interactive
controls (delete/stop on `/channels`, refresh/row-select on `/playouts`,
dialogs, forms) silently stopped working while plain `<a>` links kept working.
SSR and unit tests never caught it; it only showed up in a real browser as a
fatal `expected a marker node` hydration panic that aborts `hydrate_islands()`
before any handler attaches.

**Root cause (confirmed via git blame).** The broken pattern was codified in
commit `b6f4e8b` ("eliminate failed_to_cast_marker_node across all admin
islands") and enshrined as **TEST.md hydration rule 3**, which instructs:
*"have the island own a `Resource::new_blocking` internally."* This is exactly
backwards for islands that render a **dynamic node list** (table rows, card
grids, `<option>` lists, the `LetterBar`): an `#[island]` that owns an
SSR-participating `Resource` + `<Suspense>` produces a different DOM marker
shape on the client hydration walk than SSR emitted → fatal marker mismatch.
Islands that render only static markup with reactive *attributes* survive,
which is why the failure looked intermittent and slipped into shipped releases.
The `hydration-lint` rule-2 message and the per-page index both still actively
recommend the broken approach.

**The proven fix** (commits `885a5c1` → `f75913a` → `17d9c80` →
`bb8251a` → `1bebd24` → `9833488`, by Codex in the last ~7h) inverts rule 3:

- The **page wrapper** (a plain `#[component]`, no island boundary) owns
  `Resource::new_blocking` + `<Suspense>` and passes the **resolved concrete
  data as props** into the island.
- The **island renders deterministically from those props** — no `<Suspense>`,
  no SSR `Resource` inside it → SSR and hydration DOM shapes match.
- Post-hydration refresh uses a **client-only `LocalResource`** mirrored into
  `RwSignal`s via `Effect`. Reference: `media/multi_select_browser.rs`,
  `media/collections.rs`.

**Current state of the regression.** The per-page conversion is essentially
**done** — every dynamic-list island has been converted. What remains is the
*durable* work so this can never come back: the proven pattern is hand-copied
across ~25 files with zero shared abstraction, the documented contract still
prescribes the broken approach, and nothing in CI stops the next agent from
reintroducing an island-owned `Resource`.

**Chosen invariant (decided with maintainer): one strict rule —
*an `#[island]` never owns an SSR-participating `Resource`.*** All SSR data
loads in the wrapper and arrives as props; islands use only `LocalResource`
for post-hydration refresh. This includes converting the editor/detail islands
that currently (legitimately, but inconsistently) own a `Resource::new_blocking`,
so the invariant — and its lint — is unambiguous.

> Coordination caveat: several admin web files are still dirty in the working
> tree (`collections.rs`, `smart_collections.rs`, `multi_select_browser.rs`,
> …) alongside an unrelated health/migration workstream. Confirm Codex has
> stopped touching the web tree (or rebase onto its final commit) before
> starting, to avoid clobbering in-flight edits.

## Work

### 1. Extract the shared "seeded island" primitive (the core of the fix)

Today every converted page hand-rolls the same triad: wrapper
`Resource::new_blocking` → `<Suspense>` → island props → in-island
`LocalResource` + `Effect`-mirror into an `RwSignal`. Consolidate it so pages
stop copy-pasting it and future pages can't get it wrong.

- Add a helper module under `crates/ersatzrs-web/src/admin/ui/` (e.g.
  `seeded.rs`, exported from `ui/mod.rs`) providing:
  - A **wrapper helper** that takes a loader `async fn`/closure and a view
    builder, runs the `Resource::new_blocking` + `<Suspense>` boilerplate, and
    hands the resolved value to the island view. (A generic `#[component]` or a
    small function returning `impl IntoView`; verify the ergonomics against
    Leptos 0.8.19 generics — keep it a thin helper, not a macro, if generics
    fight back.)
  - An **in-island refresh helper** that, given a reactive key + an async
    loader, creates the client-only `LocalResource`, mirrors it into an
    `RwSignal` seeded from the prop value via `Effect`, and returns the
    `RwSignal` to read in the view. This captures the exact
    `LocalResource::new(...) + Effect::new(|_| if let Some(v) = r.get() {...})`
    shape now duplicated everywhere.
- Refactor the ~25 already-converted list/browser pages onto these helpers.
  Representative paths: `media/multi_select_browser.rs`, `media/collections.rs`,
  `media/smart_collections.rs`, `media/multi_collections.rs`,
  `media/rerun_collections.rs`, `media/filler_presets.rs`, `media/trash.rs`,
  `media/playlists.rs`, `media/remote_streams.rs`, `media/trakt_lists.rs`,
  `scheduling/{playouts,blocks,decos,schedules,templates}.rs`,
  `system/{health,logs}.rs`, `libraries/mod.rs`, `watermarks/list.rs`,
  `ffmpeg_profiles/list.rs`, `channels/list.rs`,
  `media_sources/{local,plex,jellyfin}.rs`.
- Where the dynamic list is itself produced by a shared primitive
  (`ui/primitives/table.rs` `PagedTable`, `ui/primitives/letter_bar.rs`),
  fold the seeding contract into the primitive's prop surface so a single
  call site fixes many routes and the row-rendering markers stay consistent.

### 2. Convert the remaining island-owned `Resource` pages to satisfy the invariant

These islands still own an SSR `Resource` (mostly editor/detail forms that pass
today but violate the strict invariant). Move the load to the wrapper and pass
resolved props, same as the list pages:

- Detail: `media/{artists,shows,season_detail,collection_detail}.rs`.
- Editors: `scheduling/{block_editor,playout_kind_editor,playout_nested_editor,
  yaml_editor}.rs`, `watermarks/editor.rs`, and the editor islands inside
  `media/{collections,smart_collections,multi_collections,rerun_collections,
  filler_presets,playlists,trakt_lists}.rs` and `scheduling/{decos,templates,
  schedules}.rs`, plus `media_sources/{local,emby,jellyfin,plex}.rs`.

(Enumerate authoritatively with the grep used during investigation:
`Resource::new` / `Resource::new_blocking` appearing after a `#[island]` in a
file, excluding `LocalResource`.)

### 3. Add hydration-lint rule-3 (the regression guard)

In `xtask/src/hydration_lint.rs` — the linter already AST-visits with an
`in_island` flag on `LintVisitor` (see `visit_item_fn`, `has_island_attr`):

- Add **rule-3**: flag a `Resource::new` or `Resource::new_blocking` call
  expression when `in_island` is true. Explicitly **allow `LocalResource::new`**
  (match the call path precisely so `LocalResource` is not caught).
- Message: island must not own an SSR `Resource`; load in the page wrapper and
  pass resolved props (point at the new TEST.md rule 3 and the `seeded` helper).
- **Fix the stale rule-2 message** in the same file (it currently advises
  "have the island own a `Resource::new_blocking`") to point at the wrapper-seed
  pattern instead.
- Seed `.hydration-lint-baseline` only if any not-yet-converted file remains at
  the time the rule lands; the goal is an **empty** baseline for rule-3.
- Wire is automatic: `cargo xtask parity-check web` already runs
  `hydration-lint`, so CI enforces it with no workflow change.

### 4. Rewrite the documented contract

- **TEST.md rule 3** (lines ~70–79): replace the "island owns
  `Resource::new_blocking`" guidance with the strict invariant — islands never
  own an SSR `Resource`; wrapper loads via `Resource::new_blocking` + `Suspense`
  and passes resolved props; islands refresh via `LocalResource` + `Effect`
  using the `seeded` helper. Remove the `FfmpegProfileEditorIsland` "owns
  blocking resource" reference; cite the new helper + a converted page.
- Update the **hydration-lint rule list** (TEST.md ~134–138) to document
  rule-3, and fix the per-page index notes that say "Island owns blocking
  resource" (e.g. `/ffmpeg/profiles/:id`, `/media/collections/:id`,
  `/media/filler/presets/:id/edit`, `/media/movies/:id`).
- Update memory `project-island-hydration-smoke-wip` (and add/refresh a
  `feedback`-type memory capturing the invariant + why) so future sessions
  start from the corrected rule, not the broken one.
- CHANGELOG `[Unreleased]`: a `Fixed`/`Changed` entry for the systemic admin-UI
  island hydration fix + the new lint guard.

## Verification

1. **Static, fast:** `cargo xtask hydration-lint -p ersatzrs-web --no-baseline`
   → zero rule-3 findings (proves no island owns an SSR `Resource`).
   Then `cargo xtask parity-check web` (fmt + lint + `cargo test -p
   ersatzrs-web` + clippy).
2. **Build consistency:** `cargo leptos build` from `crates/ersatzrs-web`
   (single consistent SSR+WASM build — see the wasm-checksum memory).
3. **Whole-app gate (authoritative):** `scripts/e2e/hydration-smoke.sh` →
   target **56/56** routes green (`window.__ersatzrs_hydrated === true`, no
   marker-mismatch console errors, no `[data-not-found]` fallthrough). This is
   the durable regression guard for the whole class.
4. **In-browser behavior spot-check** (the original report): on `/channels`
   click Delete → dialog opens; on `/playouts` row-select → detail table
   populates. Confirms handlers actually attach post-hydration.
5. **Negative lint check:** temporarily add a `Resource::new` inside an island
   in a scratch edit and confirm rule-3 fails — then revert. Proves the guard
   bites.
