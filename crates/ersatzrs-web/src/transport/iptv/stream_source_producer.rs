//! One admitted producer owner for retained intervals or observed live add-on input.

use ersatzrs_core::repositories::Result;
use ersatzrs_domain::{RemoteStreamLiveness, SourceTeletextCaptionDelivery};
use ersatzrs_ffmpeg::{FfmpegCommand, InputFile, SourceTeletextInputPlan};
use ersatzrs_infra::{
    db::DbPool,
    playback::{transient::TransientPlaybackTracks, PlaybackSubtitle},
    published_interval_source::RetainedIntervalPlayback,
    remote_stream::{
        PreparedNativeCaptionPlayback, PreparedRemoteStreamInput, RemoteStreamCommand,
        RemoteStreamError, RemoteStreamSource, ResolvedRemoteStream,
    },
};

pub(super) fn live_addon(stream: Option<&ResolvedRemoteStream>) -> Option<RemoteStreamCommand> {
    let stream = stream.filter(|stream| stream.liveness == RemoteStreamLiveness::Live)?;
    match &stream.source {
        RemoteStreamSource::Addon(command) => Some(
            command
                .with_playback_seek(RemoteStreamLiveness::Live, std::time::Duration::ZERO)
                .0,
        ),
        _ => None,
    }
}

pub(super) enum PreparedSourcePlayback {
    Retained(Box<RetainedIntervalPlayback>),
    Streaming(Box<PreparedNativeCaptionPlayback>),
}
impl PreparedSourcePlayback {
    pub(super) fn into_prepared(self) -> PreparedRemoteStreamInput {
        match self {
            Self::Retained(playback) => playback.prepared,
            Self::Streaming(playback) => playback.prepared,
        }
    }
    pub(super) fn video_input(&self) -> &InputFile {
        match self {
            Self::Retained(playback) => &playback.video_input,
            Self::Streaming(playback) => &playback.video_input,
        }
    }
    pub(super) const fn is_retained(&self) -> bool {
        matches!(self, Self::Retained(_))
    }
    pub(super) const fn is_streaming(&self) -> bool {
        matches!(self, Self::Streaming(_))
    }
    pub(super) fn audio_complete(&self) -> bool {
        match self {
            Self::Retained(_) => true,
            Self::Streaming(playback) => playback.probe.is_some(),
        }
    }
    pub(super) async fn streaming_tracks(
        &self,
        pool: &DbPool,
        channel_id: i64,
    ) -> Result<Option<TransientPlaybackTracks>> {
        let Self::Streaming(playback) = self else {
            return Ok(None);
        };
        let mut tracks = playback.tracks();
        if !tracks
            .subtitles
            .iter()
            .any(|subtitle| subtitle.codec.as_deref() == Some("dvb_teletext"))
        {
            tracing::warn!(channel_id, "live source teletext caption evidence is unavailable; retaining the same producer for ordinary AV playback");
        }

        let delivery =
            ersatzrs_infra::source_teletext_delivery::load_channel_source_teletext_delivery(
                pool, channel_id,
            )
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
        Ok(Some(tracks))
    }
    pub(super) fn subtitle_input(
        &mut self,
        subtitle: &PlaybackSubtitle,
    ) -> std::result::Result<Option<InputFile>, RemoteStreamError> {
        match self {
            Self::Streaming(playback) => playback.subtitle_input(subtitle),
            Self::Retained(_) => Ok(None),
        }
    }
    pub(super) fn bind_caption_consumer(
        &mut self,
        command: &FfmpegCommand,
        plan: Option<&SourceTeletextInputPlan>,
    ) -> std::result::Result<(), RemoteStreamError> {
        match self {
            Self::Streaming(playback) => playback.bind_caption_consumer(command, plan),
            Self::Retained(_) => Ok(()),
        }
    }
}

pub(super) fn selection_policy(
    mode: ersatzrs_domain::StreamingMode,
) -> ersatzrs_infra::playback::transient::TrackSelectionPolicy {
    ersatzrs_infra::playback::transient::TrackSelectionPolicy {
        mode,
        engine: ersatzrs_infra::playback::SubtitleSelectionEngine::Native,
        media_server: false,
        // The producer has already removed raw text and unqualified teletext.
        prepare_embedded_text: true,
    }
}

/// A raw stream-index pin cannot choose between multiple qualified page rows.
pub(super) fn pinned_subtitle(
    tracks: &TransientPlaybackTracks,
    stream_index: i32,
) -> Result<PlaybackSubtitle> {
    let mut candidates = tracks
        .subtitles
        .iter()
        .filter(|subtitle| subtitle.stream_index == stream_index);
    match (candidates.next(), candidates.next()) {
        (Some(subtitle), None) => Ok(subtitle.clone()),
        _ => Err(ersatzrs_core::repositories::RepositoryError::Unavailable {
            operation: "live source subtitle selection",
            message: "pinned track is absent or ambiguous in the observed producer".into(),
        }),
    }
}

/// Optional caption observation must not discard ordinary preferred audio.
/// A verified empty inventory, unlike a failed probe, replaces the old tracks.
pub(super) fn observed_audio_or_fallback(
    ordinary: &[ersatzrs_ffmpeg::SourceAudioStream],
    observed: &[ersatzrs_ffmpeg::SourceAudioStream],
    probe_available: bool,
) -> Vec<ersatzrs_ffmpeg::SourceAudioStream> {
    if probe_available {
        observed.to_vec()
    } else {
        ordinary.to_vec()
    }
}

/// Unknown probe inventory cannot authorize an absent-audio decision.
pub(super) fn audio_selection(
    selection: ersatzrs_infra::playback::PlaybackAudioSelection,
    audio: &[ersatzrs_ffmpeg::SourceAudioStream],
    complete: bool,
) -> ersatzrs_infra::playback::PlaybackAudioSelection {
    if complete {
        super::retained_audio_selection(selection, audio).with_complete_source_audio(audio)
    } else {
        selection
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ersatzrs_infra::playback::PlaybackAudioSelection;
    #[test]
    fn unknown_audio_inventory_preserves_automatic_and_explicit_disabled() {
        assert_eq!(
            audio_selection(PlaybackAudioSelection::Automatic, &[], false),
            PlaybackAudioSelection::Automatic
        );
        assert_eq!(
            audio_selection(PlaybackAudioSelection::Disabled, &[], false),
            PlaybackAudioSelection::Disabled
        );
        assert_eq!(
            audio_selection(PlaybackAudioSelection::Exact(2), &[], false),
            PlaybackAudioSelection::Exact(2)
        );
        assert_eq!(
            audio_selection(PlaybackAudioSelection::Automatic, &[], true),
            PlaybackAudioSelection::AutomaticSilence
        );
    }
    #[test]
    fn raw_subtitle_pin_requires_one_exact_fresh_page_row() {
        let mut tracks = TransientPlaybackTracks::from_streams(&[
            ersatzrs_domain::MediaStream {
                index: 2,
                kind: ersatzrs_domain::MediaStreamKind::Subtitle,
                codec: Some("dvb_teletext".into()),
                language: Some("nld".into()),
                ..ersatzrs_domain::MediaStream::default()
            },
            ersatzrs_domain::MediaStream {
                index: 2,
                kind: ersatzrs_domain::MediaStreamKind::Subtitle,
                codec: Some("dvb_teletext".into()),
                language: Some("eng".into()),
                ..ersatzrs_domain::MediaStream::default()
            },
        ]);
        assert!(pinned_subtitle(&tracks, 2).is_err());
        assert!(pinned_subtitle(&tracks, 9).is_err());
        tracks.subtitles.pop();
        assert_eq!(pinned_subtitle(&tracks, 2).unwrap(), tracks.subtitles[0]);
    }
    #[test]
    fn missing_optional_probe_retains_preferred_audio_but_fresh_inventory_replaces_it() {
        use ersatzrs_ffmpeg::{SourceAudioCodec, SourceAudioStream};
        let mut english = SourceAudioStream::new(SourceAudioCodec::Aac).with_index(1);
        english.language = Some("eng".into());
        english.default = true;
        let mut dutch = SourceAudioStream::new(SourceAudioCodec::Aac).with_index(3);
        dutch.language = Some("nld".into());
        let ordinary = vec![english, dutch];
        let unknown = observed_audio_or_fallback(&ordinary, &[], false);
        assert_eq!(unknown, ordinary);
        assert_eq!(
            ersatzrs_infra::playback::select_playback_audio_stream_index(
                &unknown,
                Some("nld"),
                None
            ),
            Some(3)
        );
        assert_eq!(
            audio_selection(PlaybackAudioSelection::Automatic, &[], false),
            PlaybackAudioSelection::Automatic
        );
        let mut replacement = ordinary[1].clone();
        replacement.index = 5;
        let fresh = observed_audio_or_fallback(&ordinary, &[replacement.clone()], true);
        assert_eq!(fresh, vec![replacement]);
        assert_eq!(
            ersatzrs_infra::playback::select_playback_audio_stream_index(&fresh, Some("nld"), None),
            Some(5)
        );
        assert!(observed_audio_or_fallback(&ordinary, &[], true).is_empty());
    }
}
