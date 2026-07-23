//! In-process entity locking, ported from
//! `ErsatzTV.Infrastructure.Locking.EntityLocker` and the
//! `IEntityLocker` interface in `ErsatzTV.Core/Interfaces/Locking/`.
//!
//! `ErsatzTV` uses a single `IEntityLocker` singleton across the web host,
//! scheduler, scanner channel, and worker channel — all hosted in one
//! process. Long-running operations (library scans, playout builds, Trakt
//! sync, …) acquire the relevant lock, do their work, and release it; other
//! requests skip silently when the lock is already held. The locker is
//! intentionally per-process because every consumer lives in the same
//! process.
//!
//! `ErsatzRS` keeps the same model: `EntityLocker` is a process-local
//! `Arc`-shareable struct that the scheduler binary (and, later, the web
//! binary) constructs once and hands to every loop and command that needs
//! mutual exclusion. This is parity with `ErsatzTV`'s design — not a
//! divergence.
//!
//! Only the lock surfaces `ErsatzRS` actually exercises today are exposed.
//! Additional `IEntityLocker` members (`LockTrakt`, `LockEmbyCollections`,
//! …) can be ported alongside the work that needs them.
//!
//! [`EntityLocker::lock_playout`] / [`EntityLocker::unlock_playout`] /
//! [`EntityLocker::is_playout_locked`] mirror the C# `LockPlayout`,
//! `UnlockPlayout`, and `IsPlayoutLocked` helpers used by
//! `BuildPlayoutHandler` and friends.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use ersatzrs_domain::EntityId;

use crate::scheduling::{PlayoutBuildMode, PlayoutBuildRequest};

/// Process-local mutual-exclusion registry for long-running entity
/// operations. Cheap to clone; share via `Arc<EntityLocker>` rather than
/// rebuilding inside each task.
#[derive(Debug, Default, Clone)]
pub struct EntityLocker {
    inner: Arc<EntityLockerInner>,
}

/// Process-local visibility for playout build requests that have been accepted
/// by the web host but have not yet reached a terminal worker outcome.
///
/// ErsatzTV uses an unbounded single-reader worker channel. ErsatzRS keeps the
/// same delivery model while exposing queued work separately from the existing
/// [`EntityLocker`] build lock so the admin UI can acknowledge an accepted
/// request before the scheduler starts it.
#[derive(Debug, Default, Clone)]
pub struct PlayoutBuildActivity {
    inner: Arc<PlayoutBuildActivityInner>,
}

/// Result of attempting to add one playout to the accepted-build queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayoutBuildAdmission {
    /// The playout was not active and is now queued.
    Accepted,
    /// The playout is building and its first follow-up is now queued.
    FollowUpAccepted,
    /// The playout already has accepted work waiting for the worker.
    AlreadyQueued,
    /// The worker is building and already has one follow-up waiting.
    AlreadyBuilding,
}

/// Worker-lifecycle guard that clears queued/building activity on drop.
#[must_use = "bind the guard for the complete worker build lifecycle"]
pub struct PlayoutBuildActivityGuard {
    activity: PlayoutBuildActivity,
    playout_id: EntityId,
}

impl Drop for PlayoutBuildActivityGuard {
    fn drop(&mut self) {
        self.activity.finish_build(self.playout_id);
    }
}

#[derive(Debug, Default)]
struct PlayoutBuildActivityInner {
    states: Mutex<HashMap<EntityId, PlayoutBuildState>>,
}

#[derive(Debug, Default)]
struct PlayoutBuildState {
    building: bool,
    queued_mode: Option<PlayoutBuildMode>,
}

impl PlayoutBuildActivity {
    /// Atomically admits a request and delivers a queue token when needed.
    ///
    /// Repeated queued requests upgrade the effective pending mode in place.
    /// A request received during an active build creates exactly one follow-up
    /// token; later requests upgrade that follow-up instead of adding more
    /// tokens. The worker resolves the effective mode through
    /// [`Self::start_guard`] when it consumes a token.
    ///
    /// # Errors
    ///
    /// Returns the queue adapter's error when a new token cannot be delivered.
    /// Registry state is changed only after successful delivery.
    pub fn admit<E>(
        &self,
        request: PlayoutBuildRequest,
        enqueue: impl FnOnce(PlayoutBuildRequest) -> Result<(), E>,
    ) -> Result<PlayoutBuildAdmission, E> {
        let mut states = recover(self.inner.states.lock());
        if let Some(state) = states.get_mut(&request.playout_id) {
            if state.building {
                if let Some(mode) = state.queued_mode {
                    state.queued_mode = Some(stronger_build_mode(mode, request.mode));
                    return Ok(PlayoutBuildAdmission::AlreadyBuilding);
                }

                enqueue(request)?;
                state.queued_mode = Some(request.mode);
                return Ok(PlayoutBuildAdmission::FollowUpAccepted);
            }

            state.queued_mode = Some(stronger_build_mode(
                state.queued_mode.unwrap_or(request.mode),
                request.mode,
            ));
            return Ok(PlayoutBuildAdmission::AlreadyQueued);
        }

        enqueue(request)?;
        states.insert(
            request.playout_id,
            PlayoutBuildState {
                building: false,
                queued_mode: Some(request.mode),
            },
        );
        Ok(PlayoutBuildAdmission::Accepted)
    }

    /// Marks a playout request as queued.
    ///
    /// This compatibility helper retains the original activity-only behavior:
    /// it returns `false` when the playout is already queued or building. New
    /// application code should call [`Self::admit`] with the real queue adapter
    /// so delivery, mode upgrades, and follow-up state remain atomic.
    #[must_use]
    pub fn queue(&self, playout_id: EntityId) -> bool {
        let mut states = recover(self.inner.states.lock());
        if states.contains_key(&playout_id) {
            return false;
        }
        states.insert(
            playout_id,
            PlayoutBuildState {
                building: false,
                queued_mode: Some(PlayoutBuildMode::Continue),
            },
        );
        true
    }

    /// Moves an accepted token from queued to building and resolves its
    /// strongest coalesced mode.
    #[must_use]
    pub fn start(&self, request: PlayoutBuildRequest) -> PlayoutBuildRequest {
        let mut states = recover(self.inner.states.lock());
        let state = states.entry(request.playout_id).or_default();
        let mode = state.queued_mode.take().map_or(request.mode, |queued| {
            stronger_build_mode(request.mode, queued)
        });
        state.building = true;
        PlayoutBuildRequest { mode, ..request }
    }

    /// Starts worker processing and returns a guard that clears activity after
    /// success, failure, cancellation, or any other terminal return path. The
    /// returned request contains the strongest mode coalesced for this token.
    pub fn start_guard(
        &self,
        request: PlayoutBuildRequest,
    ) -> (PlayoutBuildRequest, PlayoutBuildActivityGuard) {
        let request = self.start(request);
        let guard = PlayoutBuildActivityGuard {
            activity: self.clone(),
            playout_id: request.playout_id,
        };
        (request, guard)
    }

    /// Removes all tracked work for a playout.
    ///
    /// This is intended for explicit cancellation and compatibility cleanup.
    /// Worker lifecycle uses the guard so a queued follow-up is retained.
    pub fn finish(&self, playout_id: EntityId) {
        let _ = recover(self.inner.states.lock()).remove(&playout_id);
    }

    /// Snapshot queued playout identifiers for polling UI clients.
    #[must_use]
    pub fn queued_playouts(&self) -> Vec<EntityId> {
        recover(self.inner.states.lock())
            .iter()
            .filter_map(|(&playout_id, state)| state.queued_mode.map(|_| playout_id))
            .collect()
    }

    /// Snapshot building playout identifiers for polling UI clients.
    #[must_use]
    pub fn building_playouts(&self) -> Vec<EntityId> {
        recover(self.inner.states.lock())
            .iter()
            .filter_map(|(&playout_id, state)| state.building.then_some(playout_id))
            .collect()
    }

    fn finish_build(&self, playout_id: EntityId) {
        let mut states = recover(self.inner.states.lock());
        let remove = states.get_mut(&playout_id).is_some_and(|state| {
            state.building = false;
            state.queued_mode.is_none()
        });
        if remove {
            let _ = states.remove(&playout_id);
        }
    }
}

fn stronger_build_mode(left: PlayoutBuildMode, right: PlayoutBuildMode) -> PlayoutBuildMode {
    use PlayoutBuildMode::{Continue, Refresh, Reset};

    match (left, right) {
        (Reset, _) | (_, Reset) => Reset,
        (Refresh, _) | (_, Refresh) => Refresh,
        (Continue, Continue) => Continue,
    }
}

#[derive(Debug, Default)]
struct EntityLockerInner {
    locked_playouts: Mutex<HashSet<EntityId>>,
    locked_libraries: Mutex<HashSet<EntityId>>,
}

impl EntityLocker {
    /// Construct an empty locker.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Try to acquire the lock for a playout.
    ///
    /// Returns `true` when the caller now holds the lock. Returns `false`
    /// when another caller already holds it — at which point the caller
    /// should skip the work, mirroring `EntityLocker.LockPlayout` in
    /// `ErsatzTV.Infrastructure.Locking.EntityLocker`.
    #[must_use]
    pub fn lock_playout(&self, playout_id: EntityId) -> bool {
        recover(self.inner.locked_playouts.lock()).insert(playout_id)
    }

    /// Release a playout lock previously acquired by [`Self::lock_playout`].
    /// Returns `true` when the lock was held and has been released.
    #[must_use]
    pub fn unlock_playout(&self, playout_id: EntityId) -> bool {
        recover(self.inner.locked_playouts.lock()).remove(&playout_id)
    }

    /// Whether a playout is currently locked.
    #[must_use]
    pub fn is_playout_locked(&self, playout_id: EntityId) -> bool {
        recover(self.inner.locked_playouts.lock()).contains(&playout_id)
    }

    /// Snapshot of every currently locked playout id. Mirrors how the admin UI
    /// polls `IEntityLocker` to render the per-row build spinner on the
    /// playouts page.
    #[must_use]
    pub fn locked_playouts(&self) -> Vec<EntityId> {
        recover(self.inner.locked_playouts.lock())
            .iter()
            .copied()
            .collect()
    }

    /// Try to acquire the lock for a library scan. Mirrors `LockLibrary`.
    #[must_use]
    pub fn lock_library(&self, library_id: EntityId) -> bool {
        recover(self.inner.locked_libraries.lock()).insert(library_id)
    }

    /// Release a library lock previously acquired by [`Self::lock_library`].
    #[must_use]
    pub fn unlock_library(&self, library_id: EntityId) -> bool {
        recover(self.inner.locked_libraries.lock()).remove(&library_id)
    }

    /// Whether a library is currently locked.
    #[must_use]
    pub fn is_library_locked(&self, library_id: EntityId) -> bool {
        recover(self.inner.locked_libraries.lock()).contains(&library_id)
    }

    /// Snapshot every library id whose scan lock is currently held.
    #[must_use]
    pub fn locked_libraries(&self) -> Vec<EntityId> {
        recover(self.inner.locked_libraries.lock())
            .iter()
            .copied()
            .collect()
    }
}

fn recover<'a, T>(
    result: std::result::Result<MutexGuard<'a, T>, PoisonError<MutexGuard<'a, T>>>,
) -> MutexGuard<'a, T> {
    // A poisoned mutex here means a previous holder panicked while mutating
    // the lock set. The data is just a `HashSet<EntityId>`, so recovering the
    // inner guard is safe and avoids cascading the panic into unrelated
    // maintenance work.
    result.unwrap_or_else(PoisonError::into_inner)
}

/// RAII-style guard that releases a playout lock on drop. Acquire via
/// [`PlayoutLockGuard::try_acquire`].
#[must_use = "dropping the guard releases the lock; bind it to a name"]
pub struct PlayoutLockGuard {
    locker: EntityLocker,
    playout_id: EntityId,
}

impl PlayoutLockGuard {
    /// Attempt to acquire the lock for `playout_id`. Returns `None` when
    /// another caller already holds it.
    #[must_use]
    pub fn try_acquire(locker: &EntityLocker, playout_id: EntityId) -> Option<Self> {
        if locker.lock_playout(playout_id) {
            Some(Self {
                locker: locker.clone(),
                playout_id,
            })
        } else {
            None
        }
    }
}

impl Drop for PlayoutLockGuard {
    fn drop(&mut self) {
        let _ = self.locker.unlock_playout(self.playout_id);
    }
}

/// RAII-style guard that releases a library scan lock on drop. Acquire via
/// [`LibraryLockGuard::try_acquire`].
#[must_use = "dropping the guard releases the lock; bind it to a name"]
pub struct LibraryLockGuard {
    locker: EntityLocker,
    library_id: EntityId,
}

impl LibraryLockGuard {
    /// Attempt to acquire the lock for `library_id`. Returns `None` while a
    /// scan already owns the same library lock.
    #[must_use]
    pub fn try_acquire(locker: &EntityLocker, library_id: EntityId) -> Option<Self> {
        if locker.lock_library(library_id) {
            Some(Self {
                locker: locker.clone(),
                library_id,
            })
        } else {
            None
        }
    }
}

impl Drop for LibraryLockGuard {
    fn drop(&mut self) {
        let _ = self.locker.unlock_library(self.library_id);
    }
}

#[cfg(test)]
mod tests {
    use std::convert::Infallible;

    use super::*;

    #[test]
    fn lock_playout_is_exclusive_until_unlocked() {
        let locker = EntityLocker::new();
        assert!(locker.lock_playout(7));
        assert!(locker.is_playout_locked(7));
        assert!(!locker.lock_playout(7));
        assert!(locker.unlock_playout(7));
        assert!(!locker.is_playout_locked(7));
        assert!(locker.lock_playout(7));
    }

    #[test]
    fn playout_lock_guard_releases_on_drop() {
        let locker = EntityLocker::new();
        {
            let guard = PlayoutLockGuard::try_acquire(&locker, 11).expect("first acquire");
            assert!(locker.is_playout_locked(11));
            assert!(PlayoutLockGuard::try_acquire(&locker, 11).is_none());
            drop(guard);
        }
        assert!(!locker.is_playout_locked(11));
    }

    #[test]
    fn library_lock_is_independent_of_playout_lock() {
        let locker = EntityLocker::new();
        assert!(locker.lock_library(3));
        assert!(locker.lock_playout(3));
        assert!(locker.is_library_locked(3));
        assert!(locker.is_playout_locked(3));
        assert!(locker.unlock_library(3));
        assert!(!locker.is_library_locked(3));
        assert!(locker.is_playout_locked(3));
    }

    #[test]
    fn library_lock_guard_releases_and_reports_snapshot() {
        let locker = EntityLocker::new();
        let guard = LibraryLockGuard::try_acquire(&locker, 5).expect("first acquire");

        assert!(LibraryLockGuard::try_acquire(&locker, 5).is_none());
        assert_eq!(locker.locked_libraries(), [5]);

        drop(guard);

        assert!(locker.locked_libraries().is_empty());
    }

    #[test]
    fn playout_build_activity_upgrades_one_queued_token_to_strongest_mode() {
        let activity = PlayoutBuildActivity::default();
        let mut tokens = Vec::new();
        let mut admit = |mode| {
            activity
                .admit(
                    PlayoutBuildRequest {
                        playout_id: 7,
                        mode,
                    },
                    |request| {
                        tokens.push(request);
                        Ok::<(), Infallible>(())
                    },
                )
                .expect("infallible queue")
        };

        assert_eq!(
            admit(PlayoutBuildMode::Continue),
            PlayoutBuildAdmission::Accepted
        );
        assert_eq!(
            admit(PlayoutBuildMode::Refresh),
            PlayoutBuildAdmission::AlreadyQueued
        );
        assert_eq!(
            admit(PlayoutBuildMode::Reset),
            PlayoutBuildAdmission::AlreadyQueued
        );
        assert_eq!(tokens.len(), 1, "queued upgrades reuse the existing token");
        assert_eq!(activity.queued_playouts(), [7]);

        let (request, guard) = activity.start_guard(tokens[0]);
        assert_eq!(request.mode, PlayoutBuildMode::Reset);
        assert!(activity.queued_playouts().is_empty());
        assert_eq!(activity.building_playouts(), [7]);

        drop(guard);
        assert!(activity.building_playouts().is_empty());
        assert!(activity.queue(7));
    }

    #[test]
    fn playout_build_activity_retains_one_upgradable_follow_up_during_build() {
        let activity = PlayoutBuildActivity::default();
        let mut tokens = Vec::new();
        let request = PlayoutBuildRequest {
            playout_id: 9,
            mode: PlayoutBuildMode::Continue,
        };
        assert_eq!(
            activity.admit(request, |token| {
                tokens.push(token);
                Ok::<(), Infallible>(())
            }),
            Ok(PlayoutBuildAdmission::Accepted)
        );
        let (_, active_guard) = activity.start_guard(tokens[0]);

        assert_eq!(
            activity.admit(
                PlayoutBuildRequest {
                    mode: PlayoutBuildMode::Refresh,
                    ..request
                },
                |token| {
                    tokens.push(token);
                    Ok::<(), Infallible>(())
                },
            ),
            Ok(PlayoutBuildAdmission::FollowUpAccepted)
        );
        assert_eq!(
            activity.admit(
                PlayoutBuildRequest {
                    mode: PlayoutBuildMode::Reset,
                    ..request
                },
                |token| {
                    tokens.push(token);
                    Ok::<(), Infallible>(())
                },
            ),
            Ok(PlayoutBuildAdmission::AlreadyBuilding)
        );
        assert_eq!(tokens.len(), 2, "active requests share one follow-up token");
        assert_eq!(activity.queued_playouts(), [9]);
        assert_eq!(activity.building_playouts(), [9]);

        drop(active_guard);
        assert_eq!(activity.queued_playouts(), [9]);
        assert!(activity.building_playouts().is_empty());

        let (follow_up, follow_up_guard) = activity.start_guard(tokens[1]);
        assert_eq!(follow_up.mode, PlayoutBuildMode::Reset);
        drop(follow_up_guard);
        assert!(activity.queued_playouts().is_empty());
        assert!(activity.building_playouts().is_empty());
    }

    #[test]
    fn playout_build_activity_retains_large_accepted_batches() {
        let activity = PlayoutBuildActivity::default();
        for id in 1..=512 {
            assert!(activity.queue(id));
        }
        assert_eq!(activity.queued_playouts().len(), 512);
    }

    #[test]
    fn playout_build_activity_guard_cleans_up_worker_failure_path() {
        let activity = PlayoutBuildActivity::default();
        assert!(activity.queue(12));

        {
            let (_, _guard) = activity.start_guard(PlayoutBuildRequest {
                playout_id: 12,
                mode: PlayoutBuildMode::Continue,
            });
            assert_eq!(activity.building_playouts(), [12]);
        }

        assert!(activity.queued_playouts().is_empty());
        assert!(activity.building_playouts().is_empty());
        assert!(activity.queue(12), "terminal cleanup must permit a retry");
    }
}
