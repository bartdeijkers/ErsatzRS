//! Shared native playback bindings for external JSON programmes.
use crate::{
    db::DbPool,
    external_json::{playback as external, subtitles::PreparedSubtitle},
    playback::{
        subtitle_input_for_stream, transient::TrackSelectionPolicy, PlaybackSubtitle,
        SubtitleSelectionEngine,
    },
};
use ersatzrs_domain::StreamingMode;
use ersatzrs_ffmpeg::{InputFile, OutputFormat};

/// Read native subtitle eligibility settings.
///
/// # Errors
/// Returns global configuration read failures.
pub async fn policy(
    pool: &DbPool,
    mode: StreamingMode,
    media: &external::ResolvedExternalMedia,
) -> ersatzrs_core::repositories::Result<TrackSelectionPolicy> {
    Ok(TrackSelectionPolicy {
        mode,
        engine: SubtitleSelectionEngine::Native,
        media_server: matches!(media.source, external::ExternalMediaSource::Plex { .. }),
        prepare_embedded_text: crate::ffmpeg_config::use_embedded_subtitles(pool).await?
            && crate::ffmpeg_config::extract_embedded_subtitles(pool).await?,
    })
}

/// Prepare transient native subtitles while retaining temporary-file ownership.
///
/// # Errors
/// Returns bounded extraction or sidecar download failures.
pub async fn prepare_subtitle(
    pool: &DbPool,
    media: &external::ResolvedExternalMedia,
    path: &str,
    selected: PlaybackSubtitle,
    mode: StreamingMode,
    deadline: tokio::time::Instant,
) -> Result<
    (PlaybackSubtitle, Option<PreparedSubtitle>),
    crate::external_json::subtitles::PreparationError,
> {
    use crate::{metadata::subtitle_kind, next_engine::protocol::Source};
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

/// Bind inherited decorations to the external programme playout.
#[must_use]
pub fn decoration_item(media: &external::ResolvedExternalMedia) -> ersatzrs_domain::PlayoutItem {
    ersatzrs_domain::PlayoutItem {
        playout_id: media.occurrence.playout_id,
        ..Default::default()
    }
}

/// Map transient sidecars by input stream index rather than provider stream ID.
#[must_use]
pub fn subtitle_input(
    subtitle: &PlaybackSubtitle,
    media_path: &str,
    format: OutputFormat,
    mode: StreamingMode,
) -> Option<InputFile> {
    if mode != StreamingMode::HttpLiveStreamingDirect
        && subtitle.is_image()
        && subtitle.subtitle_kind == crate::metadata::subtitle_kind::SIDECAR
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
