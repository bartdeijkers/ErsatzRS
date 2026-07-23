//! Shared process-local coordination for long-running application operations.

use std::collections::HashSet;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use ersatzrs_core::locking::{
    EntityLocker, LibraryLockGuard, PlayoutBuildActivity, PlayoutLockGuard,
};
use ersatzrs_domain::EntityId;

use crate::commands::BeginOperation;
use crate::queries::{GetOperationStatus, OperationStatus};

/// Stable identity for an operation coordinated across transport and worker
/// adapters.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum OperationKey {
    /// Scan one library.
    LibraryScan(EntityId),
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
    Library {
        _guard: LibraryLockGuard,
    },
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

/// Single application-owned coordination surface shared by web and workers.
#[derive(Clone)]
pub struct OperationCoordinator {
    locker: EntityLocker,
    playout_activity: PlayoutBuildActivity,
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
            inner: Arc::new(OperationCoordinatorInner::default()),
        }
    }

    /// Attempt to acquire one operation command.
    #[must_use]
    pub fn begin(&self, command: BeginOperation) -> BeginOperationOutcome {
        let guard = match command.key {
            OperationKey::LibraryScan(library_id) => {
                LibraryLockGuard::try_acquire(&self.locker, library_id).map(|guard| {
                    OperationGuard {
                        kind: OperationGuardKind::Library { _guard: guard },
                    }
                })
            }
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
            OperationKey::LibraryScan(id) => self.locker.is_library_locked(id),
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
        self.locker.locked_libraries()
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
                OperationKey::LibraryScan(_) | OperationKey::PlayoutBuild(_) => None,
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

fn recover<'a, T>(
    result: std::result::Result<MutexGuard<'a, T>, PoisonError<MutexGuard<'a, T>>>,
) -> MutexGuard<'a, T> {
    result.unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::{BeginOperationOutcome, OperationCoordinator, OperationKey};
    use crate::commands::BeginOperation;
    use crate::queries::{GetOperationStatus, OperationStatus};
    use ersatzrs_core::locking::{EntityLocker, PlayoutBuildActivity};

    #[test]
    fn one_coordinator_excludes_library_and_playout_operations() {
        let coordinator =
            OperationCoordinator::new(EntityLocker::default(), PlayoutBuildActivity::default());

        let library = OperationKey::LibraryScan(4);
        let library_guard = coordinator
            .begin(BeginOperation::new(library.clone()))
            .into_guard()
            .expect("first library operation");
        assert_eq!(
            coordinator.status(GetOperationStatus::new(library.clone())),
            OperationStatus::Running
        );
        assert!(matches!(
            coordinator.begin(BeginOperation::new(library.clone())),
            BeginOperationOutcome::AlreadyRunning
        ));
        drop(library_guard);
        assert_eq!(
            coordinator.status(GetOperationStatus::new(library)),
            OperationStatus::Idle
        );

        let playout = OperationKey::PlayoutBuild(9);
        let _playout_guard = coordinator
            .begin(BeginOperation::new(playout.clone()))
            .into_guard()
            .expect("first playout operation");
        assert_eq!(coordinator.active_playout_builds(), [9]);
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
}
