<!--
generated: 2026-07-02
last_processed_commit: 4ce96714dae25c3731f05dad52c070ca3524be3a
parity_baseline_commit: 0d301df5e8512fdfccf3a611188d27ab1690ee31
generator: docs/automation/daily-progress-routine.md (ersatzrs-daily-progress)
rules: docs/automation/gap-prompt-rules.md
-->

# Top three chores to do - goal prompts

Actionable paste-and-run goal prompts, each closing one ErsatzTV parity gap,
ADR-backed runtime hardening gap, or spec-backed chore. These prompts are
selected from the current progress files, ADR index, active plans, and phase
state per [gap-prompt-rules.md](automation/gap-prompt-rules.md).

Current selection: updated 2026-07-02 after all three 2026-07-01 reopened
items closed (`channels-add-default-group-ersatzrs`,
`profile-editor-video-audio-section-spacing`, and
`default-profile-video-bitrate-halving` resolved; channels admin UI, the
FFmpeg profile editor, and FFmpeg command generation returned to `Ported`).
The three registered media curation follow-ups now fill the queue: ADR 0048
same-collection duplicate movie rows with distinct playback overrides,
stay-in-context Add To Collection/Add To Playlist behavior, and artist-detail
music-video checkbox multi-select. ADR 0020 playout rebuild, ADR 0019
similar-video cooldown, proposed localization ADRs, active Docker release
plans, and module split work remain candidates, but rank behind the selected
items in this pass.

---

## 1. Build the next ErsatzRS spec chore: ADR 0048 duplicate collection rows with distinct playback overrides (slug `multi-collections-rerun-collections-playlists-filler-presets`)

Use the repo workflow from AGENTS.md and the governing ADR. This is a
spec-backed ErsatzRS-owned extension — ErsatzTV has no equivalent, so do not
force the parity-slice workflow; follow
[ADR 0048](adr/0048-collection-item-playback-overrides.md) instead. The goal is
the complete follow-up slice ADR 0048's Implementation section names, without
stopgaps.

Tracked debt: `collection-item-duplicate-playback-overrides` in
[`docs/progress/parity/multi-collections-rerun-collections-playlists-filler-presets.toml`](progress/parity/multi-collections-rerun-collections-playlists-filler-presets.toml).
Current debt text: the same movie should be addable to the same collection
multiple times when each row has a different playback override, so movies with
multiple commentary or alternate audio tracks can have every track represented
in one collection. Current SQLite and MySQL schemas keep `CollectionItem`
keyed by `(CollectionId, MediaItemId)` (`PK_CollectionItem`), so the model
cannot represent two rows for the same movie even when `PreferredAudioTitle`,
preferred audio/subtitle language, or `SubtitleMode` differ.

Before editing:
1. Read `docs/adr/0048-collection-item-playback-overrides.md`,
   `docs/adr/0018-extras-alternate-versions-and-stream-roles.md`,
   `STYLE.md` (SQL migration rules), and
   `docs/how-to/testing.md`.
2. Inspect the current collection item identity and override columns in
   `migrations/0001_initial.sql`,
   `migrations/202607011455_collection_item_playback_overrides.sql`, and the
   matching `migrations/mysql/` files.
3. Inspect the add/inspect/schedule paths:
   `crates/ersatzrs-infra/src/repositories/collection.rs`,
   `crates/ersatzrs-infra/src/repositories/scheduling.rs`,
   `crates/ersatzrs-web/src/admin/server_fns/media_browser.rs`, and
   `crates/ersatzrs-web/src/admin/pages/media/collection_detail.rs`.
4. Build a Source Map covering collection item identity, add-to-collection
   flow, collection detail rendering of saved overrides, scheduling expansion
   into playout items, and SQLite/MySQL migration parity.
5. Reproduce the current limitation before editing: adding the same movie
   twice with different overrides must currently collapse to one row.

Implement the coherent end-to-end slice that genuinely closes the gap:
- Change collection item identity so the same movie can appear more than once
  in one collection when playback overrides differ, in both SQLite and MySQL
  migrations, following STYLE.md's EF-style constraint naming.
- Keep default rows (no override) behaving exactly like the ErsatzTV row shape
  and keep import/parity tooling treating override fields as ErsatzRS-only.
- Preserve ADR 0048 scheduling propagation so each duplicate row schedules
  separately with its own override.
- Make collection detail UI distinguish duplicate rows by their saved override.
- Add focused regression coverage for duplicate-row persistence, collection
  detail projection, and scheduling expansion.
- Update the owning parity TOML, ADR 0048's Implementation text,
  `docs/progress/phase.toml`, `CHANGELOG.md`, this prompt file, and the
  session log when the debt closes.

Verification:
- Focused `cargo test -p ersatzrs-infra` collection/scheduling tests covering
  duplicate rows with distinct overrides.
- `cargo test -p ersatzrs-web --all-targets <collection_route_filter>` for the
  admin flows touched.
- `cargo xtask hydration-lint -p ersatzrs-web` if collection detail islands
  change.
- `npx playwright test tests/e2e/collections.spec.ts` when the UI flow changes.
- `cargo xtask progress check`.

When done, report the Source Map, behavior implemented, migration changes,
any remaining debt, verification run, and changed files.

---

## 2. Build the next ErsatzRS spec chore: stay-in-context Add To Collection / Add To Playlist (slug `media-browsers`)

Use the repo workflow from AGENTS.md. This is a maintainer-requested ErsatzRS
UX chore recorded as structured debt — ErsatzTV's flows navigate away, so do
not treat the current navigation as a parity requirement; the debt entry is
the contract. The goal is the complete stay-in-context behavior without
stopgaps.

Tracked debt: `add-to-collection-playlist-stay-in-context` in
[`docs/progress/parity/media-browsers.toml`](progress/parity/media-browsers.toml).
Current debt text: after adding one or more media items to a collection or
playlist from a media page, the admin UI should keep the user in the current
browsing/detail context by default instead of always navigating to the target
collection or playlist page. Most add-to-collection/add-to-playlist sessions
are iterative curation sessions where the user wants to continue adding items.

Before editing:
1. Read `docs/progress/parity/media-browsers.toml`, `TEST.md`, and `STYLE.md`.
2. Inspect the current add flows in
   `crates/ersatzrs-web/src/admin/server_fns/media_browser.rs`,
   `crates/ersatzrs-web/src/admin/pages/media/browser.rs`, the media detail
   pages under `crates/ersatzrs-web/src/admin/pages/media/`, and the existing
   browser proof in `tests/e2e/media-browser-actions.spec.ts` and
   `tests/e2e/hydration-pending-routes.spec.ts`.
3. Build a Source Map covering the Add To Collection / Add To Playlist dialog
   flow, single-item and bulk selection paths, post-add navigation, success
   feedback (toast) primitives, and localization of any new labels.
4. Reproduce the current always-navigate behavior in a running browser before
   editing.

Implement the coherent end-to-end slice that genuinely closes the gap:
- Keep the user on the current media browse/detail page by default after a
  successful add, for single-item and bulk flows, collections and playlists.
- Show a clear success state that still offers an explicit View
  Collection/View Playlist navigation path.
- Localize any new user-visible labels for every supported culture under
  `crates/ersatzrs-web/src/admin/languages/`.
- Preserve SSR/no-JS fallbacks and hydration stability per `TEST.md`.
- Add focused browser coverage proving bulk and single-item flows for
  collections and playlists keep context and can still navigate explicitly.
- Update `docs/progress/parity/media-browsers.toml`,
  `docs/progress/hydration.toml` if proof state changes,
  `docs/progress/phase.toml`, `CHANGELOG.md`, this prompt file, and the
  session log when the debt closes.

Verification:
- Focused Rust tests for any server-fn/projection changes.
- `cargo xtask hydration-lint -p ersatzrs-web`.
- `npx playwright test tests/e2e/media-browser-actions.spec.ts` plus the
  focused stay-in-context cases added for this chore.
- `cargo xtask progress check`.

When done, report the Source Map, behavior implemented, any remaining debt,
verification run, and changed files.

---

## 3. Build the next ErsatzRS spec chore: artist-detail music-video multi-select actions (slug `media-browsers`)

Use the repo workflow from AGENTS.md. This is a maintainer-requested ErsatzRS
UX chore recorded as structured debt; ErsatzTV's artist page has no bulk
music-video selection, so the debt entry is the contract. The goal is the
complete multi-select behavior without stopgaps.

Tracked debt: `artist-detail-music-video-multi-select-actions` in
[`docs/progress/parity/media-browsers.toml`](progress/parity/media-browsers.toml).
Current debt text: on `/media/music/artists/{artist_id}`, the artist detail
page should allow selecting multiple music-video rows with checkboxes and
adding the selected videos to collections or playlists in bulk. The current
artist detail page restored artist-level Add To Collection and music-video
preview artwork, but the listed music videos still lack a multi-select
checkbox/action toolbar for bulk Add To Collection and Add To Playlist.

Before editing:
1. Read `docs/progress/parity/media-browsers.toml`, `TEST.md`, and `STYLE.md`.
2. Inspect the current artist detail implementation in
   `crates/ersatzrs-web/src/admin/pages/media/artists.rs` (and its detail
   module/CSS), the shared selection toolbar used by the media browser lists in
   `crates/ersatzrs-web/src/admin/pages/media/browser.rs`,
   `crates/ersatzrs-web/src/admin/server_fns/media_browser.rs`, and the
   existing browser proof in `tests/e2e/media-browser-actions.spec.ts`.
3. Build a Source Map covering artist detail row rendering, the shared
   checkbox/selection toolbar primitives, bulk Add To Collection / Add To
   Playlist dialog flows, persistence, and localization of any new labels.
4. Reproduce the missing behavior in a running browser before editing: artist
   music-video rows expose no checkboxes or bulk actions today.

Implement the coherent end-to-end slice that genuinely closes the gap:
- Add checkbox multi-select to the artist detail music-video rows, reusing the
  shared media-browser selection toolbar/action primitives over ad hoc UI.
- Bulk Add To Collection and Add To Playlist dialogs persist all selected
  music-video items; individual row actions keep working.
- Localize any new user-visible labels for every supported culture under
  `crates/ersatzrs-web/src/admin/languages/`.
- Keep the route hydrated and console-clean per `TEST.md` island rules.
- Add focused browser coverage proving the bulk flow persists membership for
  multiple selected music videos.
- Update `docs/progress/parity/media-browsers.toml`,
  `docs/progress/hydration.toml` if proof state changes,
  `docs/progress/phase.toml`, `CHANGELOG.md`, this prompt file, and the
  session log when the debt closes.

Verification:
- Focused Rust tests for any server-fn/projection changes.
- `cargo xtask hydration-lint -p ersatzrs-web`.
- `npx playwright test tests/e2e/media-browser-actions.spec.ts` plus the
  focused multi-select cases added for this chore.
- `cargo xtask progress check`.

When done, report the Source Map, behavior implemented, any remaining debt,
verification run, and changed files.

---
