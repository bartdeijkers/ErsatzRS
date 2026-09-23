//! Completed selected-track artifacts, shared by demand-time Burn callbacks.

use std::{
    collections::HashSet,
    future::Future,
    io::{Read as _, Write as _},
    path::{Path, PathBuf},
    sync::Arc,
};

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use tokio::{sync::Mutex, time::Instant};

use super::{burn_subtitles, protocol::Source};

const VERSION: u32 = 1;
const MAX_ARTIFACT_BYTES: u64 = 32 * 1024 * 1024;
const MAX_CACHE_BYTES: u64 = 256 * 1024 * 1024;
const MAX_MANIFEST_BYTES: u64 = 2048;
const MAX_ENTRIES: usize = 4096;
const GRACE_SECONDS: i64 = 60;

/// One owner per cache directory; clones of a surrounding host share this owner.
/// The directory must not be shared by independent processes or cache owners.
pub(crate) struct BurnCache {
    root: PathBuf,
    gate: Arc<Mutex<()>>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub(crate) enum CacheError {
    #[error("Next subtitle cache key or expiry is invalid")]
    Identity,
    #[error("Next subtitle cache source binding changed")]
    Stale,
    #[error("Next subtitle cache deadline expired")]
    Deadline,
    #[error("Next subtitle cache is full")]
    Full,
    #[error("Next subtitle cache storage failed")]
    Storage,
    #[error(transparent)]
    Extraction(#[from] burn_subtitles::BurnError),
}

#[derive(Serialize, Deserialize)]
struct Manifest {
    version: u32,
    key: String,
    format: String,
    bytes: u64,
    expires: DateTime<Utc>,
}

impl BurnCache {
    pub(crate) fn new(root: PathBuf) -> Self {
        Self {
            root,
            gate: Arc::new(Mutex::new(())),
        }
    }

    /// Return a complete artifact, extending its lease through the latest finish
    /// plus sixty seconds. Only expired leases and incomplete artifacts are removed.
    /// The caller computes a lowercase SHA-256 key over the exact immutable source
    /// binding/revision; this cache never persists URLs, credentials or source paths.
    ///
    /// # Errors
    /// Fails closed for invalid identity, admission/deadline, extraction, storage,
    /// or insufficient capacity for one maximum-size extraction. Cancellation
    /// drops the managed child and temporary file; no partial artifact is admitted.
    /// Revalidation runs after admission on hits and before promotion on misses.
    /// Atomic filesystem workers retain admission until completion even when their
    /// caller times out; they may finish committing a complete artifact afterwards.
    #[allow(clippy::too_many_arguments)] // Existing extraction contract supplies source, timing and validation.
    pub(crate) async fn prepare<F, Fut>(
        &self,
        key: &str,
        source: &Source,
        stream: u32,
        codec: &str,
        ffmpeg: &Path,
        expires: DateTime<Utc>,
        deadline: Instant,
        validate: F,
    ) -> Result<PathBuf, CacheError>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = bool>,
    {
        let format = if codec.eq_ignore_ascii_case("ass") || codec.eq_ignore_ascii_case("ssa") {
            "ass"
        } else {
            "srt"
        };
        self.prepare_artifact(
            key,
            format,
            expires,
            deadline,
            validate,
            |path, _file| async move {
                burn_subtitles::extract(source, stream, codec, ffmpeg, &path, deadline)
                    .await
                    .map_err(CacheError::from)
            },
        )
        .await
    }

    /// Store complete rebased sidecar bytes under the same bounded leases as Burn artifacts.
    pub(crate) async fn prepare_text<F, Fut>(
        &self,
        key: &str,
        format: &str,
        content: &str,
        expires: DateTime<Utc>,
        deadline: Instant,
        validate: F,
    ) -> Result<PathBuf, CacheError>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = bool>,
    {
        if content.is_empty()
            || u64::try_from(content.len()).map_err(|_| CacheError::Storage)? > MAX_ARTIFACT_BYTES
        {
            return Err(CacheError::Storage);
        }
        self.prepare_artifact(
            key,
            format,
            expires,
            deadline,
            validate,
            |_path, file| async move {
                use tokio::io::AsyncWriteExt as _;
                let mut file = tokio::fs::File::from_std(file);
                tokio::time::timeout_at(deadline, async {
                    file.write_all(content.as_bytes()).await?;
                    file.flush().await
                })
                .await
                .map_err(|_| CacheError::Deadline)?
                .map_err(|_| CacheError::Storage)
            },
        )
        .await
    }

    #[allow(clippy::too_many_lines)] // One admission owns cleanup, validation, building and atomic promotion.
    async fn prepare_artifact<F, Fut, Build, Built>(
        &self,
        key: &str,
        format: &str,
        expires: DateTime<Utc>,
        deadline: Instant,
        validate: F,
        build: Build,
    ) -> Result<PathBuf, CacheError>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = bool>,
        Build: FnOnce(PathBuf, std::fs::File) -> Built,
        Built: Future<Output = Result<(), CacheError>>,
    {
        if !matches!(format, "srt" | "ass" | "vtt") {
            return Err(CacheError::Identity);
        }
        let lease = expires
            .checked_add_signed(Duration::seconds(GRACE_SECONDS))
            .filter(|expiry| *expiry > Utc::now())
            .ok_or(CacheError::Identity)?;
        if !valid_key(key) {
            return Err(CacheError::Identity);
        }
        let guard = tokio::time::timeout_at(deadline, self.gate.clone().lock_owned())
            .await
            .map_err(|_| CacheError::Deadline)?;
        check_deadline(deadline)?;
        let root = self.root.clone();
        let requested_key = key.to_owned();
        // The filesystem worker retains admission even if its awaiter is cancelled.
        // A later callback cannot race an abandoned orphan-removal operation.
        let (guard, total, entries, cached) = tokio::time::timeout_at(
            deadline,
            tokio::task::spawn_blocking(move || {
                let (total, entries) = cleanup_root(&root)?;
                let cached = read_manifest(&root, &requested_key)?;
                Ok::<_, CacheError>((guard, total, entries, cached))
            }),
        )
        .await
        .map_err(|_| CacheError::Deadline)?
        .map_err(|_| CacheError::Storage)??;
        let path = self.root.join(format!("{key}.{format}"));
        if let Some(mut manifest) = cached {
            if !tokio::time::timeout_at(deadline, validate())
                .await
                .map_err(|_| CacheError::Deadline)?
            {
                return Err(CacheError::Stale);
            }
            if manifest.format != format {
                return Err(CacheError::Identity);
            }
            if manifest.expires < lease {
                manifest.expires = lease;
                check_deadline(deadline)?;
                if total
                    .checked_add(MAX_MANIFEST_BYTES)
                    .is_none_or(|bytes| bytes > MAX_CACHE_BYTES)
                {
                    return Err(CacheError::Full);
                }
                let root = self.root.clone();
                commit(deadline, move || {
                    let _guard = guard;
                    persist_manifest(&root, &manifest)
                })
                .await?;
            }
            return Ok(path);
        }
        // Reserve the extraction maximum before spawning; no live lease is evicted.
        if entries > MAX_ENTRIES - 2
            || total
                .checked_add(MAX_ARTIFACT_BYTES + MAX_MANIFEST_BYTES)
                .is_none_or(|reserved| reserved > MAX_CACHE_BYTES)
        {
            return Err(CacheError::Full);
        }
        check_deadline(deadline)?;
        let pending = tempfile::Builder::new()
            .prefix(".burn-")
            .tempfile_in(&self.root)
            .map_err(|_| CacheError::Storage)?;
        build(
            pending.path().to_owned(),
            pending.reopen().map_err(|_| CacheError::Storage)?,
        )
        .await?;
        if !tokio::time::timeout_at(deadline, validate())
            .await
            .map_err(|_| CacheError::Deadline)?
        {
            return Err(CacheError::Stale);
        }
        check_deadline(deadline)?;
        let bytes = pending
            .as_file()
            .metadata()
            .map_err(|_| CacheError::Storage)?
            .len();
        if bytes == 0 || bytes > MAX_ARTIFACT_BYTES {
            return Err(CacheError::Storage);
        }
        let root = self.root.clone();
        let promoted = path.clone();
        let manifest = Manifest {
            version: VERSION,
            key: key.to_owned(),
            format: format.to_owned(),
            bytes,
            expires: lease,
        };
        commit(deadline, move || {
            let _guard = guard;
            pending
                .as_file()
                .sync_all()
                .map_err(|_| CacheError::Storage)?;
            check_deadline(deadline)?;
            pending
                .persist(&promoted)
                .map_err(|_| CacheError::Storage)?;
            // Commit record comes last. A crash here leaves an orphan, never a hit.
            persist_manifest(&root, &manifest)
        })
        .await?;
        Ok(path)
    }
}

fn cleanup_root(root: &Path) -> Result<(u64, usize), CacheError> {
    cleanup_with_limit(root, MAX_ENTRIES)
}

fn cleanup_with_limit(root: &Path, entry_limit: usize) -> Result<(u64, usize), CacheError> {
    std::fs::create_dir_all(root).map_err(|_| CacheError::Storage)?;
    let directory = std::fs::read_dir(root).map_err(|_| CacheError::Storage)?;
    let mut entries = Vec::new();
    for entry in directory {
        let entry = entry.map_err(|_| CacheError::Storage)?;
        // Allow an interrupted output/manifest transaction to leave its temporary
        // slots beyond the admitted live count; clean them before enforcing it.
        if entries.len() >= entry_limit.saturating_add(2) {
            return Err(CacheError::Full);
        }
        entries.push(entry.path());
    }
    let mut live = HashSet::new();
    for path in &entries {
        let Some(key) = path
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| name.strip_suffix(".json"))
            .filter(|key| valid_key(key))
        else {
            continue;
        };
        match read_manifest(root, key)? {
            Some(manifest) if manifest.expires > Utc::now() => {
                live.insert(format!("{}.{}", manifest.key, manifest.format));
                live.insert(format!("{}.json", manifest.key));
            }
            _ => {
                remove(path)?;
            }
        }
    }
    let mut total = 0_u64;
    let mut retained = 0_usize;
    for path in entries {
        let name = path.file_name().and_then(|name| name.to_str());
        let owned = name.is_some_and(owned_file);
        if owned && !name.is_some_and(|name| live.contains(name)) {
            remove(&path)?;
            continue;
        }
        match std::fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.is_file() => {
                retained += 1;
                total = total.checked_add(metadata.len()).ok_or(CacheError::Full)?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Ok(_) | Err(_) => return Err(CacheError::Storage),
        }
    }
    if total > MAX_CACHE_BYTES || retained > entry_limit {
        return Err(CacheError::Full);
    }
    Ok((total, retained))
}

fn valid_key(key: &str) -> bool {
    key.len() == 64
        && key
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn owned_file(name: &str) -> bool {
    name.starts_with(".burn-")
        || [".json", ".ass", ".srt", ".vtt"]
            .into_iter()
            .any(|suffix| name.strip_suffix(suffix).is_some_and(valid_key))
}

fn check_deadline(deadline: Instant) -> Result<(), CacheError> {
    if Instant::now() >= deadline {
        Err(CacheError::Deadline)
    } else {
        Ok(())
    }
}

fn remove(path: &Path) -> Result<(), CacheError> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(CacheError::Storage),
    }
}

fn read_manifest(root: &Path, key: &str) -> Result<Option<Manifest>, CacheError> {
    let path = root.join(format!("{key}.json"));
    match std::fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.is_file() && metadata.len() <= MAX_MANIFEST_BYTES => {}
        Ok(_) => return Ok(None),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(CacheError::Storage),
    }
    let file = std::fs::File::open(&path).map_err(|_| CacheError::Storage)?;
    let mut bytes = Vec::new();
    file.take(MAX_MANIFEST_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| CacheError::Storage)?;
    if bytes.len() > usize::try_from(MAX_MANIFEST_BYTES).unwrap() {
        return Ok(None);
    }
    let Ok(manifest) = serde_json::from_slice::<Manifest>(&bytes) else {
        return Ok(None);
    };
    if manifest.version != VERSION
        || manifest.key != key
        || !matches!(manifest.format.as_str(), "ass" | "srt" | "vtt")
        || manifest.bytes == 0
        || manifest.bytes > MAX_ARTIFACT_BYTES
    {
        return Ok(None);
    }
    match std::fs::symlink_metadata(root.join(format!("{key}.{}", manifest.format))) {
        Ok(metadata) if metadata.is_file() && metadata.len() == manifest.bytes => {
            Ok(Some(manifest))
        }
        Ok(_) => Ok(None),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(CacheError::Storage),
    }
}

async fn commit<F>(deadline: Instant, operation: F) -> Result<(), CacheError>
where
    F: FnOnce() -> Result<(), CacheError> + Send + 'static,
{
    tokio::time::timeout_at(deadline, tokio::task::spawn_blocking(operation))
        .await
        .map_err(|_| CacheError::Deadline)?
        .map_err(|_| CacheError::Storage)?
}

fn persist_manifest(root: &Path, manifest: &Manifest) -> Result<(), CacheError> {
    let bytes = serde_json::to_vec(manifest).map_err(|_| CacheError::Storage)?;
    if bytes.len() > usize::try_from(MAX_MANIFEST_BYTES).unwrap() {
        return Err(CacheError::Storage);
    }
    let mut pending = tempfile::Builder::new()
        .prefix(".burn-")
        .tempfile_in(root)
        .map_err(|_| CacheError::Storage)?;
    pending.write_all(&bytes).map_err(|_| CacheError::Storage)?;
    pending
        .as_file()
        .sync_all()
        .map_err(|_| CacheError::Storage)?;
    pending
        .persist(root.join(format!("{}.json", manifest.key)))
        .map_err(|_| CacheError::Storage)?;
    Ok(())
}

#[cfg(test)]
#[path = "burn_cache_tests.rs"]
mod tests;
