//! Shared media-server (Plex/Jellyfin/Emby) scanner helpers.
//!
//! Ports the per-scanner "missing item" passes from ErsatzTV's
//! `MediaServerXLibraryScanner` family. After the add/update pass, ErsatzTV
//! diffs the set of item ids it just saw against what is stored and trashes
//! the leftovers (`existing.Except(incomingItemIds)` →
//! `FlagFileNotFound`), and separately flags items whose backing local file
//! has gone missing as `Unavailable`/`RemoteOnly`. None of that was ported, so
//! moved/deleted media-server items stayed `Normal` forever — never surfacing
//! in `/media/trash` or the File Not Found / Unavailable health checks.

use std::collections::HashSet;
use std::path::Path;

use ersatzrs_core::path_replacement::{replace_path, PathPlatform, PathReplacement};
use sqlx::Row;

use crate::db::{assert_sql_safe, DbPool};

/// State values mirroring `ErsatzTV.Core.Domain.MediaItemState`.
pub(crate) const STATE_NORMAL: i64 = 0;
pub(crate) const STATE_FILE_NOT_FOUND: i64 = 1;
pub(crate) const STATE_UNAVAILABLE: i64 = 2;
pub(crate) const STATE_REMOTE_ONLY: i64 = 3;

/// Platform whose separator conventions the running host uses, for resolving
/// a replaced local path before the existence check.
#[cfg(windows)]
const HOST_PLATFORM: PathPlatform = PathPlatform::Windows;
#[cfg(not(windows))]
const HOST_PLATFORM: PathPlatform = PathPlatform::Unix;

/// Flag media-server items whose server-side id is no longer present in the
/// just-completed scan as `MediaItemState::FileNotFound` (`State = 1`).
///
/// `table` is the per-kind media-server table (`JellyfinMovie`, `EmbyEpisode`,
/// `PlexShow`, …) and `id_column` its server-id column (`ItemId` for
/// Jellyfin/Emby, `Key` for Plex). Rows join `MediaItem` on the shared primary
/// key and are scoped to `library_id` through `LibraryPath`. Items already at
/// `FileNotFound` are skipped so the log and the write stay meaningful.
///
/// Returns the number of items newly flagged. Port of the
/// `existing.Except(incomingItemIds)` → `FlagFileNotFound` pass.
///
/// # Errors
///
/// Returns an error when a database read or write fails.
pub async fn flag_missing_media_server_items<S: std::hash::BuildHasher>(
    pool: &DbPool,
    table: &str,
    id_column: &str,
    library_id: i64,
    present_item_ids: &HashSet<String, S>,
) -> Result<u64, sqlx::Error> {
    let select = format!(
        r#"SELECT "t"."Id" AS "Id", "t"."{id_column}" AS "ItemId"
           FROM "{table}" AS "t"
           JOIN "MediaItem" AS "mi" ON "mi"."Id" = "t"."Id"
           JOIN "LibraryPath" AS "lp" ON "lp"."Id" = "mi"."LibraryPathId"
           WHERE "lp"."LibraryId" = ? AND "mi"."State" <> {STATE_FILE_NOT_FOUND}"#
    );
    let rows = sqlx::query(assert_sql_safe(&select))
        .bind(library_id)
        .fetch_all(pool)
        .await?;

    let mut missing_ids: Vec<i64> = Vec::new();
    for row in &rows {
        let item_id: Option<String> = row.try_get("ItemId")?;
        let present = item_id
            .as_deref()
            .is_some_and(|id| present_item_ids.contains(id));
        if !present {
            missing_ids.push(row.try_get("Id")?);
        }
    }

    flag_media_items(pool, &missing_ids, STATE_FILE_NOT_FOUND).await?;
    Ok(missing_ids.len() as u64)
}

/// Flag media-server items that are still present on the server but whose
/// resolved local file is missing.
///
/// With `supports_remote_streaming` the item becomes `RemoteOnly` (`State = 3`,
/// still playable via the server); otherwise `Unavailable` (`State = 2`). A
/// present local file restores a previously `RemoteOnly`/`Unavailable` item to
/// `Normal`. Items absent from `present_item_ids` are skipped — the
/// [`flag_missing_media_server_items`] trash pass already owns them. Only leaf
/// tables with media files (`*Movie`, `*Episode`) should be passed; `table`'s
/// rows join `MediaVersion`/`MediaFile` through the shared primary key.
///
/// Port of the `ShouldScanItem` `FlagRemoteOnly`/`FlagUnavailable` branch in
/// ErsatzTV's `MediaServerXLibraryScanner`.
///
/// # Errors
///
/// Returns an error when a database read or write fails.
pub async fn flag_unavailable_media_server_items<S: std::hash::BuildHasher>(
    pool: &DbPool,
    table: &str,
    id_column: &str,
    library_id: i64,
    present_item_ids: &HashSet<String, S>,
    replacements: &[PathReplacement],
    supports_remote_streaming: bool,
) -> Result<(), sqlx::Error> {
    let select = format!(
        r#"SELECT "t"."Id" AS "Id", "t"."{id_column}" AS "ItemId",
                  "mf"."Path" AS "Path", "mi"."State" AS "State"
           FROM "{table}" AS "t"
           JOIN "MediaItem" AS "mi" ON "mi"."Id" = "t"."Id"
           JOIN "LibraryPath" AS "lp" ON "lp"."Id" = "mi"."LibraryPathId"
           JOIN "MediaVersion" AS "mv"
             ON "t"."Id" = COALESCE("mv"."MovieId", "mv"."EpisodeId")
           JOIN "MediaFile" AS "mf" ON "mv"."Id" = "mf"."MediaVersionId"
           WHERE "lp"."LibraryId" = ?"#
    );
    let rows = sqlx::query(assert_sql_safe(&select))
        .bind(library_id)
        .fetch_all(pool)
        .await?;

    let missing_state = if supports_remote_streaming {
        STATE_REMOTE_ONLY
    } else {
        STATE_UNAVAILABLE
    };

    let mut to_missing: Vec<i64> = Vec::new();
    let mut to_normal: Vec<i64> = Vec::new();
    for row in &rows {
        let item_id: Option<String> = row.try_get("ItemId")?;
        // Only touch items the server still reports; trashed items are handled
        // by the FileNotFound pass and must not be clobbered here.
        if !item_id
            .as_deref()
            .is_some_and(|id| present_item_ids.contains(id))
        {
            continue;
        }

        let id: i64 = row.try_get("Id")?;
        let state: i64 = row.try_get("State")?;
        let server_path: Option<String> = row.try_get("Path")?;
        let exists = server_path.as_deref().is_some_and(|path| {
            let local = replace_path(replacements, path, HOST_PLATFORM);
            Path::new(&local).exists()
        });

        if exists {
            if state == STATE_REMOTE_ONLY || state == STATE_UNAVAILABLE {
                to_normal.push(id);
            }
        } else if state != missing_state {
            tracing::info!(
                table,
                id,
                path = server_path.as_deref().unwrap_or_default(),
                "flagging media-server item with missing local file"
            );
            to_missing.push(id);
        }
    }

    flag_media_items(pool, &to_missing, missing_state).await?;
    flag_media_items(pool, &to_normal, STATE_NORMAL).await?;
    Ok(())
}

/// Load a media server's path replacements as a `PathReplacement` set, mirroring
/// the IPTV stream loader (`source_platform` inferred from each source path).
///
/// `table`/`path_column`/`fk_column` vary per server
/// (`PlexPathReplacement`/`PlexPath`/`PlexMediaSourceId`, etc.).
///
/// # Errors
///
/// Returns an error when the read fails.
pub async fn load_media_server_path_replacements(
    pool: &DbPool,
    table: &str,
    path_column: &str,
    fk_column: &str,
    media_source_id: i64,
) -> Result<Vec<PathReplacement>, sqlx::Error> {
    let sql = format!(
        r#"SELECT "{path_column}" AS "SourcePath", "LocalPath" AS "LocalPath"
           FROM "{table}" WHERE "{fk_column}" = ?"#
    );
    let rows = sqlx::query(assert_sql_safe(&sql))
        .bind(media_source_id)
        .fetch_all(pool)
        .await?;

    let mut replacements = Vec::new();
    for row in &rows {
        let source_path: Option<String> = row.try_get("SourcePath")?;
        let replacement_path: Option<String> = row.try_get("LocalPath")?;
        if let (Some(source_path), Some(replacement_path)) = (source_path, replacement_path) {
            let source_platform = if source_path.contains('\\') || source_path.contains(':') {
                PathPlatform::Windows
            } else {
                PathPlatform::Unix
            };
            replacements.push(PathReplacement {
                source_platform,
                source_path,
                replacement_path,
            });
        }
    }
    Ok(replacements)
}

/// Set `MediaItem.State` for the given primary keys, building an `IN (...)`
/// list of bound placeholders. No-op when `ids` is empty.
///
/// # Errors
///
/// Returns an error when the update fails.
pub(crate) async fn flag_media_items(
    pool: &DbPool,
    ids: &[i64],
    state: i64,
) -> Result<(), sqlx::Error> {
    if ids.is_empty() {
        return Ok(());
    }
    let placeholders = vec!["?"; ids.len()].join(", ");
    let sql = format!(r#"UPDATE "MediaItem" SET "State" = {state} WHERE "Id" IN ({placeholders})"#);
    let mut query = sqlx::query(assert_sql_safe(&sql));
    for id in ids {
        query = query.bind(id);
    }
    query.execute(pool).await?;
    Ok(())
}
