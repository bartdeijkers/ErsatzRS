# ADR 0020: Rebuild impacted playouts when channels, schedules, or library content change

## Status

Accepted 2026-07-03

## Implementation

Partially implemented; reopened on 2026-08-23. The scheduler core treats
`Refresh` as a window rewrite request for classic, YAML, and scripted playouts.
Classic builds consume `PlayoutBuildMode`: `Continue` keeps the normal main
anchor path, `Refresh` clears generated items while ignoring the stale main
anchor and retaining same-day checkpoint anchors, and `Reset` starts with no
retained anchors. Channel saves do not enqueue a playout build, matching
ErsatzTV's `UpdateChannelHandler`; they retain channel/EPG side effects and
embedded-subtitle extraction. The Playouts admin reset path keeps ErsatzTV's
mapping (classic Reset Playout enqueues `Refresh`; block, sequential, and
scripted playouts enqueue `Reset`).

Classic continuity was completed on 2026-08-24. Fresh builds now persist one
dated per-source anchor set and one versioned `PlayoutClassicCheckpoint` at the
initial rebuild seam and each subsequent day seam, alongside the undated
Continue anchors. The complete checkpoint includes schedule order and state,
source ordering keyed by stable playback identity, fill groups, Smart Shuffle,
Random, and Random Rotation context. Refresh validates the whole set before the
transactional clear, restores it across process restart, reconciles membership
without shifting a raw index onto consumed shuffled media, and re-stamps mutable
settings. Legacy, partial, wrong-day, duplicate, or incompatible state is
refused with a failing build/health diagnostic that requires explicit Reset;
the existing window remains untouched. The remaining reopened work is crf-02:
the scanner still selects every playout with any program-schedule anchor as
affected by Remote Stream membership instead of resolving real dependencies.

## Context

A playout is a materialized timeline: rows in `PlayoutItem` that each carry a
snapshot of the settings that were in effect when the item was scheduled. Stream
generation reads those baked snapshots, not the live configuration. Concretely,
the subtitle mode is resolved at stream time as
`playout_item.subtitle_mode.unwrap_or(channel.subtitle_mode)` (a faithful port of
ErsatzTV `GetPlayoutItemProcessByChannelNumberHandler.cs:450`,
`playoutItem.SubtitleMode ?? channel.SubtitleMode`), and the per-item value is
copied from the schedule item at build time
(`scheduling/classic/mod.rs` `item.subtitle_mode = schedule_item.subtitle_mode`).
The same "snapshot at build time" property holds for preferred subtitle/audio
language, watermark/graphics/deco selection, filler, and the resolved media
items themselves.

This means a settings change only takes effect once the affected `PlayoutItem`
rows are rewritten. Today the rebuild triggers are incomplete and, for the most
common engine, ineffective:

- **Schedule edits** enqueue `PlayoutBuildMode::Refresh` for every playout using
  the schedule (`server_fns/schedules.rs`, `server_fns/media_browser.rs`
  `enqueue_schedule_playout_refresh`). This matches ErsatzTV, which writes
  `BuildPlayout(..., Refresh)` from its schedule-item handlers.
- **Library/collection edits** (collection, multi-collection, smart-collection,
  and rerun-collection membership changes) enqueue `Refresh` for playouts whose
  schedules reference them (`media_browser.rs`
  `enqueue_*_collection_playout_refresh`).
- **Channel edits** enqueue no playout build. This matches ErsatzTV's
  `UpdateChannelHandler`, which saves channel data, refreshes channel/EPG
  metadata, and may extract embedded subtitles without calling `BuildPlayout`.

Even where `Refresh` is enqueued, ErsatzRS does not rebuild existing items the
way ErsatzTV does. In ErsatzTV `PlayoutBuilder.RefreshPlayout` sets
`ClearItems = true` and nulls the main anchor, then rebuilds the whole window
while preserving *checkpoint* program-schedule anchors so shuffle/enumerator
continuity is kept. In ErsatzRS:

- `clear_items` is set only for `PlayoutBuildMode::Reset`
  (`scheduling/yaml/mod.rs`, `scheduling/scripted/mod.rs`).
- The **classic** engine ignores `request.mode` entirely: it branches on whether
  `request.playout.anchor` is present (`scheduling/classic/mod.rs`), trims only
  items older than `now - 4h`, and appends from the anchor's far-future
  `next_start`. Classic `Refresh` therefore behaves like ErsatzTV `Continue`
  (append-only) — already-scheduled items, including the one on air, are never
  rewritten. Classic is also the kind the "Reset Playout" button maps to
  `Refresh` for (`server_fns/playouts.rs`, mirroring `Playouts.razor`), so today
  neither editing the schedule nor clicking Reset reliably re-stamps near-term
  classic items with new settings.

The observed symptom that motivated this ADR: a channel's subtitle mode was
changed from Forced to Any and a Dutch sidecar still would not burn in, because
all already-scheduled classic items retained `SubtitleMode = Forced` and no
rebuild rewrote them.

Constraints:

- Preserve ErsatzTV behavioral parity, including deterministic shuffle continuity
  across a rebuild (a rebuild must not reshuffle already-watched content or jump
  the enumerator).
- Do not disrupt an item that is currently streaming more than ErsatzTV does; the
  on-air segment's own transcode is already running and is only re-derived on the
  next item boundary.
- Avoid rebuild storms: a bulk library scan or a multi-field channel save must
  coalesce into at most one build request per affected playout.

## Decision

Adopt a single, explicit policy: **whenever a change can alter what a playout's
items should contain, the impacted playouts are rebuilt so their materialized
items reflect the change, not just extended past it.**

1. **Trigger matrix.** The following edits enqueue a build for every impacted
   playout (a playout is impacted when it belongs to the changed channel, uses
   the changed schedule, or resolves content from the changed collection/library
   set):
   - Schedule and schedule-item edits (already wired; keep).
   - Library/collection content changes that a schedule resolves —
     collection/multi-collection/smart-collection/rerun membership, and library
     scans that add, remove, or re-identify items in a referenced source
     (already partially wired for collection edits; extend to scan completion).

2. **Rebuild, don't just append.** `Refresh` must rewrite the current build
   window, not only extend it. Port ErsatzTV `RefreshPlayout` semantics to every
   engine, including classic: clear the window's items, null the main anchor,
   retain checkpoint program-schedule/collection anchors so enumerator state and
   shuffle continuity carry across the rebuild. The classic engine must consume
   `request.mode` instead of inferring intent from anchor presence.

3. **Mode selection.** Content-preserving edits use `Refresh` (keep continuity);
   edits that invalidate enumerator state, and the explicit "Reset" action, use
   `Reset` (wipe anchors, fresh shuffle). Channel/schedule/library edits covered
   here default to `Refresh`.

4. **Coalescing and safety.** Build requests for the same playout are deduplicated
   while queued; a bulk operation (library scan, multi-field save) yields one
   request per playout. Builds respect the existing per-playout lock
   (`EntityLocker` / `PlayoutLockGuard`) and never interrupt the on-air item's
   running transcode — the change is picked up at the next item boundary, as in
   ErsatzTV.

5. **User feedback.** Because a rebuild is asynchronous and bounded to the build
   window, the admin UI should indicate that a settings change triggers a playout
   rebuild, so operators do not expect an instant change to content already on
   air.

## Consequences

- Channel saves stay lightweight and do not unexpectedly rewrite an operator's
  playout window; operators use the explicit per-playout or Reset All action
  when they want regenerated items.
- Classic gains real `Refresh`/`Reset` semantics, removing a silent parity
  divergence and making the "Reset Playout" button behave as documented.
- One correctness-sensitive area is enumerator continuity: the classic rebuild
  must preserve checkpoint anchors exactly like ErsatzTV, or a rebuild could
  reshuffle or repeat content. This needs focused scheduler tests asserting that a
  `Refresh` after a settings change reproduces the same media order from the
  checkpoint while re-stamping per-item settings.
- Rebuild frequency rises after impacting schedule or library changes. Channel
  saves still do not enqueue a build. Coalescing plus the existing lock keep
  this bounded; library scans must batch rather than enqueue per item.
- A Refresh without a restorable checkpoint is unsafe: it can turn a
  deterministic shuffle into a replay. Existing databases now take the
  observable refusal path unless a complete versioned checkpoint is present;
  they are never silently treated as a fresh Reset.
- The on-air item is unchanged until its boundary; operators watching live see the
  change one item later, matching ErsatzTV and avoiding mid-item restarts.

## Alternatives Considered

- **Resolve settings live at stream time instead of baking them.** Read subtitle
  mode, languages, watermark, etc. from the current channel/schedule when the item
  streams, ignoring the baked snapshot. Rejected: it diverges from ErsatzTV's
  `playoutItem.SubtitleMode ?? channel.SubtitleMode` snapshot model, breaks
  imported-playout reproducibility, and would still leave resolved *media* stale
  (the item list itself is materialized), so a rebuild path is required regardless.
- **Automatically rebuild on channel save.** Rejected: it diverges from
  ErsatzTV and makes ordinary channel editing unexpectedly expensive; explicit
  playout refresh actions already provide the intended operator control.
- **Full `Reset` on every impacting change.** Rejected: `Reset` wipes enumerator
  anchors and reshuffles, which would visibly reorder upcoming content after a
  trivial edit. `Refresh` preserves continuity and is the ErsatzTV choice for
  these edits.

## References

- `crates/ersatzrs-web/src/admin/server_fns/schedules.rs` — schedule-save Refresh
  enqueue.
- `crates/ersatzrs-web/src/admin/server_fns/media_browser.rs` — collection/schedule
  refresh helpers (`enqueue_playout_refreshes`, `enqueue_*_playout_refresh`).
- `crates/ersatzrs-web/src/admin/server_fns/channels.rs` — channel save parity
  (no playout build enqueue).
- `crates/ersatzrs-web/src/admin/server_fns/playouts.rs` — `reset_playout` /
  `reset_all_playouts` (classic → Refresh mapping).
- `crates/ersatzrs-core/src/scheduling/classic/mod.rs` — anchor-based build,
  `remove_before = now - 4h`, `item.subtitle_mode = schedule_item.subtitle_mode`.
- `crates/ersatzrs-core/src/scheduling/{yaml.rs,scripted.rs}` —
  `clear_items = mode == Reset`.
- `crates/ersatzrs-infra/src/maintenance.rs` — `build_playout`,
  `run_requested_playout_build`, daily Reset vs Continue.
- `$ERSATZTV_SRC/ErsatzTV.Core/Scheduling/PlayoutBuilder.cs` — `RefreshPlayout`
  (`ClearItems = true`, null anchor, keep checkpoint anchors) vs `ResetPlayout`.
- `$ERSATZTV_SRC/ErsatzTV.Application/ProgramSchedules/Commands/*Handler.cs` —
  `BuildPlayout(..., Refresh)` on schedule changes.
- `$ERSATZTV_SRC/ErsatzTV.Application/Streaming/Queries/GetPlayoutItemProcessByChannelNumberHandler.cs:450`
  — `playoutItem.SubtitleMode ?? channel.SubtitleMode`.
- Related: [[project-playout-baked-subtitle-settings]] memory; ADR 0019
  (scheduling continuity/anchor state).

## Revision history

- 2026-07-01: Original decision, prompted by a channel subtitle-mode change not
  reaching already-scheduled classic items.
- 2026-07-20: Corrected channel-save behavior to match ErsatzTV: no automatic
  playout rebuild; manual requests now have reliable queued-work feedback.
- 2026-08-23: Reopened implementation after Windows dogfood showed that real
  Classic builds create no dated Refresh checkpoints and Remote Stream scan
  impact selection can enqueue unrelated playouts.
- 2026-08-24: Completed Classic checkpoint generation, restart restoration,
  stable membership reconciliation, destructive preflight, legacy refusal,
  transactional SQLite/MySQL persistence, and health coverage. Dependency-
  scoped scan invalidation remains under crf-02.
