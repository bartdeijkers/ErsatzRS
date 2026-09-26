//! On-demand progress belongs to viewer demand, not transcode-ahead or heartbeat.

use std::{
    convert::Infallible,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

use chrono::{DateTime, Utc};
use ersatzrs_core::locking::{EntityLocker, PlayoutLockGuard};
use tokio::time::{interval_at, timeout, Instant, MissedTickBehavior};

use super::{process::ProcessError, session::NextSession};
use crate::db::DbPool;

const CHECKPOINT_INTERVAL: Duration = Duration::from_mins(5);
const WRITE_BUDGET: Duration = Duration::from_secs(5);

pub(super) struct Checkpoints {
    pool: DbPool,
    locker: EntityLocker,
    channel_id: i64,
    preview: bool,
    active: AtomicBool,
}

impl Checkpoints {
    pub(super) fn new(pool: DbPool, locker: EntityLocker, channel_id: i64, preview: bool) -> Self {
        Self {
            pool,
            locker,
            channel_id,
            preview,
            active: AtomicBool::new(false),
        }
    }

    /// The caller's startup deadline/cancellation bounds lock acquisition.
    pub(super) async fn prepare(&self) -> Result<(), ProcessError> {
        if self.preview {
            return Ok(());
        }
        let Some(_guard) = self
            .lock()
            .await
            .map_err(|_| ProcessError::Preparation("on-demand lookup"))?
        else {
            return Ok(());
        };
        crate::hls_worker::time_shift_on_demand_playout(&self.pool, self.channel_id, Utc::now())
            .await
            .map_err(|_| ProcessError::Preparation("on-demand time shift"))?;
        self.active.store(true, Ordering::Release);
        Ok(())
    }

    pub(super) async fn run(&self, session: &NextSession) -> Infallible {
        let mut ticks = interval_at(Instant::now() + CHECKPOINT_INTERVAL, CHECKPOINT_INTERVAL);
        ticks.set_missed_tick_behavior(MissedTickBehavior::Skip);
        let mut written = None;
        loop {
            ticks.tick().await;
            if let Some(touch) = session
                .checkpoint_time()
                .filter(|touch| Some(*touch) > written)
            {
                if self.save(touch).await {
                    written = Some(touch);
                }
            }
        }
    }

    pub(super) async fn finish(&self, session: &NextSession) {
        if let Some(touch) = session.checkpoint_time() {
            self.save(touch).await;
        }
    }

    async fn save(&self, touch: DateTime<Utc>) -> bool {
        if !self.active.load(Ordering::Acquire) {
            return false;
        }
        let write = async {
            let Some(_guard) = self.lock().await? else {
                return Ok::<_, sqlx::Error>(());
            };
            self.write_viewer_checkpoint(touch).await
        };
        if matches!(timeout(WRITE_BUDGET, write).await, Ok(Ok(()))) {
            return true;
        }
        tracing::warn!(
            channel_id = self.channel_id,
            "Next on-demand checkpoint could not be saved"
        );
        false
    }

    async fn write_viewer_checkpoint(&self, touch: DateTime<Utc>) -> Result<(), sqlx::Error> {
        // Upstream UpdateOnDemandCheckpoint preserves the configured segmenter
        // safety interval and never rewinds or checkpoints before the first item.
        let configured: Option<String> =
            sqlx::query_scalar(r#"SELECT "Value" FROM "ConfigElement" WHERE "Key"=?"#)
                .bind(ersatzrs_domain::CONFIG_KEY_FFMPEG_SEGMENTER_TIMEOUT)
                .fetch_optional(&self.pool)
                .await?;
        let seconds = configured
            .as_deref()
            .unwrap_or("60")
            .parse::<i64>()
            .ok()
            .filter(|seconds| *seconds >= 0)
            .and_then(chrono::Duration::try_seconds)
            .ok_or_else(|| sqlx::Error::Protocol("invalid on-demand checkpoint interval".into()))?;
        let cutoff = touch
            .checked_sub_signed(seconds)
            .ok_or_else(|| sqlx::Error::Protocol("invalid on-demand checkpoint time".into()))?;
        sqlx::query(
            r#"UPDATE "Playout" SET "OnDemandCheckpoint" = MAX(
            COALESCE("OnDemandCheckpoint", ?1), ?1,
            COALESCE((SELECT MIN("Start") FROM "PlayoutItem" WHERE "PlayoutId"="Playout"."Id"), ?1))
            WHERE "ChannelId"=?2 AND EXISTS (
                SELECT 1 FROM "Channel" WHERE "Id"="Playout"."ChannelId" AND "PlayoutMode"=1)"#,
        )
        .bind(crate::repositories::common::encode_datetime(cutoff))
        .bind(self.channel_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn lock(&self) -> Result<Option<PlayoutLockGuard>, sqlx::Error> {
        let id: Option<i64> = sqlx::query_scalar(r#"SELECT p."Id" FROM "Playout" p JOIN "Channel" c ON c."Id"=p."ChannelId" WHERE c."Id"=? AND c."PlayoutMode"=1 ORDER BY p."Id" LIMIT 1"#)
            .bind(self.channel_id).fetch_optional(&self.pool).await?;
        let Some(id) = id else {
            return Ok(None);
        };
        loop {
            if let Some(guard) = PlayoutLockGuard::try_acquire(&self.locker, id) {
                return Ok(Some(guard));
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }
}

#[cfg(test)]
#[path = "on_demand_tests.rs"]
mod tests;
