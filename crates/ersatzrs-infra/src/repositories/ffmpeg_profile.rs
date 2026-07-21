use crate::db::DbRow;
use async_trait::async_trait;
use ersatzrs_core::repositories::{FfmpegProfileRepository, Result};
use ersatzrs_domain::{EntityId, FFmpegProfile};
use sqlx::{Pool, Row};

use super::common::{
    decode_bool_column, decode_enum_column, decode_optional_bool_column,
    decode_optional_string_column, decode_string_column, encode_enum, not_found,
};

#[derive(Clone)]
pub struct SqliteFfmpegProfileRepository(pub Pool<crate::db::Db>);

#[async_trait]
impl FfmpegProfileRepository for SqliteFfmpegProfileRepository {
    async fn get(&self, id: EntityId) -> Result<FFmpegProfile> {
        let row = sqlx::query(r#"SELECT * FROM "FFmpegProfile" WHERE "Id" = ?"#)
            .bind(id)
            .fetch_one(&self.0)
            .await
            .map_err(|error| not_found("ffmpeg profile", id, error))?;
        map_profile(&row)
    }

    async fn list(&self) -> Result<Vec<FFmpegProfile>> {
        let rows = sqlx::query(r#"SELECT * FROM "FFmpegProfile" ORDER BY "Name", "Id""#)
            .fetch_all(&self.0)
            .await?;
        rows.iter().map(map_profile).collect()
    }

    async fn upsert(&self, profile: &FFmpegProfile) -> Result<()> {
        sqlx::query(
            r#"REPLACE INTO "FFmpegProfile" (
                "Id", "Name", "ThreadCount", "TranscodeAudio", "TranscodeVideo",
                "HardwareAcceleration", "VaapiDisplay", "VaapiDriver", "VaapiDevice",
                "QsvExtraHardwareFrames", "ResolutionId", "ScalingBehavior", "PadMode",
                "VideoFormat", "VideoProfile", "VideoPreset", "AllowBFrames", "BitDepth",
                "VideoBitrate", "VideoBufferSize", "TonemapAlgorithm", "AudioFormat",
                "AudioBitrate", "AudioBufferSize", "NormalizeLoudnessMode", "TargetLoudness",
                "AudioChannels", "AudioSampleRate", "NormalizeFramerate", "NormalizeColors",
                "DeinterlaceVideo"
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#
        )
        .bind(profile.id)
        .bind(&profile.name)
        .bind(profile.thread_count)
        .bind(profile.transcode_audio)
        .bind(profile.transcode_video)
        .bind(encode_enum(profile.hardware_acceleration))
        .bind(&profile.vaapi_display)
        .bind(encode_enum(profile.vaapi_driver))
        .bind(&profile.vaapi_device)
        .bind(profile.qsv_extra_hardware_frames)
        .bind(profile.resolution_id)
        .bind(encode_enum(profile.scaling_behavior))
        .bind(encode_enum(profile.pad_mode))
        .bind(encode_enum(profile.video_format))
        .bind(&profile.video_profile)
        .bind(&profile.video_preset)
        .bind(profile.allow_b_frames)
        .bind(encode_enum(profile.bit_depth))
        .bind(profile.video_bitrate)
        .bind(profile.video_buffer_size)
        .bind(encode_enum(profile.tonemap_algorithm))
        .bind(encode_enum(profile.audio_format))
        .bind(profile.audio_bitrate)
        .bind(profile.audio_buffer_size)
        .bind(encode_enum(profile.normalize_loudness_mode))
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
        sqlx::query(r#"DELETE FROM "FFmpegProfile" WHERE "Id" = ?"#)
            .bind(id)
            .execute(&self.0)
            .await?;
        Ok(())
    }
}

fn map_profile(row: &DbRow) -> Result<FFmpegProfile> {
    Ok(FFmpegProfile {
        id: row.try_get("Id")?,
        name: decode_string_column(row, "Name")?,
        thread_count: row.try_get("ThreadCount")?,
        transcode_audio: decode_bool_column(row, "TranscodeAudio")?,
        transcode_video: decode_bool_column(row, "TranscodeVideo")?,
        hardware_acceleration: decode_enum_column(
            "ffmpeg profile hardware_acceleration",
            row,
            "HardwareAcceleration",
        )?,
        vaapi_display: decode_optional_string_column(row, "VaapiDisplay")?,
        vaapi_driver: decode_enum_column("ffmpeg profile vaapi_driver", row, "VaapiDriver")?,
        vaapi_device: decode_optional_string_column(row, "VaapiDevice")?,
        qsv_extra_hardware_frames: row.try_get("QsvExtraHardwareFrames")?,
        resolution_id: row.try_get("ResolutionId")?,
        scaling_behavior: decode_enum_column(
            "ffmpeg profile scaling_behavior",
            row,
            "ScalingBehavior",
        )?,
        pad_mode: decode_enum_column("ffmpeg profile pad_mode", row, "PadMode")?,
        video_format: decode_enum_column("ffmpeg profile video_format", row, "VideoFormat")?,
        video_profile: decode_optional_string_column(row, "VideoProfile")?,
        video_preset: decode_optional_string_column(row, "VideoPreset")?,
        allow_b_frames: decode_bool_column(row, "AllowBFrames")?,
        bit_depth: decode_enum_column("ffmpeg profile bit_depth", row, "BitDepth")?,
        video_bitrate: row.try_get("VideoBitrate")?,
        video_buffer_size: row.try_get("VideoBufferSize")?,
        tonemap_algorithm: decode_enum_column(
            "ffmpeg profile tonemap_algorithm",
            row,
            "TonemapAlgorithm",
        )?,
        audio_format: decode_enum_column("ffmpeg profile audio_format", row, "AudioFormat")?,
        audio_bitrate: row.try_get("AudioBitrate")?,
        audio_buffer_size: row.try_get("AudioBufferSize")?,
        normalize_loudness_mode: decode_enum_column(
            "ffmpeg profile normalize_loudness_mode",
            row,
            "NormalizeLoudnessMode",
        )?,
        target_loudness: row.try_get("TargetLoudness")?,
        audio_channels: row.try_get("AudioChannels")?,
        audio_sample_rate: row.try_get("AudioSampleRate")?,
        normalize_framerate: decode_bool_column(row, "NormalizeFramerate")?,
        normalize_colors: decode_bool_column(row, "NormalizeColors")?,
        deinterlace_video: decode_optional_bool_column(row, "DeinterlaceVideo")?,
    })
}
