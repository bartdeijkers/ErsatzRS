use std::collections::HashMap;

use ersatzrs_domain::{FFmpegProfileAudioFormat, FFmpegProfileVideoFormat};
use ersatzrs_infra::db::DbPool;
use ersatzrs_infra::db::DbRow;
use sqlx::Row;

/// Streaming specs for the multi-variant HLS playlist and `tvc-stream-*codec`
/// tags on the channels M3U.
///
/// Ports the subset of `GetChannelStreamingSpecsHandler` that the playlist
/// endpoints consume: video/audio format, output resolution, and bitrate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelStreamingSpec {
    pub video_format: FFmpegProfileVideoFormat,
    pub audio_format: FFmpegProfileAudioFormat,
    pub width: i32,
    pub height: i32,
    pub video_bitrate: i32,
    pub audio_bitrate: i32,
}

const SELECT_SPECS_SQL: &str = r#"
    SELECT c."Id"            AS "ChannelId",
           fp."VideoFormat"  AS "VideoFormat",
           fp."AudioFormat"  AS "AudioFormat",
           fp."VideoBitrate" AS "VideoBitrate",
           fp."AudioBitrate" AS "AudioBitrate",
           r."Width"         AS "Width",
           r."Height"        AS "Height"
    FROM "Channel" c
    JOIN "FFmpegProfile" fp ON fp."Id" = c."FFmpegProfileId"
    JOIN "Resolution"    r  ON r."Id"  = fp."ResolutionId"
"#;

pub async fn load_all_specs(
    pool: &DbPool,
) -> Result<HashMap<i64, ChannelStreamingSpec>, sqlx::Error> {
    let rows = sqlx::query(SELECT_SPECS_SQL).fetch_all(pool).await?;
    let mut out = HashMap::with_capacity(rows.len());
    for row in &rows {
        let channel_id: i64 = row.try_get("ChannelId")?;
        out.insert(channel_id, map_spec(row)?);
    }
    Ok(out)
}

pub async fn load_spec(
    pool: &DbPool,
    channel_id: i64,
) -> Result<Option<ChannelStreamingSpec>, sqlx::Error> {
    let sql = format!("{SELECT_SPECS_SQL} WHERE c.\"Id\" = ?");
    let row = sqlx::query(&sql)
        .bind(channel_id)
        .fetch_optional(pool)
        .await?;
    row.as_ref().map(map_spec).transpose()
}

fn map_spec(row: &DbRow) -> Result<ChannelStreamingSpec, sqlx::Error> {
    let video_int: i32 = row.try_get("VideoFormat")?;
    let audio_int: i32 = row.try_get("AudioFormat")?;
    Ok(ChannelStreamingSpec {
        video_format: map_video_format(video_int),
        audio_format: map_audio_format(audio_int),
        video_bitrate: row.try_get("VideoBitrate")?,
        audio_bitrate: row.try_get("AudioBitrate")?,
        width: row.try_get("Width")?,
        height: row.try_get("Height")?,
    })
}

fn map_video_format(value: i32) -> FFmpegProfileVideoFormat {
    match value {
        1 => FFmpegProfileVideoFormat::H264,
        2 => FFmpegProfileVideoFormat::Hevc,
        3 => FFmpegProfileVideoFormat::Mpeg2Video,
        4 => FFmpegProfileVideoFormat::Av1,
        99 => FFmpegProfileVideoFormat::Copy,
        _ => FFmpegProfileVideoFormat::None,
    }
}

fn map_audio_format(value: i32) -> FFmpegProfileAudioFormat {
    match value {
        1 => FFmpegProfileAudioFormat::Aac,
        2 => FFmpegProfileAudioFormat::Ac3,
        3 => FFmpegProfileAudioFormat::AacLatm,
        99 => FFmpegProfileAudioFormat::Copy,
        _ => FFmpegProfileAudioFormat::None,
    }
}
