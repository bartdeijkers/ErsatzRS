---
name: per-show-targeted-sync
description: Plex/Jellyfin/Emby single-show sync pattern in ersatzrs-infra and how scan_show dispatches.
metadata:
  type: project
---

ErsatzTV's `Synchronize{Plex,Jellyfin,Emby}ShowById` ports to a 3-piece pattern
in `ersatzrs-infra::{plex,jellyfin,emby}`:

1. **Pure refactor** — extract the per-show body of
   `sync_<p>_show_library_with_progress` (upsert show → page seasons → page
   episodes, including the `deep_scan`/`item_etag_matches` skip) into
   `async fn sync_one_<p>_show(...)`. The library sync calls the helper, so no
   existing tests/snapshots change.
2. **Single-item fetch** — add an inherent method on the concrete service
   struct (NOT the trait). Plex uses `/library/metadata/{ratingKey}` directly.
   Jellyfin/Emby use `/Items?parentId=<library>&ids=<show>&recursive=false&limit=1`
   and project the first hit. Return `Option<...ShowItem>`; `None` ⇒ deleted
   upstream and we no-op.
3. **`sync_<p>_show_by_id(pool, library_id, show_id, deep_scan, progress)`** —
   resolves the active connection + token via DB the same way
   `sync_library_by_id_with_progress` does, maps local `Show.Id` → provider
   key/item-id + title from `<Provider>Show` joined to `ShowMetadata` (mirrors
   ETV `GetShowTitleKey`/`GetShowTitleItemId`), fetches the one show, calls
   the per-show helper. Honours `should_sync_items = 0` with a warn-and-noop.

**Hydration:** none — this is all in `ersatzrs-infra`. The UI side just calls
`scan_show(library_id, show_id, kind, deep_scan)` in
`crates/ersatzrs-web/src/admin/server_fns/libraries.rs`, which match-dispatches
by `kind` and falls back to `scan_library` for not-yet-ported providers (Local
intentionally stays library-scan — matches ETV).

**Deferred at the call site** (don't silently drop):
`ExtractEmbeddedShowSubtitles` on deep scan is not ported; left as a
`tracing::debug!` after a successful deep scan in `scan_show`.

**Tests** live in `crates/ersatzrs-infra/tests/<p>_service.rs`. Seed two
`<Provider>Show` rows (target + other) so the test can assert only the target
is touched. Run the targeted sync three times: initial → shallow (etag skip,
MediaFile is left alone, no `PlaybackInfo` round-trip on Jellyfin/Emby) →
deep (re-sync overwrites). Call-count assertions on each mock catch any
accidental fan-out to siblings.
