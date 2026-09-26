use async_trait::async_trait;
use ersatzrs_core::repositories::{
    BlockPlayoutHistoryAdmin, BlockPlayoutOptionAdmin, MpegTroubleshootingChannelAdmin,
    PlaybackChannelSummaryAdmin, PlaybackMediaSummaryAdmin, PlaybackSubtitleAdmin,
    PlaybackTroubleshootingAdmin, RepositoryError, Result, TroubleshootingBlockAdmin,
    TroubleshootingChannelSummaryAdmin, TroubleshootingFfmpegProfileSummaryAdmin,
    TroubleshootingOptionAdmin, TroubleshootingPreviewMediaAdmin, TroubleshootingRepository,
};
use ersatzrs_domain::{Channel, ChannelSubtitleMode, EntityId, StreamingMode};
use serde_json::Value;
use sqlx::{Pool, Row};

use crate::repositories::SqlxResultExt as _;

#[derive(Clone)]
pub struct SqliteTroubleshootingRepository(pub Pool<crate::db::Db>);

fn decode_streaming_mode(value: i64) -> Result<StreamingMode> {
    match value {
        0 => Ok(StreamingMode::TransportStream),
        1 => Ok(StreamingMode::HttpLiveStreamingDirect),
        2 => Ok(StreamingMode::HttpLiveStreamingSegmenter),
        3 => Ok(StreamingMode::TransportStreamHybrid),
        _ => Err(RepositoryError::Decode {
            kind: "channel streaming mode",
            value: value.to_string(),
        }),
    }
}

fn decode_channel_subtitle_mode(value: i64) -> Result<ChannelSubtitleMode> {
    match value {
        0 => Ok(ChannelSubtitleMode::None),
        1 => Ok(ChannelSubtitleMode::Forced),
        2 => Ok(ChannelSubtitleMode::Default),
        3 => Ok(ChannelSubtitleMode::Any),
        _ => Err(RepositoryError::Decode {
            kind: "channel subtitle mode",
            value: value.to_string(),
        }),
    }
}

async fn load_options(
    pool: &Pool<crate::db::Db>,
    sql: &'static str,
) -> Result<Vec<TroubleshootingOptionAdmin>> {
    let rows = sqlx::query(sql).fetch_all(pool).await.repository()?;
    rows.iter()
        .map(|row| {
            Ok(TroubleshootingOptionAdmin {
                id: row.try_get("Id").repository()?,
                name: row.try_get("Name").repository()?,
            })
        })
        .collect()
}

async fn collection_name_from_key(pool: &Pool<crate::db::Db>, key: &Value) -> Result<String> {
    for (field, table) in [
        ("CollectionId", "Collection"),
        ("SmartCollectionId", "SmartCollection"),
    ] {
        if let Some(id) = key.get(field).and_then(Value::as_i64).filter(|id| *id > 0) {
            let sql = format!(r#"SELECT COALESCE("Name", '') FROM "{table}" WHERE "Id" = ?"#);
            return sqlx::query_scalar(crate::db::assert_sql_safe(&sql))
                .bind(id)
                .fetch_optional(pool)
                .await
                .repository()
                .map(Option::unwrap_or_default);
        }
    }
    Ok(String::new())
}

async fn media_item_summary(pool: &Pool<crate::db::Db>, id: EntityId) -> Result<(String, String)> {
    let row = sqlx::query(
        r#"SELECT
             CASE
               WHEN EXISTS (SELECT 1 FROM "Movie" WHERE "Id" = media."Id") THEN 'Movie'
               WHEN EXISTS (SELECT 1 FROM "Episode" WHERE "Id" = media."Id") THEN 'Episode'
               WHEN EXISTS (SELECT 1 FROM "MusicVideo" WHERE "Id" = media."Id") THEN 'MusicVideo'
               WHEN EXISTS (SELECT 1 FROM "OtherVideo" WHERE "Id" = media."Id") THEN 'OtherVideo'
               WHEN EXISTS (SELECT 1 FROM "RemoteStream" WHERE "Id" = media."Id") THEN 'RemoteStream'
               WHEN EXISTS (SELECT 1 FROM "Song" WHERE "Id" = media."Id") THEN 'Song'
               WHEN EXISTS (SELECT 1 FROM "Image" WHERE "Id" = media."Id") THEN 'Image'
               WHEN EXISTS (SELECT 1 FROM "Show" WHERE "Id" = media."Id") THEN 'Show'
               WHEN EXISTS (SELECT 1 FROM "Season" WHERE "Id" = media."Id") THEN 'Season'
               WHEN EXISTS (SELECT 1 FROM "Artist" WHERE "Id" = media."Id") THEN 'Artist'
               ELSE ''
             END AS "Kind",
             COALESCE(
               (SELECT "Title" FROM "EpisodeMetadata" WHERE "EpisodeId" = media."Id" LIMIT 1),
               (SELECT "Title" FROM "MovieMetadata" WHERE "MovieId" = media."Id" LIMIT 1),
               (SELECT "Title" FROM "MusicVideoMetadata" WHERE "MusicVideoId" = media."Id" LIMIT 1),
               (SELECT "Title" FROM "OtherVideoMetadata" WHERE "OtherVideoId" = media."Id" LIMIT 1),
               (SELECT "Title" FROM "RemoteStreamMetadata" WHERE "RemoteStreamId" = media."Id" LIMIT 1),
               (SELECT "Title" FROM "SongMetadata" WHERE "SongId" = media."Id" LIMIT 1),
               (SELECT "Title" FROM "ImageMetadata" WHERE "ImageId" = media."Id" LIMIT 1),
               (SELECT "Title" FROM "ShowMetadata" WHERE "ShowId" = media."Id" LIMIT 1),
               (SELECT "Title" FROM "SeasonMetadata" WHERE "SeasonId" = media."Id" LIMIT 1),
               (SELECT "Title" FROM "ArtistMetadata" WHERE "ArtistId" = media."Id" LIMIT 1),
               ''
             ) AS "Title"
           FROM "MediaItem" media WHERE media."Id" = ?"#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .repository()?;
    row.map_or_else(
        || Ok((String::new(), String::new())),
        |row| {
            Ok((
                row.try_get("Kind").repository()?,
                row.try_get("Title").repository()?,
            ))
        },
    )
}

async fn playback_media(
    pool: &Pool<crate::db::Db>,
    id: EntityId,
) -> Result<Option<PlaybackMediaSummaryAdmin>> {
    let row = sqlx::query(
        r#"SELECT
             CASE
               WHEN movie."Id" IS NOT NULL THEN 'Movie'
               WHEN episode."Id" IS NOT NULL THEN 'Episode'
               WHEN other_video."Id" IS NOT NULL THEN 'OtherVideo'
               WHEN remote_stream."Id" IS NOT NULL THEN 'RemoteStream'
               WHEN song."Id" IS NOT NULL THEN 'Song'
               WHEN image."Id" IS NOT NULL THEN 'Image'
               ELSE 'Playback'
             END AS "Kind",
             COALESCE(movie_meta."Title", episode_meta."Title", other_video_meta."Title",
                      remote_stream_meta."Title", song_meta."Title", image_meta."Title", '') AS "Title",
             COALESCE(version."Duration", '00:00:00') AS "Duration"
           FROM "MediaItem" media
           LEFT JOIN "Movie" movie ON movie."Id" = media."Id"
           LEFT JOIN "MovieMetadata" movie_meta ON movie_meta."MovieId" = movie."Id"
           LEFT JOIN "Episode" episode ON episode."Id" = media."Id"
           LEFT JOIN "EpisodeMetadata" episode_meta ON episode_meta."EpisodeId" = episode."Id"
           LEFT JOIN "OtherVideo" other_video ON other_video."Id" = media."Id"
           LEFT JOIN "OtherVideoMetadata" other_video_meta
             ON other_video_meta."OtherVideoId" = other_video."Id"
           LEFT JOIN "RemoteStream" remote_stream ON remote_stream."Id" = media."Id"
           LEFT JOIN "RemoteStreamMetadata" remote_stream_meta
             ON remote_stream_meta."RemoteStreamId" = remote_stream."Id"
           LEFT JOIN "Song" song ON song."Id" = media."Id"
           LEFT JOIN "SongMetadata" song_meta ON song_meta."SongId" = song."Id"
           LEFT JOIN "Image" image ON image."Id" = media."Id"
           LEFT JOIN "ImageMetadata" image_meta ON image_meta."ImageId" = image."Id"
           LEFT JOIN "MediaVersion" version
             ON version."MovieId" = movie."Id" OR version."EpisodeId" = episode."Id"
             OR version."OtherVideoId" = other_video."Id"
             OR version."RemoteStreamId" = remote_stream."Id"
             OR version."SongId" = song."Id" OR version."ImageId" = image."Id"
           WHERE media."Id" = ? ORDER BY version."Id" LIMIT 1"#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .repository()?;
    row.map(|row| {
        Ok(PlaybackMediaSummaryAdmin {
            kind: row.try_get("Kind").repository()?,
            title: row.try_get("Title").repository()?,
            duration: row.try_get("Duration").repository()?,
        })
    })
    .transpose()
}

async fn playback_subtitles(
    pool: &Pool<crate::db::Db>,
    id: EntityId,
) -> Result<Vec<PlaybackSubtitleAdmin>> {
    let rows = sqlx::query(
        r#"SELECT subtitle."Id", COALESCE(subtitle."Language", '') AS "Language",
                  COALESCE(subtitle."Title", '') AS "Title",
                  COALESCE(subtitle."Codec", '') AS "Codec"
           FROM "Subtitle" subtitle
           WHERE (
             subtitle."MovieMetadataId" IN
               (SELECT "Id" FROM "MovieMetadata" WHERE "MovieId" = ?)
             OR subtitle."EpisodeMetadataId" IN
               (SELECT "Id" FROM "EpisodeMetadata" WHERE "EpisodeId" = ?)
             OR subtitle."OtherVideoMetadataId" IN
               (SELECT "Id" FROM "OtherVideoMetadata" WHERE "OtherVideoId" = ?)
           )
           AND NOT (subtitle."SubtitleKind" = 0 AND subtitle."IsExtracted" = 0
             AND COALESCE(subtitle."Codec", '') NOT IN
               ('dvd_subtitle', 'hdmv_pgs_subtitle', 'xsub'))
           ORDER BY subtitle."StreamIndex", subtitle."Id""#,
    )
    .bind(id)
    .bind(id)
    .bind(id)
    .fetch_all(pool)
    .await
    .repository()?;
    rows.iter()
        .map(|row| {
            Ok(PlaybackSubtitleAdmin {
                id: row.try_get("Id").repository()?,
                language: row.try_get("Language").repository()?,
                title: row.try_get("Title").repository()?,
                codec: row.try_get("Codec").repository()?,
            })
        })
        .collect()
}

#[async_trait]
impl TroubleshootingRepository for SqliteTroubleshootingRepository {
    async fn list_block_playout_options_admin(&self) -> Result<Vec<BlockPlayoutOptionAdmin>> {
        let rows = sqlx::query(
            r#"SELECT playout."Id", COALESCE(channel."Number", '') AS "ChannelNumber",
                      COALESCE(channel."Name", '') AS "ChannelName"
               FROM "Playout" playout
               JOIN "Channel" channel ON channel."Id" = playout."ChannelId"
               WHERE playout."ScheduleKind" = 3
               ORDER BY channel."SortNumber", channel."Number", playout."Id""#,
        )
        .fetch_all(&self.0)
        .await
        .repository()?;
        rows.iter()
            .map(|row| {
                Ok(BlockPlayoutOptionAdmin {
                    playout_id: row.try_get("Id").repository()?,
                    channel_number: row.try_get("ChannelNumber").repository()?,
                    channel_name: row.try_get("ChannelName").repository()?,
                })
            })
            .collect()
    }

    async fn list_blocks_for_playout_admin(
        &self,
        playout_id: EntityId,
    ) -> Result<Vec<TroubleshootingBlockAdmin>> {
        if playout_id <= 0 {
            return Ok(Vec::new());
        }
        let rows = sqlx::query(
            r#"SELECT DISTINCT block."Id", block."BlockGroupId",
                      COALESCE(block_group."Name", '') AS "GroupName",
                      COALESCE(block."Name", '') AS "Name",
                      block."Minutes", block."StopScheduling"
               FROM "TemplateItem" template_item
               JOIN "PlayoutTemplate" playout_template
                 ON playout_template."TemplateId" = template_item."TemplateId"
               JOIN "Block" block ON block."Id" = template_item."BlockId"
               JOIN "BlockGroup" block_group ON block_group."Id" = block."BlockGroupId"
               WHERE playout_template."PlayoutId" = ?
               ORDER BY "GroupName", "Name", block."Id""#,
        )
        .bind(playout_id)
        .fetch_all(&self.0)
        .await
        .repository()?;
        rows.iter()
            .map(|row| {
                Ok(TroubleshootingBlockAdmin {
                    id: row.try_get("Id").repository()?,
                    group_id: row.try_get("BlockGroupId").repository()?,
                    group_name: row.try_get("GroupName").repository()?,
                    name: row.try_get("Name").repository()?,
                    minutes: row.try_get("Minutes").repository()?,
                    stop_scheduling: row.try_get("StopScheduling").repository()?,
                })
            })
            .collect()
    }

    async fn list_block_playout_history_admin(
        &self,
        playout_id: EntityId,
        block_id: EntityId,
    ) -> Result<Vec<BlockPlayoutHistoryAdmin>> {
        if playout_id <= 0 || block_id <= 0 {
            return Ok(Vec::new());
        }
        let rows = sqlx::query(
            r#"SELECT "Id", "When", "Finish", "Key", "Details", "PlaybackOrder"
               FROM "PlayoutHistory"
               WHERE "PlayoutId" = ? AND "BlockId" = ?
               ORDER BY "Id" LIMIT 100"#,
        )
        .bind(playout_id)
        .bind(block_id)
        .fetch_all(&self.0)
        .await
        .repository()?;
        let mut history = Vec::with_capacity(rows.len());
        for row in rows {
            let key: String = row.try_get("Key").repository()?;
            let details: String = row.try_get("Details").repository()?;
            let key_json =
                serde_json::from_str::<Value>(&key).map_err(|error| RepositoryError::Decode {
                    kind: "playout history key",
                    value: error.to_string(),
                })?;
            let details_json = serde_json::from_str::<Value>(&details).map_err(|error| {
                RepositoryError::Decode {
                    kind: "playout history details",
                    value: error.to_string(),
                }
            })?;
            let (media_item_type, media_item_title) =
                if let Some(id) = details_json.get("MediaItemId").and_then(Value::as_i64) {
                    media_item_summary(&self.0, id).await?
                } else {
                    (String::new(), String::new())
                };
            history.push(BlockPlayoutHistoryAdmin {
                id: row.try_get("Id").repository()?,
                start: row.try_get("When").repository()?,
                finish: row.try_get("Finish").repository()?,
                key,
                details,
                playback_order: row.try_get("PlaybackOrder").repository()?,
                collection_name: collection_name_from_key(&self.0, &key_json).await?,
                media_item_type,
                media_item_title,
            });
        }
        Ok(history)
    }

    async fn load_playback_context_admin(
        &self,
        media_item_id: EntityId,
        channel_id: EntityId,
    ) -> Result<PlaybackTroubleshootingAdmin> {
        let channel = if channel_id > 0 {
            sqlx::query(
                r#"SELECT COALESCE("Name", '') AS "Name", "FFmpegProfileId"
                   FROM "Channel" WHERE "Id" = ?"#,
            )
            .bind(channel_id)
            .fetch_optional(&self.0)
            .await
            .repository()?
            .map(|row| {
                Ok(PlaybackChannelSummaryAdmin {
                    name: row.try_get("Name").repository()?,
                    ffmpeg_profile_id: row.try_get("FFmpegProfileId").repository()?,
                })
            })
            .transpose()?
        } else {
            None
        };
        Ok(PlaybackTroubleshootingAdmin {
            media: if media_item_id > 0 {
                playback_media(&self.0, media_item_id).await?
            } else {
                None
            },
            channel,
            ffmpeg_profiles: load_options(
                &self.0,
                r#"SELECT "Id", COALESCE("Name", '') AS "Name"
                   FROM "FFmpegProfile" ORDER BY "Name", "Id""#,
            )
            .await?,
            watermarks: load_options(
                &self.0,
                r#"SELECT "Id", COALESCE("Name", '') AS "Name"
                   FROM "ChannelWatermark" ORDER BY "Name", "Id""#,
            )
            .await?,
            graphics_elements: load_options(
                &self.0,
                r#"SELECT "Id", COALESCE("Name", "Path", '') AS "Name"
                   FROM "GraphicsElement" ORDER BY "Name", "Id""#,
            )
            .await?,
            subtitles: if media_item_id > 0 {
                playback_subtitles(&self.0, media_item_id).await?
            } else {
                Vec::new()
            },
        })
    }

    async fn list_mpeg_channels_admin(&self) -> Result<Vec<MpegTroubleshootingChannelAdmin>> {
        let rows = sqlx::query(
            r#"SELECT COALESCE("Number", '') AS "Number", COALESCE("Name", '') AS "Name"
               FROM "Channel" WHERE "IsEnabled" = 1
               ORDER BY "SortNumber", "Number", "Id""#,
        )
        .fetch_all(&self.0)
        .await
        .repository()?;
        let mut channels = Vec::new();
        for row in rows {
            let number: String = row.try_get("Number").repository()?;
            if !number.trim().is_empty() {
                channels.push(MpegTroubleshootingChannelAdmin {
                    number,
                    name: row.try_get("Name").repository()?,
                });
            }
        }
        Ok(channels)
    }

    async fn list_report_channels_admin(&self) -> Result<Vec<TroubleshootingChannelSummaryAdmin>> {
        let rows = sqlx::query(
            r#"SELECT "Id", "Number", "Name", "FFmpegProfileId", "StreamingMode",
                      "StreamSelectorMode", "StreamSelector", "IsEnabled"
               FROM "Channel" ORDER BY "SortNumber", "Number", "Id""#,
        )
        .fetch_all(&self.0)
        .await
        .repository()?;
        rows.iter()
            .map(|row| {
                Ok(TroubleshootingChannelSummaryAdmin {
                    id: row.try_get("Id").repository()?,
                    number: row.try_get("Number").repository()?,
                    name: row.try_get("Name").repository()?,
                    ffmpeg_profile_id: row.try_get("FFmpegProfileId").repository()?,
                    streaming_mode: row.try_get("StreamingMode").repository()?,
                    stream_selector_mode: row.try_get("StreamSelectorMode").repository()?,
                    stream_selector: row.try_get("StreamSelector").repository()?,
                    is_enabled: row.try_get("IsEnabled").repository()?,
                })
            })
            .collect()
    }

    async fn list_report_ffmpeg_profiles_admin(
        &self,
    ) -> Result<Vec<TroubleshootingFfmpegProfileSummaryAdmin>> {
        let rows = sqlx::query(
            r#"SELECT "Id", "Name", "HardwareAcceleration", "VideoFormat",
                      "AudioFormat", "ResolutionId"
               FROM "FFmpegProfile" ORDER BY "Name", "Id""#,
        )
        .fetch_all(&self.0)
        .await
        .repository()?;
        rows.iter()
            .map(|row| {
                Ok(TroubleshootingFfmpegProfileSummaryAdmin {
                    id: row.try_get("Id").repository()?,
                    name: row.try_get("Name").repository()?,
                    hardware_acceleration: row.try_get("HardwareAcceleration").repository()?,
                    video_format: row.try_get("VideoFormat").repository()?,
                    audio_format: row.try_get("AudioFormat").repository()?,
                    resolution_id: row.try_get("ResolutionId").repository()?,
                })
            })
            .collect()
    }

    async fn load_preview_media_admin(
        &self,
        media_item_id: EntityId,
        media_version_id: Option<EntityId>,
    ) -> Result<Option<TroubleshootingPreviewMediaAdmin>> {
        let row = sqlx::query(
            r#"SELECT mv."Id" AS "MediaVersionId",
                      mf."Path" AS "Path",
                      mv."Duration" AS "Duration",
                      CASE
                        WHEN mv."MovieId" IS NOT NULL THEN 'Movie'
                        WHEN mv."EpisodeId" IS NOT NULL THEN 'Episode'
                        WHEN mv."MusicVideoId" IS NOT NULL THEN 'MusicVideo'
                        WHEN mv."OtherVideoId" IS NOT NULL THEN 'OtherVideo'
                        WHEN mv."SongId" IS NOT NULL THEN 'Song'
                        WHEN mv."ImageId" IS NOT NULL THEN 'Image'
                        WHEN mv."RemoteStreamId" IS NOT NULL THEN 'RemoteStream'
                        ELSE 'MediaItem'
                      END AS "Kind"
               FROM "MediaVersion" mv
               JOIN "MediaFile" mf ON mf."MediaVersionId" = mv."Id"
               WHERE (mv."MovieId" = ?
                  OR mv."EpisodeId" = ?
                  OR mv."OtherVideoId" = ?
                  OR mv."MusicVideoId" = ?
                  OR mv."SongId" = ?
                  OR mv."ImageId" = ?
                  OR mv."RemoteStreamId" = ?)
                 AND (? IS NULL OR mv."Id" = ?)
               ORDER BY mv."Id", mf."Id"
               LIMIT 1"#,
        )
        .bind(media_item_id)
        .bind(media_item_id)
        .bind(media_item_id)
        .bind(media_item_id)
        .bind(media_item_id)
        .bind(media_item_id)
        .bind(media_item_id)
        .bind(media_version_id)
        .bind(media_version_id)
        .fetch_optional(&self.0)
        .await
        .repository()?;
        row.map(|row| {
            let duration = row.try_get::<Option<String>, _>("Duration").repository()?;
            let duration =
                super::common::try_decode_duration_text(duration.as_deref().unwrap_or("00:00:00"))?
                    .to_std()
                    .map_err(|_| {
                        RepositoryError::unavailable("preview duration", "invalid time span")
                    })?;
            Ok(TroubleshootingPreviewMediaAdmin {
                media_item_id,
                media_version_id: row.try_get("MediaVersionId").repository()?,
                path: row
                    .try_get::<Option<String>, _>("Path")
                    .repository()?
                    .unwrap_or_default(),
                kind: row.try_get("Kind").repository()?,
                duration_seconds: duration.as_secs_f64(),
            })
        })
        .transpose()
    }

    async fn load_preview_channel_admin(&self, channel_id: EntityId) -> Result<Option<Channel>> {
        let row = sqlx::query(
            r#"SELECT "Id", "Number", COALESCE("Name", '') AS "Name",
                      "FFmpegProfileId", "WatermarkId", "StreamingMode",
                      "StreamSelector", "PreferredAudioLanguageCode",
                      "PreferredSubtitleLanguageCode",
                      "SubtitleMode", "IsEnabled"
               FROM "Channel"
               WHERE "Id" = ?"#,
        )
        .bind(channel_id)
        .fetch_optional(&self.0)
        .await
        .repository()?;
        row.map(|row| {
            Ok(Channel {
                id: row.try_get("Id").repository()?,
                number: row.try_get("Number").repository()?,
                name: row.try_get("Name").repository()?,
                ffmpeg_profile_id: row.try_get("FFmpegProfileId").repository()?,
                watermark_id: row.try_get("WatermarkId").repository()?,
                streaming_mode: decode_streaming_mode(row.try_get("StreamingMode").repository()?)?,
                stream_selector: row.try_get("StreamSelector").repository()?,
                preferred_audio_language_code: row
                    .try_get("PreferredAudioLanguageCode")
                    .repository()?,
                preferred_subtitle_language_code: row
                    .try_get("PreferredSubtitleLanguageCode")
                    .repository()?,
                subtitle_mode: decode_channel_subtitle_mode(
                    row.try_get("SubtitleMode").repository()?,
                )?,
                is_enabled: row.try_get::<i64, _>("IsEnabled").repository()? != 0,
                ..Channel::default()
            })
        })
        .transpose()
    }

    async fn load_media_item_path_admin(&self, media_item_id: EntityId) -> Result<Option<String>> {
        let row = sqlx::query(
            r#"SELECT mf."Path"
               FROM "MediaFile" mf
               JOIN "MediaVersion" mv ON mv."Id" = mf."MediaVersionId"
               WHERE mv."MovieId" = ?
                  OR mv."EpisodeId" = ?
                  OR mv."OtherVideoId" = ?
                  OR mv."MusicVideoId" = ?
                  OR mv."SongId" = ?
                  OR mv."ImageId" = ?
                  OR mv."RemoteStreamId" = ?
               ORDER BY mv."Id", mf."Id"
               LIMIT 1"#,
        )
        .bind(media_item_id)
        .bind(media_item_id)
        .bind(media_item_id)
        .bind(media_item_id)
        .bind(media_item_id)
        .bind(media_item_id)
        .bind(media_item_id)
        .fetch_optional(&self.0)
        .await
        .repository()?;
        row.map(|row| row.try_get::<Option<String>, _>("Path").repository())
            .transpose()
            .map(Option::flatten)
    }
}
