#![forbid(unsafe_code)]

use std::collections::{BTreeMap, VecDeque};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::time::Duration as StdDuration;

use chrono::{DateTime, Duration, Utc};
use ersatzrs_core::repositories::RepositoryError;
use ersatzrs_domain::{
    classify_alternate_version, classify_extra_video, normalize_group_key,
    normalize_sub_kind_token, AlternateVersionClassification, ContentKind, ExtraParentHint,
    ExtraVideoClassification, LibraryMediaKind, MediaChapter, MediaStream, MediaStreamKind,
    ScanProgressSink,
};
use ersatzrs_infra::db::{Db, DbPool, DbRow};
use ersatzrs_infra::ffprobe::{FfprobeError, FfprobeExtractor, FfprobeMediaVersion};
use ersatzrs_infra::metadata::{
    artwork_kind, replace_artwork, replace_guids, replace_people, replace_strings,
    update_local_file_subtitles, ArtworkRef, PersonRef, SubtitleMetadataOwner,
};
use ersatzrs_infra::remote_stream::{
    parse_remote_stream_definition, resolve_remote_stream, RemoteStreamError,
};
use sha2::{Digest, Sha256};
use sqlx::{Row, Transaction};
use thiserror::Error;
use tracing::warn;

mod folder_etag;
mod library_folder;
pub mod local_television;
pub mod nfo;

const METADATA_KIND_FALLBACK: i32 = 0;
const METADATA_KIND_SIDECAR: i32 = 1;
const SQLITE_BUSY_RETRY_DELAYS_MS: &[u64] = &[250, 500, 1_000, 2_000, 4_000];

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
    #[error(transparent)]
    RemoteStream(#[from] RemoteStreamError),
    #[error("local television reconciliation failed: {0}")]
    Reconciliation(String),
    #[error("failed to read remote stream definition {path}: {source}")]
    RemoteStreamRead {
        path: String,
        source: std::io::Error,
    },
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
    pub failed_files: usize,
    pub issues: Vec<String>,
}

/// One pre-enumerated unit of work for the local scanner. Materializing the
/// full work list up front lets the with-progress variant report a true
/// fraction.
enum LocalScanWork {
    MoviePrimary {
        path: PathBuf,
    },
    ExtraVideo {
        path: PathBuf,
        extra: ExtraVideoClassification,
    },
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

#[derive(Debug, Clone)]
struct AlternateVersionRow {
    item_id: i64,
    disambiguator: String,
    path: Option<String>,
    classification: AlternateVersionClassification,
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
    retry_sqlite_busy(|| scan_local_library_with_progress_once(pool, config, progress)).await
}

#[allow(
    clippy::too_many_lines,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation
)]
async fn scan_local_library_with_progress_once(
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
            LibraryMediaKind::Movies => {
                let items = collect_movie_files(&library_path.path);
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
                    let is_remote_stream = matches!(&work, LocalScanWork::RemoteStream { .. });
                    match work {
                        LocalScanWork::ExtraVideo { path, extra } => {
                            let parent_media_item_id =
                                resolve_movie_extra_parent(pool, &library_path, &extra).await?;
                            let probed = extractor.extract(&path).await?;
                            retry_sqlite_busy(|| {
                                let probed = probed.clone();
                                async {
                                    upsert_local_extra_video(
                                        pool,
                                        &library_path,
                                        &path,
                                        probed,
                                        &extra.sub_kind,
                                        parent_media_item_id,
                                    )
                                    .await
                                }
                            })
                            .await?;
                        }
                        LocalScanWork::MoviePrimary { path } | LocalScanWork::Generic { path } => {
                            let probed = extractor.extract(&path).await?;
                            retry_sqlite_busy(|| {
                                let probed = probed.clone();
                                async {
                                    upsert_local_file(pool, &library_path, &path, probed).await
                                }
                            })
                            .await?;
                        }
                        LocalScanWork::MusicVideo {
                            artist_folder,
                            file_path,
                        } => {
                            let artist_id = retry_sqlite_busy(|| async {
                                upsert_artist_for_folder(pool, &library_path, &artist_folder).await
                            })
                            .await?;
                            let artist_dir = Path::new(&library_path.path).join(&artist_folder);
                            let probed = extractor.extract(&file_path).await?;
                            retry_sqlite_busy(|| {
                                let probed = probed.clone();
                                async {
                                    upsert_local_music_video(
                                        pool,
                                        &library_path,
                                        artist_id,
                                        &artist_dir,
                                        &file_path,
                                        probed,
                                    )
                                    .await
                                }
                            })
                            .await?;
                        }
                        LocalScanWork::RemoteStream { path } => {
                            let result = match load_remote_stream_definition(&path) {
                                Ok((definition, resolved)) => {
                                    let ffmpeg_path = paired_ffmpeg_path(&config.ffprobe_path);
                                    let probed = match extractor
                                        .extract_remote(&resolved, &ffmpeg_path)
                                        .await
                                    {
                                        Ok(probed) => Some(probed),
                                        Err(error) => {
                                            warn!(
                                                error = %error,
                                                definition = %path
                                                    .strip_prefix(&library_path.path)
                                                    .unwrap_or(&path)
                                                    .display(),
                                                "remote stream probe failed; persisting definition with configured duration"
                                            );
                                            None
                                        }
                                    };
                                    retry_sqlite_busy(|| {
                                        let definition = definition.clone();
                                        let resolved = resolved.clone();
                                        let probed = probed.clone();
                                        let library_path = &library_path;
                                        let path = &path;
                                        async move {
                                            upsert_local_remote_stream(
                                                pool,
                                                library_path,
                                                path,
                                                &definition,
                                                &resolved,
                                                probed.as_ref(),
                                            )
                                            .await
                                        }
                                    })
                                    .await
                                }
                                Err(error) => Err(error),
                            };
                            if let Err(error) = result {
                                retry_sqlite_busy(|| async {
                                    flag_invalid_remote_stream(pool, &path).await
                                })
                                .await?;
                                report.failed_files += 1;
                                let display_path = path
                                    .strip_prefix(&library_path.path)
                                    .unwrap_or(&path)
                                    .display()
                                    .to_string();
                                report.issues.push(format!("{display_path}: {error}"));
                                warn!(
                                    error = %error,
                                    definition = %display_path,
                                    "remote stream definition scan failed; continuing"
                                );
                            } else {
                                report.scanned_files += 1;
                            }
                        }
                    }
                    if !is_remote_stream {
                        report.scanned_files += 1;
                    }
                    processed += 1;
                    if let Some(sink) = progress {
                        if total_items > 0 {
                            sink.report(processed as f32 / total_items as f32);
                        }
                    }
                }
                retry_sqlite_busy(|| async {
                    link_alternate_versions_for_library_path(pool, &library_path).await
                })
                .await?;
            }
            LocalScanBucketKind::Shows { expected_episodes } => {
                let mut local_processed = 0usize;
                let scanned = local_television::sync_local_show_library(
                    pool,
                    &library_path,
                    &extractor,
                    |count| {
                        local_processed += count;
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
        // Trailing pass: flag items whose backing file disappeared since the
        // last scan, mirroring ErsatzTV's per-FolderScanner FlagFileNotFound.
        retry_sqlite_busy(|| async { flag_missing_local_files(pool, library_path.id).await })
            .await?;
        let now = Utc::now();
        retry_sqlite_busy(|| async { update_library_path_scan(pool, library_path.id, now).await })
            .await?;
    }

    retry_sqlite_busy(|| async { update_library_scan(pool, config.library_id, Utc::now()).await })
        .await?;
    if let Some(sink) = progress {
        sink.report(1.0);
    }
    Ok(report)
}

async fn retry_sqlite_busy<T, Fut, F>(mut operation: F) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T>>,
{
    let mut attempt = 0usize;
    loop {
        match operation().await {
            Ok(value) => return Ok(value),
            Err(error)
                if is_sqlite_busy_error(&error) && attempt < SQLITE_BUSY_RETRY_DELAYS_MS.len() =>
            {
                let delay = SQLITE_BUSY_RETRY_DELAYS_MS[attempt];
                attempt += 1;
                warn!(
                    attempt,
                    delay_ms = delay,
                    error = %error,
                    "sqlite database was busy during scanner write; retrying"
                );
                tokio::time::sleep(StdDuration::from_millis(delay)).await;
            }
            Err(error) => return Err(error),
        }
    }
}

fn is_sqlite_busy_error(error: &ScannerError) -> bool {
    let ScannerError::Sqlx(sqlx::Error::Database(database_error)) = error else {
        return false;
    };
    let code_matches = database_error
        .code()
        .as_deref()
        .is_some_and(|code| matches!(code, "5" | "6" | "SQLITE_BUSY" | "SQLITE_LOCKED"));
    let message = database_error.message().to_ascii_lowercase();
    code_matches || message.contains("database is locked") || message.contains("database is busy")
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
            let is_dot_underscore = path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("._"));
            if !is_dot_underscore && matches!(extension.as_deref(), Some("yml" | "yaml")) {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

fn collect_movie_files(root: &str) -> Vec<LocalScanWork> {
    let mut primary_files = Vec::new();
    let mut extra_files = Vec::new();
    let mut queue = VecDeque::from([PathBuf::from(root)]);

    while let Some(path) = queue.pop_front() {
        let Ok(entries) = std::fs::read_dir(&path) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                queue.push_back(path);
            } else if is_scannable_for_kind(&path, LibraryMediaKind::Movies) {
                if let Some(extra) = classify_extra_video(Path::new(root), &path) {
                    extra_files.push(LocalScanWork::ExtraVideo { path, extra });
                } else {
                    primary_files.push(LocalScanWork::MoviePrimary { path });
                }
            }
        }
    }

    primary_files.sort_by(|left, right| work_path(left).cmp(work_path(right)));
    extra_files.sort_by(|left, right| work_path(left).cmp(work_path(right)));
    primary_files.extend(extra_files);
    primary_files
}

fn work_path(work: &LocalScanWork) -> &Path {
    match work {
        LocalScanWork::MoviePrimary { path }
        | LocalScanWork::ExtraVideo { path, .. }
        | LocalScanWork::RemoteStream { path }
        | LocalScanWork::Generic { path } => path,
        LocalScanWork::MusicVideo { file_path, .. } => file_path,
    }
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

async fn link_alternate_versions_for_library_path(
    pool: &DbPool,
    library_path: &LocalLibraryPath,
) -> Result<()> {
    let rows = load_alternate_version_rows(pool, library_path).await?;
    if rows.is_empty() {
        return Ok(());
    }

    let mut groups: BTreeMap<(String, String), Vec<AlternateVersionRow>> = BTreeMap::new();
    for row in rows {
        let key = (
            normalize_group_key(&row.disambiguator),
            normalize_group_key(&row.classification.base_title),
        );
        if key.1.is_empty() {
            reset_primary_variant_fields(pool, row.item_id).await?;
            continue;
        }
        groups.entry(key).or_default().push(row);
    }

    for (_, mut group) in groups {
        group.sort_by(|left, right| {
            left.path
                .cmp(&right.path)
                .then_with(|| left.item_id.cmp(&right.item_id))
        });
        apply_alternate_version_group(pool, &group).await?;
    }

    Ok(())
}

async fn apply_alternate_version_group(pool: &DbPool, group: &[AlternateVersionRow]) -> Result<()> {
    let alternate_count = group
        .iter()
        .filter(|row| row.classification.sub_kind.is_some())
        .count();
    if alternate_count == 0 {
        for row in group {
            reset_primary_variant_fields(pool, row.item_id).await?;
        }
        return Ok(());
    }

    if group.len() == 1 {
        let row = &group[0];
        if let Some(sub_kind) = &row.classification.sub_kind {
            set_alternate_variant_fields(pool, row.item_id, sub_kind, None).await?;
        } else {
            reset_primary_variant_fields(pool, row.item_id).await?;
        }
        return Ok(());
    }

    let canonical_id = group
        .iter()
        .find(|row| row.classification.sub_kind.is_none() && row.classification.canonical_hint)
        .or_else(|| {
            group
                .iter()
                .find(|row| row.classification.sub_kind.is_none())
        })
        .unwrap_or(&group[0])
        .item_id;

    for row in group {
        if row.item_id == canonical_id {
            reset_primary_variant_fields(pool, row.item_id).await?;
        } else if let Some(sub_kind) = &row.classification.sub_kind {
            set_alternate_variant_fields(pool, row.item_id, sub_kind, Some(canonical_id)).await?;
        } else {
            reset_primary_variant_fields(pool, row.item_id).await?;
        }
    }

    Ok(())
}

async fn reset_primary_variant_fields(pool: &DbPool, item_id: i64) -> Result<()> {
    sqlx::query(
        r#"UPDATE "MediaItem"
           SET "ContentKind" = ?, "SubKind" = NULL, "ParentMediaItemId" = NULL
           WHERE "Id" = ?"#,
    )
    .bind(ContentKind::Primary.db_value())
    .bind(item_id)
    .execute(pool)
    .await?;
    Ok(())
}

async fn set_alternate_variant_fields(
    pool: &DbPool,
    item_id: i64,
    sub_kind: &str,
    parent_media_item_id: Option<i64>,
) -> Result<()> {
    let normalized_sub_kind = normalize_sub_kind_token(sub_kind);
    sqlx::query(
        r#"UPDATE "MediaItem"
           SET "ContentKind" = ?, "SubKind" = ?, "ParentMediaItemId" = ?
           WHERE "Id" = ?"#,
    )
    .bind(ContentKind::AlternateVersion.db_value())
    .bind(&normalized_sub_kind)
    .bind(parent_media_item_id)
    .bind(item_id)
    .execute(pool)
    .await?;
    Ok(())
}

async fn load_alternate_version_rows(
    pool: &DbPool,
    library_path: &LocalLibraryPath,
) -> Result<Vec<AlternateVersionRow>> {
    match library_path.media_kind {
        LibraryMediaKind::Movies => load_movie_alternate_version_rows(pool, library_path).await,
        LibraryMediaKind::MusicVideos => {
            load_music_video_alternate_version_rows(pool, library_path).await
        }
        LibraryMediaKind::Songs => load_song_alternate_version_rows(pool, library_path).await,
        _ => Ok(Vec::new()),
    }
}

async fn load_movie_alternate_version_rows(
    pool: &DbPool,
    library_path: &LocalLibraryPath,
) -> Result<Vec<AlternateVersionRow>> {
    let rows = sqlx::query(
        r#"SELECT "m"."Id" AS "Id",
                  COALESCE("mm"."Title", "mm"."SortTitle", "mm"."OriginalTitle", '') AS "Title",
                  "mf"."Path" AS "Path"
           FROM "Movie" AS "m"
           JOIN "MediaItem" AS "mi" ON "mi"."Id" = "m"."Id"
           JOIN "MovieMetadata" AS "mm" ON "mm"."MovieId" = "m"."Id"
           LEFT JOIN "MediaVersion" AS "mv" ON "mv"."MovieId" = "m"."Id"
           LEFT JOIN "MediaFile" AS "mf" ON "mf"."MediaVersionId" = "mv"."Id"
           WHERE "mi"."LibraryPathId" = ?
             AND COALESCE("mi"."ContentKind", 0) <> ?
           ORDER BY "mf"."Path", "m"."Id""#,
    )
    .bind(library_path.id)
    .bind(ContentKind::Extra.db_value())
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| alternate_version_row_from_db_row(&row, library_path, ""))
        .collect())
}

async fn load_music_video_alternate_version_rows(
    pool: &DbPool,
    library_path: &LocalLibraryPath,
) -> Result<Vec<AlternateVersionRow>> {
    let rows = sqlx::query(
        r#"SELECT "mv"."Id" AS "Id",
                  COALESCE("mvm"."Title", "mvm"."SortTitle", "mvm"."OriginalTitle", '') AS "Title",
                  COALESCE("am"."Title", '') AS "Artist",
                  "mf"."Path" AS "Path"
           FROM "MusicVideo" AS "mv"
           JOIN "MediaItem" AS "mi" ON "mi"."Id" = "mv"."Id"
           JOIN "MusicVideoMetadata" AS "mvm" ON "mvm"."MusicVideoId" = "mv"."Id"
           LEFT JOIN "ArtistMetadata" AS "am" ON "am"."ArtistId" = "mv"."ArtistId"
           LEFT JOIN "MediaVersion" AS "ver" ON "ver"."MusicVideoId" = "mv"."Id"
           LEFT JOIN "MediaFile" AS "mf" ON "mf"."MediaVersionId" = "ver"."Id"
           WHERE "mi"."LibraryPathId" = ?
           ORDER BY "mf"."Path", "mv"."Id""#,
    )
    .bind(library_path.id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| {
            let artist = row.get::<Option<String>, _>("Artist").unwrap_or_default();
            alternate_version_row_from_db_row(&row, library_path, &artist)
        })
        .collect())
}

async fn load_song_alternate_version_rows(
    pool: &DbPool,
    library_path: &LocalLibraryPath,
) -> Result<Vec<AlternateVersionRow>> {
    let rows = sqlx::query(
        r#"SELECT "s"."Id" AS "Id",
                  COALESCE("sm"."Title", "sm"."SortTitle", "sm"."OriginalTitle", '') AS "Title",
                  COALESCE("sm"."Artists", "sm"."AlbumArtists", '') AS "Artist",
                  "mf"."Path" AS "Path"
           FROM "Song" AS "s"
           JOIN "MediaItem" AS "mi" ON "mi"."Id" = "s"."Id"
           JOIN "SongMetadata" AS "sm" ON "sm"."SongId" = "s"."Id"
           LEFT JOIN "MediaVersion" AS "mv" ON "mv"."SongId" = "s"."Id"
           LEFT JOIN "MediaFile" AS "mf" ON "mf"."MediaVersionId" = "mv"."Id"
           WHERE "mi"."LibraryPathId" = ?
           ORDER BY "mf"."Path", "s"."Id""#,
    )
    .bind(library_path.id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| {
            let artist = row.get::<Option<String>, _>("Artist").unwrap_or_default();
            alternate_version_row_from_db_row(&row, library_path, &artist)
        })
        .collect())
}

fn alternate_version_row_from_db_row(
    row: &DbRow,
    library_path: &LocalLibraryPath,
    disambiguator: &str,
) -> AlternateVersionRow {
    let item_id = row.get::<i64, _>("Id");
    let title = row.get::<Option<String>, _>("Title").unwrap_or_default();
    let path = row.get::<Option<String>, _>("Path");
    let path_ref = path.as_deref().map(Path::new);
    let classification =
        classify_alternate_version(Path::new(&library_path.path), &title, path_ref);
    AlternateVersionRow {
        item_id,
        disambiguator: disambiguator.to_owned(),
        path,
        classification,
    }
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
    const EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "gif", "tbn", "webp"];
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

    sqlx::query(
        r#"UPDATE "MediaItem"
           SET "LibraryPathId" = ?, "State" = 0, "ContentKind" = ?,
               "SubKind" = NULL, "ParentMediaItemId" = NULL
           WHERE "Id" = ?"#,
    )
    .bind(library_path.id)
    .bind(ContentKind::Primary.db_value())
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
    if let Some(thumb) = locate_music_video_artwork(
        path,
        artist_folder,
        &["thumb"],
        &["-thumb"],
        &[".jpg", ".jpeg", ".png", ".gif", ".tbn", ".webp"],
    ) {
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
    update_local_file_subtitles(
        &mut tx,
        SubtitleMetadataOwner::MusicVideo(metadata_id),
        &probed.streams,
        path,
        true,
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
/// 2. Bare `{video_basename}{exact_ext}` (e.g. bare `Song.jpg`, the primary
///    Thumbnail convention per `MusicVideoFolderScanner.LocateThumbnail`).
/// 3. Generic `{folder_name}.{ext}` next to the video (e.g. `poster.jpg`).
/// 4. Same generic name in the parent artist folder as a final fallback.
fn locate_music_video_artwork(
    video_path: &Path,
    artist_folder: &Path,
    folder_names: &[&str],
    per_video_suffixes: &[&str],
    exact_extensions: &[&str],
) -> Option<PathBuf> {
    const EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "gif", "tbn", "webp"];
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

#[allow(clippy::too_many_lines)]
async fn upsert_local_remote_stream(
    pool: &DbPool,
    library_path: &LocalLibraryPath,
    path: &Path,
    definition: &ersatzrs_infra::remote_stream::RemoteStreamDefinition,
    resolved: &ersatzrs_infra::remote_stream::ResolvedRemoteStream,
    probed: Option<&FfprobeMediaVersion>,
) -> Result<()> {
    let mut tx = pool.begin().await?;
    let path_text = path.display().to_string();
    let path_hash = path_hash(&path_text);

    let maybe_existing = sqlx::query(
        r#"SELECT "mf"."MediaVersionId",
                  "mv"."RemoteStreamId" AS "MediaItemId",
                  "rsm"."Id" AS "MetadataId"
           FROM "MediaFile" AS "mf"
           JOIN "MediaVersion" AS "mv" ON "mv"."Id" = "mf"."MediaVersionId"
           JOIN "RemoteStreamMetadata" AS "rsm"
             ON "rsm"."RemoteStreamId" = "mv"."RemoteStreamId"
           WHERE "mf"."PathHash" = ?"#,
    )
    .bind(&path_hash)
    .fetch_optional(&mut *tx)
    .await?;

    let fallback_title = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_default()
        .to_owned();
    let title = definition
        .title
        .as_deref()
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .unwrap_or(&fallback_title)
        .to_owned();
    let original_title = path
        .strip_prefix(&library_path.path)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/");
    let now = Utc::now();

    let (item_id, metadata_id, version_id, existed) = if let Some(row) = maybe_existing {
        let item_id: i64 = row.get::<Option<i64>, _>("MediaItemId").unwrap_or_default();
        let metadata_id: i64 = row.get("MetadataId");
        let version_id: i64 = row.get("MediaVersionId");
        sqlx::query(
            r#"UPDATE "RemoteStreamMetadata"
               SET "DateUpdated" = ?, "MetadataKind" = 1, "Title" = ?,
                   "OriginalTitle" = ?, "SortTitle" = ?, "Year" = ?,
                   "ContentRating" = ?, "Plot" = ?
               WHERE "Id" = ?"#,
        )
        .bind(encode_datetime(now))
        .bind(&title)
        .bind(&original_title)
        .bind(&title)
        .bind(definition.year)
        .bind(definition.content_rating.as_deref())
        .bind(definition.plot.as_deref())
        .bind(metadata_id)
        .execute(&mut *tx)
        .await?;
        (item_id, metadata_id, version_id, true)
    } else {
        let item_id = insert_media_item(&mut tx, library_path.id).await?;
        sqlx::query(r#"INSERT INTO "RemoteStream" ("Id") VALUES (?)"#)
            .bind(item_id)
            .execute(&mut *tx)
            .await?;
        let metadata_result = sqlx::query(
            r#"INSERT INTO "RemoteStreamMetadata" (
                "RemoteStreamId", "DateAdded", "DateUpdated", "MetadataKind",
                "Title", "OriginalTitle", "SortTitle", "Year", "ContentRating", "Plot"
            ) VALUES (?, ?, ?, 1, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(item_id)
        .bind(encode_datetime(now))
        .bind(encode_datetime(now))
        .bind(&title)
        .bind(&original_title)
        .bind(&title)
        .bind(definition.year)
        .bind(definition.content_rating.as_deref())
        .bind(definition.plot.as_deref())
        .execute(&mut *tx)
        .await?;
        let metadata_id = ersatzrs_infra::db::last_insert_id(metadata_result);
        let result = sqlx::query(
            r#"INSERT INTO "MediaVersion" (
                "DateAdded", "DateUpdated", "Duration", "Height", "Name",
                "RemoteStreamId", "VideoScanKind", "Width"
            ) VALUES (?, ?, ?, 0, 'Main', ?, 0, 0)"#,
        )
        .bind(encode_datetime(now))
        .bind(encode_datetime(now))
        .bind(
            resolved
                .configured_duration
                .as_deref()
                .unwrap_or("00:00:00"),
        )
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
        (item_id, metadata_id, version_id, false)
    };

    sqlx::query(
        r#"UPDATE "RemoteStream"
           SET "Duration" = ?, "FallbackQuery" = ?, "Script" = ?, "Url" = ?, "IsLive" = ?
           WHERE "Id" = ?"#,
    )
    .bind(definition.duration.as_deref())
    .bind(definition.fallback_query.as_deref())
    .bind(definition.script.as_deref())
    .bind(definition.url.as_deref())
    .bind(i32::from(resolved.is_live))
    .bind(item_id)
    .execute(&mut *tx)
    .await?;
    if let Some(probed) = probed {
        update_media_version(&mut tx, version_id, probed).await?;
        replace_streams(&mut tx, version_id, &probed.streams).await?;
        replace_chapters(&mut tx, version_id, &probed.chapters).await?;
        if probed.version.duration <= Duration::zero() {
            sqlx::query(r#"UPDATE "MediaVersion" SET "Duration" = ? WHERE "Id" = ?"#)
                .bind(
                    resolved
                        .configured_duration
                        .as_deref()
                        .unwrap_or("00:00:00"),
                )
                .bind(version_id)
                .execute(&mut *tx)
                .await?;
        }
    } else if resolved.configured_duration.is_some() || !existed {
        sqlx::query(
            r#"UPDATE "MediaVersion" SET "DateUpdated" = ?, "Duration" = ? WHERE "Id" = ?"#,
        )
        .bind(encode_datetime(now))
        .bind(
            resolved
                .configured_duration
                .as_deref()
                .unwrap_or("00:00:00"),
        )
        .bind(version_id)
        .execute(&mut *tx)
        .await?;
    }

    sqlx::query(
        r#"UPDATE "MediaItem"
           SET "LibraryPathId" = ?, "State" = 0, "ContentKind" = ?,
               "SubKind" = NULL, "ParentMediaItemId" = NULL
           WHERE "Id" = ?"#,
    )
    .bind(library_path.id)
    .bind(ContentKind::Primary.db_value())
    .bind(item_id)
    .execute(&mut *tx)
    .await?;

    let tags = remote_stream_folder_tags(Path::new(&library_path.path), path);
    replace_strings(&mut tx, "Tag", "RemoteStreamMetadataId", metadata_id, &tags).await?;
    let artwork = locate_remote_stream_artwork(path)
        .map(|path| vec![artwork_ref(&path, artwork_kind::THUMBNAIL)])
        .unwrap_or_default();
    replace_artwork(
        &mut tx,
        "RemoteStreamMetadataId",
        metadata_id,
        &[artwork_kind::THUMBNAIL],
        &artwork,
    )
    .await?;

    tx.commit().await?;
    Ok(())
}

async fn flag_invalid_remote_stream(pool: &DbPool, path: &Path) -> Result<()> {
    let path_hash = path_hash(&path.display().to_string());
    sqlx::query(
        r#"UPDATE "MediaItem"
           SET "State" = 1
           WHERE "Id" IN (
               SELECT "mv"."RemoteStreamId"
               FROM "MediaFile" AS "mf"
               JOIN "MediaVersion" AS "mv" ON "mv"."Id" = "mf"."MediaVersionId"
               WHERE "mf"."PathHash" = ?
                 AND "mv"."RemoteStreamId" IS NOT NULL
           )"#,
    )
    .bind(path_hash)
    .execute(pool)
    .await?;
    Ok(())
}

fn load_remote_stream_definition(
    path: &Path,
) -> Result<(
    ersatzrs_infra::remote_stream::RemoteStreamDefinition,
    ersatzrs_infra::remote_stream::ResolvedRemoteStream,
)> {
    let yaml = std::fs::read_to_string(path).map_err(|source| ScannerError::RemoteStreamRead {
        path: path.display().to_string(),
        source,
    })?;
    let definition = parse_remote_stream_definition(&yaml)?;
    let resolved = resolve_remote_stream(&definition, path)?;
    Ok((definition, resolved))
}

fn paired_ffmpeg_path(ffprobe_path: &str) -> String {
    let path = Path::new(ffprobe_path);
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("ffprobe");
    let ffmpeg_name = if filename.to_ascii_lowercase().ends_with(".exe") {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    };
    path.with_file_name(ffmpeg_name).display().to_string()
}

fn remote_stream_folder_tags(root: &Path, definition: &Path) -> Vec<String> {
    let mut tags = Vec::new();
    if let Some(root_name) = root.file_name().and_then(|name| name.to_str()) {
        tags.push(root_name.to_owned());
    }
    if let Some(parent) = definition.parent() {
        if let Ok(relative) = parent.strip_prefix(root) {
            tags.extend(relative.components().filter_map(|component| {
                let value = component.as_os_str().to_str()?.trim();
                (!value.is_empty()).then(|| value.to_owned())
            }));
        }
    }
    tags
}

fn locate_remote_stream_artwork(definition: &Path) -> Option<PathBuf> {
    const EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "webp"];
    let parent = definition.parent()?;
    let stem = definition.file_stem()?.to_str()?;
    EXTENSIONS
        .iter()
        .map(|extension| parent.join(format!("{stem}.{extension}")))
        .find(|candidate| candidate.is_file())
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

    sqlx::query(
        r#"UPDATE "MediaItem"
           SET "LibraryPathId" = ?, "State" = 0, "ContentKind" = ?,
               "SubKind" = NULL, "ParentMediaItemId" = NULL
           WHERE "Id" = ?"#,
    )
    .bind(library_path.id)
    .bind(ContentKind::Primary.db_value())
    .bind(item_id)
    .execute(&mut *tx)
    .await?;
    update_media_version(&mut tx, version_id, &probed).await?;
    replace_streams(&mut tx, version_id, &probed.streams).await?;
    replace_chapters(&mut tx, version_id, &probed.chapters).await?;
    if let Some(owner) =
        local_file_subtitle_owner(&mut tx, item_id, library_path.media_kind).await?
    {
        update_local_file_subtitles(&mut tx, owner, &probed.streams, path, true).await?;
    }
    if library_path.media_kind == LibraryMediaKind::Movies {
        enrich_movie_metadata(&mut tx, item_id, library_path, path).await?;
    }

    tx.commit().await?;
    Ok(())
}

#[allow(clippy::too_many_lines)]
pub(crate) async fn upsert_local_extra_video(
    pool: &DbPool,
    library_path: &LocalLibraryPath,
    path: &Path,
    probed: FfprobeMediaVersion,
    sub_kind: &str,
    parent_media_item_id: Option<i64>,
) -> Result<i64> {
    let mut tx = pool.begin().await?;
    let path_text = path.display().to_string();
    let path_hash = path_hash(&path_text);

    let maybe_existing = sqlx::query(
        r#"SELECT "mf"."MediaVersionId",
                  COALESCE(
                    "mv"."OtherVideoId", "mv"."MovieId", "mv"."EpisodeId",
                    "mv"."MusicVideoId", "mv"."SongId", "mv"."ImageId"
                  ) AS "MediaItemId",
                  "mv"."OtherVideoId" AS "OtherVideoId"
           FROM "MediaFile" AS "mf"
           JOIN "MediaVersion" AS "mv" ON "mv"."Id" = "mf"."MediaVersionId"
           WHERE "mf"."PathHash" = ?"#,
    )
    .bind(&path_hash)
    .fetch_optional(&mut *tx)
    .await?;

    let (item_id, version_id) = if let Some(row) = maybe_existing {
        let item_id = row.get::<Option<i64>, _>("MediaItemId").unwrap_or_default();
        let version_id = row.get::<i64, _>("MediaVersionId");
        if item_id != 0 && row.get::<Option<i64>, _>("OtherVideoId").is_none() {
            convert_existing_video_to_other_video(&mut tx, item_id, version_id).await?;
        }
        insert_fallback_metadata_for_kind(
            &mut tx,
            item_id,
            LibraryMediaKind::OtherVideos,
            library_path,
            path,
        )
        .await?;
        (item_id, version_id)
    } else {
        let item_id = insert_media_item(&mut tx, library_path.id).await?;
        insert_media_subtype(&mut tx, item_id, LibraryMediaKind::OtherVideos).await?;
        insert_fallback_metadata_for_kind(
            &mut tx,
            item_id,
            LibraryMediaKind::OtherVideos,
            library_path,
            path,
        )
        .await?;
        let version_id =
            insert_media_version(&mut tx, item_id, &probed, LibraryMediaKind::OtherVideos).await?;
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

    let normalized_sub_kind = normalize_sub_kind_token(sub_kind);
    sqlx::query(
        r#"UPDATE "MediaItem"
           SET "LibraryPathId" = ?, "State" = 0, "ContentKind" = ?,
               "SubKind" = ?, "ParentMediaItemId" = ?
           WHERE "Id" = ?"#,
    )
    .bind(library_path.id)
    .bind(ContentKind::Extra.db_value())
    .bind(&normalized_sub_kind)
    .bind(parent_media_item_id)
    .bind(item_id)
    .execute(&mut *tx)
    .await?;
    update_media_version(&mut tx, version_id, &probed).await?;
    replace_streams(&mut tx, version_id, &probed.streams).await?;
    replace_chapters(&mut tx, version_id, &probed.chapters).await?;
    if let Some(owner) =
        local_file_subtitle_owner(&mut tx, item_id, LibraryMediaKind::OtherVideos).await?
    {
        update_local_file_subtitles(&mut tx, owner, &probed.streams, path, true).await?;
    }

    tx.commit().await?;
    Ok(item_id)
}

async fn convert_existing_video_to_other_video(
    tx: &mut Transaction<'_, Db>,
    item_id: i64,
    version_id: i64,
) -> Result<()> {
    ensure_other_video_row(tx, item_id).await?;
    sqlx::query(
        r#"UPDATE "MediaVersion"
           SET "EpisodeId" = NULL, "ImageId" = NULL, "MovieId" = NULL,
               "MusicVideoId" = NULL, "SongId" = NULL, "OtherVideoId" = ?
           WHERE "Id" = ?"#,
    )
    .bind(item_id)
    .bind(version_id)
    .execute(&mut **tx)
    .await?;
    sqlx::query(r#"DELETE FROM "Movie" WHERE "Id" = ?"#)
        .bind(item_id)
        .execute(&mut **tx)
        .await?;
    sqlx::query(r#"DELETE FROM "Episode" WHERE "Id" = ?"#)
        .bind(item_id)
        .execute(&mut **tx)
        .await?;
    sqlx::query(r#"DELETE FROM "MusicVideo" WHERE "Id" = ?"#)
        .bind(item_id)
        .execute(&mut **tx)
        .await?;
    sqlx::query(r#"DELETE FROM "Song" WHERE "Id" = ?"#)
        .bind(item_id)
        .execute(&mut **tx)
        .await?;
    sqlx::query(r#"DELETE FROM "Image" WHERE "Id" = ?"#)
        .bind(item_id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

async fn ensure_other_video_row(tx: &mut Transaction<'_, Db>, item_id: i64) -> Result<()> {
    let exists: i64 =
        sqlx::query_scalar(r#"SELECT EXISTS(SELECT 1 FROM "OtherVideo" WHERE "Id" = ?)"#)
            .bind(item_id)
            .fetch_one(&mut **tx)
            .await?;
    if exists == 0 {
        sqlx::query(r#"INSERT INTO "OtherVideo" ("Id") VALUES (?)"#)
            .bind(item_id)
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}

async fn resolve_movie_extra_parent(
    pool: &DbPool,
    library_path: &LocalLibraryPath,
    extra: &ExtraVideoClassification,
) -> Result<Option<i64>> {
    match &extra.parent_hint {
        ExtraParentHint::Sibling {
            folder,
            primary_stem,
        } => find_movie_parent_by_folder(pool, library_path.id, folder, Some(primary_stem)).await,
        ExtraParentHint::Folder { owner_folder } => {
            find_movie_parent_by_folder(pool, library_path.id, owner_folder, None).await
        }
    }
}

async fn find_movie_parent_by_folder(
    pool: &DbPool,
    library_path_id: i64,
    owner_folder: &Path,
    primary_stem: Option<&str>,
) -> Result<Option<i64>> {
    let rows = sqlx::query(
        r#"SELECT "m"."Id" AS "MovieId", "mf"."Path" AS "Path"
           FROM "Movie" AS "m"
           JOIN "MediaItem" AS "mi" ON "mi"."Id" = "m"."Id"
           JOIN "MediaVersion" AS "mv" ON "mv"."MovieId" = "m"."Id"
           JOIN "MediaFile" AS "mf" ON "mf"."MediaVersionId" = "mv"."Id"
           WHERE "mi"."LibraryPathId" = ?
             AND COALESCE("mi"."ContentKind", 0) = 0"#,
    )
    .bind(library_path_id)
    .fetch_all(pool)
    .await?;

    let mut matches = Vec::new();
    for row in rows {
        let path_text: Option<String> = row.get("Path");
        let Some(path_text) = path_text else {
            continue;
        };
        let candidate_path = Path::new(&path_text);
        if candidate_path.parent() != Some(owner_folder) {
            continue;
        }
        if let Some(primary_stem) = primary_stem {
            let candidate_stem = candidate_path.file_stem().and_then(|stem| stem.to_str());
            if candidate_stem != Some(primary_stem) {
                continue;
            }
        }
        matches.push(row.get::<i64, _>("MovieId"));
    }

    if matches.len() == 1 {
        Ok(matches.into_iter().next())
    } else {
        Ok(None)
    }
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
    let result = sqlx::query(ersatzrs_infra::db::assert_sql_safe(&sql))
        .bind(encode_datetime(version.date_added))
        .bind(encode_datetime(now))
        .bind(&version.display_aspect_ratio)
        .bind(encode_duration(version.duration))
        .bind(version.height)
        .bind(&version.name)
        .bind(item_id)
        .bind(&version.r_frame_rate)
        .bind(&version.sample_aspect_ratio)
        .bind(version.video_scan_kind.db_value())
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
    sqlx::query(ersatzrs_infra::db::assert_sql_safe(&sql))
        .bind(item_id)
        .execute(&mut **tx)
        .await?;
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
/// the library root. This helper only establishes that baseline identity;
/// media-kind folder scanners enrich metadata, artwork, and relationships in
/// their supported scan paths without disturbing this idempotent insert.
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
    let exists: i64 = sqlx::query_scalar(ersatzrs_infra::db::assert_sql_safe(&sql))
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
    insert_fallback_metadata_for_kind(tx, item_id, library_path.media_kind, library_path, path)
        .await
}

async fn insert_fallback_metadata_for_kind(
    tx: &mut Transaction<'_, Db>,
    item_id: i64,
    media_kind: LibraryMediaKind,
    library_path: &LocalLibraryPath,
    path: &Path,
) -> Result<()> {
    if fallback_metadata_exists(tx, item_id, media_kind).await? {
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

    match media_kind {
        LibraryMediaKind::Movies => {
            sqlx::query(
                r#"INSERT INTO "MovieMetadata" (
                    "MovieId", "DateAdded", "DateUpdated", "MetadataKind",
                    "Title", "SortTitle", "OriginalTitle"
                ) VALUES (?, ?, ?, 0, ?, ?, ?)"#,
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

fn original_title_for_path(library_path: &LocalLibraryPath, path: &Path) -> Option<String> {
    path.strip_prefix(&library_path.path)
        .ok()
        .and_then(|path| path.to_str())
        .map(str::to_owned)
}

fn locate_movie_nfo(video_path: &Path) -> Option<PathBuf> {
    let file_nfo = video_path.with_extension("nfo");
    if file_nfo.is_file() {
        return Some(file_nfo);
    }

    let movie_nfo = video_path.parent()?.join("movie.nfo");
    movie_nfo.is_file().then_some(movie_nfo)
}

fn locate_movie_artwork(video_path: &Path, kind: i32) -> Option<PathBuf> {
    let folder = video_path.parent()?;
    let stem = video_path.file_stem().and_then(|stem| stem.to_str());
    let candidates = match (kind, stem) {
        (artwork_kind::POSTER, Some(stem)) => {
            vec![
                "poster".to_owned(),
                format!("{stem}-poster"),
                "folder".to_owned(),
            ]
        }
        (artwork_kind::POSTER, None) => vec!["poster".to_owned(), "folder".to_owned()],
        (artwork_kind::FAN_ART, Some(stem)) => {
            vec![
                "fanart".to_owned(),
                format!("{stem}-fanart"),
                "backdrop".to_owned(),
            ]
        }
        (artwork_kind::FAN_ART, None) => vec!["fanart".to_owned(), "backdrop".to_owned()],
        _ => return None,
    };
    local_television::locate_artwork(folder, &candidates)
}

#[allow(clippy::too_many_lines)]
async fn enrich_movie_metadata(
    tx: &mut Transaction<'_, Db>,
    movie_id: i64,
    library_path: &LocalLibraryPath,
    path: &Path,
) -> Result<i64> {
    let nfo_path = locate_movie_nfo(path);
    let movie_nfo = nfo_path
        .as_ref()
        .and_then(|path| match nfo::parse_movie_nfo(path) {
            Ok(parsed) => Some(parsed),
            Err(err) => {
                warn!(
                    path = %path.display(),
                    error = %err,
                    "failed to parse movie.nfo; falling back"
                );
                None
            }
        });

    let fallback_title = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_default()
        .to_owned();
    let title = movie_nfo
        .as_ref()
        .and_then(|nfo| nfo.title.clone())
        .unwrap_or(fallback_title);
    let sort_title = movie_nfo
        .as_ref()
        .and_then(|nfo| nfo.sort_title.clone())
        .unwrap_or_else(|| nfo::sort_title(&title));
    let original_title = original_title_for_path(library_path, path);
    let metadata_kind = if movie_nfo.is_some() {
        METADATA_KIND_SIDECAR
    } else {
        METADATA_KIND_FALLBACK
    };
    let release_date = movie_nfo
        .as_ref()
        .and_then(|nfo| nfo.premiered.map(encode_datetime));
    let now = encode_datetime(Utc::now());

    let existing_id: Option<i64> =
        sqlx::query_scalar(r#"SELECT "Id" FROM "MovieMetadata" WHERE "MovieId" = ? LIMIT 1"#)
            .bind(movie_id)
            .fetch_optional(&mut **tx)
            .await?;

    let metadata_id = if let Some(id) = existing_id {
        sqlx::query(
            r#"UPDATE "MovieMetadata"
               SET "DateUpdated" = ?, "MetadataKind" = ?, "Title" = ?,
                   "SortTitle" = ?, "OriginalTitle" = ?, "Year" = ?,
                   "ReleaseDate" = ?, "ContentRating" = ?, "Plot" = ?,
                   "Outline" = ?, "Tagline" = NULL
               WHERE "Id" = ?"#,
        )
        .bind(&now)
        .bind(metadata_kind)
        .bind(&title)
        .bind(&sort_title)
        .bind(&original_title)
        .bind(movie_nfo.as_ref().and_then(|nfo| nfo.year))
        .bind(&release_date)
        .bind(
            movie_nfo
                .as_ref()
                .and_then(|nfo| nfo.content_rating.clone()),
        )
        .bind(movie_nfo.as_ref().and_then(|nfo| nfo.plot.clone()))
        .bind(movie_nfo.as_ref().and_then(|nfo| nfo.outline.clone()))
        .bind(id)
        .execute(&mut **tx)
        .await?;
        id
    } else {
        let result = sqlx::query(
            r#"INSERT INTO "MovieMetadata" (
                "MovieId", "DateAdded", "DateUpdated", "MetadataKind",
                "Title", "SortTitle", "OriginalTitle", "Year", "ReleaseDate",
                "ContentRating", "Plot", "Outline", "Tagline"
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, NULL)"#,
        )
        .bind(movie_id)
        .bind(&now)
        .bind(&now)
        .bind(metadata_kind)
        .bind(&title)
        .bind(&sort_title)
        .bind(&original_title)
        .bind(movie_nfo.as_ref().and_then(|nfo| nfo.year))
        .bind(&release_date)
        .bind(
            movie_nfo
                .as_ref()
                .and_then(|nfo| nfo.content_rating.clone()),
        )
        .bind(movie_nfo.as_ref().and_then(|nfo| nfo.plot.clone()))
        .bind(movie_nfo.as_ref().and_then(|nfo| nfo.outline.clone()))
        .execute(&mut **tx)
        .await?;
        ersatzrs_infra::db::last_insert_id(result)
    };

    if let Some(parsed) = movie_nfo.as_ref() {
        replace_strings(tx, "Genre", "MovieMetadataId", metadata_id, &parsed.genres).await?;
        replace_strings(tx, "Tag", "MovieMetadataId", metadata_id, &parsed.tags).await?;
        replace_strings(
            tx,
            "Studio",
            "MovieMetadataId",
            metadata_id,
            &parsed.studios,
        )
        .await?;
        replace_people(
            tx,
            "Actor",
            "MovieMetadataId",
            metadata_id,
            &local_television::actors_from_nfo(&parsed.actors),
        )
        .await?;
        replace_people(
            tx,
            "Director",
            "MovieMetadataId",
            metadata_id,
            &local_television::directors_from(&parsed.directors),
        )
        .await?;
        replace_people(
            tx,
            "Writer",
            "MovieMetadataId",
            metadata_id,
            &local_television::directors_from(&parsed.writers),
        )
        .await?;
        replace_guids(
            tx,
            "MovieMetadataId",
            metadata_id,
            &local_television::guids_from_nfo(&parsed.guids),
        )
        .await?;
    } else {
        replace_strings(tx, "Genre", "MovieMetadataId", metadata_id, &[]).await?;
        replace_strings(tx, "Tag", "MovieMetadataId", metadata_id, &[]).await?;
        replace_strings(tx, "Studio", "MovieMetadataId", metadata_id, &[]).await?;
        replace_people(tx, "Actor", "MovieMetadataId", metadata_id, &[]).await?;
        replace_people(tx, "Director", "MovieMetadataId", metadata_id, &[]).await?;
        replace_people(tx, "Writer", "MovieMetadataId", metadata_id, &[]).await?;
        replace_guids(tx, "MovieMetadataId", metadata_id, &[]).await?;
    }

    let mut artwork = Vec::new();
    if let Some(path) = locate_movie_artwork(path, artwork_kind::POSTER) {
        artwork.push(local_television::artwork_ref(&path, artwork_kind::POSTER));
    }
    if let Some(path) = locate_movie_artwork(path, artwork_kind::FAN_ART) {
        artwork.push(local_television::artwork_ref(&path, artwork_kind::FAN_ART));
    }
    replace_artwork(
        tx,
        "MovieMetadataId",
        metadata_id,
        &[artwork_kind::POSTER, artwork_kind::FAN_ART],
        &artwork,
    )
    .await?;

    Ok(metadata_id)
}

async fn local_file_subtitle_owner(
    tx: &mut Transaction<'_, Db>,
    item_id: i64,
    media_kind: LibraryMediaKind,
) -> Result<Option<SubtitleMetadataOwner>> {
    let (table, owner_column, owner) = match media_kind {
        LibraryMediaKind::Movies => (
            "MovieMetadata",
            "MovieId",
            SubtitleMetadataOwner::Movie as fn(i64) -> SubtitleMetadataOwner,
        ),
        LibraryMediaKind::OtherVideos => (
            "OtherVideoMetadata",
            "OtherVideoId",
            SubtitleMetadataOwner::OtherVideo as fn(i64) -> SubtitleMetadataOwner,
        ),
        _ => return Ok(None),
    };
    let sql = format!(r#"SELECT "Id" FROM "{table}" WHERE "{owner_column}" = ? LIMIT 1"#);
    let metadata_id: Option<i64> = sqlx::query_scalar(ersatzrs_infra::db::assert_sql_safe(&sql))
        .bind(item_id)
        .fetch_optional(&mut **tx)
        .await?;
    Ok(metadata_id.map(owner))
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
    .bind(version.video_scan_kind.db_value())
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

/// Flag library-path items whose backing file no longer exists on disk as
/// `MediaItemState::FileNotFound` (`State = 1`).
///
/// Port of the trailing pass every `ErsatzTV` `*FolderScanner` runs after the
/// add/update pass (e.g. `MovieFolderScanner.FindMoviePaths` →
/// `File.Exists` → `MediaItemRepository.FlagFileNotFound`). The add/update
/// pass already restores re-found items to `Normal` (`State = 0`), so this is
/// the only place the forward `Normal → FileNotFound` transition happens for
/// local libraries. Without it, deleted/moved source files keep playing,
/// never appear in `/media/trash`, and never trip the File Not Found health
/// check.
///
/// Scoped to a single `LibraryPathId`; because a `LibraryPath` belongs to a
/// `Library` with one media kind, every joined item shares that kind. The
/// `COALESCE` over the per-kind owner columns mirrors ErsatzTV's join in
/// `MediaItemRepository.FlagFileNotFound`.
async fn flag_missing_local_files(pool: &DbPool, library_path_id: i64) -> Result<usize> {
    let rows = sqlx::query(
        r#"SELECT "mi"."Id" AS "Id", "mf"."Path" AS "Path"
           FROM "MediaItem" AS "mi"
           JOIN "MediaVersion" AS "mv"
             ON "mi"."Id" = COALESCE("mv"."MovieId", "mv"."MusicVideoId",
                                     "mv"."OtherVideoId", "mv"."SongId",
                                     "mv"."ImageId", "mv"."RemoteStreamId",
                                     "mv"."EpisodeId")
           JOIN "MediaFile" AS "mf" ON "mv"."Id" = "mf"."MediaVersionId"
           WHERE "mi"."LibraryPathId" = ? AND "mi"."State" <> 1"#,
    )
    .bind(library_path_id)
    .fetch_all(pool)
    .await?;

    let mut missing_ids: Vec<i64> = Vec::new();
    for row in &rows {
        let path: String = row.get("Path");
        if !Path::new(&path).exists() {
            tracing::info!(%path, "flagging missing file");
            missing_ids.push(row.get("Id"));
        }
    }

    if missing_ids.is_empty() {
        return Ok(0);
    }

    let placeholders = vec!["?"; missing_ids.len()].join(", ");
    let sql = format!(r#"UPDATE "MediaItem" SET "State" = 1 WHERE "Id" IN ({placeholders})"#);
    let mut query = sqlx::query(ersatzrs_infra::db::assert_sql_safe(&sql));
    for id in &missing_ids {
        query = query.bind(id);
    }
    query.execute(pool).await?;

    Ok(missing_ids.len())
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
    // Matches ErsatzTV.Core/Domain/MediaItem/MediaStreamKind.cs: the persisted
    // enum is 1-based (Video=1, Audio=2, Subtitle=3, Attachment=4,
    // ExternalSubtitle=5). Readers (scheduling.rs::media_stream_kind_from_row,
    // repositories/common.rs db_enum, the show-languages query) all expect
    // these values.
    match kind {
        MediaStreamKind::Video => 1,
        MediaStreamKind::Audio => 2,
        MediaStreamKind::Subtitle => 3,
        MediaStreamKind::Attachment => 4,
        MediaStreamKind::ExternalSubtitle => 5,
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
    use super::{is_scannable_for_kind, locate_music_video_artwork, path_hash, retry_sqlite_busy};
    use ersatzrs_domain::{
        classify_alternate_version, classify_extra_directory_name, classify_extra_video,
        ExtraParentHint, LibraryMediaKind,
    };
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

    #[test]
    fn music_video_thumbnail_resolves_bare_same_basename_image() {
        use tempfile::TempDir;

        let temp = TempDir::new().expect("temp dir");
        let artist_folder = temp.path().join("Talking Heads");
        std::fs::create_dir_all(&artist_folder).expect("artist folder");
        let video_path = artist_folder.join("Once in a Lifetime.mkv");
        std::fs::write(&video_path, b"\x00").expect("video file");

        // No `-thumb` suffix and no generic `thumb.jpg` folder file — only
        // the bare same-basename image ErsatzTV's `LocateThumbnail` treats
        // as the primary Thumbnail convention.
        std::fs::write(artist_folder.join("Once in a Lifetime.jpg"), b"\x00")
            .expect("bare thumbnail image");

        let resolved = locate_music_video_artwork(
            &video_path,
            &artist_folder,
            &["thumb"],
            &["-thumb"],
            &[".jpg", ".jpeg", ".png", ".gif", ".tbn", ".webp"],
        )
        .expect("bare-basename thumbnail should resolve");
        assert_eq!(resolved, artist_folder.join("Once in a Lifetime.jpg"));
    }

    #[test]
    fn classifies_extra_suffixes_and_directories() {
        let suffix = classify_extra_video(
            Path::new("/library"),
            Path::new("/library/The Movie/The Movie-trailer.mkv"),
        )
        .expect("trailer suffix");
        assert_eq!(suffix.sub_kind, "trailer");
        assert_eq!(
            suffix.parent_hint,
            ExtraParentHint::Sibling {
                folder: Path::new("/library/The Movie").to_path_buf(),
                primary_stem: "The Movie".to_owned()
            }
        );

        let directory = classify_extra_video(
            Path::new("/library"),
            Path::new("/library/The Movie/Featurettes/Making Of.mkv"),
        )
        .expect("featurette directory");
        assert_eq!(directory.sub_kind, "featurette");
        assert_eq!(
            directory.parent_hint,
            ExtraParentHint::Folder {
                owner_folder: Path::new("/library/The Movie").to_path_buf()
            }
        );

        assert_eq!(
            classify_extra_directory_name("Deleted Scenes").as_deref(),
            Some("deleted_scene")
        );
        assert!(
            classify_extra_video(Path::new("/library"), Path::new("/library/Movie.mkv")).is_none()
        );
    }

    #[test]
    fn classifies_unplugged_alternate_version_tokens_without_rewriting_unmarked_titles() {
        let live = classify_alternate_version(Path::new("/library"), "Song (Live)", None);
        assert_eq!(live.base_title, "song");
        assert_eq!(live.sub_kind.as_deref(), Some("live"));
        assert!(!live.canonical_hint);

        let folder = classify_alternate_version(
            Path::new("/library"),
            "Song",
            Some(Path::new("/library/Artist/Unplugged Sessions/Song.mkv")),
        );
        assert_eq!(folder.base_title, "Song");
        assert_eq!(folder.sub_kind.as_deref(), Some("unplugged"));
        assert!(!folder.canonical_hint);

        let official =
            classify_alternate_version(Path::new("/library"), "Song (Official Video)", None);
        assert_eq!(official.base_title, "song");
        assert!(official.sub_kind.is_none());
        assert!(official.canonical_hint);

        let unmarked = classify_alternate_version(Path::new("/library"), "Live Wire", None);
        assert_eq!(unmarked.base_title, "Live Wire");
        assert!(unmarked.sub_kind.is_none());
        assert!(unmarked.canonical_hint);
    }

    #[tokio::test]
    async fn sqlite_busy_retry_preserves_non_busy_errors() {
        let attempts = std::cell::Cell::new(0usize);

        let error = retry_sqlite_busy(|| {
            attempts.set(attempts.get() + 1);
            async { Err::<(), super::ScannerError>(super::ScannerError::MissingLibraryPaths(42)) }
        })
        .await
        .expect_err("non-busy errors should not be retried away");

        assert_eq!(attempts.get(), 1);
        assert!(matches!(
            error,
            super::ScannerError::MissingLibraryPaths(42)
        ));
    }

    #[cfg(not(feature = "mysql"))]
    #[tokio::test]
    async fn movie_extra_imports_as_linked_other_video_and_stays_hidden_by_default() {
        use super::{
            resolve_movie_extra_parent, upsert_local_extra_video, upsert_local_file,
            LocalLibraryPath,
        };
        use ersatzrs_infra::repositories::SqliteSchedulingContentRepository;
        use sqlx::sqlite::SqlitePoolOptions;
        use sqlx::{Executor, Row};
        use tempfile::TempDir;

        let temp = TempDir::new().expect("temp dir");
        let library_root = temp.path().join("library");
        let movie_folder = library_root.join("The Movie");
        std::fs::create_dir_all(&movie_folder).expect("movie folder");
        let movie_path = movie_folder.join("The Movie.mkv");
        let extra_path = movie_folder.join("Featurettes").join("Making Of.mkv");
        std::fs::create_dir_all(extra_path.parent().expect("extra parent")).expect("extra folder");

        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("connect sqlite");
        pool.execute("PRAGMA foreign_keys = ON")
            .await
            .expect("foreign keys");
        sqlx::migrate!("../../migrations")
            .run(&pool)
            .await
            .expect("migrate");
        seed_movie_library(&pool, &library_root).await;
        let library_path = LocalLibraryPath {
            id: 1,
            path: library_root.display().to_string(),
            media_kind: LibraryMediaKind::Movies,
        };

        upsert_local_file(&pool, &library_path, &movie_path, probed_version())
            .await
            .expect("primary movie");
        upsert_local_file(&pool, &library_path, &extra_path, probed_version())
            .await
            .expect("pre-existing primary-shaped extra");
        let extra = classify_extra_video(&library_root, &extra_path).expect("extra classified");
        let parent_id = resolve_movie_extra_parent(&pool, &library_path, &extra)
            .await
            .expect("resolve parent");
        let extra_id = upsert_local_extra_video(
            &pool,
            &library_path,
            &extra_path,
            probed_version(),
            &extra.sub_kind,
            parent_id,
        )
        .await
        .expect("extra import");

        let row = sqlx::query(
            r#"SELECT "ContentKind", "SubKind", "ParentMediaItemId"
               FROM "MediaItem" WHERE "Id" = ?"#,
        )
        .bind(extra_id)
        .fetch_one(&pool)
        .await
        .expect("extra media item");
        assert_eq!(
            row.get::<i32, _>("ContentKind"),
            ersatzrs_domain::ContentKind::Extra.db_value()
        );
        assert_eq!(
            row.get::<Option<String>, _>("SubKind").as_deref(),
            Some("featurette")
        );
        assert_eq!(row.get::<Option<i64>, _>("ParentMediaItemId"), parent_id);
        assert_eq!(count_table(&pool, "Movie").await, 1);
        assert_eq!(count_table(&pool, "OtherVideo").await, 1);

        let repository = SqliteSchedulingContentRepository::new(pool.clone());
        let hidden = search_ids(&repository, "Making").await;
        assert!(
            !hidden.contains(&extra_id),
            "implicit content_kind:primary should hide imported extras"
        );
        let visible = search_ids(&repository, "content_kind:extra AND sub_kind:featurette").await;
        assert_eq!(visible, vec![extra_id]);
    }

    #[cfg(not(feature = "mysql"))]
    #[tokio::test]
    async fn music_video_unplugged_versions_link_to_official_primary_and_stay_hidden_by_default() {
        use super::{
            link_alternate_versions_for_library_path, upsert_artist_for_folder,
            upsert_local_music_video,
        };
        use ersatzrs_infra::repositories::SqliteSchedulingContentRepository;
        use sqlx::Row;
        use tempfile::TempDir;

        let temp = TempDir::new().expect("temp dir");
        let artist_dir = temp.path().join("Talking Heads");
        let unplugged_dir = artist_dir.join("Unplugged Sessions");
        std::fs::create_dir_all(&unplugged_dir).expect("artist dirs");
        let official_path = artist_dir.join("Once in a Lifetime (Official Video).mkv");
        let unplugged_path = unplugged_dir.join("Once in a Lifetime.mkv");
        std::fs::write(&official_path, b"not probed").expect("official video");
        std::fs::write(&unplugged_path, b"not probed").expect("unplugged video");

        let pool = test_db().await;
        let library_path = seed_music_video_library_path(temp.path());
        seed_music_video_library(&pool, &library_path).await;
        let artist_id = upsert_artist_for_folder(&pool, &library_path, "Talking Heads")
            .await
            .expect("artist");

        upsert_local_music_video(
            &pool,
            &library_path,
            artist_id,
            &artist_dir,
            &official_path,
            probed_version(),
        )
        .await
        .expect("official upsert");
        upsert_local_music_video(
            &pool,
            &library_path,
            artist_id,
            &artist_dir,
            &unplugged_path,
            probed_version(),
        )
        .await
        .expect("unplugged upsert");

        link_alternate_versions_for_library_path(&pool, &library_path)
            .await
            .expect("link versions");

        let official_id = media_item_id_for_path(&pool, &official_path).await;
        let unplugged_id = media_item_id_for_path(&pool, &unplugged_path).await;
        let row = sqlx::query(
            r#"SELECT "ContentKind", "SubKind", "ParentMediaItemId"
               FROM "MediaItem" WHERE "Id" = ?"#,
        )
        .bind(unplugged_id)
        .fetch_one(&pool)
        .await
        .expect("unplugged row");
        assert_eq!(
            row.get::<i32, _>("ContentKind"),
            ersatzrs_domain::ContentKind::AlternateVersion.db_value()
        );
        assert_eq!(
            row.get::<Option<String>, _>("SubKind").as_deref(),
            Some("unplugged")
        );
        assert_eq!(
            row.get::<Option<i64>, _>("ParentMediaItemId"),
            Some(official_id)
        );

        let repository = SqliteSchedulingContentRepository::new(pool.clone());
        assert_eq!(search_ids(&repository, "Once").await, vec![official_id]);
        assert_eq!(
            search_ids(
                &repository,
                "content_kind:alternate_version AND sub_kind:unplugged"
            )
            .await,
            vec![unplugged_id]
        );
    }

    #[cfg(not(feature = "mysql"))]
    #[tokio::test]
    async fn mtv_unplugged_music_video_orphan_is_searchable_by_adr_selector() {
        use super::{
            link_alternate_versions_for_library_path, upsert_artist_for_folder,
            upsert_local_music_video,
        };
        use ersatzrs_infra::repositories::SqliteSchedulingContentRepository;
        use sqlx::Row;
        use tempfile::TempDir;

        let temp = TempDir::new().expect("temp dir");
        let mtv_dir = temp.path().join("MTV Unplugged");
        let unplugged_dir = mtv_dir.join("Alice in Chains Unplugged");
        std::fs::create_dir_all(&unplugged_dir).expect("unplugged dir");
        let unplugged_path =
            unplugged_dir.join("Alice In Chains - Down in a Hole MTV Unplugged - HD Video.mkv");
        std::fs::write(&unplugged_path, b"not probed").expect("unplugged video");

        let pool = test_db().await;
        let library_path = seed_music_video_library_path(temp.path());
        seed_music_video_library(&pool, &library_path).await;
        let artist_id = upsert_artist_for_folder(&pool, &library_path, "MTV Unplugged")
            .await
            .expect("artist");

        upsert_local_music_video(
            &pool,
            &library_path,
            artist_id,
            &mtv_dir,
            &unplugged_path,
            probed_version(),
        )
        .await
        .expect("unplugged upsert");
        link_alternate_versions_for_library_path(&pool, &library_path)
            .await
            .expect("link versions");

        let unplugged_id = media_item_id_for_path(&pool, &unplugged_path).await;
        let row = sqlx::query(
            r#"SELECT "ContentKind", "SubKind", "ParentMediaItemId"
               FROM "MediaItem" WHERE "Id" = ?"#,
        )
        .bind(unplugged_id)
        .fetch_one(&pool)
        .await
        .expect("unplugged row");
        assert_eq!(
            row.get::<i32, _>("ContentKind"),
            ersatzrs_domain::ContentKind::AlternateVersion.db_value()
        );
        assert_eq!(
            row.get::<Option<String>, _>("SubKind").as_deref(),
            Some("unplugged")
        );
        assert!(row.get::<Option<i64>, _>("ParentMediaItemId").is_none());

        let repository = SqliteSchedulingContentRepository::new(pool.clone());
        assert_eq!(
            search_ids(
                &repository,
                "content_kind:alternate_version AND sub_kind:unplugged"
            )
            .await,
            vec![unplugged_id]
        );
    }

    #[cfg(not(feature = "mysql"))]
    #[tokio::test]
    async fn orphan_song_alternate_is_typed_but_unlinked() {
        use super::{link_alternate_versions_for_library_path, upsert_local_file};
        use sqlx::Row;
        use tempfile::TempDir;

        let temp = TempDir::new().expect("temp dir");
        let song_path = temp.path().join("Track (Acoustic).flac");
        std::fs::write(&song_path, b"not probed").expect("song file");

        let pool = test_db().await;
        let library_path = seed_song_library(&pool, temp.path()).await;
        upsert_local_file(&pool, &library_path, &song_path, probed_version())
            .await
            .expect("song upsert");
        link_alternate_versions_for_library_path(&pool, &library_path)
            .await
            .expect("link versions");

        let song_id = media_item_id_for_path(&pool, &song_path).await;
        let row = sqlx::query(
            r#"SELECT "ContentKind", "SubKind", "ParentMediaItemId"
               FROM "MediaItem" WHERE "Id" = ?"#,
        )
        .bind(song_id)
        .fetch_one(&pool)
        .await
        .expect("song row");
        assert_eq!(
            row.get::<i32, _>("ContentKind"),
            ersatzrs_domain::ContentKind::AlternateVersion.db_value()
        );
        assert_eq!(
            row.get::<Option<String>, _>("SubKind").as_deref(),
            Some("acoustic")
        );
        assert!(row.get::<Option<i64>, _>("ParentMediaItemId").is_none());
    }

    #[cfg(not(feature = "mysql"))]
    #[tokio::test]
    async fn movie_directors_cut_links_to_unmarked_primary() {
        use super::{link_alternate_versions_for_library_path, upsert_local_file};
        use sqlx::Row;
        use tempfile::TempDir;

        let temp = TempDir::new().expect("temp dir");
        let primary_path = temp.path().join("Blade Runner.mkv");
        let cut_path = temp.path().join("Blade Runner (Director's Cut).mkv");
        std::fs::write(&primary_path, b"not probed").expect("primary movie");
        std::fs::write(&cut_path, b"not probed").expect("directors cut");

        let pool = test_db().await;
        let library_path = seed_movie_library_path(&pool, temp.path()).await;
        upsert_local_file(&pool, &library_path, &primary_path, probed_version())
            .await
            .expect("primary upsert");
        upsert_local_file(&pool, &library_path, &cut_path, probed_version())
            .await
            .expect("cut upsert");
        link_alternate_versions_for_library_path(&pool, &library_path)
            .await
            .expect("link versions");

        let primary_id = media_item_id_for_path(&pool, &primary_path).await;
        let cut_id = media_item_id_for_path(&pool, &cut_path).await;
        let row = sqlx::query(
            r#"SELECT "ContentKind", "SubKind", "ParentMediaItemId"
               FROM "MediaItem" WHERE "Id" = ?"#,
        )
        .bind(cut_id)
        .fetch_one(&pool)
        .await
        .expect("cut row");
        assert_eq!(
            row.get::<i32, _>("ContentKind"),
            ersatzrs_domain::ContentKind::AlternateVersion.db_value()
        );
        assert_eq!(
            row.get::<Option<String>, _>("SubKind").as_deref(),
            Some("directors_cut")
        );
        assert_eq!(
            row.get::<Option<i64>, _>("ParentMediaItemId"),
            Some(primary_id)
        );
    }

    #[cfg(not(feature = "mysql"))]
    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn remote_stream_scan_persists_definition_metadata_and_continues_after_invalid_yaml() {
        use sqlx::Row;
        use tempfile::TempDir;

        let temp = TempDir::new().expect("temp dir");
        let news = temp.path().join("News");
        std::fs::create_dir_all(&news).expect("news dir");
        let definition_path = news.join("Example.yaml");
        std::fs::write(
            &definition_path,
            r#"
url: https://example.test/live.m3u8
script: '"tools/fallback producer.sh" "ignored"'
is_live: true
duration: 01:02:03
fallback_query: tag:backup
title: Example Live
plot: Example plot
year: 2024
content_rating: TV-PG
"#,
        )
        .expect("definition");
        std::fs::write(news.join("Example.jpg"), b"artwork").expect("artwork");
        std::fs::write(
            temp.path().join("Broken.yml"),
            "is_live: false\nduration: invalid",
        )
        .expect("broken definition");

        let pool = test_db().await;
        let library_path = super::LocalLibraryPath {
            id: 1,
            path: temp.path().display().to_string(),
            media_kind: LibraryMediaKind::RemoteStreams,
        };
        seed_local_library(&pool, &library_path, "Remote Streams", 7).await;

        let report = super::scan_local_library(
            &pool,
            &super::LocalScanConfig {
                library_id: 1,
                ffprobe_path: "ffprobe-unused-for-definition-scan".to_owned(),
            },
        )
        .await
        .expect("scan");
        assert_eq!(report.scanned_files, 1);
        assert_eq!(report.failed_files, 1);
        assert_eq!(report.issues.len(), 1);
        assert!(report.issues[0].starts_with("Broken.yml:"));

        let row = sqlx::query(
            r#"SELECT rs."Url", rs."Script", rs."Duration", rs."FallbackQuery", rs."IsLive",
                      rsm."Title", rsm."Plot", rsm."Year", rsm."ContentRating",
                      mv."Duration" AS "VersionDuration", mf."Path"
               FROM "RemoteStream" rs
               JOIN "RemoteStreamMetadata" rsm ON rsm."RemoteStreamId" = rs."Id"
               JOIN "MediaVersion" mv ON mv."RemoteStreamId" = rs."Id"
               JOIN "MediaFile" mf ON mf."MediaVersionId" = mv."Id""#,
        )
        .fetch_one(&pool)
        .await
        .expect("remote stream row");
        assert_eq!(
            row.get::<Option<String>, _>("Url").as_deref(),
            Some("https://example.test/live.m3u8")
        );
        assert!(row
            .get::<Option<String>, _>("Script")
            .is_some_and(|script| script.contains("fallback producer.sh")));
        assert_eq!(
            row.get::<Option<String>, _>("Duration").as_deref(),
            Some("01:02:03")
        );
        assert_eq!(
            row.get::<Option<String>, _>("FallbackQuery").as_deref(),
            Some("tag:backup")
        );
        assert_eq!(row.get::<i64, _>("IsLive"), 1);
        assert_eq!(
            row.get::<Option<String>, _>("Title").as_deref(),
            Some("Example Live")
        );
        assert_eq!(
            row.get::<Option<String>, _>("Plot").as_deref(),
            Some("Example plot")
        );
        assert_eq!(row.get::<Option<i32>, _>("Year"), Some(2024));
        assert_eq!(
            row.get::<Option<String>, _>("ContentRating").as_deref(),
            Some("TV-PG")
        );
        assert_eq!(row.get::<String, _>("VersionDuration"), "01:02:03");
        assert_eq!(
            row.get::<String, _>("Path"),
            definition_path.display().to_string()
        );

        let tags = sqlx::query_scalar::<_, String>(r#"SELECT "Name" FROM "Tag" ORDER BY "Name""#)
            .fetch_all(&pool)
            .await
            .expect("tags");
        assert!(tags.contains(&"News".to_owned()));
        assert!(tags.contains(
            &temp
                .path()
                .file_name()
                .and_then(|name| name.to_str())
                .expect("root folder name")
                .to_owned()
        ));
        assert_eq!(
            sqlx::query_scalar::<_, i64>(r#"SELECT COUNT(*) FROM "Artwork""#)
                .fetch_one(&pool)
                .await
                .expect("artwork count"),
            1
        );

        std::fs::write(&definition_path, "is_live: false\n").expect("invalidate definition");
        let second_report = super::scan_local_library(
            &pool,
            &super::LocalScanConfig {
                library_id: 1,
                ffprobe_path: "ffprobe-unused-for-definition-scan".to_owned(),
            },
        )
        .await
        .expect("second scan");
        assert_eq!(second_report.failed_files, 2);
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                r#"SELECT "State"
                   FROM "MediaItem"
                   JOIN "RemoteStream" ON "RemoteStream"."Id" = "MediaItem"."Id""#,
            )
            .fetch_one(&pool)
            .await
            .expect("remote stream state"),
            1
        );
    }

    #[cfg(not(feature = "mysql"))]
    async fn search_ids(
        repository: &ersatzrs_infra::repositories::SqliteSchedulingContentRepository,
        query: &str,
    ) -> Vec<i64> {
        let (hits, _, _) = repository
            .search_documents_page(query, 0, 0)
            .await
            .expect("search page");
        hits.into_iter()
            .map(|ersatzrs_core::search::SearchDocument { media_item_id, .. }| media_item_id)
            .collect()
    }

    #[cfg(not(feature = "mysql"))]
    fn probed_version() -> ersatzrs_infra::ffprobe::FfprobeMediaVersion {
        ersatzrs_infra::ffprobe::FfprobeMediaVersion {
            version: ersatzrs_domain::MediaVersion {
                name: "Main".to_owned(),
                duration: chrono::Duration::seconds(180),
                video_scan_kind: ersatzrs_domain::VideoScanKind::Progressive,
                width: 1920,
                height: 1080,
                ..ersatzrs_domain::MediaVersion::default()
            },
            streams: Vec::new(),
            chapters: Vec::new(),
        }
    }

    #[cfg(not(feature = "mysql"))]
    async fn test_db() -> sqlx::SqlitePool {
        use sqlx::sqlite::SqlitePoolOptions;
        use sqlx::Executor;

        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("connect sqlite");
        pool.execute("PRAGMA foreign_keys = ON")
            .await
            .expect("foreign keys");
        sqlx::migrate!("../../migrations")
            .run(&pool)
            .await
            .expect("migrate");
        pool
    }

    #[cfg(not(feature = "mysql"))]
    fn seed_music_video_library_path(path: &Path) -> super::LocalLibraryPath {
        super::LocalLibraryPath {
            id: 1,
            path: path.display().to_string(),
            media_kind: LibraryMediaKind::MusicVideos,
        }
    }

    #[cfg(not(feature = "mysql"))]
    async fn seed_music_video_library(
        pool: &sqlx::SqlitePool,
        library_path: &super::LocalLibraryPath,
    ) {
        seed_local_library(pool, library_path, "Music Videos", 3).await;
    }

    #[cfg(not(feature = "mysql"))]
    async fn seed_song_library(pool: &sqlx::SqlitePool, path: &Path) -> super::LocalLibraryPath {
        let library_path = super::LocalLibraryPath {
            id: 1,
            path: path.display().to_string(),
            media_kind: LibraryMediaKind::Songs,
        };
        seed_local_library(pool, &library_path, "Songs", 5).await;
        library_path
    }

    #[cfg(not(feature = "mysql"))]
    async fn seed_movie_library_path(
        pool: &sqlx::SqlitePool,
        path: &Path,
    ) -> super::LocalLibraryPath {
        let library_path = super::LocalLibraryPath {
            id: 1,
            path: path.display().to_string(),
            media_kind: LibraryMediaKind::Movies,
        };
        seed_local_library(pool, &library_path, "Movies", 1).await;
        library_path
    }

    #[cfg(not(feature = "mysql"))]
    async fn seed_local_library(
        pool: &sqlx::SqlitePool,
        library_path: &super::LocalLibraryPath,
        name: &str,
        media_kind: i32,
    ) {
        use sqlx::Executor;

        pool.execute(r#"INSERT INTO "MediaSource" ("Id") VALUES (1)"#)
            .await
            .expect("media source");
        pool.execute(r#"INSERT INTO "LocalMediaSource" ("Id") VALUES (1)"#)
            .await
            .expect("local source");
        sqlx::query(
            r#"INSERT INTO "Library" ("Id", "Name", "MediaKind", "MediaSourceId")
               VALUES (1, ?, ?, 1)"#,
        )
        .bind(name)
        .bind(media_kind)
        .execute(pool)
        .await
        .expect("library");
        pool.execute(r#"INSERT INTO "LocalLibrary" ("Id") VALUES (1)"#)
            .await
            .expect("local library");
        sqlx::query(r#"INSERT INTO "LibraryPath" ("Id", "LibraryId", "Path") VALUES (1, 1, ?)"#)
            .bind(&library_path.path)
            .execute(pool)
            .await
            .expect("library path");
    }

    #[cfg(not(feature = "mysql"))]
    async fn media_item_id_for_path(pool: &sqlx::SqlitePool, path: &Path) -> i64 {
        use sqlx::Row;

        let path_text = path.display().to_string();
        let hash = path_hash(&path_text);
        sqlx::query(
            r#"SELECT COALESCE(
                    "mv"."MovieId", "mv"."MusicVideoId", "mv"."SongId",
                    "mv"."OtherVideoId", "mv"."ImageId"
               ) AS "MediaItemId"
               FROM "MediaFile" AS "mf"
               JOIN "MediaVersion" AS "mv" ON "mv"."Id" = "mf"."MediaVersionId"
               WHERE "mf"."PathHash" = ?"#,
        )
        .bind(hash)
        .fetch_one(pool)
        .await
        .expect("media file")
        .get::<i64, _>("MediaItemId")
    }

    #[cfg(not(feature = "mysql"))]
    async fn seed_movie_library(pool: &sqlx::SqlitePool, path: &Path) {
        use sqlx::Executor;

        pool.execute(r#"INSERT INTO "MediaSource" ("Id") VALUES (1)"#)
            .await
            .expect("media source");
        pool.execute(r#"INSERT INTO "LocalMediaSource" ("Id") VALUES (1)"#)
            .await
            .expect("local source");
        sqlx::query(
            r#"INSERT INTO "Library" ("Id", "Name", "MediaKind", "MediaSourceId")
               VALUES (1, 'Movies', 1, 1)"#,
        )
        .execute(pool)
        .await
        .expect("library");
        pool.execute(r#"INSERT INTO "LocalLibrary" ("Id") VALUES (1)"#)
            .await
            .expect("local library");
        sqlx::query(r#"INSERT INTO "LibraryPath" ("Id", "LibraryId", "Path") VALUES (1, 1, ?)"#)
            .bind(path.display().to_string())
            .execute(pool)
            .await
            .expect("library path");
    }

    #[cfg(not(feature = "mysql"))]
    async fn count_table(pool: &sqlx::SqlitePool, table: &str) -> i64 {
        let sql = format!(r#"SELECT COUNT(*) FROM "{table}""#);
        sqlx::query_scalar(ersatzrs_infra::db::assert_sql_safe(sql))
            .fetch_one(pool)
            .await
            .expect("count")
    }

    #[cfg(not(feature = "mysql"))]
    #[tokio::test]
    async fn sqlite_busy_retry_waits_for_concurrent_writer() {
        use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
        use sqlx::{Executor, Row};
        use std::time::Duration as StdDuration;
        use tempfile::TempDir;

        async fn pool(database_url: &str) -> sqlx::SqlitePool {
            let options = database_url
                .parse::<SqliteConnectOptions>()
                .expect("sqlite options")
                .create_if_missing(true)
                .foreign_keys(true)
                .busy_timeout(StdDuration::from_millis(50));
            SqlitePoolOptions::new()
                .max_connections(1)
                .connect_with(options)
                .await
                .expect("connect sqlite")
        }

        let temp = TempDir::new().expect("temp dir");
        let database_url = format!("sqlite:{}", temp.path().join("busy.db").display());
        let holder = pool(&database_url).await;
        let writer = pool(&database_url).await;
        holder
            .execute(r#"CREATE TABLE "BusyRetry" ("Id" INTEGER PRIMARY KEY, "Name" TEXT)"#)
            .await
            .expect("create table");

        let lock_pool = holder.clone();
        let lock = tokio::spawn(async move {
            let mut tx = lock_pool.begin().await.expect("begin lock tx");
            sqlx::query(r#"INSERT INTO "BusyRetry" ("Name") VALUES ('held')"#)
                .execute(&mut *tx)
                .await
                .expect("hold write lock");
            tokio::time::sleep(StdDuration::from_millis(300)).await;
            tx.commit().await.expect("release lock");
        });
        tokio::time::sleep(StdDuration::from_millis(50)).await;

        retry_sqlite_busy(|| async {
            sqlx::query(r#"INSERT INTO "BusyRetry" ("Name") VALUES ('retried')"#)
                .execute(&writer)
                .await?;
            Ok(())
        })
        .await
        .expect("retry should wait for lock release");
        lock.await.expect("lock task");

        let count: i64 = sqlx::query(r#"SELECT COUNT(*) AS "Count" FROM "BusyRetry""#)
            .fetch_one(&writer)
            .await
            .expect("count rows")
            .get("Count");
        assert_eq!(count, 2);
    }

    #[cfg(not(feature = "mysql"))]
    mod movie_enrichment {
        use super::super::{
            enrich_movie_metadata, insert_media_item, original_title_for_path,
            METADATA_KIND_FALLBACK, METADATA_KIND_SIDECAR,
        };
        use crate::LocalLibraryPath;
        use ersatzrs_domain::LibraryMediaKind;
        use ersatzrs_infra::db::assert_sql_safe;
        use sqlx::sqlite::SqlitePoolOptions;
        use sqlx::{Executor, Row, SqlitePool};
        use std::path::Path;
        use tempfile::TempDir;

        #[tokio::test]
        async fn enriches_movie_sidecar_artwork_and_child_rows_idempotently() {
            let temp = TempDir::new().expect("temp dir");
            let movie_path = temp.path().join("Film (2020).mkv");
            std::fs::write(&movie_path, b"not probed").expect("movie file");
            std::fs::write(
                temp.path().join("Film (2020).nfo"),
                r#"<movie>
                    <title>The Film</title>
                    <sorttitle>Film</sorttitle>
                    <outline>Short summary.</outline>
                    <plot>Detailed plot.</plot>
                    <year>2020</year>
                    <mpaa>PG-13</mpaa>
                    <premiered>2020-02-03</premiered>
                    <genre>Drama</genre>
                    <tag>Festival</tag>
                    <studio>Studio One</studio>
                    <actor><name>Ada Actor</name><role>Lead</role><order>1</order></actor>
                    <director>Dana Director</director>
                    <credits>Will Writer</credits>
                    <uniqueid type="imdb" default="true">tt1234567</uniqueid>
                </movie>"#,
            )
            .expect("nfo file");
            std::fs::write(temp.path().join("poster.jpg"), b"poster").expect("poster");
            std::fs::write(temp.path().join("fanart.jpg"), b"fanart").expect("fanart");

            let pool = test_db().await;
            let library_path = seed_movie_library(&pool, temp.path()).await;
            let movie_id = seed_movie(&pool, library_path.id).await;

            let first_metadata_id = enrich_once(&pool, movie_id, &library_path, &movie_path).await;
            let second_metadata_id = enrich_once(&pool, movie_id, &library_path, &movie_path).await;

            assert_eq!(first_metadata_id, second_metadata_id);

            let row = sqlx::query(
                r#"SELECT "MetadataKind", "Title", "SortTitle", "OriginalTitle", "Year",
                          "ReleaseDate", "ContentRating", "Plot", "Outline"
                   FROM "MovieMetadata" WHERE "Id" = ?"#,
            )
            .bind(first_metadata_id)
            .fetch_one(&pool)
            .await
            .expect("metadata row");

            assert_eq!(row.get::<i32, _>("MetadataKind"), METADATA_KIND_SIDECAR);
            assert_eq!(
                row.get::<Option<String>, _>("Title").as_deref(),
                Some("The Film")
            );
            assert_eq!(
                row.get::<Option<String>, _>("SortTitle").as_deref(),
                Some("Film")
            );
            assert_eq!(
                row.get::<Option<String>, _>("OriginalTitle").as_deref(),
                Some("Film (2020).mkv")
            );
            assert_eq!(row.get::<Option<i32>, _>("Year"), Some(2020));
            assert_eq!(
                row.get::<Option<String>, _>("ReleaseDate").as_deref(),
                Some("2020-02-03 00:00:00")
            );
            assert_eq!(
                row.get::<Option<String>, _>("ContentRating").as_deref(),
                Some("PG-13")
            );
            assert_eq!(
                row.get::<Option<String>, _>("Plot").as_deref(),
                Some("Detailed plot.")
            );
            assert_eq!(
                row.get::<Option<String>, _>("Outline").as_deref(),
                Some("Short summary.")
            );

            assert_eq!(
                count_for_metadata(&pool, "Artwork", first_metadata_id).await,
                2
            );
            assert_eq!(
                count_for_metadata(&pool, "Genre", first_metadata_id).await,
                1
            );
            assert_eq!(count_for_metadata(&pool, "Tag", first_metadata_id).await, 1);
            assert_eq!(
                count_for_metadata(&pool, "Studio", first_metadata_id).await,
                1
            );
            assert_eq!(
                count_for_metadata(&pool, "Actor", first_metadata_id).await,
                1
            );
            assert_eq!(
                count_for_metadata(&pool, "Director", first_metadata_id).await,
                1
            );
            assert_eq!(
                count_for_metadata(&pool, "Writer", first_metadata_id).await,
                1
            );
            assert_eq!(
                count_for_metadata(&pool, "MetadataGuid", first_metadata_id).await,
                1
            );
            assert_eq!(count_table(&pool, "MovieMetadata").await, 1);
        }

        #[tokio::test]
        async fn fallback_movie_keeps_metadata_kind_zero_and_attaches_artwork() {
            let temp = TempDir::new().expect("temp dir");
            let movie_path = temp.path().join("Plain Film.mkv");
            std::fs::write(&movie_path, b"not probed").expect("movie file");
            std::fs::write(temp.path().join("poster.jpg"), b"poster").expect("poster");
            std::fs::write(temp.path().join("fanart.jpg"), b"fanart").expect("fanart");

            let pool = test_db().await;
            let library_path = seed_movie_library(&pool, temp.path()).await;
            let movie_id = seed_movie(&pool, library_path.id).await;

            let metadata_id = enrich_once(&pool, movie_id, &library_path, &movie_path).await;

            let row = sqlx::query(
                r#"SELECT "MetadataKind", "Title", "Plot"
                   FROM "MovieMetadata" WHERE "Id" = ?"#,
            )
            .bind(metadata_id)
            .fetch_one(&pool)
            .await
            .expect("metadata row");
            assert_eq!(row.get::<i32, _>("MetadataKind"), METADATA_KIND_FALLBACK);
            assert_eq!(
                row.get::<Option<String>, _>("Title").as_deref(),
                Some("Plain Film")
            );
            assert!(row.get::<Option<String>, _>("Plot").is_none());
            assert_eq!(count_for_metadata(&pool, "Artwork", metadata_id).await, 2);
        }

        #[tokio::test]
        async fn malformed_movie_nfo_falls_back_without_failing_scan_transaction() {
            let temp = TempDir::new().expect("temp dir");
            let movie_path = temp.path().join("Broken Film.mkv");
            std::fs::write(&movie_path, b"not probed").expect("movie file");
            std::fs::write(temp.path().join("Broken Film.nfo"), b"<movie><title>Broken")
                .expect("nfo file");

            let pool = test_db().await;
            let library_path = seed_movie_library(&pool, temp.path()).await;
            let movie_id = seed_movie(&pool, library_path.id).await;

            let metadata_id = enrich_once(&pool, movie_id, &library_path, &movie_path).await;

            let row = sqlx::query(
                r#"SELECT "MetadataKind", "Title" FROM "MovieMetadata" WHERE "Id" = ?"#,
            )
            .bind(metadata_id)
            .fetch_one(&pool)
            .await
            .expect("metadata row");
            assert_eq!(row.get::<i32, _>("MetadataKind"), METADATA_KIND_FALLBACK);
            assert_eq!(
                row.get::<Option<String>, _>("Title").as_deref(),
                Some("Broken Film")
            );
        }

        async fn test_db() -> SqlitePool {
            let pool = SqlitePoolOptions::new()
                .max_connections(1)
                .connect("sqlite::memory:")
                .await
                .expect("connect sqlite");
            pool.execute("PRAGMA foreign_keys = ON")
                .await
                .expect("foreign keys");
            sqlx::migrate!("../../migrations")
                .run(&pool)
                .await
                .expect("migrate");
            pool
        }

        async fn seed_movie_library(pool: &SqlitePool, path: &Path) -> LocalLibraryPath {
            pool.execute(r#"INSERT INTO "MediaSource" ("Id") VALUES (1)"#)
                .await
                .expect("media source");
            pool.execute(r#"INSERT INTO "LocalMediaSource" ("Id") VALUES (1)"#)
                .await
                .expect("local source");
            sqlx::query(
                r#"INSERT INTO "Library" ("Id", "Name", "MediaKind", "MediaSourceId")
                   VALUES (1, 'Movies', 1, 1)"#,
            )
            .execute(pool)
            .await
            .expect("library");
            pool.execute(r#"INSERT INTO "LocalLibrary" ("Id") VALUES (1)"#)
                .await
                .expect("local library");
            sqlx::query(
                r#"INSERT INTO "LibraryPath" ("Id", "LibraryId", "Path") VALUES (1, 1, ?)"#,
            )
            .bind(path.display().to_string())
            .execute(pool)
            .await
            .expect("library path");

            LocalLibraryPath {
                id: 1,
                path: path.display().to_string(),
                media_kind: LibraryMediaKind::Movies,
            }
        }

        async fn seed_movie(pool: &SqlitePool, library_path_id: i64) -> i64 {
            let mut tx = pool.begin().await.expect("begin");
            let movie_id = insert_media_item(&mut tx, library_path_id)
                .await
                .expect("media item");
            sqlx::query(r#"INSERT INTO "Movie" ("Id") VALUES (?)"#)
                .bind(movie_id)
                .execute(&mut *tx)
                .await
                .expect("movie");
            tx.commit().await.expect("commit");
            movie_id
        }

        async fn enrich_once(
            pool: &SqlitePool,
            movie_id: i64,
            library_path: &LocalLibraryPath,
            movie_path: &Path,
        ) -> i64 {
            assert_eq!(
                original_title_for_path(library_path, movie_path).as_deref(),
                movie_path.file_name().and_then(|name| name.to_str())
            );
            let mut tx = pool.begin().await.expect("begin");
            let metadata_id = enrich_movie_metadata(&mut tx, movie_id, library_path, movie_path)
                .await
                .expect("enrich metadata");
            tx.commit().await.expect("commit");
            metadata_id
        }

        async fn count_for_metadata(pool: &SqlitePool, table: &str, metadata_id: i64) -> i64 {
            let sql = format!(r#"SELECT COUNT(*) FROM "{table}" WHERE "MovieMetadataId" = ?"#);
            sqlx::query_scalar(assert_sql_safe(sql))
                .bind(metadata_id)
                .fetch_one(pool)
                .await
                .expect("count")
        }

        async fn count_table(pool: &SqlitePool, table: &str) -> i64 {
            let sql = format!(r#"SELECT COUNT(*) FROM "{table}""#);
            sqlx::query_scalar(assert_sql_safe(sql))
                .fetch_one(pool)
                .await
                .expect("count")
        }
    }

    #[cfg(not(feature = "mysql"))]
    mod music_video_enrichment {
        use super::super::{
            upsert_artist_for_folder, upsert_local_music_video, LocalLibraryPath,
            METADATA_KIND_FALLBACK, METADATA_KIND_SIDECAR,
        };
        use chrono::Duration;
        use ersatzrs_domain::{LibraryMediaKind, MediaVersion, VideoScanKind};
        use ersatzrs_infra::db::assert_sql_safe;
        use ersatzrs_infra::ffprobe::FfprobeMediaVersion;
        use sqlx::sqlite::SqlitePoolOptions;
        use sqlx::{Executor, Row, SqlitePool};
        use std::path::Path;
        use tempfile::TempDir;

        #[tokio::test]
        async fn writes_music_video_metadata_from_fragment_nfo() {
            let temp = TempDir::new().expect("temp dir");
            let library_path = seed_music_video_library_path(temp.path());
            let artist_dir = temp.path().join("Talking Heads");
            std::fs::create_dir_all(&artist_dir).expect("artist dir");
            let video_path = artist_dir.join("Once in a Lifetime.mkv");
            std::fs::write(&video_path, b"not probed").expect("video file");
            std::fs::write(
                artist_dir.join("Once in a Lifetime.nfo"),
                r"https://example.invalid/nfo
                <musicvideo>
                    <artist>Talking Heads</artist>
                    <title>Once in a Lifetime</title>
                    <album>Remain in Light</album>
                    <track>4</track>
                    <plot>Iconic music video.</plot>
                    <year>1980</year>
                    <genre>New Wave</genre>
                    <tag>iconic</tag>
                    <studio>Sire</studio>
                    <director>Toni Basil</director>
                </musicvideo>",
            )
            .expect("nfo file");

            let pool = test_db().await;
            seed_music_video_library(&pool, &library_path).await;
            let artist_id = upsert_artist_for_folder(&pool, &library_path, "Talking Heads")
                .await
                .expect("artist");

            upsert_local_music_video(
                &pool,
                &library_path,
                artist_id,
                &artist_dir,
                &video_path,
                probed_version(),
            )
            .await
            .expect("music video upsert");

            let row = sqlx::query(
                r#"SELECT "MetadataKind", "Title", "Album", "Plot", "Year", "Track"
                   FROM "MusicVideoMetadata" LIMIT 1"#,
            )
            .fetch_one(&pool)
            .await
            .expect("metadata row");
            assert_eq!(row.get::<i32, _>("MetadataKind"), METADATA_KIND_SIDECAR);
            assert_eq!(
                row.get::<Option<String>, _>("Title").as_deref(),
                Some("Once in a Lifetime")
            );
            assert_eq!(
                row.get::<Option<String>, _>("Album").as_deref(),
                Some("Remain in Light")
            );
            assert_eq!(
                row.get::<Option<String>, _>("Plot").as_deref(),
                Some("Iconic music video.")
            );
            assert_eq!(row.get::<Option<i32>, _>("Year"), Some(1980));
            assert_eq!(row.get::<Option<i32>, _>("Track"), Some(4));

            assert_eq!(count(&pool, "MusicVideoArtist").await, 1);
            assert_eq!(count(&pool, "Genre").await, 1);
            assert_eq!(count(&pool, "Tag").await, 1);
            assert_eq!(count(&pool, "Studio").await, 1);
            assert_eq!(count(&pool, "Director").await, 1);
        }

        #[tokio::test]
        async fn invalid_music_video_nfo_falls_back_to_filename_metadata() {
            let temp = TempDir::new().expect("temp dir");
            let library_path = seed_music_video_library_path(temp.path());
            let artist_dir = temp.path().join("Talking Heads");
            std::fs::create_dir_all(&artist_dir).expect("artist dir");
            let video_path = artist_dir.join("Broken Video.mkv");
            std::fs::write(&video_path, b"not probed").expect("video file");
            std::fs::write(artist_dir.join("Broken Video.nfo"), b"<not-music-video />")
                .expect("nfo file");

            let pool = test_db().await;
            seed_music_video_library(&pool, &library_path).await;
            let artist_id = upsert_artist_for_folder(&pool, &library_path, "Talking Heads")
                .await
                .expect("artist");

            upsert_local_music_video(
                &pool,
                &library_path,
                artist_id,
                &artist_dir,
                &video_path,
                probed_version(),
            )
            .await
            .expect("music video upsert");

            let row = sqlx::query(
                r#"SELECT "MetadataKind", "Title", "Album", "Track"
                   FROM "MusicVideoMetadata" LIMIT 1"#,
            )
            .fetch_one(&pool)
            .await
            .expect("metadata row");
            assert_eq!(row.get::<i32, _>("MetadataKind"), METADATA_KIND_FALLBACK);
            assert_eq!(
                row.get::<Option<String>, _>("Title").as_deref(),
                Some("Broken Video")
            );
            assert!(row.get::<Option<String>, _>("Album").is_none());
            assert!(row.get::<Option<i32>, _>("Track").is_none());
        }

        fn seed_music_video_library_path(path: &Path) -> LocalLibraryPath {
            LocalLibraryPath {
                id: 1,
                path: path.display().to_string(),
                media_kind: LibraryMediaKind::MusicVideos,
            }
        }

        async fn seed_music_video_library(pool: &SqlitePool, library_path: &LocalLibraryPath) {
            pool.execute(r#"INSERT INTO "MediaSource" ("Id") VALUES (1)"#)
                .await
                .expect("media source");
            pool.execute(r#"INSERT INTO "LocalMediaSource" ("Id") VALUES (1)"#)
                .await
                .expect("local source");
            pool.execute(
                r#"INSERT INTO "Library" ("Id", "Name", "MediaKind", "MediaSourceId")
                   VALUES (1, 'Music Videos', 3, 1)"#,
            )
            .await
            .expect("library");
            pool.execute(r#"INSERT INTO "LocalLibrary" ("Id") VALUES (1)"#)
                .await
                .expect("local library");
            sqlx::query(
                r#"INSERT INTO "LibraryPath" ("Id", "LibraryId", "Path") VALUES (1, 1, ?)"#,
            )
            .bind(&library_path.path)
            .execute(pool)
            .await
            .expect("library path");
        }

        fn probed_version() -> FfprobeMediaVersion {
            FfprobeMediaVersion {
                version: MediaVersion {
                    name: "Main".to_owned(),
                    duration: Duration::seconds(180),
                    video_scan_kind: VideoScanKind::Progressive,
                    width: 1920,
                    height: 1080,
                    ..MediaVersion::default()
                },
                streams: Vec::new(),
                chapters: Vec::new(),
            }
        }

        async fn test_db() -> SqlitePool {
            let pool = SqlitePoolOptions::new()
                .max_connections(1)
                .connect("sqlite::memory:")
                .await
                .expect("connect sqlite");
            pool.execute("PRAGMA foreign_keys = ON")
                .await
                .expect("foreign keys");
            sqlx::migrate!("../../migrations")
                .run(&pool)
                .await
                .expect("migrate");
            pool
        }

        async fn count(pool: &SqlitePool, table: &str) -> i64 {
            let sql = format!(r#"SELECT COUNT(*) FROM "{table}""#);
            sqlx::query_scalar(assert_sql_safe(sql))
                .fetch_one(pool)
                .await
                .expect("count")
        }
    }
}
