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

Current selection: regenerated 2026-07-02 from `cargo xtask progress gaps`,
open `[[debt]]` items, `cargo xtask progress adr list`, active plans, and
`docs/progress/phase.toml`. The completed movie-detail, media-browser action,
and static-image health-check prompts were removed. The top three are the
remaining in-progress Phase 7 items: `/channels/add` default group,
`/ffmpeg/{id}` profile-editor section spacing, and ADR 0017 default FFmpeg
profile video bitrate retune. Newly registered media curation follow-ups cover
same-collection duplicate movie rows with distinct ADR 0048 playback overrides
and stay-in-context Add To Collection/Add To Playlist behavior, plus
artist-detail music-video checkbox multi-select for bulk collection/playlist
adds. ADR 0020 playout rebuild, ADR 0019 similar-video cooldown, proposed
localization ADRs, active Docker release plans, and module split work remain
candidates, but rank behind the selected phase-focus regressions in this pass.

---

## 1. Fix the next ErsatzTV parity gap in ErsatzRS: `/channels/add` default group (slug `channels-admin-ui`)

Use the repo workflow from AGENTS.md plus `skills/parity-slice.md`.
The goal is fully functional ErsatzTV parity without stopgaps.

Tracked debt: `channels-add-default-group-ersatzrs` in
[`docs/progress/parity/channels-admin-ui.toml`](progress/parity/channels-admin-ui.toml).
Current debt text: creating a channel at `/channels/add` shows `ErsatzTV` as
the default channel group instead of `ErsatzRS`; closeout requires browser
proof that `/channels/add` pre-fills `Group` with `ErsatzRS`, saving a new
channel persists that value, and existing channel edit behavior is unchanged.

Before editing:
1. Read `docs/progress/parity/channels-admin-ui.toml`,
   `docs/automation/parity-baseline.md`, `TEST.md`, `PORTING.md`, and `STYLE.md`.
2. Ensure `$ERSATZTV_SRC` is at the pinned parity baseline, then inspect
   `ErsatzTV/Pages/Channel*.razor`,
   `ErsatzTV/ViewModels/ChannelEditViewModel.cs`,
   `ErsatzTV/Validators/ChannelEditViewModelValidator.cs`, and
   `ErsatzTV/Locals/Pages/Channels*.resx`.
3. Inspect the current ErsatzRS channel editor/default path in
   `crates/ersatzrs-core/src/repositories/channel.rs`,
   `crates/ersatzrs-infra/src/repositories/channel.rs`,
   `crates/ersatzrs-web/src/admin/pages/channels/`,
   `crates/ersatzrs-web/src/admin/server_fns/channels.rs`, and
   `crates/ersatzrs-web/src/api/channels.rs`.
4. Build a Source Map covering default construction, editor form binding,
   validation, multipart/no-JS save fallback, repository persistence, and
   side effects.
5. Build a Visual Source Map for `/channels/add` and `/channels/{id}`,
   including the Group field, surrounding form geometry, save actions,
   validation surfaces, and mobile layout.

Implement the coherent end-to-end slice that genuinely closes the gap:
- Reproduce the current `/channels/add` default value before editing.
- Make fresh Add Channel forms default `Group` to `ErsatzRS`.
- Preserve existing-channel edit behavior and user-saved group values.
- Preserve SSR, hydrated, and multipart/no-JS save paths.
- Add focused regression coverage for default form projection and save
  persistence.
- Update `docs/progress/parity/channels-admin-ui.toml`,
  `docs/progress/phase.toml`, `CHANGELOG.md` if the user-visible behavior needs
  a release note, this prompt file, and the session log when the debt closes.

Visual verification for UI routes:
- Start the app with `./run.sh serve` after rebuilding Leptos assets.
- Browser-proof `/channels/add` in desktop and mobile viewports.
- Save a new channel and verify the persisted Group remains `ErsatzRS`.
- Open an existing channel and verify its saved group is not overwritten.
- Inspect the console/page errors and confirm the form hydrates.

Verification:
- Focused Rust tests for channel default projection/save behavior.
- `cargo xtask hydration-lint -p ersatzrs-web`.
- `npx playwright test tests/e2e/channels-edit-numbers.spec.ts` plus the
  focused channel-add spec or test case added for this regression.
- `cargo xtask progress check`.

When done, report the Source Map, Visual Source Map, ErsatzTV baseline files
consulted, behavior implemented, any remaining `ParityDebt`, verification run,
and changed files.

---

## 2. Fix the next ErsatzTV parity gap in ErsatzRS: FFmpeg profile editor spacing (slug `ffmpeg-profile-editor`)

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

## 3. Fix the next ErsatzRS parity-infrastructure gap: ADR 0017 default FFmpeg profile bitrate retune (slug `ffmpeg-command-generation`)

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
