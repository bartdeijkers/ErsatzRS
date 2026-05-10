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

use std::collections::HashSet;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use ersatzrs_domain::EntityId;

/// Process-local mutual-exclusion registry for long-running entity
/// operations. Cheap to clone; share via `Arc<EntityLocker>` rather than
/// rebuilding inside each task.
#[derive(Debug, Default, Clone)]
pub struct EntityLocker {
    inner: Arc<EntityLockerInner>,
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

#[cfg(test)]
mod tests {
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
}
