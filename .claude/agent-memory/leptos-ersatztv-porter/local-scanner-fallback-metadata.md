---
name: local-scanner-fallback-metadata
description: Local scanner must insert fallback Movie/OtherVideo/Song/Image metadata or admin browser INNER-JOINs hide every scanned file.
metadata:
  type: project
---

The local scanner (`crates/ersatzrs-scanner/src/lib.rs`) writes the subtype
row (`Movie`/`OtherVideo`/`Song`/`Image`) plus `MediaVersion`/`MediaFile`, but
the admin browser server fns in
`crates/ersatzrs-web/src/admin/server_fns/media_browser.rs` query via
`INNER JOIN "<Subtype>Metadata"`. Without a metadata row the page renders
"No items to display." even after a successful scan.

**Why:** ErsatzTV inserts fallback metadata in
`FallbackMetadataProvider.GetFallbackMetadata*` whenever no NFO is present;
the Rust port omitted that step, so only Plex/Jellyfin/Emby-scanned items
(which write metadata in their own paths) showed up in the UI.

**How to apply:** Any new local-scan subtype (or any new browser query that
inner-joins on `*Metadata`) must call `insert_fallback_metadata` on the
insert branch of `upsert_local_file`. Title defaults to the file stem and
`OriginalTitle` is the path relative to the library root. Tags / artwork /
actors are deferred — folder-scanner parity work can enrich the row later.

MusicVideo and RemoteStream go through their own upsert paths and already
write metadata; do not double-insert for them.

**Idempotency / backfill:** `insert_fallback_metadata` is idempotent — it
short-circuits via `fallback_metadata_exists` when any metadata row already
references the item. `upsert_local_file` calls it from BOTH branches (newly
inserted MediaItem AND existing MediaFile match), so libraries that were
scanned before fallback metadata existed get backfilled on the next scan
without needing a manual SQL migration. Do not remove the existing-branch
call: items created by the pre-fix scanner are still in the wild.
