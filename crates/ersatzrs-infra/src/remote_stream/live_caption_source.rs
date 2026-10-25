//! Shared Native Live source admission and observed-track policy (ADR 0121).

use ersatzrs_core::repositories::{RepositoryError, Result};
use ersatzrs_domain::{RemoteStreamLiveness, SourceTeletextCaptionDelivery, StreamingMode};

use super::{
    PreparedNativeCaptionPlayback, RemoteStreamCommand, RemoteStreamSource, ResolvedRemoteStream,
};
use crate::{
    db::DbPool,
    playback::{
        transient::{TrackSelectionPolicy, TransientPlaybackTracks},
        PlaybackSubtitle, SubtitleSelectionEngine,
    },
};

/// Admit the existing Live producer without granting page readiness or opening it.
/// # Errors
/// Returns configuration/inventory errors before producer startup.
pub async fn native_live_caption_command(
    pool: &DbPool,
    item: i64,
    version: i64,
    stream: Option<&ResolvedRemoteStream>,
) -> Result<Option<RemoteStreamCommand>> {
    let Some(stream) = stream.filter(|stream| stream.liveness == RemoteStreamLiveness::Live) else {
        return Ok(None);
    };
    match &stream.source {
        RemoteStreamSource::Addon(command) => {
            if !crate::ffmpeg_config::use_embedded_subtitles(pool).await? {
                return Ok(None);
            }
            Ok(Some(
                command
                    .with_playback_seek(RemoteStreamLiveness::Live, std::time::Duration::ZERO)
                    .0,
            ))
        }
        RemoteStreamSource::Url(_) => {
            super::native_live_url_caption_producer(pool, item, version, stream).await
        }
        _ => Ok(None),
    }
}

/// Keep the same observed bitmap/teletext choices on MPEG-TS and HLS.
/// # Errors
/// Returns the existing channel delivery-policy error without a new producer.
pub async fn native_live_caption_tracks(
    playback: &PreparedNativeCaptionPlayback,
    pool: &DbPool,
    channel_id: i64,
) -> Result<TransientPlaybackTracks> {
    let mut tracks = playback.tracks();
    if !tracks
        .subtitles
        .iter()
        .any(|subtitle| subtitle.codec.as_deref() == Some("dvb_teletext"))
    {
        tracing::warn!(channel_id, "live source teletext caption evidence is unavailable; retaining the same producer for ordinary AV playback");
    }
    let delivery =
        crate::source_teletext_delivery::load_channel_source_teletext_delivery(pool, channel_id)
            .await?;
    if delivery == SourceTeletextCaptionDelivery::DvbSubtitles {
        let before = tracks.subtitles.len();
        tracks
            .subtitles
            .retain(|subtitle| subtitle.codec.as_deref() != Some("dvb_teletext"));
        if before != tracks.subtitles.len() {
            tracing::warn!(channel_id, "live source teletext bitmap carriage is unavailable; retaining ordinary playback candidates without converting to Burn");
        }
    }
    Ok(tracks)
}

/// Raw stream pins cannot resolve multiple qualified pages on one stream.
/// # Errors
/// Rejects an absent or ambiguous observed candidate.
pub fn pinned_native_caption_subtitle(
    tracks: &TransientPlaybackTracks,
    index: i32,
) -> Result<PlaybackSubtitle> {
    let mut candidates = tracks
        .subtitles
        .iter()
        .filter(|subtitle| subtitle.stream_index == index);
    match (candidates.next(), candidates.next()) {
        (Some(subtitle), None) => Ok(subtitle.clone()),
        _ => Err(RepositoryError::Unavailable {
            operation: "live source subtitle selection",
            message: "pinned track is absent or ambiguous in the observed producer".into(),
        }),
    }
}

/// Only caller-qualified Native bitmap choices reach this ordinary selector.
#[must_use]
pub const fn native_live_caption_policy(mode: StreamingMode) -> TrackSelectionPolicy {
    TrackSelectionPolicy {
        mode,
        engine: SubtitleSelectionEngine::Native,
        media_server: false,
        prepare_embedded_text: true,
    }
}
