# ADR 0020: Rebuild impacted playouts when channels, schedules, or library content change

## Status

Accepted 2026-07-03

## Implementation

Implemented. The scheduler core treats `Refresh` as a window rewrite request for
classic, YAML, and scripted playouts. Classic builds consume
`PlayoutBuildMode`: `Continue` keeps the normal main anchor path, `Refresh`
clears generated items while ignoring the stale main anchor and retaining the
same-day checkpoint anchors needed for enumerator/shuffle continuity, and
`Reset` starts with no retained anchors. Channel saves compare the
playback-impacting channel signature (FFmpeg profile, subtitle/audio
preferences, watermark, streaming mode, and playout offset) and enqueue one
coalesced `Refresh` build per affected playout when that signature changes. The
Playouts admin reset path keeps ErsatzTV's mapping (classic Reset Playout
enqueues `Refresh`; block, sequential, and scripted playouts enqueue `Reset`),
and classic `Refresh` clears the near-term generated rows it previously left
stale. Focused scheduler, channel-save, reset, and maintenance tests cover the
three closeout slices; the post-sequence gates passed on 2026-07-03, including
workspace clippy, the MySQL clippy pass, workspace all-features tests,
`cargo xtask parity-check web`, and `cargo xtask smoke`.

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
- **Channel edits** enqueue nothing. `server_fns/channels.rs` saves the row and
  returns; no build is requested. The channel's subtitle mode, preferred
  languages, watermark, and streaming settings therefore never reach
  already-scheduled items.

Even where `Refresh` is enqueued, ErsatzRS does not rebuild existing items the
way ErsatzTV does. In ErsatzTV `PlayoutBuilder.RefreshPlayout` sets
`ClearItems = true` and nulls the main anchor, then rebuilds the whole window
while preserving *checkpoint* program-schedule anchors so shuffle/enumerator
continuity is kept. In ErsatzRS:

- `clear_items` is set only for `PlayoutBuildMode::Reset`
  (`scheduling/yaml.rs`, `scheduling/scripted.rs`).
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
   - Channel edits that affect playback selection or stream shape (subtitle mode,
     preferred subtitle/audio language and title, watermark, streaming mode,
     FFmpeg profile, playout offset). ErsatzRS currently enqueues nothing here;
     this is the primary gap to close.
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

- Editing a channel's subtitle/language/watermark/streaming settings takes effect
  on upcoming items without a manual Reset, closing the reported bug class.
- Classic gains real `Refresh`/`Reset` semantics, removing a silent parity
  divergence and making the "Reset Playout" button behave as documented.
- One correctness-sensitive area is enumerator continuity: the classic rebuild
  must preserve checkpoint anchors exactly like ErsatzTV, or a rebuild could
  reshuffle or repeat content. This needs focused scheduler tests asserting that a
  `Refresh` after a settings change reproduces the same media order from the
  checkpoint while re-stamping per-item settings.
- Rebuild frequency rises (channel saves now build). Coalescing plus the existing
  lock keep this bounded; library scans must batch rather than enqueue per item.
- The on-air item is unchanged until its boundary; operators watching live see the
  change one item later, matching ErsatzTV and avoiding mid-item restarts.

## Alternatives Considered

- **Resolve settings live at stream time instead of baking them.** Read subtitle
  mode, languages, watermark, etc. from the current channel/schedule when the item
  streams, ignoring the baked snapshot. Rejected: it diverges from ErsatzTV's
  `playoutItem.SubtitleMode ?? channel.SubtitleMode` snapshot model, breaks
  imported-playout reproducibility, and would still leave resolved *media* stale
  (the item list itself is materialized), so a rebuild path is required regardless.
- **Leave it manual (status quo) and only document "Reset the playout".**
  Rejected: the reported confusion shows operators reasonably expect a saved
  setting to apply, and for classic the Reset button does not even rewrite
  near-term items today.
- **Full `Reset` on every impacting change.** Rejected: `Reset` wipes enumerator
  anchors and reshuffles, which would visibly reorder upcoming content after a
  trivial edit. `Refresh` preserves continuity and is the ErsatzTV choice for
  these edits.

## References

- `crates/ersatzrs-web/src/admin/server_fns/schedules.rs` — schedule-save Refresh
  enqueue.
- `crates/ersatzrs-web/src/admin/server_fns/media_browser.rs` — collection/schedule
  refresh helpers (`enqueue_playout_refreshes`, `enqueue_*_playout_refresh`).
- `crates/ersatzrs-web/src/admin/server_fns/channels.rs` — channel save (no build
  enqueue; gap).
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
