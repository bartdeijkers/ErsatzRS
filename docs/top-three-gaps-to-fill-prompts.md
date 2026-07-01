<!--
generated: 2026-07-01
last_processed_commit: 7696320cde7a1db543dff185950933961b4a277e
parity_baseline_commit: 0d301df5e8512fdfccf3a611188d27ab1690ee31
generator: docs/automation/daily-progress-routine.md (ersatzrs-daily-progress)
rules: docs/automation/gap-prompt-rules.md
-->

# Top three chores to do - goal prompts

Actionable paste-and-run goal prompts, each closing one ErsatzTV parity gap,
ADR-backed runtime hardening gap, or spec-backed chore. These prompts are
selected from the current progress files, ADR index, active plans, and phase
state per [gap-prompt-rules.md](automation/gap-prompt-rules.md).

Current selection: maintainer-reported regressions reopened six parity
features on 2026-07-01, and ADR 0019 added one Not started scheduling
extension. Closed 2026-07-01: the movie-detail gap spanning `media-browsers`
and `content-variants` now renders `/media/movies/{movie_id}` extras as
episode-style rows, restores movie-extra collection/playlist actions, and
surfaces playable alternate-edition plus persistable commentary/alternate-audio
and subtitle options through ADR 0048 collection/playlist item overrides.
Closed 2026-07-01: the remaining `media-browsers` action/toolbar gap now gives
songs/images the shared right-side selection toolbar and item action menu,
restores artist-detail Add To Collection plus preview artwork, and fixes the
show-detail hydration shape found by focused browser proof. Closed 2026-07-01:
the `ersatzrs-healthcheck` Zero Duration check now ignores ordinary static
image media versions while preserving warnings for animated-capable image paths
and duration-bearing media. The next tracked follow-ups cover `/channels/add`
defaulting to `ErsatzTV` instead of `ErsatzRS`, ADR 0017 default FFmpeg profile
video bitrate halving with buffer sizes untouched, inconsistent Video/Audio
section spacing on `/ffmpeg/{ffmpeg_profile_id}`, and the proposed
`similar-video-cooldown` scheduling feature.

---

## 1. Fix reopened movie-detail extras and content-variant cards (slugs `media-browsers`, `content-variants`)

Status: closed 2026-07-01 by `fix(web): restore movie detail extras and variant cards`.
Evidence: focused Rust tests for movie detail projection/action persistence,
`cargo xtask hydration-lint -p ersatzrs-web`, and focused Chromium
`tests/e2e/content-variants.spec.ts` proof, including commentary subtitle
selection preservation in the playback troubleshooting form.

Use the repo workflow from AGENTS.md. This is a parity regression fix for
`/media/movies/{movie_id}`, tracked by these debts:
`movie-detail-extras-row-layout-regression` and
`movie-detail-extra-collection-playlist-actions` in
[`docs/progress/parity/media-browsers.toml`](progress/parity/media-browsers.toml),
and `movie-detail-commentary-edition-cards` in
[`docs/progress/parity/content-variants.toml`](progress/parity/content-variants.toml).

Before editing:
1. Read `docs/progress/parity/media-browsers.toml`,
   `docs/progress/parity/content-variants.toml`,
   `docs/adr/0018-extras-alternate-versions-and-stream-roles.md`,
   `plans/extras-alternate-versions-and-stream-roles.md`, `TEST.md`,
   `PORTING.md`, and `STYLE.md`.
2. Inspect the ErsatzTV movie detail baseline and the ErsatzRS TV show/season
   extras implementation first. Build a Source Map for movie detail extras,
   TV extras row cards, Add To Collection/Playlist action plumbing, ADR 0018
   alternate-version projection, and commentary stream-selection surfaces.
3. Build a Visual Source Map for `/media/movies/{movie_id}` covering the main
   movie header, Extras / Special Features, Versions / alternate editions,
   commentary-track options, and action menus.
4. Identify the seeded or live movie fixture that proves extras, commentary
   stream metadata, and alternate editions without recording private media paths
   in source, fixtures, logs, or snapshots.

Implement the slice end-to-end:
- Reproduce the movie detail regressions in browser/runtime proof.
- Render movie extras with the same episode-card/row treatment used for TV
  show/season extras instead of poster-style cards.
- Add movie-extra Add To Collection and Add To Playlist actions through the
  same validated action paths used by TV episode extras.
- Surface playable cards/options for commentary-track viewing and alternate
  movie editions such as theatrical, remastered, director's cut,
  black-and-white, and related `SubKind` values.
- Keep ADR 0018 default-exclusion behavior intact: normal searches and
  schedules remain primary-only unless variant fields are requested.
- Add focused regression coverage for layout/projection and action persistence.
- When browser proof passes, mark all three structured debts resolved, return
  `media-browsers` and `content-variants` to `Ported`, restore ADR 0018's
  implementation marker to `Implemented`, refresh `docs/progress/hydration.toml`,
  `TEST.md`, `docs/progress/phase.toml`, this prompt file, and add a session
  log entry.

Verification:
- Focused Rust tests for changed media-browser/content-variant projections and
  action server functions.
- `cargo xtask hydration-lint -p ersatzrs-web`.
- Browser or Playwright proof for `/media/movies/{movie_id}` showing extras use
  episode-style rows, movie-extra Add To Collection/Playlist works,
  commentary/edition cards render and select the intended variant/stream, the
  page hydrates, and there are no console/page errors.
- `cargo xtask progress check`.
- Run broader web parity/e2e gates if the fix touches shared media cards,
  collection/playlist action APIs, playback selection, or search projection.

Report the Source Map, Visual Source Map, ErsatzTV baseline files consulted,
root cause, verification run, changed files, and any remaining parity debt.

---

## 2. Fix media-browser action, toolbar, and artist-detail regressions (slug `media-browsers`)

Status: closed 2026-07-01 by `fix(web): restore media browser actions and artist detail parity`.
Evidence: focused Rust test for song/image playlist type projection,
`cargo xtask hydration-lint -p ersatzrs-web`, focused Chromium
`tests/e2e/media-browser-actions.spec.ts` proof for songs/images toolbar,
item actions, Add To Collection/Playlist persistence, and artist artwork/action
parity, plus focused Chromium `tests/e2e/content-variants.spec.ts` proof for
hydrated movie/show/season/artist detail action surfaces and no console/page
errors.

Use the repo workflow from AGENTS.md. This is a parity regression fix for the
remaining media-browser action and layout debts, tracked as
`artist-detail-add-to-collection-and-previews-regression`,
`media-detail-bulk-overflow-actions`, and `songs-images-toolbar-placement` in
[`docs/progress/parity/media-browsers.toml`](progress/parity/media-browsers.toml).

Before editing:
1. Read `docs/progress/parity/media-browsers.toml`,
   `docs/progress/hydration.toml`, `TEST.md`, `PORTING.md`, `STYLE.md`, and
   the admin UI hydration rules in `TEST.md`.
2. Inspect the ErsatzTV baselines for the affected routes first, including the
   movie browser action layout, TV show/season detail actions, artist detail,
   songs browser, and images browser.
3. Build a Source Map for shared media selection state, per-item overflow
   menus, collection/playlist/schedule/media-info/troubleshoot actions, artist
   artwork URL generation, and the relevant server functions/API endpoints.
4. Build a Visual Source Map for `/media/movies`, `/media/movies/{movie_id}`,
   `/media/tv/shows/{tvshow_id}`, `/media/tv/seasons/{season_id}`,
   `/media/music/artists/{artist_id}`, `/media/music/songs`, and
   `/media/browser/images`.

Implement the slice end-to-end:
- Reproduce the missing action menus/toolbars, top-positioned songs/images
  toolbars, broken artist Add To Collection button, and missing preview images
  against seeded or live pages.
- Restore the artist-level Add To Collection interaction so it opens the
  expected dialog and persists collection membership through the same validated
  path used by related media detail pages.
- Restore preview artwork on `/media/music/artists/{artist_id}` for the artist
  header and music-video cards, preserving ErsatzTV's thumbnail/fanart/fallback
  ordering and avoiding private absolute media paths in source, fixtures, logs,
  or snapshots.
- Add the relevant multi-select toolbar and per-item three-dots overflow menus
  to `/media/movies/{movie_id}`, `/media/tv/shows/{tvshow_id}`,
  `/media/tv/seasons/{season_id}`, `/media/music/artists/{artist_id}`,
  `/media/music/songs`, and `/media/browser/images`, matching the actions that
  `/media/movies` exposes where they are valid for the media type.
- Align `/media/music/songs` and `/media/browser/images` action toolbar
  placement with the right-side movie/TV browser layout, including responsive
  behavior.
- Add focused regression coverage for actions, rendered artwork, and toolbar
  geometry.
- When browser proof passes, mark the three structured debts resolved, return
  `media-browsers` to `Ported` if no other media-browser debt remains, refresh
  `docs/progress/hydration.toml`, `TEST.md`, `docs/progress/phase.toml`, this
  prompt file, and add a session log entry.

Verification:
- Focused Rust tests for the changed server functions/projections.
- `cargo xtask hydration-lint -p ersatzrs-web`.
- Browser or Playwright proof for every affected route showing the expected
  toolbar/overflow actions, right-side songs/images toolbar placement,
  functional artist Add To Collection, rendered artist/music-video preview
  images, hydrated pages, and no console/page errors.
- `cargo xtask progress check`.
- Run broader web parity/e2e gates if the fix touches shared media-card,
  artwork, collection-action, or server-function plumbing.

Report the Source Map, Visual Source Map, ErsatzTV baseline files consulted,
root cause, verification run, changed files, and any remaining parity debt.

---

## 3. Fix static-image Zero Duration health warnings (slug `ersatzrs-healthcheck`)

Status: closed 2026-07-01 by `fix(health): ignore static images in zero-duration health checks`.
Evidence: focused `ersatzrs-infra` health tests prove a zero-duration `.jpg`
image passes, a zero-duration movie plus `.gif` animated-image candidate still
warn, and remote-stream placeholders remain ignored.

Use the repo workflow from AGENTS.md. This is a health parity regression fix
for `zero-duration-healthcheck-static-images` in
[`docs/progress/parity/ersatzrs-healthcheck.toml`](progress/parity/ersatzrs-healthcheck.toml).

Before editing:
1. Read `docs/progress/parity/ersatzrs-healthcheck.toml`, `docs/progress/phase.toml`,
   `STYLE.md`, and `docs/how-to/testing.md`.
2. Inspect `crates/ersatzrs-infra/src/health/checks/zero_duration.rs`, the
   health registry in `crates/ersatzrs-infra/src/health/mod.rs`, and existing
   zero-duration health tests.
3. Build a Source Map for `MediaVersion` image linkage, remote-stream
   placeholder exclusion, static image rows, animated/timed image rows, and the
   `/system/health` result rendering path.

Implement the slice end-to-end:
- Make the Zero Duration health check ignore ordinary static image media
  versions whose `Duration` is `00:00:00`.
- Keep the existing remote-stream placeholder exclusion.
- Preserve warnings for duration-bearing media with missing duration, including
  movies, episodes, music videos, other videos, songs where applicable, and
  animated/timed image formats that should carry duration metadata.
- Add focused regression coverage for a static image row and at least one
  zero-duration duration-bearing media row.
- When proof passes, mark the structured debt resolved, return
  `ersatzrs-healthcheck` to `Ported` if no other health debt remains, refresh
  `docs/progress/phase.toml`, this prompt file, and add a session log entry.

Verification:
- Focused `ersatzrs-infra` health tests for `ZeroDurationHealthCheck`.
- `cargo fmt --all -- --check`.
- `cargo clippy --workspace --all-targets -- -D warnings`.
- `cargo clippy --workspace --all-targets --features mysql -- -D warnings`.
- `cargo test --workspace --all-features`.
- `cargo xtask progress check`.
- Browser or API proof for `/system/health` if the rendering or persisted
  result path changes.

Report the Source Map, query/classification change, verification run, changed
files, and any remaining health parity debt.

---
