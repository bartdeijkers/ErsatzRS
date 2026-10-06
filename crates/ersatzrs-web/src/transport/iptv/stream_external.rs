//! Native external JSON bindings. Ephemeral IDs never reach media-row queries.

use super::{
    encode_sql_datetime, load_active_media_path, ActivePlayoutItem, AppState, Arc, DateTime,
    Duration, RepositoryError, Utc,
};
use ersatzrs_infra::external_json::playback as external;

fn failure(error: impl std::fmt::Display) -> RepositoryError {
    RepositoryError::Unavailable {
        operation: "external JSON playback",
        message: error.to_string(),
    }
}

pub(super) async fn load(
    state: &AppState,
    channel_id: i64,
    now: DateTime<Utc>,
    deadline: tokio::time::Instant,
) -> ersatzrs_core::repositories::Result<Option<ActivePlayoutItem>> {
    let occurrence = match external::select(&state.pool, channel_id, now)
        .await
        .map_err(|error| {
            if matches!(
                error,
                external::ExternalPlaybackError::Schedule(
                    ersatzrs_infra::external_json::ExternalJsonError::StartTime
                )
            ) {
                tracing::error!(
                    channel_id,
                    "external JSON schedule has an invalid start time"
                );
            }
            failure(error)
        })? {
        external::ExternalPlayoutSelection::NotConfigured => {
            return load_active_media_path(&state.pool, channel_id, now).await
        }
        external::ExternalPlayoutSelection::Unavailable => return Ok(None),
        external::ExternalPlayoutSelection::Item(item) => *item,
    };
    let pool = state.pool.clone();
    let (cancel, cancellation) = tokio::sync::oneshot::channel();
    let worker = tokio::spawn(async move {
        external::resolve(
            &pool,
            occurrence,
            &ersatzrs_infra::ffprobe::FfprobeExtractor::new(ersatzrs_ffmpeg::ffprobe_path()),
            deadline,
            cancellation,
        )
        .await
    });
    let media = worker
        .await
        .map_err(|_| failure("external preparation worker failed"))?
        .map_err(failure)?;
    drop(cancel);
    let path = match &media.source {
        external::ExternalMediaSource::Local(path) => path.clone(),
        external::ExternalMediaSource::Plex {
            media_source_id,
            part_key,
        } => {
            let expires = media
                .occurrence
                .finish
                .checked_add_signed(chrono::Duration::hours(2))
                .ok_or_else(|| failure("external media lifetime overflow"))?;
            state
                .next_engine
                .media
                .external_source_url(*media_source_id, part_key, expires)?
        }
    };
    Ok(Some(ActivePlayoutItem {
        playout_item_id: 0,
        media_item_id: 0,
        media_version_id: 0,
        media_path: path,
        content_duration: media.probe.version.duration.to_std().ok(),
        in_point: Duration::ZERO,
        out_point: Duration::ZERO,
        playout_start: encode_sql_datetime(media.occurrence.start),
        playout_finish: encode_sql_datetime(media.occurrence.finish),
        disable_watermarks: false,
        preferred_audio_language_code: None,
        preferred_audio_title: None,
        preferred_subtitle_language_code: None,
        subtitle_mode: None,
        has_pinned_stream_selection: false,
        video_stream_index: None,
        audio_stream_index: None,
        subtitle_stream_index: None,
        remote_stream: None,
        external: Some(Arc::new(media)),
    }))
}

pub(super) use ersatzrs_infra::external_json::native::{
    decoration_item, policy, prepare_subtitle, subtitle_input,
};

#[cfg(test)]
#[path = "stream_external_tests.rs"]
mod tests;
