//! Automatic local source captions in the existing owned HLS item worker.

use ersatzrs_core::{
    hls::HlsSessionHandle as _, locking::EntityLocker, repositories::RepositoryError,
};
use ersatzrs_domain::{
    Channel, ChannelSubtitleMode, Resolution, SourceTeletextCaptionDelivery, StreamingEngine,
    StreamingMode,
};

use super::{external, ActiveHlsItem, DbPool, HlsSessionWorker, RepositoryResult};
use crate::playback::source_teletext::runtime::{self, RuntimeSelectionContext, Selection};
use crate::repositories::{common::decode_enum_column, SqlxResultExt as _};

pub(super) use crate::playback::source_teletext::runtime::SourceTeletextPlaybackInterval;

pub(super) struct AutomaticCaptionRequest<'a> {
    pub active: &'a ActiveHlsItem,
    pub languages: &'a [String],
    pub mode: ChannelSubtitleMode,
    pub channel: &'a Channel,
    pub resolution: &'a Resolution,
    pub start: i64,
    pub playback_interval: Option<SourceTeletextPlaybackInterval>,
    pub ordinary_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CaptionOccurrence {
    playout_item: i64,
    start: chrono::DateTime<chrono::Utc>,
    item: i64,
    version: i64,
    file: Option<i64>,
}

impl From<&ActiveHlsItem> for CaptionOccurrence {
    fn from(active: &ActiveHlsItem) -> Self {
        Self {
            playout_item: active.playout_item_id,
            start: active.playout_start,
            item: active.media_item_id,
            version: active.media_version_id,
            file: active.media_file_id,
        }
    }
}

impl HlsSessionWorker {
    /// Share the application's existing teletext preparation admission before
    /// starting the worker. Playback itself retains its transcode reservation.
    ///
    /// # Errors
    /// Returns the supplied locker when one was already configured.
    pub fn configure_source_teletext_locker(
        &self,
        locker: EntityLocker,
    ) -> Result<(), EntityLocker> {
        self.source_teletext_locker.set(locker)
    }

    /// Qualify one already-admitted Live producer; never cancel by dropping the
    /// preparation future. Optional observations keep the same ordinary AV owner.
    pub(super) async fn prepare_live_caption_playback(
        &self,
        pool: &DbPool,
        command: &crate::remote_stream::RemoteStreamCommand,
        reservation: &crate::transcode_capacity::ActiveTranscodeReservation,
        slot_id: u64,
        idle: Option<std::time::Duration>,
        watch_idle: Option<std::time::Duration>,
    ) -> Result<Box<crate::remote_stream::PreparedNativeCaptionPlayback>, super::HlsProcessOutcome>
    {
        use crate::remote_stream::{
            prepare_native_caption_producer, RemoteStreamError, RemoteStreamSpawnContext,
        };
        let Some(locker) = self.source_teletext_locker.get() else {
            return Err(super::HlsProcessOutcome::SourceCaptionUnavailable);
        };
        let deadline = external::deadline(self);
        let work = runtime::begin(pool, locker, deadline)
            .await
            .map_err(|_| super::HlsProcessOutcome::SourceCaptionUnavailable)?;
        let hold = work.hold_work();
        let extractor = crate::ffprobe::FfprobeExtractor::new(ersatzrs_ffmpeg::ffprobe_path());
        let (cancel, receiver) = tokio::sync::oneshot::channel();
        let command = command.clone();
        let context = RemoteStreamSpawnContext::hls(&self.channel_number);
        let capacity = reservation.clone();
        let worker_hold = hold.clone();
        // Request cancellation closes the sender, while this existing owned
        // worker keeps admission until preparation and native cleanup settle.
        tracing::debug!(
            channel_id = self.channel_id,
            "preparing owned Live HLS caption input"
        );
        let mut operation = tokio::spawn(async move {
            let _work = work;
            let result =
                prepare_native_caption_producer(&command, &extractor, context, deadline, receiver)
                    .await;
            match result {
                Ok(mut playback) => {
                    if playback
                        .prepared
                        .retain_playback_capacity(capacity.clone())
                        .is_err()
                    {
                        if playback.prepared.cancel().await.is_err() {
                            capacity.retain_until_restart();
                            worker_hold.retain_until_restart();
                        }
                        return Err(RemoteStreamError::PreparedCancelled);
                    }
                    playback.video_input = playback.video_input.without_readrate();
                    Ok(Box::new(playback))
                }
                Err(error) => {
                    if matches!(error, RemoteStreamError::PreparedTerminationUnconfirmed) {
                        worker_hold.retain_until_restart();
                        capacity.retain_until_restart();
                    }
                    Err(error)
                }
            }
        });
        let interrupted = loop {
            tokio::select! {
                biased;
                () = self.cancel.notified() => break super::HlsProcessOutcome::Cancelled,
                () = self.wait_for_idle_timeout(idle) => break super::HlsProcessOutcome::Cancelled,
                () = self.wait_for_idle_timeout(watch_idle) => break super::HlsProcessOutcome::Reclaimed,
                () = self.reclaim_transcode.notified() => {
                    if self.take_reclaim_request(slot_id) { break super::HlsProcessOutcome::Reclaimed; }
                }
                result = &mut operation => {
                    return match result {
                        Ok(Ok(playback)) => {
                            tracing::debug!(channel_id = self.channel_id, probe_available = playback.probe.is_some(),
                                audio_tracks = playback.source_audio.len(), subtitle_choices = playback.tracks().subtitles.len(),
                                "prepared owned Live HLS caption input");
                            Ok(playback)
                        },
                        Ok(Err(_)) => {
                            tracing::warn!(channel_id=self.channel_id, "Live HLS producer preparation failed");
                            Err(super::HlsProcessOutcome::Cancelled)
                        }
                        Err(_) => {
                            hold.retain_until_restart();
                            reservation.retain_until_restart();
                            Err(super::HlsProcessOutcome::Cancelled)
                        }
                    };
                }
            }
        };
        let _ = cancel.send(());
        match operation.await {
            Ok(Ok(playback)) => {
                if playback.prepared.cancel().await.is_err() {
                    hold.retain_until_restart();
                    reservation.retain_until_restart();
                }
            }
            Ok(Err(RemoteStreamError::PreparedTerminationUnconfirmed)) | Err(_) => {
                hold.retain_until_restart();
                reservation.retain_until_restart();
            }
            Ok(Err(_)) => (),
        }
        Err(interrupted)
    }

    pub(super) async fn automatic_source_caption(
        &self,
        pool: &DbPool,
        request: AutomaticCaptionRequest<'_>,
    ) -> RepositoryResult<Selection> {
        let active = request.active;
        let Some(locker) = self
            .source_teletext_locker
            .get()
            .filter(|_| active.external.is_none() && active.remote_stream.is_none())
        else {
            return external::subtitle(
                pool,
                active,
                request.languages,
                request.mode,
                request.channel,
                request.resolution,
                request.start,
            )
            .await
            .map(|subtitle| Selection {
                subtitle,
                source: None,
            });
        };
        // Preserve existing generated music-video credits ahead of all source
        // subtitle preferences, including their established independent policy.
        if let Some(subtitle) = crate::music_video_credits::generate_music_video_credits_subtitle(
            pool,
            active.media_item_id,
            request.channel,
            request.resolution,
            request.start,
        )
        .await?
        {
            return Ok(Selection {
                subtitle: Some(subtitle),
                source: None,
            });
        }
        if request.ordinary_only {
            return ordinary_caption_fallback(pool, &request).await;
        }
        let delivery =
            native_hybrid_caption_delivery(pool, self.channel_id, self.output_format).await?;
        let streaming_mode = if delivery == SourceTeletextCaptionDelivery::DvbSubtitles {
            StreamingMode::TransportStreamHybrid
        } else {
            StreamingMode::HttpLiveStreamingSegmenter
        };
        let selected = runtime::select(RuntimeSelectionContext {
            source_teletext_delivery: delivery,
            pool,
            locker,
            media_item_id: Some(active.media_item_id),
            media_version_id: Some(active.media_version_id),
            media_file_id: active.media_file_id,
            original_path: &active.original_media_path,
            resolved_path: &active.media_path,
            languages: request.languages,
            subtitle_mode: request.mode,
            streaming_mode,
            playback_interval: request.playback_interval,
            deadline: external::deadline(self),
        })
        .await;
        let Ok(selected) = selected else {
            tracing::warn!(
                media_item_id = active.media_item_id,
                "HLS source caption unavailable; retaining ordinary caption preferences"
            );
            return ordinary_caption_fallback(pool, &request).await;
        };
        if let Some(selected) = selected {
            return Ok(selected);
        }
        crate::playback::select_playback_subtitle(
            pool,
            active.media_item_id,
            request.languages,
            request.mode,
            StreamingMode::HttpLiveStreamingSegmenter,
        )
        .await
        .map(|subtitle| Selection {
            subtitle,
            source: None,
        })
    }

    pub(super) fn source_caption_check_deadline(
        &self,
        initial: tokio::time::Instant,
    ) -> tokio::time::Instant {
        caption_check_deadline(
            self.startup_readiness(),
            initial,
            tokio::time::Instant::now(),
        )
    }
}

async fn native_hybrid_caption_delivery(
    pool: &DbPool,
    output_channel: i64,
    output_format: ersatzrs_ffmpeg::OutputFormat,
) -> RepositoryResult<SourceTeletextCaptionDelivery> {
    if output_format != ersatzrs_ffmpeg::OutputFormat::Hls {
        return Ok(SourceTeletextCaptionDelivery::Burn);
    }
    // Runtime stream preferences are a partial Channel, not output policy.
    // This read-only transaction observes the actual output mode and its
    // UUID-bound setting in one snapshot.
    let mut tx = pool.begin().await.repository()?;
    let row =
        sqlx::query(r#"SELECT "StreamingEngine", "StreamingMode" FROM "Channel" WHERE "Id" = ?"#)
            .bind(output_channel)
            .fetch_optional(&mut *tx)
            .await
            .repository()?
            .ok_or(RepositoryError::NotFound {
                kind: "channel",
                id: output_channel,
            })?;
    let engine: StreamingEngine =
        decode_enum_column("channel streaming_engine", &row, "StreamingEngine")?;
    let mode: StreamingMode = decode_enum_column("channel streaming_mode", &row, "StreamingMode")?;
    let delivery = if engine == StreamingEngine::Legacy
        && mode == StreamingMode::TransportStreamHybrid
    {
        let uuid = crate::source_teletext_delivery::channel_uuid(&mut tx, output_channel).await?;
        crate::source_teletext_delivery::load_for_uuid(&mut tx, &uuid).await?
    } else {
        SourceTeletextCaptionDelivery::Burn
    };
    tx.commit().await.repository()?;
    Ok(delivery)
}

fn caption_check_deadline(
    readiness: super::HlsStartupReadiness,
    initial: tokio::time::Instant,
    now: tokio::time::Instant,
) -> tokio::time::Instant {
    let bounded = now + std::time::Duration::from_secs(15);
    match readiness {
        // Initial session startup is never renewed by caption checks.
        super::HlsStartupReadiness::Pending { deadline } => {
            tokio::time::Instant::from_std(deadline).min(bounded)
        }
        super::HlsStartupReadiness::Failed => now,
        super::HlsStartupReadiness::Legacy | super::HlsStartupReadiness::Ready => {
            if initial > now {
                initial.min(bounded)
            } else {
                bounded
            }
        }
    }
}

async fn ordinary_caption_fallback(
    pool: &DbPool,
    request: &AutomaticCaptionRequest<'_>,
) -> RepositoryResult<Selection> {
    let selected = crate::playback::source_teletext::select_native_playback_subtitle_for_version(
        pool,
        request.active.media_item_id,
        request.active.media_version_id,
        request.languages,
        request.mode,
        StreamingMode::HttpLiveStreamingSegmenter,
        &[],
    )
    .await?;
    Ok(Selection {
        subtitle: selected.map(|selected| selected.into_parts().0),
        source: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::Instant;

    #[test]
    fn pending_startup_deadline_is_not_renewed_by_source_rechecks() {
        let now = Instant::now();
        let deadline = now + Duration::from_secs(5);
        assert_eq!(
            caption_check_deadline(
                super::super::HlsStartupReadiness::Pending {
                    deadline: deadline.into_std()
                },
                now - Duration::from_secs(1),
                now,
            ),
            deadline
        );
        let expired = now - Duration::from_secs(1);
        assert_eq!(
            caption_check_deadline(
                super::super::HlsStartupReadiness::Pending {
                    deadline: expired.into_std()
                },
                now + Duration::from_secs(15),
                now,
            ),
            expired
        );
    }

    #[test]
    fn established_worker_can_recheck_for_a_later_retry_after_initial_budget_expires() {
        let now = Instant::now();
        for readiness in [
            super::super::HlsStartupReadiness::Legacy,
            super::super::HlsStartupReadiness::Ready,
        ] {
            assert_eq!(
                caption_check_deadline(readiness, now - Duration::from_secs(1), now),
                now + Duration::from_secs(15)
            );
            assert_eq!(
                caption_check_deadline(readiness, now + Duration::from_secs(5), now),
                now + Duration::from_secs(5)
            );
        }
        assert_eq!(
            caption_check_deadline(
                super::super::HlsStartupReadiness::Failed,
                now + Duration::from_secs(15),
                now
            ),
            now
        );
    }

    async fn output_policy_fixture() -> DbPool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::raw_sql(
            r#"
            CREATE TABLE "Channel" (
                "Id" INTEGER PRIMARY KEY, "UniqueId" TEXT NOT NULL,
                "StreamingEngine" INTEGER NOT NULL, "StreamingMode" INTEGER NOT NULL,
                "MirrorSourceChannelId" INTEGER
            );
            CREATE TABLE "ConfigElement" ("Key" TEXT PRIMARY KEY, "Value" TEXT);
            INSERT INTO "Channel" VALUES
                (1, '00000000-0000-0000-0000-000000000001', 0, 5, NULL),
                (2, '00000000-0000-0000-0000-000000000002', 0, 5, 1),
                (3, '00000000-0000-0000-0000-000000000003', 0, 4, NULL),
                (4, '00000000-0000-0000-0000-000000000004', 1, 5, NULL);
            INSERT INTO "ConfigElement" VALUES
                ('channel.00000000-0000-0000-0000-000000000001.source_caption_delivery', '1'),
                ('channel.00000000-0000-0000-0000-000000000003.source_caption_delivery', '1'),
                ('channel.00000000-0000-0000-0000-000000000004.source_caption_delivery', '1');
        "#,
        )
        .execute(&pool)
        .await
        .unwrap();
        pool
    }

    #[tokio::test]
    async fn hybrid_delivery_observes_actual_output_policy_not_partial_stream_preferences() {
        use ersatzrs_ffmpeg::OutputFormat;
        let pool = output_policy_fixture().await;
        assert_eq!(
            Channel::default().streaming_mode,
            StreamingMode::TransportStream
        );
        assert_eq!(
            native_hybrid_caption_delivery(&pool, 1, OutputFormat::Hls)
                .await
                .unwrap(),
            SourceTeletextCaptionDelivery::DvbSubtitles
        );
        for (channel, format) in [
            (3, OutputFormat::Hls),
            (4, OutputFormat::Hls),
            (1, OutputFormat::HlsMp4),
            (1, OutputFormat::MpegTs),
        ] {
            assert_eq!(
                native_hybrid_caption_delivery(&pool, channel, format)
                    .await
                    .unwrap(),
                SourceTeletextCaptionDelivery::Burn
            );
        }
        sqlx::query(r#"UPDATE "Channel" SET "StreamingMode" = 4 WHERE "Id" = 1"#)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(
            native_hybrid_caption_delivery(&pool, 1, OutputFormat::Hls)
                .await
                .unwrap(),
            SourceTeletextCaptionDelivery::Burn
        );
    }

    #[tokio::test]
    async fn hybrid_delivery_never_inherits_mirrored_source_setting_and_rejects_invalid_policy() {
        use ersatzrs_ffmpeg::OutputFormat;
        let pool = output_policy_fixture().await;
        assert_eq!(
            native_hybrid_caption_delivery(&pool, 2, OutputFormat::Hls)
                .await
                .unwrap(),
            SourceTeletextCaptionDelivery::Burn
        );
        sqlx::query(
            r#"INSERT INTO "ConfigElement" VALUES
            ('channel.00000000-0000-0000-0000-000000000002.source_caption_delivery', '1')"#,
        )
        .execute(&pool)
        .await
        .unwrap();
        assert_eq!(
            native_hybrid_caption_delivery(&pool, 2, OutputFormat::Hls)
                .await
                .unwrap(),
            SourceTeletextCaptionDelivery::DvbSubtitles
        );
        sqlx::query(
            r#"UPDATE "ConfigElement" SET "Value" = '0' WHERE "Key" =
            'channel.00000000-0000-0000-0000-000000000001.source_caption_delivery'"#,
        )
        .execute(&pool)
        .await
        .unwrap();
        assert_eq!(
            native_hybrid_caption_delivery(&pool, 2, OutputFormat::Hls)
                .await
                .unwrap(),
            SourceTeletextCaptionDelivery::DvbSubtitles
        );
        sqlx::query(r#"UPDATE "Channel" SET "StreamingMode" = 99 WHERE "Id" = 2"#)
            .execute(&pool)
            .await
            .unwrap();
        assert!(native_hybrid_caption_delivery(&pool, 2, OutputFormat::Hls)
            .await
            .is_err());
    }
}
