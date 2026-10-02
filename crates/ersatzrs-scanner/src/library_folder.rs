//! `LibraryFolder` SQL helpers.
//!
//! Ports the folder-row behavior of
//! `ErsatzTV.Infrastructure/Data/Repositories/LibraryRepository.cs`
//! (`GetParentFolderId`, `GetOrAddFolder`, `SetEtag`, `UpdateLibraryFolderId`,
//! `CleanEtagsForLibraryPath`) into scanner-local SQL.
//!
//! These remain scanner-local until another scanner slice needs them. SQL
//! uses quoted EF-style identifiers and `?` placeholders against `SQLite`,
//! the supported database backend.
//!
//! Parity notes versus the C# repository:
//!
//! - `get_or_add_library_folder` merges C#'s `GetOrAddFolder` lookup/insert
//!   and the parent-repair branch, returning the persisted row. New rows are
//!   created with `Etag = NULL`, matching `CreateNewFolder`.
//! - `set_library_folder_etag` is C#'s `SetEtag` reduced to its UPDATE branch;
//!   the scanner flow always creates the row via `get_or_add_library_folder`
//!   first, so the insert branch of `SetEtag` is unreachable here.
//! - `update_media_file_library_folder_id` keys on `MediaVersionId` (the link
//!   the episode sync establishes) instead of C#'s `MediaFile.Id`; the effect
//!   on `MediaFile.LibraryFolderId` is identical.
//! - `clean_library_folders_for_path` mirrors `CleanEtagsForLibraryPath`:
//!   delete rows whose path no longer exists, deepest-first by path length,
//!   only when no `MediaFile` references them and no child folder points at
//!   them.

use std::collections::BTreeMap;
use std::path::Path;

use ersatzrs_infra::db::{last_insert_id, Db, DbPool};
use sqlx::{Row, Transaction};

use crate::Result;

#[cfg(windows)]
pub(crate) fn case_insensitive_paths() -> bool {
    true
}

// Unix directories may differ only by case; preserve their separate identities.
#[cfg(not(windows))]
pub(crate) fn case_insensitive_paths() -> bool {
    false
}

pub(crate) fn same_local_path(left: &str, right: &str) -> bool {
    match (std::fs::canonicalize(left), std::fs::canonicalize(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

/// Repair Windows folder aliases before inspecting local television provenance.
/// Keep media IDs, versions, stream pins and scheduling references intact.
pub(crate) async fn reconcile_television_folder_aliases(
    pool: &DbPool,
    library_path_id: i64,
) -> Result<u64> {
    reconcile_folder_aliases(pool, library_path_id, case_insensitive_paths()).await
}

async fn reconcile_folder_aliases(
    pool: &DbPool,
    library_path_id: i64,
    case_insensitive: bool,
) -> Result<u64> {
    if !case_insensitive {
        return Ok(0);
    }
    let mut tx = ersatzrs_infra::db::begin_write(pool).await?;
    let folders = sqlx::query(
        r#"SELECT "Id", "Path" FROM "LibraryFolder"
           WHERE "LibraryPathId" = ? ORDER BY "Id" DESC"#,
    )
    .bind(library_path_id)
    .fetch_all(&mut *tx)
    .await?;
    let mut canonical = BTreeMap::<String, (i64, String)>::new();
    let mut repaired = 0;
    for folder in folders {
        let Some(path) = folder.get::<Option<String>, _>("Path") else {
            continue;
        };
        let key = path.replace('\\', "/").trim_end_matches('/').to_lowercase();
        let id: i64 = folder.get("Id");
        let Some((retained, retained_path)) = canonical.get(&key) else {
            canonical.insert(key, (id, path));
            continue;
        };
        // Windows can also host case-sensitive directories. Require physical
        // directory agreement, and leave missing/unreadable aliases untouched.
        if !same_local_path(retained_path, &path) {
            continue;
        }
        // Image duration settings belong to the image scanner; deleting their
        // folder would cascade away operator-owned configuration.
        let image_settings: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM "ImageFolderDuration" WHERE "LibraryFolderId" IN (?, ?)"#,
        )
        .bind(retained)
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
        if image_settings != 0 {
            continue;
        }
        sqlx::query(r#"UPDATE "MediaFile" SET "LibraryFolderId" = ? WHERE "LibraryFolderId" = ?"#)
            .bind(retained)
            .bind(id)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            r#"UPDATE "LibraryFolder" SET "ParentId" = ?, "Etag" = NULL WHERE "ParentId" = ?"#,
        )
        .bind(retained)
        .bind(id)
        .execute(&mut *tx)
        .await?;
        sqlx::query(r#"UPDATE "LibraryFolder" SET "Etag" = NULL WHERE "Id" = ?"#)
            .bind(retained)
            .execute(&mut *tx)
            .await?;
        sqlx::query(r#"DELETE FROM "LibraryFolder" WHERE "Id" = ?"#)
            .bind(id)
            .execute(&mut *tx)
            .await?;
        repaired += 1;
    }
    tx.commit().await?;
    Ok(repaired)
}

async fn find_library_folder(
    pool: &DbPool,
    library_path_id: i64,
    path: &str,
    case_insensitive: bool,
) -> Result<Option<sqlx::sqlite::SqliteRow>> {
    let exact = sqlx::query(
        r#"SELECT "Id", "Path", "ParentId", "Etag" FROM "LibraryFolder"
           WHERE "LibraryPathId" = ? AND "Path" = ? ORDER BY "Id" LIMIT 1"#,
    )
    .bind(library_path_id)
    .bind(path)
    .fetch_optional(pool)
    .await?;
    if exact.is_some() || !case_insensitive {
        return Ok(exact);
    }
    let aliases = sqlx::query(
        r#"SELECT "Id", "Path", "ParentId", "Etag" FROM "LibraryFolder"
           WHERE "LibraryPathId" = ? AND "Path" = ? COLLATE NOCASE ORDER BY "Id""#,
    )
    .bind(library_path_id)
    .bind(path)
    .fetch_all(pool)
    .await?;
    Ok(aliases.into_iter().find(|row| {
        row.get::<Option<String>, _>("Path")
            .is_some_and(|stored| same_local_path(&stored, path))
    }))
}

/// One `LibraryFolder` row, mirroring the columns ErsatzTV's
/// `GetOrAddFolder` returns to the scanner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LibraryFolderRow {
    pub id: i64,
    pub path: String,
    pub parent_id: Option<i64>,
    pub etag: Option<String>,
}

/// Port of `LibraryRepository.GetParentFolderId`: resolve the immediate
/// parent directory of `folder` and return the id of the `LibraryFolder` row
/// for that parent path under the same library path. `None` when `folder` has
/// no parent or no row matches (same as C# returning `Option<int>.None`).
pub(crate) async fn get_parent_folder_id(
    pool: &DbPool,
    library_path_id: i64,
    folder: &Path,
) -> Result<Option<i64>> {
    let Some(parent) = folder.parent() else {
        return Ok(None);
    };
    // C# uses DirectoryInfo(folder).Parent which is empty for a root path;
    // an empty parent cannot match a stored folder path either.
    if parent.as_os_str().is_empty() {
        return Ok(None);
    }
    let parent_text = parent.to_string_lossy();

    Ok(find_library_folder(
        pool,
        library_path_id,
        &parent_text,
        case_insensitive_paths(),
    )
    .await?
    .map(|row| row.get("Id")))
}

/// Port of `LibraryRepository.GetOrAddFolder`: select the row for
/// `(library_path_id, folder)`; insert it (with `Etag = NULL`) when missing;
/// reuse physically verified Windows case aliases and repair `ParentId` when
/// a parent is known and differs. Returns the folder ID, `ETag` and current path.
pub(crate) async fn get_or_add_library_folder(
    pool: &DbPool,
    library_path_id: i64,
    parent_id: Option<i64>,
    folder: &Path,
) -> Result<LibraryFolderRow> {
    get_or_add_library_folder_with_case_mode(
        pool,
        library_path_id,
        parent_id,
        folder,
        case_insensitive_paths(),
    )
    .await
}

async fn get_or_add_library_folder_with_case_mode(
    pool: &DbPool,
    library_path_id: i64,
    parent_id: Option<i64>,
    folder: &Path,
    case_insensitive: bool,
) -> Result<LibraryFolderRow> {
    let folder_text = folder.to_string_lossy().into_owned();

    let existing =
        find_library_folder(pool, library_path_id, &folder_text, case_insensitive).await?;

    let mut row = if let Some(found) = existing {
        LibraryFolderRow {
            id: found.get("Id"),
            path: folder_text.clone(),
            parent_id: found.get::<Option<i64>, _>("ParentId"),
            etag: found.get::<Option<String>, _>("Etag"),
        }
    } else {
        let result = sqlx::query(
            r#"INSERT INTO "LibraryFolder" ("Etag", "LibraryPathId", "ParentId", "Path")
               VALUES (NULL, ?, ?, ?)"#,
        )
        .bind(library_path_id)
        .bind(parent_id)
        .bind(&folder_text)
        .execute(pool)
        .await?;
        LibraryFolderRow {
            id: last_insert_id(result),
            path: folder_text.clone(),
            parent_id,
            etag: None,
        }
    };

    // Repair the parent link only when a parent is known and it changed,
    // matching C#'s `foreach (int parentFolder in maybeParentFolder)` guard.
    if let Some(parent) = parent_id {
        if row.parent_id != Some(parent) {
            sqlx::query(r#"UPDATE "LibraryFolder" SET "ParentId" = ? WHERE "Id" = ?"#)
                .bind(parent)
                .bind(row.id)
                .execute(pool)
                .await?;
            row.parent_id = Some(parent);
        }
    }

    Ok(row)
}

/// Port of the UPDATE branch of `LibraryRepository.SetEtag`: write `etag` to a
/// `LibraryFolder` row that already exists.
pub(crate) async fn set_library_folder_etag(
    pool: &DbPool,
    folder_id: i64,
    etag: &str,
) -> Result<()> {
    sqlx::query(r#"UPDATE "LibraryFolder" SET "Etag" = ? WHERE "Id" = ?"#)
        .bind(etag)
        .bind(folder_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Port of `LibraryRepository.UpdateLibraryFolderId`: link the
/// `MediaFile` head for `media_version_id` to its season `LibraryFolder`.
/// Runs inside the episode-sync transaction.
pub(crate) async fn update_media_file_library_folder_id(
    tx: &mut Transaction<'_, Db>,
    media_version_id: i64,
    library_folder_id: i64,
) -> Result<()> {
    sqlx::query(r#"UPDATE "MediaFile" SET "LibraryFolderId" = ? WHERE "MediaVersionId" = ?"#)
        .bind(library_folder_id)
        .bind(media_version_id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// Port of `LibraryRepository.CleanEtagsForLibraryPath`: drop `LibraryFolder`
/// rows whose path no longer exists on disk, processed deepest-first (longest
/// path string first, like C#'s `OrderByDescending(lp => lp.Path.Length)`),
/// and only when no `MediaFile` references the row and no child folder points
/// at it.
pub(crate) async fn clean_library_folders_for_path(
    pool: &DbPool,
    library_path_id: i64,
) -> Result<()> {
    let folders =
        sqlx::query(r#"SELECT "Id", "Path" FROM "LibraryFolder" WHERE "LibraryPathId" = ?"#)
            .bind(library_path_id)
            .fetch_all(pool)
            .await?;

    let mut missing: Vec<(i64, String)> = folders
        .into_iter()
        .filter_map(|row| {
            let path: Option<String> = row.get("Path");
            let path = path?;
            if Path::new(&path).is_dir() {
                None
            } else {
                Some((row.get("Id"), path))
            }
        })
        .collect();

    // Deepest-first so a child is deleted before its parent is reconsidered.
    missing.sort_by_key(|(_, path)| std::cmp::Reverse(path.len()));

    for (folder_id, _) in missing {
        sqlx::query(
            r#"DELETE FROM "LibraryFolder" WHERE "Id" = ?
               AND NOT EXISTS (SELECT "Id" FROM "MediaFile" WHERE "LibraryFolderId" = ?)
               AND NOT EXISTS (SELECT "Id" FROM "LibraryFolder" WHERE "ParentId" = ?)"#,
        )
        .bind(folder_id)
        .bind(folder_id)
        .bind(folder_id)
        .execute(pool)
        .await?;
    }

    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use sqlx::sqlite::SqlitePoolOptions;
    use sqlx::Executor;

    use super::*;

    async fn test_db() -> DbPool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("create in-memory sqlite pool");
        pool.execute("PRAGMA foreign_keys = ON")
            .await
            .expect("enable sqlite foreign keys");
        sqlx::migrate!("../../migrations")
            .run(&pool)
            .await
            .expect("run migrations");
        pool
    }

    /// Seed one `MediaSource` + `Library` + `LibraryPath` and return the
    /// library path id.
    async fn seed_library_path(pool: &DbPool) -> i64 {
        sqlx::query(r#"INSERT INTO "MediaSource" ("Id") VALUES (1)"#)
            .execute(pool)
            .await
            .expect("seed media source");
        sqlx::query(
            r#"INSERT INTO "Library" ("Id", "Name", "MediaKind", "MediaSourceId")
               VALUES (1, 'TV', 2, 1)"#,
        )
        .execute(pool)
        .await
        .expect("seed library");
        sqlx::query(
            r#"INSERT INTO "LibraryPath" ("Id", "Path", "LibraryId") VALUES (1, '/lib', 1)"#,
        )
        .execute(pool)
        .await
        .expect("seed library path");
        1
    }

    /// Seed a minimal `MediaItem` -> `MediaVersion` -> `MediaFile` chain so a
    /// `MediaFile` can reference a `LibraryFolder`. Returns the media
    /// version id.
    async fn seed_media_file(
        pool: &DbPool,
        library_path_id: i64,
        media_file_id: i64,
        media_version_id: i64,
        library_folder_id: Option<i64>,
        path: &str,
    ) {
        let item_id = media_file_id;
        sqlx::query(r#"INSERT INTO "MediaItem" ("Id", "LibraryPathId", "State") VALUES (?, ?, 0)"#)
            .bind(item_id)
            .bind(library_path_id)
            .execute(pool)
            .await
            .expect("seed media item");
        // EpisodeId stays NULL: these helpers only exercise the
        // MediaFile -> LibraryFolder link, not episode resolution.
        sqlx::query(
            r#"INSERT INTO "MediaVersion"
               ("Id", "DateAdded", "DateUpdated", "Duration", "Height", "Width",
                "VideoScanKind")
               VALUES (?, '2024-01-01 00:00:00', '2024-01-01 00:00:00', '00:20:00',
                       1080, 1920, 0)"#,
        )
        .bind(media_version_id)
        .execute(pool)
        .await
        .expect("seed media version");
        sqlx::query(
            r#"INSERT INTO "MediaFile"
               ("Id", "LibraryFolderId", "MediaVersionId", "Path", "PathHash")
               VALUES (?, ?, ?, ?, ?)"#,
        )
        .bind(media_file_id)
        .bind(library_folder_id)
        .bind(media_version_id)
        .bind(path)
        .bind(format!("hash-{media_file_id}"))
        .execute(pool)
        .await
        .expect("seed media file");
    }

    async fn folder_row(pool: &DbPool, id: i64) -> LibraryFolderRow {
        let row = sqlx::query(
            r#"SELECT "Id", "Path", "ParentId", "Etag" FROM "LibraryFolder" WHERE "Id" = ?"#,
        )
        .bind(id)
        .fetch_one(pool)
        .await
        .expect("load folder row");
        LibraryFolderRow {
            id: row.get("Id"),
            path: row.get::<Option<String>, _>("Path").unwrap(),
            parent_id: row.get("ParentId"),
            etag: row.get("Etag"),
        }
    }

    // Unix uses a symlink to exercise the Windows alias policy against two
    // spellings of one physical directory. Windows resolves the case alias
    // directly, without requiring symlink privileges.
    pub(crate) fn case_alias_fixture() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf)
    {
        let root = tempfile::tempdir().unwrap();
        let lower = root.path().join("show");
        let upper = root.path().join("Show");
        std::fs::create_dir_all(lower.join("Season 01")).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&lower, &upper).unwrap();
        #[cfg(windows)]
        assert!(upper.is_dir());
        (root, lower, upper)
    }

    async fn seed_folder_aliases(pool: &DbPool, lower: &Path, upper: &Path) {
        let lp = seed_library_path(pool).await;
        for (id, parent, path) in [
            (10, None, lower.to_path_buf()),
            (20, None, upper.to_path_buf()),
            (11, Some(10), lower.join("Season 01")),
            (21, Some(20), upper.join("Season 01")),
        ] {
            sqlx::query(
                r#"INSERT INTO "LibraryFolder" ("Id", "LibraryPathId", "ParentId", "Path", "Etag")
                   VALUES (?, ?, ?, ?, 'cached')"#,
            )
            .bind(id)
            .bind(lp)
            .bind(parent)
            .bind(path.to_string_lossy().as_ref())
            .execute(pool)
            .await
            .unwrap();
        }
        seed_media_file(pool, lp, 100, 10, Some(11), "fixture-a.mkv").await;
        seed_media_file(pool, lp, 200, 20, Some(21), "fixture-b.mkv").await;
        pool.execute(r#"INSERT INTO "Collection" ("Id", "Name", "UseCustomPlaybackOrder") VALUES (1, 'Fixture', 0)"#).await.unwrap();
        pool.execute(r#"INSERT INTO "CollectionItem" ("CollectionId", "MediaItemId", "PreferredAudioLanguageCode", "PreferredSubtitleLanguageCode") VALUES (1, 100, 'eng', 'nld'), (1, 200, 'nld', 'eng')"#).await.unwrap();
        pool.execute(r#"INSERT INTO "PlaylistGroup" ("Id", "Name") VALUES (1, 'Fixture')"#)
            .await
            .unwrap();
        pool.execute(r#"INSERT INTO "Playlist" ("Id", "Name", "IsSystem", "PlaylistGroupId") VALUES (1, 'Fixture', 0, 1)"#).await.unwrap();
        pool.execute(r#"INSERT INTO "PlaylistItem" ("Id", "Index", "PlaylistId", "CollectionType", "MediaItemId", "MediaVersionId", "AudioStreamIndex", "SubtitleStreamIndex") VALUES (1, 0, 1, 0, 200, 20, 1, 2)"#).await.unwrap();
    }

    #[tokio::test]
    async fn case_alias_repair_preserves_media_references_and_is_idempotent() {
        let pool = test_db().await;
        let (_root, lower, upper) = case_alias_fixture();
        seed_folder_aliases(&pool, &lower, &upper).await;
        assert_eq!(reconcile_folder_aliases(&pool, 1, true).await.unwrap(), 2);
        assert_eq!(reconcile_folder_aliases(&pool, 1, true).await.unwrap(), 0);
        let folders: Vec<i64> =
            sqlx::query_scalar(r#"SELECT "Id" FROM "LibraryFolder" ORDER BY "Id""#)
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(folders, [20, 21]);
        assert_eq!(folder_row(&pool, 21).await.parent_id, Some(20));
        assert_eq!(folder_row(&pool, 21).await.etag, None);
        let files: Vec<(i64, i64, i64)> = sqlx::query_as(
            r#"SELECT "Id", "MediaVersionId", "LibraryFolderId" FROM "MediaFile" ORDER BY "Id""#,
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(files, [(100, 10, 21), (200, 20, 21)]);
        let collection: Vec<(i64, String, String)> = sqlx::query_as(r#"SELECT "MediaItemId", "PreferredAudioLanguageCode", "PreferredSubtitleLanguageCode" FROM "CollectionItem" ORDER BY "MediaItemId""#).fetch_all(&pool).await.unwrap();
        assert_eq!(
            collection,
            [
                (100, "eng".into(), "nld".into()),
                (200, "nld".into(), "eng".into())
            ]
        );
        let pin: (i64, i64, i64, i64) = sqlx::query_as(r#"SELECT "MediaItemId", "MediaVersionId", "AudioStreamIndex", "SubtitleStreamIndex" FROM "PlaylistItem" WHERE "Id" = 1"#).fetch_one(&pool).await.unwrap();
        assert_eq!(pin, (200, 20, 1, 2));
        let repeated = get_or_add_library_folder_with_case_mode(
            &pool,
            1,
            Some(20),
            &lower.join("Season 01"),
            true,
        )
        .await
        .unwrap();
        assert_eq!(repeated.id, 21);
    }

    #[tokio::test]
    async fn case_alias_repair_rolls_back_all_links_on_failure() {
        let pool = test_db().await;
        let (_root, lower, upper) = case_alias_fixture();
        seed_folder_aliases(&pool, &lower, &upper).await;
        pool.execute(r#"CREATE TRIGGER "RejectAliasDeletion" BEFORE DELETE ON "LibraryFolder" BEGIN SELECT RAISE(ABORT, 'injected failure'); END"#).await.unwrap();
        assert!(reconcile_folder_aliases(&pool, 1, true).await.is_err());
        assert_eq!(folder_row(&pool, 21).await.parent_id, Some(20));
        assert_eq!(folder_row(&pool, 21).await.etag.as_deref(), Some("cached"));
        let linked: i64 =
            sqlx::query_scalar(r#"SELECT "LibraryFolderId" FROM "MediaFile" WHERE "Id" = 100"#)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(linked, 11);
    }

    #[tokio::test]
    async fn case_alias_repair_preserves_image_duration_settings() {
        let pool = test_db().await;
        let (_root, lower, upper) = case_alias_fixture();
        seed_folder_aliases(&pool, &lower, &upper).await;
        pool.execute(r#"INSERT INTO "ImageFolderDuration" ("Id", "LibraryFolderId", "DurationSeconds") VALUES (1, 11, 7.5)"#).await.unwrap();
        assert_eq!(reconcile_folder_aliases(&pool, 1, true).await.unwrap(), 1);
        let setting: (i64, f64) = sqlx::query_as(r#"SELECT "LibraryFolderId", "DurationSeconds" FROM "ImageFolderDuration" WHERE "Id" = 1"#).fetch_one(&pool).await.unwrap();
        assert_eq!(setting, (11, 7.5));
        assert_eq!(folder_row(&pool, 11).await.parent_id, Some(20));
    }

    #[tokio::test]
    async fn case_alias_repair_preserves_case_sensitive_and_missing_paths() {
        let pool = test_db().await;
        let root = tempfile::tempdir().unwrap();
        let lower = root.path().join("missing");
        let upper = root.path().join("Missing");
        seed_folder_aliases(&pool, &lower, &upper).await;
        assert_eq!(reconcile_folder_aliases(&pool, 1, true).await.unwrap(), 0);
        let (_alias_root, lower, upper) = case_alias_fixture();
        sqlx::query(r#"UPDATE "LibraryFolder" SET "Path" = ? WHERE "Id" = 10"#)
            .bind(lower.to_string_lossy().as_ref())
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(r#"UPDATE "LibraryFolder" SET "Path" = ? WHERE "Id" = 20"#)
            .bind(upper.to_string_lossy().as_ref())
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(reconcile_folder_aliases(&pool, 1, false).await.unwrap(), 0);
        let count: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "LibraryFolder""#)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 4);
    }

    // Unix can create two genuinely distinct directories differing only by
    // case. They must survive even when exercising the Windows matching mode.
    #[cfg(unix)]
    #[tokio::test]
    async fn case_alias_repair_never_merges_distinct_physical_directories() {
        let pool = test_db().await;
        let root = tempfile::tempdir().unwrap();
        let lower = root.path().join("show");
        let upper = root.path().join("Show");
        std::fs::create_dir_all(lower.join("Season 01")).unwrap();
        std::fs::create_dir_all(upper.join("Season 01")).unwrap();
        seed_folder_aliases(&pool, &lower, &upper).await;
        assert_eq!(reconcile_folder_aliases(&pool, 1, true).await.unwrap(), 0);
        let first = get_or_add_library_folder_with_case_mode(&pool, 1, None, &lower, true)
            .await
            .unwrap();
        let second = get_or_add_library_folder_with_case_mode(&pool, 1, None, &upper, true)
            .await
            .unwrap();
        assert_ne!(first.id, second.id);
    }

    #[tokio::test]
    async fn inserts_then_returns_existing_row() {
        let pool = test_db().await;
        let lp = seed_library_path(&pool).await;
        let show = Path::new("/lib/Show");

        let first = get_or_add_library_folder(&pool, lp, None, show)
            .await
            .expect("insert show folder");
        assert!(first.id >= 1);
        assert_eq!(first.path, "/lib/Show");
        assert_eq!(first.parent_id, None);
        assert_eq!(first.etag, None);

        // Second call must not insert a duplicate.
        let second = get_or_add_library_folder(&pool, lp, None, show)
            .await
            .expect("reload show folder");
        assert_eq!(second, first);

        let count: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "LibraryFolder""#)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn resolves_and_repairs_parent() {
        let pool = test_db().await;
        let lp = seed_library_path(&pool).await;

        let show = get_or_add_library_folder(&pool, lp, None, Path::new("/lib/Show"))
            .await
            .expect("insert show");

        // Parent lookup resolves the show row for the season's parent dir.
        let resolved = get_parent_folder_id(&pool, lp, Path::new("/lib/Show/Season 01"))
            .await
            .expect("parent lookup");
        assert_eq!(resolved, Some(show.id));

        // Insert the season without a parent, then repair it.
        let season = get_or_add_library_folder(&pool, lp, None, Path::new("/lib/Show/Season 01"))
            .await
            .expect("insert season");
        assert_eq!(season.parent_id, None);

        let repaired =
            get_or_add_library_folder(&pool, lp, Some(show.id), Path::new("/lib/Show/Season 01"))
                .await
                .expect("repair season parent");
        assert_eq!(repaired.id, season.id);
        assert_eq!(repaired.parent_id, Some(show.id));
        assert_eq!(folder_row(&pool, season.id).await.parent_id, Some(show.id));
    }

    #[tokio::test]
    async fn parent_lookup_returns_none_without_match() {
        let pool = test_db().await;
        let lp = seed_library_path(&pool).await;
        assert_eq!(
            get_parent_folder_id(&pool, lp, Path::new("/lib/Show/Season 01"))
                .await
                .expect("lookup"),
            None
        );
        assert_eq!(
            get_parent_folder_id(&pool, lp, Path::new("/"))
                .await
                .expect("root lookup"),
            None
        );
    }

    #[tokio::test]
    async fn set_etag_updates_known_row() {
        let pool = test_db().await;
        let lp = seed_library_path(&pool).await;
        let folder = get_or_add_library_folder(&pool, lp, None, Path::new("/lib/Show"))
            .await
            .expect("insert");

        set_library_folder_etag(&pool, folder.id, "abc123")
            .await
            .expect("set etag");
        assert_eq!(
            folder_row(&pool, folder.id).await.etag,
            Some("abc123".to_string())
        );
    }

    #[tokio::test]
    async fn update_media_file_links_to_folder() {
        let pool = test_db().await;
        let lp = seed_library_path(&pool).await;
        let folder = get_or_add_library_folder(&pool, lp, None, Path::new("/lib/Show"))
            .await
            .expect("insert folder");

        seed_media_file(&pool, lp, 100, 10, None, "/lib/Show/Season 01/e01.mkv").await;

        let mut tx = pool.begin().await.unwrap();
        update_media_file_library_folder_id(&mut tx, 10, folder.id)
            .await
            .expect("link media file");
        tx.commit().await.unwrap();

        let linked: Option<i64> =
            sqlx::query_scalar(r#"SELECT "LibraryFolderId" FROM "MediaFile" WHERE "Id" = 100"#)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(linked, Some(folder.id));
    }

    #[tokio::test]
    async fn clean_keeps_referenced_and_removes_stale() {
        let pool = test_db().await;
        let lp = seed_library_path(&pool).await;

        // Real on-disk folder that must survive cleanup.
        let tmp = tempfile::tempdir().unwrap();
        let live_path = tmp.path().to_string_lossy().into_owned();
        let live = get_or_add_library_folder(&pool, lp, None, Path::new(&live_path))
            .await
            .expect("insert live folder");

        // Stale parent + stale child, both gone from disk. Matching
        // ErsatzTV's `CleanEtagsForLibraryPath`, a single deepest-first pass
        // deletes the child first, so the parent's child guard then passes
        // and it is removed in the same pass.
        let stale_parent = get_or_add_library_folder(&pool, lp, None, Path::new("/lib/Gone"))
            .await
            .expect("insert stale parent");
        let stale_child = get_or_add_library_folder(
            &pool,
            lp,
            Some(stale_parent.id),
            Path::new("/lib/Gone/Season 01"),
        )
        .await
        .expect("insert stale child");

        // Stale parent whose child is still on disk: the child survives (its
        // path exists) and its presence keeps the stale parent via the
        // `NOT EXISTS child` guard.
        let kept_parent = get_or_add_library_folder(&pool, lp, None, Path::new("/lib/KeptParent"))
            .await
            .expect("insert kept parent");
        let live_child_path = tmp.path().join("live-child");
        std::fs::create_dir(&live_child_path).unwrap();
        let live_child =
            get_or_add_library_folder(&pool, lp, Some(kept_parent.id), &live_child_path)
                .await
                .expect("insert live child");

        // Stale folder referenced by a MediaFile -> kept by the file guard.
        let referenced = get_or_add_library_folder(&pool, lp, None, Path::new("/lib/Referenced"))
            .await
            .expect("insert referenced");
        seed_media_file(
            &pool,
            lp,
            100,
            10,
            Some(referenced.id),
            "/lib/Referenced/e01.mkv",
        )
        .await;

        clean_library_folders_for_path(&pool, lp)
            .await
            .expect("clean");

        let remaining: Vec<i64> =
            sqlx::query_scalar(r#"SELECT "Id" FROM "LibraryFolder" ORDER BY "Id""#)
                .fetch_all(&pool)
                .await
                .unwrap();
        // Survivors: on-disk live folder, referenced stale folder, the live
        // child, and its stale parent (kept by the still-present child).
        assert!(remaining.contains(&live.id));
        assert!(remaining.contains(&referenced.id));
        assert!(remaining.contains(&live_child.id));
        assert!(remaining.contains(&kept_parent.id));
        // Removed in one pass: the fully-stale parent/child pair.
        assert!(!remaining.contains(&stale_child.id));
        assert!(!remaining.contains(&stale_parent.id));
    }
}
