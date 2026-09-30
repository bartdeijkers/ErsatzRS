//! Upstream Prepare/StartTroubleshootingPlayback credits, using owned artifacts.
//! The preview lock owns these files; no channel or playout is persisted.

use super::{
    duration_as_i64_seconds, ApiResult, Channel, DbPool, Duration, FsPath, PlaybackPreviewQuery,
    PlaybackProfile, PlaybackSubtitle, PreviewMediaItem,
};
use ersatzrs_domain::ChannelMusicVideoCreditsMode;
use ersatzrs_infra::music_video_credits::render_music_video_credits_artifact;

pub(super) fn channel(
    query: &PlaybackPreviewQuery,
    media: &PreviewMediaItem,
    original: &Channel,
) -> Channel {
    let mut channel = original.clone();
    if media.kind == "MusicVideo" {
        if let Some(template) = query
            .music_video_credits_template
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            channel.music_video_credits_mode = ChannelMusicVideoCreditsMode::GenerateSubtitles;
            channel.music_video_credits_template = Some(template.to_owned());
        }
    }
    channel
}

pub(super) fn enabled(channel: &Channel, media: &PreviewMediaItem) -> bool {
    media.kind == "MusicVideo"
        && channel.music_video_credits_mode == ChannelMusicVideoCreditsMode::GenerateSubtitles
}

pub(super) async fn prepare(
    pool: &DbPool,
    media: &PreviewMediaItem,
    channel: &Channel,
    profile: &PlaybackProfile,
    seek: Duration,
    working: &FsPath,
) -> ApiResult<Option<PlaybackSubtitle>> {
    if !enabled(channel, media) {
        return Ok(None);
    }
    let Some((ass, template)) = render_music_video_credits_artifact(
        pool,
        media.media_item_id,
        channel,
        &profile.resolution,
        duration_as_i64_seconds(seek),
    )
    .await?
    else {
        return Ok(None);
    };
    tokio::fs::write(working.join("music-video-credits.ass"), &ass).await?;
    let playback = ersatzrs_infra::subtitles::rebase_text_cues_milliseconds(
        ass.as_bytes().to_vec(),
        ersatzrs_ffmpeg::SubtitleTextFormat::Ass,
        u64::try_from(seek.as_millis())?,
        4 * 1024 * 1024,
    )?
    .unwrap_or_else(|| {
        ass.lines()
            .filter(|line| !line.trim_start().starts_with("Dialogue:"))
            .collect::<Vec<_>>()
            .join("\n")
    });
    let path = working.join("music-video-credits-playback.ass");
    tokio::fs::write(&path, playback).await?;
    tokio::fs::write(
        working.join("music-video-credits-template.sbntxt"),
        template,
    )
    .await?;
    Ok(Some(PlaybackSubtitle {
        id: 0,
        subtitle_kind: ersatzrs_infra::metadata::subtitle_kind::GENERATED,
        title: Some("Music Video Credits".to_owned()),
        stream_index: 0,
        codec: Some("ass".to_owned()),
        default: true,
        forced: true,
        sdh: false,
        language: None,
        is_extracted: false,
        path: Some(
            path.to_str()
                .ok_or("invalid credits path encoding")?
                .to_owned(),
        ),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ersatzrs_app::PlaybackPreviewRequest;

    #[test]
    fn credits_selection_is_music_only_optional_and_does_not_change_original_channel() {
        let mut query: PlaybackPreviewQuery = PlaybackPreviewRequest::default().into();
        let original = Channel::default();
        let media = PreviewMediaItem {
            media_item_id: 1,
            media_version_id: 1,
            kind: "MusicVideo".into(),
            path: "fixture.mkv".into(),
            duration_seconds: 60.0,
        };
        for value in [None, Some(""), Some("  ")] {
            query.music_video_credits_template = value.map(str::to_owned);
            assert!(!enabled(&channel(&query, &media, &original), &media));
        }
        query.music_video_credits_template = Some("_MusicFactory_ArtistTitle".into());
        assert!(enabled(&channel(&query, &media, &original), &media));
        assert_eq!(
            original.music_video_credits_mode,
            ChannelMusicVideoCreditsMode::None
        );
        for kind in ["Movie", "Episode", "RemoteStream", "Song", "OtherVideo"] {
            let mut other = media.clone();
            other.kind = kind.into();
            assert!(!enabled(&channel(&query, &other, &original), &other));
        }
    }
}
