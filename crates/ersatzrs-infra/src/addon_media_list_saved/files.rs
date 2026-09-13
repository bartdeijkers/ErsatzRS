//! Read-only byte verification for portable saved-file references.

use std::{fmt::Write as _, path::Path};

use sha2::{Digest as _, Sha256};
use tokio::io::AsyncReadExt as _;

use super::model::SavedFile;

/// File presence alone never authorizes playback or substitutes delivery modes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SavedFileState {
    Verified,
    Missing,
}

/// Safe diagnostics: never echo copied paths, URLs or file contents.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SavedFileError {
    #[error("the saved file reference is not portable")]
    InvalidReference,
    #[error("the saved file is unsafe or could not be read")]
    Unavailable,
    #[error("the saved file does not match its recorded content")]
    Integrity,
}

/// Validate the portable path and recorded SHA before any filesystem access.
///
/// The format uses forward slashes on every platform and excludes Windows
/// device/alias names even when validated on Linux. See the Win32 naming rules:
/// <https://learn.microsoft.com/en-us/windows/win32/fileio/naming-a-file>.
///
/// # Errors
/// Rejects traversal, absolute/device paths, ambiguous components and invalid SHA.
pub fn validate_reference(file: &SavedFile) -> Result<(), SavedFileError> {
    let path = &file.relative_path;
    if path.is_empty()
        || path.chars().any(|character| {
            character.is_control()
                || matches!(character, '\\' | ':' | '<' | '>' | '"' | '|' | '?' | '*')
        })
        || path.split('/').any(invalid_component)
        || file.sha256.len() != 64
        || !file
            .sha256
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(SavedFileError::InvalidReference);
    }
    Ok(())
}

fn invalid_component(component: &str) -> bool {
    if component.is_empty()
        || component == "."
        || component == ".."
        || component.ends_with([' ', '.'])
    {
        return true;
    }
    let stem = component
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    if matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL") {
        return true;
    }
    ["COM", "LPT"].iter().any(|prefix| {
        stem.strip_prefix(prefix).is_some_and(|suffix| {
            matches!(
                suffix,
                "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
            )
        })
    })
}

/// Verify one reference without writes or loading media into memory.
///
/// Callers own the managed-tree fence and cancellation of this async future;
/// no detached verification task is spawned. Reads use one bounded buffer and
/// refuse growth past the recorded length. `Missing` is distinct from corruption
/// or unsafe paths. The receiving document validator must reject missing required
/// assets and turn only missing Download ahead media into local preparation.
/// This proof does not validate the document or grant copied execution ownership.
///
/// # Errors
/// Rejects unsafe references, symlinks, non-files, IO failures and changed bytes.
pub async fn verify_file(
    root: &Path,
    reference: &SavedFile,
) -> Result<SavedFileState, SavedFileError> {
    let Some(mut file) = open_file(root, reference).await? else {
        return Ok(SavedFileState::Missing);
    };
    let mut digest = Sha256::new();
    let mut bytes = 0u64;
    let mut buffer = vec![0u8; 16 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .await
            .map_err(|_| SavedFileError::Unavailable)?;
        if count == 0 {
            break;
        }
        bytes = bytes
            .checked_add(count as u64)
            .filter(|total| *total <= reference.byte_length)
            .ok_or(SavedFileError::Integrity)?;
        digest.update(&buffer[..count]);
    }
    let actual = hex_digest(&digest.finalize());
    if bytes != reference.byte_length || actual != reference.sha256 {
        return Err(SavedFileError::Integrity);
    }
    Ok(SavedFileState::Verified)
}

/// Read one required image or subtitle, bounded before allocating its contents.
///
/// The returned bytes themselves are verified; a separate earlier path/hash
/// check is not substituted for verifying the bytes being consumed. Callers
/// retain the same managed-tree fence required by [`verify_file`].
///
/// # Errors
/// Rejects missing, unsafe, changed or oversized files without writing anything.
pub async fn read_verified_file(
    root: &Path,
    reference: &SavedFile,
    maximum_bytes: u64,
) -> Result<Vec<u8>, SavedFileError> {
    if reference.byte_length > maximum_bytes {
        return Err(SavedFileError::Integrity);
    }
    let file = open_file(root, reference)
        .await?
        .ok_or(SavedFileError::Unavailable)?;
    let mut bytes = Vec::new();
    file.take(reference.byte_length.saturating_add(1))
        .read_to_end(&mut bytes)
        .await
        .map_err(|_| SavedFileError::Unavailable)?;
    if bytes.len() as u64 != reference.byte_length
        || hex_digest(&Sha256::digest(&bytes)) != reference.sha256
    {
        return Err(SavedFileError::Integrity);
    }
    Ok(bytes)
}

fn hex_digest(digest: &[u8]) -> String {
    let mut actual = String::with_capacity(64);
    for byte in digest {
        let _ = write!(actual, "{byte:02x}");
    }
    actual
}

async fn open_file(
    root: &Path,
    reference: &SavedFile,
) -> Result<Option<tokio::fs::File>, SavedFileError> {
    validate_reference(reference)?;
    let root_metadata = tokio::fs::symlink_metadata(root)
        .await
        .map_err(|_| SavedFileError::Unavailable)?;
    if root_metadata.is_symlink() || !root_metadata.is_dir() {
        return Err(SavedFileError::Unavailable);
    }
    let path =
        crate::addon_media_list_artifacts::confined_file_if_present(root, &reference.relative_path)
            .await
            .map_err(|_| SavedFileError::Unavailable)?;
    let Some(path) = path else {
        return Ok(None);
    };
    let file = tokio::fs::File::open(path)
        .await
        .map_err(|_| SavedFileError::Unavailable)?;
    let metadata = file
        .metadata()
        .await
        .map_err(|_| SavedFileError::Unavailable)?;
    if !metadata.is_file() {
        return Err(SavedFileError::Unavailable);
    }
    if metadata.len() != reference.byte_length {
        return Err(SavedFileError::Integrity);
    }
    Ok(Some(file))
}

#[cfg(test)]
mod tests;
