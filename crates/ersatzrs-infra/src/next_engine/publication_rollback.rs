//! Reversible file replacement for the private-SQL build finalizer.

use super::{io_error, PreparedPlayout, PublicationError};
use std::{fs::File, io, path::PathBuf};

/// Used only on the owned blocking finalizer. Drop compensates an unaccepted
/// replacement, including unwinding; accepted SQL disarms every owner first.
pub(crate) struct AppliedPlayout {
    destination: PathBuf,
    previous: Option<tempfile::NamedTempFile>,
    accepted: bool,
}

impl PreparedPlayout {
    pub(crate) fn publish_reversible(
        self,
        is_current: impl Fn() -> bool,
    ) -> Result<AppliedPlayout, PublicationError> {
        let previous = match File::open(&self.destination) {
            Ok(mut source) => {
                let parent = self.destination.parent().ok_or_else(|| {
                    io_error("prepare rollback", &io::ErrorKind::InvalidInput.into())
                })?;
                let mut previous = tempfile::Builder::new()
                    .prefix(".next-rollback-")
                    .tempfile_in(parent)
                    .map_err(|error| io_error("prepare rollback", &error))?;
                io::copy(&mut source, previous.as_file_mut())
                    .map_err(|error| io_error("copy rollback", &error))?;
                previous
                    .as_file()
                    .sync_all()
                    .map_err(|error| io_error("flush rollback", &error))?;
                Some(previous)
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => return Err(io_error("read rollback", &error)),
        };
        let destination = self.destination.clone();
        self.publish(is_current)?;
        Ok(AppliedPlayout {
            destination,
            previous,
            accepted: false,
        })
    }
}

impl AppliedPlayout {
    /// SQL has committed. File rollback is no longer permitted.
    pub(crate) fn accept(&mut self) {
        self.accepted = true;
        self.previous.take();
    }

    pub(crate) fn restore(&mut self) -> Result<(), PublicationError> {
        if self.accepted {
            return Ok(());
        }
        // Compensation must not inherit the expired *candidate* deadline. It
        // restores the accepted generation and retains the bounded I/O retry.
        let result = if let Some(previous) = self.previous.as_mut() {
            let parent = self.destination.parent().ok_or_else(|| {
                io_error("prepare restoration", &io::ErrorKind::InvalidInput.into())
            })?;
            let mut source = File::open(previous.path())
                .map_err(|error| io_error("read restoration", &error))?;
            let mut pending = tempfile::Builder::new()
                .prefix(".next-restore-")
                .tempfile_in(parent)
                .map_err(|error| io_error("prepare restoration", &error))?;
            io::copy(&mut source, pending.as_file_mut())
                .map_err(|error| io_error("copy restoration", &error))?;
            pending
                .as_file()
                .sync_all()
                .map_err(|error| io_error("flush restoration", &error))?;
            PreparedPlayout {
                pending,
                destination: self.destination.clone(),
                deadline: None,
            }
            .publish(|| true)
        } else {
            match std::fs::remove_file(&self.destination) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(io_error("withdraw candidate", &error)),
            }
        };
        if result.is_ok() {
            self.accept();
        }
        result
    }
}

impl Drop for AppliedPlayout {
    fn drop(&mut self) {
        if let Err(error) = self.restore() {
            // Retain the old bytes for a storage incident. Startup always
            // projects accepted SQL before launching a new Next producer.
            if let Some(previous) = self.previous.take() {
                let _ = previous.keep();
            }
            tracing::error!(%error, "Next file rollback failed; accepted SQL must be republished");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::next_engine::publication::{prepare_with, PLAYOUT_FILENAME};
    use std::io::Write;

    fn staged(folder: &std::path::Path) -> PreparedPlayout {
        prepare_with(
            folder,
            |file| {
                file.write_all(b"candidate").unwrap();
                Ok(())
            },
            || true,
            None,
        )
        .unwrap()
    }

    #[test]
    fn rollback_restores_exact_previous_bytes_and_cleans_private_files() {
        for exists in [false, true] {
            let root = tempfile::tempdir().unwrap();
            let destination = root.path().join(PLAYOUT_FILENAME);
            if exists {
                std::fs::write(&destination, b"accepted").unwrap();
            }
            let mut applied = staged(root.path()).publish_reversible(|| true).unwrap();
            assert_eq!(std::fs::read(&destination).unwrap(), b"candidate");
            applied.restore().unwrap();
            if exists {
                assert_eq!(std::fs::read(&destination).unwrap(), b"accepted");
            } else {
                assert!(!destination.exists());
            }
            assert_eq!(
                std::fs::read_dir(root.path()).unwrap().count(),
                usize::from(exists)
            );
        }
    }

    #[test]
    fn unwinding_rolls_back_but_acceptance_keeps_the_candidate() {
        let root = tempfile::tempdir().unwrap();
        let destination = root.path().join(PLAYOUT_FILENAME);
        std::fs::write(&destination, b"accepted").unwrap();
        let result = std::panic::catch_unwind(|| {
            let _applied = staged(root.path()).publish_reversible(|| true).unwrap();
            panic!("finalizer failed before SQL commit");
        });
        assert!(result.is_err());
        assert_eq!(std::fs::read(&destination).unwrap(), b"accepted");
        let mut applied = staged(root.path()).publish_reversible(|| true).unwrap();
        applied.accept();
        drop(applied);
        assert_eq!(std::fs::read(&destination).unwrap(), b"candidate");
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
    }
}
