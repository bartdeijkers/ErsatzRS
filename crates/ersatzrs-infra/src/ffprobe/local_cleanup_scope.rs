//! Operation-local ownership of local `FFprobe` cleanup, without scan deadlines.

use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::watch;
use tokio::time::Instant;

const MAX_ACTIVE_OWNERS: usize = 64;
const CLOSED_DRAIN_TIMEOUT: Duration = Duration::from_secs(8);

tokio::task_local! {
    static CURRENT: LocalProbeCleanupScope;
}

/// Cleanup failed or an owner disappeared without confirming process and pipe exit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum LocalProbeCleanupError {
    #[error("local ffprobe cleanup could not be confirmed")]
    CleanupUnconfirmed,
}

#[derive(Debug, Clone, Copy, Default)]
struct State {
    active: usize,
    closed_at: Option<Instant>,
    uncertain: bool,
}

#[derive(Debug)]
struct Inner {
    state: Mutex<State>,
    changed: watch::Sender<State>,
}

/// Tracks only local `FFprobe` owners belonging to one operation.
///
/// This scope owns no database lease. Its coordinator must close it, destroy
/// the execution future, and await cleanup before releasing operation resources.
/// Tokio task-local state is **not inherited by spawned tasks**: explicitly scope
/// their futures when needed. Nested scopes use the innermost scope; nesting the
/// same scope is safe, while distinct scopes must each be drained by their owner.
#[derive(Debug, Clone)]
pub struct LocalProbeCleanupScope {
    inner: Arc<Inner>,
}

impl Default for LocalProbeCleanupScope {
    fn default() -> Self {
        Self::new()
    }
}

impl LocalProbeCleanupScope {
    /// Create an independent operation scope, initially open and idle.
    #[must_use]
    pub fn new() -> Self {
        let state = State::default();
        let (changed, _) = watch::channel(state);
        Self {
            inner: Arc::new(Inner {
                state: Mutex::new(state),
                changed,
            }),
        }
    }

    /// Poll an execution future with this scope as its local probe owner.
    pub async fn scope<F: Future>(&self, future: F) -> F::Output {
        CURRENT.scope(self.clone(), future).await
    }

    /// Return the innermost scope of the currently polled task, if present.
    #[must_use]
    pub fn current() -> Option<Self> {
        CURRENT.try_with(Clone::clone).ok()
    }

    /// Fence new registrations and signal every already registered probe owner.
    /// Repeated calls preserve the original cleanup deadline.
    pub fn close(&self) {
        self.update(|state| {
            state.closed_at.get_or_insert_with(Instant::now);
        });
    }

    /// Whether no owners are active and every completed owner confirmed cleanup.
    /// An open idle scope can still admit later probes; this is a status snapshot.
    ///
    /// # Errors
    /// Returns sticky uncertainty even when the active count has reached zero.
    pub fn cleanup_status(&self) -> Result<bool, LocalProbeCleanupError> {
        let state = self.state();
        if state.uncertain {
            Err(LocalProbeCleanupError::CleanupUnconfirmed)
        } else {
            Ok(state.active == 0)
        }
    }

    /// Await every registered owner's process and reader cleanup.
    ///
    /// An open scope has no operation timeout: an active ordinary probe may run
    /// indefinitely. Closing starts a fresh eight-second drain budget, covering
    /// the existing six-second process and one-second pipe cleanup budgets.
    /// Timeout records sticky uncertainty and leaves each live owner intact.
    /// Callers must stop polling execution before relying on a drained snapshot.
    ///
    /// # Errors
    /// Returns unconfirmed cleanup after an uncertain owner or closed-scope timeout.
    pub async fn wait_cleanup(&self) -> Result<(), LocalProbeCleanupError> {
        let mut changed = self.inner.changed.subscribe();
        loop {
            let state = *changed.borrow_and_update();
            if state.active == 0 {
                match self.cleanup_status() {
                    Ok(true) => return Ok(()),
                    Ok(false) => continue,
                    Err(problem) => return Err(problem),
                }
            }
            let advanced = if let Some(closed_at) = state.closed_at {
                tokio::time::timeout_at(closed_at + CLOSED_DRAIN_TIMEOUT, changed.changed())
                    .await
                    .is_ok_and(|result| result.is_ok())
            } else {
                changed.changed().await.is_ok()
            };
            if !advanced {
                self.update(mark_uncertain);
                return Err(LocalProbeCleanupError::CleanupUnconfirmed);
            }
        }
    }

    fn state(&self) -> State {
        *self
            .inner
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn update(&self, update: impl FnOnce(&mut State)) {
        let mut state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        update(&mut state);
        self.inner.changed.send_replace(*state);
    }

    fn register(&self) -> Result<OwnerGuard, AdmissionError> {
        let mut state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.uncertain {
            return Err(AdmissionError::Unconfirmed);
        }
        if state.closed_at.is_some() {
            return Err(AdmissionError::Closed);
        }
        if state.active == MAX_ACTIVE_OWNERS {
            return Err(AdmissionError::Capacity);
        }
        state.active += 1;
        self.inner.changed.send_replace(*state);
        Ok(OwnerGuard {
            scope: self.clone(),
            completed: false,
        })
    }
}

fn mark_uncertain(state: &mut State) {
    state.uncertain = true;
    state.closed_at.get_or_insert_with(Instant::now);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AdmissionError {
    Closed,
    Unconfirmed,
    Capacity,
}

pub(super) fn register_current() -> Result<Option<OwnerGuard>, AdmissionError> {
    CURRENT
        .try_with(|scope| scope.register().map(Some))
        .unwrap_or(Ok(None))
}

pub(super) struct OwnerGuard {
    scope: LocalProbeCleanupScope,
    completed: bool,
}

impl OwnerGuard {
    pub(super) fn cancellation_requested(&self) -> bool {
        self.scope.state().closed_at.is_some()
    }

    pub(super) async fn cancelled(&self) {
        let mut changed = self.scope.inner.changed.subscribe();
        let _ = changed.wait_for(|state| state.closed_at.is_some()).await;
    }

    pub(super) fn complete(mut self, confirmed: bool) {
        self.finish(confirmed);
        self.completed = true;
    }

    fn finish(&self, confirmed: bool) {
        self.scope.update(|state| {
            state.active = state.active.saturating_sub(1);
            if !confirmed {
                mark_uncertain(state);
            }
        });
    }
}

impl Drop for OwnerGuard {
    fn drop(&mut self) {
        if !self.completed {
            self.finish(false);
        }
    }
}

#[cfg(test)]
#[path = "local_cleanup_scope_tests.rs"]
mod tests;
