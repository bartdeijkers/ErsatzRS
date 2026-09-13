//! Guarded, bounded saved-file replacement; `SQLite` commitment remains the caller's job.

use std::fmt::Write as _;
use std::fs::{self, File};
use std::io::{self, BufReader, Read, Seek as _, Write};

use ersatzrs_core::repositories::{RepositoryError, Result};
use sha2::{Digest as _, Sha256};

use super::assembly::ManagedTreeGuard;
use super::document_io::{self, MAX_SAVED_DOCUMENT_BYTES};
use super::model::{SavedListDocument, SavedListPublication, SAVED_LIST_FILE_NAME};

/// Identity of the saved publication accepted by projection/queue, not proof of
/// active playback. Hash the existing portable model without another large buffer.
pub(crate) fn publication_digest(publication: Option<&SavedListPublication>) -> Result<String> {
    let mut output = DigestWriter {
        output: io::sink(),
        digest: Sha256::new(),
    };
    serde_json::to_writer(&mut output, &publication).map_err(|_| unavailable())?;
    Ok(hex(output.digest.finalize()))
}

/// Read the exact authoritative bytes under the caller's existing tree guard.
/// Run on a blocking thread, before acquiring `SQLite` write ownership.
pub(crate) fn read(tree: &ManagedTreeGuard) -> Result<(SavedListDocument, String)> {
    let path = tree.managed_root().join(SAVED_LIST_FILE_NAME);
    let metadata = fs::symlink_metadata(&path).map_err(|_| unavailable())?;
    if metadata.is_symlink() || !metadata.is_file() {
        return Err(unavailable());
    }
    let file = File::open(&path).map_err(|_| unavailable())?;
    let mut reader = BufReader::new(file);
    let document = document_io::read_document(&mut reader)
        .map_err(|error| RepositoryError::Conflict(error.to_string()))?;
    let mut file = reader.into_inner();
    file.rewind().map_err(|_| unavailable())?;
    let digest = digest_file(file)?;
    Ok((document, digest))
}

/// Replace a digest-matched document, synchronizing data before the namespace.
/// The caller must preserve this file if its subsequent database commit fails.
pub(crate) fn replace(
    tree: &ManagedTreeGuard,
    document: &SavedListDocument,
    expected_digest: &str,
) -> Result<String> {
    // Size/version errors must leave both the old file and its directory intact.
    document_io::serialized_size(document)
        .map_err(|error| RepositoryError::Conflict(error.to_string()))?;
    let path = tree.managed_root().join(SAVED_LIST_FILE_NAME);
    let metadata = fs::symlink_metadata(&path).map_err(|_| unavailable())?;
    if metadata.is_symlink()
        || !metadata.is_file()
        || digest_file(File::open(&path).map_err(|_| unavailable())?)? != expected_digest
    {
        return Err(RepositoryError::Conflict(
            "the saved list folder changed; refresh its database projection before saving".into(),
        ));
    }
    let mut temporary = tempfile::Builder::new()
        .prefix(".ersatzrs-saved-document-")
        .tempfile_in(tree.managed_root())
        .map_err(|_| unavailable())?;
    let digest = {
        let mut output = DigestWriter {
            output: temporary.as_file_mut(),
            digest: Sha256::new(),
        };
        document_io::write_document(&mut output, document)
            .map_err(|error| RepositoryError::Conflict(error.to_string()))?;
        hex(output.digest.finalize())
    };
    temporary.as_file().sync_all().map_err(|_| unavailable())?;
    // Close the file handle before Windows replacement. TempPath's cleanup only
    // addresses the now-absent temporary name, never the destination document.
    let temporary = temporary.into_temp_path();
    atomicwrites::replace_atomic(&temporary, &path).map_err(|_| {
        RepositoryError::Conflict(
            "the saved list file may have changed; verify and refresh its database projection before retrying".into(),
        )
    })?;
    Ok(digest)
}

fn digest_file(file: File) -> Result<String> {
    let mut reader = file.take(MAX_SAVED_DOCUMENT_BYTES + 1);
    let mut digest = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        let count = reader.read(&mut buffer).map_err(|_| unavailable())?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    if reader.limit() == 0 {
        return Err(RepositoryError::Conflict(
            "the saved list text exceeds the 256 MiB limit".into(),
        ));
    }
    Ok(hex(digest.finalize()))
}

struct DigestWriter<W> {
    output: W,
    digest: Sha256,
}

impl<W: Write> Write for DigestWriter<W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let count = self.output.write(bytes)?;
        self.digest.update(&bytes[..count]);
        Ok(count)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.output.flush()
    }
}

fn hex(bytes: impl AsRef<[u8]>) -> String {
    let mut output = String::with_capacity(64);
    for byte in bytes.as_ref() {
        let _ = write!(output, "{byte:02x}");
    }
    output
}

fn unavailable() -> RepositoryError {
    RepositoryError::Conflict("the saved list file is unavailable or could not be durably replaced; retry after it is available".into())
}
