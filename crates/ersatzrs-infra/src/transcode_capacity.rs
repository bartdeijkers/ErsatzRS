use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicI32, AtomicI64, AtomicU64, Ordering},
    Arc, Mutex as StdMutex, OnceLock,
};
use std::time::Duration;

use chrono::Utc;
use tokio::sync::Notify;

use crate::db::DbPool;

const DEFAULT_ACTIVE_TRANSCODE_LIMIT: i32 = 2;

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
    #[must_use]
    pub fn new_hls(
        channel_number: &str,
        kind: ActiveTranscodeKind,
        last_access_ms: Arc<AtomicI64>,
        reclaim: ActiveTranscodeReclaimHandle,
    ) -> Self {
        let slot_id = next_active_transcode_slot_id();
        Self {
            slot_id,
            key: format!("hls:{channel_number}"),
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

pub struct ActiveTranscodeReservation {
    key: String,
    slot_id: u64,
}

impl ActiveTranscodeReservation {
    #[must_use]
    pub const fn slot_id(&self) -> u64 {
        self.slot_id
    }
}

impl Drop for ActiveTranscodeReservation {
    fn drop(&mut self) {
        release_active_transcode(&self.key);
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

pub async fn load_active_transcode_limit(pool: &DbPool) -> i32 {
    sqlx::query_scalar::<_, String>(r#"SELECT "Value" FROM "ConfigElement" WHERE "Key" = ?"#)
        .bind(ersatzrs_domain::CONFIG_KEY_FFMPEG_ACTIVE_TRANSCODE_LIMIT)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .and_then(|value| value.trim().parse::<i32>().ok())
        .unwrap_or(DEFAULT_ACTIVE_TRANSCODE_LIMIT)
        .max(1)
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

    Some(ActiveTranscodeReservation { key, slot_id })
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
        slot.kind == ActiveTranscodeKind::HlsWorkAhead
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
}
