use async_trait::async_trait;
use ersatzrs_core::repositories::{FfmpegProfileRepository, Result};
use ersatzrs_domain::{EntityId, FFmpegProfile};
use sqlx::sqlite::SqliteRow;
use sqlx::{Pool, Row, Sqlite};

use super::common::{decode_enum, not_found};

#[derive(Clone)]
pub struct SqliteFfmpegProfileRepository(pub Pool<Sqlite>);

#[async_trait]
impl FfmpegProfileRepository for SqliteFfmpegProfileRepository {
    async fn get(&self, id: EntityId) -> Result<FFmpegProfile> {
        let row = sqlx::query("SELECT * FROM ffmpeg_profiles WHERE id = ?")
            .bind(id)
            .fetch_one(&self.0)
            .await
            .map_err(|error| not_found("ffmpeg profile", id, error))?;
        map_profile(&row)
    }

    async fn list(&self) -> Result<Vec<FFmpegProfile>> {
        let rows = sqlx::query("SELECT * FROM ffmpeg_profiles ORDER BY name, id")
            .fetch_all(&self.0)
            .await?;
        rows.iter().map(map_profile).collect()
    }

    async fn upsert(&self, profile: &FFmpegProfile) -> Result<()> {
        sqlx::query(
            "INSERT OR REPLACE INTO ffmpeg_profiles (
                id, name, thread_count, normalize_audio, normalize_video, hardware_acceleration,
                vaapi_display, vaapi_driver, vaapi_device, qsv_extra_hardware_frames,
                resolution_id, scaling_behavior, pad_mode, video_format, video_profile,
                video_preset, allow_b_frames, bit_depth, video_bitrate, video_buffer_size,
                tonemap_algorithm, audio_format, audio_bitrate, audio_buffer_size,
                normalize_loudness_mode, target_loudness, audio_channels, audio_sample_rate,
                normalize_framerate, normalize_colors, deinterlace_video
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(profile.id)
        .bind(&profile.name)
        .bind(profile.thread_count)
        .bind(profile.normalize_audio)
        .bind(profile.normalize_video)
        .bind(format!("{:?}", profile.hardware_acceleration))
        .bind(&profile.vaapi_display)
        .bind(format!("{:?}", profile.vaapi_driver))
        .bind(&profile.vaapi_device)
        .bind(profile.qsv_extra_hardware_frames)
        .bind(profile.resolution_id)
        .bind(format!("{:?}", profile.scaling_behavior))
        .bind(format!("{:?}", profile.pad_mode))
        .bind(format!("{:?}", profile.video_format))
        .bind(&profile.video_profile)
        .bind(&profile.video_preset)
        .bind(profile.allow_b_frames)
        .bind(format!("{:?}", profile.bit_depth))
        .bind(profile.video_bitrate)
        .bind(profile.video_buffer_size)
        .bind(format!("{:?}", profile.tonemap_algorithm))
        .bind(format!("{:?}", profile.audio_format))
        .bind(profile.audio_bitrate)
        .bind(profile.audio_buffer_size)
        .bind(format!("{:?}", profile.normalize_loudness_mode))
        .bind(profile.target_loudness)
        .bind(profile.audio_channels)
        .bind(profile.audio_sample_rate)
        .bind(profile.normalize_framerate)
        .bind(profile.normalize_colors)
        .bind(profile.deinterlace_video)
        .execute(&self.0)
        .await?;
        Ok(())
    }

    async fn delete(&self, id: EntityId) -> Result<()> {
        sqlx::query("DELETE FROM ffmpeg_profiles WHERE id = ?")
            .bind(id)
            .execute(&self.0)
            .await?;
        Ok(())
    }
}

fn map_profile(row: &SqliteRow) -> Result<FFmpegProfile> {
    Ok(FFmpegProfile {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
        thread_count: row.try_get("thread_count")?,
        normalize_audio: row.try_get("normalize_audio")?,
        normalize_video: row.try_get("normalize_video")?,
        hardware_acceleration: decode_enum(
            "ffmpeg profile hardware_acceleration",
            &row.try_get::<String, _>("hardware_acceleration")?,
        )?,
        vaapi_display: row.try_get("vaapi_display")?,
        vaapi_driver: decode_enum(
            "ffmpeg profile vaapi_driver",
            &row.try_get::<String, _>("vaapi_driver")?,
        )?,
        vaapi_device: row.try_get("vaapi_device")?,
        qsv_extra_hardware_frames: row.try_get("qsv_extra_hardware_frames")?,
        resolution_id: row.try_get("resolution_id")?,
        scaling_behavior: decode_enum(
            "ffmpeg profile scaling_behavior",
            &row.try_get::<String, _>("scaling_behavior")?,
        )?,
        pad_mode: decode_enum(
            "ffmpeg profile pad_mode",
            &row.try_get::<String, _>("pad_mode")?,
        )?,
        video_format: decode_enum(
            "ffmpeg profile video_format",
            &row.try_get::<String, _>("video_format")?,
        )?,
        video_profile: row.try_get("video_profile")?,
        video_preset: row.try_get("video_preset")?,
        allow_b_frames: row.try_get("allow_b_frames")?,
        bit_depth: decode_enum(
            "ffmpeg profile bit_depth",
            &row.try_get::<String, _>("bit_depth")?,
        )?,
        video_bitrate: row.try_get("video_bitrate")?,
        video_buffer_size: row.try_get("video_buffer_size")?,
        tonemap_algorithm: decode_enum(
            "ffmpeg profile tonemap_algorithm",
            &row.try_get::<String, _>("tonemap_algorithm")?,
        )?,
        audio_format: decode_enum(
            "ffmpeg profile audio_format",
            &row.try_get::<String, _>("audio_format")?,
        )?,
        audio_bitrate: row.try_get("audio_bitrate")?,
        audio_buffer_size: row.try_get("audio_buffer_size")?,
        normalize_loudness_mode: decode_enum(
            "ffmpeg profile normalize_loudness_mode",
            &row.try_get::<String, _>("normalize_loudness_mode")?,
        )?,
        target_loudness: row.try_get("target_loudness")?,
        audio_channels: row.try_get("audio_channels")?,
        audio_sample_rate: row.try_get("audio_sample_rate")?,
        normalize_framerate: row.try_get("normalize_framerate")?,
        normalize_colors: row.try_get("normalize_colors")?,
        deinterlace_video: row.try_get("deinterlace_video")?,
    })
}
