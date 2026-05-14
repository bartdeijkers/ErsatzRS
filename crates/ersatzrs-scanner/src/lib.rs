#![forbid(unsafe_code)]

use std::collections::VecDeque;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration, Utc};
use ersatzrs_domain::{
    LibraryMediaKind, MediaChapter, MediaStream, MediaStreamKind, ScanProgressSink, VideoScanKind,
};
use ersatzrs_infra::db::{Db, DbPool, DbRow};
use ersatzrs_infra::ffprobe::{FfprobeError, FfprobeExtractor, FfprobeMediaVersion};
use sha2::{Digest, Sha256};
use sqlx::{Row, Transaction};
use thiserror::Error;

pub mod nfo;

#[derive(Debug, Error)]
pub enum ScannerError {
    #[error("database error: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[error(transparent)]
    Ffprobe(#[from] FfprobeError),
    #[error("library {0} does not have local paths")]
    MissingLibraryPaths(i64),
    #[error("local scan for {0:?} libraries is not implemented yet")]
    UnsupportedLibraryMediaKind(LibraryMediaKind),
}

pub type Result<T> = std::result::Result<T, ScannerError>;

#[derive(Debug, Clone)]
pub struct LocalScanConfig {
    pub library_id: i64,
    pub ffprobe_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LocalScanReport {
    pub scanned_files: usize,
}

/// One pre-enumerated unit of work for the local scanner. Materializing the
/// full work list up front lets the with-progress variant report a true
/// fraction.
enum LocalScanWork {
    MusicVideo {
        artist_folder: String,
        file_path: PathBuf,
    },
    RemoteStream {
        path: PathBuf,
    },
    Generic {
        path: PathBuf,
    },
}

struct LocalScanBucket {
    library_path: LocalLibraryPath,
    items: Vec<LocalScanWork>,
}

/// Scan a local `ErsatzTV` library path into media database rows.
///
/// # Errors
///
/// Returns an error when the library has no paths, ffprobe fails, or database
/// writes fail.
pub async fn scan_local_library(
    pool: &DbPool,
    config: &LocalScanConfig,
) -> Result<LocalScanReport> {
    scan_local_library_with_progress(pool, config, None).await
}

/// Scan a local library while emitting fractional progress to `progress` if
/// provided. Mirrors `scan_local_library` and is the entry point used by the
/// web admin so the libraries page can render a percentage ring.
///
/// Progress is reported as `(files_processed / total_files)` after each item.
/// When `progress` is `None` this is a thin wrapper over the original
/// algorithm.
///
/// # Errors
///
/// Returns an error when the library has no paths, ffprobe fails, or database
/// writes fail.
#[allow(
    clippy::too_many_lines,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation
)]
pub async fn scan_local_library_with_progress(
    pool: &DbPool,
    config: &LocalScanConfig,
    progress: Option<&dyn ScanProgressSink>,
) -> Result<LocalScanReport> {
    let paths = load_library_paths(pool, config.library_id).await?;
    if paths.is_empty() {
        return Err(ScannerError::MissingLibraryPaths(config.library_id));
    }

    // Pre-enumerate files per path so we can report a true fraction. ETV's
    // `LocalFolderScanner` does the same up-front discovery before probing.
    let mut total_items: usize = 0;
    let mut buckets: Vec<LocalScanBucket> = Vec::new();
    for library_path in paths {
        if matches!(library_path.media_kind, LibraryMediaKind::Shows) {
            return Err(ScannerError::UnsupportedLibraryMediaKind(
                library_path.media_kind,
            ));
        }
        let items: Vec<LocalScanWork> = match library_path.media_kind {
            LibraryMediaKind::MusicVideos => collect_music_video_files(&library_path.path)
                .into_iter()
                .map(|(artist_folder, file_path)| LocalScanWork::MusicVideo {
                    artist_folder,
                    file_path,
                })
                .collect(),
            LibraryMediaKind::RemoteStreams => collect_remote_stream_files(&library_path.path)
                .into_iter()
                .map(|path| LocalScanWork::RemoteStream { path })
                .collect(),
            _ => collect_scannable_files(&library_path.path, library_path.media_kind)
                .into_iter()
                .map(|path| LocalScanWork::Generic { path })
                .collect(),
        };
        total_items = total_items.saturating_add(items.len());
        buckets.push(LocalScanBucket {
            library_path,
            items,
        });
    }

    if let Some(sink) = progress {
        sink.report(0.0);
    }

    let extractor = FfprobeExtractor::new(config.ffprobe_path.clone());
    let mut report = LocalScanReport::default();
    let mut processed: usize = 0;
    for bucket in buckets {
        let library_path = bucket.library_path;
        for work in bucket.items {
            match work {
                LocalScanWork::MusicVideo {
                    artist_folder,
                    file_path,
                } => {
                    let artist_id =
                        upsert_artist_for_folder(pool, library_path.id, &artist_folder).await?;
                    let probed = extractor.extract(&file_path).await?;
                    upsert_local_music_video(pool, &library_path, artist_id, &file_path, probed)
                        .await?;
                }
                LocalScanWork::RemoteStream { path } => {
                    upsert_local_remote_stream(pool, &library_path, &path).await?;
                }
                LocalScanWork::Generic { path } => {
                    let probed = extractor.extract(&path).await?;
                    upsert_local_file(pool, &library_path, &path, probed).await?;
                }
            }
            report.scanned_files += 1;
            processed += 1;
            if let Some(sink) = progress {
                if total_items > 0 {
                    sink.report(processed as f32 / total_items as f32);
                }
            }
        }
        let now = Utc::now();
        update_library_path_scan(pool, library_path.id, now).await?;
    }

    update_library_scan(pool, config.library_id, Utc::now()).await?;
    if let Some(sink) = progress {
        sink.report(1.0);
    }
    Ok(report)
}

/// Walks the music-video library root and yields `(artist_folder, file_path)`
/// pairs for each scannable video file, where `artist_folder` is the
/// top-level directory under the library root that owns the file.
///
/// Mirrors `ErsatzTV`'s per-artist sub-folder layout: each immediate
/// subdirectory of the library path is treated as one artist, matching
/// `MusicVideoFolderScanner.ListSubdirectories` behavior.
fn collect_music_video_files(root: &str) -> Vec<(String, PathBuf)> {
    let mut files = Vec::new();
    let Ok(entries) = std::fs::read_dir(root) else {
        return files;
    };
    let mut artist_folders: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
    artist_folders.sort();
    for artist_folder in artist_folders {
        if !artist_folder.is_dir() {
            continue;
        }
        let artist_name = artist_folder
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_owned();
        if artist_name.starts_with('.') {
            continue;
        }
        let mut queue = VecDeque::from([artist_folder.clone()]);
        let mut artist_files = Vec::new();
        while let Some(path) = queue.pop_front() {
            let Ok(entries) = std::fs::read_dir(&path) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    queue.push_back(path);
                } else if is_scannable_for_kind(&path, LibraryMediaKind::MusicVideos) {
                    artist_files.push(path);
                }
            }
        }
        artist_files.sort();
        for file_path in artist_files {
            files.push((artist_name.clone(), file_path));
        }
    }
    files
}

/// Walks the remote-stream library root and yields `.yml`/`.yaml` definition
/// files. `ErsatzTV`'s `RemoteStreamFolderScanner` parses the same extensions.
fn collect_remote_stream_files(root: &str) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut queue = VecDeque::from([PathBuf::from(root)]);
    while let Some(path) = queue.pop_front() {
        let Ok(entries) = std::fs::read_dir(&path) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                queue.push_back(path);
                continue;
            }
            let extension = path
                .extension()
                .and_then(|extension| extension.to_str())
                .map(str::to_ascii_lowercase);
            if matches!(extension.as_deref(), Some("yml" | "yaml")) {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

fn collect_scannable_files(root: &str, media_kind: LibraryMediaKind) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut queue = VecDeque::from([PathBuf::from(root)]);

    while let Some(path) = queue.pop_front() {
        let Ok(entries) = std::fs::read_dir(&path) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                queue.push_back(path);
            } else if is_scannable_for_kind(&path, media_kind) {
                files.push(path);
            }
        }
    }

    files.sort();
    files
}

fn is_scannable_for_kind(path: &Path, media_kind: LibraryMediaKind) -> bool {
    if path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with("._"))
    {
        return false;
    }

    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase);

    matches!(
        (media_kind, extension.as_deref()),
        (
            LibraryMediaKind::Movies
                | LibraryMediaKind::OtherVideos
                | LibraryMediaKind::MusicVideos,
            Some("mp4" | "mkv" | "avi" | "mov" | "m4v" | "webm")
        ) | (
            LibraryMediaKind::Songs,
            Some("mp3" | "flac" | "m4a" | "aac" | "ogg" | "opus" | "wav")
        ) | (
            LibraryMediaKind::Images,
            Some("jpg" | "jpeg" | "png" | "webp")
        )
    )
}

async fn load_library_paths(pool: &DbPool, library_id: i64) -> Result<Vec<LocalLibraryPath>> {
    let rows = sqlx::query(
        r#"SELECT "lp"."Id", "lp"."Path", "l"."MediaKind"
           FROM "LibraryPath" AS "lp"
           JOIN "Library" AS "l" ON "l"."Id" = "lp"."LibraryId"
           WHERE "lp"."LibraryId" = ?
           ORDER BY "lp"."Id""#,
    )
    .bind(library_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| LocalLibraryPath {
            id: row.get("Id"),
            path: row.get("Path"),
            media_kind: decode_library_media_kind(&row),
        })
        .collect())
}

async fn upsert_artist_for_folder(
    pool: &DbPool,
    library_path_id: i64,
    artist_name: &str,
) -> Result<i64> {
    let mut tx = pool.begin().await?;
    let existing: Option<i64> = sqlx::query_scalar(
        r#"SELECT "a"."Id"
           FROM "Artist" AS "a"
           JOIN "MediaItem" AS "mi" ON "mi"."Id" = "a"."Id"
           JOIN "ArtistMetadata" AS "am" ON "am"."ArtistId" = "a"."Id"
           WHERE "mi"."LibraryPathId" = ? AND "am"."Title" = ?
           LIMIT 1"#,
    )
    .bind(library_path_id)
    .bind(artist_name)
    .fetch_optional(&mut *tx)
    .await?;

    if let Some(id) = existing {
        sqlx::query(r#"UPDATE "MediaItem" SET "State" = 0 WHERE "Id" = ?"#)
            .bind(id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        return Ok(id);
    }

    let result = sqlx::query(r#"INSERT INTO "MediaItem" ("LibraryPathId", "State") VALUES (?, 0)"#)
        .bind(library_path_id)
        .execute(&mut *tx)
        .await?;
    let id = ersatzrs_infra::db::last_insert_id(result);
    sqlx::query(r#"INSERT INTO "Artist" ("Id") VALUES (?)"#)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    let now = encode_datetime(Utc::now());
    sqlx::query(
        r#"INSERT INTO "ArtistMetadata" (
            "ArtistId", "DateAdded", "DateUpdated", "MetadataKind", "Title"
        ) VALUES (?, ?, ?, 1, ?)"#,
    )
    .bind(id)
    .bind(&now)
    .bind(&now)
    .bind(artist_name)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(id)
}

async fn upsert_local_music_video(
    pool: &DbPool,
    library_path: &LocalLibraryPath,
    artist_id: i64,
    path: &Path,
    probed: FfprobeMediaVersion,
) -> Result<()> {
    let mut tx = pool.begin().await?;
    let path_text = path.display().to_string();
    let path_hash = path_hash(&path_text);

    let maybe_existing = sqlx::query(
        r#"SELECT "mf"."MediaVersionId",
                  "mv"."MusicVideoId" AS "MediaItemId"
           FROM "MediaFile" AS "mf"
           JOIN "MediaVersion" AS "mv" ON "mv"."Id" = "mf"."MediaVersionId"
           WHERE "mf"."PathHash" = ?"#,
    )
    .bind(&path_hash)
    .fetch_optional(&mut *tx)
    .await?;

    let (item_id, version_id) = if let Some(row) = maybe_existing {
        (
            row.get::<Option<i64>, _>("MediaItemId").unwrap_or_default(),
            row.get::<i64, _>("MediaVersionId"),
        )
    } else {
        let item_id = insert_media_item(&mut tx, library_path.id).await?;
        sqlx::query(r#"INSERT INTO "MusicVideo" ("Id", "ArtistId") VALUES (?, ?)"#)
            .bind(item_id)
            .bind(artist_id)
            .execute(&mut *tx)
            .await?;
        let now = encode_datetime(Utc::now());
        let title = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or_default();
        sqlx::query(
            r#"INSERT INTO "MusicVideoMetadata" (
                "MusicVideoId", "DateAdded", "DateUpdated", "MetadataKind", "Title"
            ) VALUES (?, ?, ?, 1, ?)"#,
        )
        .bind(item_id)
        .bind(&now)
        .bind(&now)
        .bind(title)
        .execute(&mut *tx)
        .await?;
        let version_id =
            insert_media_version(&mut tx, item_id, &probed, LibraryMediaKind::MusicVideos).await?;
        sqlx::query(
            r#"INSERT INTO "MediaFile" ("MediaVersionId", "Path", "PathHash")
               VALUES (?, ?, ?)"#,
        )
        .bind(version_id)
        .bind(&path_text)
        .bind(&path_hash)
        .execute(&mut *tx)
        .await?;
        (item_id, version_id)
    };

    sqlx::query(r#"UPDATE "MediaItem" SET "LibraryPathId" = ?, "State" = 0 WHERE "Id" = ?"#)
        .bind(library_path.id)
        .bind(item_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query(r#"UPDATE "MusicVideo" SET "ArtistId" = ? WHERE "Id" = ?"#)
        .bind(artist_id)
        .bind(item_id)
        .execute(&mut *tx)
        .await?;
    update_media_version(&mut tx, version_id, &probed).await?;
    replace_streams(&mut tx, version_id, &probed.streams).await?;
    replace_chapters(&mut tx, version_id, &probed.chapters).await?;

    tx.commit().await?;
    Ok(())
}

async fn upsert_local_remote_stream(
    pool: &DbPool,
    library_path: &LocalLibraryPath,
    path: &Path,
) -> Result<()> {
    let mut tx = pool.begin().await?;
    let path_text = path.display().to_string();
    let path_hash = path_hash(&path_text);

    let maybe_existing = sqlx::query(
        r#"SELECT "mf"."MediaVersionId",
                  "mv"."RemoteStreamId" AS "MediaItemId"
           FROM "MediaFile" AS "mf"
           JOIN "MediaVersion" AS "mv" ON "mv"."Id" = "mf"."MediaVersionId"
           WHERE "mf"."PathHash" = ?"#,
    )
    .bind(&path_hash)
    .fetch_optional(&mut *tx)
    .await?;

    let title = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_default()
        .to_owned();
    let now = Utc::now();

    let item_id = if let Some(row) = maybe_existing {
        let item_id: i64 = row.get::<Option<i64>, _>("MediaItemId").unwrap_or_default();
        sqlx::query(
            r#"UPDATE "RemoteStreamMetadata" SET "DateUpdated" = ?, "Title" = ?
               WHERE "RemoteStreamId" = ?"#,
        )
        .bind(encode_datetime(now))
        .bind(&title)
        .bind(item_id)
        .execute(&mut *tx)
        .await?;
        item_id
    } else {
        let item_id = insert_media_item(&mut tx, library_path.id).await?;
        sqlx::query(r#"INSERT INTO "RemoteStream" ("Id") VALUES (?)"#)
            .bind(item_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            r#"INSERT INTO "RemoteStreamMetadata" (
                "RemoteStreamId", "DateAdded", "DateUpdated", "MetadataKind", "Title"
            ) VALUES (?, ?, ?, 1, ?)"#,
        )
        .bind(item_id)
        .bind(encode_datetime(now))
        .bind(encode_datetime(now))
        .bind(&title)
        .execute(&mut *tx)
        .await?;
        let result = sqlx::query(
            r#"INSERT INTO "MediaVersion" (
                "DateAdded", "DateUpdated", "Duration", "Height", "Name",
                "RemoteStreamId", "VideoScanKind", "Width"
            ) VALUES (?, ?, '00:00:00', 0, 'Main', ?, 0, 0)"#,
        )
        .bind(encode_datetime(now))
        .bind(encode_datetime(now))
        .bind(item_id)
        .execute(&mut *tx)
        .await?;
        let version_id = ersatzrs_infra::db::last_insert_id(result);
        sqlx::query(
            r#"INSERT INTO "MediaFile" ("MediaVersionId", "Path", "PathHash")
               VALUES (?, ?, ?)"#,
        )
        .bind(version_id)
        .bind(&path_text)
        .bind(&path_hash)
        .execute(&mut *tx)
        .await?;
        item_id
    };

    sqlx::query(r#"UPDATE "MediaItem" SET "LibraryPathId" = ?, "State" = 0 WHERE "Id" = ?"#)
        .bind(library_path.id)
        .bind(item_id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok(())
}

async fn upsert_local_file(
    pool: &DbPool,
    library_path: &LocalLibraryPath,
    path: &Path,
    probed: FfprobeMediaVersion,
) -> Result<()> {
    let mut tx = pool.begin().await?;
    let path_text = path.display().to_string();
    let path_hash = path_hash(&path_text);

    let maybe_existing = sqlx::query(
        r#"SELECT "mf"."MediaVersionId",
                  COALESCE(
                    "mv"."MovieId", "mv"."OtherVideoId", "mv"."SongId", "mv"."ImageId"
                  ) AS "MediaItemId"
           FROM "MediaFile" AS "mf"
           JOIN "MediaVersion" AS "mv" ON "mv"."Id" = "mf"."MediaVersionId"
           WHERE "mf"."PathHash" = ?"#,
    )
    .bind(&path_hash)
    .fetch_optional(&mut *tx)
    .await?;

    let (item_id, version_id) = if let Some(row) = maybe_existing {
        let item_id = row.get::<Option<i64>, _>("MediaItemId").unwrap_or_default();
        // Backfill fallback metadata for items created before this insert
        // existed in the scanner. `insert_fallback_metadata` is idempotent.
        if item_id != 0 {
            insert_fallback_metadata(&mut tx, item_id, library_path, path).await?;
        }
        (item_id, row.get::<i64, _>("MediaVersionId"))
    } else {
        let item_id = insert_media_item(&mut tx, library_path.id).await?;
        insert_media_subtype(&mut tx, item_id, library_path.media_kind).await?;
        insert_fallback_metadata(&mut tx, item_id, library_path, path).await?;
        let version_id =
            insert_media_version(&mut tx, item_id, &probed, library_path.media_kind).await?;
        sqlx::query(
            r#"INSERT INTO "MediaFile" ("MediaVersionId", "Path", "PathHash")
               VALUES (?, ?, ?)"#,
        )
        .bind(version_id)
        .bind(&path_text)
        .bind(&path_hash)
        .execute(&mut *tx)
        .await?;
        (item_id, version_id)
    };

    sqlx::query(r#"UPDATE "MediaItem" SET "LibraryPathId" = ?, "State" = 0 WHERE "Id" = ?"#)
        .bind(library_path.id)
        .bind(item_id)
        .execute(&mut *tx)
        .await?;
    update_media_version(&mut tx, version_id, &probed).await?;
    replace_streams(&mut tx, version_id, &probed.streams).await?;
    replace_chapters(&mut tx, version_id, &probed.chapters).await?;

    tx.commit().await?;
    Ok(())
}

async fn insert_media_item(tx: &mut Transaction<'_, Db>, library_path_id: i64) -> Result<i64> {
    let result = sqlx::query(r#"INSERT INTO "MediaItem" ("LibraryPathId", "State") VALUES (?, 0)"#)
        .bind(library_path_id)
        .execute(&mut **tx)
        .await?;
    Ok(ersatzrs_infra::db::last_insert_id(result))
}

async fn insert_media_version(
    tx: &mut Transaction<'_, Db>,
    item_id: i64,
    probed: &FfprobeMediaVersion,
    media_kind: LibraryMediaKind,
) -> Result<i64> {
    let version = &probed.version;
    let now = Utc::now();
    let owner_column = media_version_owner_column(media_kind)?;
    let sql = format!(
        r#"INSERT INTO "MediaVersion" (
            "DateAdded", "DateUpdated", "DisplayAspectRatio", "Duration", "Height",
            "Name", "{owner_column}", "RFrameRate", "SampleAspectRatio",
            "VideoScanKind", "Width", "InterlacedRatio"
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#
    );
    let result = sqlx::query(&sql)
        .bind(encode_datetime(version.date_added))
        .bind(encode_datetime(now))
        .bind(&version.display_aspect_ratio)
        .bind(encode_duration(version.duration))
        .bind(version.height)
        .bind(&version.name)
        .bind(item_id)
        .bind(&version.r_frame_rate)
        .bind(&version.sample_aspect_ratio)
        .bind(encode_video_scan_kind(version.video_scan_kind))
        .bind(version.width)
        .bind(version.interlaced_ratio)
        .execute(&mut **tx)
        .await?;
    Ok(ersatzrs_infra::db::last_insert_id(result))
}

async fn insert_media_subtype(
    tx: &mut Transaction<'_, Db>,
    item_id: i64,
    media_kind: LibraryMediaKind,
) -> Result<()> {
    let table = match media_kind {
        LibraryMediaKind::Movies => "Movie",
        LibraryMediaKind::OtherVideos => "OtherVideo",
        LibraryMediaKind::Songs => "Song",
        LibraryMediaKind::Images => "Image",
        LibraryMediaKind::Shows
        | LibraryMediaKind::MusicVideos
        | LibraryMediaKind::RemoteStreams => {
            return Err(ScannerError::UnsupportedLibraryMediaKind(media_kind));
        }
    };
    let sql = format!(r#"INSERT INTO "{table}" ("Id") VALUES (?)"#);
    sqlx::query(&sql).bind(item_id).execute(&mut **tx).await?;
    Ok(())
}

/// Inserts a fallback metadata row for the newly created media item so the
/// admin browser queries (which `INNER JOIN` on the subtype's metadata table)
/// have something to return.
///
/// Idempotent: if a metadata row already exists for this item (any
/// `MetadataKind`), this is a no-op. That lets us call it from both the
/// "newly inserted" and the "existing row" branches of `upsert_local_file`
/// so libraries scanned before fallback metadata was introduced get
/// backfilled on the next scan.
///
/// Mirrors `ErsatzTV`'s `FallbackMetadataProvider.GetFallbackMetadata*` — the
/// title defaults to the file stem and `OriginalTitle` is the path relative to
/// the library root. Tags / artwork / actors are intentionally deferred; the
/// folder scanner can enrich the row later without disturbing this insert.
async fn fallback_metadata_exists(
    tx: &mut Transaction<'_, Db>,
    item_id: i64,
    media_kind: LibraryMediaKind,
) -> Result<bool> {
    let (table, column) = match media_kind {
        LibraryMediaKind::Movies => ("MovieMetadata", "MovieId"),
        LibraryMediaKind::OtherVideos => ("OtherVideoMetadata", "OtherVideoId"),
        LibraryMediaKind::Songs => ("SongMetadata", "SongId"),
        LibraryMediaKind::Images => ("ImageMetadata", "ImageId"),
        LibraryMediaKind::MusicVideos
        | LibraryMediaKind::RemoteStreams
        | LibraryMediaKind::Shows => return Ok(true),
    };
    let sql = format!(r#"SELECT EXISTS(SELECT 1 FROM "{table}" WHERE "{column}" = ?)"#);
    let exists: i64 = sqlx::query_scalar(&sql)
        .bind(item_id)
        .fetch_one(&mut **tx)
        .await?;
    Ok(exists != 0)
}

async fn insert_fallback_metadata(
    tx: &mut Transaction<'_, Db>,
    item_id: i64,
    library_path: &LocalLibraryPath,
    path: &Path,
) -> Result<()> {
    if fallback_metadata_exists(tx, item_id, library_path.media_kind).await? {
        return Ok(());
    }

    let title = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_default()
        .to_owned();
    let original_title = std::path::Path::new(path)
        .strip_prefix(&library_path.path)
        .ok()
        .and_then(|p| p.to_str())
        .map(str::to_owned);
    let now = encode_datetime(Utc::now());

    match library_path.media_kind {
        LibraryMediaKind::Movies => {
            sqlx::query(
                r#"INSERT INTO "MovieMetadata" (
                    "MovieId", "DateAdded", "DateUpdated", "MetadataKind",
                    "Title", "SortTitle", "OriginalTitle"
                ) VALUES (?, ?, ?, 1, ?, ?, ?)"#,
            )
            .bind(item_id)
            .bind(&now)
            .bind(&now)
            .bind(&title)
            .bind(&title)
            .bind(&original_title)
            .execute(&mut **tx)
            .await?;
        }
        LibraryMediaKind::OtherVideos => {
            sqlx::query(
                r#"INSERT INTO "OtherVideoMetadata" (
                    "OtherVideoId", "DateAdded", "DateUpdated", "MetadataKind",
                    "Title", "SortTitle", "OriginalTitle"
                ) VALUES (?, ?, ?, 1, ?, ?, ?)"#,
            )
            .bind(item_id)
            .bind(&now)
            .bind(&now)
            .bind(&title)
            .bind(&title)
            .bind(&original_title)
            .execute(&mut **tx)
            .await?;
        }
        LibraryMediaKind::Songs => {
            sqlx::query(
                r#"INSERT INTO "SongMetadata" (
                    "SongId", "DateAdded", "DateUpdated", "MetadataKind",
                    "Title", "SortTitle", "OriginalTitle"
                ) VALUES (?, ?, ?, 1, ?, ?, ?)"#,
            )
            .bind(item_id)
            .bind(&now)
            .bind(&now)
            .bind(&title)
            .bind(&title)
            .bind(&original_title)
            .execute(&mut **tx)
            .await?;
        }
        LibraryMediaKind::Images => {
            sqlx::query(
                r#"INSERT INTO "ImageMetadata" (
                    "ImageId", "DateAdded", "DateUpdated", "MetadataKind",
                    "Title", "SortTitle", "OriginalTitle"
                ) VALUES (?, ?, ?, 1, ?, ?, ?)"#,
            )
            .bind(item_id)
            .bind(&now)
            .bind(&now)
            .bind(&title)
            .bind(&title)
            .bind(&original_title)
            .execute(&mut **tx)
            .await?;
        }
        LibraryMediaKind::MusicVideos
        | LibraryMediaKind::RemoteStreams
        | LibraryMediaKind::Shows => {
            // MusicVideo / RemoteStream metadata is written by their dedicated
            // upsert paths; Shows are not supported by the local scanner.
        }
    }
    Ok(())
}

fn media_version_owner_column(media_kind: LibraryMediaKind) -> Result<&'static str> {
    match media_kind {
        LibraryMediaKind::Movies => Ok("MovieId"),
        LibraryMediaKind::OtherVideos => Ok("OtherVideoId"),
        LibraryMediaKind::Songs => Ok("SongId"),
        LibraryMediaKind::Images => Ok("ImageId"),
        LibraryMediaKind::MusicVideos => Ok("MusicVideoId"),
        LibraryMediaKind::RemoteStreams => Ok("RemoteStreamId"),
        LibraryMediaKind::Shows => Err(ScannerError::UnsupportedLibraryMediaKind(media_kind)),
    }
}

async fn update_media_version(
    tx: &mut Transaction<'_, Db>,
    version_id: i64,
    probed: &FfprobeMediaVersion,
) -> Result<()> {
    let version = &probed.version;
    sqlx::query(
        r#"UPDATE "MediaVersion"
           SET "DateUpdated" = ?, "DisplayAspectRatio" = ?, "Duration" = ?,
               "Height" = ?, "Name" = ?, "RFrameRate" = ?,
               "SampleAspectRatio" = ?, "VideoScanKind" = ?, "Width" = ?,
               "InterlacedRatio" = ?
           WHERE "Id" = ?"#,
    )
    .bind(encode_datetime(Utc::now()))
    .bind(&version.display_aspect_ratio)
    .bind(encode_duration(version.duration))
    .bind(version.height)
    .bind(&version.name)
    .bind(&version.r_frame_rate)
    .bind(&version.sample_aspect_ratio)
    .bind(encode_video_scan_kind(version.video_scan_kind))
    .bind(version.width)
    .bind(version.interlaced_ratio)
    .bind(version_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn replace_streams(
    tx: &mut Transaction<'_, Db>,
    version_id: i64,
    streams: &[MediaStream],
) -> Result<()> {
    sqlx::query(r#"DELETE FROM "MediaStream" WHERE "MediaVersionId" = ?"#)
        .bind(version_id)
        .execute(&mut **tx)
        .await?;

    for stream in streams {
        sqlx::query(
            r#"INSERT INTO "MediaStream" (
                "Index", "Codec", "Profile", "MediaStreamKind", "Language", "Channels",
                "Title", "Default", "Forced", "MediaVersionId",
                "PixelFormat", "ColorRange", "ColorSpace", "ColorTransfer", "ColorPrimaries",
                "BitsPerRawSample", "AttachedPic", "FileName", "MimeType"
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(stream.index)
        .bind(&stream.codec)
        .bind(&stream.profile)
        .bind(encode_stream_kind(stream.kind))
        .bind(&stream.language)
        .bind(stream.channels.unwrap_or_default())
        .bind(&stream.title)
        .bind(stream.default)
        .bind(stream.forced)
        .bind(version_id)
        .bind(&stream.pixel_format)
        .bind(&stream.color_range)
        .bind(&stream.color_space)
        .bind(&stream.color_transfer)
        .bind(&stream.color_primaries)
        .bind(stream.bits_per_raw_sample)
        .bind(stream.attached_pic)
        .bind(&stream.file_name)
        .bind(&stream.mime_type)
        .execute(&mut **tx)
        .await?;
    }

    Ok(())
}

async fn replace_chapters(
    tx: &mut Transaction<'_, Db>,
    version_id: i64,
    chapters: &[MediaChapter],
) -> Result<()> {
    sqlx::query(r#"DELETE FROM "MediaChapter" WHERE "MediaVersionId" = ?"#)
        .bind(version_id)
        .execute(&mut **tx)
        .await?;

    for chapter in chapters {
        sqlx::query(
            r#"INSERT INTO "MediaChapter" (
                "MediaVersionId", "ChapterId", "StartTime", "EndTime", "Title"
            ) VALUES (?, ?, ?, ?, ?)"#,
        )
        .bind(version_id)
        .bind(chapter.chapter_id)
        .bind(encode_duration(chapter.start_time))
        .bind(encode_duration(chapter.end_time))
        .bind(&chapter.title)
        .execute(&mut **tx)
        .await?;
    }

    Ok(())
}

async fn update_library_path_scan(
    pool: &DbPool,
    library_path_id: i64,
    when: DateTime<Utc>,
) -> Result<()> {
    sqlx::query(r#"UPDATE "LibraryPath" SET "LastScan" = ? WHERE "Id" = ?"#)
        .bind(encode_datetime(when))
        .bind(library_path_id)
        .execute(pool)
        .await?;
    Ok(())
}

async fn update_library_scan(pool: &DbPool, library_id: i64, when: DateTime<Utc>) -> Result<()> {
    sqlx::query(r#"UPDATE "Library" SET "LastScan" = ? WHERE "Id" = ?"#)
        .bind(encode_datetime(when))
        .bind(library_id)
        .execute(pool)
        .await?;
    Ok(())
}

fn path_hash(path: &str) -> String {
    let digest = Sha256::digest(path.as_bytes());
    let mut hash = String::with_capacity(digest.len() * 2);

    for byte in digest {
        use std::fmt::Write as _;

        write!(&mut hash, "{byte:02x}").expect("writing to String should not fail");
    }

    hash
}

fn encode_datetime(value: DateTime<Utc>) -> String {
    value.naive_utc().format("%Y-%m-%d %H:%M:%S").to_string()
}

fn encode_duration(value: Duration) -> String {
    let total_seconds = value.num_seconds().max(0);
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;
    format!("{hours:02}:{minutes:02}:{seconds:02}")
}

fn encode_stream_kind(kind: MediaStreamKind) -> i32 {
    match kind {
        MediaStreamKind::Video => 0,
        MediaStreamKind::Audio => 1,
        MediaStreamKind::Subtitle => 2,
        MediaStreamKind::Attachment => 3,
        MediaStreamKind::ExternalSubtitle => 4,
    }
}

fn encode_video_scan_kind(kind: VideoScanKind) -> i32 {
    match kind {
        VideoScanKind::Progressive => 0,
        VideoScanKind::Interlaced => 1,
        VideoScanKind::Unknown => 2,
    }
}

#[derive(Debug, Clone)]
struct LocalLibraryPath {
    id: i64,
    path: String,
    media_kind: LibraryMediaKind,
}

fn decode_library_media_kind(row: &DbRow) -> LibraryMediaKind {
    if let Ok(value) = row.try_get::<i32, _>("MediaKind") {
        if value == 4 {
            return LibraryMediaKind::OtherVideos;
        }
        return match value {
            1 => LibraryMediaKind::Movies,
            2 => LibraryMediaKind::Shows,
            3 => LibraryMediaKind::MusicVideos,
            5 => LibraryMediaKind::Songs,
            6 => LibraryMediaKind::Images,
            7 => LibraryMediaKind::RemoteStreams,
            _ => LibraryMediaKind::OtherVideos,
        };
    }

    let value = row.try_get::<String, _>("MediaKind").unwrap_or_default();
    if value == "OtherVideos" {
        return LibraryMediaKind::OtherVideos;
    }

    match value.as_str() {
        "Movies" => LibraryMediaKind::Movies,
        "Shows" => LibraryMediaKind::Shows,
        "MusicVideos" => LibraryMediaKind::MusicVideos,
        "Songs" => LibraryMediaKind::Songs,
        "Images" => LibraryMediaKind::Images,
        "RemoteStreams" => LibraryMediaKind::RemoteStreams,
        _ => LibraryMediaKind::OtherVideos,
    }
}

#[cfg(test)]
mod tests {
    use super::{is_scannable_for_kind, path_hash};
    use ersatzrs_domain::LibraryMediaKind;
    use std::path::Path;

    #[test]
    fn path_hash_matches_sha256sum_output() {
        assert_eq!(
            path_hash("/media/movie.mkv"),
            "255ca6b14e280f5d5ca28f4e6e010b19038627dbd263a1d17556460b7ee06e24"
        );
    }

    #[test]
    fn local_scanner_classifies_extensions_by_library_kind() {
        assert!(is_scannable_for_kind(
            Path::new("/media/movie.mkv"),
            LibraryMediaKind::Movies
        ));
        assert!(is_scannable_for_kind(
            Path::new("/media/song.flac"),
            LibraryMediaKind::Songs
        ));
        assert!(is_scannable_for_kind(
            Path::new("/media/image.webp"),
            LibraryMediaKind::Images
        ));
        assert!(!is_scannable_for_kind(
            Path::new("/media/song.flac"),
            LibraryMediaKind::Movies
        ));
    }
}
