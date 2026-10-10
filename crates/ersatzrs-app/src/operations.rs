//! Shared process-local coordination for long-running application operations.

use std::collections::{BTreeMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use ersatzrs_core::locking::{EntityLocker, PlayoutBuildActivity, PlayoutLockGuard};
use ersatzrs_domain::EntityId;

use crate::commands::{BeginOperation, ScanOperationId, ScanRequest, ScanWorkRequest};
use crate::queries::{
    GetLibraryScanStatus, GetOperationStatus, LibraryScanStatus, OperationStatus,
    ScanOperationSnapshot, ScanOperationState,
};

/// Stable identity for an operation coordinated across transport and worker
/// adapters.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum OperationKey {
    /// Build one playout.
    PlayoutBuild(EntityId),
    /// Synchronize remote collections for one media source.
    ExternalCollections {
        provider: String,
        media_source_id: EntityId,
    },
}

/// Outcome of trying to begin an operation.
pub enum BeginOperationOutcome {
    /// The operation lock was acquired and remains held by the guard.
    Started(OperationGuard),
    /// Another caller already owns the same operation lock.
    AlreadyRunning,
}

impl BeginOperationOutcome {
    /// Consume the outcome and return its guard when admission succeeded.
    #[must_use]
    pub fn into_guard(self) -> Option<OperationGuard> {
        match self {
            Self::Started(guard) => Some(guard),
            Self::AlreadyRunning => None,
        }
    }
}

/// Guard that releases its operation lock when dropped.
#[must_use = "dropping the guard releases the operation; bind it to a name"]
pub struct OperationGuard {
    kind: OperationGuardKind,
}

enum OperationGuardKind {
    Playout {
        _guard: PlayoutLockGuard,
    },
    External {
        coordinator: OperationCoordinator,
        key: OperationKey,
    },
}

impl Drop for OperationGuard {
    fn drop(&mut self) {
        if let OperationGuardKind::External { coordinator, key } = &self.kind {
            recover(coordinator.inner.external.lock()).remove(key);
        }
    }
}

#[derive(Default)]
struct OperationCoordinatorInner {
    external: Mutex<HashSet<OperationKey>>,
}

const MAX_TERMINAL_SCAN_OPERATIONS: usize = 256;

struct ScanOperationRecord {
    request: ScanRequest,
    state: ScanOperationState,
    progress: Option<f32>,
}

struct ScanOperationRegistryInner {
    next_id: u64,
    worker_available: bool,
    operations: BTreeMap<ScanOperationId, ScanOperationRecord>,
    terminal_order: VecDeque<ScanOperationId>,
}

impl Default for ScanOperationRegistryInner {
    fn default() -> Self {
        Self {
            next_id: 0,
            worker_available: true,
            operations: BTreeMap::new(),
            terminal_order: VecDeque::new(),
        }
    }
}

/// Application-owned library/show scan admission and lifecycle registry.
#[derive(Clone, Default)]
pub struct ScanOperationRegistry {
    inner: Arc<Mutex<ScanOperationRegistryInner>>,
}

/// Scan admission failure before a queue token was accepted.
#[derive(Debug)]
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) enum ScanAdmissionError<E> {
    /// The native worker has stopped.
    WorkerUnavailable,
    /// The queue adapter rejected the token.
    Queue(E),
}

/// Guard that records cancellation if worker execution exits unexpectedly.
#[must_use = "bind the guard for the complete scan execution lifecycle"]
pub struct ScanExecutionGuard {
    registry: ScanOperationRegistry,
    operation_id: ScanOperationId,
    finished: bool,
}

impl ScanExecutionGuard {
    /// Mark worker execution as successfully completed.
    pub fn complete(mut self) {
        self.registry.complete(self.operation_id);
        self.finished = true;
    }

    /// Mark worker execution as failed with an observable diagnostic.
    pub fn fail(mut self, message: impl Into<String>) {
        self.registry.fail(self.operation_id, message.into());
        self.finished = true;
    }
}

impl Drop for ScanExecutionGuard {
    fn drop(&mut self) {
        if !self.finished {
            self.registry
                .fail(self.operation_id, "scan cancelled".to_owned());
        }
    }
}

/// Single application-owned coordination surface shared by web and workers.
#[derive(Clone)]
pub struct OperationCoordinator {
    locker: EntityLocker,
    playout_activity: PlayoutBuildActivity,
    scan_activity: ScanOperationRegistry,
    inner: Arc<OperationCoordinatorInner>,
}

impl OperationCoordinator {
    /// Construct a coordinator around the process-wide typed locker and
    /// playout request activity tracker.
    #[must_use]
    pub fn new(locker: EntityLocker, playout_activity: PlayoutBuildActivity) -> Self {
        Self {
            locker,
            playout_activity,
            scan_activity: ScanOperationRegistry::default(),
            inner: Arc::new(OperationCoordinatorInner::default()),
        }
    }

    /// Attempt to acquire one operation command.
    #[must_use]
    pub fn begin(&self, command: BeginOperation) -> BeginOperationOutcome {
        let guard = match command.key {
            OperationKey::PlayoutBuild(playout_id) => {
                PlayoutLockGuard::try_acquire(&self.locker, playout_id).map(|guard| {
                    OperationGuard {
                        kind: OperationGuardKind::Playout { _guard: guard },
                    }
                })
            }
            key @ OperationKey::ExternalCollections { .. } => {
                if recover(self.inner.external.lock()).insert(key.clone()) {
                    Some(OperationGuard {
                        kind: OperationGuardKind::External {
                            coordinator: self.clone(),
                            key,
                        },
                    })
                } else {
                    None
                }
            }
        };

        guard.map_or(
            BeginOperationOutcome::AlreadyRunning,
            BeginOperationOutcome::Started,
        )
    }

    /// Read one operation's current state.
    #[must_use]
    pub fn status(&self, query: GetOperationStatus) -> OperationStatus {
        let running = match query.key {
            OperationKey::PlayoutBuild(id) => self.locker.is_playout_locked(id),
            key @ OperationKey::ExternalCollections { .. } => {
                recover(self.inner.external.lock()).contains(&key)
            }
        };
        if running {
            OperationStatus::Running
        } else {
            OperationStatus::Idle
        }
    }

    /// Snapshot active library scan ids for polling adapters.
    #[must_use]
    pub fn active_library_scans(&self) -> Vec<EntityId> {
        self.scan_activity.active_library_ids()
    }

    /// Snapshot active playout build ids for polling adapters.
    #[must_use]
    pub fn active_playout_builds(&self) -> Vec<EntityId> {
        self.locker.locked_playouts()
    }

    /// Snapshot active external collection synchronizations.
    #[must_use]
    pub fn active_external_collections(&self) -> Vec<(String, EntityId)> {
        recover(self.inner.external.lock())
            .iter()
            .filter_map(|key| match key {
                OperationKey::ExternalCollections {
                    provider,
                    media_source_id,
                } => Some((provider.clone(), *media_source_id)),
                OperationKey::PlayoutBuild(_) => None,
            })
            .collect()
    }

    /// Typed locker shared with the existing scheduler worker until p7b-02
    /// moves build admission behind an application command.
    #[must_use]
    pub fn entity_locker(&self) -> EntityLocker {
        self.locker.clone()
    }

    /// Accepted/building activity shared by application admission, scheduler
    /// consumption, and polling transport adapters.
    #[must_use]
    pub fn playout_activity(&self) -> PlayoutBuildActivity {
        self.playout_activity.clone()
    }

    /// Shared scan admission, progress, and terminal-state registry.
    #[must_use]
    pub fn scan_activity(&self) -> ScanOperationRegistry {
        self.scan_activity.clone()
    }
}

impl std::fmt::Debug for OperationCoordinator {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OperationCoordinator")
            .field("active_library_scans", &self.active_library_scans())
            .field("active_playout_builds", &self.active_playout_builds())
            .field(
                "active_external_collections",
                &self.active_external_collections(),
            )
            .finish()
    }
}

impl ScanOperationRegistry {
    /// Mirror one persisted operation into the process-local progress view.
    pub(crate) fn observe_persisted(&self, operation_id: ScanOperationId, request: ScanRequest) {
        let mut inner = recover(self.inner.lock());
        inner.next_id = inner.next_id.max(operation_id.0);
        match inner.operations.entry(operation_id) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(ScanOperationRecord {
                    request,
                    state: ScanOperationState::Queued,
                    progress: None,
                });
            }
            std::collections::btree_map::Entry::Occupied(mut entry)
                if matches!(
                    entry.get().state,
                    ScanOperationState::Completed | ScanOperationState::Failed { .. }
                ) =>
            {
                entry.insert(ScanOperationRecord {
                    request,
                    state: ScanOperationState::Queued,
                    progress: None,
                });
                inner
                    .terminal_order
                    .retain(|candidate| *candidate != operation_id);
            }
            std::collections::btree_map::Entry::Occupied(_) => {}
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn admit<E>(
        &self,
        request: ScanRequest,
        enqueue: impl FnOnce(ScanWorkRequest) -> std::result::Result<(), E>,
    ) -> std::result::Result<ScanOperationId, ScanAdmissionError<E>> {
        self.admit_inner(request, false, enqueue)
            .map(|operation_id| operation_id.expect("manual scan admission cannot coalesce"))
    }

    #[cfg_attr(not(test), allow(dead_code))]
    fn admit_inner<E>(
        &self,
        request: ScanRequest,
        coalesce_if_active: bool,
        enqueue: impl FnOnce(ScanWorkRequest) -> std::result::Result<(), E>,
    ) -> std::result::Result<Option<ScanOperationId>, ScanAdmissionError<E>> {
        let mut inner = recover(self.inner.lock());
        if !inner.worker_available {
            return Err(ScanAdmissionError::WorkerUnavailable);
        }
        if coalesce_if_active
            && inner.operations.values().any(|record| {
                record.request.library_id() == request.library_id()
                    && matches!(
                        record.state,
                        ScanOperationState::Queued | ScanOperationState::Running
                    )
            })
        {
            return Ok(None);
        }

        inner.next_id = inner.next_id.saturating_add(1);
        let operation_id = ScanOperationId(inner.next_id);
        let work = ScanWorkRequest {
            operation_id,
            request,
        };
        enqueue(work).map_err(ScanAdmissionError::Queue)?;
        inner.operations.insert(
            operation_id,
            ScanOperationRecord {
                request,
                state: ScanOperationState::Queued,
                progress: None,
            },
        );
        Ok(Some(operation_id))
    }

    /// Mark the native FIFO worker available for admission.
    pub fn worker_started(&self) {
        recover(self.inner.lock()).worker_available = true;
    }

    /// Read the existing worker lifecycle state before durable scan admission.
    #[must_use]
    pub fn is_worker_available(&self) -> bool {
        recover(self.inner.lock()).worker_available
    }

    /// Fail every non-terminal request and stop accepting new work.
    pub fn worker_stopped(&self, message: impl Into<String>) {
        let message = message.into();
        let mut inner = recover(self.inner.lock());
        inner.worker_available = false;
        let active_ids = inner
            .operations
            .iter()
            .filter_map(|(id, record)| {
                matches!(
                    record.state,
                    ScanOperationState::Queued | ScanOperationState::Running
                )
                .then_some(*id)
            })
            .collect::<Vec<_>>();
        for operation_id in active_ids {
            mark_terminal(
                &mut inner,
                operation_id,
                ScanOperationState::Failed {
                    message: message.clone(),
                },
            );
        }
    }

    /// Start one queued request and acquire its per-library exclusion guard.
    #[must_use]
    pub fn start_guard(&self, operation_id: ScanOperationId) -> Option<ScanExecutionGuard> {
        let mut inner = recover(self.inner.lock());
        let library_id = inner.operations.get(&operation_id)?.request.library_id();
        let another_running = inner.operations.iter().any(|(candidate_id, record)| {
            *candidate_id != operation_id
                && record.request.library_id() == library_id
                && matches!(record.state, ScanOperationState::Running)
        });
        if another_running {
            return None;
        }

        let record = inner.operations.get_mut(&operation_id)?;
        if !matches!(record.state, ScanOperationState::Queued) {
            return None;
        }
        record.state = ScanOperationState::Running;
        record.progress = None;
        Some(ScanExecutionGuard {
            registry: self.clone(),
            operation_id,
            finished: false,
        })
    }

    /// Record clamped active progress for one running request.
    pub fn report_progress(&self, operation_id: ScanOperationId, fraction: f32) {
        let mut inner = recover(self.inner.lock());
        if let Some(record) = inner.operations.get_mut(&operation_id) {
            if matches!(record.state, ScanOperationState::Running) {
                record.progress = Some(fraction.clamp(0.0, 1.0));
            }
        }
    }

    /// Snapshot all retained active and terminal operations.
    #[must_use]
    pub fn snapshots(&self) -> Vec<ScanOperationSnapshot> {
        recover(self.inner.lock())
            .operations
            .iter()
            .map(|(operation_id, record)| ScanOperationSnapshot {
                operation_id: *operation_id,
                request: record.request,
                state: record.state.clone(),
                progress: record.progress,
            })
            .collect()
    }

    /// Snapshot distinct library ids with queued or running work.
    #[must_use]
    pub fn active_library_ids(&self) -> Vec<EntityId> {
        let mut ids = recover(self.inner.lock())
            .operations
            .values()
            .filter_map(|record| {
                matches!(
                    record.state,
                    ScanOperationState::Queued | ScanOperationState::Running
                )
                .then_some(record.request.library_id())
            })
            .collect::<Vec<_>>();
        ids.sort_unstable();
        ids.dedup();
        ids
    }

    /// Snapshot active progress by library.
    #[must_use]
    pub fn progress_by_library(&self) -> Vec<(EntityId, f32)> {
        let mut result = recover(self.inner.lock())
            .operations
            .values()
            .filter_map(|record| {
                matches!(record.state, ScanOperationState::Running)
                    .then(|| {
                        record
                            .progress
                            .map(|progress| (record.request.library_id(), progress))
                    })
                    .flatten()
            })
            .collect::<Vec<_>>();
        result.sort_unstable_by_key(|(library_id, _)| *library_id);
        result
    }

    /// Read the aggregate state for one library.
    #[must_use]
    pub fn library_status(&self, query: GetLibraryScanStatus) -> LibraryScanStatus {
        let inner = recover(self.inner.lock());
        let matching = inner
            .operations
            .iter()
            .rev()
            .filter(|(_, record)| record.request.library_id() == query.library_id)
            .collect::<Vec<_>>();
        if matching
            .iter()
            .any(|(_, record)| matches!(record.state, ScanOperationState::Running))
        {
            return LibraryScanStatus::Running;
        }
        if matching
            .iter()
            .any(|(_, record)| matches!(record.state, ScanOperationState::Queued))
        {
            return LibraryScanStatus::Queued;
        }
        match matching.first().map(|(_, record)| &record.state) {
            Some(ScanOperationState::Completed) => LibraryScanStatus::Completed,
            Some(ScanOperationState::Failed { message }) => LibraryScanStatus::Failed {
                message: message.clone(),
            },
            Some(ScanOperationState::Queued | ScanOperationState::Running) => {
                unreachable!("active states handled above")
            }
            None => LibraryScanStatus::Idle,
        }
    }

    fn complete(&self, operation_id: ScanOperationId) {
        mark_terminal(
            &mut recover(self.inner.lock()),
            operation_id,
            ScanOperationState::Completed,
        );
    }

    fn fail(&self, operation_id: ScanOperationId, message: String) {
        mark_terminal(
            &mut recover(self.inner.lock()),
            operation_id,
            ScanOperationState::Failed { message },
        );
    }
}

fn mark_terminal(
    inner: &mut ScanOperationRegistryInner,
    operation_id: ScanOperationId,
    state: ScanOperationState,
) {
    let Some(record) = inner.operations.get_mut(&operation_id) else {
        return;
    };
    if matches!(
        record.state,
        ScanOperationState::Completed | ScanOperationState::Failed { .. }
    ) {
        return;
    }
    record.state = state;
    record.progress = None;
    inner.terminal_order.push_back(operation_id);
    while inner.terminal_order.len() > MAX_TERMINAL_SCAN_OPERATIONS {
        if let Some(expired) = inner.terminal_order.pop_front() {
            inner.operations.remove(&expired);
        }
    }
}

fn recover<'a, T>(
    result: std::result::Result<MutexGuard<'a, T>, PoisonError<MutexGuard<'a, T>>>,
) -> MutexGuard<'a, T> {
    result.unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::{BeginOperationOutcome, OperationCoordinator, OperationKey};
    use crate::commands::{BeginOperation, ScanOperationId, ScanRequest};
    use crate::queries::{
        GetLibraryScanStatus, GetOperationStatus, LibraryScanStatus, OperationStatus,
        ScanOperationState,
    };
    use ersatzrs_core::locking::{EntityLocker, PlayoutBuildActivity};
    use ersatzrs_domain::MediaSourceKind;

    #[test]
    fn one_coordinator_excludes_playout_operations() {
        let coordinator =
            OperationCoordinator::new(EntityLocker::default(), PlayoutBuildActivity::default());

        let playout = OperationKey::PlayoutBuild(9);
        let playout_guard = coordinator
            .begin(BeginOperation::new(playout.clone()))
            .into_guard()
            .expect("first playout operation");
        assert_eq!(
            coordinator.status(GetOperationStatus::new(playout.clone())),
            OperationStatus::Running
        );
        assert!(matches!(
            coordinator.begin(BeginOperation::new(playout.clone())),
            BeginOperationOutcome::AlreadyRunning
        ));
        assert_eq!(coordinator.active_playout_builds(), [9]);
        drop(playout_guard);
        assert_eq!(
            coordinator.status(GetOperationStatus::new(playout)),
            OperationStatus::Idle
        );
    }

    #[test]
    fn cloned_coordinators_admit_one_external_owner_under_contention() {
        use std::sync::{Arc, Barrier};

        for provider in ["Plex", "Jellyfin", "Emby"] {
            let coordinator =
                OperationCoordinator::new(EntityLocker::default(), PlayoutBuildActivity::default());
            let key = OperationKey::ExternalCollections {
                provider: provider.into(),
                media_source_id: 3,
            };
            let start = Arc::new(Barrier::new(8));
            let attempted = Arc::new(Barrier::new(8));
            let guards = std::thread::scope(|scope| {
                let threads: Vec<_> = (0..8)
                    .map(|_| {
                        let coordinator = coordinator.clone();
                        let key = key.clone();
                        let start = Arc::clone(&start);
                        let attempted = Arc::clone(&attempted);
                        scope.spawn(move || {
                            start.wait();
                            let guard = coordinator.begin(BeginOperation::new(key)).into_guard();
                            // All contenders must attempt admission while the winner
                            // still owns its guard, without relying on a timed sleep.
                            attempted.wait();
                            guard
                        })
                    })
                    .collect();
                threads
                    .into_iter()
                    .filter_map(|thread| thread.join().unwrap())
                    .collect::<Vec<_>>()
            });
            assert_eq!(
                guards.len(),
                1,
                "{provider}: only one caller may be admitted"
            );
            assert_eq!(
                coordinator.status(GetOperationStatus::new(key.clone())),
                OperationStatus::Running
            );
            assert!(matches!(
                coordinator.begin(BeginOperation::new(key.clone())),
                BeginOperationOutcome::AlreadyRunning
            ));
            let independent = OperationKey::ExternalCollections {
                provider: provider.into(),
                media_source_id: 4,
            };
            let independent_owner = coordinator
                .begin(BeginOperation::new(independent))
                .into_guard()
                .unwrap();
            drop(independent_owner);
            assert_eq!(
                coordinator.status(GetOperationStatus::new(key.clone())),
                OperationStatus::Running
            );
            drop(guards);
            assert_eq!(
                coordinator.status(GetOperationStatus::new(key.clone())),
                OperationStatus::Idle
            );
            assert!(coordinator
                .begin(BeginOperation::new(key))
                .into_guard()
                .is_some());
        }
    }

    #[test]
    fn external_operation_guard_releases_shared_key() {
        let coordinator =
            OperationCoordinator::new(EntityLocker::default(), PlayoutBuildActivity::default());
        let key = OperationKey::ExternalCollections {
            provider: "Plex".to_owned(),
            media_source_id: 3,
        };

        let guard = coordinator
            .begin(BeginOperation::new(key.clone()))
            .into_guard()
            .expect("first external operation");
        assert_eq!(
            coordinator.active_external_collections(),
            [("Plex".to_owned(), 3)]
        );

        drop(guard);

        assert_eq!(
            coordinator.status(GetOperationStatus::new(key)),
            OperationStatus::Idle
        );
    }

    #[test]
    fn scan_registry_tracks_success_and_cleans_terminal_progress() {
        let coordinator =
            OperationCoordinator::new(EntityLocker::default(), PlayoutBuildActivity::default());
        let registry = coordinator.scan_activity();
        let request = ScanRequest::Library {
            provider: MediaSourceKind::Plex,
            library_id: 7,
            deep_scan: false,
        };
        let operation_id = registry
            .admit(request, |_| Ok::<(), ()>(()))
            .expect("accepted");
        assert_eq!(
            registry.library_status(GetLibraryScanStatus::new(7)),
            LibraryScanStatus::Queued
        );

        let guard = registry.start_guard(operation_id).expect("worker start");
        registry.report_progress(operation_id, 1.5);
        assert_eq!(registry.progress_by_library(), [(7, 1.0)]);
        assert_eq!(
            registry.library_status(GetLibraryScanStatus::new(7)),
            LibraryScanStatus::Running
        );

        guard.complete();

        assert!(registry.active_library_ids().is_empty());
        assert!(registry.progress_by_library().is_empty());
        assert_eq!(
            registry.library_status(GetLibraryScanStatus::new(7)),
            LibraryScanStatus::Completed
        );
        assert!(matches!(
            registry.snapshots()[0].state,
            ScanOperationState::Completed
        ));
        assert_eq!(registry.snapshots()[0].progress, None);
    }

    #[test]
    fn scan_registry_retains_failure_and_releases_shared_library_boundary() {
        let coordinator =
            OperationCoordinator::new(EntityLocker::default(), PlayoutBuildActivity::default());
        let registry = coordinator.scan_activity();
        let library = ScanRequest::Library {
            provider: MediaSourceKind::Emby,
            library_id: 11,
            deep_scan: false,
        };
        let show = ScanRequest::Show {
            provider: MediaSourceKind::Emby,
            library_id: 11,
            show_id: 12,
            deep_scan: true,
        };
        let first = registry
            .admit(library, |_| Ok::<(), ()>(()))
            .expect("first");
        let second = registry
            .admit(show, |_| Ok::<(), ()>(()))
            .expect("repeated request remains queued");
        let first_guard = registry.start_guard(first).expect("first start");
        assert!(
            registry.start_guard(second).is_none(),
            "library and show commands share one exclusion boundary"
        );

        first_guard.fail("provider offline");
        let second_guard = registry
            .start_guard(second)
            .expect("next queued request starts after failure");
        drop(second_guard);

        assert!(matches!(
            registry.library_status(GetLibraryScanStatus::new(11)),
            LibraryScanStatus::Failed { .. }
        ));
        let failures = registry
            .snapshots()
            .into_iter()
            .filter(|snapshot| matches!(snapshot.state, ScanOperationState::Failed { .. }))
            .count();
        assert_eq!(failures, 2);
    }

    #[test]
    fn persisted_reclaim_resets_terminal_execution_without_resetting_active_work() {
        let coordinator =
            OperationCoordinator::new(EntityLocker::default(), PlayoutBuildActivity::default());
        let registry = coordinator.scan_activity();
        let operation_id = ScanOperationId(41);
        let request = ScanRequest::Library {
            provider: MediaSourceKind::Local,
            library_id: 14,
            deep_scan: true,
        };

        registry.observe_persisted(operation_id, request);
        registry
            .start_guard(operation_id)
            .expect("first durable execution")
            .complete();

        registry.observe_persisted(operation_id, request);
        let reclaimed = registry
            .start_guard(operation_id)
            .expect("completed durable execution is runnable after reclaim");
        registry.observe_persisted(operation_id, request);
        assert!(
            registry.start_guard(operation_id).is_none(),
            "observing an active claim must not admit overlapping local execution"
        );
        reclaimed.fail("lease lost");

        registry.observe_persisted(operation_id, request);
        registry
            .start_guard(operation_id)
            .expect("failed durable execution is runnable after reclaim")
            .complete();
        assert_eq!(
            registry.library_status(GetLibraryScanStatus::new(14)),
            LibraryScanStatus::Completed
        );
    }

    #[test]
    fn worker_shutdown_fails_queued_work_and_rejects_new_admission() {
        let coordinator =
            OperationCoordinator::new(EntityLocker::default(), PlayoutBuildActivity::default());
        let registry = coordinator.scan_activity();
        let request = ScanRequest::Library {
            provider: MediaSourceKind::Local,
            library_id: 13,
            deep_scan: false,
        };
        registry
            .admit(request, |_| Ok::<(), ()>(()))
            .expect("queued");

        registry.worker_stopped("shutdown");

        assert!(matches!(
            registry.library_status(GetLibraryScanStatus::new(13)),
            LibraryScanStatus::Failed { ref message } if message == "shutdown"
        ));
        assert!(matches!(
            registry.admit(request, |_| Ok::<(), ()>(())),
            Err(super::ScanAdmissionError::WorkerUnavailable)
        ));
    }
}
