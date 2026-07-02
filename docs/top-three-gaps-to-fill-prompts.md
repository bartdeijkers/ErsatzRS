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

Current selection: the previous three prompts all closed in the
`7696320..4ce9671` window — `media-browsers` and `content-variants` returned to
`Ported` (movie-detail extras/variant cards and media-browser action/toolbar/
artist-detail regressions), and `ersatzrs-healthcheck` returned to `Ported`
(static-image Zero Duration warnings). Their slots are refilled from the
remaining open work. Prompt 1 leads with the confirmed-blocking playout-rebuild
debt (ADR 0020): a channel Subtitle Mode change from Forced to Any did not take
effect at runtime because scheduled items kept the old baked snapshot and no
rebuild was enqueued. Prompt 2 is the ADR 0017 default-profile video bitrate
halving (backend/pipeline). Prompt 3 fixes the `/channels/add` default channel
group defaulting to `ErsatzTV` instead of `ErsatzRS`.

Note: the pinned ErsatzTV baseline
(`0d301df5e8512fdfccf3a611188d27ab1690ee31`) could not be checked out this run
(upstream egress blocked by the session network policy), so no parity status was
re-measured against upstream — the window's own commits had already reconciled
the closed features. Each prompt below still instructs the implementing agent to
read ErsatzTV source at the pinned commit before porting behavior. Lower-ranked
open work not selected this run: the `/ffmpeg/{ffmpeg_profile_id}` Video/Audio
section spacing debt (`ffmpeg-profile-editor`, cosmetic) and the Not-started
`similar-video-cooldown` scheduling feature (ADR 0019).

---

## 1. Build the next ErsatzRS spec chore: ADR 0020 playout rebuild on impacting changes (slug `playout-rebuild-on-impacting-changes`)

Use the repo workflow from AGENTS.md. This is a spec-backed chore governed by
[`docs/adr/0020-playout-rebuild-on-impacting-changes.md`](adr/0020-playout-rebuild-on-impacting-changes.md);
port the ErsatzTV `RefreshPlayout`/`ResetPlayout` semantics it names. The goal
is the complete, working rebuild behavior the ADR describes for this slice, with
every shortfall recorded as visible `ParityDebt` — no stopgaps. This is the
top pick because it carries confirmed user-visible runtime evidence: a channel
Subtitle Mode change did not reach already-scheduled items.

This slice targets three open debts in
[`docs/progress/parity/playout-rebuild-on-impacting-changes.toml`](progress/parity/playout-rebuild-on-impacting-changes.toml),
led by the blocking one:
- `channel-edit-triggers-no-playout-rebuild` (blocking runtime): "Saving a
  channel enqueues no playout build (`server_fns/channels.rs`), so changes to
  subtitle mode, preferred subtitle/audio language and title, watermark,
  streaming mode, FFmpeg profile, or playout offset never reach
  already-scheduled items." Evidence: a Subtitle Mode change Forced -> Any left
  a Dutch sidecar unburned because scheduled items kept the old baked snapshot.
- `classic-refresh-appends-instead-of-rebuilding`: classic ignores
  `request.mode` and branches only on anchor presence, so classic `Refresh`
  behaves like ErsatzTV `Continue` (append-only) instead of `RefreshPlayout`
  (ClearItems=true, null main anchor, retained checkpoint anchors).
- `reset-playout-must-rewrite-near-term-items`: "Reset Playout" must rewrite ALL
  items in the build window (only the on-air segment may wait for its next item
  boundary), not just append future ones.

Before editing:
1. Read the ADR, `docs/progress/parity/playout-rebuild-on-impacting-changes.toml`,
   `PORTING.md`, and `STYLE.md`.
2. Read the ErsatzTV source at the pinned commit
   `0d301df5e8512fdfccf3a611188d27ab1690ee31` for the `etv_source` paths:
   `ErsatzTV.Core/Scheduling/PlayoutBuilder.cs` (RefreshPlayout/ResetPlayout),
   `ErsatzTV.Application/ProgramSchedules/Commands/*Handler.cs` (BuildPlayout
   Refresh), `ErsatzTV.Application/Channels/Commands/UpdateChannelHandler.cs`,
   and `ErsatzTV/Pages/Playouts.razor`.
3. Build a Source Map for classic/yaml/scripted `request.mode` handling,
   `clear_items`, checkpoint-anchor retention, the channel/schedule save server
   functions in `crates/ersatzrs-web/src/admin/server_fns/`, and the maintenance
   enqueue path in `crates/ersatzrs-infra/src/maintenance.rs`.

Implement the slice end-to-end:
- Reproduce the blocking debt: prove a channel setting change (e.g. Subtitle
  Mode) does not reach an already-scheduled item today.
- On channel save, enqueue a coalesced `Refresh` build for every playout on the
  channel whenever a field that affects playback selection or stream shape
  changes; a test must prove an upcoming item reflects the new setting.
- Make the classic engine consume `request.mode`: `Refresh` clears and rebuilds
  the window while preserving checkpoint anchors and enumerator/shuffle
  continuity; port ErsatzTV Refresh/Reset semantics to yaml/scripted too.
- Make "Reset Playout" (and Reset All Playouts) rewrite every future item in the
  window, not just the tail.
- Add focused scheduler regression tests asserting identical media order from
  the checkpoint with re-stamped per-item settings.
- When proof passes, resolve the three structured debts, advance
  `playout-rebuild-on-impacting-changes` toward `Ported` (only if no open debt
  remains), mark ADR 0020 implemented, update `docs/progress/phase.toml`, this
  prompt file, and add a session log entry.

Verification (backend/pipeline; no visual section):
- Focused `ersatzrs-core` scheduler tests for classic/yaml/scripted Refresh/Reset.
- Focused `ersatzrs-web`/`ersatzrs-infra` tests for the channel-save enqueue path.
- `cargo xtask progress check`.
- `cargo xtask smoke` if the change touches stream generation or the baked
  snapshot read path.

Report the Source Map, ErsatzTV baseline files consulted at the pin, root cause,
behavior implemented, remaining `ParityDebt`, verification run, and changed files.

---

## 2. Build the next ErsatzRS spec chore: ADR 0017 default FFmpeg profile video bitrate halving (slug `ffmpeg-command-generation`)

Use the repo workflow from AGENTS.md. This is a spec-backed chore under
[`docs/adr/0017-default-ffmpeg-profile-1080p60.md`](adr/0017-default-ffmpeg-profile-1080p60.md),
closing the open debt `default-profile-video-bitrate-halving` in
[`docs/progress/parity/ffmpeg-command-generation.toml`](progress/parity/ffmpeg-command-generation.toml).
The goal is the exact seeded values the debt records, with any shortfall visible
as `ParityDebt` — no stopgaps.

Current debt text: "The ADR 0017 fresh-database default FFmpeg profiles still
use the earlier video bitrate targets; their video bitrates should be halved,
e.g. 16000 kbit/s to 8000 kbit/s for the 1080p default, while video buffer-size
values remain untouched." Reclose only after fresh DB seeding and tests prove
video bitrates are 4000, 8000, 10000, 22500, and 30000 kbit/s respectively while
video buffer sizes stay 8000, 16000, 20000, 45000, and 60000 kbit.

Before editing:
1. Read the ADR, `docs/progress/parity/ffmpeg-command-generation.toml`, and
   `STYLE.md`.
2. Read the ErsatzTV command layer at the pinned commit
   `0d301df5e8512fdfccf3a611188d27ab1690ee31`
   (`$ERSATZTV_SRC/ErsatzTV.FFmpeg/CommandGenerator.cs`) only where it informs
   the profile bitrate/VBV contract.
3. Build a Source Map for the fresh-database default profile seed in
   `crates/ersatzrs-infra/src/db.rs` (the table-driven five-profile seed: 720p,
   1080p default, 1080p HDR, 4K, 4K HDR), the seed idempotence tests, and the
   seed-time hardware-acceleration updater in `crates/ersatzrs-ffmpeg`.

Implement the slice end-to-end:
- Halve the five seeded video bitrate targets to 4000, 8000, 10000, 22500, and
  30000 kbit/s, leaving the video buffer sizes at 8000, 16000, 20000, 45000, and
  60000 kbit and all other profile fields (AC-3 6ch, presets, VBV wiring,
  B-frames, deinterlace, framerate-normalize) unchanged.
- Do not overwrite existing saved profile tables; only the fresh-DB seed changes.
- Update the seed tests (`media_server_initializer_seeds_upstream_defaults`,
  `media_server_initializer_is_idempotent`, and related) to assert the new
  bitrate/buffer pairs.
- When proof passes, resolve `default-profile-video-bitrate-halving`, return
  `ffmpeg-command-generation` to `Ported` (only if no open debt remains), update
  the ADR 0017 status note, `docs/progress/phase.toml`, this prompt file, and add
  a session log entry.

Verification (backend/pipeline; no visual section):
- Focused `ersatzrs-infra` seed tests and any `ersatzrs-ffmpeg` snapshot tests
  touching the default profiles (`insta` is the regression gate).
- `cargo fmt --all -- --check`.
- `cargo clippy --workspace --all-targets -- -D warnings`.
- `cargo test --workspace --all-features`.
- `cargo xtask progress check`.

Report the Source Map, the ErsatzTV baseline files consulted at the pin, the
before/after seeded values, verification run, changed files, and any remaining
`ParityDebt`.

---

## 3. Fix the next ErsatzTV parity gap in ErsatzRS: Channels admin UI default channel group (slug `channels-admin-ui`, route `/channels/add`)

Use the repo workflow from AGENTS.md plus `skills/parity-slice.md`. The goal is
fully functional parity without stopgaps, closing the open debt
`channels-add-default-group-ersatzrs` in
[`docs/progress/parity/channels-admin-ui.toml`](progress/parity/channels-admin-ui.toml).

Current debt text: "Creating a channel at `/channels/add` shows `ErsatzTV` as
the default channel group instead of `ErsatzRS`." Reclose only with browser
proof that `/channels/add` pre-fills `Group` with `ErsatzRS`, save persistence
keeps that value for a new channel, and existing channel edit behavior is
unchanged.

Before editing:
1. Read `docs/progress/parity/channels-admin-ui.toml`,
   `docs/progress/hydration.toml`, `TEST.md`, and `STYLE.md`.
2. Read the ErsatzTV channel-edit baseline at the pinned commit
   `0d301df5e8512fdfccf3a611188d27ab1690ee31`
   (`ErsatzTV/Pages/Channel*.razor`, `ErsatzTV/ViewModels/ChannelEditViewModel.cs`)
   to confirm how the default channel group is constructed for a new channel.
3. Build a Source Map for the channel-add default construction in
   `crates/ersatzrs-core/src/repositories/channel.rs`, the SSR/hydrated add
   editor under `crates/ersatzrs-web/src/admin/pages/channels/`, and the save
   server function.
4. Build a Visual Source Map for `/channels/add` covering the Group field, its
   default value, and the supporting-lists source.

Implement the slice end-to-end:
- Make `/channels/add` pre-fill the default channel group with `ErsatzRS`.
- Ensure a save from that default persists `ErsatzRS` for the new channel and
  that editing an existing channel is unaffected.
- Add focused regression coverage for the default and its persistence.
- Preserve the SSR multipart fallback and follow `TEST.md` hydration rules.
- When browser proof passes, resolve `channels-add-default-group-ersatzrs`,
  return `channels-admin-ui` to `Ported` (only if no other channels-admin debt
  remains), refresh `docs/progress/hydration.toml`, `docs/progress/phase.toml`,
  this prompt file, and add a session log entry.

Visual verification:
- Serve via `./run.sh serve` after rebuilding Leptos assets.
- Capture `/channels/add` at a recorded viewport and confirm the Group field
  shows `ErsatzRS`, the page hydrates, and there are no console/page errors.

Verification:
- Focused Rust test for the channel default construction/persistence.
- `cargo xtask hydration-lint -p ersatzrs-web`.
- Browser or Playwright proof for `/channels/add`.
- `cargo xtask parity-check web` if shared channel-editor plumbing changes.
- `cargo xtask progress check`.

Report the Source Map, Visual Source Map, ErsatzTV baseline files consulted at
the pin, root cause, verification run, changed files, and any remaining parity
debt.

---
