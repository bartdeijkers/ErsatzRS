use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicBool, AtomicI32, AtomicI64, AtomicU64, Ordering},
    Arc, Mutex as StdMutex, OnceLock,
};
use std::time::Duration;

use chrono::Utc;
use ersatzrs_core::repositories::{RepositoryError, Result};
use tokio::sync::Notify;

use crate::db::DbPool;
use crate::repositories::common::map_data_access_error;

/// Effective active-transcode limit when the persisted setting is absent.
pub const DEFAULT_ACTIVE_TRANSCODE_LIMIT: i32 = 2;

static ACTIVE_TRANSCODE_SLOT_ID: AtomicU64 = AtomicU64::new(1);
static ACTIVE_TRANSCODE_PEAK: AtomicI32 = AtomicI32::new(0);
static ACTIVE_TRANSCODE_SLOTS: OnceLock<StdMutex<HashMap<String, ActiveTranscodeSlot>>> =
    OnceLock::new();
#[cfg(any(test, debug_assertions))]
static ACTIVE_TRANSCODE_TEST_LOCK: OnceLock<StdMutex<()>> = OnceLock::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveTranscodeKind {
    HlsRealtime,
    HlsWorkAhead,
    MpegTs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveTranscodeReclaimReason {
    WorkAhead,
    Cooling,
    StaleWatched,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActiveTranscodeSnapshot {
    pub active: i32,
    pub limit: i32,
    pub peak: i32,
}

#[derive(Clone)]
pub struct ActiveTranscodeReclaimHandle {
    notify: Arc<Notify>,
    requested_slot_id: Arc<AtomicU64>,
    reacquire_deferred_until_ms: Arc<AtomicI64>,
    allow_watched_preemption: bool,
}

impl ActiveTranscodeReclaimHandle {
    #[must_use]
    pub fn new(
        notify: Arc<Notify>,
        requested_slot_id: Arc<AtomicU64>,
        reacquire_deferred_until_ms: Arc<AtomicI64>,
        allow_watched_preemption: bool,
    ) -> Self {
        Self {
            notify,
            requested_slot_id,
            reacquire_deferred_until_ms,
            allow_watched_preemption,
        }
    }
}

#[derive(Clone)]
pub struct ActiveTranscodeSlot {
    slot_id: u64,
    key: String,
    channel_number: String,
    kind: ActiveTranscodeKind,
    last_access_ms: Arc<AtomicI64>,
    reclaim: Option<ActiveTranscodeReclaimHandle>,
}

impl ActiveTranscodeSlot {
    /// Next owns its encoding process tree for the session lifetime. Its slot
    /// shares native HLS identity/capacity but cannot be reclaimed per programme.
    #[must_use]
    pub fn new_next_hls(channel_number: &str) -> Self {
        Self::new_next_hls_with_capacity_key(channel_number, channel_number)
    }

    /// Give a Next preview independent admission while retaining its channel identity.
    #[must_use]
    pub fn new_next_hls_with_capacity_key(channel_number: &str, capacity_key: &str) -> Self {
        Self {
            slot_id: next_active_transcode_slot_id(),
            key: format!("hls:{capacity_key}"),
            channel_number: channel_number.to_owned(),
            kind: ActiveTranscodeKind::HlsRealtime,
            last_access_ms: Arc::new(AtomicI64::new(Utc::now().timestamp_millis())),
            reclaim: None,
        }
    }

    #[must_use]
    pub fn new_hls(
        channel_number: &str,
        kind: ActiveTranscodeKind,
        last_access_ms: Arc<AtomicI64>,
        reclaim: ActiveTranscodeReclaimHandle,
    ) -> Self {
        Self::new_hls_with_capacity_key(
            channel_number,
            channel_number,
            kind,
            last_access_ms,
            reclaim,
        )
    }

    /// Create an HLS slot whose global admission identity is distinct from its
    /// physical channel identity.
    ///
    /// Preview and normal HLS workers for the same channel use different
    /// capacity keys so both may run when the shared limit has room. The
    /// channel number remains unchanged for reporting and reclaim policy.
    #[must_use]
    pub fn new_hls_with_capacity_key(
        channel_number: &str,
        capacity_key: &str,
        kind: ActiveTranscodeKind,
        last_access_ms: Arc<AtomicI64>,
        reclaim: ActiveTranscodeReclaimHandle,
    ) -> Self {
        let slot_id = next_active_transcode_slot_id();
        Self {
            slot_id,
            key: format!("hls:{capacity_key}"),
            channel_number: channel_number.to_owned(),
            kind,
            last_access_ms,
            reclaim: Some(reclaim),
        }
    }

    #[must_use]
    pub fn new_mpeg_ts(channel_number: &str) -> Self {
        let slot_id = next_active_transcode_slot_id();
        Self {
            slot_id,
            key: format!("mpegts:{channel_number}:{slot_id}"),
            channel_number: channel_number.to_owned(),
            kind: ActiveTranscodeKind::MpegTs,
            last_access_ms: Arc::new(AtomicI64::new(Utc::now().timestamp_millis())),
            reclaim: None,
        }
    }

    #[must_use]
    pub const fn slot_id(&self) -> u64 {
        self.slot_id
    }

    fn idle_for(&self, now_ms: i64) -> Duration {
        let idle_ms = now_ms
            .saturating_sub(self.last_access_ms.load(Ordering::Relaxed))
            .max(0);
        Duration::from_millis(u64::try_from(idle_ms).unwrap_or(u64::MAX))
    }

    fn request_reclaim(&self, now_ms: i64, reclaimed_reacquire_delay: Duration) {
        let Some(reclaim) = &self.reclaim else {
            return;
        };
        let delay_ms = i64::try_from(reclaimed_reacquire_delay.as_millis()).unwrap_or(i64::MAX);
        reclaim
            .reacquire_deferred_until_ms
            .store(now_ms.saturating_add(delay_ms), Ordering::Release);
        reclaim
            .requested_slot_id
            .store(self.slot_id, Ordering::Release);
    }

    fn can_reclaim_stale_watched(&self) -> bool {
        self.reclaim
            .as_ref()
            .is_some_and(|reclaim| reclaim.allow_watched_preemption)
    }
}

#[derive(Clone)]
pub struct ActiveTranscodeReservation {
    lease: Arc<ActiveTranscodeLease>,
}

struct ActiveTranscodeLease {
    key: String,
    slot_id: u64,
    release_on_drop: AtomicBool,
}

impl ActiveTranscodeReservation {
    #[must_use]
    pub fn slot_id(&self) -> u64 {
        self.lease.slot_id
    }

    /// Preserve admission after a managed process cannot be reaped. The
    /// existing bounded slot registry owns the quarantine until host restart.
    pub fn retain_until_restart(&self) {
        self.lease.release_on_drop.store(false, Ordering::Release);
        let mut slots = active_transcode_slots()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(slot) = slots.get_mut(&self.lease.key) {
            slot.reclaim = None;
        }
    }
}

impl Drop for ActiveTranscodeLease {
    fn drop(&mut self) {
        if self.release_on_drop.load(Ordering::Acquire) {
            release_active_transcode(&self.key);
        }
    }
}

pub struct ActiveTranscodeReclaim {
    pub channel_number: String,
    pub slot_id: u64,
    pub notify: Arc<Notify>,
    pub reason: ActiveTranscodeReclaimReason,
}

#[must_use]
pub fn active_transcode_snapshot(limit: i32) -> ActiveTranscodeSnapshot {
    let slots = active_transcode_slots()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let active = i32::try_from(slots.len()).unwrap_or(i32::MAX);
    ActiveTranscodeSnapshot {
        active,
        limit: limit.max(1),
        peak: ACTIVE_TRANSCODE_PEAK.load(Ordering::Acquire),
    }
}

/// Load the configured active-transcode limit.
///
/// `Ok(None)` means the setting is absent and callers may apply
/// [`DEFAULT_ACTIVE_TRANSCODE_LIMIT`].
///
/// # Errors
///
/// Returns a database error when the query fails and a decode error for null,
/// non-numeric, or out-of-range persisted values.
pub async fn load_active_transcode_limit(pool: &DbPool) -> Result<Option<i32>> {
    let value = sqlx::query_scalar::<_, Option<String>>(
        r#"SELECT "Value" FROM "ConfigElement" WHERE "Key" = ?"#,
    )
    .bind(ersatzrs_domain::CONFIG_KEY_FFMPEG_ACTIVE_TRANSCODE_LIMIT)
    .fetch_optional(pool)
    .await
    .map_err(|error| map_data_access_error("active transcode limit", &error))?;
    let Some(value) = value else {
        return Ok(None);
    };
    let value = value.ok_or_else(|| RepositoryError::Decode {
        kind: "active transcode limit",
        value: "persisted value is null".to_owned(),
    })?;
    let value = value
        .trim()
        .parse::<i32>()
        .map_err(|error| RepositoryError::Decode {
            kind: "active transcode limit",
            value: error.to_string(),
        })?;
    if value < 1 {
        return Err(RepositoryError::Decode {
            kind: "active transcode limit",
            value: format!("expected a value greater than or equal to 1, found {value}"),
        });
    }
    Ok(Some(value))
}

#[must_use]
pub fn try_reserve_active_transcode(
    limit: i32,
    slot: ActiveTranscodeSlot,
) -> Option<ActiveTranscodeReservation> {
    if limit <= 0 {
        return None;
    }

    let mut slots = active_transcode_slots()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if slots.len() >= usize::try_from(limit).unwrap_or(usize::MAX) {
        return None;
    }
    if slots.contains_key(&slot.key) {
        return None;
    }

    let key = slot.key.clone();
    let slot_id = slot.slot_id;
    slots.insert(key.clone(), slot);
    let active = i32::try_from(slots.len()).unwrap_or(i32::MAX);
    ACTIVE_TRANSCODE_PEAK.fetch_max(active, Ordering::AcqRel);

    Some(ActiveTranscodeReservation {
        lease: Arc::new(ActiveTranscodeLease {
            key,
            slot_id,
            release_on_drop: AtomicBool::new(true),
        }),
    })
}

pub fn request_active_transcode_reclaim(
    requester_channel: &str,
    watch_window: Duration,
    stale_watched_preemption_threshold: Duration,
    reclaimed_reacquire_delay: Duration,
) -> Option<ActiveTranscodeReclaim> {
    let now_ms = Utc::now().timestamp_millis();
    let slots = active_transcode_slots()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    let work_ahead = best_reclaim_candidate(&slots, requester_channel, now_ms, |slot, _| {
        slot.kind == ActiveTranscodeKind::HlsWorkAhead && slot.reclaim.is_some()
    })
    .map(|slot| (slot, ActiveTranscodeReclaimReason::WorkAhead));

    let cooling = || {
        best_reclaim_candidate(&slots, requester_channel, now_ms, |slot, idle_for| {
            slot.reclaim.is_some() && idle_for >= watch_window
        })
        .map(|slot| (slot, ActiveTranscodeReclaimReason::Cooling))
    };

    let stale_watched = || {
        best_reclaim_candidate(&slots, requester_channel, now_ms, |slot, idle_for| {
            slot.can_reclaim_stale_watched()
                && idle_for >= stale_watched_preemption_threshold
                && idle_for < watch_window
        })
        .map(|slot| (slot, ActiveTranscodeReclaimReason::StaleWatched))
    };

    let (slot, reason) = work_ahead.or_else(cooling).or_else(stale_watched)?;
    slot.request_reclaim(now_ms, reclaimed_reacquire_delay);
    let notify = Arc::clone(&slot.reclaim.as_ref()?.notify);
    Some(ActiveTranscodeReclaim {
        channel_number: slot.channel_number.clone(),
        slot_id: slot.slot_id,
        notify,
        reason,
    })
}

/// Reclassify a running HLS work-ahead transcode after it has filled its
/// startup buffer and continues as a realtime producer.
///
/// Returns `true` only when the identified slot still exists and was promoted.
#[must_use]
pub fn promote_hls_transcode_to_realtime(slot_id: u64) -> bool {
    let mut slots = active_transcode_slots()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some(slot) = slots.values_mut().find(|slot| slot.slot_id == slot_id) else {
        return false;
    };
    if slot.kind != ActiveTranscodeKind::HlsWorkAhead {
        return false;
    }
    slot.kind = ActiveTranscodeKind::HlsRealtime;
    true
}

fn next_active_transcode_slot_id() -> u64 {
    ACTIVE_TRANSCODE_SLOT_ID.fetch_add(1, Ordering::Relaxed)
}

fn active_transcode_slots() -> &'static StdMutex<HashMap<String, ActiveTranscodeSlot>> {
    ACTIVE_TRANSCODE_SLOTS.get_or_init(|| StdMutex::new(HashMap::new()))
}

fn release_active_transcode(key: &str) {
    let mut slots = active_transcode_slots()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    slots.remove(key);
}

fn best_reclaim_candidate<'a>(
    slots: &'a HashMap<String, ActiveTranscodeSlot>,
    requester_channel: &str,
    now_ms: i64,
    predicate: impl Fn(&ActiveTranscodeSlot, Duration) -> bool,
) -> Option<&'a ActiveTranscodeSlot> {
    slots
        .values()
        .filter(|slot| slot.channel_number != requester_channel)
        .filter_map(|slot| {
            let idle_for = slot.idle_for(now_ms);
            predicate(slot, idle_for).then_some((slot, idle_for))
        })
        .max_by_key(|(_, idle_for)| *idle_for)
        .map(|(slot, _)| slot)
}

#[cfg(any(test, debug_assertions))]
pub fn reset_active_transcode_state_for_tests() {
    let mut slots = active_transcode_slots()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    slots.clear();
    ACTIVE_TRANSCODE_PEAK.store(0, Ordering::Release);
}

#[cfg(any(test, debug_assertions))]
/// Serializes tests that mutate global active-transcode state.
///
/// # Panics
///
/// Panics if another test poisons the shared lock.
pub fn active_transcode_test_guard_for_tests() -> std::sync::MutexGuard<'static, ()> {
    ACTIVE_TRANSCODE_TEST_LOCK
        .get_or_init(|| StdMutex::new(()))
        .lock()
        .expect("active transcode test lock")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preparation_clone_holds_one_slot_until_its_last_owner_finishes() {
        let _guard = active_transcode_test_guard_for_tests();
        reset_active_transcode_state_for_tests();
        let request = try_reserve_active_transcode(1, hls_slot("song", "song")).unwrap();
        let preparation = request.clone();
        assert_eq!(request.slot_id(), preparation.slot_id());
        assert_eq!(active_transcode_snapshot(1).active, 1);
        drop(request);
        assert!(try_reserve_active_transcode(1, hls_slot("other", "other")).is_none());
        drop(preparation);
        assert_eq!(active_transcode_snapshot(1).active, 0);
        reset_active_transcode_state_for_tests();
    }

    #[test]
    fn unconfirmed_process_keeps_capacity_and_disables_reclaim_until_restart() {
        let _guard = active_transcode_test_guard_for_tests();
        reset_active_transcode_state_for_tests();
        let request = try_reserve_active_transcode(1, hls_slot("song", "song")).unwrap();
        let preparation = request.clone();
        preparation.retain_until_restart();
        drop(preparation);
        drop(request);
        assert_eq!(active_transcode_snapshot(1).active, 1);
        assert!(try_reserve_active_transcode(1, hls_slot("other", "other")).is_none());
        assert!(request_active_transcode_reclaim(
            "other",
            Duration::ZERO,
            Duration::ZERO,
            Duration::ZERO
        )
        .is_none());
        reset_active_transcode_state_for_tests();
    }

    #[test]
    fn quarantined_work_ahead_does_not_hide_another_reclaimable_slot() {
        let _guard = active_transcode_test_guard_for_tests();
        reset_active_transcode_state_for_tests();
        let mut old_slot = hls_slot("uncertain", "uncertain");
        old_slot.kind = ActiveTranscodeKind::HlsWorkAhead;
        old_slot
            .last_access_ms
            .store(Utc::now().timestamp_millis() - 60_000, Ordering::Release);
        let old = try_reserve_active_transcode(2, old_slot).unwrap();
        old.retain_until_restart();
        let mut healthy_slot = hls_slot("healthy", "healthy");
        healthy_slot.kind = ActiveTranscodeKind::HlsWorkAhead;
        let healthy = try_reserve_active_transcode(2, healthy_slot).unwrap();
        let reclaim = request_active_transcode_reclaim(
            "other",
            Duration::ZERO,
            Duration::ZERO,
            Duration::ZERO,
        )
        .unwrap();
        assert_eq!(reclaim.slot_id, healthy.slot_id());
        assert_eq!(reclaim.channel_number, "healthy");
        assert_eq!(reclaim.reason, ActiveTranscodeReclaimReason::WorkAhead);
        drop(healthy);
        drop(old);
        reset_active_transcode_state_for_tests();
    }

    fn hls_slot(channel_number: &str, capacity_key: &str) -> ActiveTranscodeSlot {
        ActiveTranscodeSlot::new_hls_with_capacity_key(
            channel_number,
            capacity_key,
            ActiveTranscodeKind::HlsRealtime,
            Arc::new(AtomicI64::new(Utc::now().timestamp_millis())),
            ActiveTranscodeReclaimHandle::new(
                Arc::new(Notify::new()),
                Arc::new(AtomicU64::new(0)),
                Arc::new(AtomicI64::new(0)),
                true,
            ),
        )
    }

    #[test]
    fn work_ahead_slot_can_be_promoted_without_releasing_capacity() {
        let _guard = active_transcode_test_guard_for_tests();
        reset_active_transcode_state_for_tests();
        let slot = ActiveTranscodeSlot::new_hls_with_capacity_key(
            "42",
            "42",
            ActiveTranscodeKind::HlsWorkAhead,
            Arc::new(AtomicI64::new(Utc::now().timestamp_millis())),
            ActiveTranscodeReclaimHandle::new(
                Arc::new(Notify::new()),
                Arc::new(AtomicU64::new(0)),
                Arc::new(AtomicI64::new(0)),
                true,
            ),
        );
        let slot_id = slot.slot_id();
        let reservation = try_reserve_active_transcode(1, slot).expect("reserve work ahead");

        assert!(promote_hls_transcode_to_realtime(slot_id));
        assert!(!promote_hls_transcode_to_realtime(slot_id));
        assert_eq!(active_transcode_snapshot(1).active, 1);
        assert!(request_active_transcode_reclaim(
            "other",
            Duration::from_secs(30),
            Duration::from_secs(25),
            Duration::from_millis(500),
        )
        .is_none());

        drop(reservation);
        reset_active_transcode_state_for_tests();
    }

    async fn config_pool(value: Option<&str>) -> DbPool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("open transcode-capacity test database");
        sqlx::query(
            r#"CREATE TABLE "ConfigElement" (
                "Key" TEXT NULL,
                "Value" TEXT NULL
            )"#,
        )
        .execute(&pool)
        .await
        .expect("create ConfigElement");
        if let Some(value) = value {
            sqlx::query(r#"INSERT INTO "ConfigElement" ("Key", "Value") VALUES (?, ?)"#)
                .bind(ersatzrs_domain::CONFIG_KEY_FFMPEG_ACTIVE_TRANSCODE_LIMIT)
                .bind(value)
                .execute(&pool)
                .await
                .expect("seed active transcode limit");
        }
        pool
    }

    #[tokio::test]
    async fn active_transcode_limit_distinguishes_absence_invalid_value_and_database_failure() {
        let absent_pool = config_pool(None).await;
        assert_eq!(
            load_active_transcode_limit(&absent_pool)
                .await
                .expect("load absent limit"),
            None
        );

        let invalid_pool = config_pool(Some("not-a-number")).await;
        assert!(matches!(
            load_active_transcode_limit(&invalid_pool).await,
            Err(RepositoryError::Decode { .. })
        ));

        let closed_pool = config_pool(Some("2")).await;
        closed_pool.close().await;
        assert!(matches!(
            load_active_transcode_limit(&closed_pool).await,
            Err(RepositoryError::Unavailable { .. })
        ));
    }

    #[test]
    fn snapshot_tracks_active_limit_and_peak() {
        let _guard = active_transcode_test_guard_for_tests();
        reset_active_transcode_state_for_tests();

        let first = try_reserve_active_transcode(2, ActiveTranscodeSlot::new_mpeg_ts("1")).unwrap();
        assert_eq!(
            active_transcode_snapshot(2),
            ActiveTranscodeSnapshot {
                active: 1,
                limit: 2,
                peak: 1
            }
        );

        let second =
            try_reserve_active_transcode(2, ActiveTranscodeSlot::new_mpeg_ts("2")).unwrap();
        assert_eq!(
            active_transcode_snapshot(2),
            ActiveTranscodeSnapshot {
                active: 2,
                limit: 2,
                peak: 2
            }
        );

        drop(second);
        drop(first);
        assert_eq!(
            active_transcode_snapshot(2),
            ActiveTranscodeSnapshot {
                active: 0,
                limit: 2,
                peak: 2
            }
        );
    }

    #[test]
    fn mpegts_slots_count_toward_limit_but_are_not_hls_reclaim_targets() {
        let _guard = active_transcode_test_guard_for_tests();
        reset_active_transcode_state_for_tests();
        let _reservation =
            try_reserve_active_transcode(1, ActiveTranscodeSlot::new_mpeg_ts("7")).unwrap();

        assert!(try_reserve_active_transcode(1, ActiveTranscodeSlot::new_mpeg_ts("8")).is_none());
        assert!(request_active_transcode_reclaim(
            "8",
            Duration::from_secs(30),
            Duration::from_secs(25),
            Duration::from_millis(500),
        )
        .is_none());
    }

    #[test]
    fn normal_and_preview_hls_sessions_share_limit_without_key_collision() {
        let _guard = active_transcode_test_guard_for_tests();
        reset_active_transcode_state_for_tests();

        let normal =
            try_reserve_active_transcode(2, hls_slot("42", "42")).expect("normal reservation");
        let preview = try_reserve_active_transcode(2, hls_slot("42", "preview:42"))
            .expect("preview reservation");

        assert_eq!(active_transcode_snapshot(2).active, 2);
        assert!(try_reserve_active_transcode(2, hls_slot("43", "43")).is_none());

        drop(preview);
        drop(normal);
    }
}
