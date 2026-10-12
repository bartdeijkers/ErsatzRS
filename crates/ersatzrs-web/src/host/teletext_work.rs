//! Immediate process-wide teletext admission with actual-worker ownership.

use std::{num::NonZeroUsize, time::Instant};

use ersatzrs_app::ApplicationServices;
use ersatzrs_core::locking::{TeletextWorkGuard, TeletextWorkHold};

/// Existing `ConfigElement` resource policy, read afresh at operation admission.
pub const MAX_CONCURRENT_SETTING: &str = "teletext.work.max_concurrent";

/// Redacted admission failures; none changes accepted output or playback.
#[derive(Debug, thiserror::Error)]
pub enum TeletextWorkError {
    #[error("teletext work deadline has expired")]
    TimedOut,
    #[error("teletext work configuration is unavailable")]
    ConfigurationUnavailable,
    #[error("teletext work concurrency must be a positive integer")]
    InvalidConfiguration,
    #[error("teletext work capacity is occupied")]
    Saturated,
}

/// Admit before retrieving original documents or launching producer/native work.
/// Absence serializes work conservatively; it is not a per-candidate RSS bound.
/// A lower configured limit affects new admission without preempting owners.
/// There is no waiting queue and no use of ordinary playback reservations.
/// # Errors
/// Rejects expired deadlines, invalid/unavailable configuration or saturation.
pub async fn begin(
    application: &ApplicationServices,
    deadline: Option<Instant>,
) -> Result<TeletextWorkGuard, TeletextWorkError> {
    check_deadline(deadline)?;
    let read = application.get_optional_setting(MAX_CONCURRENT_SETTING);
    let configured = match deadline {
        Some(deadline) => tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), read)
            .await
            .map_err(|_| TeletextWorkError::TimedOut)?,
        None => read.await,
    }
    .map_err(|_| TeletextWorkError::ConfigurationUnavailable)?;
    let maximum = match configured {
        None => NonZeroUsize::MIN,
        Some(value) => value
            .parse::<NonZeroUsize>()
            .map_err(|_| TeletextWorkError::InvalidConfiguration)?,
    };
    check_deadline(deadline)?;
    application
        .operations()
        .try_begin_teletext_work(maximum.get())
        .ok_or(TeletextWorkError::Saturated)
}

fn check_deadline(deadline: Option<Instant>) -> Result<(), TeletextWorkError> {
    if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
        Err(TeletextWorkError::TimedOut)
    } else {
        Ok(())
    }
}

/// Move a shared lease into the actual queued/running native closure.
/// Dropping its `JoinHandle` or requester cannot release this worker's ownership.
pub(super) fn spawn_blocking<T: Send + 'static>(
    hold: TeletextWorkHold,
    work: impl FnOnce() -> T + Send + 'static,
) -> tokio::task::JoinHandle<T> {
    tokio::task::spawn_blocking(move || {
        let _hold = hold;
        work()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ersatzrs_core::locking::EntityLocker;

    #[tokio::test]
    async fn abandoned_native_worker_retains_admission_until_actual_completion() {
        let locker = EntityLocker::default();
        let guard = TeletextWorkGuard::try_acquire(&locker, 1).unwrap();
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (finish_tx, finish_rx) = std::sync::mpsc::channel();
        let worker = spawn_blocking(guard.hold_work(), move || {
            started_tx.send(()).unwrap();
            finish_rx.recv().unwrap();
        });
        started_rx.await.unwrap();
        drop(worker);
        drop(guard);
        assert!(TeletextWorkGuard::try_acquire(&locker, 1).is_none());
        finish_tx.send(()).unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                if let Some(guard) = TeletextWorkGuard::try_acquire(&locker, 1) {
                    drop(guard);
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert_eq!(locker.active_teletext_work(), 0);
    }

    #[tokio::test]
    async fn native_panic_releases_worker_ownership() {
        let locker = EntityLocker::default();
        let guard = TeletextWorkGuard::try_acquire(&locker, 1).unwrap();
        let worker = spawn_blocking(guard.hold_work(), || panic!("synthetic worker failure"));
        drop(guard);
        assert!(worker.await.unwrap_err().is_panic());
        assert_eq!(locker.active_teletext_work(), 0);
        assert!(TeletextWorkGuard::try_acquire(&locker, 1).is_some());
    }
}
