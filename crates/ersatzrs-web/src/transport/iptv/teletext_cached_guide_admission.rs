//! Response-local waiting for an existing guide proof, never a producer queue.

use std::{
    future::Future,
    time::{Duration, Instant},
};

use ersatzrs_core::locking::TeletextWorkGuard;
use tokio::sync::futures::Notified;

use crate::host::teletext_work::TeletextWorkError;

pub(super) async fn begin_cached_confirmation(
    application: &ersatzrs_app::ApplicationServices,
    deadline: Instant,
    cadence: Duration,
) -> Result<TeletextWorkGuard, TeletextWorkError> {
    wait_for_admission(
        || application.operations().teletext_work_released(),
        deadline,
        cadence,
        || crate::host::teletext_work::begin(application, Some(deadline)),
    )
    .await
}

async fn wait_for_admission<'a, F, Fut>(
    released: impl Fn() -> Notified<'a>,
    deadline: Instant,
    cadence: Duration,
    mut admit: F,
) -> Result<TeletextWorkGuard, TeletextWorkError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<TeletextWorkGuard, TeletextWorkError>>,
{
    if cadence.is_zero() {
        return Err(TeletextWorkError::InvalidConfiguration);
    }
    loop {
        if Instant::now() >= deadline {
            return Err(TeletextWorkError::TimedOut);
        }
        // Register before attempting admission: a last worker can release
        // between the failed attempt and awaiting without losing its wakeup.
        let notified = released();
        tokio::pin!(notified);
        notified.as_mut().enable();
        match admit().await {
            Ok(guard) if Instant::now() < deadline => return Ok(guard),
            Ok(_) => return Err(TeletextWorkError::TimedOut),
            Err(TeletextWorkError::Saturated) => {}
            Err(error) => return Err(error),
        }
        let next_poll = Instant::now()
            .checked_add(cadence)
            .map_or(deadline, |next| next.min(deadline));
        tokio::select! {
            () = notified.as_mut() => {},
            () = tokio::time::sleep_until(next_poll.into()) => {},
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ersatzrs_core::locking::EntityLocker;
    use std::future::ready;

    #[tokio::test]
    async fn cached_admission_retains_worker_ownership_handles_release_race_and_deadline() {
        let locker = EntityLocker::default();
        let owner = TeletextWorkGuard::try_acquire(&locker, 1).unwrap();
        let mut actual_worker = Some(owner.hold_work());
        drop(owner);
        {
            let mut cancelled = Box::pin(wait_for_admission(
                || locker.teletext_work_released(),
                Instant::now() + Duration::from_secs(1),
                Duration::from_secs(1),
                || {
                    ready(
                        TeletextWorkGuard::try_acquire(&locker, 1)
                            .ok_or(TeletextWorkError::Saturated),
                    )
                },
            ));
            assert!(
                tokio::time::timeout(Duration::from_millis(10), cancelled.as_mut())
                    .await
                    .is_err()
            );
            // Cancelling the waiter must not release the actual worker's slot.
        }
        assert_eq!(locker.active_teletext_work(), 1);
        let mut attempts = 0;
        let next = wait_for_admission(
            || locker.teletext_work_released(),
            Instant::now() + Duration::from_secs(1),
            Duration::from_secs(1),
            || {
                attempts += 1;
                let result =
                    TeletextWorkGuard::try_acquire(&locker, 1).ok_or(TeletextWorkError::Saturated);
                // Release after the saturated attempt, before the helper awaits.
                drop(actual_worker.take());
                ready(result)
            },
        )
        .await
        .unwrap();
        assert_eq!(
            attempts, 2,
            "enabled notification must avoid the lost wakeup"
        );
        assert_eq!(locker.active_teletext_work(), 1);
        drop(next);
        assert_eq!(locker.active_teletext_work(), 0);
        let owner = TeletextWorkGuard::try_acquire(&locker, 1).unwrap();
        let result = wait_for_admission(
            || locker.teletext_work_released(),
            Instant::now() + Duration::from_millis(20),
            Duration::from_secs(1),
            || {
                ready(
                    TeletextWorkGuard::try_acquire(&locker, 1).ok_or(TeletextWorkError::Saturated),
                )
            },
        )
        .await;
        assert!(matches!(result, Err(TeletextWorkError::TimedOut)));
        assert_eq!(locker.active_teletext_work(), 1);
        drop(owner);
        assert_eq!(locker.active_teletext_work(), 0);
    }

    #[tokio::test]
    async fn cached_admission_never_retries_configuration_failures() {
        let locker = EntityLocker::default();
        let owner = TeletextWorkGuard::try_acquire(&locker, 1).unwrap();
        for error in [
            TeletextWorkError::InvalidConfiguration,
            TeletextWorkError::ConfigurationUnavailable,
        ] {
            let mut error = Some(error);
            let mut attempts = 0;
            let result = wait_for_admission(
                || locker.teletext_work_released(),
                Instant::now() + Duration::from_secs(1),
                Duration::from_secs(1),
                || {
                    attempts += 1;
                    ready(Err(error
                        .take()
                        .expect("configuration failures must not retry")))
                },
            )
            .await;
            assert!(matches!(
                result,
                Err(TeletextWorkError::InvalidConfiguration
                    | TeletextWorkError::ConfigurationUnavailable)
            ));
            assert_eq!(attempts, 1);
            assert_eq!(locker.active_teletext_work(), 1);
        }
        drop(owner);
        assert_eq!(locker.active_teletext_work(), 0);
    }
}
