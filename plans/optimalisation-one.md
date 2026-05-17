# Local TV Folder Etag And LibraryFolder Parity Plan

## Goal

Port the two local television scanner optimizations from ErsatzTV's
`TelevisionFolderScanner.cs` into ErsatzRS:

- **Folder-etag fast path:** compute and persist an etag for every season
  folder, then skip expensive episode work when the folder has not changed.
- **`LibraryFolder` row inserts:** persist scanned show and season folders in
  `LibraryFolder`, and link each episode `MediaFile` to its season
  `LibraryFolder`.

This is Phase 7 scanner hardening/parity work. It should preserve ErsatzTV
behavior by default and avoid a new ADR unless implementation proves a
deliberate deviation is necessary.

## Authoritative C# Sources

- `$ERSATZTV_SRC/ErsatzTV.Scanner/Core/Metadata/TelevisionFolderScanner.cs`
- `$ERSATZTV_SRC/ErsatzTV.Scanner/Core/Metadata/FolderEtag.cs`
- `$ERSATZTV_SRC/ErsatzTV.Infrastructure/Data/Repositories/LibraryRepository.cs`
- `$ERSATZTV_SRC/ErsatzTV.Core/Domain/Library/LibraryFolder.cs`
- `$ERSATZTV_SRC/ErsatzTV.Infrastructure/Data/Configurations/Library/LibraryFolderConfiguration.cs`

Relevant ErsatzTV behavior:

- `ScanFolder` creates a `LibraryFolder` row for each show folder. The show
  folder is not used by the show media item directly; it exists so season
  folders can point at it through `ParentId`.
- `ScanSeasons` computes `FolderEtag.CalculateWithSubfolders(seasonFolder)`,
  calls `GetOrAddFolder`, compares the stored `LibraryFolder.Etag`, and skips
  `ScanEpisodes` when the etag matches.
- Even when `etagMatch` is true, ErsatzTV still creates/loads the season and
  updates season metadata/poster before skipping episode scans.
- If previously trashed items exist under the season folder, ErsatzTV disables
  the skip and scans episodes so restored files can return to `Normal`.
- `ScanEpisodes` calls `UpdateLibraryFolderId`, which writes the season
  folder id to the episode head `MediaFile.LibraryFolderId`.
- After scanning, `CleanEtagsForLibraryPath` removes missing `LibraryFolder`
  rows only when no `MediaFile` references them and no child folder points at
  them.

## Current ErsatzRS State

The schema already has the required persistence columns in both backends:

- `LibraryFolder(Id, Etag, LibraryPathId, ParentId, Path)`
- `MediaFile.LibraryFolderId`
- indexes `IX_LibraryFolder_LibraryPathId`, `IX_LibraryFolder_ParentId`, and
  `IX_MediaFile_LibraryFolderId`

The current Rust implementation in
[`crates/ersatzrs-scanner/src/local_television.rs`](../crates/ersatzrs-scanner/src/local_television.rs)
explicitly documents the gap: it skips the folder-etag fast path and
`LibraryFolder` writes, so every scan walks/probes every episode even when the
season folder is unchanged. `sync_local_episode` inserts `MediaFile` rows
without `LibraryFolderId`, and existing-file updates do not repair it.

No schema migration is expected for this slice. Existing databases will be
backfilled naturally by the next local TV scan.

## Design

### Folder Etag

Add an ErsatzTV-compatible folder-etag helper in `local_television.rs` or a
small scanner-local module:

- `folder_etag(folder: &Path) -> String` mirrors `FolderEtag.Calculate`:
  list top-level files only, exclude `._*`, sort by full path string, append
  path + UTC last-write ticks, and return lowercase MD5 hex.
- `folder_etag_with_subfolders(folder: &Path) -> String` mirrors
  `CalculateWithSubfolders`: include top-level files, then sorted direct
  subfolders as `subfolder_path + folder_etag(subfolder)`.
- Add an MD5 dependency to `crates/ersatzrs-scanner/Cargo.toml` if no existing
  workspace crate supplies it. Keep SHA256 `MediaFile.PathHash` unchanged.
- Convert file modification times to .NET ticks
  (`unix_seconds * 10_000_000 + unix_subsec_100ns + 621355968000000000`) so
  imported ErsatzTV etags can match when paths and mtimes are the same.
- On metadata errors or inaccessible files, follow ErsatzTV's local filesystem
  style: log and keep scanning instead of failing the whole library.

### LibraryFolder Helpers

Implement scanner-local SQL helpers first; move them to an infra repository
only if another scanner slice needs them.

- `get_parent_folder_id(tx/pool, library_path_id, folder) -> Option<i64>`:
  resolve the immediate parent directory and look up a row with the same
  `LibraryPathId` and parent path.
- `get_or_add_library_folder(pool, library_path_id, parent_id, folder) ->
  LibraryFolderRow`: select by `LibraryPathId + Path`; insert when missing;
  update `ParentId` when the parent changed.
- `set_library_folder_etag(pool/tx, folder_id, etag)`: update the known row.
- `update_media_file_library_folder_id(tx, media_version_id or path_hash,
  library_folder_id)`: set `MediaFile.LibraryFolderId` when missing or stale.
- `clean_library_folders_for_path(pool, library_path_id)`: delete rows whose
  path no longer exists, ordered deepest-first, only when they have no
  `MediaFile` references and no child `LibraryFolder` rows.

Use quoted identifiers and backend-neutral SQL compatible with the existing
SQLite/MySQL abstraction. Do not add `ON CONFLICT` or SQLite-only syntax in
production scanner code.

### Scanner Flow

Update `sync_local_show_library` to mirror ErsatzTV's order:

1. For each show folder, get the parent id, create/update the show
   `LibraryFolder`, then run the existing `upsert_show` metadata/artwork path.
2. For each season folder, get the parent id, compute the etag with
   subfolders, create/update the season `LibraryFolder`, and cache
   `etag_match = known_folder.etag == current_etag`.
3. If any non-normal or trashed episode row under that season folder exists,
   force `etag_match = false`. ErsatzTV uses "all trashed items"; if ErsatzRS
   still lacks a dedicated trash model, query `MediaFile` joined through
   `MediaVersion`/`Episode`/`MediaItem` and treat `MediaItem.State != Normal`
   under the season path as the parity-preserving trigger.
4. Always run `upsert_season` so season rows, season metadata, and season
   poster refresh still happen on unchanged folders.
5. If `etag_match` is true, skip all episode probing/upsert work for that
   season and advance scan progress for the skipped episode count without
   increasing `LocalScanReport.scanned_files`.
6. If `etag_match` is false, scan episodes exactly as today, but pass the
   season `LibraryFolder` id into `sync_local_episode`.
7. After a successful season episode scan, update `LibraryFolder.Etag` to the
   current value.
8. After all shows are processed for a library path, call
   `clean_library_folders_for_path`.

Update `sync_local_episode`:

- Add `library_folder_id: i64` to the function input.
- New episode insert path:
  `INSERT INTO "MediaFile" ("LibraryFolderId", "MediaVersionId", "Path",
  "PathHash") VALUES (?, ?, ?, ?)`.
- Existing episode path:
  update `Episode.SeasonId`, `MediaItem.LibraryPathId`, `MediaItem.State`, and
  `MediaFile.LibraryFolderId` in the same transaction.
- Keep the existing stream/chapter/metadata/artwork behavior unchanged.

Progress/reporting:

- `LocalScanReport.scanned_files` should count episodes actually scanned or
  probed, not episodes skipped by the etag fast path.
- The progress sink should still reach 100%. Either call the progress callback
  for skipped episode files or change the callback contract to advance by a
  count.

## Implementation Order steps

1. [x] **Add etag helper and tests.**
   Port `FolderEtag.Calculate` and `CalculateWithSubfolders` into scanner-local
   Rust. Add fixture tests with sorted files, ignored `._*` files, direct
   subfolders, and modified mtimes. Include one test vector generated from
   ErsatzTV if possible.
2. [x] **Add `LibraryFolder` SQL helpers.**
   Add a private `LibraryFolderRow { id, path, parent_id, etag }` and helper
   functions in `local_television.rs` or a sibling module. Cover insert,
   lookup, parent repair, etag update, and clean-up behavior with SQLite tests.
3. [x] **Wire show/season folder rows.**
   Create the show folder before `upsert_show`; create the season folder before
   `upsert_season`. Preserve the current show/season metadata behavior.
4. [x] **Link episode `MediaFile` rows.**
   Pass the season folder id into `sync_local_episode` and write
   `MediaFile.LibraryFolderId` for both insert and existing-file update paths.
5. [ ] **Enable etag skip.**
   Compare the stored etag to the current etag after season folder creation.
   Skip only episode scanning, not season metadata/poster refresh. Update the
   etag only after the non-skipped episode scan succeeds.
6. [ ] **Restore/non-normal guard.**
   Add the query that disables the fast path when non-normal episode rows exist
   under the season folder. If a future trash table is added, replace this with
   exact `GetAllTrashedItems` parity.
7. [ ] **Clean stale folders.**
   Call the clean-up helper after a library path scan, deepest-first, matching
   ErsatzTV's `CleanEtagsForLibraryPath`.
8. [ ] **Document and verify.**
   Update the scanner row in
   [`docs/reference/parity-matrix.md`](../docs/reference/parity-matrix.md) and
   remove/replace the current gap comment in `local_television.rs`.

## Tests

Add focused scanner tests rather than broad end-to-end UI coverage:

- First scan of a show with one season and two episodes creates:
  one show `LibraryFolder`, one season `LibraryFolder`, two `MediaFile` rows
  linked to the season folder, and a non-null season folder etag.
- Second scan with unchanged folder etag skips `ffprobe`/episode upserts and
  leaves `scanned_files == 0` for that unchanged season while preserving season
  metadata refresh behavior.
- Touching or adding a file under the season folder changes the etag and causes
  only that season's episodes to be scanned.
- A non-normal episode row under an otherwise unchanged season disables the
  fast path and restores the item to `Normal`.
- Moving a season folder creates/updates the correct parent relationship and
  repairs `MediaFile.LibraryFolderId`.
- Removing a folder deletes stale unreferenced `LibraryFolder` rows but keeps
  rows still referenced by `MediaFile` or child folders.
- Run the SQLite scanner tests by default and add MySQL coverage when the
  repository's MySQL integration test setup is available.

## Acceptance Checks

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test --workspace --all-features`
- `cargo test -p ersatzrs-scanner local_television`
- Optional, when MySQL is available:
  `cargo test -p ersatzrs-infra --features mysql mysql_backend`

Manual check with a real TV library:

1. Run `cargo run --bin ersatzrs-scanner -- --provider local --library-id <id>`.
2. Confirm `LibraryFolder` rows exist for show and season folders.
3. Confirm episode `MediaFile.LibraryFolderId` values point at season folders.
4. Run the same scan again and confirm unchanged seasons do not invoke episode
   ffprobe work.
5. Touch one episode or sidecar file in one season and confirm only that season
   rescans.

## Deferred Work

- Porting the same folder-etag optimization to movie, other-video, song, image,
  music-video, and remote-stream scanners is separate work. The helper shape
  should be reusable, but this plan is scoped to
  `TelevisionFolderScanner.cs` parity.
- Exact trash semantics should be revisited if ErsatzRS gains a distinct trash
  repository/table beyond `MediaItem.State`.
