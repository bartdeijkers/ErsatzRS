//! Bounded saved-document encoding, separate from validation and durable commit.

use std::io::{self, BufRead, Read, Seek, SeekFrom, Write};

use serde::{de::DeserializeOwned, Deserialize};

use super::model::{SavedListDocument, SAVED_LIST_VERSION};

/// ADR 0093: saved draft plus publication text; external asset bytes do not count.
pub const MAX_SAVED_DOCUMENT_BYTES: u64 = 256 * 1024 * 1024;

/// Redacted outcomes; copied text, paths and parser messages are never exposed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SavedDocumentError {
    #[error("the saved list text exceeds the 256 MiB limit")]
    TooLarge,
    #[error("the saved list document version is not supported")]
    UnsupportedVersion,
    #[error("the saved list document is invalid")]
    Invalid,
    #[error("the saved list document could not be read or written")]
    Io,
}

/// Decode from a caller-owned, buffered regular file under the managed-tree fence.
///
/// This synchronous helper belongs in the caller's blocking filesystem section.
/// It does not open paths, spawn work, grant execution authority or validate the
/// add-on, field values or referenced assets. Those receiving checks still apply.
/// Two bounded passes inspect the version then decode directly into the model,
/// without a second complete JSON string or `Value` tree. The byte cap is not a
/// parsed-memory ceiling. Input is read from offset zero, regardless of position.
///
/// # Errors
/// Rejects oversize, unsupported versions, malformed documents and IO failures.
pub fn read_document(
    reader: &mut (impl BufRead + Seek),
) -> Result<SavedListDocument, SavedDocumentError> {
    read_with_limit(reader, MAX_SAVED_DOCUMENT_BYTES)
}

fn read_with_limit(
    reader: &mut (impl Read + Seek),
    limit: u64,
) -> Result<SavedListDocument, SavedDocumentError> {
    // Unknown future fields must not mask a readable unsupported version.
    #[derive(Deserialize)]
    struct Version {
        version: u32,
    }

    let length = reader
        .seek(SeekFrom::End(0))
        .map_err(|_| SavedDocumentError::Io)?;
    if length > limit {
        return Err(SavedDocumentError::TooLarge);
    }
    reader.rewind().map_err(|_| SavedDocumentError::Io)?;
    let version: Version = read_pass(reader, limit)?;
    if version.version != SAVED_LIST_VERSION {
        return Err(SavedDocumentError::UnsupportedVersion);
    }
    reader.rewind().map_err(|_| SavedDocumentError::Io)?;
    let document: SavedListDocument = read_pass(reader, limit)?;
    if document.version != SAVED_LIST_VERSION {
        return Err(SavedDocumentError::UnsupportedVersion);
    }
    Ok(document)
}

fn read_pass<T: DeserializeOwned>(
    reader: &mut impl Read,
    limit: u64,
) -> Result<T, SavedDocumentError> {
    // The initial seek is not sufficient if input grows while being read.
    let mut bounded = reader.take(limit + 1);
    let result = serde_json::from_reader(&mut bounded);
    if bounded.limit() == 0 {
        return Err(SavedDocumentError::TooLarge);
    }
    result.map_err(|error| json_error(&error))
}

/// Measure compact JSON without allocating its encoded text or writing a file.
///
/// # Errors
/// Rejects unsupported versions, invalid serialization and an oversized document.
pub fn serialized_size(document: &SavedListDocument) -> Result<u64, SavedDocumentError> {
    size_with_limit(document, MAX_SAVED_DOCUMENT_BYTES)
}

fn size_with_limit(document: &SavedListDocument, limit: u64) -> Result<u64, SavedDocumentError> {
    if document.version != SAVED_LIST_VERSION {
        return Err(SavedDocumentError::UnsupportedVersion);
    }
    let mut counter = SizeCounter {
        remaining: limit,
        exceeded: false,
    };
    let result = serde_json::to_writer(&mut counter, document);
    if counter.exceeded {
        return Err(SavedDocumentError::TooLarge);
    }
    result.map_err(|error| json_error(&error))?;
    Ok(limit - counter.remaining)
}

/// Write compact JSON through a buffer only after a successful size preflight.
///
/// The caller must supply an unpublished temporary output, not the authoritative
/// file. Oversize and version errors write no bytes; IO errors may leave partial
/// temporary output. The buffer is flushed before success; the caller owns file
/// synchronization, cleanup, durable replacement and
/// database ordering. This function neither commits nor acknowledges a Save.
///
/// # Errors
/// Rejects oversize, unsupported versions, invalid serialization and IO failures.
pub fn write_document(
    writer: &mut impl Write,
    document: &SavedListDocument,
) -> Result<u64, SavedDocumentError> {
    write_with_limit(writer, document, MAX_SAVED_DOCUMENT_BYTES)
}

fn write_with_limit(
    writer: &mut impl Write,
    document: &SavedListDocument,
    limit: u64,
) -> Result<u64, SavedDocumentError> {
    let size = size_with_limit(document, limit)?;
    let mut buffered = io::BufWriter::new(writer);
    serde_json::to_writer(&mut buffered, document).map_err(|error| json_error(&error))?;
    buffered.flush().map_err(|_| SavedDocumentError::Io)?;
    Ok(size)
}

fn json_error(error: &serde_json::Error) -> SavedDocumentError {
    if error.is_io() {
        SavedDocumentError::Io
    } else {
        SavedDocumentError::Invalid
    }
}

struct SizeCounter {
    remaining: u64,
    exceeded: bool,
}

impl Write for SizeCounter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let Some(remaining) = self.remaining.checked_sub(bytes.len() as u64) else {
            self.exceeded = true;
            return Err(io::Error::other("saved document size limit"));
        };
        self.remaining = remaining;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests;
