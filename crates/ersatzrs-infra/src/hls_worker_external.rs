//! External programmes remain transient throughout the native HLS worker.
use super::{
    ffprobe_path, load_active_hls_item, select_channel_custom_streams,
    select_channel_playback_subtitle, ActiveHlsItem, Arc, Channel, ChannelStreamSelectorMode,
    ChannelSubtitleMode, CustomStreamSelection, DateTime, DbPool, Duration, HlsSessionHandle,
    HlsSessionWorker, HlsStartupReadiness, PathReplacement, PlaybackSubtitle, RepositoryError,
    RepositoryResult, StreamingMode, Utc,
};
use crate::external_json::{native, playback as media};
use ersatzrs_core::repositories::ChannelRepository as _;

impl HlsSessionWorker {
    /// Share the host's private media relay before starting this worker.
    ///
    /// # Errors
    /// Rejects replacing an already configured relay.
    pub fn configure_media_relay(
        &self,
        relay: Arc<crate::next_engine::media::MediaRelay>,
    ) -> RepositoryResult<()> {
        self.media_relay
            .set(relay)
            .map_err(|_| failure("HLS media relay already configured"))
    }
}

fn failure(message: impl std::fmt::Display) -> RepositoryError {
    tracing::warn!(reason = %message, "external HLS programme unavailable");
    RepositoryError::Unavailable {
        operation: "external JSON playback",
        message: message.to_string(),
    }
}

fn selection_failure(channel_id: i64, error: media::ExternalPlaybackError) -> RepositoryError {
    if matches!(
        error,
        media::ExternalPlaybackError::Schedule(crate::external_json::ExternalJsonError::StartTime)
    ) {
        tracing::error!(
            channel_id,
            "external JSON schedule has an invalid start time"
        );
    }
    failure(error)
}

pub(super) fn deadline(worker: &HlsSessionWorker) -> tokio::time::Instant {
    let limit = tokio::time::Instant::now() + Duration::from_secs(20);
    match worker.startup_readiness() {
        HlsStartupReadiness::Pending { deadline } => limit.min(deadline.into()),
        _ => limit,
    }
}

/// Classify admission without probing or opening external media.
pub(super) async fn retained(
    pool: &DbPool,
    channel_id: i64,
    replacements: &[PathReplacement],
    now: DateTime<Utc>,
) -> RepositoryResult<Option<bool>> {
    match media::select(pool, channel_id, now)
        .await
        .map_err(|error| selection_failure(channel_id, error))?
    {
        media::ExternalPlayoutSelection::Item(_) => Ok(Some(false)),
        media::ExternalPlayoutSelection::Unavailable => Ok(None),
        media::ExternalPlayoutSelection::NotConfigured => {
            Ok(load_active_hls_item(pool, channel_id, replacements, now)
                .await?
                .map(|active| {
                    active.remote_stream.is_some_and(|stream| {
                        matches!(
                            stream.source,
                            crate::remote_stream::RemoteStreamSource::RetainedIntervals(_)
                        )
                    })
                }))
        }
    }
}

pub(super) async fn load(
    worker: &HlsSessionWorker,
    pool: &DbPool,
    channel_id: i64,
    replacements: &[PathReplacement],
    now: DateTime<Utc>,
) -> RepositoryResult<Option<ActiveHlsItem>> {
    let occurrence = match media::select(pool, channel_id, now)
        .await
        .map_err(|error| selection_failure(channel_id, error))?
    {
        media::ExternalPlayoutSelection::NotConfigured => {
            return load_active_hls_item(pool, channel_id, replacements, now).await;
        }
        media::ExternalPlayoutSelection::Unavailable => return Ok(None),
        media::ExternalPlayoutSelection::Item(item) => *item,
    };
    let channel = crate::repositories::make_repositories(pool.clone())
        .channel
        .get_with_profile(channel_id)
        .await?;
    let pool = pool.clone();
    let deadline = deadline(worker);
    let (cancel, cancellation) = tokio::sync::oneshot::channel();
    // The owned resolver observes cancellation if startup or worker shutdown
    // drops this await; its FFprobe supervisor remains responsible for cleanup.
    let task = tokio::spawn(async move {
        media::resolve(
            &pool,
            occurrence,
            &crate::ffprobe::FfprobeExtractor::new(ffprobe_path()),
            deadline,
            cancellation,
        )
        .await
    });
    let resolved = task
        .await
        .map_err(|_| failure("external preparation worker failed"))?
        .map_err(failure)?;
    drop(cancel);
    let path = match &resolved.source {
        media::ExternalMediaSource::Local(path) => path.clone(),
        media::ExternalMediaSource::Plex {
            media_source_id,
            part_key,
        } => {
            let expires = resolved
                .occurrence
                .finish
                .checked_add_signed(chrono::Duration::hours(2))
                .ok_or_else(|| failure("external media lifetime overflow"))?;
            worker
                .media_relay
                .get()
                .ok_or_else(|| failure("HLS media relay unavailable"))?
                .external_source_url(*media_source_id, part_key, expires)?
        }
    };
    Ok(Some(ActiveHlsItem {
        original_media_path: path.clone(),
        media_path: path,
        playout_item_id: 0,
        media_item_id: 0,
        media_version_id: 0,
        media_file_id: None,
        content_duration: resolved.probe.version.duration.to_std().ok(),
        in_point: Duration::ZERO,
        program_duration: (resolved.occurrence.finish - resolved.occurrence.start)
            .to_std()
            .ok(),
        playout_start: resolved.occurrence.start,
        playout_finish: Some(resolved.occurrence.finish),
        disable_watermarks: false,
        preferred_audio_language_code: None,
        preferred_audio_title: None,
        preferred_subtitle_language_code: None,
        subtitle_mode: None,
        has_pinned_stream_selection: false,
        video_stream_index: None,
        audio_stream_index: None,
        subtitle_stream_index: None,
        channel_name: channel.name,
        channel_preferred_audio_language_code: channel.preferred_audio_language_code,
        channel_preferred_audio_title: channel.preferred_audio_title,
        channel_stream_selector_mode: channel.stream_selector_mode,
        channel_stream_selector: channel.stream_selector,
        channel_preferred_subtitle_language_code: channel.preferred_subtitle_language_code,
        channel_subtitle_mode: channel.subtitle_mode,
        channel_watermark_id: channel.watermark_id,
        channel_music_video_credits_mode: channel.music_video_credits_mode,
        channel_music_video_credits_template: channel.music_video_credits_template,
        remote_stream: None,
        external: Some(Arc::new(resolved)),
    }))
}

pub(super) async fn custom(
    pool: &DbPool,
    active: &ActiveHlsItem,
    channel: &Channel,
) -> RepositoryResult<Option<CustomStreamSelection>> {
    if let Some(media) = &active.external {
        if channel.stream_selector_mode != ChannelStreamSelectorMode::Custom {
            return Ok(None);
        }
        let policy = native::policy(pool, StreamingMode::HttpLiveStreamingSegmenter, media).await?;
        media
            .tracks()
            .select_custom(channel, active.playout_start, policy)
            .await
    } else {
        select_channel_custom_streams(
            pool,
            active.media_version_id,
            active.media_item_id,
            channel,
            active.playout_start,
            StreamingMode::HttpLiveStreamingSegmenter,
        )
        .await
    }
}

pub(super) async fn subtitle(
    pool: &DbPool,
    active: &ActiveHlsItem,
    languages: &[String],
    mode: ChannelSubtitleMode,
    channel: &Channel,
    resolution: &ersatzrs_domain::Resolution,
    start: i64,
) -> RepositoryResult<Option<PlaybackSubtitle>> {
    if let Some(media) = &active.external {
        let policy = native::policy(pool, StreamingMode::HttpLiveStreamingSegmenter, media).await?;
        media
            .tracks()
            .select_subtitle(pool, languages, mode, policy)
            .await
    } else {
        select_channel_playback_subtitle(
            pool,
            active.media_item_id,
            languages,
            mode,
            StreamingMode::HttpLiveStreamingSegmenter,
            channel,
            resolution,
            start,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::retained;
    use sqlx::Executor as _;

    #[tokio::test]
    async fn external_hls_admission_needs_no_provider_or_media_tables() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("schedule.json");
        let now = chrono::Utc::now();
        std::fs::write(
            &path,
            serde_json::to_vec(&serde_json::json!({
                "startTime": now.to_rfc3339(), "programs": [{
                    "serverKey": "Absent provider", "ratingKey": "42", "duration": 60000
                }]
            }))
            .unwrap(),
        )
        .unwrap();
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        pool.execute(r#"CREATE TABLE "Channel" ("Id" INTEGER, "PlayoutSource" INTEGER, "MirrorSourceChannelId" INTEGER, "PlayoutOffset" TEXT);
            CREATE TABLE "Playout" ("Id" INTEGER, "ChannelId" INTEGER, "ScheduleKind" INTEGER, "ScheduleFile" TEXT);
            INSERT INTO "Channel" VALUES (1, 0, NULL, NULL);"#).await.unwrap();
        // Timeline admission also consults accepted planning; this remains a
        // scheduling-only fixture without any provider or media tables.
        pool.execute(include_str!(
            "../../../migrations/202610041537_external_json_accepted_snapshot.sql"
        ))
        .await
        .unwrap();
        sqlx::query(r#"INSERT INTO "Playout" VALUES (1, 1, 20, ?)"#)
            .bind(path.to_str().unwrap())
            .execute(&pool)
            .await
            .unwrap();
        // Resolving this source would require missing provider tables. Admission
        // must only classify the timeline, even when no slot is available.
        assert_eq!(retained(&pool, 1, &[], now).await.unwrap(), Some(false));
        assert_eq!(
            retained(&pool, 1, &[], now + chrono::Duration::seconds(61))
                .await
                .unwrap(),
            None
        );
    }
}
