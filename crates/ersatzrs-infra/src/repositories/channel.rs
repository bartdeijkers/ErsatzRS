use async_trait::async_trait;
use ersatzrs_core::repositories::{ChannelRepository, Result};
use ersatzrs_domain::{Channel, EntityId};
use sqlx::sqlite::SqliteRow;
use sqlx::{Pool, Row, Sqlite};
use uuid::Uuid;

use super::common::{
    decode_duration_text, decode_enum_column, decode_message, encode_duration, encode_enum,
    load_ids, not_found,
};

#[derive(Clone)]
pub struct SqliteChannelRepository(pub Pool<Sqlite>);

#[async_trait]
impl ChannelRepository for SqliteChannelRepository {
    async fn get_with_profile(&self, id: EntityId) -> Result<Channel> {
        let row = sqlx::query(r#"SELECT * FROM "Channel" WHERE "Id" = ?"#)
            .bind(id)
            .fetch_one(&self.0)
            .await
            .map_err(|error| not_found("channel", id, error))?;
        let mut channel = map_channel(&row)?;
        channel.playout_ids = load_ids(
            &self.0,
            r#"SELECT "Id" FROM "Playout" WHERE "ChannelId" = ? ORDER BY "Id""#,
            id,
        )
        .await?;
        channel.artwork_ids = load_ids(
            &self.0,
            r#"SELECT "Id" FROM "Artwork" WHERE "ChannelId" = ? ORDER BY "Id""#,
            id,
        )
        .await?;
        Ok(channel)
    }

    async fn get_by_number(&self, number: &str) -> Result<Channel> {
        let row = sqlx::query(r#"SELECT * FROM "Channel" WHERE "Number" = ?"#)
            .bind(number)
            .fetch_one(&self.0)
            .await?;
        map_channel(&row)
    }

    async fn list_enabled(&self) -> Result<Vec<Channel>> {
        let rows = sqlx::query(
            r#"SELECT * FROM "Channel" WHERE "IsEnabled" = 1 ORDER BY "SortNumber", "Number""#,
        )
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
            r#"INSERT OR REPLACE INTO "Channel" (
                "Id", "UniqueId", "Number", "SortNumber", "Name", "Group", "Categories",
                "FFmpegProfileId", "SlugSeconds", "WatermarkId", "FallbackFillerId",
                "StreamingMode", "StreamSelectorMode", "StreamSelector",
                "PreferredAudioLanguageCode", "PreferredAudioTitle",
                "PreferredSubtitleLanguageCode", "SubtitleMode",
                "MusicVideoCreditsMode", "MusicVideoCreditsTemplate",
                "SongVideoMode", "PlayoutSource", "PlayoutMode", "MirrorSourceChannelId",
                "PlayoutOffset", "TranscodeMode", "IdleBehavior", "IsEnabled", "ShowInEpg"
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
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
        .bind(encode_enum(channel.streaming_mode))
        .bind(encode_enum(channel.stream_selector_mode))
        .bind(&channel.stream_selector)
        .bind(&channel.preferred_audio_language_code)
        .bind(&channel.preferred_audio_title)
        .bind(&channel.preferred_subtitle_language_code)
        .bind(encode_enum(channel.subtitle_mode))
        .bind(encode_enum(channel.music_video_credits_mode))
        .bind(&channel.music_video_credits_template)
        .bind(encode_enum(channel.song_video_mode))
        .bind(encode_enum(channel.playout_source))
        .bind(encode_enum(channel.playout_mode))
        .bind(channel.mirror_source_channel_id)
        .bind(channel.playout_offset.map(encode_duration))
        .bind(encode_enum(channel.transcode_mode))
        .bind(encode_enum(channel.idle_behavior))
        .bind(channel.is_enabled)
        .bind(channel.show_in_epg)
        .execute(&mut *tx)
        .await?;

        // Update artwork rows that belong to this channel: clear old ones, set new ones.
        // In ErsatzTV, Artwork.ChannelId is a direct FK - we don't use a junction table.
        // We clear the ChannelId on previously linked artwork and set it on the new ones.
        sqlx::query(r#"UPDATE "Artwork" SET "ChannelId" = NULL WHERE "ChannelId" = ?"#)
            .bind(channel.id)
            .execute(&mut *tx)
            .await?;
        for artwork_id in &channel.artwork_ids {
            sqlx::query(r#"UPDATE "Artwork" SET "ChannelId" = ? WHERE "Id" = ?"#)
                .bind(channel.id)
                .bind(artwork_id)
                .execute(&mut *tx)
                .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    async fn delete(&self, id: EntityId) -> Result<()> {
        sqlx::query(r#"DELETE FROM "Channel" WHERE "Id" = ?"#)
            .bind(id)
            .execute(&self.0)
            .await?;
        Ok(())
    }
}

fn map_channel(row: &SqliteRow) -> Result<Channel> {
    let unique_id = row.try_get::<String, _>("UniqueId")?;
    Ok(Channel {
        id: row.try_get("Id")?,
        unique_id: Uuid::parse_str(&unique_id)
            .map_err(|source| decode_message("channel unique_id", &source.to_string()))?,
        number: row.try_get("Number")?,
        sort_number: row.try_get("SortNumber")?,
        name: row.try_get("Name")?,
        group: row.try_get("Group")?,
        categories: row.try_get("Categories")?,
        ffmpeg_profile_id: row.try_get("FFmpegProfileId")?,
        ffmpeg_profile: None,
        slug_seconds: row.try_get("SlugSeconds")?,
        watermark_id: row.try_get("WatermarkId")?,
        fallback_filler_id: row.try_get("FallbackFillerId")?,
        fallback_filler: None,
        streaming_mode: decode_enum_column("channel streaming_mode", row, "StreamingMode")?,
        playout_ids: Vec::new(),
        artwork_ids: Vec::new(),
        stream_selector_mode: decode_enum_column(
            "channel stream_selector_mode",
            row,
            "StreamSelectorMode",
        )?,
        stream_selector: row.try_get("StreamSelector")?,
        preferred_audio_language_code: row.try_get("PreferredAudioLanguageCode")?,
        preferred_audio_title: row.try_get("PreferredAudioTitle")?,
        preferred_subtitle_language_code: row.try_get("PreferredSubtitleLanguageCode")?,
        subtitle_mode: decode_enum_column("channel subtitle_mode", row, "SubtitleMode")?,
        music_video_credits_mode: decode_enum_column(
            "channel music_video_credits_mode",
            row,
            "MusicVideoCreditsMode",
        )?,
        music_video_credits_template: row.try_get("MusicVideoCreditsTemplate")?,
        song_video_mode: decode_enum_column("channel song_video_mode", row, "SongVideoMode")?,
        playout_source: decode_enum_column("channel playout_source", row, "PlayoutSource")?,
        playout_mode: decode_enum_column("channel playout_mode", row, "PlayoutMode")?,
        mirror_source_channel_id: row.try_get("MirrorSourceChannelId")?,
        playout_offset: row
            .try_get::<Option<String>, _>("PlayoutOffset")?
            .as_deref()
            .map(decode_duration_text),
        transcode_mode: decode_enum_column("channel transcode_mode", row, "TranscodeMode")?,
        idle_behavior: decode_enum_column("channel idle_behavior", row, "IdleBehavior")?,
        is_enabled: row.try_get("IsEnabled")?,
        show_in_epg: row.try_get("ShowInEpg")?,
    })
}
