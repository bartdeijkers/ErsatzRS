//! Host-owned transactional archive files for incremental add-on list imports.

use std::fmt::Write as _;
use std::path::{Component, Path, PathBuf};

use ersatzrs_core::repositories::{AddonMediaListImportCheckpoint, RepositoryError, Result};
use ersatzrs_domain::EntityId;
use sha2::{Digest as _, Sha256};
use tokio::io::AsyncReadExt as _;
use uuid::Uuid;

pub(crate) const MAX_IMPORT_ARCHIVE_BYTES: u64 = 4 * 1024 * 1024;
pub(crate) const MAX_IMPORT_ARCHIVE_ENTRIES: u32 = 10_000;

/// One invocation-private archive copy. The provider never receives the live path.
#[derive(Debug)]
pub(crate) struct PreparedImportArchive {
    data_root: PathBuf,
    stage_dir: PathBuf,
    staged_path: PathBuf,
    list_id: EntityId,
    generation: i32,
    initial_sha256: String,
    initial_entry_count: u32,
    previous_relative_path: Option<String>,
}

/// A fully written immutable archive candidate ready for a database pointer commit.
#[derive(Debug)]
pub(crate) struct SealedImportArchive {
    pub(crate) relative_path: String,
    pub(crate) sha256: String,
    pub(crate) entry_count: u32,
    pub(crate) initial_entry_count: u32,
    pub(crate) previous_relative_path: Option<String>,
    pub(crate) reused_previous: bool,
    data_root: PathBuf,
    absolute_path: PathBuf,
}

impl PreparedImportArchive {
    pub(crate) fn staged_path(&self) -> &Path {
        &self.staged_path
    }

    pub(crate) fn generation(&self) -> Result<u32> {
        u32::try_from(self.generation).map_err(|_| {
            RepositoryError::Conflict("the import generation is outside the archive bound".into())
        })
    }

    pub(crate) fn entry_count(&self) -> u32 {
        self.initial_entry_count
    }

    pub(crate) fn sha256(&self) -> &str {
        &self.initial_sha256
    }

    pub(crate) async fn discard(self) {
        let _ = tokio::fs::remove_dir_all(self.stage_dir).await;
    }

    pub(crate) async fn seal(self) -> Result<SealedImportArchive> {
        let (sha256, entry_count) = inspect_archive(&self.staged_path).await?;
        if self.previous_relative_path.is_some()
            && sha256 == self.initial_sha256
            && entry_count == self.initial_entry_count
        {
            let relative_path = self.previous_relative_path.clone().unwrap_or_default();
            let absolute_path = resolve_relative(&self.data_root, &relative_path)?;
            let _ = tokio::fs::remove_dir_all(&self.stage_dir).await;
            return Ok(SealedImportArchive {
                relative_path,
                sha256,
                entry_count,
                initial_entry_count: self.initial_entry_count,
                previous_relative_path: self.previous_relative_path,
                reused_previous: true,
                data_root: self.data_root,
                absolute_path,
            });
        }

        let candidate_id = Uuid::new_v4().simple().to_string();
        let relative_path = format!(
            "media-list-checkpoints/list-{}/generation-{}/{candidate_id}/archive.txt",
            self.list_id, self.generation
        );
        let absolute_path = resolve_relative(&self.data_root, &relative_path)?;
        let parent = absolute_path.parent().ok_or_else(|| {
            RepositoryError::Conflict("the archive candidate has no parent directory".into())
        })?;
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|_| archive_io_error("create archive candidate directory"))?;
        tokio::fs::rename(&self.staged_path, &absolute_path)
            .await
            .map_err(|_| archive_io_error("seal archive candidate"))?;
        let _ = tokio::fs::remove_dir_all(&self.stage_dir).await;
        Ok(SealedImportArchive {
            relative_path,
            sha256,
            entry_count,
            initial_entry_count: self.initial_entry_count,
            previous_relative_path: self.previous_relative_path,
            reused_previous: false,
            data_root: self.data_root,
            absolute_path,
        })
    }
}

impl SealedImportArchive {
    pub(crate) fn data_root(&self) -> &Path {
        &self.data_root
    }

    pub(crate) async fn discard_candidate(&self) {
        if !self.reused_previous {
            let _ = tokio::fs::remove_file(&self.absolute_path).await;
            remove_empty_archive_parents(&self.data_root, &self.absolute_path).await;
        }
    }
}

pub(crate) async fn prepare_import_archive(
    data_root: PathBuf,
    list_id: EntityId,
    generation: i32,
    checkpoint: Option<&AddonMediaListImportCheckpoint>,
) -> Result<PreparedImportArchive> {
    let stage_id = Uuid::new_v4().simple().to_string();
    let stage_dir = data_root
        .join("media-list-checkpoints")
        .join(format!("list-{list_id}"))
        .join(".staging")
        .join(stage_id);
    tokio::fs::create_dir_all(&stage_dir)
        .await
        .map_err(|_| archive_io_error("create archive staging directory"))?;
    let staged_path = stage_dir.join("archive.txt");

    let prepared = async {
        let (initial_sha256, initial_entry_count, previous_relative_path) =
            if let Some(checkpoint) = checkpoint {
                let source = resolve_relative(&data_root, &checkpoint.relative_path)?;
                let (sha256, entry_count) = inspect_archive(&source).await?;
                if sha256 != checkpoint.sha256 || u64::from(entry_count) != checkpoint.entry_count {
                    return Err(RepositoryError::Conflict(
                        "the persisted import archive no longer matches its checkpoint".into(),
                    ));
                }
                tokio::fs::copy(&source, &staged_path)
                    .await
                    .map_err(|_| archive_io_error("copy import archive into staging"))?;
                (sha256, entry_count, Some(checkpoint.relative_path.clone()))
            } else {
                tokio::fs::OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .open(&staged_path)
                    .await
                    .map_err(|_| archive_io_error("create empty import archive"))?;
                (sha256_hex(&[]), 0, None)
            };
        Ok(PreparedImportArchive {
            data_root,
            stage_dir: stage_dir.clone(),
            staged_path,
            list_id,
            generation,
            initial_sha256,
            initial_entry_count,
            previous_relative_path,
        })
    }
    .await;
    if prepared.is_err() {
        let _ = tokio::fs::remove_dir_all(stage_dir).await;
    }
    prepared
}

pub(crate) async fn remove_relative_archive(data_root: &Path, relative: &str) -> Result<()> {
    let path = resolve_relative(data_root, relative)?;
    match tokio::fs::remove_file(&path).await {
        Ok(()) => remove_empty_archive_parents(data_root, &path).await,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err(archive_io_error("remove import archive")),
    }
    Ok(())
}

async fn inspect_archive(path: &Path) -> Result<(String, u32)> {
    let metadata = tokio::fs::symlink_metadata(path)
        .await
        .map_err(|_| archive_io_error("inspect import archive"))?;
    if !metadata.file_type().is_file() || metadata.len() > MAX_IMPORT_ARCHIVE_BYTES {
        return Err(RepositoryError::Conflict(
            "the staged import archive is not a bounded regular file".into(),
        ));
    }
    let file = tokio::fs::File::open(path)
        .await
        .map_err(|_| archive_io_error("open import archive"))?;
    let mut bytes = Vec::new();
    file.take(MAX_IMPORT_ARCHIVE_BYTES.saturating_add(1))
        .read_to_end(&mut bytes)
        .await
        .map_err(|_| archive_io_error("read import archive"))?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_IMPORT_ARCHIVE_BYTES {
        return Err(RepositoryError::Conflict(
            "the staged import archive exceeds its byte limit".into(),
        ));
    }
    let entry_count = bytes
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .count();
    let entry_count = u32::try_from(entry_count).map_err(|_| {
        RepositoryError::Conflict("the staged import archive exceeds its entry limit".into())
    })?;
    if entry_count > MAX_IMPORT_ARCHIVE_ENTRIES {
        return Err(RepositoryError::Conflict(
            "the staged import archive exceeds its entry limit".into(),
        ));
    }
    Ok((sha256_hex(&bytes), entry_count))
}

fn resolve_relative(root: &Path, relative: &str) -> Result<PathBuf> {
    let path = Path::new(relative);
    if relative.is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(RepositoryError::Conflict(
            "the import archive identity is not a contained relative path".into(),
        ));
    }
    Ok(root.join(path))
}

async fn remove_empty_archive_parents(data_root: &Path, file: &Path) {
    let stop = data_root.join("media-list-checkpoints");
    let mut parent = file.parent();
    while let Some(directory) = parent {
        if directory == stop || !directory.starts_with(&stop) {
            break;
        }
        if tokio::fs::remove_dir(directory).await.is_err() {
            break;
        }
        parent = directory.parent();
    }
}

fn archive_io_error(action: &'static str) -> RepositoryError {
    RepositoryError::unavailable(action, "the private import archive is unavailable")
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        let _ = write!(output, "{byte:02x}");
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn archive_candidates_are_immutable_and_reuse_unchanged_content() {
        let temporary = tempfile::tempdir().expect("temporary archive root");
        let first = prepare_import_archive(temporary.path().to_path_buf(), 7, 1, None)
            .await
            .expect("prepare first archive");
        tokio::fs::write(first.staged_path(), b"youtube first\nyoutube second\n")
            .await
            .expect("write first archive");
        let first = first.seal().await.expect("seal first archive");
        assert_eq!(first.entry_count, 2);
        assert!(!first.reused_previous);

        let checkpoint = AddonMediaListImportCheckpoint {
            id: 1,
            list_id: 7,
            relative_path: first.relative_path.clone(),
            generation: 1,
            sha256: first.sha256.clone(),
            entry_count: u64::from(first.entry_count),
            promoted_at: chrono::Utc::now(),
        };
        let unchanged =
            prepare_import_archive(temporary.path().to_path_buf(), 7, 2, Some(&checkpoint))
                .await
                .expect("prepare unchanged archive")
                .seal()
                .await
                .expect("seal unchanged archive");
        assert!(unchanged.reused_previous);
        assert_eq!(unchanged.relative_path, first.relative_path);
    }

    #[tokio::test]
    async fn archive_preparation_rejects_a_tampered_checkpoint() {
        let temporary = tempfile::tempdir().expect("temporary archive root");
        let relative = "media-list-checkpoints/list-1/archive.txt";
        let absolute = temporary.path().join(relative);
        tokio::fs::create_dir_all(absolute.parent().expect("archive parent"))
            .await
            .unwrap();
        tokio::fs::write(&absolute, b"youtube first\n")
            .await
            .unwrap();
        let checkpoint = AddonMediaListImportCheckpoint {
            id: 1,
            list_id: 1,
            relative_path: relative.to_owned(),
            generation: 1,
            sha256: sha256_hex(b"different"),
            entry_count: 1,
            promoted_at: chrono::Utc::now(),
        };
        let error = prepare_import_archive(temporary.path().to_path_buf(), 1, 2, Some(&checkpoint))
            .await
            .expect_err("tampered checkpoint must fail");
        assert!(error.to_string().contains("no longer matches"));
    }
}
