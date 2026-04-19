use leptos::prelude::*;
use serde::{Deserialize, Serialize};

/// Lightweight row for the `FFmpegProfile` list table.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct FfmpegProfileListRow {
    pub id: i64,
    pub name: String,
    pub hardware_accel: String,
    pub video_format: String,
}

/// Form data for create / edit.
///
/// Enum columns are stored as DB integers to keep serialisation trivial.
/// `resolution_id` is preserved from the loaded row on update; on insert the
/// server function seeds a default 1080p resolution if none exists.
/// Fields not surfaced in the UI (`audio_buffer_size`, `normalize_video`, etc.)
/// are kept here so UPDATE can be partial and INSERT can supply defaults.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct FfmpegProfileForm {
    pub id: i64,
    pub name: String,
    pub thread_count: i32,
    /// 0=None 1=Qsv 2=Nvenc 3=Vaapi 4=VideoToolbox 5=Amf 6=V4l2m2m 7=Rkmpp
    pub hardware_acceleration: i32,
    /// 0=None 1=H264 2=Hevc 3=Mpeg2Video 4=Av1 5=Copy
    pub video_format: i32,
    pub video_bitrate: i32,
    pub video_buffer_size: i32,
    /// 0=None 1=Aac 2=Ac3 3=AacLatm 4=Copy
    pub audio_format: i32,
    pub audio_bitrate: i32,
    pub audio_channels: i32,
    pub audio_sample_rate: i32,
    /// 0=Off 1=LoudNorm
    pub normalize_loudness_mode: i32,
    pub deinterlace_video: bool,
    pub vaapi_device: String,
    /// 0=Auto 1=IHD 2=I965 3=RadeonSI 4=Nouveau
    pub vaapi_driver: i32,
    /// Preserved from the loaded row; 0 triggers default-resolution seeding.
    pub resolution_id: i64,
}

impl FfmpegProfileForm {
    #[must_use]
    pub fn new_defaults() -> Self {
        Self {
            video_format: 1, // H264
            video_bitrate: 2000,
            video_buffer_size: 4000,
            audio_format: 1, // AAC
            audio_bitrate: 192,
            audio_channels: 2,
            audio_sample_rate: 48,
            ..Default::default()
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn hw_label(v: i32) -> &'static str {
    match v {
        1 => "QSV",
        2 => "NVENC",
        3 => "VAAPI",
        4 => "VideoToolbox",
        5 => "AMF",
        6 => "V4L2M2M",
        7 => "RKMPP",
        _ => "None",
    }
}

fn vf_label(v: i32) -> &'static str {
    match v {
        1 => "H264",
        2 => "H265 (HEVC)",
        3 => "MPEG-2",
        4 => "AV1",
        5 => "Copy",
        _ => "None",
    }
}

// ---------------------------------------------------------------------------
// Server functions
// ---------------------------------------------------------------------------

/// Returns paginated profile rows.
#[server]
pub async fn list_ffmpeg_profile_rows(
    page: u32,
    page_size: u32,
) -> Result<Vec<FfmpegProfileListRow>, ServerFnError> {
    use crate::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();
    let offset = i64::from(page) * i64::from(page_size);

    let rows = sqlx::query(
        r#"SELECT "Id", "Name", "HardwareAcceleration", "VideoFormat"
           FROM "FFmpegProfile"
           ORDER BY "Name", "Id"
           LIMIT ? OFFSET ?"#,
    )
    .bind(i64::from(page_size))
    .bind(offset)
    .fetch_all(&state.pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| FfmpegProfileListRow {
            id: r.try_get("Id").unwrap_or(0),
            name: r.try_get("Name").unwrap_or_default(),
            hardware_accel: hw_label(r.try_get("HardwareAcceleration").unwrap_or(0)).to_string(),
            video_format: vf_label(r.try_get("VideoFormat").unwrap_or(0)).to_string(),
        })
        .collect())
}

/// Returns total profile count.
#[server]
pub async fn count_ffmpeg_profiles() -> Result<u64, ServerFnError> {
    use crate::state::AppState;

    let state = expect_context::<AppState>();
    let count: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "FFmpegProfile""#)
        .fetch_one(&state.pool)
        .await?;
    Ok(u64::try_from(count).unwrap_or(0))
}

/// Loads a profile into `FfmpegProfileForm`. Returns `None` if not found.
#[server]
pub async fn get_ffmpeg_profile_form(id: i64) -> Result<Option<FfmpegProfileForm>, ServerFnError> {
    use crate::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();
    let maybe = sqlx::query(r#"SELECT * FROM "FFmpegProfile" WHERE "Id" = ?"#)
        .bind(id)
        .fetch_optional(&state.pool)
        .await?;

    Ok(maybe.map(|r| FfmpegProfileForm {
        id: r.try_get("Id").unwrap_or(0),
        name: r
            .try_get::<Option<String>, _>("Name")
            .unwrap_or_default()
            .unwrap_or_default(),
        thread_count: r.try_get("ThreadCount").unwrap_or(0),
        hardware_acceleration: r.try_get("HardwareAcceleration").unwrap_or(0),
        video_format: r.try_get("VideoFormat").unwrap_or(1),
        video_bitrate: r.try_get("VideoBitrate").unwrap_or(2000),
        video_buffer_size: r.try_get("VideoBufferSize").unwrap_or(4000),
        audio_format: r.try_get("AudioFormat").unwrap_or(1),
        audio_bitrate: r.try_get("AudioBitrate").unwrap_or(192),
        audio_channels: r.try_get("AudioChannels").unwrap_or(2),
        audio_sample_rate: r.try_get("AudioSampleRate").unwrap_or(48),
        normalize_loudness_mode: r.try_get("NormalizeLoudnessMode").unwrap_or(0),
        deinterlace_video: r
            .try_get::<Option<i32>, _>("DeinterlaceVideo")
            .unwrap_or_default()
            .unwrap_or(1)
            != 0,
        vaapi_device: r
            .try_get::<Option<String>, _>("VaapiDevice")
            .unwrap_or_default()
            .unwrap_or_default(),
        vaapi_driver: r.try_get("VaapiDriver").unwrap_or(0),
        resolution_id: r.try_get("ResolutionId").unwrap_or(0),
    }))
}

/// Creates or updates a profile. Returns the profile id.
#[server]
pub async fn save_ffmpeg_profile(form: FfmpegProfileForm) -> Result<i64, ServerFnError> {
    use crate::state::AppState;

    let state = expect_context::<AppState>();

    let vaapi_device: Option<&str> = if form.vaapi_device.is_empty() {
        None
    } else {
        Some(&form.vaapi_device)
    };

    let deinterlace: Option<i32> = Some(i32::from(form.deinterlace_video));

    if form.id == 0 {
        // Resolve ResolutionId — seed a default 1080p entry if none exists.
        let resolution_id = match sqlx::query_scalar::<_, i64>(
            r#"SELECT "Id" FROM "Resolution" ORDER BY "Id" LIMIT 1"#,
        )
        .fetch_optional(&state.pool)
        .await?
        {
            Some(id) => id,
            None => sqlx::query(
                r#"INSERT INTO "Resolution" ("Name", "IsCustom", "Width", "Height")
                       VALUES ('1920x1080', 0, 1920, 1080)"#,
            )
            .execute(&state.pool)
            .await?
            .last_insert_rowid(),
        };

        let result = sqlx::query(
            r#"INSERT INTO "FFmpegProfile" (
                "Name", "ThreadCount", "HardwareAcceleration", "VideoFormat",
                "VideoBitrate", "VideoBufferSize", "AudioFormat", "AudioBitrate",
                "AudioChannels", "AudioSampleRate", "NormalizeLoudnessMode",
                "DeinterlaceVideo", "VaapiDevice", "VaapiDriver", "ResolutionId",
                "AudioBufferSize", "BitDepth", "ScalingBehavior",
                "NormalizeVideo", "NormalizeAudio", "NormalizeFramerate",
                "NormalizeColors", "AllowBFrames", "PadMode", "TonemapAlgorithm"
               ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0, 0, 0, 1, 1, 0, 1, 0, 0, 0)"#,
        )
        .bind(&form.name)
        .bind(form.thread_count)
        .bind(form.hardware_acceleration)
        .bind(form.video_format)
        .bind(form.video_bitrate)
        .bind(form.video_buffer_size)
        .bind(form.audio_format)
        .bind(form.audio_bitrate)
        .bind(form.audio_channels)
        .bind(form.audio_sample_rate)
        .bind(form.normalize_loudness_mode)
        .bind(deinterlace)
        .bind(vaapi_device)
        .bind(form.vaapi_driver)
        .bind(resolution_id)
        .execute(&state.pool)
        .await?;

        Ok(result.last_insert_rowid())
    } else {
        sqlx::query(
            r#"UPDATE "FFmpegProfile" SET
                "Name" = ?, "ThreadCount" = ?, "HardwareAcceleration" = ?,
                "VideoFormat" = ?, "VideoBitrate" = ?, "VideoBufferSize" = ?,
                "AudioFormat" = ?, "AudioBitrate" = ?, "AudioChannels" = ?,
                "AudioSampleRate" = ?, "NormalizeLoudnessMode" = ?,
                "DeinterlaceVideo" = ?, "VaapiDevice" = ?, "VaapiDriver" = ?
               WHERE "Id" = ?"#,
        )
        .bind(&form.name)
        .bind(form.thread_count)
        .bind(form.hardware_acceleration)
        .bind(form.video_format)
        .bind(form.video_bitrate)
        .bind(form.video_buffer_size)
        .bind(form.audio_format)
        .bind(form.audio_bitrate)
        .bind(form.audio_channels)
        .bind(form.audio_sample_rate)
        .bind(form.normalize_loudness_mode)
        .bind(deinterlace)
        .bind(vaapi_device)
        .bind(form.vaapi_driver)
        .bind(form.id)
        .execute(&state.pool)
        .await?;

        Ok(form.id)
    }
}

/// Deletes a profile by id.
#[server]
pub async fn delete_ffmpeg_profile(id: i64) -> Result<(), ServerFnError> {
    use crate::state::AppState;

    let state = expect_context::<AppState>();
    sqlx::query(r#"DELETE FROM "FFmpegProfile" WHERE "Id" = ?"#)
        .bind(id)
        .execute(&state.pool)
        .await?;
    Ok(())
}

/// Copies a profile, giving the copy `new_name`. Returns the new profile's id.
#[server]
pub async fn copy_ffmpeg_profile(id: i64, new_name: String) -> Result<i64, ServerFnError> {
    use crate::state::AppState;

    let state = expect_context::<AppState>();
    let result = sqlx::query(
        r#"INSERT INTO "FFmpegProfile" (
               "Name", "ThreadCount", "HardwareAcceleration", "VideoFormat",
               "VideoBitrate", "VideoBufferSize", "AudioFormat", "AudioBitrate",
               "AudioBufferSize", "AudioChannels", "AudioSampleRate",
               "NormalizeLoudnessMode", "DeinterlaceVideo", "VaapiDevice",
               "VaapiDriver", "VaapiDisplay", "ResolutionId", "BitDepth",
               "ScalingBehavior", "PadMode", "NormalizeVideo", "NormalizeAudio",
               "NormalizeFramerate", "NormalizeColors", "AllowBFrames",
               "TonemapAlgorithm", "QsvExtraHardwareFrames", "VideoProfile",
               "VideoPreset", "TargetLoudness"
           )
           SELECT
               ?, "ThreadCount", "HardwareAcceleration", "VideoFormat",
               "VideoBitrate", "VideoBufferSize", "AudioFormat", "AudioBitrate",
               "AudioBufferSize", "AudioChannels", "AudioSampleRate",
               "NormalizeLoudnessMode", "DeinterlaceVideo", "VaapiDevice",
               "VaapiDriver", "VaapiDisplay", "ResolutionId", "BitDepth",
               "ScalingBehavior", "PadMode", "NormalizeVideo", "NormalizeAudio",
               "NormalizeFramerate", "NormalizeColors", "AllowBFrames",
               "TonemapAlgorithm", "QsvExtraHardwareFrames", "VideoProfile",
               "VideoPreset", "TargetLoudness"
           FROM "FFmpegProfile"
           WHERE "Id" = ?"#,
    )
    .bind(&new_name)
    .bind(id)
    .execute(&state.pool)
    .await?;

    Ok(result.last_insert_rowid())
}
