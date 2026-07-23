//! Application command requests.

use ersatzrs_core::locking::PlayoutBuildAdmission;
use ersatzrs_core::scheduling::{PlayoutBuildMode, PlayoutBuildRequest};
use ersatzrs_domain::EntityId;

use crate::operations::OperationCoordinator;
use crate::operations::OperationKey;
use crate::ports::PlayoutBuildQueue;

/// Request to begin one long-running operation under the shared coordinator.
///
/// Playout-build admission has its dedicated command below; scan admission
/// lands in milestone p7b-03. This command remains the generic exclusion
/// boundary for other application operations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BeginOperation {
    /// Stable operation identity to acquire.
    pub key: OperationKey,
}

impl BeginOperation {
    /// Create an operation-admission command.
    #[must_use]
    pub const fn new(key: OperationKey) -> Self {
        Self { key }
    }
}

/// Request admission for one scheduler-owned playout build.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RequestPlayoutBuild {
    /// Persisted playout identity.
    pub playout_id: EntityId,
    /// Existing Refresh, Reset, or Continue build semantics.
    pub mode: PlayoutBuildMode,
}

impl RequestPlayoutBuild {
    /// Create a playout-build application command.
    #[must_use]
    pub const fn new(playout_id: EntityId, mode: PlayoutBuildMode) -> Self {
        Self { playout_id, mode }
    }
}

/// Validation reason for a rejected playout-build command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RequestPlayoutBuildRejection {
    /// Persisted entity identifiers must be positive.
    InvalidPlayoutId,
}

/// Typed application outcome for playout-build admission.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RequestPlayoutBuildOutcome {
    /// Work was accepted by the FIFO scheduler queue.
    Accepted,
    /// Equivalent work is already waiting for the scheduler.
    AlreadyQueued,
    /// The scheduler is currently building this playout.
    AlreadyBuilding,
    /// The scheduler work queue is unavailable.
    Unavailable {
        /// Adapter diagnostic suitable for correlated logs or transport errors.
        message: String,
    },
    /// The command failed application validation.
    Rejected(RequestPlayoutBuildRejection),
}

pub(crate) fn request_playout_build(
    command: RequestPlayoutBuild,
    queue: &dyn PlayoutBuildQueue,
    operations: &OperationCoordinator,
) -> RequestPlayoutBuildOutcome {
    if command.playout_id <= 0 {
        return RequestPlayoutBuildOutcome::Rejected(
            RequestPlayoutBuildRejection::InvalidPlayoutId,
        );
    }

    let request = PlayoutBuildRequest {
        playout_id: command.playout_id,
        mode: command.mode,
    };
    match operations
        .playout_activity()
        .admit(request, |request| queue.enqueue(request))
    {
        Ok(PlayoutBuildAdmission::Accepted) => RequestPlayoutBuildOutcome::Accepted,
        Ok(PlayoutBuildAdmission::FollowUpAccepted | PlayoutBuildAdmission::AlreadyBuilding) => {
            RequestPlayoutBuildOutcome::AlreadyBuilding
        }
        Ok(PlayoutBuildAdmission::AlreadyQueued) => RequestPlayoutBuildOutcome::AlreadyQueued,
        Err(error) => RequestPlayoutBuildOutcome::Unavailable {
            message: error.to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use ersatzrs_core::locking::{EntityLocker, PlayoutBuildActivity};
    use ersatzrs_core::scheduling::{PlayoutBuildMode, PlayoutBuildRequest};

    use super::{
        request_playout_build, RequestPlayoutBuild, RequestPlayoutBuildOutcome,
        RequestPlayoutBuildRejection,
    };
    use crate::operations::OperationCoordinator;
    use crate::ports::{PlayoutBuildQueue, PlayoutBuildQueueError};

    #[derive(Default)]
    struct RecordingQueue {
        requests: Mutex<Vec<PlayoutBuildRequest>>,
        unavailable: bool,
    }

    impl RecordingQueue {
        fn unavailable() -> Self {
            Self {
                requests: Mutex::default(),
                unavailable: true,
            }
        }
    }

    impl PlayoutBuildQueue for RecordingQueue {
        fn enqueue(&self, request: PlayoutBuildRequest) -> Result<(), PlayoutBuildQueueError> {
            if self.unavailable {
                Err(PlayoutBuildQueueError::unavailable("worker closed"))
            } else {
                self.requests.lock().expect("request lock").push(request);
                Ok(())
            }
        }
    }

    fn coordinator() -> OperationCoordinator {
        OperationCoordinator::new(EntityLocker::default(), PlayoutBuildActivity::default())
    }

    #[test]
    fn accepts_refresh_and_reset_requests_without_changing_mode() {
        let queue = RecordingQueue::default();
        let operations = coordinator();

        assert_eq!(
            request_playout_build(
                RequestPlayoutBuild::new(7, PlayoutBuildMode::Refresh),
                &queue,
                &operations,
            ),
            RequestPlayoutBuildOutcome::Accepted
        );
        operations.playout_activity().finish(7);
        assert_eq!(
            request_playout_build(
                RequestPlayoutBuild::new(7, PlayoutBuildMode::Reset),
                &queue,
                &operations,
            ),
            RequestPlayoutBuildOutcome::Accepted
        );

        assert_eq!(
            *queue.requests.lock().expect("request lock"),
            [
                PlayoutBuildRequest {
                    playout_id: 7,
                    mode: PlayoutBuildMode::Refresh,
                },
                PlayoutBuildRequest {
                    playout_id: 7,
                    mode: PlayoutBuildMode::Reset,
                },
            ]
        );
    }

    #[test]
    fn upgrades_queued_mode_and_retains_one_active_follow_up() {
        let queue = RecordingQueue::default();
        let operations = coordinator();
        let continue_command = RequestPlayoutBuild::new(8, PlayoutBuildMode::Continue);

        assert_eq!(
            request_playout_build(continue_command, &queue, &operations),
            RequestPlayoutBuildOutcome::Accepted
        );
        assert_eq!(
            request_playout_build(
                RequestPlayoutBuild::new(8, PlayoutBuildMode::Reset),
                &queue,
                &operations,
            ),
            RequestPlayoutBuildOutcome::AlreadyQueued
        );
        assert_eq!(queue.requests.lock().expect("request lock").len(), 1);

        let first_token = queue.requests.lock().expect("request lock")[0];
        let (first_request, first_guard) = operations.playout_activity().start_guard(first_token);
        assert_eq!(first_request.mode, PlayoutBuildMode::Reset);
        assert_eq!(
            request_playout_build(
                RequestPlayoutBuild::new(8, PlayoutBuildMode::Refresh),
                &queue,
                &operations,
            ),
            RequestPlayoutBuildOutcome::AlreadyBuilding
        );
        assert_eq!(
            request_playout_build(
                RequestPlayoutBuild::new(8, PlayoutBuildMode::Reset),
                &queue,
                &operations,
            ),
            RequestPlayoutBuildOutcome::AlreadyBuilding
        );
        assert_eq!(
            queue.requests.lock().expect("request lock").len(),
            2,
            "active mutations must share one follow-up token"
        );

        drop(first_guard);
        let follow_up_token = queue.requests.lock().expect("request lock")[1];
        let (follow_up, follow_up_guard) =
            operations.playout_activity().start_guard(follow_up_token);
        assert_eq!(follow_up.mode, PlayoutBuildMode::Reset);
        drop(follow_up_guard);
        assert!(operations.playout_activity().queued_playouts().is_empty());
        assert!(operations.playout_activity().building_playouts().is_empty());
    }

    #[test]
    fn preserves_fifo_admission_order_across_playouts() {
        let queue = RecordingQueue::default();
        let operations = coordinator();

        for playout_id in [30, 10, 20] {
            assert_eq!(
                request_playout_build(
                    RequestPlayoutBuild::new(playout_id, PlayoutBuildMode::Continue),
                    &queue,
                    &operations,
                ),
                RequestPlayoutBuildOutcome::Accepted
            );
        }

        let ids = queue
            .requests
            .lock()
            .expect("request lock")
            .iter()
            .map(|request| request.playout_id)
            .collect::<Vec<_>>();
        assert_eq!(ids, [30, 10, 20]);
    }

    #[test]
    fn unavailable_worker_releases_admission_for_retry() {
        let queue = RecordingQueue::unavailable();
        let operations = coordinator();
        let command = RequestPlayoutBuild::new(9, PlayoutBuildMode::Reset);

        assert_eq!(
            request_playout_build(command, &queue, &operations),
            RequestPlayoutBuildOutcome::Unavailable {
                message: "worker closed".to_owned(),
            }
        );
        assert!(operations.playout_activity().queued_playouts().is_empty());
        assert!(matches!(
            request_playout_build(command, &queue, &operations),
            RequestPlayoutBuildOutcome::Unavailable { .. }
        ));
    }

    #[test]
    fn rejects_invalid_identity_without_touching_queue_or_activity() {
        let queue = RecordingQueue::default();
        let operations = coordinator();

        assert_eq!(
            request_playout_build(
                RequestPlayoutBuild::new(0, PlayoutBuildMode::Reset),
                &queue,
                &operations,
            ),
            RequestPlayoutBuildOutcome::Rejected(RequestPlayoutBuildRejection::InvalidPlayoutId)
        );
        assert!(queue.requests.lock().expect("request lock").is_empty());
        assert!(operations.playout_activity().queued_playouts().is_empty());
    }
}
