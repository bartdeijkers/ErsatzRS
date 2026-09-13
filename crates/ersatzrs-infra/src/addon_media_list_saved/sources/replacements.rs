//! Inspect an exact selected replacement without copying or changing its binding.

use std::path::{Component, Path};

use ersatzrs_core::repositories::{RepositoryError, Result};

use crate::addon_media_list_artifacts::{confined_file_if_present, inspect_file};
use crate::addon_media_list_saved::{
    capture::CapturedReplacement, files::validate_reference, model::SavedFile,
};
use crate::db::DbPool;

/// Describe a selected Local video only when its single source is inside this list.
/// The returned relative path names the existing source, not an installed immutable
/// asset. Caller owns the normalized root, capture/tree fence, reference verification and
/// separate attachment to draft or publication. No latest-version fallback occurs.
/// Supply the root normalized by `addon_media_storage` using `dunce`, not the
/// Windows verbatim path returned by `std::fs::canonicalize`. Native Windows
/// acceptance of the eventual caller remains a separate integration requirement.
///
/// # Errors
/// Returns redacted errors for unsafe roots/paths, database failures or changed bytes.
/// Missing selections, ambiguous rows, outside sources and non-portable local
/// names return None, preserving the unresolved selection without renaming it.
pub async fn inspect_owned_replacement(
    pool: &DbPool,
    managed_root: &Path,
    captured: &CapturedReplacement,
) -> Result<Option<SavedFile>> {
    let invalid =
        || RepositoryError::Conflict("the captured replacement source is unsafe or changed".into());
    let root = tokio::fs::symlink_metadata(managed_root)
        .await
        .map_err(|_| invalid())?;
    if !root.is_dir() || root.is_symlink() {
        return Err(invalid());
    }
    let Some(version) = captured.media_version_id else {
        return Ok(None);
    };
    let count: i64 =
        sqlx::query_scalar(r#"SELECT COUNT(*) FROM "MediaFile" WHERE "MediaVersionId"=?"#)
            .bind(version)
            .fetch_one(pool)
            .await
            .map_err(|_| invalid())?;
    if count != 1 {
        return Ok(None);
    }
    let Some(candidate) =
        crate::repositories::load_replacement_candidate(pool, captured.media_item_id, version)
            .await
            .map_err(|_| invalid())?
    else {
        return Ok(None);
    };
    if candidate.path.trim().is_empty() {
        return Ok(None);
    }
    let Ok(relative) = Path::new(&candidate.path).strip_prefix(managed_root) else {
        return Ok(None);
    };
    let mut components = Vec::new();
    for component in relative.components() {
        let Component::Normal(value) = component else {
            return Err(invalid());
        };
        let Some(value) = value.to_str() else {
            return Ok(None);
        };
        components.push(value);
    }
    // Test portability before reading media. A valid local name need not be a
    // portable saved-file name, and cannot block preserving an unresolved choice.
    let mut file = SavedFile {
        relative_path: components.join("/"),
        sha256: "0".repeat(64),
        byte_length: 0,
    };
    if validate_reference(&file).is_err() {
        return Ok(None);
    }
    let Some(source) = confined_file_if_present(managed_root, &file.relative_path)
        .await
        .map_err(|_| invalid())?
    else {
        return Ok(None);
    };
    let expected = tokio::fs::metadata(&source)
        .await
        .map_err(|_| invalid())?
        .len();
    i64::try_from(expected).map_err(|_| invalid())?;
    let (byte_length, sha256) = inspect_file(&source, expected)
        .await
        .map_err(|_| invalid())?;
    if byte_length != expected {
        return Err(invalid());
    }
    file.sha256 = sha256;
    file.byte_length = byte_length;
    Ok(Some(file))
}

#[cfg(test)]
mod tests;
