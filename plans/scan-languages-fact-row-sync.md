**TV-show-page parity gaps: per-show sync, playout-refresh, languages**

Status: Slice 0 done (this file). Slices 1–5 pending. Created 2026-05-18.

**Authoritative C# sources**

- Per-show scan dispatch: [$ERSATZTV_SRC/ErsatzTV.Application/Libraries/Commands/QueueShowScanByLibraryIdHandler.cs](../../ErsatzTV/ErsatzTV.Application/Libraries/Commands/QueueShowScanByLibraryIdHandler.cs)
- Single-show sync: `$ERSATZTV_SRC/ErsatzTV.Scanner/Application/{Plex,Jellyfin,Emby}/Commands/Synchronize*ShowByIdHandler.cs`, `$ERSATZTV_SRC/ErsatzTV.Scanner/Core/*/​*TelevisionLibraryScanner.cs::ScanSingleShow`
- Playout refresh worker: [$ERSATZTV_SRC/ErsatzTV/Services/WorkerService.cs](../../ErsatzTV/ErsatzTV/Services/WorkerService.cs), `ErsatzTV.Application/Playouts/Commands/BuildPlayout.cs`, `ErsatzTV.Core/Scheduling/PlayoutBuildMode.cs`
- Refresh triggers: `ErsatzTV.Application/ProgramSchedules/Commands/AddProgramScheduleItemHandler.cs`, `ErsatzTV.Application/MediaCollections/Commands/{AddItemsToCollectionHandler,RemoveItemsFromCollectionHandler}.cs`, `ErsatzTV.Infrastructure/Data/Repositories/MediaCollectionRepository.cs` (`PlayoutIdsUsingCollection`)
- Languages: `ErsatzTV.Infrastructure/Data/Repositories/SearchRepository.cs` (`GetLanguagesForShow`), `ErsatzTV.Core/Metadata/LanguageCodeService.cs`, `ErsatzTV.Infrastructure/Metadata/LanguageCodeCache.cs` (`ErsatzTV/Resources/ISO-639-2_utf-8.txt`), `ErsatzTV.Application/Television/Mapper.cs` (`LanguagesForShow`)

## Context

The TV show details page (`/media/tv/shows/:id`, shipped 2026-05-18) landed
with three documented parity gaps versus ErsatzTV. Close all three as proper
follow-up slices rather than inline notes:

1. **Per-show sync** — ErsatzTV's Scan button re-syncs *one* show
   (`SynchronizePlex/Jellyfin/EmbyShowById`); ErsatzRS currently re-scans the
   whole owning library (`scan_library`). Functional regression.
2. **Playout refresh on add** — ErsatzTV enqueues `BuildPlayout(Refresh)` for
   every playout that uses the changed schedule/collection; ErsatzRS persists
   correctly but never refreshes (only eventual consistency on the next
   scheduler tick). Also the pre-existing `collection_detail.rs` gap.
3. **Languages fact row** — ErsatzTV resolves show languages from episode
   media-stream codes and shows a "Languages" row; ErsatzRS omits it.

Outcome: full behavioural parity for the show page's Scan + Add-to-Schedule
actions, the collection add/remove refresh, and the metadata facts list.

## Conventions (apply to every slice)

- **PORTING.md**: read the ErsatzTV C# first; map idioms per its table
  (MediatR handler → async fn; LanguageExt `Either` → `Result`; hosted
  `WorkerService` → tokio task). Deliberate behavioural divergence ⇒ ADR +
  `docs/explanation/deviations.md`. None of these slices diverge behaviourally,
  so **no ADR expected**; mechanism mappings get a code comment citing the C#
  source path.
- **ARCHITECTURE.md** crate boundaries: provider/sql code in
  `ersatzrs-infra`, scheduling types/traits in `ersatzrs-core`, HTTP/server-fn
  in `ersatzrs-web`, scheduler loop in `ersatzrs-scheduler`. No cycles.
- **STYLE.md**: `PascalCase` quoted SQL identifiers, `PK_/FK_/IX_` names,
  preserve ErsatzTV JSON/casing contracts, `thiserror`/`anyhow` split.
- **TEST.md**: provider work ⇒ `httpmock` integration tests in
  `crates/<crate>/tests/`; pure logic ⇒ `#[cfg(test)]`; Leptos UI ⇒ stable DOM
  + manual Playwright; update the per-page E2E index row.
- **Gates per slice** (clippy-CI memory): `cargo fmt --all`,
  `cargo clippy --workspace --all-targets -- -D warnings` **and**
  `cargo clippy -p ersatzrs-web -p ersatzrs-infra --all-targets
  --features ersatzrs-web/mysql -- -D warnings`,
  `cargo test --workspace`. Scanner integration tests that probe ffmpeg need
  `LD_LIBRARY_PATH=/home/bart/.local/ffmpeg-8.1/lib` (project memory).
- **Session handoff** (AGENTS.md): each slice appends a STATUS.md session-log
  row (top of table) and updates "What an Agent Should Do Next";
  `docs/reference/parity-matrix.md` updated when a gap closes. Commit per slice
  with Conventional Commits **only when the user asks**.

---

## Slice 0 — Plan scaffolding (done)

- Create `plans/scan-languages-fact-row-sync.md` = this file.
- Index it in `plans/INDEX.md`.
- Add a STATUS.md "What an Agent Should Do Next" entry naming Slices 1–5.
- No code. Gate: docs only.

---

## Slice 1 — Languages fact row (smallest, isolated; finishes the show page)

**Implementation:**
- Vendor `/home/bart/ErsatzTV/ErsatzTV/Resources/ISO-639-2_utf-8.txt` into
  `assets/iso-639-2_utf-8.txt` (same data source ⇒ identical name resolution;
  faithful, no divergence).
- New `ersatzrs-core` module `language_codes` porting `LanguageCodeService` /
  `LanguageCodeCache`: parse the pipe-delimited file (`include_str!` the
  asset), build the bidirectional 3-letter/2-letter group lookup, expose
  `all_language_codes(codes) -> Vec<String>` and
  `english_name(code) -> Option<String>` (the `CultureInfo.EnglishName`
  equivalent — the ISO-639-2 file already carries the English-name column;
  use it instead of .NET `CultureInfo` so resolution is data-driven and
  platform-independent — note this in a code comment as the mechanism map).
- `get_show_details` (`crates/ersatzrs-web/src/admin/server_fns/media_browser.rs`):
  add `languages: Vec<String>` to `ShowDetailRow`; SQL mirrors
  `GetLanguagesForShow` (`SELECT DISTINCT ms."Language" FROM "MediaStream" ms
  JOIN "MediaVersion" mv ON mv."Id"=ms."MediaVersionId" JOIN "Episode" e ON
  e."Id"=mv."EpisodeId" JOIN "Season" s ON s."Id"=e."SeasonId" WHERE
  ms."MediaStreamKind"=2 AND s."ShowId"=?`), then
  `language_codes::all_language_codes` → `english_name`, sorted, deduped.
- `crates/ersatzrs-web/src/admin/pages/media/shows.rs`: render a "Languages"
  `fact_row` (reuse existing `fact_row` helper); remove the deferred comment.
- Reuse `english_name` later from the search `language:` path if convenient
  (not required this slice).

**Tests:** `ersatzrs-core` unit tests for `language_codes` (e.g.
`eng`/`en` → group, `english_name("nld") == "Dutch"`); an `ersatzrs-infra`
or web integration test seeding Show→Season→Episode→MediaVersion→MediaStream
asserting the resolved list. Update TEST.md `/media/tv/shows/:id` row note.

---

## Slice 2 — Playout-refresh channel

**Parity caveat:** ErsatzTV's `AddItemsToPlaylistHandler` does **not** enqueue
— so ErsatzRS must **not** add playlist refresh (match the source).

**Mechanism (parity-faithful, fits ErsatzRS):** in-process
`tokio::sync::mpsc` channel; the **receiver is consumed inside
`run_scheduler`'s `tokio::select!` loop** so the scheduler stays the *single*
playout builder (mirrors the one `WorkerService`) and reuses its existing
`EntityLocker` + pool — avoids a second builder racing the periodic loop.
This is a mechanism map (MediatR+hosted-service → mpsc+tokio task), not a
behavioural change ⇒ no ADR; cite `WorkerService.cs` in comments.

**Implementation:**
- `ersatzrs-core/src/scheduling/mod.rs`: add
  `pub struct PlayoutBuildRequest { pub playout_id: EntityId, pub mode:
  PlayoutBuildMode }` (reuses existing `PlayoutBuildMode`, which already has
  `Refresh`).
- `ersatzrs-infra/src/maintenance.rs`: make the existing private
  `build_playout` reusable — add
  `pub async fn run_requested_playout_build(pool, locker, req, now)` that
  takes the `PlayoutLockGuard` then calls the existing `build_playout` (no
  logic duplication; the periodic loop keeps using it unchanged).
- `ersatzrs-scheduler/src/runner.rs`: `SchedulerOptions` gains
  `build_requests: Option<mpsc::Receiver<PlayoutBuildRequest>>`; add a
  `select!` arm draining it via `run_requested_playout_build` with the
  loop's `locker`/pool.
- `crates/ersatzrs-web/src/host/state.rs`: `AppState` gains
  `playout_build_tx: tokio::sync::mpsc::Sender<PlayoutBuildRequest>` (bounded;
  `try_send`, log-and-drop on full — periodic loop is the safety net).
- `host/bootstrap.rs` + `host/tasks.rs`: create the channel before
  `build_app_state`, store `tx` in `AppState`, thread `rx` into
  `run_scheduler` via `SchedulerOptions`.
- New `ersatzrs-web` helper `enqueue_playout_refresh`:
  - schedule: `SELECT "Id" FROM "Playout" WHERE "ProgramScheduleId" = ?`
  - collection: `SELECT DISTINCT "PlayoutId" FROM
    "PlayoutProgramScheduleAnchor" WHERE "CollectionId" = ?`
  (mirrors ErsatzTV's two lookups; `IX_Playout_ProgramScheduleId` exists).
- Hook after successful persist in
  `crates/ersatzrs-web/src/admin/server_fns/media_browser.rs`:
  `add_show_to_schedule` (schedule lookup),
  `add_media_items_to_collection` + the collection
  remove/`set_collection_use_custom_order`/`update_collection_custom_order`
  paths used by `collection_detail.rs` (collection lookup). Each sends
  `PlayoutBuildRequest{ id, PlayoutBuildMode::Refresh }`.
- Update the deferred-comment blocks in `shows.rs` and `collection_detail.rs`.

**Tests:** unit test the playout-lookup SQL (seed Playout +
PlayoutProgramScheduleAnchor); a scheduler test that feeds a request through
the channel and asserts `build_playout` runs (or is lock-skipped). Existing
periodic-build tests must stay green (no behaviour change to `build_playout`).

---

## Slices 3–5 — Per-show targeted sync (one slice per provider)

**Shared design (done in Slice 3, reused by 4–5):**
- `ersatzrs-infra` {plex,jellyfin,emby}.rs: **pure refactor** — extract the
  per-show body of `sync_<p>_show_library_with_progress` (upsert show → loop
  seasons → loop episodes, incl. the `deep_scan`/`item_etag_matches` skip)
  into `async fn sync_one_<p>_show(...)`; the existing library sync calls it
  (no behaviour change → keeps provider tests/snapshots green).
- Add a single-show fetch to each provider service mirroring ETV
  `GetSingleShow` (Plex `/library/metadata/{ratingKey}` + children; Jellyfin/
  Emby `Items/{itemId}`); map local `Show.Id` → provider key/title from DB
  (mirrors ETV `GetShowTitleKey`).
- New infra entry `pub async fn sync_<p>_show_by_id(pool, library_id,
  show_id, deep_scan, progress)` — resolves connection/config like
  `sync_library_by_id_with_progress`, fetches the one show, calls
  `sync_one_<p>_show`.
- `crates/ersatzrs-web/src/admin/server_fns/libraries.rs`: new
  `scan_show(library_id, show_id, kind, deep_scan)` server fn — same
  `entity_locker.lock_library` guard + progress sink as `scan_library`;
  dispatch by `kind` to `sync_<p>_show_by_id`; **for not-yet-ported providers
  fall back to `scan_library`** so the button always works.
- `crates/ersatzrs-web/src/admin/pages/media/shows.rs`: Scan buttons call
  `scan_show(library_id, show_id, kind, deep)` instead of `scan_library`
  (`show_id` is already on `ShowDetailRow`). Update the deferred comment.
- Carry forward (still deferred, documented in code, **not silently
  dropped**): `ExtractEmbeddedShowSubtitles` on deep scan (subtitle
  extraction unported); Local single-show stays library-scan (matches ETV).

**Slice 3 = refactor + `scan_show` + UI rewire + Plex end-to-end.**
**Slice 4 = Jellyfin single-show.** **Slice 5 = Emby single-show.**

**Tests per provider:** `httpmock` integration test in
`crates/ersatzrs-infra/tests/<p>_service.rs` mirroring existing patterns: mock
single-show + its seasons/episodes, assert only that show's rows upsert, and
that non-deep re-run is etag-skipped while deep re-syncs. Update
`docs/reference/parity-matrix.md` row when all three land.

---

## End-to-end verification

1. Per slice: the gate commands above (fmt, both clippy passes,
   `cargo test --workspace`).
2. Slice 1: `cargo leptos build`; load `/media/tv/shows/<id>` and confirm a
   "Languages" row with English names for a show whose episodes have audio
   language streams.
3. Slice 2: with a Playout bound to a ProgramSchedule, call Add-to-Schedule
   on the show page; confirm a `Refresh` build runs (scheduler log line /
   `PlayoutBuildStatus.LastBuild` advances) without waiting a full tick;
   repeat via a collection add from `collection_detail`.
4. Slices 3–5: point at a Plex/Jellyfin/Emby source, Quick/Deep Scan from the
   show page, confirm only that show's seasons/episodes are re-synced (logs +
   row counts) and the library lock is held/released.
5. Server runtime note: this sandbox kills long-lived servers; verify with a
   `timeout`-bounded boot + pre-queued curls or defer live UI checks to the
   user.

## Parity status after completion

All three gaps closed with faithful behaviour. Only remaining documented
deferral: `ExtractEmbeddedShowSubtitles` on deep show scan (subtitle
extraction subsystem unported) — tracked in code + parity-matrix, not silently
dropped.
