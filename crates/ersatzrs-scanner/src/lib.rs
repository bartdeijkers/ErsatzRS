#![forbid(unsafe_code)]

use std::collections::{BTreeMap, VecDeque};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::time::Duration as StdDuration;

use chrono::{DateTime, Duration, Utc};
use ersatzrs_core::metadata_facets::{
    normalize_content_ratings, normalize_facet_values, normalize_language_codes,
    preferred_facet_values, split_embedded_values, union_facet_values,
};
use ersatzrs_core::repositories::RepositoryError;
use ersatzrs_domain::{
    classify_alternate_version, classify_extra_video, normalize_group_key,
    normalize_sub_kind_token, AlternateVersionClassification, ContentKind, ExtraParentHint,
    ExtraVideoClassification, LibraryMediaKind, MediaChapter, MediaStream, MediaStreamKind,
    ScanPlayoutRefresh, ScanProgressSink,
};
use ersatzrs_infra::addons::AddonRuntimeContext;
use ersatzrs_infra::db::{Db, DbPool, DbRow};
use ersatzrs_infra::ffprobe::{FfprobeError, FfprobeExtractor, FfprobeMediaVersion};
use ersatzrs_infra::metadata::{
    artwork_kind, replace_artwork, replace_guids, replace_metadata_languages, replace_people,
    replace_strings, update_local_file_subtitles, ArtworkRef, PersonRef, SubtitleMetadataOwner,
};
use ersatzrs_infra::remote_stream::{
    parse_remote_stream_definition, resolve_remote_stream_for_scan_with_addons, RemoteStreamError,
};
use sha2::{Digest, Sha256};
use sqlx::{Row, Transaction};
use thiserror::Error;
use tracing::{debug, warn};

mod folder_etag;
mod library_folder;
pub mod local_television;
pub mod nfo;
pub mod remote_stream_playlist;

const METADATA_KIND_FALLBACK: i32 = 0;
const METADATA_KIND_SIDECAR: i32 = 1;
const METADATA_KIND_EMBEDDED: i32 = 3;
const SQLITE_BUSY_RETRY_DELAYS_MS: &[u64] = &[250, 500, 1_000, 2_000, 4_000];

#[derive(Debug, Clone, Default)]
pub(crate) struct EmbeddedFacets {
    pub(crate) content_ratings: Vec<String>,
    pub(crate) studios: Vec<String>,
    pub(crate) music_labels: Vec<String>,
    pub(crate) languages: Vec<String>,
    pub(crate) genres: Vec<String>,
    pub(crate) tags: Vec<String>,
}

impl EmbeddedFacets {
    pub(crate) fn from_probe(probed: &FfprobeMediaVersion) -> Self {
        let ratings = embedded_metadata_values(
            &probed.format_metadata,
            &["content_rating", "contentrating", "mpaa", "parental_rating"],
        );
        Self {
            content_ratings: normalize_content_ratings(ratings),
            studios: embedded_metadata_values(
                &probed.format_metadata,
                &["studio", "production_studio"],
            ),
            music_labels: embedded_metadata_values(
                &probed.format_metadata,
                &["label", "publisher", "organization"],
            ),
            languages: normalize_language_codes(embedded_metadata_values(
                &probed.format_metadata,
                &["language"],
            )),
            genres: embedded_metadata_values(&probed.format_metadata, &["genre"]),
            tags: embedded_metadata_values(
                &probed.format_metadata,
                &["tag", "tags", "keywords", "grouping"],
            ),
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.content_ratings.is_empty()
            && self.studios.is_empty()
            && self.music_labels.is_empty()
            && self.languages.is_empty()
            && self.genres.is_empty()
            && self.tags.is_empty()
    }
}

fn local_metadata_kind(
    has_sidecar: bool,
    probed: &FfprobeMediaVersion,
    embedded: &EmbeddedFacets,
    scalar_aliases: &[&str],
) -> i32 {
    if has_sidecar {
        return METADATA_KIND_SIDECAR;
    }
    let has_scalar = scalar_aliases.iter().any(|alias| {
        probed
            .format_metadata
            .keys()
            .any(|key| key.eq_ignore_ascii_case(alias))
    });
    if !embedded.is_empty() || has_scalar {
        METADATA_KIND_EMBEDDED
    } else {
        METADATA_KIND_FALLBACK
    }
}

fn embedded_metadata_values(metadata: &BTreeMap<String, String>, aliases: &[&str]) -> Vec<String> {
    normalize_facet_values(
        aliases
            .iter()
            .flat_map(|alias| {
                metadata.iter().filter_map(move |(key, value)| {
                    key.eq_ignore_ascii_case(alias).then_some(value)
                })
            })
            .flat_map(|value| split_embedded_values(value)),
    )
}

pub(crate) fn audio_stream_languages(probed: &FfprobeMediaVersion) -> Vec<String> {
    normalize_language_codes(
        probed
            .streams
            .iter()
            .filter(|stream| stream.kind == MediaStreamKind::Audio)
            .filter_map(|stream| stream.language.as_deref()),
    )
}

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
    #[error(transparent)]
    RemoteStreamPlaylist(#[from] remote_stream_playlist::PlaylistError),
    #[error("local television reconciliation failed: {0}")]
    Reconciliation(String),
    #[error("failed to read remote stream definition {path}: {source}")]
    RemoteStreamRead {
        path: String,
        source: std::io::Error,
    },
    #[error("failed to read remote stream playlist manifest {path}: {source}")]
    RemoteStreamPlaylistRead {
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
    pub playout_refreshes: Vec<ScanPlayoutRefresh>,
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
    RemoteStreamPlaylist {
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
    is_available: bool,
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
                let items = collect_remote_stream_files(&library_path.path);
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
                    let is_remote_stream = matches!(
                        &work,
                        LocalScanWork::RemoteStream { .. }
                            | LocalScanWork::RemoteStreamPlaylist { .. }
                    );
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
                            let result = match load_remote_stream_definition(
                                pool,
                                &path,
                                &config.ffprobe_path,
                            )
                            .await
                            {
                                Ok((definition, resolved)) => {
                                    let probed = if is_managed_remote_stream_definition(&path)
                                        || resolved.liveness
                                            == ersatzrs_domain::RemoteStreamLiveness::Unknown
                                    {
                                        None
                                    } else {
                                        match extractor.extract_remote(&resolved).await {
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
                        LocalScanWork::RemoteStreamPlaylist { path } => {
                            match synchronize_remote_stream_playlist(
                                pool,
                                config.library_id,
                                &library_path,
                                &path,
                                &config.ffprobe_path,
                            )
                            .await
                            {
                                Ok(playout_refreshes) => {
                                    report.playout_refreshes.extend(playout_refreshes);
                                    report.scanned_files += 1;
                                }
                                Err(error) => {
                                    report.failed_files += 1;
                                    let display_path = path
                                        .strip_prefix(&library_path.path)
                                        .unwrap_or(&path)
                                        .display()
                                        .to_string();
                                    report.issues.push(format!("{display_path}: {error}"));
                                    warn!(
                                        error = %error,
                                        manifest = %display_path,
                                        "remote stream playlist validation failed; continuing"
                                    );
                                }
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
        retry_sqlite_busy(|| async {
            link_alternate_versions_for_library_path(pool, &library_path).await
        })
        .await?;
        if library_path.media_kind == LibraryMediaKind::MusicVideos {
            retry_sqlite_busy(|| async { delete_empty_artists(pool, library_path.id).await })
                .await?;
        }
        let now = Utc::now();
        retry_sqlite_busy(|| async { update_library_path_scan(pool, library_path.id, now).await })
            .await?;
    }

    retry_sqlite_busy(|| async { update_library_scan(pool, config.library_id, Utc::now()).await })
        .await?;
    if let Some(sink) = progress {
        sink.report(1.0);
    }
    let mut refreshes = BTreeMap::new();
    for refresh in std::mem::take(&mut report.playout_refreshes) {
        refreshes
            .entry(refresh.playout_id)
            .and_modify(|reason: &mut ersatzrs_domain::ScanPlayoutRefreshReason| {
                *reason = reason.merge(refresh.reason);
            })
            .or_insert(refresh.reason);
    }
    report.playout_refreshes = refreshes
        .into_iter()
        .map(|(playout_id, reason)| ScanPlayoutRefresh { playout_id, reason })
        .collect();
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

/// Walks the remote-stream library root and distinguishes playlist manifests
/// from ordinary `.yml`/`.yaml` definitions before strict definition parsing.
fn collect_remote_stream_files(root: &str) -> Vec<LocalScanWork> {
    let mut files = Vec::new();
    let mut queue = VecDeque::from([PathBuf::from(root)]);
    while let Some(path) = queue.pop_front() {
        let Ok(entries) = std::fs::read_dir(&path) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with('.'))
                {
                    continue;
                }
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
                if remote_stream_playlist::is_playlist_manifest(&path) {
                    files.push(LocalScanWork::RemoteStreamPlaylist { path });
                } else {
                    files.push(LocalScanWork::RemoteStream { path });
                }
            }
        }
    }
    files.sort_by(|left, right| {
        let class = |work: &LocalScanWork| match work {
            LocalScanWork::RemoteStreamPlaylist { .. } => 0_u8,
            LocalScanWork::RemoteStream { .. } => 1_u8,
            _ => 2_u8,
        };
        class(left)
            .cmp(&class(right))
            .then_with(|| work_path(left).cmp(work_path(right)))
    });
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
        | LocalScanWork::RemoteStreamPlaylist { path }
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
        apply_alternate_version_group(pool, &group, library_path.media_kind).await?;
    }

    Ok(())
}

async fn apply_alternate_version_group(
    pool: &DbPool,
    group: &[AlternateVersionRow],
    media_kind: LibraryMediaKind,
) -> Result<()> {
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

    // For music videos only, a playable remastered rendition is preferred over
    // a plain/"official" original: its video quality is almost always higher
    // and it is what channel playout should prefer. Movie and song remasters
    // retain the normal alternate-version rules and vocabulary.
    let available_count = group.iter().filter(|row| row.is_available).count();
    if available_count == 0 {
        return Ok(());
    }

    let remaster_id = if media_kind == LibraryMediaKind::MusicVideos {
        group
            .iter()
            .find(|row| {
                row.is_available && row.classification.sub_kind.as_deref() == Some("remaster")
            })
            .map(|row| row.item_id)
    } else {
        None
    };

    if group.len() == 1 {
        let row = &group[0];
        if Some(row.item_id) == remaster_id {
            reset_primary_variant_fields(pool, row.item_id).await?;
        } else if let Some(sub_kind) = &row.classification.sub_kind {
            set_alternate_variant_fields(pool, row.item_id, sub_kind, None).await?;
        } else {
            reset_primary_variant_fields(pool, row.item_id).await?;
        }
        return Ok(());
    }

    let canonical_id = remaster_id
        .or_else(|| {
            group
                .iter()
                .find(|row| {
                    row.is_available
                        && row.classification.sub_kind.is_none()
                        && row.classification.canonical_hint
                })
                .map(|row| row.item_id)
        })
        .or_else(|| {
            group
                .iter()
                .find(|row| row.is_available && row.classification.sub_kind.is_none())
                .map(|row| row.item_id)
        })
        .or_else(|| {
            group
                .iter()
                .find(|row| row.is_available)
                .map(|row| row.item_id)
        })
        .expect("an available alternate-version row exists");
    let promoted_remaster = remaster_id == Some(canonical_id);

    for row in group {
        if row.item_id == canonical_id {
            reset_primary_variant_fields(pool, row.item_id).await?;
        } else if let Some(sub_kind) = &row.classification.sub_kind {
            set_alternate_variant_fields(pool, row.item_id, sub_kind, Some(canonical_id)).await?;
        } else if promoted_remaster {
            // The untagged original master, superseded by the promoted
            // remaster above, is demoted rather than left as its own
            // primary; `official_video` is the existing ADR 0018 vocabulary
            // slot for "the (now non-canonical) official rendition".
            set_alternate_variant_fields(pool, row.item_id, "official_video", Some(canonical_id))
                .await?;
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
                  "mi"."State" AS "State",
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
                  "mi"."State" AS "State",
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
                  "mi"."State" AS "State",
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
        is_available: row.get::<i32, _>("State") == 0,
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
    let mut tx = ersatzrs_infra::db::begin_write(pool).await?;

    let existing: Option<(i64, Option<i64>)> = sqlx::query(
        r#"SELECT "a"."Id" AS "ArtistId", "am"."Id" AS "ArtistMetadataId"
           FROM "Artist" AS "a"
           JOIN "MediaItem" AS "mi" ON "mi"."Id" = "a"."Id"
           LEFT JOIN "ArtistMetadata" AS "am" ON "am"."ArtistId" = "a"."Id"
           WHERE "mi"."LibraryPathId" = ?
             AND COALESCE("am"."Title", '') = ?
             AND (? = 0 OR COALESCE("am"."Disambiguation", '') = COALESCE(?, ''))
           LIMIT 1"#,
    )
    .bind(library_path.id)
    .bind(&title)
    .bind(metadata_kind)
    .bind(disambiguation.as_deref())
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
    let embedded = EmbeddedFacets::from_probe(&probed);
    let metadata_kind = local_metadata_kind(parsed_nfo.is_some(), &probed, &embedded, &[]);
    let sidecar_ratings = parsed_nfo
        .as_ref()
        .map(|nfo| normalize_content_ratings(&nfo.content_ratings))
        .unwrap_or_default();
    let content_ratings = preferred_facet_values(&sidecar_ratings, &embedded.content_ratings, &[]);
    let content_rating = (!content_ratings.is_empty()).then(|| content_ratings.join("/"));

    let mut tx = ersatzrs_infra::db::begin_write(pool).await?;
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
        content_rating.as_deref(),
        metadata_kind,
        &now,
    )
    .await?;

    let genres = preferred_facet_values(
        &parsed_nfo
            .as_ref()
            .map(|n| n.genres.clone())
            .unwrap_or_default(),
        &embedded.genres,
        &[],
    );
    let tags = preferred_facet_values(
        &parsed_nfo
            .as_ref()
            .map(|n| n.tags.clone())
            .unwrap_or_default(),
        &embedded.tags,
        &[],
    );
    let studios = preferred_facet_values(
        &parsed_nfo
            .as_ref()
            .map(|n| n.studios.clone())
            .unwrap_or_default(),
        &embedded.studios,
        &[],
    );
    let music_labels = preferred_facet_values(
        &parsed_nfo
            .as_ref()
            .map(|n| n.music_labels.clone())
            .unwrap_or_default(),
        &embedded.music_labels,
        &[],
    );
    let explicit_languages = preferred_facet_values(
        &parsed_nfo
            .as_ref()
            .map(|n| normalize_language_codes(&n.languages))
            .unwrap_or_default(),
        &embedded.languages,
        &[],
    );
    let languages = union_facet_values(&explicit_languages, &audio_stream_languages(&probed));
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
    replace_strings(
        &mut tx,
        "MusicLabel",
        "MusicVideoMetadataId",
        metadata_id,
        &music_labels,
    )
    .await?;
    replace_metadata_languages(&mut tx, "MusicVideoMetadataId", metadata_id, &languages).await?;
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
    content_rating: Option<&str>,
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
                   "ReleaseDate" = ?, "Track" = ?, "ContentRating" = ?
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
        .bind(content_rating)
        .bind(id)
        .execute(&mut **tx)
        .await?;
        Ok(id)
    } else {
        let result = sqlx::query(
            r#"INSERT INTO "MusicVideoMetadata" (
                "MusicVideoId", "DateAdded", "DateUpdated", "MetadataKind",
                "Title", "SortTitle", "Album", "Plot", "Year", "ReleaseDate", "Track",
                "ContentRating"
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
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
        .bind(content_rating)
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

fn remote_stream_definition_with_nfo(
    path: &Path,
    definition: &ersatzrs_infra::remote_stream::RemoteStreamDefinition,
) -> ersatzrs_infra::remote_stream::RemoteStreamDefinition {
    let mut effective = definition.clone();
    let nfo_path = path.with_extension("nfo");
    if !nfo_path.is_file() {
        return effective;
    }
    let parsed = match nfo::parse_other_video_nfo(&nfo_path) {
        Ok(parsed) => parsed,
        Err(error) => {
            warn!(path = %nfo_path.display(), error = %error, "failed to parse Remote Stream NFO; retaining definition metadata");
            return effective;
        }
    };
    effective.title = parsed.title.or(effective.title);
    effective.original_title = parsed.original_title.or(effective.original_title);
    effective.sort_title = parsed.sort_title.or(effective.sort_title);
    effective.outline = parsed.outline.or(effective.outline);
    effective.plot = parsed.plot.or(effective.plot);
    effective.tagline = parsed.tagline.or(effective.tagline);
    effective.show_title = parsed.show_title.or(effective.show_title);
    effective.season = parsed.season.or(effective.season);
    effective.episode = parsed.episode.or(effective.episode);
    effective.year = parsed.year.or(effective.year);
    effective.release_date = parsed
        .release_datetime
        .or(parsed.premiered)
        .map(|value| value.to_rfc3339())
        .or(effective.release_date);
    effective.content_rating = parsed
        .content_ratings
        .first()
        .cloned()
        .or(effective.content_rating);
    if !parsed.genres.is_empty() {
        effective.genres = parsed.genres;
    }
    if !parsed.tags.is_empty() {
        effective.tags = parsed.tags;
    }
    if !parsed.studios.is_empty() {
        effective.studios = parsed.studios;
    }
    effective.collection = parsed.collection.or(effective.collection);
    if !parsed.producers.is_empty() {
        effective.producers = parsed.producers;
    }
    if !parsed.original_broadcasters.is_empty() {
        effective.original_broadcasters = parsed.original_broadcasters;
    }
    if !parsed.broadcasters.is_empty() {
        effective.broadcasters = parsed.broadcasters;
    }
    if !parsed.languages.is_empty() {
        effective.languages = parsed.languages;
    }
    let mut people = parsed
        .actors
        .into_iter()
        .filter_map(|actor| {
            actor
                .name
                .map(|name| ersatzrs_infra::remote_stream::RemoteStreamPerson {
                    name,
                    role: actor.role.unwrap_or_else(|| "actor".to_owned()),
                })
        })
        .collect::<Vec<_>>();
    people.extend(parsed.writers.into_iter().map(|name| {
        ersatzrs_infra::remote_stream::RemoteStreamPerson {
            name,
            role: "writer".to_owned(),
        }
    }));
    people.extend(parsed.directors.into_iter().map(|name| {
        ersatzrs_infra::remote_stream::RemoteStreamPerson {
            name,
            role: "director".to_owned(),
        }
    }));
    people.extend(parsed.artists.into_iter().map(|name| {
        ersatzrs_infra::remote_stream::RemoteStreamPerson {
            name,
            role: "artist".to_owned(),
        }
    }));
    if !people.is_empty() {
        effective.people = people;
    }
    let guids = parsed
        .guids
        .iter()
        .filter_map(nfo::NfoGuid::as_metadata_guid)
        .collect::<Vec<_>>();
    if !guids.is_empty() {
        effective.guids = guids;
    }
    effective
}

#[allow(clippy::too_many_lines)]
async fn upsert_local_remote_stream(
    pool: &DbPool,
    library_path: &LocalLibraryPath,
    path: &Path,
    definition: &ersatzrs_infra::remote_stream::RemoteStreamDefinition,
    resolved: &ersatzrs_infra::remote_stream::ResolvedRemoteStream,
    probed: Option<&FfprobeMediaVersion>,
) -> Result<i64> {
    let definition = remote_stream_definition_with_nfo(path, definition);
    let definition = &definition;
    let mut tx = ersatzrs_infra::db::begin_write(pool).await?;
    let path_text = path.display().to_string();
    let path_hash = path_hash(&path_text);

    let maybe_existing = if let (Some(addon_id), Some(provider_id)) = (
        definition.addon.as_deref(),
        definition.provider_id.as_deref(),
    ) {
        let exact = sqlx::query(
            r#"SELECT "mv"."Id" AS "MediaVersionId",
                      "rs"."Id" AS "MediaItemId",
                      "rsm"."Id" AS "MetadataId"
               FROM "RemoteStream" AS "rs"
               JOIN "RemoteStreamMetadata" AS "rsm"
                 ON "rsm"."RemoteStreamId" = "rs"."Id"
               JOIN "MediaVersion" AS "mv" ON "mv"."RemoteStreamId" = "rs"."Id"
               WHERE "rs"."AddonId" = ? AND "rs"."ProviderId" = ?
               ORDER BY "mv"."Id" LIMIT 1"#,
        )
        .bind(addon_id)
        .bind(provider_id)
        .fetch_optional(&mut *tx)
        .await?;
        match exact {
            Some(row) => Some(row),
            None => {
                sqlx::query(
                    r#"SELECT "mv"."Id" AS "MediaVersionId",
                              "rs"."Id" AS "MediaItemId",
                              "rsm"."Id" AS "MetadataId"
                       FROM "RemoteStream" AS "rs"
                       JOIN "RemoteStreamMetadata" AS "rsm"
                         ON "rsm"."RemoteStreamId" = "rs"."Id"
                       JOIN "MediaVersion" AS "mv" ON "mv"."RemoteStreamId" = "rs"."Id"
                       WHERE "rs"."AddonId" = ?
                         AND LOWER("rs"."ProviderId") = LOWER(?)
                       ORDER BY "mv"."Id" LIMIT 1"#,
                )
                .bind(addon_id)
                .bind(provider_id)
                .fetch_optional(&mut *tx)
                .await?
            }
        }
    } else {
        None
    };
    let maybe_existing = match maybe_existing {
        Some(row) => Some(row),
        None => {
            sqlx::query(
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
            .await?
        }
    };

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
    let original_title = definition.original_title.clone().unwrap_or_else(|| {
        path.strip_prefix(&library_path.path)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/")
    });
    let sort_title = definition.sort_title.as_deref().unwrap_or(&title);
    let now = Utc::now();

    let (item_id, metadata_id, version_id, existed) = if let Some(row) = maybe_existing {
        let item_id: i64 = row.get::<Option<i64>, _>("MediaItemId").unwrap_or_default();
        let metadata_id: i64 = row.get("MetadataId");
        let version_id: i64 = row.get("MediaVersionId");
        sqlx::query(
            r#"UPDATE "RemoteStreamMetadata"
               SET "DateUpdated" = ?, "MetadataKind" = 1, "Title" = ?,
                   "OriginalTitle" = ?, "SortTitle" = ?, "Year" = ?,
                   "ReleaseDate" = ?, "ContentRating" = ?, "Plot" = ?,
                   "Outline" = ?, "Tagline" = ?, "ShowTitle" = ?,
                   "SeasonNumber" = ?, "EpisodeNumber" = ?,
                   "CollectionName" = ?, "OriginalBroadcasters" = ?,
                   "Broadcasters" = ?
               WHERE "Id" = ?"#,
        )
        .bind(encode_datetime(now))
        .bind(&title)
        .bind(&original_title)
        .bind(sort_title)
        .bind(definition.year)
        .bind(definition.release_date.as_deref())
        .bind(definition.content_rating.as_deref())
        .bind(definition.plot.as_deref())
        .bind(definition.outline.as_deref())
        .bind(definition.tagline.as_deref())
        .bind(definition.show_title.as_deref())
        .bind(definition.season)
        .bind(definition.episode)
        .bind(definition.collection.as_deref())
        .bind(encode_nonempty_string_list(
            &definition.original_broadcasters,
        ))
        .bind(encode_nonempty_string_list(&definition.broadcasters))
        .bind(metadata_id)
        .execute(&mut *tx)
        .await?;
        let updated_file = sqlx::query(
            r#"UPDATE "MediaFile" SET "Path" = ?, "PathHash" = ?
               WHERE "MediaVersionId" = ?"#,
        )
        .bind(&path_text)
        .bind(&path_hash)
        .bind(version_id)
        .execute(&mut *tx)
        .await?;
        if updated_file.rows_affected() == 0 {
            sqlx::query(
                r#"INSERT INTO "MediaFile" ("MediaVersionId", "Path", "PathHash")
                   VALUES (?, ?, ?)"#,
            )
            .bind(version_id)
            .bind(&path_text)
            .bind(&path_hash)
            .execute(&mut *tx)
            .await?;
        }
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
                "Title", "OriginalTitle", "SortTitle", "Year", "ReleaseDate",
                "ContentRating", "Plot", "CollectionName", "OriginalBroadcasters",
                "Broadcasters", "Outline", "Tagline", "ShowTitle", "SeasonNumber",
                "EpisodeNumber"
            ) VALUES (?, ?, ?, 1, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(item_id)
        .bind(encode_datetime(now))
        .bind(encode_datetime(now))
        .bind(&title)
        .bind(&original_title)
        .bind(sort_title)
        .bind(definition.year)
        .bind(definition.release_date.as_deref())
        .bind(definition.content_rating.as_deref())
        .bind(definition.plot.as_deref())
        .bind(definition.collection.as_deref())
        .bind(encode_nonempty_string_list(
            &definition.original_broadcasters,
        ))
        .bind(encode_nonempty_string_list(&definition.broadcasters))
        .bind(definition.outline.as_deref())
        .bind(definition.tagline.as_deref())
        .bind(definition.show_title.as_deref())
        .bind(definition.season)
        .bind(definition.episode)
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
               SET "Duration" = ?, "FallbackQuery" = ?, "AddonId" = ?,
               "Script" = NULL, "Url" = ?, "Liveness" = ?, "IsLive" = ?,
               "Availability" = ?, "AvailabilityReason" = ?, "ContentKind" = ?,
               "ProviderId" = ?
           WHERE "Id" = ?"#,
    )
    .bind(definition.duration.as_deref())
    .bind(definition.fallback_query.as_deref())
    .bind(definition.addon.as_deref())
    .bind(definition.url.as_deref())
    .bind(match resolved.liveness {
        ersatzrs_domain::RemoteStreamLiveness::Unknown => 0,
        ersatzrs_domain::RemoteStreamLiveness::Finite => 1,
        ersatzrs_domain::RemoteStreamLiveness::Live => 2,
    })
    .bind(i32::from(resolved.is_live))
    .bind(match definition.availability {
        ersatzrs_domain::AddonMediaListItemAvailability::Available => 0,
        ersatzrs_domain::AddonMediaListItemAvailability::Unavailable => 1,
        ersatzrs_domain::AddonMediaListItemAvailability::Unknown => 2,
    })
    .bind(definition.availability_reason.as_deref())
    .bind(match definition.content_kind {
        ersatzrs_domain::AddonMediaListContentKind::Auto => 0,
        ersatzrs_domain::AddonMediaListContentKind::TelevisionEpisode => 1,
        ersatzrs_domain::AddonMediaListContentKind::Movie => 2,
        ersatzrs_domain::AddonMediaListContentKind::MusicVideo => 3,
        ersatzrs_domain::AddonMediaListContentKind::OtherVideo => 4,
        ersatzrs_domain::AddonMediaListContentKind::Song => 5,
        ersatzrs_domain::AddonMediaListContentKind::Image => 6,
    })
    .bind(definition.provider_id.as_deref())
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
           SET "LibraryPathId" = ?, "State" = ?, "ContentKind" = ?,
               "SubKind" = NULL, "ParentMediaItemId" = NULL
           WHERE "Id" = ?"#,
    )
    .bind(library_path.id)
    .bind(
        if definition.availability == ersatzrs_domain::AddonMediaListItemAvailability::Unavailable {
            2
        } else {
            0
        },
    )
    .bind(ContentKind::Primary.db_value())
    .bind(item_id)
    .execute(&mut *tx)
    .await?;

    let mut definition_tags = definition.tags.clone();
    if let Some(collection) = definition.collection.as_deref() {
        definition_tags.push(collection.to_owned());
    }
    let tags = union_facet_values(
        &remote_stream_folder_tags(Path::new(&library_path.path), path),
        &definition_tags,
    );
    replace_strings(
        &mut tx,
        "Genre",
        "RemoteStreamMetadataId",
        metadata_id,
        &definition.genres,
    )
    .await?;
    replace_strings(&mut tx, "Tag", "RemoteStreamMetadataId", metadata_id, &tags).await?;
    let studios = union_facet_values(
        &union_facet_values(&definition.broadcasters, &definition.original_broadcasters),
        &union_facet_values(&definition.producers, &definition.studios),
    );
    replace_strings(
        &mut tx,
        "Studio",
        "RemoteStreamMetadataId",
        metadata_id,
        &studios,
    )
    .await?;
    let mut people = definition
        .people
        .iter()
        .enumerate()
        .map(|(index, person)| {
            PersonRef::with_role(
                person.name.clone(),
                Some(person.role.clone()),
                i32::try_from(index).ok(),
            )
        })
        .collect::<Vec<_>>();
    people.extend(
        definition
            .producers
            .iter()
            .enumerate()
            .map(|(index, producer)| {
                PersonRef::with_role(
                    producer.clone(),
                    Some("producer".to_owned()),
                    i32::try_from(definition.people.len() + index).ok(),
                )
            }),
    );
    replace_people(
        &mut tx,
        "Actor",
        "RemoteStreamMetadataId",
        metadata_id,
        &people,
    )
    .await?;
    replace_guids(
        &mut tx,
        "RemoteStreamMetadataId",
        metadata_id,
        &definition.guids,
    )
    .await?;
    replace_metadata_languages(
        &mut tx,
        "RemoteStreamMetadataId",
        metadata_id,
        &definition.languages,
    )
    .await?;
    let mut artwork = locate_remote_stream_artwork(path)
        .map(|path| vec![artwork_ref(&path, artwork_kind::THUMBNAIL)])
        .unwrap_or_default();
    artwork.extend(
        definition
            .artwork_urls
            .iter()
            .enumerate()
            .map(|(index, url)| ArtworkRef {
                kind: if index == 0 {
                    artwork_kind::THUMBNAIL
                } else {
                    artwork_kind::FAN_ART
                },
                path: url.clone(),
                date_added: Utc::now(),
            }),
    );
    replace_artwork(
        &mut tx,
        "RemoteStreamMetadataId",
        metadata_id,
        &[artwork_kind::THUMBNAIL, artwork_kind::FAN_ART],
        &artwork,
    )
    .await?;

    tx.commit().await?;
    Ok(item_id)
}

fn encode_nonempty_string_list(values: &[String]) -> Option<String> {
    (!values.is_empty())
        .then(|| serde_json::to_string(values).expect("serializing a string list cannot fail"))
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

async fn load_remote_stream_definition(
    pool: &DbPool,
    path: &Path,
    ffprobe_path: &str,
) -> Result<(
    ersatzrs_infra::remote_stream::RemoteStreamDefinition,
    ersatzrs_infra::remote_stream::ResolvedRemoteStream,
)> {
    let yaml = std::fs::read_to_string(path).map_err(|source| ScannerError::RemoteStreamRead {
        path: path.display().to_string(),
        source,
    })?;
    let definition = parse_remote_stream_definition(&yaml)?;
    let resolved = resolve_remote_stream_for_scan_with_addons(
        pool,
        &definition,
        &AddonRuntimeContext {
            ffmpeg_path: Some(paired_ffmpeg_path(ffprobe_path)),
            ffprobe_path: Some(ffprobe_path.to_owned()),
        },
    )
    .await?;
    Ok((definition, resolved))
}

async fn synchronize_remote_stream_playlist(
    pool: &DbPool,
    library_id: i64,
    library_path: &LocalLibraryPath,
    path: &Path,
    ffprobe_path: &str,
) -> Result<Vec<ScanPlayoutRefresh>> {
    synchronize_remote_stream_playlist_to_boundary(
        pool,
        library_id,
        library_path,
        path,
        ffprobe_path,
        PlaylistSyncBoundary::Published,
    )
    .await
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PlaylistSyncBoundary {
    Published,
    #[cfg(test)]
    DatabaseCommitted,
}

#[allow(clippy::too_many_lines)]
async fn synchronize_remote_stream_playlist_to_boundary(
    pool: &DbPool,
    library_id: i64,
    library_path: &LocalLibraryPath,
    path: &Path,
    ffprobe_path: &str,
    boundary: PlaylistSyncBoundary,
) -> Result<Vec<ScanPlayoutRefresh>> {
    let mut reason = ersatzrs_domain::ScanPlayoutRefreshReason::RemoteStreamMembershipMixed;
    let resolution_generation =
        ersatzrs_infra::scan_refresh::begin_resolution(pool, library_id, reason).await?;

    let result = async {
        let yaml =
            std::fs::read(path).map_err(|source| ScannerError::RemoteStreamPlaylistRead {
                path: path.display().to_string(),
                source,
            })?;
        let manifest_text = std::str::from_utf8(&yaml).map_err(|error| {
            remote_stream_playlist::PlaylistError::Manifest(format!(
                "manifest is not UTF-8: {error}"
            ))
        })?;
        let manifest = remote_stream_playlist::parse_playlist_manifest(manifest_text, path)?;
        let runtime = AddonRuntimeContext {
            ffmpeg_path: Some(paired_ffmpeg_path(ffprobe_path)),
            ffprobe_path: Some(ffprobe_path.to_owned()),
        };
        let sync =
            remote_stream_playlist::stage_playlist_files(pool, path, &yaml, &manifest, &runtime)
                .await?;
        let relative = path
            .strip_prefix(&library_path.path)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        let managed_source = format!("remote-stream-playlist:v1:{library_id}:{relative}");
        reason = membership_refresh_reason(&sync.membership_delta, false);
        let managed_playlist_id: Option<i64> =
            sqlx::query_scalar(r#"SELECT "Id" FROM "Playlist" WHERE "ManagedSource" = ?"#)
                .bind(&managed_source)
                .fetch_optional(pool)
                .await?;
        let existing_order = match managed_playlist_id {
            Some(playlist_id) => managed_playlist_media_item_ids(pool, playlist_id).await?,
            None => Vec::new(),
        };
        let playlist_existence_changed =
            manifest.manifest.create_playlist != managed_playlist_id.is_some();
        let initial_change = sync.membership_changed || playlist_existence_changed;
        if initial_change && !sync.membership_changed {
            reason = ersatzrs_domain::ScanPlayoutRefreshReason::RemoteStreamMembershipMixed;
        }

        let resolver =
            ersatzrs_infra::repositories::SqliteSchedulingContentRepository::new(pool.clone());
        let mut affected = std::collections::BTreeSet::new();
        let mut old_media_by_provider = std::collections::BTreeMap::new();
        if initial_change {
            let old_provider_ids = sync
                .membership_delta
                .removed_provider_ids
                .iter()
                .chain(&sync.membership_delta.changed_provider_ids)
                .cloned()
                .collect::<std::collections::BTreeSet<_>>();
            old_media_by_provider = remote_stream_media_ids_by_provider_id(
                pool,
                library_id,
                &manifest.manifest.addon,
                &old_provider_ids,
            )
            .await?;
            let old_media_ids = old_media_by_provider.values().copied().collect();
            affected.extend(
                resolver
                    .classic_playouts_affected_by_scan(&old_media_ids, managed_playlist_id)
                    .await?,
            );
        }

        let mut ordered_items = Vec::new();
        let mut materialized = Vec::new();
        for definition in &sync.definitions {
            let (parsed, source) =
                load_remote_stream_definition(pool, &definition.path, ffprobe_path).await?;
            let item_id = upsert_local_remote_stream(
                pool,
                library_path,
                &definition.path,
                &parsed,
                &source,
                None,
            )
            .await?;
            let liveness = parsed.effective_liveness()?;
            if definition.active
                && (liveness == ersatzrs_domain::RemoteStreamLiveness::Live
                    || (liveness == ersatzrs_domain::RemoteStreamLiveness::Finite
                        && definition.duration_seconds.is_some_and(|value| value > 0)))
            {
                ordered_items.push(item_id);
            }
            materialized.push((item_id, parsed));
        }
        link_materialized_remote_stream_fragments(pool, &materialized).await?;

        let sql_order_changed = if manifest.manifest.create_playlist {
            existing_order != ordered_items
        } else {
            managed_playlist_id.is_some()
        };
        let effective_change = initial_change || sql_order_changed;
        if effective_change && !sync.membership_changed {
            reason = ersatzrs_domain::ScanPlayoutRefreshReason::RemoteStreamMembershipMixed;
        }

        if effective_change {
            let new_provider_ids = sync
                .membership_delta
                .added_provider_ids
                .iter()
                .chain(&sync.membership_delta.changed_provider_ids)
                .cloned()
                .collect::<std::collections::BTreeSet<_>>();
            let new_media_by_provider = remote_stream_media_ids_by_provider_id(
                pool,
                library_id,
                &manifest.manifest.addon,
                &new_provider_ids,
            )
            .await?;
            let reidentified = old_media_by_provider
                .iter()
                .any(|(provider_id, old_media_id)| {
                    new_media_by_provider
                        .get(provider_id)
                        .is_some_and(|new_media_id| new_media_id != old_media_id)
                });
            reason = if sync.membership_changed {
                membership_refresh_reason(&sync.membership_delta, reidentified)
            } else {
                ersatzrs_domain::ScanPlayoutRefreshReason::RemoteStreamMembershipMixed
            };
            let new_media_ids = new_media_by_provider
                .values()
                .copied()
                .collect::<std::collections::BTreeSet<_>>();
            // Reordering changes only the exact managed playlist. Feeding all
            // of its stable members back through collection/search resolution
            // would refresh unrelated sources that merely share one item.
            affected.extend(
                resolver
                    .classic_playouts_affected_by_scan(
                        &new_media_ids,
                        sql_order_changed.then_some(managed_playlist_id).flatten(),
                    )
                    .await?,
            );
        }

        let affected = affected.into_iter().collect::<Vec<_>>();
        synchronize_managed_playlist(
            pool,
            &managed_source,
            &manifest.name,
            manifest.manifest.create_playlist,
            &ordered_items,
            effective_change,
            Some((library_id, reason, affected.as_slice())),
        )
        .await?;
        let refreshes = affected
            .into_iter()
            .map(|playout_id| ScanPlayoutRefresh { playout_id, reason })
            .collect::<Vec<_>>();
        #[cfg(test)]
        if boundary == PlaylistSyncBoundary::DatabaseCommitted {
            return Ok(refreshes);
        }
        #[cfg(not(test))]
        let _ = boundary;
        sync.publish_sidecar()?;
        complete_scan_refresh_resolution(pool, library_id, resolution_generation, reason).await?;
        Ok::<_, ScannerError>(refreshes)
    }
    .await;

    match result {
        Ok(refreshes) => Ok(refreshes),
        Err(error) => {
            if let Err(state_error) = ersatzrs_infra::scan_refresh::fail_resolution(
                pool,
                library_id,
                resolution_generation,
                &error.to_string(),
            )
            .await
            {
                warn!(
                    library_id,
                    error = %state_error,
                    "failed to retain scan refresh resolution failure"
                );
            }
            Err(error)
        }
    }
}

fn membership_refresh_reason(
    delta: &remote_stream_playlist::PlaylistMembershipDelta,
    reidentified: bool,
) -> ersatzrs_domain::ScanPlayoutRefreshReason {
    use remote_stream_playlist::PlaylistMembershipChangeReason;

    let reason = match delta.reason() {
        PlaylistMembershipChangeReason::Added => {
            ersatzrs_domain::ScanPlayoutRefreshReason::RemoteStreamMembershipAdded
        }
        PlaylistMembershipChangeReason::Removed => {
            ersatzrs_domain::ScanPlayoutRefreshReason::RemoteStreamMembershipRemoved
        }
        PlaylistMembershipChangeReason::ChangedOrReplaced => {
            ersatzrs_domain::ScanPlayoutRefreshReason::RemoteStreamMembershipReplaced
        }
        PlaylistMembershipChangeReason::Reordered => {
            ersatzrs_domain::ScanPlayoutRefreshReason::RemoteStreamMembershipReordered
        }
        PlaylistMembershipChangeReason::Unchanged | PlaylistMembershipChangeReason::Mixed => {
            ersatzrs_domain::ScanPlayoutRefreshReason::RemoteStreamMembershipMixed
        }
    };
    if reidentified
        && reason == ersatzrs_domain::ScanPlayoutRefreshReason::RemoteStreamMembershipReplaced
    {
        ersatzrs_domain::ScanPlayoutRefreshReason::RemoteStreamMembershipReidentified
    } else if reidentified {
        reason.merge(ersatzrs_domain::ScanPlayoutRefreshReason::RemoteStreamMembershipReidentified)
    } else {
        reason
    }
}

async fn remote_stream_media_ids_by_provider_id(
    pool: &DbPool,
    library_id: i64,
    addon_id: &str,
    provider_ids: &std::collections::BTreeSet<String>,
) -> Result<std::collections::BTreeMap<String, i64>> {
    if provider_ids.is_empty() {
        return Ok(std::collections::BTreeMap::new());
    }
    let rows = sqlx::query(
        r#"SELECT "rs"."Id", "rs"."ProviderId"
           FROM "RemoteStream" AS "rs"
           JOIN "MediaItem" AS "mi" ON "mi"."Id" = "rs"."Id"
           JOIN "LibraryPath" AS "lp" ON "lp"."Id" = "mi"."LibraryPathId"
           WHERE "lp"."LibraryId" = ? AND "rs"."AddonId" = ?
             AND "rs"."ProviderId" IS NOT NULL
           ORDER BY "rs"."Id""#,
    )
    .bind(library_id)
    .bind(addon_id)
    .fetch_all(pool)
    .await?;
    let mut result = std::collections::BTreeMap::new();
    for row in rows {
        let provider_id = row.get::<String, _>("ProviderId");
        if provider_ids
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case(&provider_id))
        {
            result
                .entry(provider_id.to_ascii_lowercase())
                .or_insert_with(|| row.get::<i64, _>("Id"));
        }
    }
    Ok(result)
}

async fn managed_playlist_media_item_ids(pool: &DbPool, playlist_id: i64) -> Result<Vec<i64>> {
    Ok(sqlx::query_scalar(
        r#"SELECT "MediaItemId" FROM "PlaylistItem"
           WHERE "PlaylistId" = ? AND "MediaItemId" IS NOT NULL
           ORDER BY "Index", "Id""#,
    )
    .bind(playlist_id)
    .fetch_all(pool)
    .await?)
}

async fn link_materialized_remote_stream_fragments(
    pool: &DbPool,
    materialized: &[(i64, ersatzrs_infra::remote_stream::RemoteStreamDefinition)],
) -> Result<()> {
    let identities = materialized
        .iter()
        .filter_map(|(item_id, definition)| {
            Some((
                (
                    definition.addon.as_ref()?.clone(),
                    definition.provider_id.as_ref()?.clone(),
                ),
                *item_id,
            ))
        })
        .collect::<BTreeMap<_, _>>();
    let mut tx = ersatzrs_infra::db::begin_write(pool).await?;
    for (item_id, definition) in materialized {
        let Some(parent_provider_id) = definition.parent_provider_id.as_deref() else {
            continue;
        };
        let addon_id = definition.addon.as_deref().ok_or_else(|| {
            remote_stream_playlist::PlaylistError::Enumeration(format!(
                "chapter fragment '{}' has no add-on identity",
                definition.provider_id.as_deref().unwrap_or("unknown")
            ))
        })?;
        let parent_media_item_id = identities
            .get(&(addon_id.to_owned(), parent_provider_id.to_owned()))
            .copied()
            .ok_or_else(|| {
                remote_stream_playlist::PlaylistError::Enumeration(format!(
                    "chapter fragment '{}' references full episode '{}' which was not materialized",
                    definition.provider_id.as_deref().unwrap_or("unknown"),
                    parent_provider_id
                ))
            })?;
        sqlx::query(
            r#"UPDATE "MediaItem"
               SET "SubKind" = 'chapter', "ParentMediaItemId" = ?
               WHERE "Id" = ?"#,
        )
        .bind(parent_media_item_id)
        .bind(item_id)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

fn is_managed_remote_stream_definition(path: &Path) -> bool {
    const HEADER: &str = "# Generated by ErsatzRS from a Remote Stream playlist manifest.";
    std::fs::read_to_string(path)
        .ok()
        .is_some_and(|value| value.starts_with(HEADER))
}

async fn synchronize_managed_playlist(
    pool: &DbPool,
    managed_source: &str,
    name: &str,
    enabled: bool,
    ordered_items: &[i64],
    membership_changed: bool,
    refresh_intent: Option<(i64, ersatzrs_domain::ScanPlayoutRefreshReason, &[i64])>,
) -> Result<()> {
    let mut tx = ersatzrs_infra::db::begin_write(pool).await?;
    let existing: Option<i64> =
        sqlx::query_scalar(r#"SELECT "Id" FROM "Playlist" WHERE "ManagedSource" = ?"#)
            .bind(managed_source)
            .fetch_optional(&mut *tx)
            .await?;
    if !enabled {
        if let Some(id) = existing {
            sqlx::query(r#"DELETE FROM "Playlist" WHERE "Id" = ? AND "ManagedSource" = ?"#)
                .bind(id)
                .bind(managed_source)
                .execute(&mut *tx)
                .await?;
            persist_scan_refresh_intents(&mut tx, refresh_intent).await?;
            tx.commit().await?;
            return Ok(());
        }
        persist_scan_refresh_intents(&mut tx, refresh_intent).await?;
        tx.commit().await?;
        return Ok(());
    }
    let group_id: Option<i64> = sqlx::query_scalar(
        r#"SELECT "Id" FROM "PlaylistGroup" WHERE "IsSystem" = 1 ORDER BY "Id" LIMIT 1"#,
    )
    .fetch_optional(&mut *tx)
    .await?;
    let group_id = if let Some(id) = group_id {
        id
    } else {
        let result = sqlx::query(
            r#"INSERT INTO "PlaylistGroup" ("Name", "IsSystem")
               VALUES ('System', 1)"#,
        )
        .execute(&mut *tx)
        .await?;
        ersatzrs_infra::db::last_insert_id(result)
    };
    let playlist_id = if let Some(id) = existing {
        sqlx::query(
            r#"UPDATE "Playlist"
               SET "Name" = ?, "IsSystem" = 1, "PlaylistGroupId" = ?
               WHERE "Id" = ? AND "ManagedSource" = ?"#,
        )
        .bind(name)
        .bind(group_id)
        .bind(id)
        .bind(managed_source)
        .execute(&mut *tx)
        .await?;
        id
    } else {
        let result = sqlx::query(
            r#"INSERT INTO "Playlist" (
                "IsSystem", "ManagedSource", "Name", "PlaylistGroupId"
               ) VALUES (1, ?, ?, ?)"#,
        )
        .bind(managed_source)
        .bind(name)
        .bind(group_id)
        .execute(&mut *tx)
        .await?;
        ersatzrs_infra::db::last_insert_id(result)
    };
    let persisted_order = managed_playlist_media_item_ids_in_tx(&mut tx, playlist_id).await?;
    let replace_items =
        existing.is_none() || membership_changed || persisted_order != ordered_items;
    if replace_items {
        sqlx::query(r#"DELETE FROM "PlaylistItem" WHERE "PlaylistId" = ?"#)
            .bind(playlist_id)
            .execute(&mut *tx)
            .await?;
        for (index, media_item_id) in ordered_items.iter().enumerate() {
            sqlx::query(
                r#"INSERT INTO "PlaylistItem" (
                    "Index", "PlaylistId", "CollectionType", "CollectionId",
                    "MediaItemId", "MultiCollectionId", "SmartCollectionId",
                    "IncludeInProgramGuide", "PlaybackOrder", "PlayAll"
                   ) VALUES (?, ?, 70, NULL, ?, NULL, NULL, 1, 1, 0)"#,
            )
            .bind(i32::try_from(index).unwrap_or(i32::MAX))
            .bind(playlist_id)
            .bind(media_item_id)
            .execute(&mut *tx)
            .await?;
        }
    }
    persist_scan_refresh_intents(&mut tx, refresh_intent).await?;
    tx.commit().await?;
    Ok(())
}

async fn managed_playlist_media_item_ids_in_tx(
    tx: &mut sqlx::Transaction<'_, ersatzrs_infra::db::Db>,
    playlist_id: i64,
) -> Result<Vec<i64>> {
    Ok(sqlx::query_scalar(
        r#"SELECT "MediaItemId" FROM "PlaylistItem"
           WHERE "PlaylistId" = ? AND "MediaItemId" IS NOT NULL
           ORDER BY "Index", "Id""#,
    )
    .bind(playlist_id)
    .fetch_all(&mut **tx)
    .await?)
}

async fn persist_scan_refresh_intents(
    tx: &mut sqlx::Transaction<'_, ersatzrs_infra::db::Db>,
    refresh_intent: Option<(i64, ersatzrs_domain::ScanPlayoutRefreshReason, &[i64])>,
) -> Result<()> {
    if let Some((library_id, reason, playout_ids)) = refresh_intent {
        ersatzrs_infra::scan_refresh::persist_refresh_intents(tx, library_id, reason, playout_ids)
            .await?;
    }
    Ok(())
}

async fn complete_scan_refresh_resolution(
    pool: &DbPool,
    library_id: i64,
    generation: i64,
    reason: ersatzrs_domain::ScanPlayoutRefreshReason,
) -> Result<()> {
    let mut tx = ersatzrs_infra::db::begin_write(pool).await?;
    ersatzrs_infra::scan_refresh::complete_resolution_in_tx(
        &mut tx, library_id, generation, reason,
    )
    .await?;
    tx.commit().await?;
    Ok(())
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
    let mut tx = ersatzrs_infra::db::begin_write(pool).await?;
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
    match library_path.media_kind {
        LibraryMediaKind::Movies => {
            enrich_movie_metadata(&mut tx, item_id, library_path, path, &probed).await?;
        }
        LibraryMediaKind::OtherVideos => {
            enrich_other_video_metadata(&mut tx, item_id, library_path, path, &probed).await?;
        }
        LibraryMediaKind::Songs => {
            enrich_song_metadata(&mut tx, item_id, library_path, path, &probed).await?;
        }
        LibraryMediaKind::Images => {
            enrich_image_metadata(&mut tx, item_id, library_path, path, &probed).await?;
        }
        LibraryMediaKind::Shows
        | LibraryMediaKind::MusicVideos
        | LibraryMediaKind::RemoteStreams => {}
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
    let mut tx = ersatzrs_infra::db::begin_write(pool).await?;
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
    enrich_other_video_metadata(&mut tx, item_id, library_path, path, &probed).await?;

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

/// Movie sidecar candidates, most specific first.
///
/// Both Kodi names are returned when both exist. A folder frequently holds the
/// scene-release `.nfo` under the video's basename *and* the metadata sidecar
/// Jellyfin writes as `movie.nfo`; returning only the first hid the real
/// metadata behind release art.
fn locate_movie_nfos(video_path: &Path) -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    let file_nfo = video_path.with_extension("nfo");
    if file_nfo.is_file() {
        candidates.push(file_nfo);
    }

    if let Some(movie_nfo) = video_path.parent().map(|parent| parent.join("movie.nfo")) {
        if movie_nfo.is_file() && !candidates.contains(&movie_nfo) {
            candidates.push(movie_nfo);
        }
    }

    candidates
}

/// Parse the first movie sidecar that carries a `<movie>` root.
///
/// A candidate without that root is not a metadata sidecar — release art and
/// forum-markup dumps share the extension — so it is skipped quietly. Only a
/// sidecar that claims to be metadata and then fails to parse is worth warning.
fn load_movie_nfo(video_path: &Path) -> Option<nfo::MovieNfo> {
    for candidate in locate_movie_nfos(video_path) {
        match nfo::parse_movie_nfo(&candidate) {
            Ok(parsed) => return Some(parsed),
            Err(nfo::NfoError::MissingRoot(_)) => {
                debug!(
                    path = %candidate.display(),
                    "sidecar carries no <movie> root; trying next candidate"
                );
            }
            Err(error) => {
                warn!(
                    path = %candidate.display(),
                    error = %error,
                    "failed to parse movie NFO; trying next candidate"
                );
            }
        }
    }

    None
}

fn locate_same_basename_nfo(path: &Path) -> Option<PathBuf> {
    let candidate = path.with_extension("nfo");
    candidate.is_file().then_some(candidate)
}

fn embedded_scalar(probed: &FfprobeMediaVersion, aliases: &[&str]) -> Option<String> {
    aliases.iter().find_map(|alias| {
        probed
            .format_metadata
            .get(*alias)
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
    })
}

fn local_folder_tags(root: &Path, path: &Path) -> Vec<String> {
    path.parent()
        .and_then(|parent| parent.strip_prefix(root).ok())
        .into_iter()
        .flat_map(Path::components)
        .filter_map(|component| {
            let value = component.as_os_str().to_str()?.trim();
            (!value.is_empty()).then(|| value.to_owned())
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
async fn replace_uniform_facets(
    tx: &mut Transaction<'_, Db>,
    fk_column: &'static str,
    metadata_id: i64,
    genres: &[String],
    tags: &[String],
    studios: &[String],
    music_labels: &[String],
    languages: &[String],
) -> Result<()> {
    replace_strings(tx, "Genre", fk_column, metadata_id, genres).await?;
    replace_strings(tx, "Tag", fk_column, metadata_id, tags).await?;
    replace_strings(tx, "Studio", fk_column, metadata_id, studios).await?;
    replace_strings(tx, "MusicLabel", fk_column, metadata_id, music_labels).await?;
    replace_metadata_languages(tx, fk_column, metadata_id, languages).await?;
    Ok(())
}

#[allow(clippy::too_many_lines)]
async fn enrich_other_video_metadata(
    tx: &mut Transaction<'_, Db>,
    item_id: i64,
    library_path: &LocalLibraryPath,
    path: &Path,
    probed: &FfprobeMediaVersion,
) -> Result<i64> {
    let parsed = locate_same_basename_nfo(path).and_then(|nfo_path| {
        match nfo::parse_other_video_nfo(&nfo_path) {
            Ok(parsed) => Some(parsed),
            Err(nfo::NfoError::MissingRoot(_)) => {
                debug!(path = %nfo_path.display(), "sidecar carries no metadata root; falling back");
                None
            }
            Err(error) => {
                warn!(path = %nfo_path.display(), error = %error, "failed to parse other-video nfo; falling back");
                None
            }
        }
    });
    let embedded = EmbeddedFacets::from_probe(probed);
    let fallback_title = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_default()
        .to_owned();
    let title = parsed
        .as_ref()
        .and_then(|nfo| nfo.title.clone())
        .or_else(|| embedded_scalar(probed, &["title"]))
        .unwrap_or(fallback_title);
    let sort_title = parsed
        .as_ref()
        .and_then(|nfo| nfo.sort_title.clone())
        .unwrap_or_else(|| nfo::sort_title(&title));
    let ratings = preferred_facet_values(
        &parsed
            .as_ref()
            .map(|nfo| normalize_content_ratings(&nfo.content_ratings))
            .unwrap_or_default(),
        &embedded.content_ratings,
        &[],
    );
    let genres = preferred_facet_values(
        &parsed
            .as_ref()
            .map(|nfo| nfo.genres.clone())
            .unwrap_or_default(),
        &embedded.genres,
        &[],
    );
    let tags = preferred_facet_values(
        &parsed
            .as_ref()
            .map(|nfo| nfo.tags.clone())
            .unwrap_or_default(),
        &embedded.tags,
        &[],
    );
    let studios = preferred_facet_values(
        &parsed
            .as_ref()
            .map(|nfo| nfo.studios.clone())
            .unwrap_or_default(),
        &embedded.studios,
        &[],
    );
    let music_labels = preferred_facet_values(
        &parsed
            .as_ref()
            .map(|nfo| nfo.music_labels.clone())
            .unwrap_or_default(),
        &embedded.music_labels,
        &[],
    );
    let explicit_languages = preferred_facet_values(
        &parsed
            .as_ref()
            .map(|nfo| normalize_language_codes(&nfo.languages))
            .unwrap_or_default(),
        &embedded.languages,
        &[],
    );
    let languages = union_facet_values(&explicit_languages, &audio_stream_languages(probed));
    let rating = (!ratings.is_empty()).then(|| ratings.join("/"));
    let now = encode_datetime(Utc::now());
    let metadata_id: i64 = sqlx::query_scalar(
        r#"SELECT "Id" FROM "OtherVideoMetadata" WHERE "OtherVideoId" = ? LIMIT 1"#,
    )
    .bind(item_id)
    .fetch_one(&mut **tx)
    .await?;
    sqlx::query(
        r#"UPDATE "OtherVideoMetadata"
           SET "DateUpdated" = ?, "MetadataKind" = ?, "Title" = ?,
               "SortTitle" = ?, "OriginalTitle" = ?, "Year" = ?,
               "ReleaseDate" = ?, "ContentRating" = ?, "Outline" = ?,
               "Plot" = ?, "Tagline" = ?
           WHERE "Id" = ?"#,
    )
    .bind(&now)
    .bind(local_metadata_kind(
        parsed.is_some(),
        probed,
        &embedded,
        &["title"],
    ))
    .bind(&title)
    .bind(&sort_title)
    .bind(original_title_for_path(library_path, path))
    .bind(parsed.as_ref().and_then(|nfo| nfo.year))
    .bind(
        parsed
            .as_ref()
            .and_then(|nfo| nfo.premiered)
            .map(encode_datetime),
    )
    .bind(rating)
    .bind(parsed.as_ref().and_then(|nfo| nfo.outline.clone()))
    .bind(parsed.as_ref().and_then(|nfo| nfo.plot.clone()))
    .bind(parsed.as_ref().and_then(|nfo| nfo.tagline.clone()))
    .bind(metadata_id)
    .execute(&mut **tx)
    .await?;
    replace_uniform_facets(
        tx,
        "OtherVideoMetadataId",
        metadata_id,
        &genres,
        &tags,
        &studios,
        &music_labels,
        &languages,
    )
    .await?;
    let actors = parsed
        .as_ref()
        .map(|nfo| local_television::actors_from_nfo(&nfo.actors))
        .unwrap_or_default();
    let directors = parsed
        .as_ref()
        .map(|nfo| local_television::directors_from(&nfo.directors))
        .unwrap_or_default();
    let writers = parsed
        .as_ref()
        .map(|nfo| local_television::directors_from(&nfo.writers))
        .unwrap_or_default();
    let guids = parsed
        .as_ref()
        .map(|nfo| local_television::guids_from_nfo(&nfo.guids))
        .unwrap_or_default();
    replace_people(tx, "Actor", "OtherVideoMetadataId", metadata_id, &actors).await?;
    replace_people(
        tx,
        "Director",
        "OtherVideoMetadataId",
        metadata_id,
        &directors,
    )
    .await?;
    replace_people(tx, "Writer", "OtherVideoMetadataId", metadata_id, &writers).await?;
    replace_guids(tx, "OtherVideoMetadataId", metadata_id, &guids).await?;
    Ok(metadata_id)
}

#[allow(clippy::too_many_lines)]
async fn enrich_song_metadata(
    tx: &mut Transaction<'_, Db>,
    item_id: i64,
    library_path: &LocalLibraryPath,
    path: &Path,
    probed: &FfprobeMediaVersion,
) -> Result<i64> {
    let parsed = locate_same_basename_nfo(path).and_then(|nfo_path| {
        match nfo::parse_song_nfo(&nfo_path) {
            Ok(parsed) => Some(parsed),
            Err(nfo::NfoError::MissingRoot(_)) => {
                debug!(path = %nfo_path.display(), "sidecar carries no <song> root; falling back");
                None
            }
            Err(error) => {
                warn!(path = %nfo_path.display(), error = %error, "failed to parse song nfo; falling back");
                None
            }
        }
    });
    let embedded = EmbeddedFacets::from_probe(probed);
    let fallback_title = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_default()
        .to_owned();
    let title = parsed
        .as_ref()
        .and_then(|nfo| nfo.title.clone())
        .or_else(|| embedded_scalar(probed, &["title"]))
        .unwrap_or(fallback_title);
    let sort_title = parsed
        .as_ref()
        .and_then(|nfo| nfo.sort_title.clone())
        .unwrap_or_else(|| nfo::sort_title(&title));
    let ratings = preferred_facet_values(
        &parsed
            .as_ref()
            .map(|nfo| normalize_content_ratings(&nfo.content_ratings))
            .unwrap_or_default(),
        &embedded.content_ratings,
        &[],
    );
    let genres = preferred_facet_values(
        &parsed
            .as_ref()
            .map(|nfo| nfo.genres.clone())
            .unwrap_or_default(),
        &embedded.genres,
        &[],
    );
    let sidecar_tags = parsed
        .as_ref()
        .map(|nfo| nfo.tags.clone())
        .unwrap_or_default();
    let tags = union_facet_values(
        &preferred_facet_values(&sidecar_tags, &embedded.tags, &[]),
        &local_folder_tags(Path::new(&library_path.path), path),
    );
    let studios = preferred_facet_values(
        &parsed
            .as_ref()
            .map(|nfo| nfo.studios.clone())
            .unwrap_or_default(),
        &embedded.studios,
        &[],
    );
    let music_labels = preferred_facet_values(
        &parsed
            .as_ref()
            .map(|nfo| nfo.music_labels.clone())
            .unwrap_or_default(),
        &embedded.music_labels,
        &[],
    );
    let explicit_languages = preferred_facet_values(
        &parsed
            .as_ref()
            .map(|nfo| normalize_language_codes(&nfo.languages))
            .unwrap_or_default(),
        &embedded.languages,
        &[],
    );
    let languages = union_facet_values(&explicit_languages, &audio_stream_languages(probed));
    let artists = parsed
        .as_ref()
        .map(|nfo| normalize_facet_values(&nfo.artists))
        .filter(|values| !values.is_empty())
        .unwrap_or_else(|| {
            embedded_scalar(probed, &["artist", "artists"])
                .map(|value| split_embedded_values(&value))
                .unwrap_or_default()
        });
    let album_artists = parsed
        .as_ref()
        .map(|nfo| normalize_facet_values(&nfo.album_artists))
        .filter(|values| !values.is_empty())
        .unwrap_or_else(|| {
            embedded_scalar(probed, &["album_artist", "albumartist"])
                .map(|value| split_embedded_values(&value))
                .unwrap_or_default()
        });
    let metadata_id: i64 =
        sqlx::query_scalar(r#"SELECT "Id" FROM "SongMetadata" WHERE "SongId" = ? LIMIT 1"#)
            .bind(item_id)
            .fetch_one(&mut **tx)
            .await?;
    sqlx::query(
        r#"UPDATE "SongMetadata"
           SET "DateUpdated" = ?, "MetadataKind" = ?, "Title" = ?,
               "SortTitle" = ?, "OriginalTitle" = ?, "Year" = ?,
               "ReleaseDate" = ?, "ContentRating" = ?, "Album" = ?,
               "Artists" = ?, "AlbumArtists" = ?, "Track" = ?
           WHERE "Id" = ?"#,
    )
    .bind(encode_datetime(Utc::now()))
    .bind(local_metadata_kind(
        parsed.is_some(),
        probed,
        &embedded,
        &[
            "title",
            "artist",
            "artists",
            "album_artist",
            "albumartist",
            "album",
            "track",
        ],
    ))
    .bind(&title)
    .bind(&sort_title)
    .bind(original_title_for_path(library_path, path))
    .bind(parsed.as_ref().and_then(|nfo| nfo.year))
    .bind(
        parsed
            .as_ref()
            .and_then(|nfo| nfo.release_date)
            .map(encode_datetime),
    )
    .bind((!ratings.is_empty()).then(|| ratings.join("/")))
    .bind(
        parsed
            .as_ref()
            .and_then(|nfo| nfo.album.clone())
            .or_else(|| embedded_scalar(probed, &["album"])),
    )
    .bind((!artists.is_empty()).then(|| artists.join("; ")))
    .bind((!album_artists.is_empty()).then(|| album_artists.join("; ")))
    .bind(
        parsed
            .as_ref()
            .and_then(|nfo| nfo.track.clone())
            .or_else(|| embedded_scalar(probed, &["track"])),
    )
    .bind(metadata_id)
    .execute(&mut **tx)
    .await?;
    replace_uniform_facets(
        tx,
        "SongMetadataId",
        metadata_id,
        &genres,
        &tags,
        &studios,
        &music_labels,
        &languages,
    )
    .await?;
    Ok(metadata_id)
}

#[allow(clippy::too_many_lines)]
async fn enrich_image_metadata(
    tx: &mut Transaction<'_, Db>,
    item_id: i64,
    library_path: &LocalLibraryPath,
    path: &Path,
    probed: &FfprobeMediaVersion,
) -> Result<i64> {
    let parsed = locate_same_basename_nfo(path).and_then(|nfo_path| {
        match nfo::parse_image_nfo(&nfo_path) {
            Ok(parsed) => Some(parsed),
            Err(nfo::NfoError::MissingRoot(_)) => {
                debug!(path = %nfo_path.display(), "sidecar carries no <image> root; falling back");
                None
            }
            Err(error) => {
                warn!(path = %nfo_path.display(), error = %error, "failed to parse image nfo; falling back");
                None
            }
        }
    });
    let embedded = EmbeddedFacets::from_probe(probed);
    let fallback_title = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_default()
        .to_owned();
    let title = parsed
        .as_ref()
        .and_then(|nfo| nfo.title.clone())
        .or_else(|| embedded_scalar(probed, &["title"]))
        .unwrap_or(fallback_title);
    let sort_title = parsed
        .as_ref()
        .and_then(|nfo| nfo.sort_title.clone())
        .unwrap_or_else(|| nfo::sort_title(&title));
    let ratings = preferred_facet_values(
        &parsed
            .as_ref()
            .map(|nfo| normalize_content_ratings(&nfo.content_ratings))
            .unwrap_or_default(),
        &embedded.content_ratings,
        &[],
    );
    let genres = preferred_facet_values(
        &parsed
            .as_ref()
            .map(|nfo| nfo.genres.clone())
            .unwrap_or_default(),
        &embedded.genres,
        &[],
    );
    let tags = preferred_facet_values(
        &parsed
            .as_ref()
            .map(|nfo| nfo.tags.clone())
            .unwrap_or_default(),
        &embedded.tags,
        &[],
    );
    let studios = preferred_facet_values(
        &parsed
            .as_ref()
            .map(|nfo| nfo.studios.clone())
            .unwrap_or_default(),
        &embedded.studios,
        &[],
    );
    let music_labels = preferred_facet_values(
        &parsed
            .as_ref()
            .map(|nfo| nfo.music_labels.clone())
            .unwrap_or_default(),
        &embedded.music_labels,
        &[],
    );
    // Image language is explicit-only: sidecar or embedded container metadata,
    // never a folder or media-stream inference.
    let languages = preferred_facet_values(
        &parsed
            .as_ref()
            .map(|nfo| normalize_language_codes(&nfo.languages))
            .unwrap_or_default(),
        &embedded.languages,
        &[],
    );
    let metadata_id: i64 =
        sqlx::query_scalar(r#"SELECT "Id" FROM "ImageMetadata" WHERE "ImageId" = ? LIMIT 1"#)
            .bind(item_id)
            .fetch_one(&mut **tx)
            .await?;
    sqlx::query(
        r#"UPDATE "ImageMetadata"
           SET "DateUpdated" = ?, "MetadataKind" = ?, "Title" = ?,
               "SortTitle" = ?, "OriginalTitle" = ?, "Year" = ?,
               "ReleaseDate" = ?, "ContentRating" = ?
           WHERE "Id" = ?"#,
    )
    .bind(encode_datetime(Utc::now()))
    .bind(local_metadata_kind(
        parsed.is_some(),
        probed,
        &embedded,
        &["title"],
    ))
    .bind(&title)
    .bind(&sort_title)
    .bind(original_title_for_path(library_path, path))
    .bind(parsed.as_ref().and_then(|nfo| nfo.year))
    .bind(
        parsed
            .as_ref()
            .and_then(|nfo| nfo.release_date)
            .map(encode_datetime),
    )
    .bind((!ratings.is_empty()).then(|| ratings.join("/")))
    .bind(metadata_id)
    .execute(&mut **tx)
    .await?;
    replace_uniform_facets(
        tx,
        "ImageMetadataId",
        metadata_id,
        &genres,
        &tags,
        &studios,
        &music_labels,
        &languages,
    )
    .await?;
    Ok(metadata_id)
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
    probed: &FfprobeMediaVersion,
) -> Result<i64> {
    let movie_nfo = load_movie_nfo(path);

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
    let release_date = movie_nfo
        .as_ref()
        .and_then(|nfo| nfo.premiered.map(encode_datetime));
    let now = encode_datetime(Utc::now());
    let embedded = EmbeddedFacets::from_probe(probed);
    let metadata_kind = local_metadata_kind(movie_nfo.is_some(), probed, &embedded, &[]);
    let sidecar_ratings = movie_nfo
        .as_ref()
        .map(|nfo| normalize_content_ratings(&nfo.content_ratings))
        .unwrap_or_default();
    let ratings = preferred_facet_values(&sidecar_ratings, &embedded.content_ratings, &[]);
    let rating = (!ratings.is_empty()).then(|| ratings.join("/"));

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
        .bind(rating.as_deref())
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
        .bind(rating.as_deref())
        .bind(movie_nfo.as_ref().and_then(|nfo| nfo.plot.clone()))
        .bind(movie_nfo.as_ref().and_then(|nfo| nfo.outline.clone()))
        .execute(&mut **tx)
        .await?;
        ersatzrs_infra::db::last_insert_id(result)
    };

    let genres = preferred_facet_values(
        &movie_nfo
            .as_ref()
            .map(|nfo| nfo.genres.clone())
            .unwrap_or_default(),
        &embedded.genres,
        &[],
    );
    let tags = preferred_facet_values(
        &movie_nfo
            .as_ref()
            .map(|nfo| nfo.tags.clone())
            .unwrap_or_default(),
        &embedded.tags,
        &[],
    );
    let studios = preferred_facet_values(
        &movie_nfo
            .as_ref()
            .map(|nfo| nfo.studios.clone())
            .unwrap_or_default(),
        &embedded.studios,
        &[],
    );
    let music_labels = preferred_facet_values(
        &movie_nfo
            .as_ref()
            .map(|nfo| nfo.music_labels.clone())
            .unwrap_or_default(),
        &embedded.music_labels,
        &[],
    );
    let explicit_languages = preferred_facet_values(
        &movie_nfo
            .as_ref()
            .map(|nfo| normalize_language_codes(&nfo.languages))
            .unwrap_or_default(),
        &embedded.languages,
        &[],
    );
    let languages = union_facet_values(&explicit_languages, &audio_stream_languages(probed));
    replace_strings(tx, "Genre", "MovieMetadataId", metadata_id, &genres).await?;
    replace_strings(tx, "Tag", "MovieMetadataId", metadata_id, &tags).await?;
    replace_strings(tx, "Studio", "MovieMetadataId", metadata_id, &studios).await?;
    replace_strings(
        tx,
        "MusicLabel",
        "MovieMetadataId",
        metadata_id,
        &music_labels,
    )
    .await?;
    replace_metadata_languages(tx, "MovieMetadataId", metadata_id, &languages).await?;

    if let Some(parsed) = movie_nfo.as_ref() {
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
                "BitsPerRawSample", "AttachedPic", "FileName", "MimeType", "Rotation",
                "MasteringDisplay", "MaxCll", "MaxFall",
                "DolbyVisionProfile", "DolbyVisionBlCompatibilityId",
                "DolbyVisionBlPresent", "DolbyVisionElPresent", "DolbyVisionRpuPresent",
                "DolbyVisionMetadataState", "DolbyVisionMetadataHint"
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
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
        .bind(stream.rotation)
        .bind(&stream.mastering_display)
        .bind(stream.max_cll)
        .bind(stream.max_fall)
        .bind(stream.dolby_vision_profile)
        .bind(stream.dolby_vision_bl_compatibility_id)
        .bind(stream.dolby_vision_bl_present)
        .bind(stream.dolby_vision_el_present)
        .bind(stream.dolby_vision_rpu_present)
        .bind(stream.dolby_vision_metadata_state.as_i32())
        .bind(&stream.dolby_vision_metadata_hint)
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

/// Delete artist roots that no longer own a music video in this library path.
///
/// Mirrors ErsatzTV's trailing `ArtistRepository.DeleteEmptyArtists` pass. An
/// artist metadata identity change can create a replacement artist during the
/// add/update pass; once the existing music videos move to that replacement,
/// deleting the empty artist's `MediaItem` root cascades through `Artist`,
/// `ArtistMetadata`, and its artwork.
async fn delete_empty_artists(pool: &DbPool, library_path_id: i64) -> Result<Vec<i64>> {
    let mut tx = ersatzrs_infra::db::begin_write(pool).await?;
    let rows = sqlx::query(
        r#"SELECT "a"."Id" AS "Id"
           FROM "Artist" AS "a"
           JOIN "MediaItem" AS "mi" ON "mi"."Id" = "a"."Id"
           WHERE "mi"."LibraryPathId" = ?
             AND NOT EXISTS (
                 SELECT 1 FROM "MusicVideo" AS "mv" WHERE "mv"."ArtistId" = "a"."Id"
             )
           ORDER BY "a"."Id""#,
    )
    .bind(library_path_id)
    .fetch_all(&mut *tx)
    .await?;
    let artist_ids: Vec<i64> = rows.into_iter().map(|row| row.get("Id")).collect();

    if !artist_ids.is_empty() {
        let placeholders = vec!["?"; artist_ids.len()].join(", ");
        let sql = format!(r#"DELETE FROM "MediaItem" WHERE "Id" IN ({placeholders})"#);
        let mut query = sqlx::query(ersatzrs_infra::db::assert_sql_safe(&sql));
        for artist_id in &artist_ids {
            query = query.bind(artist_id);
        }
        query.execute(&mut *tx).await?;
    }

    tx.commit().await?;
    Ok(artist_ids)
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
    use super::{
        is_scannable_for_kind, load_movie_nfo, local_metadata_kind, locate_movie_nfos,
        locate_music_video_artwork, membership_refresh_reason, path_hash, remote_stream_playlist,
        retry_sqlite_busy, EmbeddedFacets, METADATA_KIND_EMBEDDED, METADATA_KIND_FALLBACK,
        METADATA_KIND_SIDECAR,
    };
    use ersatzrs_domain::{
        classify_alternate_version, classify_extra_directory_name, classify_extra_video,
        ExtraParentHint, LibraryMediaKind, ScanPlayoutRefreshReason,
    };
    use std::path::Path;

    #[test]
    fn remote_stream_work_orders_manifests_before_definitions_deterministically() {
        let temp = tempfile::tempdir().expect("remote stream root");
        let nested = temp.path().join("nested");
        std::fs::create_dir_all(&nested).expect("nested folder");
        let mut manifests = vec![
            temp.path().join("Zulu.playlist.yml"),
            nested.join("Alpha.playlist.yaml"),
        ];
        let mut definitions = vec![temp.path().join("000.yml"), nested.join("Beta.yaml")];
        for path in manifests.iter().chain(&definitions) {
            std::fs::write(path, b"fixture").expect("remote stream fixture");
        }

        manifests.sort();
        definitions.sort();
        let expected = manifests
            .into_iter()
            .map(|path| (true, path))
            .chain(definitions.into_iter().map(|path| (false, path)))
            .collect::<Vec<_>>();
        let actual = super::collect_remote_stream_files(&temp.path().display().to_string())
            .iter()
            .map(|work| {
                (
                    matches!(work, super::LocalScanWork::RemoteStreamPlaylist { .. }),
                    super::work_path(work).to_owned(),
                )
            })
            .collect::<Vec<_>>();

        assert_eq!(actual, expected);
    }

    #[test]
    fn movie_sidecar_falls_through_release_art_to_the_jellyfin_sidecar() {
        let dir = tempfile::tempdir().expect("temp dir");
        let video = dir.path().join("Movie (1984).mkv");
        std::fs::write(&video, b"").expect("write video");
        // Release art under the video's basename, real metadata in movie.nfo.
        std::fs::write(dir.path().join("Movie (1984).nfo"), [0xdb_u8; 64])
            .expect("write release art");
        std::fs::write(
            dir.path().join("movie.nfo"),
            b"<movie><title>Real Title</title><year>1984</year></movie>",
        )
        .expect("write sidecar");

        assert_eq!(locate_movie_nfos(&video).len(), 2);

        let parsed = load_movie_nfo(&video).expect("movie.nfo should be reached");
        assert_eq!(parsed.title.as_deref(), Some("Real Title"));
        assert_eq!(parsed.year, Some(1984));
    }

    #[test]
    fn movie_sidecar_prefers_the_basename_file_when_it_carries_metadata() {
        let dir = tempfile::tempdir().expect("temp dir");
        let video = dir.path().join("Movie (1984).mkv");
        std::fs::write(&video, b"").expect("write video");
        std::fs::write(
            dir.path().join("Movie (1984).nfo"),
            b"<movie><title>Basename Wins</title></movie>",
        )
        .expect("write basename sidecar");
        std::fs::write(
            dir.path().join("movie.nfo"),
            b"<movie><title>Folder Sidecar</title></movie>",
        )
        .expect("write folder sidecar");

        let parsed = load_movie_nfo(&video).expect("basename sidecar should parse");
        assert_eq!(parsed.title.as_deref(), Some("Basename Wins"));
    }

    #[test]
    fn path_hash_matches_sha256sum_output() {
        assert_eq!(
            path_hash("/media/movie.mkv"),
            "255ca6b14e280f5d5ca28f4e6e010b19038627dbd263a1d17556460b7ee06e24"
        );
    }

    #[test]
    fn embedded_facets_use_case_insensitive_aliases_without_splitting_commas() {
        let mut format_metadata = std::collections::BTreeMap::new();
        format_metadata.insert("MPAA".to_owned(), "NL:12; US:PG-13".to_owned());
        format_metadata.insert("Production_Studio".to_owned(), "A, B Studios".to_owned());
        format_metadata.insert("Publisher".to_owned(), "Label One\0Label Two".to_owned());
        format_metadata.insert("LANGUAGE".to_owned(), "nl".to_owned());
        format_metadata.insert("Keywords".to_owned(), "one;two".to_owned());
        let probed = ersatzrs_infra::ffprobe::FfprobeMediaVersion {
            version: ersatzrs_domain::MediaVersion::default(),
            streams: Vec::new(),
            chapters: Vec::new(),
            format_metadata,
        };

        let facets = EmbeddedFacets::from_probe(&probed);
        assert_eq!(facets.content_ratings, ["NL:12", "US:PG-13"]);
        assert_eq!(facets.studios, ["A, B Studios"]);
        assert_eq!(facets.music_labels, ["Label One", "Label Two"]);
        assert_eq!(facets.languages, ["dut"]);
        assert_eq!(facets.tags, ["one", "two"]);
    }

    #[test]
    fn local_metadata_kind_distinguishes_sidecar_embedded_and_unrelated_tags() {
        let probe = |key: &str, value: &str| ersatzrs_infra::ffprobe::FfprobeMediaVersion {
            version: ersatzrs_domain::MediaVersion::default(),
            streams: Vec::new(),
            chapters: Vec::new(),
            format_metadata: [(key.to_owned(), value.to_owned())].into_iter().collect(),
        };

        let encoder = probe("encoder", "fixture");
        let encoder_facets = EmbeddedFacets::from_probe(&encoder);
        assert_eq!(
            local_metadata_kind(false, &encoder, &encoder_facets, &[]),
            METADATA_KIND_FALLBACK
        );

        let publisher = probe("publisher", "Music Label");
        let publisher_facets = EmbeddedFacets::from_probe(&publisher);
        assert_eq!(
            local_metadata_kind(false, &publisher, &publisher_facets, &[]),
            METADATA_KIND_EMBEDDED
        );
        assert_eq!(
            local_metadata_kind(true, &publisher, &publisher_facets, &[]),
            METADATA_KIND_SIDECAR
        );

        let title = probe("title", "Embedded title");
        let title_facets = EmbeddedFacets::from_probe(&title);
        assert_eq!(
            local_metadata_kind(false, &title, &title_facets, &["title"]),
            METADATA_KIND_EMBEDDED
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

    #[tokio::test]
    async fn music_video_solo_upscale_becomes_primary_with_no_versions() {
        use super::{
            link_alternate_versions_for_library_path, upsert_artist_for_folder,
            upsert_local_music_video,
        };
        use sqlx::Row;
        use tempfile::TempDir;

        let temp = TempDir::new().expect("temp dir");
        let artist_dir = temp.path().join("Daft Punk");
        std::fs::create_dir_all(&artist_dir).expect("artist dir");
        let remaster_path = artist_dir.join("Around The World (Official Video Upscaled).mkv");
        std::fs::write(&remaster_path, b"not probed").expect("upscaled video");

        let pool = test_db().await;
        let library_path = seed_music_video_library_path(temp.path());
        seed_music_video_library(&pool, &library_path).await;
        let artist_id = upsert_artist_for_folder(&pool, &library_path, "Daft Punk")
            .await
            .expect("artist");

        upsert_local_music_video(
            &pool,
            &library_path,
            artist_id,
            &artist_dir,
            &remaster_path,
            probed_version(),
        )
        .await
        .expect("upscaled video upsert");

        link_alternate_versions_for_library_path(&pool, &library_path)
            .await
            .expect("link versions");

        let remaster_id = media_item_id_for_path(&pool, &remaster_path).await;
        let row = sqlx::query(
            r#"SELECT "ContentKind", "SubKind", "ParentMediaItemId"
               FROM "MediaItem" WHERE "Id" = ?"#,
        )
        .bind(remaster_id)
        .fetch_one(&pool)
        .await
        .expect("upscaled video row");
        assert_eq!(
            row.get::<i32, _>("ContentKind"),
            ersatzrs_domain::ContentKind::Primary.db_value(),
            "a lone upscaled video is the video for this song, not an alternate of it"
        );
        assert!(row.get::<Option<String>, _>("SubKind").is_none());
        assert!(row.get::<Option<i64>, _>("ParentMediaItemId").is_none());
    }

    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn music_video_remaster_promoted_over_untagged_master_demotes_it_to_versions() {
        use super::{
            flag_missing_local_files, link_alternate_versions_for_library_path,
            upsert_artist_for_folder, upsert_local_music_video,
        };
        use sqlx::Row;
        use tempfile::TempDir;

        let temp = TempDir::new().expect("temp dir");
        let artist_dir = temp.path().join("Daft Punk");
        std::fs::create_dir_all(&artist_dir).expect("artist dir");
        let master_path = artist_dir.join("Around The World.mkv");
        let remaster_path = artist_dir.join("Around The World (Remastered).mkv");
        std::fs::write(&master_path, b"not probed").expect("master video");
        std::fs::write(&remaster_path, b"not probed").expect("remaster video");

        let pool = test_db().await;
        let library_path = seed_music_video_library_path(temp.path());
        seed_music_video_library(&pool, &library_path).await;
        let artist_id = upsert_artist_for_folder(&pool, &library_path, "Daft Punk")
            .await
            .expect("artist");

        upsert_local_music_video(
            &pool,
            &library_path,
            artist_id,
            &artist_dir,
            &master_path,
            probed_version(),
        )
        .await
        .expect("master upsert");
        upsert_local_music_video(
            &pool,
            &library_path,
            artist_id,
            &artist_dir,
            &remaster_path,
            probed_version(),
        )
        .await
        .expect("remaster upsert");

        link_alternate_versions_for_library_path(&pool, &library_path)
            .await
            .expect("link versions");

        let master_id = media_item_id_for_path(&pool, &master_path).await;
        let remaster_id = media_item_id_for_path(&pool, &remaster_path).await;

        let remaster_row = sqlx::query(
            r#"SELECT "ContentKind", "SubKind", "ParentMediaItemId"
               FROM "MediaItem" WHERE "Id" = ?"#,
        )
        .bind(remaster_id)
        .fetch_one(&pool)
        .await
        .expect("remaster row");
        assert_eq!(
            remaster_row.get::<i32, _>("ContentKind"),
            ersatzrs_domain::ContentKind::Primary.db_value(),
            "the remaster should be promoted ahead of the untagged original master"
        );
        assert!(remaster_row
            .get::<Option<i64>, _>("ParentMediaItemId")
            .is_none());

        let master_row = sqlx::query(
            r#"SELECT "ContentKind", "SubKind", "ParentMediaItemId"
               FROM "MediaItem" WHERE "Id" = ?"#,
        )
        .bind(master_id)
        .fetch_one(&pool)
        .await
        .expect("master row");
        assert_eq!(
            master_row.get::<i32, _>("ContentKind"),
            ersatzrs_domain::ContentKind::AlternateVersion.db_value(),
            "the untagged original master is demoted once a remaster sibling exists"
        );
        assert_eq!(
            master_row.get::<Option<String>, _>("SubKind").as_deref(),
            Some("official_video")
        );
        assert_eq!(
            master_row.get::<Option<i64>, _>("ParentMediaItemId"),
            Some(remaster_id)
        );

        std::fs::remove_file(&remaster_path).expect("remove remaster");
        flag_missing_local_files(&pool, library_path.id)
            .await
            .expect("flag missing remaster");
        link_alternate_versions_for_library_path(&pool, &library_path)
            .await
            .expect("relink without remaster");

        let master_row = sqlx::query(
            r#"SELECT "State", "ContentKind", "SubKind", "ParentMediaItemId"
               FROM "MediaItem" WHERE "Id" = ?"#,
        )
        .bind(master_id)
        .fetch_one(&pool)
        .await
        .expect("remaining master row");
        assert_eq!(master_row.get::<i32, _>("State"), 0);
        assert_eq!(
            master_row.get::<i32, _>("ContentKind"),
            ersatzrs_domain::ContentKind::Primary.db_value(),
            "the playable original must become primary when its remaster disappears"
        );
        assert!(master_row.get::<Option<String>, _>("SubKind").is_none());
        assert!(master_row
            .get::<Option<i64>, _>("ParentMediaItemId")
            .is_none());

        let remaster_row = sqlx::query(
            r#"SELECT "State", "ContentKind", "SubKind", "ParentMediaItemId"
               FROM "MediaItem" WHERE "Id" = ?"#,
        )
        .bind(remaster_id)
        .fetch_one(&pool)
        .await
        .expect("missing remaster row");
        assert_eq!(remaster_row.get::<i32, _>("State"), 1);
        assert_eq!(
            remaster_row.get::<i32, _>("ContentKind"),
            ersatzrs_domain::ContentKind::AlternateVersion.db_value()
        );
        assert_eq!(
            remaster_row.get::<Option<i64>, _>("ParentMediaItemId"),
            Some(master_id)
        );
    }

    #[tokio::test]
    async fn movie_and_song_enhanced_versions_do_not_replace_untagged_primaries() {
        use super::{link_alternate_versions_for_library_path, upsert_local_file};
        use tempfile::TempDir;

        let movie_temp = TempDir::new().expect("movie temp dir");
        let movie_path = movie_temp.path().join("Blade Runner.mkv");
        let movie_remaster_path = movie_temp.path().join("Blade Runner (Upscaled).mkv");
        std::fs::write(&movie_path, b"not probed").expect("movie");
        std::fs::write(&movie_remaster_path, b"not probed").expect("upscaled movie");
        let movie_pool = test_db().await;
        let movie_library = seed_movie_library_path(&movie_pool, movie_temp.path()).await;
        upsert_local_file(&movie_pool, &movie_library, &movie_path, probed_version())
            .await
            .expect("movie upsert");
        upsert_local_file(
            &movie_pool,
            &movie_library,
            &movie_remaster_path,
            probed_version(),
        )
        .await
        .expect("upscaled movie upsert");
        link_alternate_versions_for_library_path(&movie_pool, &movie_library)
            .await
            .expect("link movie versions");
        assert_primary_and_remaster_alternate(&movie_pool, &movie_path, &movie_remaster_path).await;

        let song_temp = TempDir::new().expect("song temp dir");
        let song_path = song_temp.path().join("Around The World.flac");
        let song_remaster_path = song_temp.path().join("Around The World (Remastered).flac");
        std::fs::write(&song_path, b"not probed").expect("song");
        std::fs::write(&song_remaster_path, b"not probed").expect("song remaster");
        let song_pool = test_db().await;
        let song_library = seed_song_library(&song_pool, song_temp.path()).await;
        upsert_local_file(&song_pool, &song_library, &song_path, probed_version())
            .await
            .expect("song upsert");
        upsert_local_file(
            &song_pool,
            &song_library,
            &song_remaster_path,
            probed_version(),
        )
        .await
        .expect("song remaster upsert");
        link_alternate_versions_for_library_path(&song_pool, &song_library)
            .await
            .expect("link song versions");
        assert_primary_and_remaster_alternate(&song_pool, &song_path, &song_remaster_path).await;
    }

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
            r"
url: https://example.test/live.m3u8
is_live: true
duration: 01:02:03
fallback_query: tag:backup
title: Example Live
plot: Example plot
year: 2024
release_date: 2024-03-17T21:15:00+01:00
content_rating: example:12
genres: [Magazine]
tags: [Television]
collection: Example collection
people:
  - name: Example Presenter
    role: presenter
  - name: Example Director
    role: director
producers: [Example Productions]
original_broadcasters: [Original Network]
broadcasters: [Current Network]
",
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
            r#"SELECT rs."Url", rs."AddonId", rs."Script", rs."Duration", rs."FallbackQuery", rs."IsLive",
                      rsm."Title", rsm."Plot", rsm."Year", rsm."ReleaseDate",
                      rsm."ContentRating", rsm."CollectionName",
                      rsm."OriginalBroadcasters", rsm."Broadcasters",
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
        assert_eq!(row.get::<Option<String>, _>("AddonId"), None);
        assert_eq!(row.get::<Option<String>, _>("Script"), None);
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
            row.get::<Option<String>, _>("ReleaseDate").as_deref(),
            Some("2024-03-17T21:15:00+01:00")
        );
        assert_eq!(
            row.get::<Option<String>, _>("ContentRating").as_deref(),
            Some("example:12")
        );
        assert_eq!(
            row.get::<Option<String>, _>("CollectionName").as_deref(),
            Some("Example collection")
        );
        assert_eq!(
            row.get::<Option<String>, _>("OriginalBroadcasters")
                .as_deref(),
            Some(r#"["Original Network"]"#)
        );
        assert_eq!(
            row.get::<Option<String>, _>("Broadcasters").as_deref(),
            Some(r#"["Current Network"]"#)
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
        assert!(tags.contains(&"Television".to_owned()));
        assert!(tags.contains(&"Example collection".to_owned()));
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
        assert_eq!(
            sqlx::query_scalar::<_, Vec<u8>>(
                r#"SELECT CAST(GROUP_CONCAT("Name", '|') AS BLOB)
                   FROM (SELECT "Name" FROM "Genre" ORDER BY "Name")"#,
            )
            .fetch_one(&pool)
            .await
            .expect("genres"),
            b"Magazine"
        );
        let people = sqlx::query(r#"SELECT "Name", "Role" FROM "Actor" ORDER BY "Order", "Id""#)
            .fetch_all(&pool)
            .await
            .expect("people");
        assert_eq!(
            people
                .iter()
                .map(|row| (row.get::<String, _>("Name"), row.get::<String, _>("Role")))
                .collect::<Vec<_>>(),
            [
                ("Example Presenter".to_owned(), "presenter".to_owned()),
                ("Example Director".to_owned(), "director".to_owned()),
                ("Example Productions".to_owned(), "producer".to_owned()),
            ]
        );

        let media_item_id = sqlx::query_scalar::<_, i64>(
            r#"SELECT mi."Id"
               FROM "MediaItem" mi
               JOIN "RemoteStream" rs ON rs."Id" = mi."Id""#,
        )
        .fetch_one(&pool)
        .await
        .expect("remote stream media item id");
        let repository =
            ersatzrs_infra::repositories::SqliteSchedulingContentRepository::new(pool.clone());
        for query in [
            "actor:\"Example Presenter\"",
            "genre:Magazine",
            "studio:\"Current Network\"",
            "tag:Television",
            "tag:\"Example collection\"",
        ] {
            assert_eq!(search_ids(&repository, query).await, vec![media_item_id]);
        }

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
            format_metadata: std::collections::BTreeMap::new(),
        }
    }

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

    async fn file_test_db(path: &Path) -> sqlx::SqlitePool {
        use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
        use sqlx::Executor;
        use std::time::Duration as StdDuration;

        let database_url = format!("sqlite:{}", path.display());
        let options = database_url
            .parse::<SqliteConnectOptions>()
            .expect("sqlite options")
            .create_if_missing(true)
            .foreign_keys(true)
            .busy_timeout(StdDuration::from_secs(5));
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .expect("connect file-backed sqlite");
        pool.execute("PRAGMA foreign_keys = ON")
            .await
            .expect("foreign keys");
        sqlx::migrate!("../../migrations")
            .run(&pool)
            .await
            .expect("migrate");
        pool
    }

    #[allow(clippy::too_many_lines)]
    async fn seed_managed_remote_stream_playlist(
        pool: &sqlx::SqlitePool,
        library_root: &Path,
        package_root: &Path,
    ) {
        use sqlx::Executor;

        const ADDON_ID: &str = "org.ersatzrs.addon.fixture";
        std::fs::create_dir_all(library_root).expect("remote stream library root");
        std::fs::create_dir_all(package_root).expect("fixture add-on package");
        std::fs::write(package_root.join("fixture.bin"), b"not executed")
            .expect("fixture add-on entrypoint");
        let manifest_path = library_root.join("Fixture.playlist.yml");
        std::fs::write(
            &manifest_path,
            format!(
                r"name: Fixture
url: https://example.test/playlist
addon: {ADDON_ID}
sync:
  interval: 24h
  max_items: 10
  order: upstream
  on_removed: trash
create_playlist: true
",
            ),
        )
        .expect("playlist manifest");

        let library_path = super::LocalLibraryPath {
            id: 1,
            path: library_root.display().to_string(),
            media_kind: LibraryMediaKind::RemoteStreams,
        };
        seed_local_library(pool, &library_path, "Remote Streams", 7).await;

        let addon_manifest = serde_json::json!({
            "manifest_version": 1,
            "id": ADDON_ID,
            "version": "1.0.0",
            "name": {"en-US": "Fixture"},
            "summary": {"en-US": "Fixture playlist add-on"},
            "license": "GPL-3.0-or-later",
            "source_url": "https://example.test/source",
            "host_version": "*",
            "capabilities": [
                {"id": "addon.check.v1"},
                {"id": "remote-stream.list.v4"},
                {"id": "remote-stream.play.v1"}
            ],
            "entrypoints": {
                "any": {"path": "fixture.bin", "kind": "native"}
            },
            "permissions": {},
            "settings": [],
            "panels": []
        });
        let content_sha256 = ersatzrs_infra::addons::calculate_addon_content_sha256(package_root)
            .await
            .expect("fixture add-on hash");
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            r#"INSERT INTO "AddonInstallation" (
                   "AddonId", "RepositoryId", "Version", "Enabled", "PackageSha256",
                   "ContentSha256", "ManifestJson", "InstallPath", "InstalledAt", "UpdatedAt"
               ) VALUES (?, NULL, '1.0.0', 1, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(ADDON_ID)
        .bind("a".repeat(64))
        .bind(content_sha256)
        .bind(addon_manifest.to_string())
        .bind(package_root.to_string_lossy().into_owned())
        .bind(&now)
        .bind(&now)
        .execute(pool)
        .await
        .expect("fixture add-on installation");
        // A staged publication owns a journal even when it has no playback
        // selections; an absent journal must remain a production error.
        let publication_journal = serde_json::json!({
            "previous_review_state": ersatzrs_domain::AddonMediaListReviewState::Ready,
            "previous_presentation_kind": ersatzrs_domain::AddonMediaListPresentationKind::RemoteStreams,
            "previous_items": [],
            "previous_playlist": null
        });
        sqlx::query(
            r#"INSERT INTO "AddonMediaList" (
                   "Id", "AddonId", "ProviderId", "SourceUrl", "Name",
                   "ContentKindOverride", "ReviewState", "PublicationJournalJson"
               ) VALUES (
                   50, 'org.ersatzrs.addon.fixture', 'fixture/list',
                   'https://example.test/playlist', 'Fixture', 0, 3, ?
               )"#,
        )
        .bind(publication_journal.to_string())
        .execute(pool)
        .await
        .expect("managed add-on list");
        sqlx::query(
            r#"INSERT INTO "AddonMediaListLibrary" (
                   "AddonMediaListId", "LibraryId", "FolderName", "ManifestPath"
               ) VALUES (50, 1, 'Fixture', ?)"#,
        )
        .bind(manifest_path.to_string_lossy().into_owned())
        .execute(pool)
        .await
        .expect("managed add-on list library");
        pool.execute(
            r#"INSERT INTO "AddonMediaListItem" (
                   "AddonMediaListId", "ProviderId", "DisplayTitle", "Kind", "Rank",
                   "Title", "SourceUrl", "DurationSeconds", "Published",
                   "ProviderMetadataJson", "ContentKind", "PublishedLiveness"
               ) VALUES (
                   50, 'one', 'OldMatch', 4, 0, 'OldMatch',
                   'https://example.test/one', 60, 1, '{"title":"OldMatch"}', 4, 1
               )"#,
        )
        .await
        .expect("published add-on list item");
    }

    #[tokio::test]
    async fn playlist_publication_failure_marks_resolution_and_replays_on_restart() {
        use ersatzrs_domain::ScanPlayoutRefreshReason;
        use sqlx::Row as _;

        let temp = tempfile::tempdir().expect("playlist fixture");
        let library_root = temp.path().join("remote-streams");
        let package_root = temp.path().join("addon-package");
        let pool = test_db().await;
        seed_managed_remote_stream_playlist(&pool, &library_root, &package_root).await;
        let managed_folder = library_root.join("Fixture");
        std::fs::create_dir_all(&managed_folder).expect("managed folder");
        let blocked_nfo = managed_folder.join("one.nfo");
        std::fs::create_dir(&blocked_nfo).expect("NFO publication blocker");

        let failed = super::scan_local_library(
            &pool,
            &super::LocalScanConfig {
                library_id: 1,
                ffprobe_path: "unused-ffprobe".to_owned(),
            },
        )
        .await
        .expect("failed file is reported without aborting the library scan");
        assert_eq!(failed.failed_files, 1, "issues: {:?}", failed.issues);
        let failed_resolution = sqlx::query(
            r#"SELECT "Generation", "State", "Reason", "Diagnostic"
               FROM "ScanRefreshResolution" WHERE "LibraryId" = 1"#,
        )
        .fetch_one(&pool)
        .await
        .expect("failed refresh resolution");
        assert_eq!(failed_resolution.get::<i64, _>("Generation"), 1);
        assert_eq!(failed_resolution.get::<i64, _>("State"), 2);
        assert_eq!(
            failed_resolution.get::<String, _>("Reason"),
            ScanPlayoutRefreshReason::RemoteStreamMembershipMixed.as_str()
        );
        assert!(failed_resolution
            .get::<Option<String>, _>("Diagnostic")
            .is_some());
        assert!(!managed_folder.join("one.yml").is_file());

        std::fs::remove_dir(&blocked_nfo).expect("remove NFO publication blocker");
        let replayed = super::scan_local_library(
            &pool,
            &super::LocalScanConfig {
                library_id: 1,
                ffprobe_path: "unused-ffprobe".to_owned(),
            },
        )
        .await
        .expect("restart replay");
        assert_eq!(replayed.failed_files, 0, "issues: {:?}", replayed.issues);
        assert!(managed_folder.join("one.yml").is_file());
        assert!(blocked_nfo.is_file());
        let replayed_resolution = sqlx::query(
            r#"SELECT "Generation", "State", "Reason", "Diagnostic"
               FROM "ScanRefreshResolution" WHERE "LibraryId" = 1"#,
        )
        .fetch_one(&pool)
        .await
        .expect("replayed refresh resolution");
        assert_eq!(replayed_resolution.get::<i64, _>("Generation"), 2);
        assert_eq!(replayed_resolution.get::<i64, _>("State"), 1);
        assert_eq!(
            replayed_resolution.get::<String, _>("Reason"),
            ScanPlayoutRefreshReason::RemoteStreamMembershipAdded.as_str()
        );
        assert!(replayed_resolution
            .get::<Option<String>, _>("Diagnostic")
            .is_none());
        let intent_count: i64 =
            sqlx::query_scalar(r#"SELECT COUNT(*) FROM "ScanPlayoutRefreshIntent""#)
                .fetch_one(&pool)
                .await
                .expect("refresh intent count");
        assert_eq!(intent_count, 0);
    }

    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn playlist_crash_after_database_commit_replays_without_losing_intent() {
        use super::{LocalLibraryPath, PlaylistSyncBoundary};
        use ersatzrs_domain::ScanPlayoutRefreshReason;
        use sqlx::{Executor, Row as _};

        let temp = tempfile::tempdir().expect("playlist fixture");
        let library_root = temp.path().join("remote-streams");
        let package_root = temp.path().join("addon-package");
        let database_path = temp.path().join("scan-refresh.db");
        let manifest_path = library_root.join("Fixture.playlist.yml");
        let managed_folder = library_root.join("Fixture");
        let sidecar_path = managed_folder.join(".ersatzrs-playlist.json");
        let library_path = LocalLibraryPath {
            id: 1,
            path: library_root.display().to_string(),
            media_kind: LibraryMediaKind::RemoteStreams,
        };
        let pool = file_test_db(&database_path).await;
        seed_managed_remote_stream_playlist(&pool, &library_root, &package_root).await;

        let baseline = super::scan_local_library(
            &pool,
            &super::LocalScanConfig {
                library_id: 1,
                ffprobe_path: "unused-ffprobe".to_owned(),
            },
        )
        .await
        .expect("initial playlist scan");
        assert_eq!(baseline.failed_files, 0, "issues: {:?}", baseline.issues);
        let baseline_sidecar = std::fs::read(&sidecar_path).expect("baseline sidecar");
        let playlist_id: i64 = sqlx::query_scalar(
            r#"SELECT "Id" FROM "Playlist"
               WHERE "ManagedSource" LIKE 'remote-stream-playlist:v1:1:%'"#,
        )
        .fetch_one(&pool)
        .await
        .expect("managed playlist");

        pool.execute("PRAGMA foreign_keys = OFF")
            .await
            .expect("disable fixture foreign keys");
        sqlx::query(
            r#"INSERT INTO "ProgramSchedule" (
                   "Id", "FixedStartTimeBehavior", "KeepMultiPartEpisodesTogether", "Name",
                   "RandomStartPoint", "ShuffleScheduleItems", "TreatCollectionsAsShows"
               ) VALUES (70, 0, 0, 'Managed playlist schedule', 0, 0, 0);
               INSERT INTO "ProgramScheduleItem" (
                   "Id", "CollectionType", "FillWithGroupMode", "GuideMode", "Index",
                   "MarathonGroupBy", "MarathonShuffleGroups", "MarathonShuffleItems",
                   "PlaybackOrder", "ProgramScheduleId", "PlaylistId"
               ) VALUES (71, 6, 0, 0, 0, 0, 0, 0, 0, 70, ?);
               INSERT INTO "Playout" (
                   "Id", "ChannelId", "ProgramScheduleId", "ScheduleKind", "Seed"
               ) VALUES (80, 80, 70, 1, 123)"#,
        )
        .bind(playlist_id)
        .execute(&pool)
        .await
        .expect("classic managed-playlist dependency");
        pool.execute(
            r#"INSERT INTO "AddonMediaListItem" (
                   "AddonMediaListId", "ProviderId", "DisplayTitle", "Kind", "Rank",
                   "Title", "SourceUrl", "DurationSeconds", "Published",
                   "ProviderMetadataJson", "ContentKind", "PublishedLiveness"
               ) VALUES (
                   50, 'two', 'Second', 4, 1, 'Second',
                   'https://example.test/two', 60, 1, '{"title":"Second"}', 4, 1
               )"#,
        )
        .await
        .expect("second published add-on list item");

        let interrupted = super::synchronize_remote_stream_playlist_to_boundary(
            &pool,
            1,
            &library_path,
            &manifest_path,
            "unused-ffprobe",
            PlaylistSyncBoundary::DatabaseCommitted,
        )
        .await
        .expect("commit database boundary before simulated crash");
        assert_eq!(interrupted.len(), 1);
        assert_eq!(interrupted[0].playout_id, 80);
        assert_eq!(
            interrupted[0].reason,
            ScanPlayoutRefreshReason::RemoteStreamMembershipAdded
        );
        assert_eq!(
            std::fs::read(&sidecar_path).expect("unpublished sidecar"),
            baseline_sidecar,
            "the ownership sidecar must not move at the database boundary"
        );
        assert!(managed_folder.join("two.yml").is_file());
        let committed_playlist_items: i64 =
            sqlx::query_scalar(r#"SELECT COUNT(*) FROM "PlaylistItem" WHERE "PlaylistId" = ?"#)
                .bind(playlist_id)
                .fetch_one(&pool)
                .await
                .expect("committed playlist items");
        assert_eq!(committed_playlist_items, 2);
        let resolution = sqlx::query(
            r#"SELECT "Generation", "State" FROM "ScanRefreshResolution"
               WHERE "LibraryId" = 1"#,
        )
        .fetch_one(&pool)
        .await
        .expect("unresolved database boundary");
        assert_eq!(resolution.get::<i64, _>("Generation"), 2);
        assert_eq!(resolution.get::<i64, _>("State"), 0);
        let committed_intent = sqlx::query(
            r#"SELECT "PlayoutId", "LibraryId", "Generation", "State", "Reason"
               FROM "ScanPlayoutRefreshIntent""#,
        )
        .fetch_one(&pool)
        .await
        .expect("committed refresh intent");
        assert_eq!(committed_intent.get::<i64, _>("PlayoutId"), 80);
        assert_eq!(committed_intent.get::<Option<i64>, _>("LibraryId"), Some(1));
        assert_eq!(committed_intent.get::<i64, _>("Generation"), 1);
        assert_eq!(committed_intent.get::<i64, _>("State"), 0);
        assert_eq!(
            committed_intent.get::<String, _>("Reason"),
            ScanPlayoutRefreshReason::RemoteStreamMembershipAdded.as_str()
        );

        pool.close().await;
        drop(pool);
        let reopened = file_test_db(&database_path).await;
        let recoverable = ersatzrs_infra::scan_refresh::interrupted_resolutions(&reopened)
            .await
            .expect("load interrupted resolution after restart");
        assert_eq!(recoverable.len(), 1);
        assert_eq!(recoverable[0].library_id, 1);
        assert_eq!(recoverable[0].generation, 2);

        let replayed = super::synchronize_remote_stream_playlist(
            &reopened,
            1,
            &library_path,
            &manifest_path,
            "unused-ffprobe",
        )
        .await
        .expect("replay and publish interrupted boundary");
        assert_eq!(replayed.len(), 1);
        assert_eq!(replayed[0].playout_id, 80);
        ersatzrs_infra::scan_refresh::complete_interrupted_replay(
            &reopened,
            recoverable[0].library_id,
            recoverable[0].generation,
        )
        .await
        .expect("acknowledge replayed interrupted generation");

        let published_sidecar = std::fs::read(&sidecar_path).expect("published replay sidecar");
        assert_ne!(published_sidecar, baseline_sidecar);
        let published_state: serde_json::Value =
            serde_json::from_slice(&published_sidecar).expect("parse published replay sidecar");
        assert!(published_state["entries"]
            .as_array()
            .expect("sidecar entries")
            .iter()
            .any(|entry| entry["provider_id"] == "two"));
        let completed = sqlx::query(
            r#"SELECT "Generation", "State", "Reason", "Diagnostic"
               FROM "ScanRefreshResolution" WHERE "LibraryId" = 1"#,
        )
        .fetch_one(&reopened)
        .await
        .expect("completed replay resolution");
        assert_eq!(completed.get::<i64, _>("Generation"), 3);
        assert_eq!(completed.get::<i64, _>("State"), 1);
        assert_eq!(
            completed.get::<String, _>("Reason"),
            ScanPlayoutRefreshReason::RemoteStreamMembershipAdded.as_str()
        );
        assert!(completed.get::<Option<String>, _>("Diagnostic").is_none());
        let replayed_intents = sqlx::query(
            r#"SELECT "PlayoutId", "LibraryId", "Generation", "State", "Reason"
               FROM "ScanPlayoutRefreshIntent" ORDER BY "PlayoutId""#,
        )
        .fetch_all(&reopened)
        .await
        .expect("replayed refresh intents");
        assert_eq!(replayed_intents.len(), 1);
        assert_eq!(replayed_intents[0].get::<i64, _>("PlayoutId"), 80);
        assert_eq!(
            replayed_intents[0].get::<Option<i64>, _>("LibraryId"),
            Some(1)
        );
        assert_eq!(replayed_intents[0].get::<i64, _>("Generation"), 2);
        assert_eq!(replayed_intents[0].get::<i64, _>("State"), 0);
        assert_eq!(
            replayed_intents[0].get::<String, _>("Reason"),
            ScanPlayoutRefreshReason::RemoteStreamMembershipAdded.as_str()
        );
    }

    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn manifest_first_preserves_old_search_match_for_invalidation() {
        use ersatzrs_domain::ScanPlayoutRefreshReason;
        use sqlx::{Executor, Row as _};

        let temp = tempfile::tempdir().expect("playlist fixture");
        let library_root = temp.path().join("remote-streams");
        let package_root = temp.path().join("addon-package");
        let pool = test_db().await;
        seed_managed_remote_stream_playlist(&pool, &library_root, &package_root).await;

        let first = super::scan_local_library(
            &pool,
            &super::LocalScanConfig {
                library_id: 1,
                ffprobe_path: "unused-ffprobe".to_owned(),
            },
        )
        .await
        .expect("initial playlist scan");
        assert_eq!(first.failed_files, 0, "issues: {:?}", first.issues);
        sqlx::query(r#"UPDATE "AddonMediaList" SET "ReviewState" = 2 WHERE "Id" = 50"#)
            .execute(&pool)
            .await
            .expect("finish staged publication");

        let nfo_path = library_root.join("Fixture").join("one.nfo");
        let old_nfo = std::fs::read_to_string(&nfo_path).expect("generated NFO");
        let new_nfo = old_nfo.replace("<title>OldMatch</title>", "<title>NewMatch</title>");
        assert_ne!(new_nfo, old_nfo);
        std::fs::write(&nfo_path, new_nfo).expect("interrupted-generation NFO");

        let work = super::collect_remote_stream_files(&library_root.display().to_string());
        assert_eq!(work.len(), 2);
        assert!(matches!(
            &work[0],
            super::LocalScanWork::RemoteStreamPlaylist { .. }
        ));
        assert!(matches!(
            &work[1],
            super::LocalScanWork::RemoteStream { .. }
        ));

        pool.execute("PRAGMA foreign_keys = OFF")
            .await
            .expect("disable fixture foreign keys");
        pool.execute(
            r#"INSERT INTO "ProgramSchedule" (
                   "Id", "FixedStartTimeBehavior", "KeepMultiPartEpisodesTogether", "Name",
                   "RandomStartPoint", "ShuffleScheduleItems", "TreatCollectionsAsShows"
               ) VALUES (70, 0, 0, 'Old metadata search', 0, 0, 0);
               INSERT INTO "ProgramScheduleItem" (
                   "Id", "CollectionType", "FillWithGroupMode", "GuideMode", "Index",
                   "MarathonGroupBy", "MarathonShuffleGroups", "MarathonShuffleItems",
                   "PlaybackOrder", "ProgramScheduleId", "SearchQuery"
               ) VALUES (71, 9, 0, 0, 0, 0, 0, 0, 0, 70, 'title = OldMatch');
               INSERT INTO "Playout" (
                   "Id", "ChannelId", "ProgramScheduleId", "ScheduleKind", "Seed"
               ) VALUES (80, 80, 70, 1, 123)"#,
        )
        .await
        .expect("classic search dependency");

        let rescanned = super::scan_local_library(
            &pool,
            &super::LocalScanConfig {
                library_id: 1,
                ffprobe_path: "unused-ffprobe".to_owned(),
            },
        )
        .await
        .expect("manifest-first rescan");
        assert_eq!(rescanned.failed_files, 0, "issues: {:?}", rescanned.issues);
        assert_eq!(rescanned.playout_refreshes.len(), 1);
        assert_eq!(rescanned.playout_refreshes[0].playout_id, 80);
        assert_eq!(
            rescanned.playout_refreshes[0].reason,
            ScanPlayoutRefreshReason::RemoteStreamMembershipReplaced
        );
        let persisted_title: String = sqlx::query_scalar(
            r#"SELECT "Title" FROM "RemoteStreamMetadata" WHERE "RemoteStreamId" = (
                   SELECT "Id" FROM "RemoteStream" WHERE "ProviderId" = 'one'
               )"#,
        )
        .fetch_one(&pool)
        .await
        .expect("rescanned metadata");
        assert_eq!(persisted_title, "NewMatch");
        let intent = sqlx::query(r#"SELECT "PlayoutId", "Reason" FROM "ScanPlayoutRefreshIntent""#)
            .fetch_one(&pool)
            .await
            .expect("durable search invalidation");
        assert_eq!(intent.get::<i64, _>("PlayoutId"), 80);
        assert_eq!(
            intent.get::<String, _>("Reason"),
            ScanPlayoutRefreshReason::RemoteStreamMembershipReplaced.as_str()
        );
        let resolution_reason: String = sqlx::query_scalar(
            r#"SELECT "Reason" FROM "ScanRefreshResolution" WHERE "LibraryId" = 1"#,
        )
        .fetch_one(&pool)
        .await
        .expect("completed refresh resolution");
        assert_eq!(
            resolution_reason,
            ScanPlayoutRefreshReason::RemoteStreamMembershipReplaced.as_str()
        );
    }

    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn other_song_and_image_scans_write_and_replace_uniform_facets() {
        use ersatzrs_domain::{MediaStream, MediaStreamKind};
        use sqlx::{Executor, Row};

        let temp = tempfile::TempDir::new().expect("temp dir");
        let album = temp.path().join("Album Folder");
        std::fs::create_dir_all(&album).expect("album folder");
        let other_path = temp.path().join("Concert.mkv");
        let song_path = album.join("Song.flac");
        let image_path = temp.path().join("Photo.jpg");
        for path in [&other_path, &song_path, &image_path] {
            std::fs::write(path, b"fixture").expect("media fixture");
        }
        std::fs::write(
            temp.path().join("Concert.nfo"),
            r#"<movie><title>Concert</title><mpaa>NL:12</mpaa><studio>Video Studio</studio><label>Video Label</label><language>fr</language><genre>Music</genre><tag>Live</tag><director>Director</director><uniqueid type="imdb">tt1</uniqueid></movie>"#,
        )
        .expect("other nfo");
        std::fs::write(
            album.join("Song.nfo"),
            r"<song><title>Song</title><mpaa>US:PG</mpaa><studio>Song Studio</studio><label>Song Label</label><language>nl</language><genre>Rock</genre><tag>Favorite</tag></song>",
        )
        .expect("song nfo");
        std::fs::write(
            temp.path().join("Photo.nfo"),
            r"<image><title>Photo</title><mpaa>PEGI:3</mpaa><studio>Image Studio</studio><label>Image Label</label><genre>Portrait</genre><tag>Gallery</tag></image>",
        )
        .expect("image nfo");

        let pool = test_db().await;
        pool.execute(
            r#"INSERT INTO "MediaSource" ("Id") VALUES (1);
               INSERT INTO "LocalMediaSource" ("Id") VALUES (1);
               INSERT INTO "Library" ("Id", "Name", "MediaKind", "MediaSourceId")
               VALUES (1, 'Uniform Facets', 4, 1);
               INSERT INTO "LocalLibrary" ("Id") VALUES (1)"#,
        )
        .await
        .expect("library");
        sqlx::query(r#"INSERT INTO "LibraryPath" ("Id", "LibraryId", "Path") VALUES (1, 1, ?)"#)
            .bind(temp.path().display().to_string())
            .execute(&pool)
            .await
            .expect("library path");
        pool.execute(
            r#"INSERT INTO "MediaItem" ("Id", "LibraryPathId", "State") VALUES (101, 1, 0), (102, 1, 0), (103, 1, 0);
               INSERT INTO "OtherVideo" ("Id") VALUES (101);
               INSERT INTO "OtherVideoMetadata" ("Id", "OtherVideoId", "MetadataKind", "Title", "DateAdded", "DateUpdated")
               VALUES (201, 101, 0, 'Old Other', '2026-08-05T00:00:00Z', '2026-08-05T00:00:00Z');
               INSERT INTO "Song" ("Id") VALUES (102);
               INSERT INTO "SongMetadata" ("Id", "SongId", "MetadataKind", "Title", "DateAdded", "DateUpdated")
               VALUES (202, 102, 0, 'Old Song', '2026-08-05T00:00:00Z', '2026-08-05T00:00:00Z');
               INSERT INTO "Image" ("Id") VALUES (103);
               INSERT INTO "ImageMetadata" ("Id", "ImageId", "MetadataKind", "Title", "DateAdded", "DateUpdated")
               VALUES (203, 103, 0, 'Old Image', '2026-08-05T00:00:00Z', '2026-08-05T00:00:00Z')"#,
        )
        .await
        .expect("media rows");
        let library_path = super::LocalLibraryPath {
            id: 1,
            path: temp.path().display().to_string(),
            media_kind: LibraryMediaKind::OtherVideos,
        };
        let mut probed = probed_version();
        probed
            .format_metadata
            .insert("publisher".to_owned(), "Embedded Label".to_owned());
        probed.streams.push(MediaStream {
            kind: MediaStreamKind::Audio,
            language: Some("eng".to_owned()),
            ..MediaStream::default()
        });

        let mut tx = pool.begin().await.expect("begin other");
        super::enrich_other_video_metadata(&mut tx, 101, &library_path, &other_path, &probed)
            .await
            .expect("other facets");
        tx.commit().await.expect("commit other");
        let mut tx = pool.begin().await.expect("begin song");
        super::enrich_song_metadata(&mut tx, 102, &library_path, &song_path, &probed)
            .await
            .expect("song facets");
        tx.commit().await.expect("commit song");
        let mut tx = pool.begin().await.expect("begin image");
        super::enrich_image_metadata(&mut tx, 103, &library_path, &image_path, &probed)
            .await
            .expect("image facets");
        tx.commit().await.expect("commit image");

        for (table, owner, metadata_id, expected_count) in [
            ("Studio", "OtherVideoMetadataId", 201_i64, 1_i64),
            ("MusicLabel", "OtherVideoMetadataId", 201, 1),
            ("Genre", "OtherVideoMetadataId", 201, 1),
            ("Tag", "OtherVideoMetadataId", 201, 1),
            ("Studio", "SongMetadataId", 202, 1),
            ("MusicLabel", "SongMetadataId", 202, 1),
            ("Genre", "SongMetadataId", 202, 1),
            ("Tag", "SongMetadataId", 202, 2),
            ("Studio", "ImageMetadataId", 203, 1),
            ("MusicLabel", "ImageMetadataId", 203, 1),
            ("Genre", "ImageMetadataId", 203, 1),
            ("Tag", "ImageMetadataId", 203, 1),
        ] {
            let sql = format!(r#"SELECT COUNT(*) FROM "{table}" WHERE "{owner}" = ?"#);
            let count: i64 = sqlx::query_scalar(ersatzrs_infra::db::assert_sql_safe(sql))
                .bind(metadata_id)
                .fetch_one(&pool)
                .await
                .expect("facet count");
            assert_eq!(count, expected_count, "unexpected {table} for {owner}");
        }
        let ratings = sqlx::query(
            r#"SELECT
                (SELECT "ContentRating" FROM "OtherVideoMetadata" WHERE "Id" = 201) AS "OtherRating",
                (SELECT "ContentRating" FROM "SongMetadata" WHERE "Id" = 202) AS "SongRating",
                (SELECT "ContentRating" FROM "ImageMetadata" WHERE "Id" = 203) AS "ImageRating""#,
        )
        .fetch_one(&pool)
        .await
        .expect("ratings");
        assert_eq!(ratings.get::<String, _>("OtherRating"), "NL:12");
        assert_eq!(ratings.get::<String, _>("SongRating"), "US:PG");
        assert_eq!(ratings.get::<String, _>("ImageRating"), "PEGI:3");
        let image_language_count: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM "MetadataLanguage" WHERE "ImageMetadataId" = 203"#,
        )
        .fetch_one(&pool)
        .await
        .expect("image languages");
        assert_eq!(image_language_count, 0, "image must ignore stream language");
        let song_languages: Vec<String> = sqlx::query_scalar(
            r#"SELECT "Code" FROM "MetadataLanguage" WHERE "SongMetadataId" = 202 ORDER BY "Id""#,
        )
        .fetch_all(&pool)
        .await
        .expect("song languages");
        assert_eq!(song_languages, ["dut", "eng"]);

        std::fs::write(
            temp.path().join("Concert.nfo"),
            r"<movie><title>Concert</title></movie>",
        )
        .expect("replace other nfo");
        let mut tx = pool.begin().await.expect("begin rescan");
        super::enrich_other_video_metadata(&mut tx, 101, &library_path, &other_path, &probed)
            .await
            .expect("other rescan");
        tx.commit().await.expect("commit rescan");
        let music_label: String = sqlx::query_scalar(
            r#"SELECT "Name" FROM "MusicLabel" WHERE "OtherVideoMetadataId" = 201"#,
        )
        .fetch_one(&pool)
        .await
        .expect("embedded music label restored");
        assert_eq!(music_label, "Embedded Label");
        let stale_studios: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM "Studio" WHERE "OtherVideoMetadataId" = 201"#,
        )
        .fetch_one(&pool)
        .await
        .expect("stale studios");
        assert_eq!(stale_studios, 0);
    }

    fn seed_music_video_library_path(path: &Path) -> super::LocalLibraryPath {
        super::LocalLibraryPath {
            id: 1,
            path: path.display().to_string(),
            media_kind: LibraryMediaKind::MusicVideos,
        }
    }

    async fn seed_music_video_library(
        pool: &sqlx::SqlitePool,
        library_path: &super::LocalLibraryPath,
    ) {
        seed_local_library(pool, library_path, "Music Videos", 3).await;
    }

    async fn seed_song_library(pool: &sqlx::SqlitePool, path: &Path) -> super::LocalLibraryPath {
        let library_path = super::LocalLibraryPath {
            id: 1,
            path: path.display().to_string(),
            media_kind: LibraryMediaKind::Songs,
        };
        seed_local_library(pool, &library_path, "Songs", 5).await;
        library_path
    }

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

    async fn assert_primary_and_remaster_alternate(
        pool: &sqlx::SqlitePool,
        primary_path: &Path,
        remaster_path: &Path,
    ) {
        use sqlx::Row;

        let primary_id = media_item_id_for_path(pool, primary_path).await;
        let remaster_id = media_item_id_for_path(pool, remaster_path).await;
        let primary = sqlx::query(
            r#"SELECT "ContentKind", "SubKind", "ParentMediaItemId"
               FROM "MediaItem" WHERE "Id" = ?"#,
        )
        .bind(primary_id)
        .fetch_one(pool)
        .await
        .expect("primary row");
        assert_eq!(
            primary.get::<i32, _>("ContentKind"),
            ersatzrs_domain::ContentKind::Primary.db_value()
        );
        assert!(primary.get::<Option<String>, _>("SubKind").is_none());
        assert!(primary.get::<Option<i64>, _>("ParentMediaItemId").is_none());

        let remaster = sqlx::query(
            r#"SELECT "ContentKind", "SubKind", "ParentMediaItemId"
               FROM "MediaItem" WHERE "Id" = ?"#,
        )
        .bind(remaster_id)
        .fetch_one(pool)
        .await
        .expect("remaster row");
        assert_eq!(
            remaster.get::<i32, _>("ContentKind"),
            ersatzrs_domain::ContentKind::AlternateVersion.db_value()
        );
        assert_eq!(
            remaster.get::<Option<String>, _>("SubKind").as_deref(),
            Some("remaster")
        );
        assert_eq!(
            remaster.get::<Option<i64>, _>("ParentMediaItemId"),
            Some(primary_id)
        );
    }

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

    async fn count_table(pool: &sqlx::SqlitePool, table: &str) -> i64 {
        let sql = format!(r#"SELECT COUNT(*) FROM "{table}""#);
        sqlx::query_scalar(ersatzrs_infra::db::assert_sql_safe(sql))
            .fetch_one(pool)
            .await
            .expect("count")
    }

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

    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn managed_playlist_keeps_identity_and_replaces_upstream_order() {
        use super::synchronize_managed_playlist;
        use ersatzrs_domain::ScanPlayoutRefreshReason;
        use sqlx::sqlite::SqlitePoolOptions;
        use sqlx::Row as _;

        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("connect sqlite");
        sqlx::migrate!("../../migrations")
            .run(&pool)
            .await
            .expect("migrate");
        sqlx::query("PRAGMA foreign_keys = OFF")
            .execute(&pool)
            .await
            .expect("disable fixture foreign keys");
        let group_result =
            sqlx::query(r#"INSERT INTO "PlaylistGroup" ("Name", "IsSystem") VALUES ('System', 1)"#)
                .execute(&pool)
                .await
                .expect("system playlist group");
        let system_group = ersatzrs_infra::db::last_insert_id(group_result);
        sqlx::query(
            r#"INSERT INTO "Playlist" ("IsSystem", "Name", "PlaylistGroupId", "ManagedSource")
               VALUES (0, 'User playlist', ?, NULL)"#,
        )
        .bind(system_group)
        .execute(&pool)
        .await
        .expect("user playlist");

        let source = "remote-stream-playlist:v1:7:Glamourland.playlist.yml";
        synchronize_managed_playlist(
            &pool,
            source,
            "Glamourland",
            true,
            &[101, 102, 103],
            true,
            None,
        )
        .await
        .expect("initial materialization");
        let playlist_id: i64 =
            sqlx::query_scalar(r#"SELECT "Id" FROM "Playlist" WHERE "ManagedSource" = ?"#)
                .bind(source)
                .fetch_one(&pool)
                .await
                .expect("managed playlist");
        let generation = ersatzrs_infra::scan_refresh::begin_resolution(
            &pool,
            7,
            ScanPlayoutRefreshReason::RemoteStreamMembershipReordered,
        )
        .await
        .expect("begin refresh resolution");
        synchronize_managed_playlist(
            &pool,
            source,
            "GLAMOURLAND",
            true,
            &[103, 101],
            true,
            Some((
                7,
                ScanPlayoutRefreshReason::RemoteStreamMembershipReordered,
                &[77],
            )),
        )
        .await
        .expect("reorder materialization");
        let resolving_state: i64 = sqlx::query_scalar(
            r#"SELECT "State" FROM "ScanRefreshResolution" WHERE "LibraryId" = 7"#,
        )
        .fetch_one(&pool)
        .await
        .expect("resolving refresh resolution");
        assert_eq!(resolving_state, 0);
        super::complete_scan_refresh_resolution(
            &pool,
            7,
            generation,
            ScanPlayoutRefreshReason::RemoteStreamMembershipReordered,
        )
        .await
        .expect("complete refresh resolution after publication boundary");
        synchronize_managed_playlist(&pool, source, "GLAMOURLAND", true, &[103, 101], false, None)
            .await
            .expect("unchanged materialization");
        let intent: (i64, String) =
            sqlx::query_as(r#"SELECT "PlayoutId", "Reason" FROM "ScanPlayoutRefreshIntent""#)
                .fetch_one(&pool)
                .await
                .expect("durable refresh intent");
        assert_eq!(intent.0, 77);
        assert_eq!(
            intent.1,
            ScanPlayoutRefreshReason::RemoteStreamMembershipReordered.as_str()
        );
        let resolution_state: i64 = sqlx::query_scalar(
            r#"SELECT "State" FROM "ScanRefreshResolution" WHERE "LibraryId" = 7"#,
        )
        .fetch_one(&pool)
        .await
        .expect("completed refresh resolution");
        assert_eq!(resolution_state, 1);

        let row = sqlx::query(
            r#"SELECT "Id", "Name", "IsSystem" FROM "Playlist" WHERE "ManagedSource" = ?"#,
        )
        .bind(source)
        .fetch_one(&pool)
        .await
        .expect("managed row");
        assert_eq!(row.get::<i64, _>("Id"), playlist_id);
        assert_eq!(row.get::<String, _>("Name"), "GLAMOURLAND");
        assert_eq!(row.get::<i64, _>("IsSystem"), 1);
        let items = sqlx::query(
            r#"SELECT "Index", "MediaItemId", "CollectionType"
               FROM "PlaylistItem" WHERE "PlaylistId" = ? ORDER BY "Index""#,
        )
        .bind(playlist_id)
        .fetch_all(&pool)
        .await
        .expect("managed items");
        assert_eq!(
            items
                .iter()
                .map(|row| (
                    row.get::<i64, _>("Index"),
                    row.get::<i64, _>("MediaItemId"),
                    row.get::<i64, _>("CollectionType")
                ))
                .collect::<Vec<_>>(),
            [(0, 103, 70), (1, 101, 70)]
        );

        synchronize_managed_playlist(&pool, source, "GLAMOURLAND", false, &[], false, None)
            .await
            .expect("disable managed playlist");
        let managed_count: i64 =
            sqlx::query_scalar(r#"SELECT COUNT(*) FROM "Playlist" WHERE "ManagedSource" = ?"#)
                .bind(source)
                .fetch_one(&pool)
                .await
                .expect("managed count");
        let user_count: i64 =
            sqlx::query_scalar(r#"SELECT COUNT(*) FROM "Playlist" WHERE "Name" = 'User playlist'"#)
                .fetch_one(&pool)
                .await
                .expect("user count");
        assert_eq!(managed_count, 0);
        assert_eq!(user_count, 1);
    }

    #[test]
    fn scan_refresh_reason_requires_stable_identity_proof_for_reidentification() {
        let replacement = remote_stream_playlist::PlaylistMembershipDelta {
            changed_provider_ids: vec!["stable-provider".to_owned()],
            ..Default::default()
        };
        assert_eq!(
            membership_refresh_reason(&replacement, false),
            ScanPlayoutRefreshReason::RemoteStreamMembershipReplaced
        );
        assert_eq!(
            membership_refresh_reason(&replacement, true),
            ScanPlayoutRefreshReason::RemoteStreamMembershipReidentified
        );

        let unrelated_add_remove = remote_stream_playlist::PlaylistMembershipDelta {
            added_provider_ids: vec!["new-provider".to_owned()],
            removed_provider_ids: vec!["old-provider".to_owned()],
            ..Default::default()
        };
        assert_eq!(
            membership_refresh_reason(&unrelated_add_remove, false),
            ScanPlayoutRefreshReason::RemoteStreamMembershipMixed
        );
    }

    #[tokio::test]
    async fn prepared_remote_stream_duration_reaches_media_version_without_changing_yaml_identity()
    {
        use super::{upsert_local_remote_stream, LocalLibraryPath};
        use ersatzrs_domain::RemoteStreamLiveness;
        use ersatzrs_infra::remote_stream::{
            RemoteStreamDefinition, RemoteStreamSource, ResolvedRemoteStream,
        };
        use sqlx::sqlite::SqlitePoolOptions;
        use sqlx::Executor as _;

        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
        pool.execute("PRAGMA foreign_keys = OFF").await.unwrap();
        let root = tempfile::tempdir().unwrap();
        let yaml_path = root.path().join("prepared.yml");
        let manifest = super::remote_stream_playlist::parse_playlist_manifest(
            "url: https://example.test/list\naddon: org.ersatzrs.addon.fixture\n",
            &root.path().join("Fixture.playlist.yml"),
        )
        .unwrap();
        let mut entry = super::remote_stream_playlist::PlaylistEntry::new(
            "item",
            "https://example.test/item",
            "Prepared",
        );
        entry.duration_seconds = Some(120);
        entry.liveness = Some(ersatzrs_addon_contract::RemoteStreamLiveness::Finite);
        let bytes = super::remote_stream_playlist::prepared_definition_bytes_for_test(
            &manifest, &entry, 90_250,
        )
        .unwrap();
        tokio::fs::write(&yaml_path, &bytes).await.unwrap();
        let definition: RemoteStreamDefinition = serde_yaml::from_slice(&bytes).unwrap();
        assert_eq!(definition.duration.as_deref(), Some("00:01:30.250"));
        let resolved = ResolvedRemoteStream {
            source: RemoteStreamSource::Url("https://example.test/item".into()),
            liveness: RemoteStreamLiveness::Finite,
            is_live: false,
            configured_duration: definition.duration.clone(),
            fallback_query: None,
            uses_addon: false,
        };
        let library_path = LocalLibraryPath {
            id: 1,
            path: root.path().display().to_string(),
            media_kind: LibraryMediaKind::RemoteStreams,
        };
        let first = upsert_local_remote_stream(
            &pool,
            &library_path,
            &yaml_path,
            &definition,
            &resolved,
            None,
        )
        .await
        .unwrap();
        let second = upsert_local_remote_stream(
            &pool,
            &library_path,
            &yaml_path,
            &definition,
            &resolved,
            None,
        )
        .await
        .unwrap();
        assert_eq!(first, second);
        let rows: Vec<(String, String)> = sqlx::query_as(r#"SELECT "version"."Duration", "file"."Path"
            FROM "MediaVersion" AS "version" JOIN "MediaFile" AS "file" ON "file"."MediaVersionId"="version"."Id"
            WHERE "version"."RemoteStreamId"=?"#).bind(first).fetch_all(&pool).await.unwrap();
        assert_eq!(
            rows,
            vec![("00:01:30.250".into(), yaml_path.display().to_string())]
        );
        assert_eq!(tokio::fs::read(&yaml_path).await.unwrap(), bytes);
        assert_eq!(entry.duration_seconds, Some(120));
    }

    #[tokio::test]
    async fn remote_stream_provider_identity_survives_generated_path_change() {
        use super::{upsert_local_remote_stream, LocalLibraryPath};
        use ersatzrs_domain::{AddonMediaListItemAvailability, RemoteStreamLiveness};
        use ersatzrs_infra::remote_stream::{
            RemoteStreamDefinition, RemoteStreamSource, ResolvedRemoteStream,
        };
        use sqlx::sqlite::SqlitePoolOptions;
        use sqlx::{Executor, Row};

        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("connect sqlite");
        sqlx::migrate!("../../migrations")
            .run(&pool)
            .await
            .expect("migrate");
        pool.execute("PRAGMA foreign_keys = OFF")
            .await
            .expect("disable fixture foreign keys");
        let root = tempfile::tempdir().expect("remote stream root");
        let library_path = LocalLibraryPath {
            id: 1,
            path: root.path().display().to_string(),
            media_kind: LibraryMediaKind::RemoteStreams,
        };
        let definition = RemoteStreamDefinition {
            provider_id: Some("stable-provider".to_owned()),
            addon: Some("org.ersatzrs.test".to_owned()),
            url: Some("https://example.test/stream".to_owned()),
            duration: Some("00:01:00".to_owned()),
            liveness: Some(RemoteStreamLiveness::Finite),
            title: Some("Stable".to_owned()),
            availability: AddonMediaListItemAvailability::Available,
            ..RemoteStreamDefinition::default()
        };
        let resolved = ResolvedRemoteStream {
            source: RemoteStreamSource::Url("https://example.test/stream".to_owned()),
            liveness: RemoteStreamLiveness::Finite,
            is_live: false,
            configured_duration: Some("00:01:00".to_owned()),
            fallback_query: None,
            uses_addon: false,
        };
        let first_path = root.path().join("old-generated-id.yml");
        let second_path = root.path().join("new-generated-id.yml");
        let third_path = root.path().join("case-only-generated-id.yml");

        let first_id = upsert_local_remote_stream(
            &pool,
            &library_path,
            &first_path,
            &definition,
            &resolved,
            None,
        )
        .await
        .expect("initial remote stream");
        let second_id = upsert_local_remote_stream(
            &pool,
            &library_path,
            &second_path,
            &definition,
            &resolved,
            None,
        )
        .await
        .expect("reidentified generated path");
        let mut case_changed_definition = definition.clone();
        case_changed_definition.provider_id = Some("Stable-Provider".to_owned());
        let third_id = upsert_local_remote_stream(
            &pool,
            &library_path,
            &third_path,
            &case_changed_definition,
            &resolved,
            None,
        )
        .await
        .expect("case-only provider identity change");

        assert_eq!(second_id, first_id);
        assert_eq!(third_id, first_id);
        let row = sqlx::query(
            r#"SELECT COUNT(*) AS "Count", MAX("mf"."Path") AS "Path"
               FROM "RemoteStream" AS "rs"
               JOIN "MediaVersion" AS "mv" ON "mv"."RemoteStreamId" = "rs"."Id"
               JOIN "MediaFile" AS "mf" ON "mf"."MediaVersionId" = "mv"."Id""#,
        )
        .fetch_one(&pool)
        .await
        .expect("remote stream identity rows");
        assert_eq!(row.get::<i64, _>("Count"), 1);
        assert_eq!(
            row.get::<String, _>("Path"),
            third_path.display().to_string()
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn remote_stream_playlist_scan_materializes_definitions_tags_and_order() {
        use super::{scan_local_library, LocalScanConfig};
        use ersatzrs_core::scheduling::SchedulingContentSource;
        use sqlx::sqlite::SqlitePoolOptions;
        use sqlx::Row as _;
        use std::os::unix::fs::PermissionsExt as _;

        let temp = tempfile::TempDir::new().expect("temp dir");
        let library_root = temp.path().join("remote-streams");
        std::fs::create_dir_all(&library_root).expect("library root");
        let package_root = temp.path().join("addon-package");
        std::fs::create_dir_all(&package_root).expect("package root");
        let script = package_root.join("fixture.sh");
        std::fs::write(
            &script,
            concat!(
                "#!/bin/sh\n",
                "printf '%s\\n' \\\n",
                "  '{\"id\":\"first-chapter\",\"provider_id\":\"item:first:chapter:0\",\"url\":\"https://example.test/1?start=0&end=30\",\"title\":\"First - Intro\",\"duration_seconds\":30,\"parent_provider_id\":\"item:first\",\"fragment_start_seconds\":0,\"fragment_end_seconds\":30}' \\\n",
                "  '{\"id\":\"first\",\"provider_id\":\"item:first\",\"url\":\"https://example.test/1\",\"title\":\"First\",\"original_title\":\"Original First\",\"sort_title\":\"A First\",\"outline\":\"First outline\",\"plot\":\"First plot\",\"tagline\":\"First tagline\",\"duration_seconds\":60,\"year\":2001,\"release_date\":\"2001-02-03T04:05:06Z\",\"content_rating\":\"12\",\"collection\":\"Example Collection\",\"original_broadcasters\":[\"Original Network\"],\"broadcasters\":[\"Current Network\"],\"content_kind\":\"other_video\"}' \\\n",
                "  '{\"id\":\"second\",\"url\":\"https://example.test/2\",\"title\":\"Second\",\"duration_seconds\":120}'\n"
            ),
        )
        .expect("fixture add-on");
        let mut permissions = std::fs::metadata(&script)
            .expect("add-on metadata")
            .permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&script, permissions).expect("add-on permissions");
        let manifest_path = library_root.join("Fixture.playlist.yml");
        std::fs::write(
            &manifest_path,
            r"name: Fixture order
url: https://example.test/playlist
addon: org.ersatzrs.addon.fixture
sync:
  interval: 24h
  max_items: 10
  order: upstream
  on_removed: trash
create_playlist: true
tags: [Fixture Tag]
defaults:
  content_rating: TV-G
  year: 1999
",
        )
        .expect("manifest");

        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("connect sqlite");
        sqlx::migrate!("../../migrations")
            .run(&pool)
            .await
            .expect("migrate");
        sqlx::query(r#"INSERT INTO "MediaSource" ("Id") VALUES (1)"#)
            .execute(&pool)
            .await
            .expect("media source");
        sqlx::query(r#"INSERT INTO "LocalMediaSource" ("Id") VALUES (1)"#)
            .execute(&pool)
            .await
            .expect("local source");
        sqlx::query(
            r#"INSERT INTO "Library" ("Id", "Name", "MediaKind", "MediaSourceId")
               VALUES (1, 'Remote Streams', 7, 1)"#,
        )
        .execute(&pool)
        .await
        .expect("library");
        sqlx::query(r#"INSERT INTO "LocalLibrary" ("Id") VALUES (1)"#)
            .execute(&pool)
            .await
            .expect("local library");
        sqlx::query(r#"INSERT INTO "LibraryPath" ("Id", "LibraryId", "Path") VALUES (1, 1, ?)"#)
            .bind(library_root.display().to_string())
            .execute(&pool)
            .await
            .expect("library path");
        let addon_manifest = serde_json::json!({
            "manifest_version": 1,
            "id": "org.ersatzrs.addon.fixture",
            "version": "1.0.0",
            "name": {"en-US": "Fixture"},
            "summary": {"en-US": "Fixture playlist add-on"},
            "license": "GPL-3.0-or-later",
            "source_url": "https://example.test/source",
            "host_version": "*",
            "capabilities": [
                {"id": "addon.check.v1"},
                {"id": "remote-stream.list.v3"},
                {"id": "remote-stream.play.v1"}
            ],
            "entrypoints": {
                "any": {"path": "fixture.sh", "kind": "posix_shell"}
            },
            "permissions": {},
            "settings": [],
            "panels": []
        });
        let now = chrono::Utc::now().to_rfc3339();
        let content_sha256 = ersatzrs_infra::addons::calculate_addon_content_sha256(&package_root)
            .await
            .expect("hash fixture package");
        sqlx::query(
            r#"INSERT INTO "AddonInstallation" (
                   "AddonId", "RepositoryId", "Version", "Enabled", "PackageSha256", "ContentSha256",
                   "ManifestJson", "InstallPath", "InstalledAt", "UpdatedAt"
               ) VALUES (?, NULL, '1.0.0', 1, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind("org.ersatzrs.addon.fixture")
        .bind("a".repeat(64))
        .bind(content_sha256)
        .bind(addon_manifest.to_string())
        .bind(package_root.to_string_lossy().into_owned())
        .bind(&now)
        .bind(&now)
        .execute(&pool)
        .await
        .expect("add-on installation");
        sqlx::query(
            r#"INSERT INTO "AddonMediaList" (
                   "Id", "AddonId", "ProviderId", "SourceUrl", "Name",
                   "ContentKindOverride", "ReviewState"
               ) VALUES (50, 'org.ersatzrs.addon.fixture', 'fixture/list',
                         'https://example.test/playlist', 'Fixture order', 0, 2)"#,
        )
        .execute(&pool)
        .await
        .expect("saved add-on list");
        sqlx::query(
            r#"INSERT INTO "AddonMediaListLibrary" (
                   "AddonMediaListId", "LibraryId", "FolderName", "ManifestPath"
               ) VALUES (50, 1, 'Fixture', ?)"#,
        )
        .bind(manifest_path.to_string_lossy().into_owned())
        .execute(&pool)
        .await
        .expect("managed list library");
        let published_metadata = [
            (
                "item:first:chapter:0",
                "First - Intro",
                "https://example.test/1?start=0&end=30",
                serde_json::json!({"title": "First - Intro"}),
            ),
            (
                "item:first",
                "First",
                "https://example.test/1",
                serde_json::json!({
                    "title": "First",
                    "original_title": "Original First",
                    "sort_title": "A First",
                    "outline": "First outline",
                    "plot": "First plot",
                    "tagline": "First tagline",
                    "year": 2001,
                    "release_date": "2001-02-03T04:05:06Z",
                    "content_ratings": ["12"],
                    "collection": "Example Collection",
                    "original_broadcasters": ["Original Network"],
                    "broadcasters": ["Current Network"]
                }),
            ),
            (
                "second",
                "Second",
                "https://example.test/2",
                serde_json::json!({"title": "Second"}),
            ),
        ];
        for (rank, (provider_id, title, source_url, metadata)) in
            published_metadata.into_iter().enumerate()
        {
            sqlx::query(
                r#"INSERT INTO "AddonMediaListItem" (
                       "AddonMediaListId", "ProviderId", "DisplayTitle", "Kind", "Rank",
                       "Title", "SourceUrl", "Published", "ProviderMetadataJson"
                   ) VALUES (50, ?, ?, 4, ?, ?, ?, 1, ?)"#,
            )
            .bind(provider_id)
            .bind(title)
            .bind(i32::try_from(rank).expect("published rank"))
            .bind(title)
            .bind(source_url)
            .bind(metadata.to_string())
            .execute(&pool)
            .await
            .expect("published list item");
        }

        let report = scan_local_library(
            &pool,
            &LocalScanConfig {
                library_id: 1,
                ffprobe_path: "unused-ffprobe".to_owned(),
            },
        )
        .await
        .expect("playlist scan");
        assert_eq!(report.failed_files, 0, "issues: {:?}", report.issues);
        assert_eq!(report.scanned_files, 1);
        let metadata = sqlx::query(
            r#"SELECT "rsm"."Title", "rsm"."OriginalTitle", "rsm"."SortTitle",
                      "rsm"."Outline", "rsm"."Plot", "rsm"."Tagline", "rsm"."Year",
                      "rsm"."ReleaseDate", "rsm"."ContentRating", "rsm"."CollectionName",
                      "rsm"."OriginalBroadcasters", "rsm"."Broadcasters"
               FROM "RemoteStreamMetadata" AS "rsm"
               JOIN "RemoteStream" AS "rs" ON "rs"."Id" = "rsm"."RemoteStreamId"
               WHERE "rs"."ProviderId" = 'item:first'"#,
        )
        .fetch_one(&pool)
        .await
        .expect("first metadata");
        assert_eq!(metadata.get::<String, _>("Title"), "First");
        assert_eq!(metadata.get::<String, _>("OriginalTitle"), "Original First");
        assert_eq!(metadata.get::<String, _>("SortTitle"), "A First");
        assert_eq!(metadata.get::<String, _>("Outline"), "First outline");
        assert_eq!(metadata.get::<String, _>("Plot"), "First plot");
        assert_eq!(metadata.get::<String, _>("Tagline"), "First tagline");
        assert_eq!(metadata.get::<i32, _>("Year"), 2001);
        assert_eq!(
            metadata.get::<String, _>("ReleaseDate"),
            "2001-02-03T04:05:06+00:00"
        );
        assert_eq!(metadata.get::<String, _>("ContentRating"), "12");
        assert_eq!(
            metadata.get::<String, _>("CollectionName"),
            "Example Collection"
        );
        assert_eq!(
            metadata.get::<String, _>("OriginalBroadcasters"),
            r#"["Original Network"]"#
        );
        assert_eq!(
            metadata.get::<String, _>("Broadcasters"),
            r#"["Current Network"]"#
        );
        let second_defaults = sqlx::query_as::<_, (Option<i32>, Option<String>)>(
            r#"SELECT "rsm"."Year", "rsm"."ContentRating"
               FROM "RemoteStreamMetadata" AS "rsm"
               JOIN "RemoteStream" AS "rs" ON "rs"."Id" = "rsm"."RemoteStreamId"
               WHERE "rs"."ProviderId" = 'second'"#,
        )
        .fetch_one(&pool)
        .await
        .expect("second metadata defaults");
        assert_eq!(second_defaults, (Some(1999), Some("TV-G".to_owned())));
        let full_item_id: i64 = sqlx::query_scalar(
            r#"SELECT "Id" FROM "RemoteStream" WHERE "ProviderId" = 'item:first'"#,
        )
        .fetch_one(&pool)
        .await
        .expect("full episode item");
        let chapter_item_id: i64 = sqlx::query_scalar(
            r#"SELECT "Id" FROM "RemoteStream" WHERE "ProviderId" = 'item:first:chapter:0'"#,
        )
        .fetch_one(&pool)
        .await
        .expect("chapter item");
        let chapter_parent_id: i64 =
            sqlx::query_scalar(r#"SELECT "ParentMediaItemId" FROM "MediaItem" WHERE "Id" = ?"#)
                .bind(chapter_item_id)
                .fetch_one(&pool)
                .await
                .expect("chapter parent");
        assert_ne!(full_item_id, chapter_item_id);
        assert_eq!(chapter_parent_id, full_item_id);
        sqlx::query(
            r#"INSERT INTO "Collection" ("Id", "Name", "UseCustomPlaybackOrder")
               VALUES (80, 'Episode and chapter', 0)"#,
        )
        .execute(&pool)
        .await
        .expect("test collection");
        for (index, media_item_id) in [full_item_id, chapter_item_id].into_iter().enumerate() {
            sqlx::query(
                r#"INSERT INTO "CollectionItem" ("CollectionId", "MediaItemId", "CustomIndex")
                   VALUES (80, ?, ?)"#,
            )
            .bind(media_item_id)
            .bind(i32::try_from(index).expect("collection index"))
            .execute(&pool)
            .await
            .expect("independent collection item");
        }
        let collection_item_count: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM "CollectionItem" WHERE "CollectionId" = 80"#,
        )
        .fetch_one(&pool)
        .await
        .expect("collection item count");
        assert_eq!(collection_item_count, 2);

        for (override_value, expected_kind, expected_root) in [
            (1, 1, "episodedetails"),
            (2, 2, "movie"),
            (3, 3, "musicvideo"),
            (4, 4, "movie"),
            (0, 4, "movie"),
        ] {
            sqlx::query(r#"UPDATE "AddonMediaList" SET "ContentKindOverride" = ? WHERE "Id" = 50"#)
                .bind(override_value)
                .execute(&pool)
                .await
                .expect("content-kind override");
            let override_scan = scan_local_library(
                &pool,
                &LocalScanConfig {
                    library_id: 1,
                    ffprobe_path: "unused-ffprobe".to_owned(),
                },
            )
            .await
            .expect("override rescan");
            assert_eq!(
                override_scan.failed_files, 0,
                "issues: {:?}",
                override_scan.issues
            );
            let persisted_kind: i32 = sqlx::query_scalar(
                r#"SELECT "ContentKind" FROM "RemoteStream" WHERE "ProviderId" = 'item:first'"#,
            )
            .fetch_one(&pool)
            .await
            .expect("persisted content kind");
            assert_eq!(persisted_kind, expected_kind);
            let nfo = std::fs::read_to_string(library_root.join("Fixture").join("first.nfo"))
                .expect("generated NFO");
            assert!(nfo.contains(&format!("<{expected_root} ")));
        }

        std::fs::write(
            &script,
            concat!(
                "#!/bin/sh\n",
                "printf '%s\\n' \\\n",
                "  '{\"id\":\"first-chapter\",\"provider_id\":\"item:first:chapter:0\",\"url\":\"https://example.test/1?start=0&end=30\",\"title\":\"Updated First - Intro\",\"duration_seconds\":30,\"parent_provider_id\":\"item:first\",\"fragment_start_seconds\":0,\"fragment_end_seconds\":30}' \\\n",
                "  '{\"id\":\"first\",\"provider_id\":\"item:first\",\"url\":\"https://example.test/1\",\"title\":\"Updated First\",\"original_title\":\"Updated Original\",\"sort_title\":\"Updated Sort\",\"outline\":\"Updated outline\",\"plot\":\"Updated plot\",\"tagline\":\"Updated tagline\",\"duration_seconds\":60,\"year\":2002,\"release_date\":\"2002-03-04T05:06:07Z\",\"content_rating\":\"16\",\"collection\":\"Updated Collection\",\"original_broadcasters\":[\"Earlier Network\"],\"broadcasters\":[\"Later Network\"],\"content_kind\":\"other_video\"}' \\\n",
                "  '{\"id\":\"second\",\"url\":\"https://example.test/2\",\"title\":\"Second\",\"duration_seconds\":120}'\n"
            ),
        )
        .expect("update fixture add-on");
        let updated_content_sha256 =
            ersatzrs_infra::addons::calculate_addon_content_sha256(&package_root)
                .await
                .expect("hash updated fixture package");
        sqlx::query(
            r#"UPDATE "AddonInstallation"
               SET "PackageSha256" = ?, "ContentSha256" = ?
               WHERE "AddonId" = 'org.ersatzrs.addon.fixture'"#,
        )
        .bind("b".repeat(64))
        .bind(updated_content_sha256)
        .execute(&pool)
        .await
        .expect("update fixture installation integrity");
        for (provider_id, metadata) in [
            (
                "item:first:chapter:0",
                serde_json::json!({"title": "Updated First - Intro"}),
            ),
            (
                "item:first",
                serde_json::json!({
                    "title": "Updated First",
                    "original_title": "Updated Original",
                    "sort_title": "Updated Sort",
                    "outline": "Updated outline",
                    "plot": "Updated plot",
                    "tagline": "Updated tagline",
                    "year": 2002,
                    "release_date": "2002-03-04T05:06:07Z",
                    "content_ratings": ["16"],
                    "collection": "Updated Collection",
                    "original_broadcasters": ["Earlier Network"],
                    "broadcasters": ["Later Network"]
                }),
            ),
        ] {
            sqlx::query(
                r#"UPDATE "AddonMediaListItem" SET "ProviderMetadataJson" = ?
                   WHERE "AddonMediaListId" = 50 AND "ProviderId" = ?"#,
            )
            .bind(metadata.to_string())
            .bind(provider_id)
            .execute(&pool)
            .await
            .expect("update reviewed provider metadata");
        }
        sqlx::query(r#"UPDATE "AddonMediaList" SET "PublishedRevision" = 2 WHERE "Id" = 50"#)
            .execute(&pool)
            .await
            .expect("advance published revision");
        let rescan = scan_local_library(
            &pool,
            &LocalScanConfig {
                library_id: 1,
                ffprobe_path: "unused-ffprobe".to_owned(),
            },
        )
        .await
        .expect("playlist rescan");
        assert_eq!(rescan.failed_files, 0, "issues: {:?}", rescan.issues);
        let updated = sqlx::query(
            r#"SELECT "rsm"."Title", "rsm"."OriginalTitle", "rsm"."SortTitle",
                      "rsm"."Outline", "rsm"."Plot", "rsm"."Tagline", "rsm"."Year",
                      "rsm"."ReleaseDate", "rsm"."ContentRating", "rsm"."CollectionName",
                      "rsm"."OriginalBroadcasters", "rsm"."Broadcasters"
               FROM "RemoteStreamMetadata" AS "rsm"
               JOIN "RemoteStream" AS "rs" ON "rs"."Id" = "rsm"."RemoteStreamId"
               WHERE "rs"."ProviderId" = 'item:first'"#,
        )
        .fetch_one(&pool)
        .await
        .expect("updated first metadata");
        assert_eq!(updated.get::<String, _>("Title"), "Updated First");
        assert_eq!(
            updated.get::<String, _>("OriginalTitle"),
            "Updated Original"
        );
        assert_eq!(updated.get::<String, _>("SortTitle"), "Updated Sort");
        assert_eq!(updated.get::<String, _>("Outline"), "Updated outline");
        assert_eq!(updated.get::<String, _>("Plot"), "Updated plot");
        assert_eq!(updated.get::<String, _>("Tagline"), "Updated tagline");
        assert_eq!(updated.get::<i32, _>("Year"), 2002);
        assert_eq!(
            updated.get::<String, _>("ReleaseDate"),
            "2002-03-04T05:06:07+00:00"
        );
        assert_eq!(updated.get::<String, _>("ContentRating"), "16");
        assert_eq!(
            updated.get::<String, _>("CollectionName"),
            "Updated Collection"
        );
        assert_eq!(
            updated.get::<String, _>("OriginalBroadcasters"),
            r#"["Earlier Network"]"#
        );
        assert_eq!(
            updated.get::<String, _>("Broadcasters"),
            r#"["Later Network"]"#
        );
        let rows = sqlx::query(
            r#"SELECT "pi"."Index", "rsm"."Title"
               FROM "Playlist" AS "p"
               JOIN "PlaylistItem" AS "pi" ON "pi"."PlaylistId" = "p"."Id"
               JOIN "RemoteStreamMetadata" AS "rsm" ON "rsm"."RemoteStreamId" = "pi"."MediaItemId"
               WHERE "p"."ManagedSource" IS NOT NULL
               ORDER BY "pi"."Index""#,
        )
        .fetch_all(&pool)
        .await
        .expect("playlist items");
        assert_eq!(
            rows.iter()
                .map(|row| (row.get::<i64, _>("Index"), row.get::<String, _>("Title")))
                .collect::<Vec<_>>(),
            [
                (0, "Updated First - Intro".to_owned()),
                (1, "Updated First".to_owned()),
                (2, "Second".to_owned())
            ]
        );
        let repositories = ersatzrs_infra::repositories::make_repositories(pool.clone());
        let scheduled = repositories
            .scheduling
            .playlist_items_for_source(&SchedulingContentSource::Playlist {
                group: Some("System".to_owned()),
                name: "Fixture order".to_owned(),
            })
            .await
            .expect("load managed playlist for scheduling")
            .expect("managed playlist scheduling items");
        assert_eq!(
            scheduled
                .iter()
                .flat_map(|item| item.media_items.iter())
                .filter_map(|item| item.title.as_deref())
                .collect::<Vec<_>>(),
            ["Updated First - Intro", "Updated First", "Second"]
        );
        let tag_count: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM "Tag" WHERE "Name" IN ('Fixture', 'Fixture Tag')"#,
        )
        .fetch_one(&pool)
        .await
        .expect("tag count");
        assert_eq!(tag_count, 6);
    }

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
                format_metadata: std::collections::BTreeMap::new(),
            }
        }

        #[tokio::test]
        #[allow(clippy::too_many_lines)]
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
                    <mpaa>PG-13</mpaa><mpaa>NL:12</mpaa>
                    <premiered>2020-02-03</premiered>
                    <genre>Drama</genre>
                    <tag>Festival</tag>
                    <studio>Studio One</studio>
                    <label>Label One</label>
                    <language>nl</language>
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
                Some("PG-13/NL:12")
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
                count_for_metadata(&pool, "MusicLabel", first_metadata_id).await,
                1
            );
            assert_eq!(
                count_for_metadata(&pool, "MetadataLanguage", first_metadata_id).await,
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
            let metadata_id = enrich_movie_metadata(
                &mut tx,
                movie_id,
                library_path,
                movie_path,
                &probed_version(),
            )
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

    mod music_video_enrichment {
        use super::super::{
            delete_empty_artists, upsert_artist_for_folder, upsert_local_music_video,
            LocalLibraryPath, METADATA_KIND_EMBEDDED, METADATA_KIND_FALLBACK,
            METADATA_KIND_SIDECAR,
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
        #[allow(clippy::too_many_lines)]
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
                    <label>Sidecar Label</label>
                    <language>fr</language>
                    <mpaa>NL:AL</mpaa>
                    <director>Toni Basil</director>
                </musicvideo>",
            )
            .expect("nfo file");

            let pool = test_db().await;
            seed_music_video_library(&pool, &library_path).await;
            let artist_id = upsert_artist_for_folder(&pool, &library_path, "Talking Heads")
                .await
                .expect("artist");

            let mut probed = probed_version();
            probed
                .format_metadata
                .insert("publisher".to_owned(), "Embedded Label".to_owned());
            probed
                .format_metadata
                .insert("language".to_owned(), "nl".to_owned());
            upsert_local_music_video(
                &pool,
                &library_path,
                artist_id,
                &artist_dir,
                &video_path,
                probed,
            )
            .await
            .expect("music video upsert");

            let row = sqlx::query(
                r#"SELECT "MetadataKind", "Title", "Album", "Plot", "Year", "Track", "ContentRating"
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
            assert_eq!(
                row.get::<Option<String>, _>("ContentRating").as_deref(),
                Some("NL:AL")
            );

            assert_eq!(count(&pool, "MusicVideoArtist").await, 1);
            assert_eq!(count(&pool, "Genre").await, 1);
            assert_eq!(count(&pool, "Tag").await, 1);
            assert_eq!(count(&pool, "Studio").await, 1);
            assert_eq!(count(&pool, "MusicLabel").await, 1);
            assert_eq!(count(&pool, "MetadataLanguage").await, 1);
            assert_eq!(count(&pool, "Director").await, 1);

            std::fs::write(
                artist_dir.join("Once in a Lifetime.nfo"),
                r"<musicvideo><title>Once in a Lifetime</title></musicvideo>",
            )
            .expect("replacement nfo");
            let mut probed = probed_version();
            probed
                .format_metadata
                .insert("publisher".to_owned(), "Embedded Label".to_owned());
            probed
                .format_metadata
                .insert("language".to_owned(), "nl".to_owned());
            upsert_local_music_video(
                &pool,
                &library_path,
                artist_id,
                &artist_dir,
                &video_path,
                probed,
            )
            .await
            .expect("authoritative music video rescan");
            let music_label: String =
                sqlx::query_scalar(r#"SELECT "Name" FROM "MusicLabel" LIMIT 1"#)
                    .fetch_one(&pool)
                    .await
                    .expect("restored embedded label");
            let language: String =
                sqlx::query_scalar(r#"SELECT "Code" FROM "MetadataLanguage" LIMIT 1"#)
                    .fetch_one(&pool)
                    .await
                    .expect("restored embedded language");
            assert_eq!(music_label, "Embedded Label");
            assert_eq!(language, "dut");
            assert_eq!(count(&pool, "Studio").await, 0);
            assert_eq!(count(&pool, "Director").await, 0);

            std::fs::remove_file(artist_dir.join("Once in a Lifetime.nfo"))
                .expect("remove sidecar");
            let mut probed = probed_version();
            probed
                .format_metadata
                .insert("publisher".to_owned(), "Embedded Label".to_owned());
            upsert_local_music_video(
                &pool,
                &library_path,
                artist_id,
                &artist_dir,
                &video_path,
                probed,
            )
            .await
            .expect("embedded-only music video rescan");
            let metadata_kind: i32 =
                sqlx::query_scalar(r#"SELECT "MetadataKind" FROM "MusicVideoMetadata" LIMIT 1"#)
                    .fetch_one(&pool)
                    .await
                    .expect("embedded metadata kind");
            assert_eq!(metadata_kind, METADATA_KIND_EMBEDDED);
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

        #[tokio::test]
        async fn disambiguation_change_replaces_and_cleans_up_artist_identity() {
            let temp = TempDir::new().expect("temp dir");
            let library_path = seed_music_video_library_path(temp.path());
            let artist_dir = temp.path().join("Talking Heads");
            std::fs::create_dir_all(&artist_dir).expect("artist dir");
            let video_path = artist_dir.join("Once in a Lifetime.mkv");
            std::fs::write(&video_path, b"not probed").expect("video file");
            let artist_nfo = artist_dir.join("artist.nfo");
            std::fs::write(
                &artist_nfo,
                r"<artist><name>Talking Heads</name><disambiguation>American rock band</disambiguation></artist>",
            )
            .expect("initial artist nfo");

            let pool = test_db().await;
            seed_music_video_library(&pool, &library_path).await;
            let original_artist_id =
                upsert_artist_for_folder(&pool, &library_path, "Talking Heads")
                    .await
                    .expect("original artist");
            upsert_local_music_video(
                &pool,
                &library_path,
                original_artist_id,
                &artist_dir,
                &video_path,
                probed_version(),
            )
            .await
            .expect("original music video upsert");

            std::fs::write(
                &artist_nfo,
                r"<artist><name>Talking Heads</name><disambiguation>New wave pioneers</disambiguation></artist>",
            )
            .expect("updated artist nfo");
            let replacement_artist_id =
                upsert_artist_for_folder(&pool, &library_path, "Talking Heads")
                    .await
                    .expect("replacement artist");
            assert_ne!(replacement_artist_id, original_artist_id);
            upsert_local_music_video(
                &pool,
                &library_path,
                replacement_artist_id,
                &artist_dir,
                &video_path,
                probed_version(),
            )
            .await
            .expect("replacement music video upsert");
            assert_eq!(count(&pool, "Artist").await, 2);

            assert_eq!(
                delete_empty_artists(&pool, library_path.id)
                    .await
                    .expect("empty artist cleanup"),
                vec![original_artist_id]
            );
            assert_eq!(count(&pool, "Artist").await, 1);
            assert_eq!(count(&pool, "ArtistMetadata").await, 1);
            assert_eq!(count(&pool, "MediaItem").await, 2);

            let row = sqlx::query(
                r#"SELECT "mv"."ArtistId", "am"."Disambiguation"
                   FROM "MusicVideo" AS "mv"
                   JOIN "ArtistMetadata" AS "am" ON "am"."ArtistId" = "mv"."ArtistId""#,
            )
            .fetch_one(&pool)
            .await
            .expect("replacement artist identity");
            assert_eq!(row.get::<i64, _>("ArtistId"), replacement_artist_id);
            assert_eq!(
                row.get::<Option<String>, _>("Disambiguation").as_deref(),
                Some("New wave pioneers")
            );
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
                format_metadata: std::collections::BTreeMap::new(),
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
