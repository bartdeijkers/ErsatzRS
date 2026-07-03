<!--
generated: 2026-07-03
last_processed_commit: f02ea8454534b68295ce25df3b047cf5559d036a
parity_baseline_commit: 0d301df5e8512fdfccf3a611188d27ab1690ee31
generator: docs/automation/daily-progress-routine.md (ersatzrs-daily-progress)
rules: docs/automation/gap-prompt-rules.md
-->

# Top three chores to do - goal prompts

Actionable paste-and-run goal prompts, each closing one ErsatzTV parity gap,
ADR-backed runtime hardening gap, or spec-backed chore. These prompts are
selected from the current progress files, ADR index, active plans, and phase
state per [gap-prompt-rules.md](automation/gap-prompt-rules.md).

Current selection: renewed 2026-07-03. The 149b4cb..f02ea84 change window was
dependency bumps only (leptos 0.8.20, leptos_router 0.8.14, jsonschema
0.46.8/0.46.9, open 5.3.6) plus one docs-housekeeping commit, so no parity
status moved. Applying the selection rules, the three `Not started`
`playout-rebuild-on-impacting-changes` debts (ADR 0020) now lead the queue:
each is confirmed user-visible runtime debt (a subtitle-mode change that never
burns because scheduled items keep the old settings and no rebuild is
requested; classic `Refresh` appending instead of clearing and rebuilding;
Reset Playout leaving near-term items stale), and the
confirmed-user-visible-runtime-debt rule outranks everything except phase
focus — above the ADR 0048 duplicate-collection-row and the two media-browser
UX chores that filled the previous queue. Those three UX debts
(`collection-item-duplicate-playback-overrides`,
`add-to-collection-playlist-stay-in-context`,
`artist-detail-music-video-multi-select-actions`) stay open and are the next
candidates, followed by ADR 0019 similar-video cooldown, proposed localization
ADRs, active Docker release plans, and module-split work. The three prompts are
ordered foundational-first: the engine Refresh/Reset semantics unblock the
channel-save enqueue and the Reset Playout rewrite.

Sequential closeout status (2026-07-03): gap 1's scheduler-core
Refresh/Reset semantics have been implemented and source-reviewed in the active
sequence. Verification commands are intentionally deferred until gap 3 has been
committed, per the combined goal. Continue with gap 2 next; do not restart gap
1 unless the deferred verification later contradicts the review.

---

## 1. Reviewed in sequence: ADR 0020 classic Refresh/Reset rebuild semantics (slug `playout-rebuild-on-impacting-changes`)

Use the repo workflow from AGENTS.md and `skills/parity-slice.md`. This is an
ErsatzTV parity gap governed by
[ADR 0020](adr/0020-playout-rebuild-on-impacting-changes.md); read ErsatzTV
source at the pinned baseline commit
`0d301df5e8512fdfccf3a611188d27ab1690ee31`
([parity-baseline.md](automation/parity-baseline.md)). The goal is fully
functional ErsatzTV `RefreshPlayout`/`ResetPlayout` parity without stopgaps;
record any shortfall as visible `ParityDebt`, never hide it in prose.

Tracked debt: `classic-refresh-appends-instead-of-rebuilding` in
[`docs/progress/parity/playout-rebuild-on-impacting-changes.toml`](progress/parity/playout-rebuild-on-impacting-changes.toml).
Current debt text: the classic scheduling engine ignores `request.mode` and
branches only on `request.playout.anchor` presence, trimming items older than
`now - 4h` and appending from the anchor `next_start`; `clear_items` is set
only for Reset in yaml/scripted and never in classic. Classic `Refresh`
therefore behaves like ErsatzTV `Continue` (append-only) instead of ErsatzTV
`RefreshPlayout` (ClearItems=true + null main anchor + retained checkpoint
anchors), so a schedule/library/channel change does not re-stamp existing
classic items. This is the foundational fix that unblocks prompts 2 and 3.

Before editing:
1. Read `docs/adr/0020-playout-rebuild-on-impacting-changes.md`, `STYLE.md`,
   and `docs/how-to/testing.md`.
2. Read ErsatzTV source at the pin:
   `$ERSATZTV_SRC/ErsatzTV.Core/Scheduling/PlayoutBuilder.cs` — study
   `RefreshPlayout` (sets `ClearItems=true`, nulls the main anchor, retains
   checkpoint anchors) and `ResetPlayout` (also `ClearItems=true`), and how
   `PlayoutBuildMode.Refresh`/`Reset` dispatch differs from `Continue`.
3. Inspect the current ErsatzRS engines:
   `crates/ersatzrs-core/src/scheduling/classic/mod.rs` (no `request.mode`
   use; anchor-presence branching), `crates/ersatzrs-core/src/scheduling/yaml.rs`
   and `scripted.rs` (`clear_items` only when `mode == Reset`).
4. Build a Source Map covering build-mode dispatch, `clear_items` handling,
   anchor/checkpoint retention, and enumerator/shuffle continuity across a
   rebuild.

Implement the coherent end-to-end slice that genuinely closes the gap:
- Make classic consume `request.mode`; `Refresh` clears and rebuilds the
  window while preserving checkpoint anchors and nulling the main anchor, per
  `RefreshPlayout`.
- Align yaml/scripted so `Refresh` and `Reset` semantics match ErsatzTV across
  every engine behind the shared trait.
- Preserve enumerator/shuffle continuity so a rebuild re-stamps per-item
  settings without reordering media from the checkpoint.
- Add scheduler tests asserting identical media order from the checkpoint with
  re-stamped per-item settings after a `Refresh`.
- Update `docs/progress/parity/playout-rebuild-on-impacting-changes.toml`
  (resolve this debt only when it fully closes; advance `status` toward
  `In progress`), ADR 0020 implementation text, `CHANGELOG.md`, this prompt
  file, and the session log.

Verification:
- Focused `cargo test -p ersatzrs-core` scheduling tests covering Refresh
  clear-and-rebuild with checkpoint retention.
- Snapshot coverage (`insta`) for any changed generated scheduling output.
- `cargo xtask progress check`.

When done, report the Source Map, behavior implemented, any remaining
`ParityDebt`, verification run, and changed files.

---

## 2. Fix the next ErsatzRS parity-infrastructure gap: channel save must enqueue playout Refresh (slug `playout-rebuild-on-impacting-changes`)

Use the repo workflow from AGENTS.md and `skills/parity-slice.md`. This is an
ErsatzTV parity gap governed by
[ADR 0020](adr/0020-playout-rebuild-on-impacting-changes.md); read ErsatzTV
source at the pinned baseline commit
`0d301df5e8512fdfccf3a611188d27ab1690ee31`. The goal is fully functional
ErsatzTV channel-edit rebuild parity without stopgaps; record any shortfall as
visible `ParityDebt`. This slice depends on prompt 1's engine Refresh
semantics actually re-stamping items.

Tracked debt: `channel-edit-triggers-no-playout-rebuild` in
[`docs/progress/parity/playout-rebuild-on-impacting-changes.toml`](progress/parity/playout-rebuild-on-impacting-changes.toml).
Current debt text: saving a channel enqueues no playout build
(`server_fns/channels.rs`), so changes to subtitle mode, preferred
subtitle/audio language and title, watermark, streaming mode, FFmpeg profile,
or playout offset never reach already-scheduled items. Channel edits that
affect playback selection or stream shape must enqueue a Refresh for every
playout on that channel. Confirmed runtime evidence: channel Subtitle Mode
changed Forced -> Any but a Dutch sidecar still did not burn because scheduled
items kept the old mode and no rebuild was requested on channel save.

Before editing:
1. Read `docs/adr/0020-playout-rebuild-on-impacting-changes.md` and `STYLE.md`.
2. Read ErsatzTV source at the pin:
   `$ERSATZTV_SRC/ErsatzTV.Application/Channels/Commands/UpdateChannelHandler.cs`
   — study which channel changes enqueue a playout rebuild and how the Refresh
   is requested per affected playout.
3. Inspect the current ErsatzRS save path:
   `crates/ersatzrs-web/src/admin/server_fns/channels.rs` (no rebuild enqueue)
   and `crates/ersatzrs-infra/src/maintenance.rs` (the build queue it should
   feed).
4. Build a Source Map covering channel-save, the playout build queue, Refresh
   coalescing for multiple playouts, and which fields are playback-impacting.

Implement the coherent end-to-end slice that genuinely closes the gap:
- On channel save, enqueue a coalesced `Refresh` build for every playout bound
  to that channel when a playback-impacting field changed (subtitle mode,
  preferred subtitle/audio language and title, watermark, streaming mode,
  FFmpeg profile, playout offset).
- Coalesce so one save does not stack redundant builds.
- Add a test proving an upcoming playout item reflects a changed channel
  setting after save (paired with prompt 1's engine rebuild).
- Update the owning parity TOML (resolve
  `channel-edit-triggers-no-playout-rebuild` only when it fully closes), ADR
  0020 implementation text, `CHANGELOG.md`, this prompt file, and the session
  log.

Verification:
- Focused `cargo test -p ersatzrs-web` channel-save tests plus
  `cargo test -p ersatzrs-infra` queue/maintenance tests.
- `cargo xtask progress check`.

When done, report the Source Map, behavior implemented, any remaining
`ParityDebt`, verification run, and changed files.

---

## 3. Fix the next ErsatzRS parity-infrastructure gap: Reset Playout must rewrite near-term items (slug `playout-rebuild-on-impacting-changes`)

Use the repo workflow from AGENTS.md and `skills/parity-slice.md`. This is an
ErsatzTV parity gap governed by
[ADR 0020](adr/0020-playout-rebuild-on-impacting-changes.md); read ErsatzTV
source at the pinned baseline commit
`0d301df5e8512fdfccf3a611188d27ab1690ee31`. The goal is fully functional
ErsatzTV Reset Playout parity without stopgaps; record any shortfall as visible
`ParityDebt`. This slice depends on prompt 1's engine Refresh/Reset semantics.

Tracked debt: `reset-playout-must-rewrite-near-term-items` in
[`docs/progress/parity/playout-rebuild-on-impacting-changes.toml`](progress/parity/playout-rebuild-on-impacting-changes.toml).
Current debt text: clicking "Reset Playout" must rewrite ALL items in the build
window, including near-term/already-scheduled items, not just append future
ones. For classic playouts the button maps to `Refresh`
(`server_fns/playouts.rs`, mirroring `Playouts.razor`), which today does not
clear near-term items, so operators cannot force current settings onto content
already on the timeline (only the on-air segment itself may legitimately wait
for its next item boundary). Reset Playout and Reset All Playouts must clear
and rebuild the whole window so the change is visible on the next item.
Maintainer request (2026-07-01): "clicking Reset Playout must rewrite all
items, including near-term items."

Before editing:
1. Read `docs/adr/0020-playout-rebuild-on-impacting-changes.md`, `TEST.md`,
   and `STYLE.md`.
2. Read ErsatzTV source at the pin:
   `$ERSATZTV_SRC/ErsatzTV/Pages/Playouts.razor` — study how `ResetPlayout`
   and `Reset All Playouts` invoke the build, and
   `$ERSATZTV_SRC/ErsatzTV.Core/Scheduling/PlayoutBuilder.cs` `ResetPlayout`
   (`ClearItems=true`).
3. Inspect the current ErsatzRS path:
   `crates/ersatzrs-web/src/admin/server_fns/playouts.rs` (the Reset button
   mapping) and the classic engine's window handling after prompt 1.
4. Build a Source Map and Visual Source Map (the Playouts admin page) covering
   the Reset control, the build request it issues, and near-term item
   retention.

Implement the coherent end-to-end slice that genuinely closes the gap:
- Make Reset Playout (and Reset All Playouts) clear and rebuild the whole
  window, rewriting every future item — not just appending the tail — while
  leaving only the currently on-air segment to finish at its next item
  boundary.
- Keep the admin control hydrated and console-clean per `TEST.md` island
  rules.
- Add a browser/scheduler test proving a settings change applied via Reset
  appears on the next scheduled item across the near-term window.
- Update the owning parity TOML (resolve
  `reset-playout-must-rewrite-near-term-items` only when it fully closes; flip
  the feature to `Ported` only once all three ADR 0020 debts are resolved),
  `docs/progress/hydration.toml` if proof state changes, ADR 0020
  implementation text, `CHANGELOG.md`, this prompt file, and the session log.

Visual verification:
- Serve via `./run.sh serve` after rebuilding Leptos assets and confirm the
  Reset Playout control behaves as above; capture same-viewport evidence.

Verification:
- Focused `cargo test -p ersatzrs-web` / `cargo test -p ersatzrs-core`
  scheduling tests for the Reset window rewrite.
- `cargo xtask hydration-lint -p ersatzrs-web` if the Playouts island changes.
- `npx playwright test` for the Playouts reset flow.
- `cargo xtask progress check`.

When done, report the Source Map, Visual Source Map, behavior implemented, any
remaining `ParityDebt`, verification run, and changed files.

---
