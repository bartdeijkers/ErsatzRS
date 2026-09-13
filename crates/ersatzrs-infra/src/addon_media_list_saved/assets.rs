//! Immutable saved assets, separate from document commitment and local ownership.

use std::fs;
use std::io::{self, Read};
use std::path::Path;

use ersatzrs_domain::AddonMediaListPublicationErrorCategory;

use crate::repositories::addon_publication_fs::{
    copy_verified_bytes, ensure_managed_subdirectories, verify_revision_blob, PublicationFsError,
};

use super::files::{validate_reference, SavedFileError};
use super::model::{SavedFile, SAVED_ASSETS_DIRECTORY};

/// Recognizable local staging names; never sufficient proof of deletion ownership.
pub const ASSET_TEMP_PREFIX: &str = ".ersatzrs-asset-tmp-";

/// Redacted failure details usable by the existing bounded operation retry policy.
#[derive(Debug, thiserror::Error)]
pub enum SavedAssetError {
    #[error(transparent)]
    Reference(#[from] SavedFileError),
    #[error("the saved asset filesystem operation failed")]
    Filesystem {
        /// Existing publication classification, including native sharing violations.
        category: AddonMediaListPublicationErrorCategory,
        /// Native error number only; never include private source error text.
        os_error_code: Option<i32>,
    },
}

/// Install or verify one immutable asset without replacing existing content.
///
/// The caller owns the managed-tree/operation fence and supplies an authorized
/// reader: retained list-owned bytes, manual subtitle originals, a portable
/// replacement or one bounded owned artwork upload. Prepared Download ahead
/// artifacts remain references: never pass a Kind 1 cache source to duplicate
/// video through this helper. Run this blocking IO outside an async executor
/// and outside a database write transaction. Keep the fence alive for the entire
/// operation, including when
/// a waiting async task is cancelled. This function spawns no work.
///
/// File bytes are synchronized before no-clobber installation. This is not a
/// durable Save acknowledgement: directory synchronization, document replacement,
/// database ordering and native platform crash recovery belong to the caller's
/// saved-content commit boundary. The fenced writer must clean up confirmed
/// locally owned temporary leftovers after abrupt death. A recognizable prefix
/// alone does not grant authority to delete files in an imported folder.
///
/// # Errors
/// Refuses paths outside reserved storage, unsafe directories/files, changed
/// bytes and IO failures. Only caller-owned temporary output is cleaned up;
/// an existing corrupt destination is preserved and reported, never replaced.
pub fn install(
    managed_root: &Path,
    reference: &SavedFile,
    source: &mut impl Read,
) -> Result<(), SavedAssetError> {
    validate_reference(reference)?;
    if reference.relative_path.split('/').next() != Some(SAVED_ASSETS_DIRECTORY)
        || !reference.relative_path.contains('/')
    {
        return Err(SavedFileError::InvalidReference.into());
    }
    let root_metadata = fs::symlink_metadata(managed_root).map_err(io_failure)?;
    if !root_metadata.is_dir() || root_metadata.file_type().is_symlink() {
        return Err(SavedFileError::Unavailable.into());
    }
    let destination = managed_root.join(&reference.relative_path);
    let parent = destination
        .parent()
        .ok_or(SavedFileError::InvalidReference)?;
    ensure_managed_subdirectories(managed_root, parent).map_err(|error| redacted(&error))?;
    match fs::symlink_metadata(&destination) {
        Ok(_) => return verify(&destination, reference),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(io_failure(error)),
    }

    let mut temporary = tempfile::Builder::new()
        .prefix(ASSET_TEMP_PREFIX)
        .tempfile_in(parent)
        .map_err(io_failure)?;
    copy_verified_bytes(
        source,
        &mut temporary,
        &reference.sha256,
        reference.byte_length,
    )
    .map_err(|error| redacted(&error))?;
    temporary.as_file().sync_all().map_err(io_failure)?;
    match temporary.persist_noclobber(&destination) {
        Ok(_) => verify(&destination, reference),
        Err(error) if error.error.kind() == io::ErrorKind::AlreadyExists => {
            verify(&destination, reference)
        }
        Err(error) => Err(io_failure(error.error)),
    }
}

fn verify(path: &Path, reference: &SavedFile) -> Result<(), SavedAssetError> {
    verify_revision_blob(path, &reference.sha256, reference.byte_length)
        .map_err(|error| redacted(&error))
}

fn redacted(error: &PublicationFsError) -> SavedAssetError {
    let (category, os_error_code) = error.redacted_cause();
    SavedAssetError::Filesystem {
        category,
        os_error_code,
    }
}

fn io_failure(source: io::Error) -> SavedAssetError {
    redacted(&PublicationFsError::Io {
        action: "access saved asset",
        source,
    })
}

#[cfg(test)]
mod tests;
