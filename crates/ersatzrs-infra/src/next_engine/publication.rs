//! Publish a complete schedule without exposing a partially written generation.
//!
//! Upstream `SyncNextPlayoutHandler` swaps Unix symlinks or Windows junctions.
//! Next's loader reopens the matching JSON on each item selection, so one stable,
//! broad-window filename permits an atomic file replacement on every platform.
//! Item timestamps, rather than the filename, still determine actual coverage.

use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};

use chrono::Datelike;

use super::protocol::{Playout, PLAYOUT_VERSION};

#[path = "publication_rollback.rs"]
mod rollback;
pub(super) use rollback::AppliedPlayout;

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
    #[error("Next playout publication was cancelled before replacement")]
    Cancelled,
    #[error("Next playout publication missed its safe deadline")]
    Expired,
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
    publish_playout_checked(folder, playout, || true, None)
}

/// Check ownership again after serialization and flush, immediately before the
/// atomic replacement. Cancellation cannot undo a replacement already started.
pub(super) fn publish_playout_checked(
    folder: &Path,
    playout: &Playout,
    is_current: impl Fn() -> bool,
    deadline: Option<std::time::Instant>,
) -> Result<(), PublicationError> {
    validate(playout)?;
    replace_with(
        folder,
        |file| serde_json::to_writer(file, playout).map_err(|_| PublicationError::Serialization),
        is_current,
        deadline,
    )
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
    is_current: impl Fn() -> bool,
    deadline: Option<std::time::Instant>,
) -> Result<(), PublicationError> {
    prepare_with(folder, write, &is_current, deadline)?.publish(is_current)
}

pub(super) fn prepare_playout_checked(
    folder: &Path,
    playout: &Playout,
    is_current: impl Fn() -> bool,
    deadline: std::time::Instant,
) -> Result<PreparedPlayout, PublicationError> {
    validate(playout)?;
    prepare_with(
        folder,
        |file| serde_json::to_writer(file, playout).map_err(|_| PublicationError::Serialization),
        is_current,
        Some(deadline),
    )
}

fn prepare_with(
    folder: &Path,
    write: impl FnOnce(&mut File) -> Result<(), PublicationError>,
    is_current: impl Fn() -> bool,
    deadline: Option<std::time::Instant>,
) -> Result<PreparedPlayout, PublicationError> {
    check_deadline(deadline)?;
    if !is_current() {
        return Err(PublicationError::Cancelled);
    }
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
    check_deadline(deadline)?;
    if !is_current() {
        return Err(PublicationError::Cancelled);
    }
    Ok(PreparedPlayout {
        pending,
        destination: folder.join(PLAYOUT_FILENAME),
        deadline,
    })
}

/// A completely serialized and flushed file which Next cannot discover yet.
/// Dropping this owner removes the temporary file without changing publication.
pub(super) struct PreparedPlayout {
    pending: tempfile::NamedTempFile,
    destination: PathBuf,
    deadline: Option<std::time::Instant>,
}

impl PreparedPlayout {
    pub(crate) fn restrict_deadline(&mut self, deadline: std::time::Instant) {
        self.deadline = Some(
            self.deadline
                .map_or(deadline, |previous| previous.min(deadline)),
        );
    }

    pub(super) fn publish(self, is_current: impl Fn() -> bool) -> Result<(), PublicationError> {
        self.publish_with(
            is_current,
            |file, destination| file.persist(destination),
            std::thread::sleep,
        )
    }

    fn publish_with(
        self,
        is_current: impl Fn() -> bool,
        mut persist: impl FnMut(tempfile::NamedTempFile, &Path) -> Result<File, tempfile::PersistError>,
        mut wait: impl FnMut(std::time::Duration),
    ) -> Result<(), PublicationError> {
        let mut pending = self.pending;
        let mut retries = 0;
        loop {
            check_deadline(self.deadline)?;
            if !is_current() {
                return Err(PublicationError::Cancelled);
            }
            // tempfile replaces on Windows and Unix. Retain the exact staged
            // file on transient failure; do not reproject a committed build or
            // release publisher/source admission between replacement attempts.
            match persist(pending, &self.destination) {
                Ok(_) => return Ok(()),
                Err(error) => {
                    if retries == 7 || !retryable_replacement(&error.error) {
                        return Err(io_error("replace", &error.error));
                    }
                    pending = error.file;
                }
            }
            // At most eight attempts and 1.775 seconds of backoff, further
            // bounded by the original deadline. Cancellation is rechecked
            // after every sleep and before any subsequent replacement.
            let delay = std::time::Duration::from_millis((25_u64 << retries).min(500));
            let delay = self.deadline.map_or(delay, |deadline| {
                delay.min(deadline.saturating_duration_since(std::time::Instant::now()))
            });
            wait(delay);
            retries += 1;
        }
    }
}

fn retryable_replacement(error: &io::Error) -> bool {
    if matches!(
        error.kind(),
        io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock
    ) {
        return true;
    }
    // Antivirus scanners and non-delete-sharing readers can temporarily deny
    // replacement on Windows. The same raw numbers mean different errors on
    // Unix, where an ordinary open reader does not prevent atomic replacement.
    #[cfg(windows)]
    {
        matches!(error.raw_os_error(), Some(5 | 32 | 33))
    }
    #[cfg(not(windows))]
    {
        false
    }
}

fn check_deadline(deadline: Option<std::time::Instant>) -> Result<(), PublicationError> {
    if deadline.is_some_and(|limit| std::time::Instant::now() >= limit) {
        return Err(PublicationError::Expired);
    }
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
