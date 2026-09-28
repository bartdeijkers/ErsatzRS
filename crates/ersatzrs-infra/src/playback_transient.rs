//! Track selection for media that has no persisted library identity.
//! Candidate records are values only; their IDs must never be used for SQL or
//! provider-subtitle lookup. Extraction owners may supply prepared subtitle paths.

use super::{
    load_custom_stream_selector, next_subtitle_supported, select_custom_stream_candidates,
    select_subtitle_preferences, CustomStreamSelection, PlaybackAudioStream, PlaybackSubtitle,
    StreamSelector, SubtitleSelectionEngine,
};
use crate::{db::DbPool, ffmpeg_config::use_embedded_subtitles, metadata::subtitle_kind};
use chrono::{DateTime, Utc};
use ersatzrs_core::repositories::Result;
use ersatzrs_domain::{Channel, ChannelSubtitleMode, MediaStream, MediaStreamKind, StreamingMode};

/// Eligibility rules for the selected playback engine and source kind.
#[derive(Clone, Copy)]
pub struct TrackSelectionPolicy {
    pub mode: StreamingMode,
    pub engine: SubtitleSelectionEngine,
    pub media_server: bool,
}

/// Original track metadata for a transient programme. Consumers retain this for
/// the lifetime of the selected occurrence rather than re-reading a library row.
#[derive(Clone, Default)]
pub struct TransientPlaybackTracks {
    pub audio: Vec<PlaybackAudioStream>,
    pub subtitles: Vec<PlaybackSubtitle>,
}

impl TransientPlaybackTracks {
    /// Preserve original codec, language, title, default and forced flags.
    /// Embedded subtitles start unextracted, just like fresh scanner metadata.
    #[must_use]
    pub fn from_streams(streams: &[MediaStream]) -> Self {
        Self {
            audio: streams
                .iter()
                .filter(|stream| stream.kind == MediaStreamKind::Audio)
                .map(|stream| PlaybackAudioStream {
                    stream_index: stream.index,
                    title: stream.title.clone(),
                    language: stream.language.clone(),
                    default: stream.default,
                    forced: stream.forced,
                    codec: stream.codec.clone(),
                    channels: stream.channels,
                })
                .collect(),
            subtitles: streams
                .iter()
                .filter(|stream| {
                    matches!(
                        stream.kind,
                        MediaStreamKind::Subtitle | MediaStreamKind::ExternalSubtitle
                    )
                })
                .map(|stream| PlaybackSubtitle {
                    id: 0,
                    subtitle_kind: if stream.kind == MediaStreamKind::ExternalSubtitle {
                        subtitle_kind::SIDECAR
                    } else {
                        subtitle_kind::EMBEDDED
                    },
                    title: stream.title.clone(),
                    stream_index: stream.index,
                    codec: stream.codec.clone(),
                    default: stream.default,
                    forced: stream.forced,
                    sdh: false,
                    language: stream.language.clone(),
                    is_extracted: false,
                    path: stream
                        .file_name
                        .clone()
                        .filter(|name| !name.trim().is_empty()),
                })
                .collect(),
        }
    }

    /// Apply the existing channel YAML rules without any media-ID lookup.
    /// These are ordinary candidates, not forced troubleshooting selections.
    ///
    /// # Errors
    /// Returns the same selector-file errors as persisted playback.
    pub async fn select_custom(
        &self,
        channel: &Channel,
        start: DateTime<Utc>,
        policy: TrackSelectionPolicy,
    ) -> Result<Option<CustomStreamSelection>> {
        let Some(selector) = load_custom_stream_selector(channel, None).await? else {
            return Ok(None);
        };
        Ok(self.select_custom_rules(selector, channel, start, policy))
    }

    fn select_custom_rules(
        &self,
        selector: StreamSelector,
        channel: &Channel,
        start: DateTime<Utc>,
        policy: TrackSelectionPolicy,
    ) -> Option<CustomStreamSelection> {
        let mut subtitles = self.subtitles.clone();
        retain_playable_subtitles(
            &mut subtitles,
            policy.mode,
            policy.engine,
            policy.media_server,
        );
        select_custom_stream_candidates(
            selector,
            &self.audio,
            &mut subtitles,
            false,
            channel,
            start,
            None,
        )
    }

    /// Use built-in language and subtitle-mode preferences on transient tracks.
    /// The only database read is the global native embedded-subtitle setting.
    ///
    /// # Errors
    /// Returns errors reading that global setting.
    pub async fn select_subtitle(
        &self,
        pool: &DbPool,
        languages: &[String],
        subtitle_mode: ChannelSubtitleMode,
        policy: TrackSelectionPolicy,
    ) -> Result<Option<PlaybackSubtitle>> {
        if subtitle_mode == ChannelSubtitleMode::None {
            return Ok(None);
        }
        let mut subtitles = self.subtitles.clone();
        if policy.engine == SubtitleSelectionEngine::Native && !use_embedded_subtitles(pool).await?
        {
            subtitles.retain(|subtitle| subtitle.subtitle_kind != subtitle_kind::EMBEDDED);
        }
        retain_playable_subtitles(
            &mut subtitles,
            policy.mode,
            policy.engine,
            policy.media_server,
        );
        Ok(select_subtitle_preferences(
            &subtitles,
            languages,
            subtitle_mode,
        ))
    }
}

pub(super) fn retain_playable_subtitles(
    subtitles: &mut Vec<PlaybackSubtitle>,
    mode: StreamingMode,
    engine: SubtitleSelectionEngine,
    media_server: bool,
) {
    if engine == SubtitleSelectionEngine::Native && mode != StreamingMode::HttpLiveStreamingDirect {
        subtitles.retain(|subtitle| {
            subtitle.subtitle_kind != subtitle_kind::EMBEDDED
                || subtitle.is_image()
                || (subtitle.is_extracted
                    && subtitle
                        .path
                        .as_deref()
                        .is_some_and(|path| !path.trim().is_empty()))
        });
    }
    if engine == SubtitleSelectionEngine::Next {
        subtitles.retain(|subtitle| next_subtitle_supported(subtitle, media_server));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone as _;

    fn tracks() -> TransientPlaybackTracks {
        TransientPlaybackTracks::from_streams(&[
            MediaStream {
                index: 0,
                kind: MediaStreamKind::Video,
                ..MediaStream::default()
            },
            MediaStream {
                index: 1,
                kind: MediaStreamKind::Audio,
                codec: Some("aac".into()),
                language: Some("eng".into()),
                default: true,
                channels: Some(2),
                ..MediaStream::default()
            },
            MediaStream {
                index: 2,
                kind: MediaStreamKind::Audio,
                codec: Some("truehd".into()),
                language: Some("nld".into()),
                forced: true,
                channels: Some(6),
                ..MediaStream::default()
            },
            MediaStream {
                index: 3,
                kind: MediaStreamKind::Subtitle,
                codec: Some("srt".into()),
                language: Some("eng".into()),
                default: true,
                ..MediaStream::default()
            },
            MediaStream {
                index: 4,
                kind: MediaStreamKind::Subtitle,
                codec: Some("srt".into()),
                language: Some("nld".into()),
                forced: true,
                ..MediaStream::default()
            },
            MediaStream {
                index: 5,
                kind: MediaStreamKind::ExternalSubtitle,
                codec: Some("srt".into()),
                language: Some("fra".into()),
                file_name: Some("fixture.fr.srt".into()),
                ..MediaStream::default()
            },
        ])
    }

    fn policy(engine: SubtitleSelectionEngine) -> TrackSelectionPolicy {
        TrackSelectionPolicy {
            mode: StreamingMode::HttpLiveStreamingSegmenter,
            engine,
            media_server: false,
        }
    }

    #[test]
    fn transient_custom_rules_preserve_flags_languages_and_original_codecs() {
        let tracks = tracks();
        assert_eq!(tracks.audio.len(), 2);
        assert_eq!(tracks.subtitles.len(), 3);
        let yaml = "items:\n- audio_language: [nld, eng]\n  audio_condition: \"codec = 'truehd' and channels = 6 and forced = true\"\n  subtitle_language: [eng]\n  subtitle_condition: 'forced = false and default = true'\n";
        let selected = tracks
            .select_custom_rules(
                serde_yaml::from_str(yaml).unwrap(),
                &Channel::default(),
                Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
                policy(SubtitleSelectionEngine::Next),
            )
            .unwrap();
        assert_eq!(selected.audio_stream_index, Some(2));
        let subtitle = selected.subtitle.unwrap();
        assert_eq!(subtitle.stream_index, 3);
        assert!(!subtitle.forced);
        assert!(subtitle.default);
        assert_eq!(tracks.subtitles[2].subtitle_kind, subtitle_kind::SIDECAR);
        assert_eq!(tracks.subtitles[2].path.as_deref(), Some("fixture.fr.srt"));
    }

    #[test]
    fn transient_builtin_preferences_preserve_native_extraction_boundary() {
        let tracks = tracks();
        assert_eq!(
            select_subtitle_preferences(
                &tracks.subtitles,
                &["nl".into()],
                ChannelSubtitleMode::Forced
            )
            .unwrap()
            .stream_index,
            4
        );
        assert!(select_subtitle_preferences(
            &tracks.subtitles,
            &["en".into()],
            ChannelSubtitleMode::Forced
        )
        .is_none());
        let mut subtitles = tracks.subtitles.clone();
        retain_playable_subtitles(
            &mut subtitles,
            StreamingMode::HttpLiveStreamingSegmenter,
            SubtitleSelectionEngine::Native,
            false,
        );
        assert_eq!(
            subtitles.iter().map(|s| s.stream_index).collect::<Vec<_>>(),
            vec![5]
        );
        let mut prepared = tracks.subtitles.clone();
        prepared[0].is_extracted = true;
        prepared[0].path = Some("fixture.en.srt".into());
        retain_playable_subtitles(
            &mut prepared,
            StreamingMode::HttpLiveStreamingSegmenter,
            SubtitleSelectionEngine::Native,
            false,
        );
        assert_eq!(
            prepared.iter().map(|s| s.stream_index).collect::<Vec<_>>(),
            vec![3, 5]
        );
        let mut direct = tracks.subtitles.clone();
        retain_playable_subtitles(
            &mut direct,
            StreamingMode::HttpLiveStreamingDirect,
            SubtitleSelectionEngine::Native,
            false,
        );
        assert_eq!(direct.len(), 3);
    }

    #[test]
    fn transient_next_keeps_embedded_bitmaps_and_rejects_external_bitmaps_and_plex_captions() {
        let mut tracks = tracks();
        tracks.subtitles[0].codec = Some("hdmv_pgs_subtitle".into());
        tracks.subtitles[1].codec = Some("eia_608".into());
        tracks.subtitles[2].codec = Some("dvd_subtitle".into());
        retain_playable_subtitles(
            &mut tracks.subtitles,
            StreamingMode::HttpLiveStreamingSegmenter,
            SubtitleSelectionEngine::Next,
            true,
        );
        assert_eq!(
            tracks
                .subtitles
                .iter()
                .map(|s| s.stream_index)
                .collect::<Vec<_>>(),
            vec![3]
        );
    }
}
