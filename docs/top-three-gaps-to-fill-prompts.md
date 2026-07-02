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

Current selection: updated 2026-07-02 after the `/channels/add` default group
regression closed (`channels-add-default-group-ersatzrs` resolved; channels
admin UI returned to `Ported`). The remaining in-progress Phase 7 items —
`/ffmpeg/{id}` profile-editor section spacing and the ADR 0017 default FFmpeg
profile video bitrate retune — move up, and the freed slot is refilled by the
top-ranked registered media curation follow-up: ADR 0048 same-collection
duplicate movie rows with distinct playback overrides. Stay-in-context Add To
Collection/Add To Playlist behavior, artist-detail music-video checkbox
multi-select, ADR 0020 playout rebuild, ADR 0019 similar-video cooldown,
proposed localization ADRs, active Docker release plans, and module split work
remain candidates, but rank behind the selected items in this pass.

---

## 1. Fix the next ErsatzTV parity gap in ErsatzRS: FFmpeg profile editor spacing (slug `ffmpeg-profile-editor`)

Use the repo workflow from AGENTS.md plus `skills/parity-slice.md`.
The goal is fully functional ErsatzTV visual/editor parity without stopgaps.

Tracked debt: `profile-editor-video-audio-section-spacing` in
[`docs/progress/parity/ffmpeg-profile-editor.toml`](progress/parity/ffmpeg-profile-editor.toml).
Current debt text: on `/ffmpeg/{ffmpeg_profile_id}`, the Video and Audio
sections use different vertical spacing between elements than the General
section; closeout requires desktop and mobile browser proof that General,
Video, and Audio render with consistent vertical rhythm without regressing
hydrated save/edit behavior.

Before editing:
1. Read `docs/progress/parity/ffmpeg-profile-editor.toml`,
   `docs/progress/hydration.toml`, `docs/automation/parity-baseline.md`,
   `TEST.md`, `PORTING.md`, and `STYLE.md`.
2. Ensure `$ERSATZTV_SRC` is at the pinned parity baseline, then inspect
   `ErsatzTV/Pages/FFmpegProfile*.razor` for the actual section structure,
   form containers, dividers, labels, and spacing.
3. Inspect the current ErsatzRS implementation in
   `crates/ersatzrs-web/src/admin/pages/ffmpeg_profiles/editor.rs`,
   `crates/ersatzrs-web/src/admin/pages/ffmpeg_profiles/editor.module.css`,
   `crates/ersatzrs-web/src/admin/server_fns/ffmpeg_profiles.rs`,
   `crates/ersatzrs-web/src/api/ffmpeg_profiles.rs`, and
   `tests/e2e/ffmpeg-profiles.spec.ts`.
4. Build a Source Map covering SSR data load, island state, no-JS fallback,
   save/copy actions, section rendering, CSS modules, and route aliases.
5. Build a Visual Source Map for `/ffmpeg/profiles/{id}` and `/ffmpeg/{id}`,
   including General, Video, Audio, Hardware Acceleration, dividers, labels,
   inputs, row gaps, and mobile stacking.

Implement the coherent end-to-end slice that genuinely closes the gap:
- Reproduce the spacing mismatch in a running browser before editing.
- Align Video and Audio section vertical rhythm with General without changing
  the field contract or hiding controls.
- Keep the hydrated editor, no-JS edit fallback, Copy Profile, validation, and
  save behavior intact.
- Prefer existing CSS-module patterns and shared form primitives over ad hoc
  route-only layout hacks.
- Add focused regression coverage that checks the section geometry or stable
  class structure in desktop and mobile layouts.
- Update `docs/progress/parity/ffmpeg-profile-editor.toml`,
  `docs/progress/hydration.toml` if proof state changes,
  `docs/progress/phase.toml`, `TEST.md` if the Playwright playbook changes,
  this prompt file, and the session log when the debt closes.

Visual verification for UI routes:
- Start the app with `./run.sh serve` after rebuilding Leptos assets.
- Browser-proof `/ffmpeg/profiles/{id}` and the `/ffmpeg/{id}` alias in desktop
  and mobile viewports.
- Capture or inspect same-viewport geometry for General, Video, and Audio:
  row gaps, label/input alignment, divider placement, clipped text, and page
  overflow.
- Save an edit after the layout change and confirm no console/page errors.

Verification:
- Focused Rust tests only if projection, server functions, or save behavior
  changes.
- `cargo xtask hydration-lint -p ersatzrs-web`.
- `npx playwright test tests/e2e/ffmpeg-profiles.spec.ts`.
- `cargo xtask progress check`.

When done, report the Source Map, Visual Source Map, ErsatzTV baseline files
consulted, behavior implemented, any remaining `ParityDebt`, verification run,
and changed files.

---

## 2. Fix the next ErsatzRS parity-infrastructure gap: ADR 0017 default FFmpeg profile bitrate retune (slug `ffmpeg-command-generation`)

Use the repo workflow from AGENTS.md and the governing ADRs. This is an
ADR-backed runtime/default-profile hardening slice; do not force an ErsatzTV
parity workflow where ADR 0017 intentionally deviates from ErsatzTV defaults.
The goal is the complete ADR 0017 v3 behavior without stopgaps.

Tracked debt: `default-profile-video-bitrate-halving` in
[`docs/progress/parity/ffmpeg-command-generation.toml`](progress/parity/ffmpeg-command-generation.toml),
backed by [ADR 0017](adr/0017-default-ffmpeg-profile-1080p60.md). Current debt
text: fresh-database default profile video bitrates should change from 8000,
16000, 20000, 45000, and 60000 kbit/s to 4000, 8000, 10000, 22500, and 30000
kbit/s while video buffer sizes remain 8000, 16000, 20000, 45000, and 60000
kbit.

Before editing:
1. Read `docs/progress/parity/ffmpeg-command-generation.toml`,
   `docs/adr/0017-default-ffmpeg-profile-1080p60.md`,
   `docs/adr/0014-runtime-learned-hardware-capability-fallback.md`,
   `docs/how-to/testing.md`, and `STYLE.md`.
2. Inspect the current default-profile seed and hardware update paths in
   `crates/ersatzrs-infra/src/db.rs`,
   `crates/ersatzrs-web/src/host/bootstrap.rs`,
   `crates/ersatzrs-domain/src/ffmpeg.rs`, and the existing focused seed tests.
3. Build a Source Map covering fresh database bootstrap, default profile ID
   selection, seeded-profile idempotence, seed-time hardware acceleration
   update, existing database non-rewrite behavior, and command-generation use of
   profile bitrate/buffer fields.
4. Reproduce or prove the current seed values before editing with a focused
   test, temp-database query, or direct failing assertion.

Implement the coherent end-to-end slice that genuinely closes the gap:
- Retune only the seeded `VideoBitrate` values to 4000, 8000, 10000, 22500, and
  30000 kbit/s.
- Keep `VideoBufferSize` exactly at 8000, 16000, 20000, 45000, and 60000 kbit.
- Do not rewrite existing saved profile tables.
- Preserve the five-profile seed, default profile pointer, AC-3 6ch audio
  defaults, hardware-acceleration seed update, and ADR 0014 fallback behavior.
- Add focused regression coverage for fresh database seeding and idempotence.
- When proof passes, mark the structured debt resolved, return
  `ffmpeg-command-generation` to `Ported` if no open debt remains, update ADR
  0017's Implementation text if it is now complete, refresh
  `docs/progress/phase.toml`, `CHANGELOG.md` if needed, this prompt file, and
  the session log.

Verification:
- `cargo test -p ersatzrs-infra media_server_initializer_seeds_upstream_defaults`.
- `cargo test -p ersatzrs-infra media_server_initializer_is_idempotent`.
- `cargo test -p ersatzrs-infra default_ffmpeg_profile_hardware_update_targets_seeded_profiles`.
- Run any additional focused `ersatzrs-ffmpeg` command-generation tests if the
  command layer or profile mapping changes.
- `cargo xtask progress check`.

When done, report the Source Map, ADR 0017 fields changed, verification run,
changed files, and any remaining FFmpeg/profile parity debt.

---

## 3. Build the next ErsatzRS spec chore: ADR 0048 duplicate collection rows with distinct playback overrides (slug `multi-collections-rerun-collections-playlists-filler-presets`)

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
