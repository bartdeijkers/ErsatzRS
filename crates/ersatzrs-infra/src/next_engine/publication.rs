//! Publish a complete schedule without exposing a partially written generation.
//!
//! Upstream `SyncNextPlayoutHandler` swaps a directory symlink and skips Windows.
//! Next's loader reopens the matching JSON on each item selection, so one stable,
//! broad-window filename permits an atomic file replacement on every platform.
//! Item timestamps, rather than the filename, still determine actual coverage.

use std::fs::File;
use std::io;
use std::path::Path;

use chrono::Datelike;

use super::protocol::{Playout, PLAYOUT_VERSION};

/// Next accepts compact ISO 8601 filename bounds; the finish is exclusive.
pub const PLAYOUT_FILENAME: &str = "00010101T000000Z_99991231T235959Z.json";

/// Diagnostics deliberately omit paths and serialized source credentials.
#[derive(Debug, thiserror::Error)]
pub enum PublicationError {
    #[error("Next playout schema version is unsupported")]
    UnsupportedVersion,
    #[error("Next playout has an invalid item time window")]
    InvalidWindow,
    #[error("Next playout items must be ordered by start time")]
    UnorderedItems,
    #[error("Next playout {operation} failed ({kind:?})")]
    Io {
        operation: &'static str,
        kind: io::ErrorKind,
    },
    #[error("Next playout serialization failed")]
    Serialization,
}

/// Atomically replace the complete playout in an exclusively owned folder.
///
/// The caller must serialize publication with schedule reads so an older read
/// cannot overwrite a newer generation. Run this blocking operation through
/// `spawn_blocking` when called from an async task. An empty schedule is valid:
/// publishing it withdraws old items rather than leaving stale playback active.
/// Only this publisher's stable JSON filename may be present as a matching
/// playout file in the folder; temporary filenames cannot match Next's parser.
///
/// # Errors
/// Returns a redacted error for invalid input or a failed filesystem operation.
/// Before replacement succeeds, the previous published file remains intact.
pub fn publish_playout(folder: &Path, playout: &Playout) -> Result<(), PublicationError> {
    validate(playout)?;
    replace_with(folder, |file| {
        serde_json::to_writer(file, playout).map_err(|_| PublicationError::Serialization)
    })
}

fn validate(playout: &Playout) -> Result<(), PublicationError> {
    if playout.version != PLAYOUT_VERSION {
        return Err(PublicationError::UnsupportedVersion);
    }
    for item in &playout.items {
        // Match the fixed filename's inclusive start and exclusive finish.
        if item.start >= item.finish
            || item.start.year() < 1
            || item.finish.timestamp() > 253_402_300_799
            || (item.finish.timestamp() == 253_402_300_799
                && item.finish.timestamp_subsec_nanos() != 0)
        {
            return Err(PublicationError::InvalidWindow);
        }
    }
    if playout
        .items
        .windows(2)
        .any(|pair| pair[0].start > pair[1].start)
    {
        return Err(PublicationError::UnorderedItems);
    }
    Ok(())
}

fn replace_with(
    folder: &Path,
    write: impl FnOnce(&mut File) -> Result<(), PublicationError>,
) -> Result<(), PublicationError> {
    std::fs::create_dir_all(folder).map_err(|error| io_error("create directory", &error))?;
    let mut pending = tempfile::Builder::new()
        .prefix(".next-")
        .tempfile_in(folder)
        .map_err(|error| io_error("create temporary file", &error))?;
    write(pending.as_file_mut())?;
    pending
        .as_file()
        .sync_all()
        .map_err(|error| io_error("flush", &error))?;
    // tempfile persists with replacement on Windows as well as Unix. Both paths
    // are in the same directory/filesystem, so readers see either complete file.
    pending
        .persist(folder.join(PLAYOUT_FILENAME))
        .map_err(|error| io_error("replace", &error.error))?;
    Ok(())
}

fn io_error(operation: &'static str, error: &io::Error) -> PublicationError {
    PublicationError::Io {
        operation,
        kind: error.kind(),
    }
}

#[cfg(test)]
#[path = "publication_tests.rs"]
mod tests;
