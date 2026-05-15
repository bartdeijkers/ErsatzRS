#![forbid(unsafe_code)]

use std::collections::VecDeque;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration, Utc};
use ersatzrs_core::repositories::RepositoryError;
use ersatzrs_domain::{
    LibraryMediaKind, MediaChapter, MediaStream, MediaStreamKind, ScanProgressSink, VideoScanKind,
};
use ersatzrs_infra::db::{Db, DbPool, DbRow};
use ersatzrs_infra::ffprobe::{FfprobeError, FfprobeExtractor, FfprobeMediaVersion};
use ersatzrs_infra::metadata::{
    artwork_kind, replace_artwork, replace_people, replace_strings, ArtworkRef, PersonRef,
};
use sha2::{Digest, Sha256};
use sqlx::{Row, Transaction};
use thiserror::Error;
use tracing::warn;

pub mod local_television;
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
    #[error(transparent)]
    Repository(#[from] RepositoryError),
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

enum LocalScanBucketKind {
    Items(Vec<LocalScanWork>),
    Shows { expected_episodes: usize },
}

struct LocalScanBucket {
    library_path: LocalLibraryPath,
    kind: LocalScanBucketKind,
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
        let kind = match library_path.media_kind {
            LibraryMediaKind::Shows => {
                let expected_episodes = local_television::count_episode_files(&library_path.path);
                total_items = total_items.saturating_add(expected_episodes);
                LocalScanBucketKind::Shows { expected_episodes }
            }
            LibraryMediaKind::MusicVideos => {
                let items: Vec<LocalScanWork> = collect_music_video_files(&library_path.path)
                    .into_iter()
                    .map(|(artist_folder, file_path)| LocalScanWork::MusicVideo {
                        artist_folder,
                        file_path,
                    })
                    .collect();
                total_items = total_items.saturating_add(items.len());
                LocalScanBucketKind::Items(items)
            }
            LibraryMediaKind::RemoteStreams => {
                let items: Vec<LocalScanWork> = collect_remote_stream_files(&library_path.path)
                    .into_iter()
                    .map(|path| LocalScanWork::RemoteStream { path })
                    .collect();
                total_items = total_items.saturating_add(items.len());
                LocalScanBucketKind::Items(items)
            }
            _ => {
                let items: Vec<LocalScanWork> =
                    collect_scannable_files(&library_path.path, library_path.media_kind)
                        .into_iter()
                        .map(|path| LocalScanWork::Generic { path })
                        .collect();
                total_items = total_items.saturating_add(items.len());
                LocalScanBucketKind::Items(items)
            }
        };
        buckets.push(LocalScanBucket { library_path, kind });
    }

    if let Some(sink) = progress {
        sink.report(0.0);
    }

    let extractor = FfprobeExtractor::new(config.ffprobe_path.clone());
    let mut report = LocalScanReport::default();
    let mut processed: usize = 0;
    for bucket in buckets {
        let library_path = bucket.library_path;
        match bucket.kind {
            LocalScanBucketKind::Items(items) => {
                for work in items {
                    match work {
                        LocalScanWork::MusicVideo {
                            artist_folder,
                            file_path,
                        } => {
                            let artist_id =
                                upsert_artist_for_folder(pool, &library_path, &artist_folder)
                                    .await?;
                            let artist_dir = Path::new(&library_path.path).join(&artist_folder);
                            let probed = extractor.extract(&file_path).await?;
                            upsert_local_music_video(
                                pool,
                                &library_path,
                                artist_id,
                                &artist_dir,
                                &file_path,
                                probed,
                            )
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
            }
            LocalScanBucketKind::Shows { expected_episodes } => {
                let mut local_processed = 0usize;
                let scanned = local_television::sync_local_show_library(
                    pool,
                    &library_path,
                    &extractor,
                    || {
                        local_processed += 1;
                        if let Some(sink) = progress {
                            if total_items > 0 {
                                let done = processed.saturating_add(local_processed);
                                sink.report(done as f32 / total_items as f32);
                            }
                        }
                    },
                )
                .await?;
                report.scanned_files = report.scanned_files.saturating_add(scanned);
                processed = processed.saturating_add(expected_episodes.max(scanned));
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

#[allow(clippy::too_many_lines)]
async fn upsert_artist_for_folder(
    pool: &DbPool,
    library_path: &LocalLibraryPath,
    artist_name: &str,
) -> Result<i64> {
    let artist_folder = Path::new(&library_path.path).join(artist_name);

    let nfo_path = locate_artist_nfo(&artist_folder);
    let parsed_nfo = nfo_path.as_ref().and_then(|path| match nfo::parse_artist_nfo(path) {
        Ok(parsed) => Some(parsed),
        Err(err) => {
            warn!(path = %path.display(), error = %err, "failed to parse artist.nfo; falling back");
            None
        }
    });

    let title = parsed_nfo
        .as_ref()
        .and_then(|n| n.name.clone())
        .unwrap_or_else(|| artist_name.to_owned());
    let sort_title = parsed_nfo
        .as_ref()
        .and_then(|n| n.sort_title.clone())
        .unwrap_or_else(|| nfo::sort_title(&title));
    let disambiguation = parsed_nfo.as_ref().and_then(|n| n.disambiguation.clone());
    let biography = parsed_nfo.as_ref().and_then(|n| n.biography.clone());
    let year = parsed_nfo.as_ref().and_then(|n| n.year);
    let formed = parsed_nfo
        .as_ref()
        .and_then(|n| n.formed)
        .map(|dt| dt.naive_utc().format("%Y-%m-%d").to_string());
    let metadata_kind = i32::from(parsed_nfo.is_some());

    let mut tx = pool.begin().await?;

    let existing: Option<(i64, Option<i64>)> = sqlx::query(
        r#"SELECT "a"."Id" AS "ArtistId", "am"."Id" AS "ArtistMetadataId"
           FROM "Artist" AS "a"
           JOIN "MediaItem" AS "mi" ON "mi"."Id" = "a"."Id"
           LEFT JOIN "ArtistMetadata" AS "am" ON "am"."ArtistId" = "a"."Id"
           WHERE "mi"."LibraryPathId" = ? AND COALESCE("am"."Title", '') = ?
           LIMIT 1"#,
    )
    .bind(library_path.id)
    .bind(artist_name)
    .fetch_optional(&mut *tx)
    .await?
    .map(|row| (row.get("ArtistId"), row.get("ArtistMetadataId")));

    let now = encode_datetime(Utc::now());

    let (artist_id, metadata_id) = if let Some((artist_id, metadata_id)) = existing {
        sqlx::query(r#"UPDATE "MediaItem" SET "State" = 0 WHERE "Id" = ?"#)
            .bind(artist_id)
            .execute(&mut *tx)
            .await?;
        let metadata_id = if let Some(id) = metadata_id {
            sqlx::query(
                r#"UPDATE "ArtistMetadata"
                   SET "DateUpdated" = ?, "MetadataKind" = ?, "Title" = ?,
                       "SortTitle" = ?, "Disambiguation" = ?, "Biography" = ?,
                       "Year" = ?, "Formed" = ?
                   WHERE "Id" = ?"#,
            )
            .bind(&now)
            .bind(metadata_kind)
            .bind(&title)
            .bind(&sort_title)
            .bind(&disambiguation)
            .bind(&biography)
            .bind(year)
            .bind(&formed)
            .bind(id)
            .execute(&mut *tx)
            .await?;
            id
        } else {
            insert_artist_metadata(
                &mut tx,
                artist_id,
                &title,
                &sort_title,
                disambiguation.as_deref(),
                biography.as_deref(),
                year,
                formed.as_deref(),
                metadata_kind,
                &now,
            )
            .await?
        };
        (artist_id, metadata_id)
    } else {
        let result =
            sqlx::query(r#"INSERT INTO "MediaItem" ("LibraryPathId", "State") VALUES (?, 0)"#)
                .bind(library_path.id)
                .execute(&mut *tx)
                .await?;
        let artist_id = ersatzrs_infra::db::last_insert_id(result);
        sqlx::query(r#"INSERT INTO "Artist" ("Id") VALUES (?)"#)
            .bind(artist_id)
            .execute(&mut *tx)
            .await?;
        let metadata_id = insert_artist_metadata(
            &mut tx,
            artist_id,
            &title,
            &sort_title,
            disambiguation.as_deref(),
            biography.as_deref(),
            year,
            formed.as_deref(),
            metadata_kind,
            &now,
        )
        .await?;
        (artist_id, metadata_id)
    };

    let genres = parsed_nfo
        .as_ref()
        .map(|n| n.genres.clone())
        .unwrap_or_default();
    let styles = parsed_nfo
        .as_ref()
        .map(|n| n.styles.clone())
        .unwrap_or_default();
    let moods = parsed_nfo
        .as_ref()
        .map(|n| n.moods.clone())
        .unwrap_or_default();
    replace_strings(&mut tx, "Genre", "ArtistMetadataId", metadata_id, &genres).await?;
    replace_strings(&mut tx, "Style", "ArtistMetadataId", metadata_id, &styles).await?;
    replace_strings(&mut tx, "Mood", "ArtistMetadataId", metadata_id, &moods).await?;

    let mut artwork = Vec::new();
    if let Some(path) = locate_artist_artwork(&artist_folder, &["thumb", "profile"]) {
        artwork.push(artwork_ref(&path, artwork_kind::THUMBNAIL));
    }
    if let Some(path) = locate_artist_artwork(&artist_folder, &["fanart", "backdrop"]) {
        artwork.push(artwork_ref(&path, artwork_kind::FAN_ART));
    }
    if let Some(path) = locate_artist_artwork(&artist_folder, &["logo"]) {
        artwork.push(artwork_ref(&path, artwork_kind::LOGO));
    }
    replace_artwork(
        &mut tx,
        "ArtistMetadataId",
        metadata_id,
        &[
            artwork_kind::THUMBNAIL,
            artwork_kind::FAN_ART,
            artwork_kind::LOGO,
        ],
        &artwork,
    )
    .await?;

    tx.commit().await?;
    Ok(artist_id)
}

#[allow(clippy::too_many_arguments)]
async fn insert_artist_metadata(
    tx: &mut Transaction<'_, Db>,
    artist_id: i64,
    title: &str,
    sort_title: &str,
    disambiguation: Option<&str>,
    biography: Option<&str>,
    year: Option<i32>,
    formed: Option<&str>,
    metadata_kind: i32,
    now: &str,
) -> Result<i64> {
    let result = sqlx::query(
        r#"INSERT INTO "ArtistMetadata" (
            "ArtistId", "DateAdded", "DateUpdated", "MetadataKind",
            "Title", "SortTitle", "Disambiguation", "Biography",
            "Year", "Formed"
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
    )
    .bind(artist_id)
    .bind(now)
    .bind(now)
    .bind(metadata_kind)
    .bind(title)
    .bind(sort_title)
    .bind(disambiguation)
    .bind(biography)
    .bind(year)
    .bind(formed)
    .execute(&mut **tx)
    .await?;
    Ok(ersatzrs_infra::db::last_insert_id(result))
}

fn locate_artist_nfo(artist_folder: &Path) -> Option<PathBuf> {
    let candidate = artist_folder.join("artist.nfo");
    candidate.is_file().then_some(candidate)
}

fn locate_artist_artwork(artist_folder: &Path, names: &[&str]) -> Option<PathBuf> {
    const EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "tbn", "webp"];
    for name in names {
        for ext in EXTENSIONS {
            let candidate = artist_folder.join(format!("{name}.{ext}"));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

fn artwork_ref(path: &Path, kind: i32) -> ArtworkRef {
    ArtworkRef {
        kind,
        path: path.display().to_string(),
        date_added: Utc::now(),
    }
}

#[allow(clippy::too_many_lines)]
async fn upsert_local_music_video(
    pool: &DbPool,
    library_path: &LocalLibraryPath,
    artist_id: i64,
    artist_folder: &Path,
    path: &Path,
    probed: FfprobeMediaVersion,
) -> Result<()> {
    let nfo_path = locate_music_video_nfo(path);
    let parsed_nfo = nfo_path
        .as_ref()
        .and_then(|p| match nfo::parse_music_video_nfo(p) {
            Ok(parsed) => Some(parsed),
            Err(err) => {
                warn!(path = %p.display(), error = %err, "failed to parse music video nfo; falling back");
                None
            }
        });

    let fallback_title = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_default()
        .to_owned();
    let title = parsed_nfo
        .as_ref()
        .and_then(|n| n.title.clone())
        .unwrap_or_else(|| fallback_title.clone());
    let sort_title = nfo::sort_title(&title);
    let album = parsed_nfo.as_ref().and_then(|n| n.album.clone());
    let plot = parsed_nfo.as_ref().and_then(|n| n.plot.clone());
    let track = parsed_nfo.as_ref().and_then(|n| n.track);
    let release_date = parsed_nfo
        .as_ref()
        .and_then(|n| n.aired.or(n.premiered))
        .map(|dt| dt.naive_utc().format("%Y-%m-%d").to_string());
    let year = parsed_nfo.as_ref().and_then(|n| {
        n.year.or_else(|| {
            n.aired
                .or(n.premiered)
                .and_then(|dt| dt.naive_utc().date().format("%Y").to_string().parse().ok())
        })
    });
    let metadata_kind = i32::from(parsed_nfo.is_some());

    let mut tx = pool.begin().await?;
    let path_text = path.display().to_string();
    let path_hash = path_hash(&path_text);
    let now = encode_datetime(Utc::now());

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

    let metadata_id = upsert_music_video_metadata(
        &mut tx,
        item_id,
        &title,
        &sort_title,
        album.as_deref(),
        plot.as_deref(),
        year,
        release_date.as_deref(),
        track,
        metadata_kind,
        &now,
    )
    .await?;

    let genres = parsed_nfo
        .as_ref()
        .map(|n| n.genres.clone())
        .unwrap_or_default();
    let tags = parsed_nfo
        .as_ref()
        .map(|n| n.tags.clone())
        .unwrap_or_default();
    let studios = parsed_nfo
        .as_ref()
        .map(|n| n.studios.clone())
        .unwrap_or_default();
    let directors = parsed_nfo
        .as_ref()
        .map(|n| {
            n.directors
                .iter()
                .cloned()
                .map(PersonRef::new)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let artists = parsed_nfo
        .as_ref()
        .map(|n| n.artists.clone())
        .unwrap_or_default();
    replace_strings(
        &mut tx,
        "Genre",
        "MusicVideoMetadataId",
        metadata_id,
        &genres,
    )
    .await?;
    replace_strings(&mut tx, "Tag", "MusicVideoMetadataId", metadata_id, &tags).await?;
    replace_strings(
        &mut tx,
        "Studio",
        "MusicVideoMetadataId",
        metadata_id,
        &studios,
    )
    .await?;
    replace_people(
        &mut tx,
        "Director",
        "MusicVideoMetadataId",
        metadata_id,
        &directors,
    )
    .await?;
    replace_strings(
        &mut tx,
        "MusicVideoArtist",
        "MusicVideoMetadataId",
        metadata_id,
        &artists,
    )
    .await?;

    let mut artwork = Vec::new();
    if let Some(poster) =
        locate_music_video_artwork(path, artist_folder, &["poster"], &["-poster"], &[])
    {
        artwork.push(artwork_ref(&poster, artwork_kind::POSTER));
    }
    if let Some(thumb) =
        locate_music_video_artwork(path, artist_folder, &["thumb"], &["-thumb"], &[".tbn"])
    {
        artwork.push(artwork_ref(&thumb, artwork_kind::THUMBNAIL));
    }
    if let Some(fanart) = locate_music_video_artwork(
        path,
        artist_folder,
        &["fanart", "backdrop"],
        &["-fanart"],
        &[],
    ) {
        artwork.push(artwork_ref(&fanart, artwork_kind::FAN_ART));
    }
    replace_artwork(
        &mut tx,
        "MusicVideoMetadataId",
        metadata_id,
        &[
            artwork_kind::POSTER,
            artwork_kind::THUMBNAIL,
            artwork_kind::FAN_ART,
        ],
        &artwork,
    )
    .await?;

    tx.commit().await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn upsert_music_video_metadata(
    tx: &mut Transaction<'_, Db>,
    music_video_id: i64,
    title: &str,
    sort_title: &str,
    album: Option<&str>,
    plot: Option<&str>,
    year: Option<i32>,
    release_date: Option<&str>,
    track: Option<i32>,
    metadata_kind: i32,
    now: &str,
) -> Result<i64> {
    let existing: Option<i64> = sqlx::query_scalar(
        r#"SELECT "Id" FROM "MusicVideoMetadata" WHERE "MusicVideoId" = ? LIMIT 1"#,
    )
    .bind(music_video_id)
    .fetch_optional(&mut **tx)
    .await?;
    if let Some(id) = existing {
        sqlx::query(
            r#"UPDATE "MusicVideoMetadata"
               SET "DateUpdated" = ?, "MetadataKind" = ?, "Title" = ?,
                   "SortTitle" = ?, "Album" = ?, "Plot" = ?, "Year" = ?,
                   "ReleaseDate" = ?, "Track" = ?
               WHERE "Id" = ?"#,
        )
        .bind(now)
        .bind(metadata_kind)
        .bind(title)
        .bind(sort_title)
        .bind(album)
        .bind(plot)
        .bind(year)
        .bind(release_date)
        .bind(track)
        .bind(id)
        .execute(&mut **tx)
        .await?;
        Ok(id)
    } else {
        let result = sqlx::query(
            r#"INSERT INTO "MusicVideoMetadata" (
                "MusicVideoId", "DateAdded", "DateUpdated", "MetadataKind",
                "Title", "SortTitle", "Album", "Plot", "Year", "ReleaseDate", "Track"
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(music_video_id)
        .bind(now)
        .bind(now)
        .bind(metadata_kind)
        .bind(title)
        .bind(sort_title)
        .bind(album)
        .bind(plot)
        .bind(year)
        .bind(release_date)
        .bind(track)
        .execute(&mut **tx)
        .await?;
        Ok(ersatzrs_infra::db::last_insert_id(result))
    }
}

fn locate_music_video_nfo(video_path: &Path) -> Option<PathBuf> {
    let candidate = video_path.with_extension("nfo");
    candidate.is_file().then_some(candidate)
}

/// Locate music-video artwork following `ErsatzTV`'s discovery order:
/// 1. `{video_basename}{per_video_suffix}.{ext}` next to the video
///    (e.g. `Song-poster.jpg`, `Song-thumb.png`).
/// 2. Bare `{video_basename}{exact_ext}` (used for `.tbn`).
/// 3. Generic `{folder_name}.{ext}` next to the video (e.g. `poster.jpg`).
/// 4. Same generic name in the parent artist folder as a final fallback.
fn locate_music_video_artwork(
    video_path: &Path,
    artist_folder: &Path,
    folder_names: &[&str],
    per_video_suffixes: &[&str],
    exact_extensions: &[&str],
) -> Option<PathBuf> {
    const EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "tbn", "webp"];
    let video_dir = video_path.parent().unwrap_or(artist_folder);
    let basename = video_path.file_stem().and_then(|s| s.to_str())?;

    for suffix in per_video_suffixes {
        for ext in EXTENSIONS {
            let candidate = video_dir.join(format!("{basename}{suffix}.{ext}"));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    for ext in exact_extensions {
        let candidate = video_dir.join(format!("{basename}{ext}"));
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    for name in folder_names {
        for ext in EXTENSIONS {
            let candidate = video_dir.join(format!("{name}.{ext}"));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    for name in folder_names {
        for ext in EXTENSIONS {
            let candidate = artist_folder.join(format!("{name}.{ext}"));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
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

pub(crate) async fn insert_media_item(
    tx: &mut Transaction<'_, Db>,
    library_path_id: i64,
) -> Result<i64> {
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
    let owner_column = media_version_owner_column(media_kind)?;
    insert_media_version_with_owner(tx, item_id, probed, owner_column).await
}

pub(crate) async fn insert_media_version_with_owner(
    tx: &mut Transaction<'_, Db>,
    item_id: i64,
    probed: &FfprobeMediaVersion,
    owner_column: &'static str,
) -> Result<i64> {
    let version = &probed.version;
    let now = Utc::now();
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
            // upsert paths; Shows are handled by the TV folder scanner.
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

pub(crate) async fn update_media_version(
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

pub(crate) async fn replace_streams(
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

pub(crate) async fn replace_chapters(
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

pub(crate) fn path_hash(path: &str) -> String {
    let digest = Sha256::digest(path.as_bytes());
    let mut hash = String::with_capacity(digest.len() * 2);

    for byte in digest {
        use std::fmt::Write as _;

        write!(&mut hash, "{byte:02x}").expect("writing to String should not fail");
    }

    hash
}

pub(crate) fn encode_datetime(value: DateTime<Utc>) -> String {
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
pub(crate) struct LocalLibraryPath {
    pub(crate) id: i64,
    pub(crate) path: String,
    pub(crate) media_kind: LibraryMediaKind,
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
