use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration, Utc};
use thiserror::Error;
use tokio::fs;

/// Cache folders maintained by the scheduler binary.
#[derive(Debug, Clone, Default)]
pub struct CacheCleanupConfig {
    /// Transcode cache contents are removed on every cleanup pass.
    pub transcode_dir: Option<PathBuf>,
    /// Image cache entries older than `image_max_age` are removed.
    pub image_cache_dir: Option<PathBuf>,
    /// Log files older than `log_max_age` are removed.
    pub log_dir: Option<PathBuf>,
    /// Maximum age for image cache files.
    pub image_max_age: Duration,
    /// Maximum age for rotated log files.
    pub log_max_age: Duration,
}

impl CacheCleanupConfig {
    /// Build a cleanup config from optional paths and default retention.
    #[must_use]
    pub fn from_paths(
        transcode_dir: Option<PathBuf>,
        image_cache_dir: Option<PathBuf>,
        log_dir: Option<PathBuf>,
    ) -> Self {
        Self {
            transcode_dir,
            image_cache_dir,
            log_dir,
            image_max_age: Duration::days(30),
            log_max_age: Duration::days(14),
        }
    }
}

/// Summary of one cache cleanup run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CacheCleanupReport {
    /// Number of files removed from the transcode folder.
    pub transcode_files_removed: usize,
    /// Number of old image-cache files removed.
    pub image_files_removed: usize,
    /// Number of old log files removed.
    pub log_files_removed: usize,
}

/// Errors raised while cleaning cache folders.
#[derive(Debug, Error)]
pub enum CacheCleanupError {
    /// A filesystem operation failed.
    #[error("cache cleanup filesystem error for {path}: {source}")]
    Io {
        /// Path being inspected or removed.
        path: PathBuf,
        /// Underlying IO error.
        #[source]
        source: std::io::Error,
    },
}

/// Clean transcode, image cache, and log folders.
///
/// # Errors
///
/// Returns an error when an existing configured folder cannot be read or a file
/// selected for removal cannot be deleted.
pub async fn clean_caches(
    config: &CacheCleanupConfig,
) -> Result<CacheCleanupReport, CacheCleanupError> {
    let now = Utc::now();
    let mut report = CacheCleanupReport::default();

    if let Some(path) = &config.transcode_dir {
        report.transcode_files_removed = remove_all_files(path).await?;
    }

    if let Some(path) = &config.image_cache_dir {
        report.image_files_removed =
            remove_files_older_than(path, now, config.image_max_age).await?;
    }

    if let Some(path) = &config.log_dir {
        report.log_files_removed = remove_files_older_than(path, now, config.log_max_age).await?;
    }

    Ok(report)
}

async fn remove_all_files(root: &Path) -> Result<usize, CacheCleanupError> {
    let mut removed = 0;
    for file in list_files(root).await? {
        remove_file(&file).await?;
        removed += 1;
    }
    Ok(removed)
}

async fn remove_files_older_than(
    root: &Path,
    now: DateTime<Utc>,
    max_age: Duration,
) -> Result<usize, CacheCleanupError> {
    let mut removed = 0;
    for file in list_files(root).await? {
        let metadata = fs::metadata(&file)
            .await
            .map_err(|source| CacheCleanupError::Io {
                path: file.clone(),
                source,
            })?;
        let modified = metadata
            .modified()
            .map_err(|source| CacheCleanupError::Io {
                path: file.clone(),
                source,
            })
            .map(DateTime::<Utc>::from)?;
        if now.signed_duration_since(modified) >= max_age {
            remove_file(&file).await?;
            removed += 1;
        }
    }
    Ok(removed)
}

async fn list_files(root: &Path) -> Result<Vec<PathBuf>, CacheCleanupError> {
    if !fs::try_exists(root)
        .await
        .map_err(|source| CacheCleanupError::Io {
            path: root.to_path_buf(),
            source,
        })?
    {
        return Ok(Vec::new());
    }

    let mut result = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(path) = pending.pop() {
        let mut entries = fs::read_dir(&path)
            .await
            .map_err(|source| CacheCleanupError::Io {
                path: path.clone(),
                source,
            })?;
        while let Some(entry) =
            entries
                .next_entry()
                .await
                .map_err(|source| CacheCleanupError::Io {
                    path: path.clone(),
                    source,
                })?
        {
            let entry_path = entry.path();
            let file_type = entry
                .file_type()
                .await
                .map_err(|source| CacheCleanupError::Io {
                    path: entry_path.clone(),
                    source,
                })?;
            if file_type.is_dir() {
                pending.push(entry_path);
            } else if file_type.is_file() {
                result.push(entry_path);
            }
        }
    }
    Ok(result)
}

async fn remove_file(path: &Path) -> Result<(), CacheCleanupError> {
    fs::remove_file(path)
        .await
        .map_err(|source| CacheCleanupError::Io {
            path: path.to_path_buf(),
            source,
        })
}
