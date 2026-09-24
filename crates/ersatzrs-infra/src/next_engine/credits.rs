//! Session-bound generated music-video credits for Next playback.

use ersatzrs_core::repositories::{ChannelRepository, RepositoryError, Result};
use ersatzrs_domain::{
    Channel, ChannelMusicVideoCreditsMode, ChannelStreamSelectorMode, ChannelSubtitleMode,
    PlayoutItem,
};
use sqlx::Row;

use crate::{
    channel_playout::ChannelPlayoutContext,
    music_video_credits::render_music_video_credits_subtitle,
    playback::load_channel_playback_profile,
    repositories::{common::decode_duration_text, make_repositories, SqlxResultExt as _},
};

use super::{host::NextHost, media::MediaRelay, protocol, selection};

pub(super) async fn attach(
    pool: &crate::db::DbPool,
    relay: &MediaRelay,
    channel: &Channel,
    item: &PlayoutItem,
    projection: &mut selection::DeferredProjection,
) -> Result<()> {
    if channel.music_video_credits_mode != ChannelMusicVideoCreditsMode::GenerateSubtitles
        || item.subtitle_mode.unwrap_or(channel.subtitle_mode) == ChannelSubtitleMode::None
        || item.pinned_stream_selection.is_some()
        || (channel.stream_selector_mode == ChannelStreamSelectorMode::Custom
            && projection.selected_subtitle.is_some())
    {
        return Ok(());
    }
    let music_video: bool =
        sqlx::query_scalar(r#"SELECT EXISTS(SELECT 1 FROM "MusicVideo" WHERE "Id" = ?)"#)
            .bind(item.media_item_id)
            .fetch_one(pool)
            .await
            .repository()?;
    if !music_video {
        return Ok(());
    }
    let in_point = u64::try_from(item.in_point.num_milliseconds())
        .map_err(|_| unavailable("credits in-point is invalid"))?;
    let out_point = u64::try_from(item.out_point.num_milliseconds())
        .map_err(|_| unavailable("credits out-point is invalid"))?;
    if out_point != 0 && out_point <= in_point {
        return Err(unavailable("credits interval is invalid"));
    }
    let source =
        relay.credits_source(channel.id, item.id, item.media_item_id, in_point, out_point)?;
    let tracks = projection
        .item
        .tracks
        .as_mut()
        .ok_or_else(|| unavailable("credits tracks are unavailable"))?;
    tracks.subtitle = Some(protocol::Track {
        source: Some(source),
        stream_index: Some(0),
    });
    projection.pending_burn = None;
    projection.selected_subtitle = None;
    Ok(())
}

impl NextHost {
    /// Render one current music-video item while its channel has an active Next session.
    /// The HTTP boundary authenticates the binding ticket before calling this.
    /// # Errors
    /// Refuses expired sessions and changed source bindings.
    pub async fn render_credits(
        &self,
        channel_id: i64,
        item_id: i64,
        media_id: i64,
        in_point_ms: u64,
        out_point_ms: u64,
    ) -> Result<Option<String>> {
        let active = self
            .dynamic_contexts
            .lock()
            .await
            .values()
            .any(|context| context.request.channel_id == channel_id);
        if !active {
            return Err(unavailable("credits session is unavailable"));
        }
        let channel = make_repositories(self.pool.clone())
            .channel
            .get_with_profile(channel_id)
            .await?;
        if channel.music_video_credits_mode != ChannelMusicVideoCreditsMode::GenerateSubtitles {
            return Ok(None);
        }
        let source_channel_id = ChannelPlayoutContext::for_channel(&channel).source_channel_id;
        let row = sqlx::query(
            r#"SELECT pi."InPoint", pi."OutPoint", pi."SubtitleMode", pi."MediaVersionId"
               FROM "PlayoutItem" pi
               JOIN "Playout" p ON p."Id" = pi."PlayoutId"
               JOIN "MusicVideo" mv ON mv."Id" = pi."MediaItemId"
               WHERE pi."Id" = ? AND pi."MediaItemId" = ? AND p."ChannelId" = ?"#,
        )
        .bind(item_id)
        .bind(media_id)
        .bind(source_channel_id)
        .fetch_optional(&self.pool)
        .await
        .repository()?;
        let Some(row) = row else {
            return Ok(None);
        };
        let subtitle_mode: Option<i64> = row.try_get("SubtitleMode").repository()?;
        if subtitle_mode == Some(0)
            || (subtitle_mode.is_none() && channel.subtitle_mode == ChannelSubtitleMode::None)
            || row
                .try_get::<Option<i64>, _>("MediaVersionId")
                .repository()?
                .is_some()
        {
            return Ok(None);
        }
        let in_point = decode_duration_text(&row.try_get::<String, _>("InPoint").repository()?);
        let out_point = decode_duration_text(&row.try_get::<String, _>("OutPoint").repository()?);
        if u64::try_from(in_point.num_milliseconds()).ok() != Some(in_point_ms)
            || u64::try_from(out_point.num_milliseconds()).ok() != Some(out_point_ms)
        {
            return Err(unavailable("credits binding changed"));
        }
        let profile = load_channel_playback_profile(&self.pool, channel_id)
            .await?
            .ok_or_else(|| unavailable("credits profile is unavailable"))?;
        let rendered = render_music_video_credits_subtitle(
            &self.pool,
            media_id,
            &channel,
            &profile.resolution,
            in_point.num_seconds(),
        )
        .await?;
        if !self
            .dynamic_contexts
            .lock()
            .await
            .values()
            .any(|context| context.request.channel_id == channel_id)
        {
            return Err(unavailable("credits session expired"));
        }
        Ok(rendered)
    }
}

fn unavailable(message: &str) -> RepositoryError {
    RepositoryError::Decode {
        kind: "Next music-video credits",
        value: message.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::next_engine::{host, protocol::Source, schedule};
    use chrono::Duration;
    use ersatzrs_core::repositories::PlayoutRepository;
    use ersatzrs_domain::{Resolution, StreamingEngine};
    use std::{path::PathBuf, sync::Arc};

    async fn fixture() -> (
        crate::db::DbPool,
        ersatzrs_core::locking::EntityLocker,
        Channel,
        PlayoutItem,
    ) {
        let (pool, locker, mut channel, _) = schedule::tests::fixture().await;
        for sql in [
            r#"INSERT INTO "MediaItem" ("Id", "LibraryPathId", "State") VALUES (31, 1, 0)"#,
            r#"INSERT INTO "Artist" ("Id") VALUES (31)"#,
            r#"INSERT INTO "ArtistMetadata" ("Id", "ArtistId", "Title", "MetadataKind", "DateAdded", "DateUpdated") VALUES (32, 31, 'Solo Artist', 0, '2026-09-24', '2026-09-24')"#,
            r#"INSERT INTO "MusicVideo" ("Id", "ArtistId") VALUES (10, 31)"#,
            r#"UPDATE "MediaVersion" SET "MovieId"=NULL, "MusicVideoId"=10, "Duration"='00:12:00' WHERE "Id"=100"#,
            r#"DELETE FROM "Movie" WHERE "Id"=10"#,
            r#"INSERT INTO "MusicVideoMetadata" ("Id", "MusicVideoId", "Title", "Album", "MetadataKind", "DateAdded", "DateUpdated") VALUES (33, 10, 'Credit Song', 'Credit Album', 0, '2026-09-24', '2026-09-24')"#,
        ] {
            sqlx::query(sql)
                .execute(&pool)
                .await
                .unwrap_or_else(|error| panic!("fixture SQL failed: {sql}: {error}"));
        }
        channel.streaming_engine = StreamingEngine::Next;
        channel.subtitle_mode = ChannelSubtitleMode::Default;
        channel.music_video_credits_mode = ChannelMusicVideoCreditsMode::GenerateSubtitles;
        channel.music_video_credits_template = Some("_ArtistTitleAlbum_CenterTop".into());
        channel.preferred_audio_language_code = Some("nld".into());
        make_repositories(pool.clone())
            .channel
            .upsert(&channel)
            .await
            .unwrap();
        let mut item = selection::tests::item();
        item.id = 1;
        item.playout_id = 1;
        item.in_point = Duration::seconds(60);
        item.out_point = Duration::seconds(120);
        make_repositories(pool.clone())
            .playout
            .upsert_item(&item)
            .await
            .unwrap();
        (pool, locker, channel, item)
    }

    async fn project(
        pool: &crate::db::DbPool,
        channel: &Channel,
        item: &PlayoutItem,
    ) -> selection::DeferredProjection {
        selection::project_deferred_item(
            pool,
            channel,
            item,
            100,
            Source::Local {
                path: "media/first.mkv".into(),
                in_point_ms: None,
                out_point_ms: None,

                probe_hint: None,
            },
            &Resolution {
                width: 320,
                height: 180,
                ..Default::default()
            },
        )
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn credits_attach_keeps_selected_audio_and_respects_explicit_off() {
        let (pool, _, channel, item) = fixture().await;
        let relay = MediaRelay::new();
        relay.configure("127.0.0.1:8409".parse().unwrap()).unwrap();
        let mut projection = project(&pool, &channel, &item).await;
        assert_eq!(
            projection
                .item
                .tracks
                .as_ref()
                .unwrap()
                .audio
                .as_ref()
                .unwrap()
                .stream_index,
            Some(2)
        );
        attach(&pool, &relay, &channel, &item, &mut projection)
            .await
            .unwrap();
        let subtitle = projection.item.tracks.unwrap().subtitle.unwrap();
        assert_eq!(subtitle.stream_index, Some(0));
        let Source::Http {
            uri,
            headers,
            in_point_ms,
            out_point_ms,
            ..
        } = subtitle.source.unwrap()
        else {
            panic!("generated credits must use an HTTP subtitle source");
        };
        let (path, ticket) = uri.split_once("?ticket=").unwrap();
        assert!(path.ends_with("/ffmpeg/next-credits/1/1/10/60000/120000"));
        assert!(relay.authorized_credits((1, 1, 10, 60_000, 120_000), ticket));
        assert!(!relay.authorized_credits((1, 1, 10, 61_000, 120_000), ticket));
        assert!(!MediaRelay::new().authorized_credits((1, 1, 10, 60_000, 120_000), ticket));
        assert_eq!((in_point_ms, out_point_ms), (Some(60_000), Some(120_000)));
        assert!(headers.is_empty());

        let mut off = item.clone();
        off.subtitle_mode = Some(ChannelSubtitleMode::None);
        let mut projected_off = project(&pool, &channel, &off).await;
        attach(&pool, &relay, &channel, &off, &mut projected_off)
            .await
            .unwrap();
        assert!(projected_off.item.tracks.unwrap().subtitle.is_none());
    }

    #[tokio::test]
    async fn credits_render_requires_live_session_and_exact_item_binding() {
        let (pool, locker, channel, item) = fixture().await;
        let root = tempfile::tempdir().unwrap();
        let host = host::NextHost::new(pool.clone(), locker, root.path().to_owned());
        let instance = uuid::Uuid::new_v4();
        host.dynamic_contexts.lock().await.insert(
            instance,
            host::DynamicContext {
                request: host::NextStart {
                    channel_id: channel.id,
                    preview: false,
                    number: channel.number.clone(),
                    executable: PathBuf::from("fixture-next"),
                    output: PathBuf::from("fixture-output"),
                    ffmpeg: PathBuf::from("fixture-ffmpeg"),
                    ffprobe: PathBuf::from("fixture-ffprobe"),
                    replacements: Vec::new(),
                    idle_timeout: None,
                    deadline: tokio::time::Instant::now() + std::time::Duration::from_secs(30),
                },
                transfers: Arc::default(),
                fallback: Arc::default(),
                gaps: Arc::default(),
            },
        );
        let rendered = host
            .render_credits(channel.id, item.id, 10, 60_000, 120_000)
            .await
            .unwrap()
            .unwrap();
        assert!(rendered.contains("PlayResX: 320"));
        assert!(rendered.contains("Dialogue: 0,00:01:09.00,00:01:16.00"));
        sqlx::query(
            r#"UPDATE "Channel" SET "MusicVideoCreditsTemplate"='missing-template' WHERE "Id"=1"#,
        )
        .execute(&pool)
        .await
        .unwrap();
        let fallback = host
            .render_credits(channel.id, item.id, 10, 60_000, 120_000)
            .await
            .unwrap()
            .unwrap();
        assert!(fallback.contains("OPTIKabel-Heavy"));
        assert!(fallback.contains("Credit Song"));
        assert!(host
            .render_credits(channel.id, item.id, 11, 60_000, 120_000)
            .await
            .unwrap()
            .is_none());
        assert!(host
            .render_credits(channel.id, item.id, 10, 61_000, 120_000)
            .await
            .is_err());
        sqlx::query(
            r#"UPDATE "PlayoutItem" SET "MediaVersionId"=100,"VideoStreamIndex"=0 WHERE "Id"=1"#,
        )
        .execute(&pool)
        .await
        .unwrap();
        assert!(host
            .render_credits(channel.id, item.id, 10, 60_000, 120_000)
            .await
            .unwrap()
            .is_none());
        host.dynamic_contexts.lock().await.remove(&instance);
        assert!(host
            .render_credits(channel.id, item.id, 10, 60_000, 120_000)
            .await
            .is_err());
    }
}
