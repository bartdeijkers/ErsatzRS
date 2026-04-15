use async_trait::async_trait;
use nastv_core::repositories::{ChannelRepository, Result};
use nastv_domain::{Channel, EntityId};
use sqlx::sqlite::SqliteRow;
use sqlx::{Pool, Row, Sqlite};
use uuid::Uuid;

use super::common::{
    decode_enum, decode_message, decode_optional_duration, load_ids, not_found, replace_ids,
};

#[derive(Clone)]
pub struct SqliteChannelRepository(pub Pool<Sqlite>);

#[async_trait]
impl ChannelRepository for SqliteChannelRepository {
    async fn get_with_profile(&self, id: EntityId) -> Result<Channel> {
        let row = sqlx::query("SELECT * FROM channels WHERE id = ?")
            .bind(id)
            .fetch_one(&self.0)
            .await
            .map_err(|error| not_found("channel", id, error))?;
        let mut channel = map_channel(&row)?;
        channel.playout_ids = load_ids(
            &self.0,
            "SELECT id FROM playouts WHERE channel_id = ? ORDER BY id",
            id,
        )
        .await?;
        channel.artwork_ids = load_ids(
            &self.0,
            "SELECT artwork_id FROM channel_artwork WHERE channel_id = ? ORDER BY artwork_id",
            id,
        )
        .await?;
        Ok(channel)
    }

    async fn get_by_number(&self, number: &str) -> Result<Channel> {
        let row = sqlx::query("SELECT * FROM channels WHERE number = ?")
            .bind(number)
            .fetch_one(&self.0)
            .await?;
        map_channel(&row)
    }

    async fn list_enabled(&self) -> Result<Vec<Channel>> {
        let rows =
            sqlx::query("SELECT * FROM channels WHERE is_enabled = 1 ORDER BY sort_number, number")
                .fetch_all(&self.0)
                .await?;
        rows.iter()
            .map(|row| {
                let mut channel = map_channel(row)?;
                channel.playout_ids.clear();
                channel.artwork_ids.clear();
                Ok(channel)
            })
            .collect()
    }

    async fn upsert(&self, channel: &Channel) -> Result<()> {
        let mut tx = self.0.begin().await?;
        sqlx::query(
            "INSERT OR REPLACE INTO channels (
                id, unique_id, number, sort_number, name, channel_group, categories,
                ffmpeg_profile_id, slug_seconds, watermark_id, fallback_filler_id,
                streaming_mode, stream_selector_mode, stream_selector,
                preferred_audio_language_code, preferred_audio_title,
                preferred_subtitle_language_code, subtitle_mode,
                music_video_credits_mode, music_video_credits_template,
                song_video_mode, playout_source, playout_mode, mirror_source_channel_id,
                playout_offset_ms, transcode_mode, idle_behavior, is_enabled, show_in_epg
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(channel.id)
        .bind(channel.unique_id.to_string())
        .bind(&channel.number)
        .bind(channel.sort_number)
        .bind(&channel.name)
        .bind(&channel.group)
        .bind(&channel.categories)
        .bind(channel.ffmpeg_profile_id)
        .bind(channel.slug_seconds)
        .bind(channel.watermark_id)
        .bind(channel.fallback_filler_id)
        .bind(format!("{:?}", channel.streaming_mode))
        .bind(format!("{:?}", channel.stream_selector_mode))
        .bind(&channel.stream_selector)
        .bind(&channel.preferred_audio_language_code)
        .bind(&channel.preferred_audio_title)
        .bind(&channel.preferred_subtitle_language_code)
        .bind(format!("{:?}", channel.subtitle_mode))
        .bind(format!("{:?}", channel.music_video_credits_mode))
        .bind(&channel.music_video_credits_template)
        .bind(format!("{:?}", channel.song_video_mode))
        .bind(format!("{:?}", channel.playout_source))
        .bind(format!("{:?}", channel.playout_mode))
        .bind(channel.mirror_source_channel_id)
        .bind(channel.playout_offset.map(|value| value.num_milliseconds()))
        .bind(format!("{:?}", channel.transcode_mode))
        .bind(format!("{:?}", channel.idle_behavior))
        .bind(channel.is_enabled)
        .bind(channel.show_in_epg)
        .execute(&mut *tx)
        .await?;

        replace_ids(
            &mut tx,
            "DELETE FROM channel_artwork WHERE channel_id = ?",
            "INSERT OR REPLACE INTO channel_artwork (channel_id, artwork_id) VALUES (?, ?)",
            channel.id,
            &channel.artwork_ids,
        )
        .await?;

        tx.commit().await?;
        Ok(())
    }

    async fn delete(&self, id: EntityId) -> Result<()> {
        sqlx::query("DELETE FROM channels WHERE id = ?")
            .bind(id)
            .execute(&self.0)
            .await?;
        Ok(())
    }
}

fn map_channel(row: &SqliteRow) -> Result<Channel> {
    let unique_id = row.try_get::<String, _>("unique_id")?;
    Ok(Channel {
        id: row.try_get("id")?,
        unique_id: Uuid::parse_str(&unique_id)
            .map_err(|source| decode_message("channel unique_id", &source.to_string()))?,
        number: row.try_get("number")?,
        sort_number: row.try_get("sort_number")?,
        name: row.try_get("name")?,
        group: row.try_get("channel_group")?,
        categories: row.try_get("categories")?,
        ffmpeg_profile_id: row.try_get("ffmpeg_profile_id")?,
        ffmpeg_profile: None,
        slug_seconds: row.try_get("slug_seconds")?,
        watermark_id: row.try_get("watermark_id")?,
        fallback_filler_id: row.try_get("fallback_filler_id")?,
        fallback_filler: None,
        streaming_mode: decode_enum(
            "channel streaming_mode",
            &row.try_get::<String, _>("streaming_mode")?,
        )?,
        playout_ids: Vec::new(),
        artwork_ids: Vec::new(),
        stream_selector_mode: decode_enum(
            "channel stream_selector_mode",
            &row.try_get::<String, _>("stream_selector_mode")?,
        )?,
        stream_selector: row.try_get("stream_selector")?,
        preferred_audio_language_code: row.try_get("preferred_audio_language_code")?,
        preferred_audio_title: row.try_get("preferred_audio_title")?,
        preferred_subtitle_language_code: row.try_get("preferred_subtitle_language_code")?,
        subtitle_mode: decode_enum(
            "channel subtitle_mode",
            &row.try_get::<String, _>("subtitle_mode")?,
        )?,
        music_video_credits_mode: decode_enum(
            "channel music_video_credits_mode",
            &row.try_get::<String, _>("music_video_credits_mode")?,
        )?,
        music_video_credits_template: row.try_get("music_video_credits_template")?,
        song_video_mode: decode_enum(
            "channel song_video_mode",
            &row.try_get::<String, _>("song_video_mode")?,
        )?,
        playout_source: decode_enum(
            "channel playout_source",
            &row.try_get::<String, _>("playout_source")?,
        )?,
        playout_mode: decode_enum(
            "channel playout_mode",
            &row.try_get::<String, _>("playout_mode")?,
        )?,
        mirror_source_channel_id: row.try_get("mirror_source_channel_id")?,
        playout_offset: decode_optional_duration(row.try_get("playout_offset_ms")?),
        transcode_mode: decode_enum(
            "channel transcode_mode",
            &row.try_get::<String, _>("transcode_mode")?,
        )?,
        idle_behavior: decode_enum(
            "channel idle_behavior",
            &row.try_get::<String, _>("idle_behavior")?,
        )?,
        is_enabled: row.try_get("is_enabled")?,
        show_in_epg: row.try_get("show_in_epg")?,
    })
}
