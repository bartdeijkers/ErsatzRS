use std::path::{Path, PathBuf};

use thiserror::Error;
use tokio::fs;

/// Cache folders maintained by the scheduler binary.
///
/// Mirrors `ErsatzTV.Services.RunOnce.CacheCleanerService`:
///
/// - `transcode_dir` is fully emptied on every pass (parity with
///   `localFileSystem.EmptyFolder(FileSystemLayout.TranscodeFolder)`).
/// - `channel_guide_dir` is pruned: anything that does not contain
///   `channels` and is not in the keeper list (typically `"{number}.xml"`)
///   is deleted (parity with `CacheCleanerService.cs:64-83`).
#[derive(Debug, Clone, Default)]
pub struct CacheCleanupConfig {
    /// Transcode cache contents are removed on every cleanup pass.
    pub transcode_dir: Option<PathBuf>,
    /// Channel-guide cache folder pruned of orphaned per-channel XML files.
    pub channel_guide_dir: Option<PathBuf>,
}

impl CacheCleanupConfig {
    /// Build a cleanup config from optional paths.
    #[must_use]
    pub fn from_paths(transcode_dir: Option<PathBuf>, channel_guide_dir: Option<PathBuf>) -> Self {
        Self {
            transcode_dir,
            channel_guide_dir,
        }
    }
}

/// Summary of one cache cleanup run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CacheCleanupReport {
    /// Number of files removed from the transcode folder.
    pub transcode_files_removed: usize,
    /// Number of orphaned channel-guide XML files removed.
    pub channel_guide_files_removed: usize,
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

/// Clean configured cache folders.
///
/// `channel_guide_keepers` is the set of XML file names that must be retained
/// in the channel-guide cache folder (typically `"{channel_number}.xml"` for
/// every active channel).
///
/// # Errors
///
/// Returns an error when an existing configured folder cannot be read or a file
/// selected for removal cannot be deleted.
pub async fn clean_caches(
    config: &CacheCleanupConfig,
    channel_guide_keepers: &[String],
) -> Result<CacheCleanupReport, CacheCleanupError> {
    let mut report = CacheCleanupReport::default();

    if let Some(path) = &config.transcode_dir {
        report.transcode_files_removed = remove_all_files(path).await?;
    }

    if let Some(path) = &config.channel_guide_dir {
        report.channel_guide_files_removed =
            prune_channel_guide_dir(path, channel_guide_keepers).await?;
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

async fn prune_channel_guide_dir(
    root: &Path,
    keepers: &[String],
) -> Result<usize, CacheCleanupError> {
    if !directory_exists(root).await? {
        return Ok(0);
    }

    let mut entries = fs::read_dir(root)
        .await
        .map_err(|source| CacheCleanupError::Io {
            path: root.to_path_buf(),
            source,
        })?;

    let mut removed = 0;
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|source| CacheCleanupError::Io {
            path: root.to_path_buf(),
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
        if !file_type.is_file() {
            continue;
        }
        let Some(name) = entry_path
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_owned)
        else {
            continue;
        };
        if name.contains("channels") || keepers.iter().any(|keeper| keeper == &name) {
            continue;
        }
        remove_file(&entry_path).await?;
        removed += 1;
    }
    Ok(removed)
}

async fn list_files(root: &Path) -> Result<Vec<PathBuf>, CacheCleanupError> {
    if !directory_exists(root).await? {
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

async fn directory_exists(root: &Path) -> Result<bool, CacheCleanupError> {
    fs::try_exists(root)
        .await
        .map_err(|source| CacheCleanupError::Io {
            path: root.to_path_buf(),
            source,
        })
}

async fn remove_file(path: &Path) -> Result<(), CacheCleanupError> {
    fs::remove_file(path)
        .await
        .map_err(|source| CacheCleanupError::Io {
            path: path.to_path_buf(),
            source,
        })
}
