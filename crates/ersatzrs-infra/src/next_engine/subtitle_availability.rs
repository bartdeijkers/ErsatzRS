//! Upstream PlayoutItemConverter.SubtitleSource availability, before publication.
use super::{
    invalid, subtitle_input_for_stream, wire, DbPool, OutputFormat, PlaybackSubtitle,
    RepositoryError, Result, StreamingMode,
};

/// Omit unavailable automatic selections, but never silently discard an exact pin.
/// Returns true when the retained selection needs provider preparation.
/// Embedded tracks keep their version-scoped preparation path; legacy extraction
/// metadata is not sufficient evidence to reuse a cached file.
pub(super) async fn retain(
    pool: &DbPool,
    selected: &mut Option<PlaybackSubtitle>,
    source: &wire::Source,
    explicit: bool,
    media_item_id: i64,
    source_pending: bool,
) -> Result<bool> {
    let Some(subtitle) = selected.as_ref() else {
        return Ok(false);
    };
    match availability(pool, subtitle, source, source_pending).await? {
        Availability::Provider => Ok(true),
        Availability::Direct => Ok(false),
        Availability::Unavailable if explicit => Err(invalid("selected subtitle is unavailable")),
        Availability::Unavailable => {
            tracing::warn!(
                subtitle_id = subtitle.id,
                media_item_id,
                "Ignoring unavailable subtitle"
            );
            *selected = None;
            Ok(false)
        }
    }
}

enum Availability {
    Provider,
    Direct,
    Unavailable,
}

async fn availability(
    pool: &DbPool,
    subtitle: &PlaybackSubtitle,
    source: &wire::Source,
    source_pending: bool,
) -> Result<Availability> {
    if subtitle.subtitle_kind == crate::metadata::subtitle_kind::EMBEDDED {
        return Ok(Availability::Direct);
    }
    match crate::provider_subtitles::binding(pool, subtitle.id).await {
        Ok(Some(binding)) => {
            // Validate the persisted identity without network access. Jellyfin
            // can lack a path; Plex/Emby require their provider stream identity.
            return Ok(
                if subtitle.id > 0 && binding.endpoint("http://subtitle.invalid").is_ok() {
                    Availability::Provider
                } else {
                    Availability::Unavailable
                },
            );
        }
        Err(RepositoryError::Decode {
            kind: "provider subtitle",
            ..
        }) => {
            return Ok(Availability::Unavailable);
        }
        Err(error) => return Err(error),
        Ok(None) => {}
    }
    let media_path = match source {
        wire::Source::Local { path, .. } => path,
        wire::Source::Http { uri, .. } | wire::Source::Rtsp { uri, .. } => uri,
        _ => return Err(invalid("subtitle requires a media source")),
    };
    let Some(input) = subtitle_input_for_stream(
        subtitle,
        media_path,
        OutputFormat::MpegTs,
        StreamingMode::HttpLiveStreamingSegmenter,
    ) else {
        return Ok(Availability::Unavailable);
    };
    if crate::playback::subtitle_is_http(&input.path) {
        return Ok(Availability::Direct);
    }
    // Prepared media is selected once for admission and again against its
    // materialized path. Only the latter can check a relative sidecar's bytes.
    if source_pending {
        return Ok(Availability::Direct);
    }
    Ok(
        if tokio::fs::metadata(&input.path)
            .await
            .is_ok_and(|metadata| metadata.is_file())
        {
            Availability::Direct
        } else {
            Availability::Unavailable
        },
    )
}
