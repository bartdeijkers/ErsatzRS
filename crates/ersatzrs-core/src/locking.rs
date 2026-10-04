//! In-process entity locking, ported from
//! `ErsatzTV.Infrastructure.Locking.EntityLocker` and the
//! `IEntityLocker` interface in `ErsatzTV.Core/Interfaces/Locking/`.
//!
//! `ErsatzTV` uses a single `IEntityLocker` singleton across the web host,
//! scheduler, scanner channel, and worker channel — all hosted in one
//! process. Long-running operations (library scans, playout builds, provider
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
//! Additional upstream `IEntityLocker` members can be ported alongside the
//! work that needs them.
//!
//! [`EntityLocker::lock_playout`] / [`EntityLocker::unlock_playout`] /
//! [`EntityLocker::is_playout_locked`] mirror the C# `LockPlayout`,
//! `UnlockPlayout`, and `IsPlayoutLocked` helpers used by
//! `BuildPlayoutHandler` and friends.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use ersatzrs_domain::EntityId;

use crate::scheduling::{PlayoutBuildMode, PlayoutBuildRequest, PlayoutPreservation};

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
    queued_request: Option<PlayoutBuildRequest>,
}

impl PlayoutBuildActivity {
    /// Atomically admits a request and delivers a queue token when needed.
    ///
    /// Automatic requests retain the strongest pending mode. The latest explicit
    /// operator choice replaces earlier queued choices, including preservation;
    /// automatic work cannot override that choice. A request received during a
    /// build creates one follow-up token, then coalesces into that token.
    /// The worker resolves the complete request through
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
                if let Some(queued) = state.queued_request {
                    state.queued_request = Some(coalesce_build_request(queued, request));
                    return Ok(PlayoutBuildAdmission::AlreadyBuilding);
                }

                enqueue(request)?;
                state.queued_request = Some(request);
                return Ok(PlayoutBuildAdmission::FollowUpAccepted);
            }

            state.queued_request = Some(coalesce_build_request(
                state.queued_request.unwrap_or(request),
                request,
            ));
            return Ok(PlayoutBuildAdmission::AlreadyQueued);
        }

        enqueue(request)?;
        states.insert(
            request.playout_id,
            PlayoutBuildState {
                building: false,
                queued_request: Some(request),
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
                queued_request: Some(PlayoutBuildRequest::new(
                    playout_id,
                    PlayoutBuildMode::Continue,
                )),
            },
        );
        true
    }

    /// Moves an accepted token from queued to building and resolves its
    /// complete coalesced choice.
    #[must_use]
    pub fn start(&self, request: PlayoutBuildRequest) -> PlayoutBuildRequest {
        let mut states = recover(self.inner.states.lock());
        let state = states.entry(request.playout_id).or_default();
        let effective = state
            .queued_request
            .take()
            .map_or(request, |queued| coalesce_build_request(request, queued));
        state.building = true;
        effective
    }

    /// Starts worker processing and returns a guard that clears activity after
    /// success, failure, cancellation, or any other terminal return path. The
    /// returned request contains the effective mode and preservation policy.
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
            .filter_map(|(&playout_id, state)| state.queued_request.map(|_| playout_id))
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
            state.queued_request.is_none()
        });
        if remove {
            let _ = states.remove(&playout_id);
        }
    }
}

fn coalesce_build_request(
    previous: PlayoutBuildRequest,
    incoming: PlayoutBuildRequest,
) -> PlayoutBuildRequest {
    if incoming.preservation != PlayoutPreservation::EngineDefault {
        incoming
    } else if previous.preservation != PlayoutPreservation::EngineDefault {
        previous
    } else {
        PlayoutBuildRequest {
            mode: stronger_build_mode(previous.mode, incoming.mode),
            ..incoming
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
    locked_playouts: Mutex<HashMap<EntityId, PlayoutLockState>>,
    locked_libraries: Mutex<HashSet<EntityId>>,
}

#[derive(Debug, Default)]
struct PlayoutLockState {
    writer: bool,
    publication_reads: bool,
    readers: usize,
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
        let mut locks = recover(self.inner.locked_playouts.lock());
        if locks.contains_key(&playout_id) {
            return false;
        }
        locks.insert(
            playout_id,
            PlayoutLockState {
                writer: true,
                ..Default::default()
            },
        );
        true
    }

    /// Release a playout lock previously acquired by [`Self::lock_playout`].
    /// Returns `true` when the lock was held and has been released.
    #[must_use]
    pub fn unlock_playout(&self, playout_id: EntityId) -> bool {
        let mut locks = recover(self.inner.locked_playouts.lock());
        let Some(state) = locks.get_mut(&playout_id) else {
            return false;
        };
        if !state.writer {
            return false;
        }
        state.writer = false;
        if state.readers == 0 {
            locks.remove(&playout_id);
        }
        true
    }

    /// Whether a playout is currently locked.
    #[must_use]
    pub fn is_playout_locked(&self, playout_id: EntityId) -> bool {
        recover(self.inner.locked_playouts.lock()).contains_key(&playout_id)
    }

    /// Snapshot of every currently locked playout id. Mirrors how the admin UI
    /// polls `IEntityLocker` to render the per-row build spinner on the
    /// playouts page.
    #[must_use]
    pub fn locked_playouts(&self) -> Vec<EntityId> {
        recover(self.inner.locked_playouts.lock())
            .keys()
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
    // the lock registry. Its values are process-local counters, so recovering the
    // inner guard is safe and avoids cascading the panic into unrelated
    // maintenance work.
    result.unwrap_or_else(PoisonError::into_inner)
}

/// RAII-style guard that releases a playout lock on drop. Acquire via
/// [`PlayoutLockGuard::try_acquire`].
#[must_use = "dropping the guard releases the lock; bind it to a name"]
pub struct PlayoutLockGuard {
    lease: Arc<PlayoutBuildLease>,
}

struct PlayoutBuildLease {
    locker: EntityLocker,
    playout_id: EntityId,
}

/// Keep a build exclusive while an owned finalizer settles external publication.
/// This hold cannot admit readers or acquire another build.
#[must_use = "retain the hold until finalization and rollback have finished"]
pub struct PlayoutBuildHold {
    _lease: Arc<PlayoutBuildLease>,
}

impl PlayoutLockGuard {
    /// Allow snapshot publication after all timeline writes have committed.
    /// The build remains exclusive against later builders while its publishers
    /// may read the committed timeline. Do not mutate that timeline after this
    /// transition; publication readers retain their own guard through file swap.
    pub fn allow_publication_reads(&mut self) {
        if let Some(state) =
            recover(self.lease.locker.inner.locked_playouts.lock()).get_mut(&self.lease.playout_id)
        {
            state.publication_reads = true;
        }
    }

    /// Retain exclusivity across cancellation of the calling async task. The
    /// owned finalizer must keep this hold until commit or rollback completes.
    pub fn hold_finalization(&self) -> PlayoutBuildHold {
        PlayoutBuildHold {
            _lease: self.lease.clone(),
        }
    }

    /// Attempt to acquire the lock for `playout_id`. Returns `None` when
    /// another caller already holds it.
    #[must_use]
    pub fn try_acquire(locker: &EntityLocker, playout_id: EntityId) -> Option<Self> {
        if locker.lock_playout(playout_id) {
            Some(Self {
                lease: Arc::new(PlayoutBuildLease {
                    locker: locker.clone(),
                    playout_id,
                }),
            })
        } else {
            None
        }
    }
}

impl Drop for PlayoutBuildLease {
    fn drop(&mut self) {
        let _ = self.locker.unlock_playout(self.playout_id);
    }
}

/// Own a stable timeline read through projection and file publication. Readers
/// may share the committed phase of a build, but never its mutation phase.
#[must_use = "retain the read guard until publication finishes"]
pub struct PlayoutReadGuard {
    locker: EntityLocker,
    playout_id: EntityId,
}

impl PlayoutReadGuard {
    /// Read an idle or fully committed playout; refuse an active mutation phase.
    #[must_use]
    pub fn try_acquire(locker: &EntityLocker, playout_id: EntityId) -> Option<Self> {
        let mut locks = recover(locker.inner.locked_playouts.lock());
        let state = locks.entry(playout_id).or_default();
        if state.writer && !state.publication_reads {
            return None;
        }
        state.readers = state.readers.checked_add(1)?;
        Some(Self {
            locker: locker.clone(),
            playout_id,
        })
    }
}

impl Drop for PlayoutReadGuard {
    fn drop(&mut self) {
        let mut locks = recover(self.locker.inner.locked_playouts.lock());
        if let Some(state) = locks.get_mut(&self.playout_id) {
            state.readers -= 1;
            if !state.writer && state.readers == 0 {
                locks.remove(&self.playout_id);
            }
        }
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
    fn finalizer_keeps_mutation_exclusive_after_caller_is_dropped() {
        let locker = EntityLocker::new();
        let build = PlayoutLockGuard::try_acquire(&locker, 7).unwrap();
        let hold = build.hold_finalization();
        drop(build);
        assert!(locker.is_playout_locked(7));
        assert!(PlayoutReadGuard::try_acquire(&locker, 7).is_none());
        assert!(PlayoutLockGuard::try_acquire(&locker, 7).is_none());
        drop(hold);
        assert!(PlayoutLockGuard::try_acquire(&locker, 7).is_some());
    }

    #[test]
    fn publication_readers_share_committed_build_but_exclude_later_writers() {
        let locker = EntityLocker::new();
        let mut build = PlayoutLockGuard::try_acquire(&locker, 7).unwrap();
        assert!(PlayoutReadGuard::try_acquire(&locker, 7).is_none());
        build.allow_publication_reads();
        let first = PlayoutReadGuard::try_acquire(&locker, 7).unwrap();
        let second = PlayoutReadGuard::try_acquire(&locker, 7).unwrap();
        assert!(PlayoutLockGuard::try_acquire(&locker, 7).is_none());
        drop(build);
        assert!(!locker.unlock_playout(7), "a reader is not a writer lease");
        assert!(PlayoutLockGuard::try_acquire(&locker, 7).is_none());
        drop(first);
        assert!(locker.is_playout_locked(7));
        drop(second);
        assert!(!locker.is_playout_locked(7));
        let fresh = PlayoutLockGuard::try_acquire(&locker, 7).unwrap();
        assert!(PlayoutReadGuard::try_acquire(&locker, 7).is_none());
        drop(fresh);
        let idle_reader = PlayoutReadGuard::try_acquire(&locker, 7).unwrap();
        assert!(PlayoutLockGuard::try_acquire(&locker, 7).is_none());
        drop(idle_reader);
        assert!(locker.locked_playouts().is_empty());
    }

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
    fn explicit_playout_choice_survives_automatic_work_and_latest_operator_choice_wins() {
        let activity = PlayoutBuildActivity::default();
        let automatic = PlayoutBuildRequest::new(7, PlayoutBuildMode::Continue);
        let preserve = PlayoutBuildRequest::new(7, PlayoutBuildMode::Refresh)
            .with_preservation(PlayoutPreservation::KeepCurrent);
        let replace = PlayoutBuildRequest::new(7, PlayoutBuildMode::Reset)
            .with_preservation(PlayoutPreservation::ReplaceCurrent);
        let mut tokens = Vec::new();
        for request in [automatic, replace, preserve, automatic] {
            activity
                .admit(request, |token| {
                    tokens.push(token);
                    Ok::<_, Infallible>(())
                })
                .unwrap();
        }
        assert_eq!(tokens, [automatic]);
        let (request, guard) = activity.start_guard(tokens[0]);
        assert_eq!(
            request, preserve,
            "the token must resolve the complete latest choice"
        );
        let mut follow_ups = Vec::new();
        for request in [preserve, replace, automatic] {
            activity
                .admit(request, |token| {
                    follow_ups.push(token);
                    Ok::<_, Infallible>(())
                })
                .unwrap();
        }
        assert_eq!(follow_ups, [preserve]);
        drop(guard);
        let (request, guard) = activity.start_guard(follow_ups[0]);
        assert_eq!(
            request, replace,
            "later automatic work cannot remove explicit replacement"
        );
        drop(guard);
        assert!(activity.queued_playouts().is_empty());
        assert!(activity.building_playouts().is_empty());
    }

    #[test]
    fn playout_build_activity_upgrades_one_queued_token_to_strongest_mode() {
        let activity = PlayoutBuildActivity::default();
        let mut tokens = Vec::new();
        let mut admit = |mode| {
            activity
                .admit(PlayoutBuildRequest::new(7, mode), |request| {
                    tokens.push(request);
                    Ok::<(), Infallible>(())
                })
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
        let request = PlayoutBuildRequest::new(9, PlayoutBuildMode::Continue);
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
            let (_, _guard) =
                activity.start_guard(PlayoutBuildRequest::new(12, PlayoutBuildMode::Continue));
            assert_eq!(activity.building_playouts(), [12]);
        }

        assert!(activity.queued_playouts().is_empty());
        assert!(activity.building_playouts().is_empty());
        assert!(activity.queue(12), "terminal cleanup must permit a retry");
    }
}
