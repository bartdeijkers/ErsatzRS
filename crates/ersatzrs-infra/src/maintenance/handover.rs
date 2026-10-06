//! Bound preserving publication before any producer can select its new future.

use super::{MaintenanceError, Result};
use chrono::{DateTime, Duration, Utc};
use ersatzrs_core::{
    hls::HLS_WORK_AHEAD_BUFFER_SECONDS,
    scheduling::{
        PlayoutBuildMode, PlayoutBuildRequest, PlayoutPreservation, SchedulingError,
        SchedulingResult,
    },
};
use ersatzrs_domain::{EntityId, PlayoutScheduleKind};

// Native buffer admission uses whole seconds, so 60.999 seconds is admitted.
// Next selects at at most 60 seconds ahead; an unknown-duration fallback can
// then reserve another minute. Known programme/gap reservations are preserved
// by selecting a complete programme boundary, not by truncating their output.
const PREFETCH_RESERVE: Duration = Duration::seconds(HLS_WORK_AHEAD_BUFFER_SECONDS + 1 + 60);
const MINIMUM_PUBLICATION_WINDOW: Duration = Duration::seconds(30);

// Two bounded replacement retry sequences total at most 3.55 seconds. Leave
// additional local-file copy/flush time per distinct channel publisher.
pub(super) const ROLLBACK_SECONDS_PER_OWNER: u32 = 5;
pub(super) const SQL_DECISION_MARGIN_SECONDS: u32 = 2;

#[derive(Clone, Copy)]
pub(super) struct Window {
    pub(super) not_before: DateTime<Utc>,
    lead: Duration,
}

pub(super) fn preserves_programmes(
    request: PlayoutBuildRequest,
    kind: PlayoutScheduleKind,
) -> bool {
    request.mode != PlayoutBuildMode::Continue
        && (request.preservation == PlayoutPreservation::KeepCurrent
            || (kind == PlayoutScheduleKind::Scripted
                && request.mode == PlayoutBuildMode::Refresh
                && request.preservation == PlayoutPreservation::EngineDefault))
}

impl Window {
    pub(super) async fn load(
        pool: &crate::db::DbPool,
        source_channel: EntityId,
        now: DateTime<Utc>,
    ) -> Result<Self> {
        // Include configured mirrors even when idle: a viewer can join while
        // the candidate is being built. A negative offset reads source future.
        let offsets = sqlx::query_scalar::<_, Option<String>>(
            r#"SELECT "PlayoutOffset" FROM "Channel"
               WHERE "PlayoutSource"=1 AND "MirrorSourceChannelId"=?"#,
        )
        .bind(source_channel)
        .fetch_all(pool)
        .await?;
        let mut mirror_lead = Duration::zero();
        for offset in offsets.into_iter().flatten() {
            let offset = crate::repositories::common::try_decode_signed_duration_text(&offset)?;
            mirror_lead = mirror_lead.max(
                Duration::zero()
                    .checked_sub(&offset)
                    .ok_or_else(invalid_window)?,
            );
        }
        let lead = PREFETCH_RESERVE
            .checked_add(&mirror_lead)
            .ok_or_else(invalid_window)?;
        let next_owners: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM "Channel" WHERE "StreamingEngine"=1
               AND ("Id"=? OR ("PlayoutSource"=1 AND "MirrorSourceChannelId"=?))"#,
        )
        .bind(source_channel)
        .bind(source_channel)
        .fetch_one(pool)
        .await?;
        let recovery = if next_owners == 0 {
            Duration::zero()
        } else {
            next_owners
                .checked_mul(i64::from(ROLLBACK_SECONDS_PER_OWNER))
                .and_then(|seconds| seconds.checked_add(i64::from(SQL_DECISION_MARGIN_SECONDS)))
                .and_then(Duration::try_seconds)
                .ok_or_else(invalid_window)?
        };
        let not_before = now
            .checked_add_signed(lead)
            .and_then(|clock| clock.checked_add_signed(MINIMUM_PUBLICATION_WINDOW))
            .and_then(|clock| clock.checked_add_signed(recovery))
            // Generated rows/checkpoints use 100 ns ticks. Round up so a new
            // empty-timeline occurrence cannot round below its own safe floor.
            .and_then(|clock| clock.checked_add_signed(Duration::nanoseconds(99)))
            .map(ersatzrs_core::scheduling::truncate_to_persisted_precision)
            .ok_or_else(invalid_window)?;
        Ok(Self { not_before, lead })
    }

    pub(super) fn publication_clock(
        self,
        result: &SchedulingResult,
        now: DateTime<Utc>,
    ) -> Result<Option<DateTime<Utc>>> {
        // An empty old timeline can still have a running fallback producer.
        // In that case the first newly generated item defines the handover.
        let boundary = result
            .retain_through
            .or_else(|| result.added_items.iter().map(|item| item.start).min());
        self.boundary_clock(boundary, now)
    }

    pub(super) fn boundary_clock(
        self,
        boundary: Option<DateTime<Utc>>,
        now: DateTime<Utc>,
    ) -> Result<Option<DateTime<Utc>>> {
        boundary
            .map(|boundary| {
                let clock = boundary
                    .checked_sub_signed(self.lead)
                    .ok_or_else(invalid_window)?;
                if boundary < self.not_before || clock <= now {
                    return Err(invalid_window());
                }
                Ok(clock)
            })
            .transpose()
    }
}

fn invalid_window() -> MaintenanceError {
    SchedulingError::InvalidRebuild(
        "cannot select a publication window that preserves prefetched programmes".into(),
    )
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[tokio::test]
    async fn configured_idle_next_mirror_extends_the_recovery_reserve() {
        use ersatzrs_core::repositories::ChannelRepository;
        let root = tempfile::tempdir().unwrap();
        let (_, pool, _, now) = crate::next_engine::host::publication_fixture(root.path()).await;
        let before = Window::load(&pool, 1, now).await.unwrap();
        let repository = crate::repositories::make_repositories(pool.clone()).channel;
        let mut mirror = repository.get_with_profile(2).await.unwrap();
        mirror.id = 3;
        mirror.number = "3".into();
        repository.upsert(&mirror).await.unwrap();
        let after = Window::load(&pool, 1, now).await.unwrap();
        assert_eq!(after.lead, before.lead);
        assert_eq!(
            after.not_before - before.not_before,
            Duration::seconds(i64::from(ROLLBACK_SECONDS_PER_OWNER))
        );
    }

    #[tokio::test]
    async fn fractional_empty_timeline_handover_is_publishable_before_prefetch() {
        let pool = crate::db::init("sqlite::memory:").await.unwrap();
        let now = Utc.with_ymd_and_hms(2026, 4, 15, 12, 0, 0).unwrap()
            + Duration::nanoseconds(123_456_789);
        let window = Window::load(&pool, 1, now).await.unwrap();
        assert_eq!(window.not_before.timestamp_subsec_nanos(), 123_456_800);
        let result = SchedulingResult {
            added_items: vec![ersatzrs_domain::PlayoutItem {
                start: window.not_before,
                finish: window.not_before + Duration::minutes(1),
                ..Default::default()
            }],
            ..Default::default()
        };
        let deadline = window.publication_clock(&result, now).unwrap().unwrap();
        assert!(deadline >= now + Duration::seconds(30));
        assert_eq!(
            deadline + Duration::seconds(121),
            result.added_items[0].start
        );
        let too_early = SchedulingResult {
            retain_through: Some(now + Duration::seconds(120)),
            ..Default::default()
        };
        assert!(window.publication_clock(&too_early, now).is_err());
    }
}
