//! Bounded ownership for an already-admitted legacy MPEG-TS wrapper.
//!
//! Tokens identify one wrapper and never grant channel-wide capacity
//! exemptions. Registration borrows the wrapper lease and owns only the
//! distinct unused first encoder lease. Supervisors retain extracted leases
//! until process cleanup finishes, including after registry cancellation.

use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex, MutexGuard, Weak,
};

use chrono::{DateTime, Utc};
use ersatzrs_infra::transcode_capacity::ActiveTranscodeReservation;
use tokio::sync::Notify;
use tokio::time::Instant;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum LegacyWrapperAdmissionError {
    #[error("legacy wrapper channel is empty")]
    InvalidChannel,
    #[error("legacy wrapper and first encoder require distinct reservations")]
    InvalidReservation,
    #[error("legacy wrapper registry is full")]
    RegistryFull,
    #[error("legacy wrapper token is unknown")]
    UnknownWrapper,
    #[error("legacy wrapper token belongs to another channel")]
    WrongChannel,
    #[error("legacy wrapper is closed")]
    Closed,
    #[error("legacy wrapper already has an item in flight")]
    ItemInFlight,
    #[error("legacy wrapper first item deadline expired")]
    FirstItemDeadlineExpired,
}

/// Weak entries do not extend wrapper lifetimes or keep tokens usable.
pub struct LegacyWrapperRegistry {
    max: usize,
    entries: Mutex<HashMap<Uuid, Weak<WrapperState>>>,
}

impl LegacyWrapperRegistry {
    #[must_use]
    pub fn new(max: usize) -> Self {
        Self {
            max,
            entries: Mutex::new(HashMap::new()),
        }
    }

    /// Register after atomic pair admission. No wrapper lease is cloned.
    /// Rejection releases the unused encoder outside the registry mutex.
    pub fn register(
        self: &Arc<Self>,
        channel: &str,
        wrapper: &ActiveTranscodeReservation,
        first_encoder: ActiveTranscodeReservation,
        first_item_deadline: Instant,
    ) -> Result<LegacyWrapperOwner, LegacyWrapperAdmissionError> {
        if channel.is_empty() {
            return Err(LegacyWrapperAdmissionError::InvalidChannel);
        }
        if wrapper.slot_id() == first_encoder.slot_id() {
            return Err(LegacyWrapperAdmissionError::InvalidReservation);
        }
        let mut entries = lock(&self.entries);
        entries.retain(|_, state| {
            state
                .upgrade()
                .is_some_and(|state| !lock(&state.inner).closed)
        });
        if entries.len() >= self.max {
            return Err(LegacyWrapperAdmissionError::RegistryFull);
        }
        let id = loop {
            let candidate = Uuid::new_v4();
            if !entries.contains_key(&candidate) {
                break candidate;
            }
        };
        let state = Arc::new(WrapperState {
            channel: channel.to_owned(),
            #[cfg(test)]
            wrapper_slot_id: wrapper.slot_id(),
            first_item_deadline,
            inner: Mutex::new(WrapperInner {
                closed: false,
                first_item_started: false,
                first_encoder: Some(first_encoder),
                in_flight: None,
                completed_playout: None,
            }),
            cancel_signal: Arc::new(Notify::new()),
            startup_changed: Notify::new(),
        });
        entries.insert(id, Arc::downgrade(&state));
        Ok(LegacyWrapperOwner {
            id,
            registry: Arc::clone(self),
            state,
        })
    }

    /// Resolve exactly one live wrapper and its original channel.
    pub fn lookup(
        &self,
        id: Uuid,
        channel: &str,
    ) -> Result<LegacyWrapperRequest, LegacyWrapperAdmissionError> {
        let state = {
            let mut entries = lock(&self.entries);
            let state = entries.get(&id).and_then(Weak::upgrade);
            if state.is_none() {
                entries.remove(&id);
            }
            state.ok_or(LegacyWrapperAdmissionError::UnknownWrapper)?
        };
        if state.channel != channel {
            return Err(LegacyWrapperAdmissionError::WrongChannel);
        }
        state.ensure_open()?;
        Ok(LegacyWrapperRequest { state })
    }
}

struct WrapperState {
    channel: String,
    // Tests correlate the exact borrowed wrapper identity with global pair
    // occupancy. Production admission validates distinct leases at register.
    #[cfg(test)]
    wrapper_slot_id: u64,
    first_item_deadline: Instant,
    inner: Mutex<WrapperInner>,
    cancel_signal: Arc<Notify>,
    startup_changed: Notify,
}

struct WrapperInner {
    closed: bool,
    first_item_started: bool,
    first_encoder: Option<ActiveTranscodeReservation>,
    in_flight: Option<Arc<ItemCancellation>>,
    completed_playout: Option<Arc<LegacyWrapperPlaybackCursor>>,
}

/// Server-selected finite interval, committed only after actual producer cleanup.
/// The next handler must revalidate this predecessor against the fresh schedule.
#[derive(Debug)]
pub(super) struct LegacyWrapperPlaybackCursor {
    pub(super) item_id: i64,
    pub(super) selected_at: DateTime<Utc>,
    pub(super) start: DateTime<Utc>,
    pub(super) finish: DateTime<Utc>,
}

impl LegacyWrapperPlaybackCursor {
    pub(super) fn new(
        item_id: i64,
        selected_at: DateTime<Utc>,
        start: DateTime<Utc>,
        finish: DateTime<Utc>,
        duration: std::time::Duration,
    ) -> Option<Self> {
        (item_id > 0
            && start <= selected_at
            && selected_at < finish
            && (finish - selected_at).to_std().ok() == Some(duration))
        .then_some(Self {
            item_id,
            selected_at,
            start,
            finish,
        })
    }
}

impl WrapperState {
    fn ensure_open(&self) -> Result<(), LegacyWrapperAdmissionError> {
        if lock(&self.inner).closed {
            Err(LegacyWrapperAdmissionError::Closed)
        } else {
            Ok(())
        }
    }

    fn close(&self) {
        self.close_matching(None);
    }

    fn close_if_current(&self, expected: &Arc<ItemCancellation>) {
        self.close_matching(Some(expected));
    }

    fn close_matching(&self, expected: Option<&Arc<ItemCancellation>>) {
        let (unused_encoder, item) = {
            let mut inner = lock(&self.inner);
            if inner.closed
                || expected.is_some_and(|expected| {
                    !inner
                        .in_flight
                        .as_ref()
                        .is_some_and(|current| Arc::ptr_eq(current, expected))
                })
            {
                return;
            }
            // Identity comparison and fencing share the same critical
            // section. A completed item's old body cannot close a successor.
            inner.closed = true;
            (inner.first_encoder.take(), inner.in_flight.clone())
        };
        // Capacity release acquires another registry mutex. It must happen
        // outside this mutex and never free a process-owned reservation.
        drop(unused_encoder);
        if let Some(item) = item {
            item.cancel();
        }
        self.cancel_signal.notify_one();
        self.startup_changed.notify_waiters();
    }
}

/// Unique lifetime owner. Drop fences outstanding request capabilities.
pub struct LegacyWrapperOwner {
    id: Uuid,
    registry: Arc<LegacyWrapperRegistry>,
    state: Arc<WrapperState>,
}

impl LegacyWrapperOwner {
    #[must_use]
    pub fn id(&self) -> Uuid {
        self.id
    }

    #[must_use]
    pub fn cancel_signal(&self) -> Arc<Notify> {
        Arc::clone(&self.state.cancel_signal)
    }

    /// Lease-free handle suitable for synchronous response-body drop.
    #[must_use]
    pub fn close_handle(&self) -> LegacyWrapperCloseHandle {
        LegacyWrapperCloseHandle {
            state: Arc::clone(&self.state),
            expected_item: None,
        }
    }

    pub fn close(&self) {
        self.state.close();
    }

    /// Resolve only when an open wrapper misses first-item admission.
    /// After admission this remains pending forever, including after item
    /// completion: completed startup must never expire later.
    pub async fn first_item_deadline_expired(&self) {
        loop {
            let changed = self.state.startup_changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            let startup_finished = {
                let inner = lock(&self.state.inner);
                inner.closed || inner.first_item_started
            };
            if startup_finished {
                std::future::pending::<()>().await;
            }
            if Instant::now() >= self.state.first_item_deadline {
                let inner = lock(&self.state.inner);
                if !inner.closed && !inner.first_item_started {
                    return;
                }
                continue;
            }
            tokio::select! {
                () = tokio::time::sleep_until(self.state.first_item_deadline) => {},
                () = &mut changed => {},
            }
        }
    }
}

impl Drop for LegacyWrapperOwner {
    fn drop(&mut self) {
        self.state.close();
        lock(&self.registry.entries).remove(&self.id);
    }
}

/// Cloneable synchronous fence with no capacity lease.
#[derive(Clone)]
pub struct LegacyWrapperCloseHandle {
    state: Arc<WrapperState>,
    expected_item: Option<Arc<ItemCancellation>>,
}

impl LegacyWrapperCloseHandle {
    pub fn close(&self) {
        match &self.expected_item {
            Some(expected) => self.state.close_if_current(expected),
            None => self.state.close(),
        }
    }
}

/// Resolved capability. Every stateful operation rechecks the wrapper fence.
pub struct LegacyWrapperRequest {
    state: Arc<WrapperState>,
}

impl LegacyWrapperRequest {
    /// Test-only identity diagnostic; it never clones or owns a reservation.
    #[cfg(test)]
    pub fn wrapper_slot_id(&self) -> Result<u64, LegacyWrapperAdmissionError> {
        self.state.ensure_open()?;
        Ok(self.state.wrapper_slot_id)
    }

    /// Only the first successful item admission receives the pre-reserved
    /// encoder. Later producers reserve independent capacity normally.
    pub fn begin_item(
        &self,
    ) -> Result<
        (LegacyWrapperItemGuard, Option<ActiveTranscodeReservation>),
        LegacyWrapperAdmissionError,
    > {
        let mut inner = lock(&self.state.inner);
        if inner.closed {
            return Err(LegacyWrapperAdmissionError::Closed);
        }
        if inner.in_flight.is_some() {
            return Err(LegacyWrapperAdmissionError::ItemInFlight);
        }
        if !inner.first_item_started && Instant::now() >= self.state.first_item_deadline {
            drop(inner);
            self.state.close();
            return Err(LegacyWrapperAdmissionError::FirstItemDeadlineExpired);
        }
        let cancellation = Arc::new(ItemCancellation {
            cancelled: AtomicBool::new(false),
            signal: Arc::new(Notify::new()),
        });
        inner.in_flight = Some(Arc::clone(&cancellation));
        inner.first_item_started = true;
        let first_encoder = inner.first_encoder.take();
        let predecessor = inner.completed_playout.clone();
        drop(inner);
        self.state.startup_changed.notify_waiters();
        Ok((
            LegacyWrapperItemGuard {
                state: Arc::clone(&self.state),
                cancellation,
                completed: false,
                predecessor,
                selected: None,
            },
            first_encoder,
        ))
    }
}

/// Exclusive item permit independent of the producer's reservation.
/// Default drop means startup/body failure and closes the parent wrapper.
pub struct LegacyWrapperItemGuard {
    state: Arc<WrapperState>,
    cancellation: Arc<ItemCancellation>,
    completed: bool,
    predecessor: Option<Arc<LegacyWrapperPlaybackCursor>>,
    selected: Option<Arc<LegacyWrapperPlaybackCursor>>,
}

impl LegacyWrapperItemGuard {
    pub(super) fn predecessor(&self) -> Option<&LegacyWrapperPlaybackCursor> {
        self.predecessor.as_deref()
    }

    pub(super) fn bind_playout(&mut self, cursor: LegacyWrapperPlaybackCursor) {
        self.selected = Some(Arc::new(cursor));
    }

    /// Fence startup abandonment or premature body drop only while this item
    /// still owns the permit. After completion this is permanently harmless,
    /// including when another item has already begun on the same wrapper.
    #[must_use]
    pub fn close_handle(&self) -> LegacyWrapperCloseHandle {
        LegacyWrapperCloseHandle {
            state: Arc::clone(&self.state),
            expected_item: Some(Arc::clone(&self.cancellation)),
        }
    }

    #[must_use]
    pub fn cancellation(&self) -> LegacyWrapperItemCancellation {
        LegacyWrapperItemCancellation {
            inner: Arc::clone(&self.cancellation),
        }
    }

    /// Call only after successful body completion AND producer cleanup.
    /// This frees the item permit, never a process-owned reservation.
    pub fn complete(mut self, produced: Option<std::time::Duration>) {
        self.selected = self.selected.take().filter(|cursor| {
            produced.is_some_and(|produced| {
                (cursor.finish - cursor.selected_at)
                    .to_std()
                    .is_ok_and(|required| produced >= required)
            })
        });
        let mut inner = lock(&self.state.inner);
        if inner
            .in_flight
            .as_ref()
            .is_some_and(|item| Arc::ptr_eq(item, &self.cancellation))
        {
            inner.in_flight = None;
            if !inner.closed {
                inner.completed_playout = self.selected.take();
            }
        }
        self.completed = true;
    }
}

impl Drop for LegacyWrapperItemGuard {
    fn drop(&mut self) {
        if !self.completed {
            self.state.close();
        }
    }
}

struct ItemCancellation {
    cancelled: AtomicBool,
    signal: Arc<Notify>,
}

impl ItemCancellation {
    fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
        self.signal.notify_waiters();
    }
}

/// Lease-free cancellation survives producer startup/body stage changes.
#[derive(Clone)]
pub struct LegacyWrapperItemCancellation {
    inner: Arc<ItemCancellation>,
}

impl LegacyWrapperItemCancellation {
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.inner.cancelled.load(Ordering::Acquire)
    }

    /// Level-triggered wait. Closure before registration, between stages or
    /// during notification registration cannot be missed.
    pub async fn cancelled(&self) {
        let notified = self.inner.signal.notified();
        tokio::pin!(notified);
        notified.as_mut().enable();
        if !self.is_cancelled() {
            notified.await;
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use ersatzrs_infra::transcode_capacity::{
        active_transcode_snapshot, active_transcode_test_guard_for_tests,
        reset_active_transcode_state_for_tests, try_reserve_mpeg_ts_wrapper_capacity,
    };

    use super::*;

    fn capacity_test(test: impl FnOnce()) {
        let _guard = active_transcode_test_guard_for_tests();
        reset_active_transcode_state_for_tests();
        test();
        assert_eq!(active_transcode_snapshot(16).active, 0);
    }

    fn owner(
        registry: &Arc<LegacyWrapperRegistry>,
        channel: &str,
    ) -> (LegacyWrapperOwner, ActiveTranscodeReservation) {
        let (wrapper, encoder) = try_reserve_mpeg_ts_wrapper_capacity(16, channel).unwrap();
        let owner = registry
            .register(
                channel,
                &wrapper,
                encoder,
                Instant::now() + Duration::from_secs(30),
            )
            .unwrap();
        (owner, wrapper)
    }

    fn runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
    }

    fn playback_cursor() -> LegacyWrapperPlaybackCursor {
        let start = "2026-11-13T12:00:00Z".parse::<DateTime<Utc>>().unwrap();
        LegacyWrapperPlaybackCursor::new(
            41,
            start + chrono::Duration::seconds(54),
            start,
            start + chrono::Duration::seconds(60),
            Duration::from_secs(6),
        )
        .unwrap()
    }

    #[test]
    fn qualified_playout_clock_commits_after_completion_and_stays_wrapper_local() {
        capacity_test(|| {
            let registry = Arc::new(LegacyWrapperRegistry::new(2));
            let (first, _first_wrapper) = owner(&registry, "same");
            let (second, _second_wrapper) = owner(&registry, "same");
            let request = registry.lookup(first.id(), "same").unwrap();
            let (mut item, encoder) = request.begin_item().unwrap();
            assert!(item.predecessor().is_none());
            item.bind_playout(playback_cursor());
            assert!(lock(&request.state.inner).completed_playout.is_none());
            assert!(matches!(
                request.begin_item(),
                Err(LegacyWrapperAdmissionError::ItemInFlight)
            ));
            drop(encoder);
            item.complete(Some(Duration::from_secs(6)));
            let (mut next, encoder) = request.begin_item().unwrap();
            assert!(encoder.is_none());
            let previous = next.predecessor().unwrap();
            assert_eq!(previous.item_id, 41);
            assert_eq!(previous.finish, playback_cursor().finish);
            let (other, encoder) = registry
                .lookup(second.id(), "same")
                .unwrap()
                .begin_item()
                .unwrap();
            assert!(other.predecessor().is_none());
            drop(encoder);
            other.complete(None);
            next.bind_playout(playback_cursor());
            next.complete(None);
            let (last, _) = request.begin_item().unwrap();
            assert!(last.predecessor().is_none());
            last.complete(None);
        });
    }

    #[test]
    fn failed_or_closed_item_cannot_publish_a_playout_clock() {
        capacity_test(|| {
            let registry = Arc::new(LegacyWrapperRegistry::new(2));
            let (first, _first_wrapper) = owner(&registry, "failed");
            let request = registry.lookup(first.id(), "failed").unwrap();
            let (mut item, encoder) = request.begin_item().unwrap();
            item.bind_playout(playback_cursor());
            drop(item);
            drop(encoder);
            assert!(lock(&request.state.inner).completed_playout.is_none());
            assert!(matches!(
                request.begin_item(),
                Err(LegacyWrapperAdmissionError::Closed)
            ));
            let (second, _second_wrapper) = owner(&registry, "closed");
            let request = registry.lookup(second.id(), "closed").unwrap();
            let (mut item, encoder) = request.begin_item().unwrap();
            item.bind_playout(playback_cursor());
            second.close();
            drop(encoder);
            item.complete(Some(Duration::from_secs(6)));
            assert!(lock(&request.state.inner).completed_playout.is_none());
            assert!(matches!(
                request.begin_item(),
                Err(LegacyWrapperAdmissionError::Closed)
            ));
        });
    }

    #[test]
    fn successful_early_eof_or_missing_progress_cannot_publish_a_nominal_finish() {
        for produced in [
            None,
            Some(Duration::from_secs(5)),
            Some(Duration::from_millis(5_999)),
        ] {
            capacity_test(|| {
                let registry = Arc::new(LegacyWrapperRegistry::new(1));
                let (owner, _wrapper) = owner(&registry, "early-eof");
                let request = registry.lookup(owner.id(), "early-eof").unwrap();
                let (mut item, encoder) = request.begin_item().unwrap();
                item.bind_playout(playback_cursor());
                drop(encoder);
                item.complete(produced);
                let (next, _) = request.begin_item().unwrap();
                assert!(next.predecessor().is_none());
                next.complete(None);
            });
        }
    }

    #[test]
    fn shortened_or_invalid_intervals_cannot_authorize_continuation() {
        let cursor = playback_cursor();
        for (id, selected, duration) in [
            (0, cursor.selected_at, Duration::from_secs(6)),
            (
                41,
                cursor.start - chrono::Duration::seconds(1),
                Duration::from_secs(61),
            ),
            (41, cursor.finish, Duration::ZERO),
            (41, cursor.selected_at, Duration::from_secs(5)),
        ] {
            assert!(LegacyWrapperPlaybackCursor::new(
                id,
                selected,
                cursor.start,
                cursor.finish,
                duration,
            )
            .is_none());
        }
    }

    #[test]
    fn same_channel_wrappers_keep_distinct_tokens_and_reservations() {
        capacity_test(|| {
            let registry = Arc::new(LegacyWrapperRegistry::new(2));
            let (first, first_wrapper) = owner(&registry, "same");
            let (second, second_wrapper) = owner(&registry, "same");
            assert_ne!(first.id(), second.id());
            assert_eq!(
                registry
                    .lookup(first.id(), "same")
                    .unwrap()
                    .wrapper_slot_id()
                    .unwrap(),
                first_wrapper.slot_id()
            );
            assert_eq!(
                registry
                    .lookup(second.id(), "same")
                    .unwrap()
                    .wrapper_slot_id()
                    .unwrap(),
                second_wrapper.slot_id()
            );
            first.close();
            assert!(registry.lookup(second.id(), "same").is_ok());
            assert_eq!(active_transcode_snapshot(16).active, 3);
        });
    }

    #[test]
    fn unknown_wrong_channel_closed_and_stale_tokens_never_admit_items() {
        capacity_test(|| {
            let registry = Arc::new(LegacyWrapperRegistry::new(1));
            let (owner, _wrapper) = owner(&registry, "one");
            assert!(matches!(
                registry.lookup(Uuid::new_v4(), "one"),
                Err(LegacyWrapperAdmissionError::UnknownWrapper)
            ));
            assert!(matches!(
                registry.lookup(owner.id(), "two"),
                Err(LegacyWrapperAdmissionError::WrongChannel)
            ));
            let request = registry.lookup(owner.id(), "one").unwrap();
            let id = owner.id();
            owner.close();
            assert!(matches!(
                request.begin_item(),
                Err(LegacyWrapperAdmissionError::Closed)
            ));
            assert_eq!(
                request.wrapper_slot_id(),
                Err(LegacyWrapperAdmissionError::Closed)
            );
            assert!(matches!(
                registry.lookup(id, "one"),
                Err(LegacyWrapperAdmissionError::Closed)
            ));
            drop(owner);
            assert!(matches!(
                registry.lookup(id, "one"),
                Err(LegacyWrapperAdmissionError::UnknownWrapper)
            ));
            assert!(matches!(
                request.begin_item(),
                Err(LegacyWrapperAdmissionError::Closed)
            ));
        });
    }

    #[test]
    fn registry_bound_reclaims_closed_entries_even_with_retained_requests() {
        capacity_test(|| {
            let registry = Arc::new(LegacyWrapperRegistry::new(1));
            let (first, _first_wrapper) = owner(&registry, "one");
            let retained = registry.lookup(first.id(), "one").unwrap();
            let (second_wrapper, second_encoder) =
                try_reserve_mpeg_ts_wrapper_capacity(16, "two").unwrap();
            assert!(matches!(
                registry.register(
                    "two",
                    &second_wrapper,
                    second_encoder,
                    Instant::now() + Duration::from_secs(30)
                ),
                Err(LegacyWrapperAdmissionError::RegistryFull)
            ));
            assert_eq!(active_transcode_snapshot(16).active, 3);
            first.close();
            let (replacement, _replacement_wrapper) = owner(&registry, "two");
            assert_eq!(lock(&registry.entries).len(), 1);
            assert!(matches!(
                retained.begin_item(),
                Err(LegacyWrapperAdmissionError::Closed)
            ));
            let replacement_id = replacement.id();
            drop(replacement);
            assert!(matches!(
                registry.lookup(replacement_id, "two"),
                Err(LegacyWrapperAdmissionError::UnknownWrapper)
            ));
            assert!(lock(&registry.entries).is_empty());
        });
    }

    #[test]
    fn registration_rejects_empty_channel_zero_bound_and_shared_lease() {
        capacity_test(|| {
            let registry = Arc::new(LegacyWrapperRegistry::new(0));
            let (wrapper, encoder) = try_reserve_mpeg_ts_wrapper_capacity(16, "one").unwrap();
            assert!(matches!(
                registry.register("one", &wrapper, encoder, Instant::now()),
                Err(LegacyWrapperAdmissionError::RegistryFull)
            ));
            assert_eq!(active_transcode_snapshot(16).active, 1);
            assert!(matches!(
                registry.register("", &wrapper, wrapper.clone(), Instant::now()),
                Err(LegacyWrapperAdmissionError::InvalidChannel)
            ));
            assert!(matches!(
                registry.register("one", &wrapper, wrapper.clone(), Instant::now()),
                Err(LegacyWrapperAdmissionError::InvalidReservation)
            ));
            assert_eq!(active_transcode_snapshot(16).active, 1);
        });
    }

    #[test]
    fn first_encoder_transfers_once_and_item_admission_is_exclusive() {
        capacity_test(|| {
            let registry = Arc::new(LegacyWrapperRegistry::new(1));
            let (owner, wrapper) = owner(&registry, "one");
            let request = registry.lookup(owner.id(), "one").unwrap();
            let competing = registry.lookup(owner.id(), "one").unwrap();
            let (first, first_encoder) = request.begin_item().unwrap();
            let first_encoder = first_encoder.unwrap();
            assert_ne!(first_encoder.slot_id(), wrapper.slot_id());
            assert!(matches!(
                competing.begin_item(),
                Err(LegacyWrapperAdmissionError::ItemInFlight)
            ));
            drop(first_encoder);
            first.complete(None);
            let (second, second_encoder) = competing.begin_item().unwrap();
            assert!(second_encoder.is_none());
            assert_eq!(active_transcode_snapshot(16).active, 1);
            second.complete(None);
        });
    }

    #[test]
    fn close_releases_only_unused_encoder_and_never_wrapper_lease() {
        capacity_test(|| {
            let registry = Arc::new(LegacyWrapperRegistry::new(1));
            let (owner, wrapper) = owner(&registry, "one");
            let handle = owner.close_handle();
            handle.close();
            handle.close();
            assert_eq!(active_transcode_snapshot(16).active, 1);
            drop(wrapper);
            assert_eq!(active_transcode_snapshot(16).active, 0);
        });
    }

    #[test]
    fn extracted_encoder_stays_owned_after_guard_failure_and_owner_drop() {
        capacity_test(|| {
            let registry = Arc::new(LegacyWrapperRegistry::new(1));
            let (owner, wrapper) = owner(&registry, "one");
            let request = registry.lookup(owner.id(), "one").unwrap();
            let (item, encoder) = request.begin_item().unwrap();
            let cancellation = item.cancellation();
            drop(item);
            assert!(cancellation.is_cancelled());
            assert!(matches!(
                request.begin_item(),
                Err(LegacyWrapperAdmissionError::Closed)
            ));
            assert_eq!(active_transcode_snapshot(16).active, 2);
            drop(owner);
            drop(wrapper);
            assert_eq!(active_transcode_snapshot(16).active, 1);
            drop(encoder);
        });
    }

    #[test]
    fn cancellation_is_level_triggered_across_stage_changes() {
        capacity_test(|| {
            runtime().block_on(async {
                let registry = Arc::new(LegacyWrapperRegistry::new(1));
                let (owner, _wrapper) = owner(&registry, "one");
                let (item, encoder) = registry
                    .lookup(owner.id(), "one")
                    .unwrap()
                    .begin_item()
                    .unwrap();
                let cancellation = item.cancellation();
                let later_stage = cancellation.clone();
                owner.close();
                assert!(cancellation.is_cancelled());
                tokio::time::timeout(Duration::from_millis(100), cancellation.cancelled())
                    .await
                    .unwrap();
                tokio::time::timeout(Duration::from_millis(100), later_stage.cancelled())
                    .await
                    .unwrap();
                item.complete(None);
                drop(encoder);
            });
        });
    }

    #[test]
    fn cancellation_wakes_all_registered_waiters() {
        capacity_test(|| {
            runtime().block_on(async {
                let registry = Arc::new(LegacyWrapperRegistry::new(1));
                let (owner, _wrapper) = owner(&registry, "one");
                let (item, encoder) = registry
                    .lookup(owner.id(), "one")
                    .unwrap()
                    .begin_item()
                    .unwrap();
                let cancellation = item.cancellation();
                let close = owner.close_handle();
                tokio::time::timeout(Duration::from_millis(100), async {
                    tokio::join!(cancellation.cancelled(), cancellation.cancelled(), async {
                        tokio::task::yield_now().await;
                        close.close();
                    });
                })
                .await
                .unwrap();
                assert!(cancellation.is_cancelled());
                item.complete(None);
                drop(encoder);
            });
        });
    }

    #[test]
    fn missed_first_item_deadline_rejects_and_fences_request() {
        capacity_test(|| {
            runtime().block_on(async {
                let registry = Arc::new(LegacyWrapperRegistry::new(1));
                let (wrapper, encoder) = try_reserve_mpeg_ts_wrapper_capacity(16, "one").unwrap();
                let owner = registry
                    .register(
                        "one",
                        &wrapper,
                        encoder,
                        Instant::now() + Duration::from_millis(10),
                    )
                    .unwrap();
                let request = registry.lookup(owner.id(), "one").unwrap();
                tokio::time::timeout(
                    Duration::from_millis(100),
                    owner.first_item_deadline_expired(),
                )
                .await
                .unwrap();
                assert!(matches!(
                    request.begin_item(),
                    Err(LegacyWrapperAdmissionError::FirstItemDeadlineExpired)
                ));
                assert!(matches!(
                    request.begin_item(),
                    Err(LegacyWrapperAdmissionError::Closed)
                ));
                assert_eq!(active_transcode_snapshot(16).active, 1);
            });
        });
    }

    #[test]
    fn first_item_deadline_stays_disarmed_after_successful_completion() {
        capacity_test(|| {
            runtime().block_on(async {
                let registry = Arc::new(LegacyWrapperRegistry::new(1));
                let (wrapper, encoder) = try_reserve_mpeg_ts_wrapper_capacity(16, "one").unwrap();
                let owner = registry
                    .register(
                        "one",
                        &wrapper,
                        encoder,
                        Instant::now() + Duration::from_millis(10),
                    )
                    .unwrap();
                let deadline = owner.first_item_deadline_expired();
                tokio::pin!(deadline);
                let request = registry.lookup(owner.id(), "one").unwrap();
                tokio::select! {
                    () = &mut deadline => panic!("deadline fired before first item"),
                    () = async { tokio::task::yield_now().await; } => {},
                }
                let (item, encoder) = request.begin_item().unwrap();
                drop(encoder);
                item.complete(None);
                assert!(
                    tokio::time::timeout(Duration::from_millis(30), &mut deadline)
                        .await
                        .is_err()
                );
                let (later, encoder) = request.begin_item().unwrap();
                assert!(encoder.is_none());
                later.complete(None);
            });
        });
    }

    #[test]
    fn owner_drop_cancels_current_item_and_fences_retained_requests() {
        capacity_test(|| {
            let registry = Arc::new(LegacyWrapperRegistry::new(1));
            let (owner, _wrapper) = owner(&registry, "one");
            let request = registry.lookup(owner.id(), "one").unwrap();
            let (item, encoder) = request.begin_item().unwrap();
            let cancellation = item.cancellation();
            drop(owner);
            assert!(cancellation.is_cancelled());
            assert!(matches!(
                request.begin_item(),
                Err(LegacyWrapperAdmissionError::Closed)
            ));
            item.complete(None);
            drop(encoder);
        });
    }

    #[test]
    fn completed_item_body_handle_cannot_close_wrapper_or_successor_item() {
        capacity_test(|| {
            let registry = Arc::new(LegacyWrapperRegistry::new(1));
            let (owner, _wrapper) = owner(&registry, "one");
            let request = registry.lookup(owner.id(), "one").unwrap();
            let (first, encoder) = request.begin_item().unwrap();
            let first_body = first.close_handle();
            first.complete(None);
            drop(encoder);
            first_body.close();
            assert!(registry.lookup(owner.id(), "one").is_ok());
            let (second, encoder) = request.begin_item().unwrap();
            assert!(encoder.is_none());
            let cancellation = second.cancellation();
            let second_body = second.close_handle();
            first_body.clone().close();
            assert!(!cancellation.is_cancelled());
            assert!(registry.lookup(owner.id(), "one").is_ok());
            assert!(matches!(
                request.begin_item(),
                Err(LegacyWrapperAdmissionError::ItemInFlight)
            ));
            second_body.close();
            assert!(cancellation.is_cancelled());
            assert!(matches!(
                request.begin_item(),
                Err(LegacyWrapperAdmissionError::Closed)
            ));
            second.complete(None);
        });
    }
}
