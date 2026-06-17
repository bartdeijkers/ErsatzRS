# Missing library items — parity closeout

## Problem

When a media file is moved or deleted from a source (local folder or
Plex/Jellyfin/Emby server), ErsatzRS never reacts. The item stays in the
database at `MediaItemState::Normal` (`State = 0`) forever: it keeps appearing
on `/media/*` pages, never shows in `/media/trash`, never trips the **File Not
Found** or **Unavailable** health checks, and stays eligible for
scheduling/playback (failing only at stream time).

## Root cause — the producer side was never ported

The *consuming* side of the missing-item state machine is fully built and
faithful to ErsatzTV:

- `FileNotFoundHealthCheck` / `UnavailableHealthCheck`
  (`crates/ersatzrs-infra/src/health.rs`) query `State = 1` / `State = 2`.
- Trash page `list_trash_items` / `load_trash_*` and `empty_trash`
  (`crates/ersatzrs-web/src/admin/server_fns/media_browser.rs`) join `State = 1`.
- Search maps `1 => FileNotFound`, `2 => Unavailable`, `3 => RemoteOnly`
  (`crates/ersatzrs-infra/src/repositories/common.rs`).

But **nothing in production code ever writes `State = 1/2/3`.** The only such
write in the tree is a test fixture. Both scanner families only upsert files
that currently exist (setting `State = 0`); the "flag what disappeared" passes
were never ported.

### ErsatzTV reference (the passes we are missing)

- **Local** — every `*FolderScanner` runs a second pass:
  `FindXPaths(libraryPath)` → `File.Exists` → `FlagFileNotFound` (sets
  `State = 1`). See `MovieFolderScanner.cs:210` and
  `MediaItemRepository.FlagFileNotFound` (`:81`).
- **Media server** — `MediaServerXLibraryScanner`:
  - trash items no longer on the server:
    `existingItems.Keys.Except(incomingItemIds)` → `FlagFileNotFound`
    (`MediaServerMovieLibraryScanner.cs:205`).
  - item still on server but local file missing (`ShouldScanItem`,
    `:269-312`): `FlagRemoteOnly` (`State = 3`) when
    `ServerSupportsRemoteStreaming`, else `FlagUnavailable` (`State = 2`).

`ServerSupportsRemoteStreaming` overrides (else `false`): Plex movie/TV/other =
true; Jellyfin movie/TV = true; Emby movie/TV = true. Others default false.

## Plan

### 1. Local scanner — FileNotFound (`crates/ersatzrs-scanner/src/lib.rs`)
After processing each `library_path` (near `update_library_path_scan`, ~`:236`),
add a post-pass: select every `MediaFile.Path` for items under that
`LibraryPathId` (join `MediaItem`→`MediaVersion`→`MediaFile`), check filesystem
existence, and `UPDATE "MediaItem" SET "State" = 1 WHERE "Id" IN (...)` for the
missing ones. Log `Flagging missing <kind> at {path}`. Restore (→ `State = 0`)
already happens on upsert.

### 2. Media-server scanner — FileNotFound trash pass (jellyfin/plex/emby.rs)
In each `sync_*_library_with_progress`, accumulate incoming `item_id`s across
all pages, then after the loop diff against existing DB item-ids for the
library and flag the leftovers `State = 1`.

### 3. Media-server scanner — Unavailable / RemoteOnly
For an incoming item whose resolved local path (via
`ersatzrs-core::path_replacement`) does not exist: set `State = 3` when the
server supports remote streaming for that media kind, else `State = 2`. Mirror
the etag/state skip logic in `ShouldScanItem`.

### 4. Health / parity / changelog
Confirm both health checks now surface data; clear the missing-item
`ParityDebt` in the scanner parity TOML(s); add a CHANGELOG `[Unreleased]`
`Fixed` entry.

### 5. Verification (full parity gate)
- Scanner integration tests: seed an item, delete the file (local) / drop it
  from the mocked server (media server), rescan, assert the `State` transition
  and that it surfaces in trash + health.
- `cargo test --workspace --all-features`; both clippy passes
  (with and without `--features mysql`); `cargo xtask parity-check web`;
  `cargo xtask smoke`. Coordinate heavy builds around the codex CI compile.

## Notes / risks
- Path resolution for transition #3 depends on per-server path replacements;
  match `MediaItem.GetLocalPath` semantics.
- Keep EF-style identifier casing and the `State IN (1,2)` trash semantics
  already used by `trashed_episode_paths`.
