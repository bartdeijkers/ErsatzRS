//! State-folder ownership from ErsatzTV `Program.TryLockConfigFolder` (UCU-13).
//!
//! Keep the handle alive for the host lifetime. Never remove the lock file:
//! deleting a locked inode could let a new process lock a different file.
//! A leftover file is harmless; the operating system releases the lock on exit.

use std::{
    fs::{self, File, OpenOptions, TryLockError},
    io,
    path::Path,
};

use thiserror::Error;

/// Lock file in the persistent application state folder.
pub const LOCK_FILE_NAME: &str = "ersatzrs.lock";

/// Exclusive ownership of one server state folder, released when dropped.
#[derive(Debug)]
pub struct Guard {
    _file: File,
}

/// Contention is distinct from unavailable filesystem locking.
#[derive(Debug, Error)]
pub enum Error {
    /// A cooperating server already owns this state folder.
    #[error("another ErsatzRS instance is already running with this state folder")]
    AlreadyRunning,
    /// The folder or lock file could not be opened or locked.
    #[error("unable to lock the ErsatzRS state folder: {0}")]
    Unavailable(#[source] io::Error),
}

impl Guard {
    /// Try to own a server state folder without waiting for another process.
    ///
    /// Read/write access is preferred; an existing read-only file is a fallback,
    /// matching upstream. Existing lock contents are never overwritten.
    /// Network-shared `SQLite` remains unsupported (ADR 0080).
    ///
    /// # Errors
    ///
    /// Returns [`Error::AlreadyRunning`] on contention. Other filesystem errors
    /// return [`Error::Unavailable`]; the composition root owns that policy.
    pub fn acquire(folder: &Path) -> Result<Self, Error> {
        // An existing readable lock can still work when directory creation fails.
        let _ = fs::create_dir_all(folder);
        let path = folder.join(LOCK_FILE_NAME);
        if fs::symlink_metadata(&path)
            .is_ok_and(|metadata| metadata.is_symlink() || !metadata.is_file())
        {
            return Err(Error::Unavailable(io::Error::new(
                io::ErrorKind::InvalidInput,
                "the instance lock must be a regular file",
            )));
        }
        match lock_with_access(&path, true) {
            Err(Error::Unavailable(_)) => lock_with_access(&path, false),
            result => result,
        }
    }
}

fn lock_with_access(path: &Path, writable: bool) -> Result<Guard, Error> {
    let file = OpenOptions::new()
        .read(true)
        .write(writable)
        .create(writable)
        .truncate(false)
        .open(path)
        .map_err(Error::Unavailable)?;
    match file.try_lock() {
        Ok(()) => Ok(Guard { _file: file }),
        Err(TryLockError::WouldBlock) => Err(Error::AlreadyRunning),
        Err(TryLockError::Error(error)) => Err(Error::Unavailable(error)),
    }
}
