//! Application command requests.

use ersatzrs_core::locking::PlayoutBuildAdmission;
use ersatzrs_core::repositories::{LibraryRepository, RepositoryError};
use ersatzrs_core::scheduling::{PlayoutBuildMode, PlayoutBuildRequest};
use ersatzrs_domain::{EntityId, LibraryMediaKind, MediaSourceKind};

use crate::operations::OperationCoordinator;
use crate::operations::OperationKey;
use crate::ports::{PlayoutBuildQueue, ScanWorkQueue};

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

/// Process-local identity for one accepted scan operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ScanOperationId(pub u64);

/// Validated scanner work consumed by the sole native FIFO worker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScanRequest {
    /// Scan a complete library.
    Library {
        /// Provider resolved from persisted library state.
        provider: MediaSourceKind,
        /// Persisted library identity.
        library_id: EntityId,
        /// Whether unchanged provider items must be refreshed.
        deep_scan: bool,
    },
    /// Scan one show through its provider-specific targeted path.
    Show {
        /// Provider resolved from persisted library state.
        provider: MediaSourceKind,
        /// Persisted library identity.
        library_id: EntityId,
        /// Persisted show identity.
        show_id: EntityId,
        /// Whether unchanged provider items must be refreshed.
        deep_scan: bool,
    },
}

impl ScanRequest {
    /// Library exclusion identity shared by full and targeted scans.
    #[must_use]
    pub const fn library_id(self) -> EntityId {
        match self {
            Self::Library { library_id, .. } | Self::Show { library_id, .. } => library_id,
        }
    }
}

/// Queue token delivered after application admission.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScanWorkRequest {
    /// Application operation identity.
    pub operation_id: ScanOperationId,
    /// Validated work to execute.
    pub request: ScanRequest,
}

/// Request a full library scan.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RequestLibraryScan {
    /// Persisted library identity.
    pub library_id: EntityId,
    /// Whether unchanged provider items must be refreshed.
    pub deep_scan: bool,
}

/// Request a shallow periodic scan unless that library already has active work.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RequestScheduledLibraryScan {
    /// Persisted library identity.
    pub library_id: EntityId,
}

impl RequestScheduledLibraryScan {
    /// Create a periodic library-scan command.
    #[must_use]
    pub const fn new(library_id: EntityId) -> Self {
        Self { library_id }
    }
}

impl RequestLibraryScan {
    /// Create a full-library scan command.
    #[must_use]
    pub const fn new(library_id: EntityId, deep_scan: bool) -> Self {
        Self {
            library_id,
            deep_scan,
        }
    }
}

/// Request one targeted show scan.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RequestShowScan {
    /// Persisted library identity.
    pub library_id: EntityId,
    /// Persisted show identity.
    pub show_id: EntityId,
    /// Whether unchanged provider items must be refreshed.
    pub deep_scan: bool,
}

impl RequestShowScan {
    /// Create a targeted-show scan command.
    #[must_use]
    pub const fn new(library_id: EntityId, show_id: EntityId, deep_scan: bool) -> Self {
        Self {
            library_id,
            show_id,
            deep_scan,
        }
    }
}

/// Validation reason for a rejected scan command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RequestScanRejection {
    /// Persisted library identifiers must be positive.
    InvalidLibraryId,
    /// Persisted show identifiers must be positive.
    InvalidShowId,
    /// The requested library does not exist.
    LibraryNotFound,
    /// The requested show does not belong to the library.
    ShowNotFound,
    /// The persisted library subtype or media kind is invalid for the request.
    InvalidLibraryState,
    /// Provider item synchronization is disabled for the library.
    SynchronizationDisabled,
}

/// Typed application outcome for library/show scan admission.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RequestScanOutcome {
    /// A distinct request was accepted by the FIFO scanner queue.
    Accepted {
        /// Stable identity for subsequent status queries.
        operation_id: ScanOperationId,
    },
    /// The scanner worker is unavailable.
    Unavailable {
        /// Adapter diagnostic suitable for correlated logs or transport errors.
        message: String,
    },
    /// The request failed application validation.
    Rejected(RequestScanRejection),
}

/// Typed application outcome for periodic scan admission.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RequestScheduledScanOutcome {
    /// A distinct request was accepted by the FIFO scanner queue.
    Accepted {
        /// Stable identity for subsequent status queries.
        operation_id: ScanOperationId,
    },
    /// The library already has queued or running work, so this cadence was
    /// deliberately coalesced.
    AlreadyActive,
    /// The scanner worker is unavailable.
    Unavailable {
        /// Adapter diagnostic suitable for correlated logs.
        message: String,
    },
    /// Persisted state rejected the periodic request.
    Rejected(RequestScanRejection),
}

/// Terminal worker result after executing one accepted scan token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExecuteScanOutcome {
    /// The scanner and any eligible post-scan work completed.
    Completed,
    /// The scanner or post-scan work failed and the registry retained detail.
    Failed {
        /// Storage-neutral adapter diagnostic.
        message: String,
    },
    /// The queue token did not correspond to queued application work.
    Missing,
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

pub(crate) async fn request_library_scan(
    command: RequestLibraryScan,
    libraries: &dyn LibraryRepository,
    queue: &dyn ScanWorkQueue,
    operations: &OperationCoordinator,
) -> RequestScanOutcome {
    if command.library_id <= 0 {
        return RequestScanOutcome::Rejected(RequestScanRejection::InvalidLibraryId);
    }

    let target = match libraries.get_scan_target(command.library_id).await {
        Ok(target) => target,
        Err(error) => return scan_repository_failure(error, RequestScanRejection::LibraryNotFound),
    };
    if !target.should_sync_items {
        return RequestScanOutcome::Rejected(RequestScanRejection::SynchronizationDisabled);
    }

    admit_scan(
        ScanRequest::Library {
            provider: target.provider,
            library_id: target.library_id,
            deep_scan: command.deep_scan,
        },
        queue,
        operations,
    )
}

pub(crate) async fn request_scheduled_library_scan(
    command: RequestScheduledLibraryScan,
    libraries: &dyn LibraryRepository,
    queue: &dyn ScanWorkQueue,
    operations: &OperationCoordinator,
) -> RequestScheduledScanOutcome {
    if command.library_id <= 0 {
        return RequestScheduledScanOutcome::Rejected(RequestScanRejection::InvalidLibraryId);
    }

    let target = match libraries.get_scan_target(command.library_id).await {
        Ok(target) => target,
        Err(error) => {
            return scheduled_scan_repository_failure(error, RequestScanRejection::LibraryNotFound);
        }
    };
    if !target.should_sync_items {
        return RequestScheduledScanOutcome::Rejected(
            RequestScanRejection::SynchronizationDisabled,
        );
    }

    match operations.scan_activity().admit_if_idle(
        ScanRequest::Library {
            provider: target.provider,
            library_id: target.library_id,
            deep_scan: false,
        },
        |work| queue.enqueue(work),
    ) {
        Ok(Some(operation_id)) => RequestScheduledScanOutcome::Accepted { operation_id },
        Ok(None) => RequestScheduledScanOutcome::AlreadyActive,
        Err(crate::operations::ScanAdmissionError::WorkerUnavailable) => {
            RequestScheduledScanOutcome::Unavailable {
                message: "scanner worker is unavailable".to_owned(),
            }
        }
        Err(crate::operations::ScanAdmissionError::Queue(error)) => {
            RequestScheduledScanOutcome::Unavailable {
                message: error.to_string(),
            }
        }
    }
}

pub(crate) async fn request_show_scan(
    command: RequestShowScan,
    libraries: &dyn LibraryRepository,
    queue: &dyn ScanWorkQueue,
    operations: &OperationCoordinator,
) -> RequestScanOutcome {
    if command.library_id <= 0 {
        return RequestScanOutcome::Rejected(RequestScanRejection::InvalidLibraryId);
    }
    if command.show_id <= 0 {
        return RequestScanOutcome::Rejected(RequestScanRejection::InvalidShowId);
    }

    let target = match libraries.get_scan_target(command.library_id).await {
        Ok(target) => target,
        Err(error) => return scan_repository_failure(error, RequestScanRejection::LibraryNotFound),
    };
    if !target.should_sync_items {
        return RequestScanOutcome::Rejected(RequestScanRejection::SynchronizationDisabled);
    }
    if target.media_kind != LibraryMediaKind::Shows {
        return RequestScanOutcome::Rejected(RequestScanRejection::InvalidLibraryState);
    }
    match libraries
        .show_belongs_to_library(command.library_id, command.show_id)
        .await
    {
        Ok(true) => {}
        Ok(false) => return RequestScanOutcome::Rejected(RequestScanRejection::ShowNotFound),
        Err(error) => return scan_repository_failure(error, RequestScanRejection::ShowNotFound),
    }

    let request = if target.provider == MediaSourceKind::Local {
        ScanRequest::Library {
            provider: target.provider,
            library_id: target.library_id,
            deep_scan: command.deep_scan,
        }
    } else {
        ScanRequest::Show {
            provider: target.provider,
            library_id: target.library_id,
            show_id: command.show_id,
            deep_scan: command.deep_scan,
        }
    };
    admit_scan(request, queue, operations)
}

fn admit_scan(
    request: ScanRequest,
    queue: &dyn ScanWorkQueue,
    operations: &OperationCoordinator,
) -> RequestScanOutcome {
    match operations
        .scan_activity()
        .admit(request, |work| queue.enqueue(work))
    {
        Ok(operation_id) => RequestScanOutcome::Accepted { operation_id },
        Err(crate::operations::ScanAdmissionError::WorkerUnavailable) => {
            RequestScanOutcome::Unavailable {
                message: "scanner worker is unavailable".to_owned(),
            }
        }
        Err(crate::operations::ScanAdmissionError::Queue(error)) => {
            RequestScanOutcome::Unavailable {
                message: error.to_string(),
            }
        }
    }
}

fn scan_repository_failure(
    error: RepositoryError,
    not_found: RequestScanRejection,
) -> RequestScanOutcome {
    match error {
        RepositoryError::NotFound { .. } => RequestScanOutcome::Rejected(not_found),
        RepositoryError::Decode { .. } | RepositoryError::Conflict(_) => {
            RequestScanOutcome::Rejected(RequestScanRejection::InvalidLibraryState)
        }
        RepositoryError::Unavailable { message, .. } => RequestScanOutcome::Unavailable { message },
    }
}

fn scheduled_scan_repository_failure(
    error: RepositoryError,
    not_found: RequestScanRejection,
) -> RequestScheduledScanOutcome {
    match error {
        RepositoryError::NotFound { .. } => RequestScheduledScanOutcome::Rejected(not_found),
        RepositoryError::Decode { .. } | RepositoryError::Conflict(_) => {
            RequestScheduledScanOutcome::Rejected(RequestScanRejection::InvalidLibraryState)
        }
        RepositoryError::Unavailable { message, .. } => {
            RequestScheduledScanOutcome::Unavailable { message }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Mutex;

    use async_trait::async_trait;
    use ersatzrs_core::locking::{EntityLocker, PlayoutBuildActivity};
    use ersatzrs_core::repositories::{LibraryRepository, LibraryScanTarget, RepositoryError};
    use ersatzrs_core::scheduling::{PlayoutBuildMode, PlayoutBuildRequest};
    use ersatzrs_domain::{EntityId, Library, LibraryMediaKind, MediaSourceKind};

    use super::{
        request_library_scan, request_playout_build, request_scheduled_library_scan,
        request_show_scan, RequestLibraryScan, RequestPlayoutBuild, RequestPlayoutBuildOutcome,
        RequestPlayoutBuildRejection, RequestScanOutcome, RequestScanRejection,
        RequestScheduledLibraryScan, RequestScheduledScanOutcome, RequestShowScan, ScanRequest,
        ScanWorkRequest,
    };
    use crate::operations::OperationCoordinator;
    use crate::ports::{
        PlayoutBuildQueue, PlayoutBuildQueueError, ScanWorkQueue, ScanWorkQueueError,
    };

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

    #[derive(Default)]
    struct RecordingScanQueue {
        requests: Mutex<Vec<ScanWorkRequest>>,
        unavailable: bool,
    }

    impl RecordingScanQueue {
        fn unavailable() -> Self {
            Self {
                requests: Mutex::default(),
                unavailable: true,
            }
        }
    }

    impl ScanWorkQueue for RecordingScanQueue {
        fn enqueue(&self, request: ScanWorkRequest) -> Result<(), ScanWorkQueueError> {
            if self.unavailable {
                Err(ScanWorkQueueError::unavailable("scanner receiver closed"))
            } else {
                self.requests.lock().expect("scan requests").push(request);
                Ok(())
            }
        }
    }

    struct FakeLibraryRepository {
        targets: BTreeMap<EntityId, LibraryScanTarget>,
        show_exists: bool,
    }

    impl FakeLibraryRepository {
        fn with_targets(targets: impl IntoIterator<Item = LibraryScanTarget>) -> Self {
            Self {
                targets: targets
                    .into_iter()
                    .map(|target| (target.library_id, target))
                    .collect(),
                show_exists: true,
            }
        }
    }

    #[async_trait]
    impl LibraryRepository for FakeLibraryRepository {
        async fn get_with_paths(
            &self,
            id: EntityId,
        ) -> ersatzrs_core::repositories::Result<Library> {
            Err(RepositoryError::NotFound {
                kind: "library",
                id,
            })
        }

        async fn list_for_source(
            &self,
            _source_id: EntityId,
        ) -> ersatzrs_core::repositories::Result<Vec<Library>> {
            Ok(Vec::new())
        }

        async fn upsert(&self, _library: &Library) -> ersatzrs_core::repositories::Result<()> {
            Ok(())
        }

        async fn get_scan_target(
            &self,
            id: EntityId,
        ) -> ersatzrs_core::repositories::Result<LibraryScanTarget> {
            self.targets
                .get(&id)
                .copied()
                .ok_or(RepositoryError::NotFound {
                    kind: "library",
                    id,
                })
        }

        async fn show_belongs_to_library(
            &self,
            _library_id: EntityId,
            _show_id: EntityId,
        ) -> ersatzrs_core::repositories::Result<bool> {
            Ok(self.show_exists)
        }
    }

    fn scan_target(library_id: EntityId, provider: MediaSourceKind) -> LibraryScanTarget {
        LibraryScanTarget {
            library_id,
            provider,
            media_kind: LibraryMediaKind::Shows,
            should_sync_items: true,
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

    #[tokio::test]
    async fn scan_admission_preserves_fifo_and_repeated_requests() {
        let repository = FakeLibraryRepository::with_targets([
            scan_target(2, MediaSourceKind::Plex),
            scan_target(1, MediaSourceKind::Local),
        ]);
        let queue = RecordingScanQueue::default();
        let operations = coordinator();

        for library_id in [2, 1, 2] {
            assert!(matches!(
                request_library_scan(
                    RequestLibraryScan::new(library_id, false),
                    &repository,
                    &queue,
                    &operations,
                )
                .await,
                RequestScanOutcome::Accepted { .. }
            ));
        }

        let ids = queue
            .requests
            .lock()
            .expect("scan requests")
            .iter()
            .map(|work| work.request.library_id())
            .collect::<Vec<_>>();
        assert_eq!(ids, [2, 1, 2]);
        assert_eq!(operations.scan_activity().active_library_ids(), [1, 2]);
    }

    #[tokio::test]
    async fn scheduled_scan_coalesces_active_work_without_dropping_manual_repeats() {
        let repository =
            FakeLibraryRepository::with_targets([scan_target(6, MediaSourceKind::Plex)]);
        let queue = RecordingScanQueue::default();
        let operations = coordinator();

        assert!(matches!(
            request_library_scan(
                RequestLibraryScan::new(6, false),
                &repository,
                &queue,
                &operations,
            )
            .await,
            RequestScanOutcome::Accepted { .. }
        ));
        assert_eq!(
            request_scheduled_library_scan(
                RequestScheduledLibraryScan::new(6),
                &repository,
                &queue,
                &operations,
            )
            .await,
            RequestScheduledScanOutcome::AlreadyActive
        );
        assert_eq!(
            queue.requests.lock().expect("scan requests").len(),
            1,
            "periodic cadence must not append duplicate active work"
        );

        assert!(matches!(
            request_library_scan(
                RequestLibraryScan::new(6, true),
                &repository,
                &queue,
                &operations,
            )
            .await,
            RequestScanOutcome::Accepted { .. }
        ));
        assert_eq!(
            queue.requests.lock().expect("scan requests").len(),
            2,
            "manual repeated requests retain FIFO admission"
        );
    }

    #[tokio::test]
    async fn show_admission_uses_persisted_provider_and_local_fallback() {
        let repository = FakeLibraryRepository::with_targets([
            scan_target(3, MediaSourceKind::Local),
            scan_target(4, MediaSourceKind::Jellyfin),
        ]);
        let queue = RecordingScanQueue::default();
        let operations = coordinator();

        assert!(matches!(
            request_show_scan(
                RequestShowScan::new(3, 30, true),
                &repository,
                &queue,
                &operations,
            )
            .await,
            RequestScanOutcome::Accepted { .. }
        ));
        assert!(matches!(
            request_show_scan(
                RequestShowScan::new(4, 40, false),
                &repository,
                &queue,
                &operations,
            )
            .await,
            RequestScanOutcome::Accepted { .. }
        ));

        let requests = queue.requests.lock().expect("scan requests");
        assert_eq!(
            requests[0].request,
            ScanRequest::Library {
                provider: MediaSourceKind::Local,
                library_id: 3,
                deep_scan: true,
            }
        );
        assert_eq!(
            requests[1].request,
            ScanRequest::Show {
                provider: MediaSourceKind::Jellyfin,
                library_id: 4,
                show_id: 40,
                deep_scan: false,
            }
        );
    }

    #[tokio::test]
    async fn scan_admission_reports_validation_and_worker_unavailability() {
        let mut disabled = scan_target(5, MediaSourceKind::Emby);
        disabled.should_sync_items = false;
        let repository = FakeLibraryRepository::with_targets([disabled]);
        let queue = RecordingScanQueue::unavailable();
        let operations = coordinator();

        assert_eq!(
            request_library_scan(
                RequestLibraryScan::new(0, false),
                &repository,
                &queue,
                &operations,
            )
            .await,
            RequestScanOutcome::Rejected(RequestScanRejection::InvalidLibraryId)
        );
        assert_eq!(
            request_library_scan(
                RequestLibraryScan::new(5, false),
                &repository,
                &queue,
                &operations,
            )
            .await,
            RequestScanOutcome::Rejected(RequestScanRejection::SynchronizationDisabled)
        );

        let available_repository =
            FakeLibraryRepository::with_targets([scan_target(6, MediaSourceKind::Plex)]);
        assert_eq!(
            request_library_scan(
                RequestLibraryScan::new(6, false),
                &available_repository,
                &queue,
                &operations,
            )
            .await,
            RequestScanOutcome::Unavailable {
                message: "scanner receiver closed".to_owned(),
            }
        );
        assert!(operations.scan_activity().snapshots().is_empty());
    }
}
