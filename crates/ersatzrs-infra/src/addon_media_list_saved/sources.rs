//! Direct local source adapters; callers retain capture and managed-tree ownership.

use std::{fmt::Write as _, fs, path::Path};

use ersatzrs_core::repositories::{AddonMediaListRevisionFile, RepositoryError};
use ersatzrs_domain::EntityId;
use sha2::{Digest as _, Sha256};

use super::{
    assets::{self, SavedAssetError},
    capture::CapturedArtwork,
    files::{validate_reference, SavedFileError},
};
use crate::repositories::addon_publication_fs::{
    revision_blob_path, PublicationFsError, REVISION_BLOB_STORE_DIRECTORY,
};
use crate::{addon_artwork::MAX_ARTWORK_BYTES, db::DbPool};

/// Read one captured upload after rechecking its exact list, scope, role and bytes.
/// The SQL byte bound precedes BLOB selection. This does not save metadata or commit
/// content; retain the caller's capture fence until its eventual document commit.
///
/// # Errors
/// Rejects missing, changed, oversized or incorrectly owned uploads without
/// including uploaded bytes or local identifiers in diagnostics.
pub async fn load_owned_artwork(
    pool: &DbPool,
    list_id: EntityId,
    captured: &CapturedArtwork,
) -> ersatzrs_core::repositories::Result<Vec<u8>> {
    let invalid = || {
        RepositoryError::Conflict("the captured artwork source is unavailable or changed".into())
    };
    validate_reference(&captured.file).map_err(|_| invalid())?;
    if captured.file.byte_length == 0 || captured.file.byte_length > MAX_ARTWORK_BYTES as u64 {
        return Err(invalid());
    }
    let (content_type, extension, bytes): (String, String, Vec<u8>) = sqlx::query_as(
        r#"SELECT "ContentType", "Extension", "Bytes" FROM "AddonMediaListArtworkUpload"
        WHERE "Id"=? AND "AddonMediaListId"=? AND "ScopeItemId"=? AND "Role"=?
          AND "Sha256"=? AND length("Bytes")=? AND length("Bytes") BETWEEN 1 AND ?"#,
    )
    .bind(captured.upload_id)
    .bind(list_id)
    .bind(captured.scope_item_id)
    .bind(captured.role)
    .bind(&captured.file.sha256)
    .bind(i64::try_from(captured.file.byte_length).map_err(|_| invalid())?)
    .bind(i64::try_from(MAX_ARTWORK_BYTES).map_err(|_| invalid())?)
    .fetch_optional(pool)
    .await
    .map_err(|_| invalid())?
    .ok_or_else(invalid)?;
    let (_, detected_extension) =
        crate::addon_artwork::validate_image(&content_type, &bytes).map_err(|_| invalid())?;
    let mut actual = String::with_capacity(64);
    for byte in Sha256::digest(&bytes) {
        write!(actual, "{byte:02x}").map_err(|_| invalid())?;
    }
    if extension != detected_extension
        || actual != captured.file.sha256
        || captured.file.relative_path != format!(".ersatzrs-saved/artwork/{actual}.{extension}")
    {
        return Err(invalid());
    }
    Ok(bytes)
}

/// Copy one captured retained file using explicit storage roots and list ownership.
/// Call in blocking context while retaining the caller's managed-tree fence. This
/// performs no database access, root discovery, document commit or publication.
/// The caller supplies only the portable revision inventory selected by capture.
///
/// # Errors
/// Rejects another list's storage, unsafe source paths and changed source bytes.
pub fn install_retained_file(
    managed_root: &Path,
    storage_parent: &Path,
    storage_owner: &str,
    list_id: EntityId,
    file: &AddonMediaListRevisionFile,
) -> Result<(), SavedAssetError> {
    let destination = super::published::saved_file(file);
    validate_reference(&destination)?;
    let expected = format!(
        "{REVISION_BLOB_STORE_DIRECTORY}/{storage_owner}/{list_id}/{}/{}",
        &file.sha256[..2],
        file.sha256
    );
    if list_id <= 0 || file.storage_relative_path != expected {
        return Err(SavedFileError::InvalidReference.into());
    }
    let root = fs::symlink_metadata(storage_parent).map_err(io_error)?;
    if !root.is_dir() || root.file_type().is_symlink() {
        return Err(SavedFileError::Unavailable.into());
    }
    let source_path =
        revision_blob_path(storage_parent, storage_owner, &file.storage_relative_path)
            .map_err(|error| redacted(&error))?;
    let metadata = fs::symlink_metadata(&source_path).map_err(io_error)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(SavedFileError::Unavailable.into());
    }
    let mut source = fs::File::open(source_path).map_err(io_error)?;
    if !source.metadata().map_err(io_error)?.is_file() {
        return Err(SavedFileError::Unavailable.into());
    }
    assets::install(managed_root, &destination, &mut source)
}

fn io_error(source: std::io::Error) -> SavedAssetError {
    redacted(&PublicationFsError::Io {
        action: "read captured retained asset",
        source,
    })
}

fn redacted(error: &PublicationFsError) -> SavedAssetError {
    let (category, os_error_code) = error.redacted_cause();
    SavedAssetError::Filesystem {
        category,
        os_error_code,
    }
}

#[cfg(test)]
mod tests;
