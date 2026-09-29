//! Native external JSON bindings. Ephemeral IDs never reach media-row queries.

use super::{
    encode_sql_datetime, load_active_media_path, subtitle_input_for_stream, ActivePlayoutItem,
    AppState, Arc, DateTime, DbPool, Duration, InputFile, OutputFormat, PlaybackSubtitle,
    RepositoryError, StreamingMode, Utc,
};
use ersatzrs_infra::external_json::{playback as external, subtitles::PreparedSubtitle};
use ersatzrs_infra::playback::transient::TrackSelectionPolicy;
use ersatzrs_infra::playback::SubtitleSelectionEngine;

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

pub(super) async fn policy(
    pool: &DbPool,
    mode: StreamingMode,
    media: &external::ResolvedExternalMedia,
) -> ersatzrs_core::repositories::Result<TrackSelectionPolicy> {
    Ok(TrackSelectionPolicy {
        mode,
        engine: SubtitleSelectionEngine::Native,
        media_server: matches!(media.source, external::ExternalMediaSource::Plex { .. }),
        prepare_embedded_text: ersatzrs_infra::ffmpeg_config::use_embedded_subtitles(pool).await?
            && ersatzrs_infra::ffmpeg_config::extract_embedded_subtitles(pool).await?,
    })
}

pub(super) async fn prepare_subtitle(
    pool: &DbPool,
    media: &external::ResolvedExternalMedia,
    path: &str,
    selected: PlaybackSubtitle,
    mode: StreamingMode,
    deadline: tokio::time::Instant,
) -> Result<
    (PlaybackSubtitle, Option<PreparedSubtitle>),
    ersatzrs_infra::external_json::subtitles::PreparationError,
> {
    use ersatzrs_infra::{metadata::subtitle_kind, next_engine::protocol::Source};
    let artifact = if selected.subtitle_kind == subtitle_kind::SIDECAR {
        let external::ExternalMediaSource::Plex {
            media_source_id, ..
        } = &media.source
        else {
            return Ok((selected, None));
        };
        PreparedSubtitle::fetch_plex(pool, *media_source_id, selected, deadline).await?
    } else if !selected.is_image() && mode != StreamingMode::HttpLiveStreamingDirect {
        let source = match &media.source {
            external::ExternalMediaSource::Local(_) => Source::Local {
                path: path.into(),
                probe_hint: None,
                in_point_ms: None,
                out_point_ms: None,
            },
            external::ExternalMediaSource::Plex { .. } => Source::Http {
                uri: path.into(),
                probe_hint: None,
                is_live: Some(false),
                headers: Vec::new(),
                user_agent: None,
                keep_alive: None,
                reconnect: Some(true),
                reconnect_delay_max: Some(2),
                timeout_us: Some(15_000_000),
                in_point_ms: None,
                out_point_ms: None,
            },
        };
        PreparedSubtitle::extract(
            source,
            selected,
            ersatzrs_ffmpeg::ffmpeg_path().into(),
            deadline,
        )
        .await?
    } else {
        return Ok((selected, None));
    };
    Ok((artifact.subtitle().clone(), Some(artifact)))
}

pub(super) fn decoration_item(
    media: &external::ResolvedExternalMedia,
) -> ersatzrs_domain::PlayoutItem {
    ersatzrs_domain::PlayoutItem {
        playout_id: media.occurrence.playout_id,
        ..Default::default()
    }
}

pub(super) fn subtitle_input(
    subtitle: &PlaybackSubtitle,
    media_path: &str,
    format: OutputFormat,
    mode: StreamingMode,
) -> Option<InputFile> {
    if mode != StreamingMode::HttpLiveStreamingDirect
        && subtitle.is_image()
        && subtitle.subtitle_kind == ersatzrs_infra::metadata::subtitle_kind::SIDECAR
    {
        let path = subtitle.path.as_ref()?;
        let mut input =
            InputFile::image_subtitle(path.clone(), ersatzrs_ffmpeg::SubtitleMethod::Burn)
                .with_stream_index(0);
        if let Some(codec) = &subtitle.codec {
            input = input.with_subtitle_codec(codec);
        }
        Some(input)
    } else {
        subtitle_input_for_stream(subtitle, media_path, format, mode)
    }
}

#[cfg(test)]
#[path = "stream_external_tests.rs"]
mod tests;
