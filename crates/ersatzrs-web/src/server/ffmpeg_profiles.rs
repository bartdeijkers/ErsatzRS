use leptos::prelude::*;
use serde::{Deserialize, Serialize};

/// Lightweight row for the `FFmpegProfile` list table.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct FfmpegProfileListRow {
    pub id: i64,
    pub name: String,
    pub resolution: String,
    pub video_format: String,
    pub audio_format: String,
}

/// Full form data for create / edit — mirrors `FFmpegProfile.New` defaults.
///
/// Enum columns use the same integer values as `ErsatzTV`'s C# enums:
/// - `HardwareAccelerationKind`: None=0, Qsv=1, Nvenc=2, Vaapi=3, …
/// - `FFmpegProfileVideoFormat`: None=0, H264=1, Hevc=2, Mpeg2Video=3, Av1=4, **Copy=99**
/// - `FFmpegProfileAudioFormat`: None=0, Aac=1, Ac3=2, AacLatm=3, **Copy=99**
/// - `FFmpegProfileBitDepth`: EightBit=0, TenBit=1
/// - `FFmpegProfileTonemapAlgorithm`: Linear=0, Clip=1, Gamma=2, Reinhard=3, Mobius=4, Hable=5
/// - `ScalingBehavior`: ScaleAndPad=0, Stretch=1, Crop=2
/// - `FilterMode` (pad mode): HardwareIfPossible=0, Software=1
/// - `VaapiDriver`: Default=0, IHD=1, I965=2, RadeonSI=3, Nouveau=4
/// - `NormalizeLoudnessMode`: Off=0, LoudNorm=1
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FfmpegProfileForm {
    pub id: i64,
    pub name: String,
    pub thread_count: i32,
    pub normalize_video: bool,
    pub normalize_audio: bool,
    pub hardware_acceleration: i32,
    pub vaapi_display: String,
    pub vaapi_driver: i32,
    pub vaapi_device: String,
    pub qsv_extra_hardware_frames: i32,
    pub resolution_id: i64,
    pub scaling_behavior: i32,
    pub pad_mode: i32,
    pub video_format: i32,
    pub video_profile: String,
    pub video_preset: String,
    pub allow_b_frames: bool,
    pub bit_depth: i32,
    pub video_bitrate: i32,
    pub video_buffer_size: i32,
    pub tonemap_algorithm: i32,
    pub audio_format: i32,
    pub audio_bitrate: i32,
    pub audio_buffer_size: i32,
    pub normalize_loudness_mode: i32,
    /// Stored as a string so the island can bind it directly to a text input.
    pub target_loudness: String,
    pub audio_channels: i32,
    pub audio_sample_rate: i32,
    pub normalize_framerate: bool,
    pub normalize_colors: bool,
    pub deinterlace_video: bool,
}

impl Default for FfmpegProfileForm {
    fn default() -> Self {
        Self::new_defaults()
    }
}

impl FfmpegProfileForm {
    /// Mirrors the defaults from `FFmpegProfile.New` in `ErsatzTV`.
    #[must_use]
    pub fn new_defaults() -> Self {
        Self {
            id: 0,
            name: String::new(),
            thread_count: 0,
            normalize_video: true,
            normalize_audio: true,
            hardware_acceleration: 0,
            vaapi_display: String::new(),
            vaapi_driver: 0,
            vaapi_device: String::new(),
            qsv_extra_hardware_frames: 64,
            resolution_id: 0,
            scaling_behavior: 0,
            pad_mode: 1,     // Software
            video_format: 1, // H264
            video_profile: "high".to_string(),
            video_preset: String::new(),
            allow_b_frames: false,
            bit_depth: 0, // EightBit
            video_bitrate: 2000,
            video_buffer_size: 4000,
            tonemap_algorithm: 0, // Linear
            audio_format: 1,      // Aac
            audio_bitrate: 192,
            audio_buffer_size: 384,
            normalize_loudness_mode: 0, // Off
            target_loudness: "-23.0".to_string(),
            audio_channels: 2,
            audio_sample_rate: 48,
            normalize_framerate: false,
            normalize_colors: true,
            deinterlace_video: true,
        }
    }
}

// ---------------------------------------------------------------------------
// Label helpers (C# enum integer → display string)
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
        99 => "Copy",
        _ => "None",
    }
}

fn af_label(v: i32) -> &'static str {
    match v {
        1 => "AAC",
        2 => "AC3",
        3 => "AAC (LATM)",
        99 => "Copy",
        _ => "None",
    }
}

// ---------------------------------------------------------------------------
// Preset helpers — ports ErsatzTV's `AvailablePresets.ForAccelAndFormat`
// ---------------------------------------------------------------------------

/// Returns the valid `VideoPreset` strings for a given hardware/format/depth
/// combination.  Mirrors `AvailablePresets.ForAccelAndFormat` exactly.
///
/// - `hw`: `HardwareAccelerationKind` integer (0=None, 1=QSV, 2=NVENC, …)
/// - `vf`: `FFmpegProfileVideoFormat` integer (1=H264, 2=HEVC, …)
/// - `bd`: `FFmpegProfileBitDepth` integer (0=EightBit, 1=TenBit)
pub fn presets_for_profile(hw: i32, vf: i32, bd: i32) -> Vec<&'static str> {
    match (hw, vf, bd) {
        // NVENC: H264 8-bit uses low-latency presets; HEVC uses them for any depth.
        // This arm must precede the H264/10-bit arm so NVENC+HEVC+10bit returns llhp/llhq.
        (2, 1, 0) | (2, 2, _) => vec!["llhp", "llhq"],
        // 10-bit H264 always uses libx264 veryfast (all accelerators, including NVENC).
        // QSV and software H264/HEVC also use veryfast.
        (_, 1, 1) | (0 | 1, 1 | 2, _) => vec!["veryfast"],
        // AV1, MPEG-2, Copy, VAAPI, VideoToolbox, AMF, V4L2M2M, RKMPP: no presets.
        _ => vec![],
    }
}

// ---------------------------------------------------------------------------
// Server functions
// ---------------------------------------------------------------------------

/// Returns paginated `FFmpegProfile` rows (Name, Resolution, VideoFormat, AudioFormat).
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
        r#"SELECT p."Id", p."Name", COALESCE(r."Name", '') AS "ResolutionName",
                  p."VideoFormat", p."AudioFormat", p."HardwareAcceleration"
           FROM "FFmpegProfile" p
           LEFT JOIN "Resolution" r ON r."Id" = p."ResolutionId"
           ORDER BY p."Name", p."Id"
           LIMIT ? OFFSET ?"#,
    )
    .bind(i64::from(page_size))
    .bind(offset)
    .fetch_all(&state.pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| {
            let hw: i32 = r.try_get("HardwareAcceleration").unwrap_or(0);
            let vf: i32 = r.try_get("VideoFormat").unwrap_or(0);
            let vf_str = if hw != 0 {
                format!("{} / {}", vf_label(vf), hw_label(hw))
            } else {
                vf_label(vf).to_string()
            };
            FfmpegProfileListRow {
                id: r.try_get("Id").unwrap_or(0),
                name: r.try_get("Name").unwrap_or_default(),
                resolution: r.try_get("ResolutionName").unwrap_or_default(),
                video_format: vf_str,
                audio_format: af_label(r.try_get("AudioFormat").unwrap_or(0)).to_string(),
            }
        })
        .collect())
}

/// Returns the total `FFmpegProfile` count.
#[server]
pub async fn count_ffmpeg_profiles() -> Result<u64, ServerFnError> {
    use crate::state::AppState;

    let state = expect_context::<AppState>();
    let count: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "FFmpegProfile""#)
        .fetch_one(&state.pool)
        .await?;
    Ok(u64::try_from(count).unwrap_or(0))
}

/// Returns `(id, name)` pairs for resolutions.
#[server]
pub async fn list_resolutions_for_select() -> Result<Vec<(i64, String)>, ServerFnError> {
    use crate::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();

    let rows =
        sqlx::query(r#"SELECT "Id", "Name" FROM "Resolution" ORDER BY "Height", "Width", "Id""#)
            .fetch_all(&state.pool)
            .await?;

    Ok(rows
        .iter()
        .map(|r| {
            (
                r.try_get("Id").unwrap_or(0),
                r.try_get::<Option<String>, _>("Name")
                    .unwrap_or_default()
                    .unwrap_or_default(),
            )
        })
        .collect())
}

/// Returns `(integer, label)` pairs for hardware acceleration kinds supported
/// by the local FFmpeg installation.  Ports `GetSupportedHardwareAccelerationKindsHandler`.
///
/// Uses the startup FFmpeg runtime report and maps reported accelerator names
/// to `HardwareAccelerationKind` integers.  Always includes `None (0)`.
/// V4L2M2M is excluded (commented out in the upstream handler too).
#[server]
#[allow(clippy::unused_async)]
pub async fn list_supported_hw_accelerations() -> Result<Vec<(i32, String)>, ServerFnError> {
    use crate::state::AppState;

    let state = expect_context::<AppState>();
    let capabilities = &state.ffmpeg_runtime.capabilities;
    let mut result = vec![(0i32, "None".to_string())];

    for (hwaccel, value, label) in [
        ("qsv", 1, "QSV"),
        ("cuda", 2, "NVENC"),
        ("vaapi", 3, "VAAPI"),
        ("videotoolbox", 4, "VideoToolbox"),
        ("amf", 5, "AMF"),
        ("rkmpp", 7, "RKMPP"),
    ] {
        if capabilities.has_hwaccel(hwaccel) {
            result.push((value, label.to_string()));
        }
    }

    Ok(result)
}

/// Loads a profile into [`FfmpegProfileForm`]. Returns `None` if not found.
#[server]
pub async fn get_ffmpeg_profile_form(id: i64) -> Result<Option<FfmpegProfileForm>, ServerFnError> {
    use crate::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();
    let maybe = sqlx::query(r#"SELECT * FROM "FFmpegProfile" WHERE "Id" = ?"#)
        .bind(id)
        .fetch_optional(&state.pool)
        .await?;

    Ok(maybe.map(|r| {
        let target: Option<f64> = r
            .try_get::<Option<f64>, _>("TargetLoudness")
            .unwrap_or_default();
        FfmpegProfileForm {
            id: r.try_get("Id").unwrap_or(0),
            name: r
                .try_get::<Option<String>, _>("Name")
                .unwrap_or_default()
                .unwrap_or_default(),
            thread_count: r.try_get("ThreadCount").unwrap_or(0),
            normalize_video: r.try_get::<i32, _>("NormalizeVideo").unwrap_or(1) != 0,
            normalize_audio: r.try_get::<i32, _>("NormalizeAudio").unwrap_or(1) != 0,
            hardware_acceleration: r.try_get("HardwareAcceleration").unwrap_or(0),
            vaapi_display: r
                .try_get::<Option<String>, _>("VaapiDisplay")
                .unwrap_or_default()
                .unwrap_or_default(),
            vaapi_driver: r.try_get("VaapiDriver").unwrap_or(0),
            vaapi_device: r
                .try_get::<Option<String>, _>("VaapiDevice")
                .unwrap_or_default()
                .unwrap_or_default(),
            qsv_extra_hardware_frames: r
                .try_get::<Option<i32>, _>("QsvExtraHardwareFrames")
                .unwrap_or_default()
                .unwrap_or(64),
            resolution_id: r.try_get("ResolutionId").unwrap_or(0),
            scaling_behavior: r.try_get("ScalingBehavior").unwrap_or(0),
            pad_mode: r.try_get("PadMode").unwrap_or(1),
            video_format: r.try_get("VideoFormat").unwrap_or(1),
            video_profile: r
                .try_get::<Option<String>, _>("VideoProfile")
                .unwrap_or_default()
                .unwrap_or_default(),
            video_preset: r
                .try_get::<Option<String>, _>("VideoPreset")
                .unwrap_or_default()
                .unwrap_or_default(),
            allow_b_frames: r.try_get::<i32, _>("AllowBFrames").unwrap_or(0) != 0,
            bit_depth: r.try_get("BitDepth").unwrap_or(0),
            video_bitrate: r.try_get("VideoBitrate").unwrap_or(2000),
            video_buffer_size: r.try_get("VideoBufferSize").unwrap_or(4000),
            tonemap_algorithm: r.try_get("TonemapAlgorithm").unwrap_or(0),
            audio_format: r.try_get("AudioFormat").unwrap_or(1),
            audio_bitrate: r.try_get("AudioBitrate").unwrap_or(192),
            audio_buffer_size: r.try_get("AudioBufferSize").unwrap_or(384),
            normalize_loudness_mode: r.try_get("NormalizeLoudnessMode").unwrap_or(0),
            // When LoudNorm is active and TargetLoudness is null, default to -16
            // (mirrors FFmpegProfileEditViewModel's NormalizeLoudnessMode setter).
            target_loudness: target.map_or_else(
                || {
                    if r.try_get::<i32, _>("NormalizeLoudnessMode").unwrap_or(0) == 1 {
                        "-16".to_string()
                    } else {
                        String::new()
                    }
                },
                |v| format!("{v}"),
            ),
            audio_channels: r.try_get("AudioChannels").unwrap_or(2),
            audio_sample_rate: r.try_get("AudioSampleRate").unwrap_or(48),
            normalize_framerate: r.try_get::<i32, _>("NormalizeFramerate").unwrap_or(0) != 0,
            normalize_colors: r.try_get::<i32, _>("NormalizeColors").unwrap_or(1) != 0,
            deinterlace_video: r
                .try_get::<Option<i32>, _>("DeinterlaceVideo")
                .unwrap_or_default()
                .unwrap_or(1)
                != 0,
        }
    }))
}

/// Validates and creates or updates a profile. Returns the profile id.
///
/// Server-side rules mirror `CreateFFmpegProfileHandler` / `UpdateFFmpegProfileHandler`:
/// - Name: non-empty, ≤ 50 chars, unique (excluding own id on update).
/// - `ThreadCount` ≥ 0.
/// - When `NormalizeVideo = false`, `VideoFormat` is forced to Copy (99) and
///   `HardwareAcceleration` to None (0).
/// - When `NormalizeAudio = false`, `AudioFormat` is forced to Copy (99).
/// - `PadMode` is Software (1) for None, VAAPI-controlled otherwise.
/// - `BitDepth` is forced to EightBit (0) for `Mpeg2Video`.
#[server]
#[allow(clippy::too_many_lines)]
pub async fn save_ffmpeg_profile(form: FfmpegProfileForm) -> Result<i64, ServerFnError> {
    use crate::state::AppState;

    let state = expect_context::<AppState>();

    // ── Validation ────────────────────────────────────────────────────────
    let name = form.name.trim().to_string();
    if name.is_empty() {
        return Err(ServerFnError::new("Profile name is required."));
    }
    if name.len() > 50 {
        return Err(ServerFnError::new(
            "Profile name must be 50 characters or fewer.",
        ));
    }
    if form.thread_count < 0 {
        return Err(ServerFnError::new("Thread count must be 0 or greater."));
    }

    // Duplicate name check.
    let dup: i64 = if form.id == 0 {
        sqlx::query_scalar(r#"SELECT COUNT(*) FROM "FFmpegProfile" WHERE "Name" = ?"#)
            .bind(&name)
            .fetch_one(&state.pool)
            .await?
    } else {
        sqlx::query_scalar(r#"SELECT COUNT(*) FROM "FFmpegProfile" WHERE "Name" = ? AND "Id" != ?"#)
            .bind(&name)
            .bind(form.id)
            .fetch_one(&state.pool)
            .await?
    };
    if dup > 0 {
        return Err(ServerFnError::new(format!(
            "An FFmpeg profile named \"{name}\" already exists."
        )));
    }

    // ── Derived values (mirror handler logic) ─────────────────────────────
    let hw_accel = if form.normalize_video {
        form.hardware_acceleration
    } else {
        0
    };
    // Copy = 99 when normalization is disabled.
    let video_format = if form.normalize_video {
        form.video_format
    } else {
        99
    };
    let audio_format = if form.normalize_audio {
        form.audio_format
    } else {
        99
    };

    // PadMode: None→Software(1); Vaapi→form value; others→HardwareIfPossible(0).
    let pad_mode: i32 = match hw_accel {
        0 => 1,
        3 => form.pad_mode,
        _ => 0,
    };

    // BitDepth: Mpeg2Video forces EightBit(0).
    let bit_depth: i32 = if form.video_format == 3 {
        0
    } else {
        form.bit_depth
    };

    let target_loudness: Option<f64> = if form.normalize_loudness_mode == 1 {
        form.target_loudness.trim().parse::<f64>().ok()
    } else {
        None
    };

    let vaapi_device: Option<&str> = if form.vaapi_device.is_empty() {
        None
    } else {
        Some(&form.vaapi_device)
    };
    let vaapi_display: Option<&str> = if form.vaapi_display.is_empty() {
        None
    } else {
        Some(&form.vaapi_display)
    };
    let video_profile: Option<&str> = if form.video_profile.is_empty() {
        None
    } else {
        Some(&form.video_profile)
    };
    // Clear preset if it is not valid for the resolved hw/format/depth combination,
    // mirroring the server-side clearing in UpdateFFmpegProfileHandler.cs.
    let valid_presets = presets_for_profile(hw_accel, video_format, bit_depth);
    let preset_str: String =
        if form.video_preset.is_empty() || !valid_presets.contains(&form.video_preset.as_str()) {
            String::new()
        } else {
            form.video_preset.clone()
        };
    let video_preset: Option<&str> = if preset_str.is_empty() {
        None
    } else {
        Some(&preset_str)
    };
    let deinterlace: Option<i32> = Some(i32::from(form.deinterlace_video));
    let qsv_frames: Option<i32> = if hw_accel == 1 {
        Some(form.qsv_extra_hardware_frames)
    } else {
        None
    };

    if form.id == 0 {
        // Resolve ResolutionId — seed a default 1080p entry if none exists.
        let resolution_id = if form.resolution_id > 0 {
            form.resolution_id
        } else {
            sqlx::query_scalar::<_, i64>(r#"SELECT "Id" FROM "Resolution" ORDER BY "Id" LIMIT 1"#)
                .fetch_optional(&state.pool)
                .await?
                .ok_or_else(|| {
                    ServerFnError::new(
                        "No FFmpeg resolutions are available; database bootstrap is incomplete",
                    )
                })?
        };

        let result = sqlx::query(
            r#"INSERT INTO "FFmpegProfile" (
                "Name", "ThreadCount", "NormalizeVideo", "NormalizeAudio",
                "HardwareAcceleration", "VaapiDisplay", "VaapiDriver", "VaapiDevice",
                "QsvExtraHardwareFrames", "ResolutionId", "ScalingBehavior", "PadMode",
                "VideoFormat", "VideoProfile", "VideoPreset", "AllowBFrames", "BitDepth",
                "VideoBitrate", "VideoBufferSize", "TonemapAlgorithm",
                "AudioFormat", "AudioBitrate", "AudioBufferSize",
                "NormalizeLoudnessMode", "TargetLoudness",
                "AudioChannels", "AudioSampleRate",
                "NormalizeFramerate", "NormalizeColors", "DeinterlaceVideo"
               ) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)"#,
        )
        .bind(&name)
        .bind(form.thread_count)
        .bind(i32::from(form.normalize_video))
        .bind(i32::from(form.normalize_audio))
        .bind(hw_accel)
        .bind(vaapi_display)
        .bind(form.vaapi_driver)
        .bind(vaapi_device)
        .bind(qsv_frames)
        .bind(resolution_id)
        .bind(form.scaling_behavior)
        .bind(pad_mode)
        .bind(video_format)
        .bind(video_profile)
        .bind(video_preset)
        .bind(i32::from(form.allow_b_frames))
        .bind(bit_depth)
        .bind(form.video_bitrate)
        .bind(form.video_buffer_size)
        .bind(form.tonemap_algorithm)
        .bind(audio_format)
        .bind(form.audio_bitrate)
        .bind(form.audio_buffer_size)
        .bind(form.normalize_loudness_mode)
        .bind(target_loudness)
        .bind(form.audio_channels)
        .bind(form.audio_sample_rate)
        .bind(i32::from(form.normalize_framerate))
        .bind(i32::from(form.normalize_colors))
        .bind(deinterlace)
        .execute(&state.pool)
        .await?;

        Ok(ersatzrs_infra::db::last_insert_id(result))
    } else {
        sqlx::query(
            r#"UPDATE "FFmpegProfile" SET
                "Name" = ?, "ThreadCount" = ?, "NormalizeVideo" = ?, "NormalizeAudio" = ?,
                "HardwareAcceleration" = ?, "VaapiDisplay" = ?, "VaapiDriver" = ?,
                "VaapiDevice" = ?, "QsvExtraHardwareFrames" = ?, "ResolutionId" = ?,
                "ScalingBehavior" = ?, "PadMode" = ?,
                "VideoFormat" = ?, "VideoProfile" = ?, "VideoPreset" = ?,
                "AllowBFrames" = ?, "BitDepth" = ?,
                "VideoBitrate" = ?, "VideoBufferSize" = ?, "TonemapAlgorithm" = ?,
                "AudioFormat" = ?, "AudioBitrate" = ?, "AudioBufferSize" = ?,
                "NormalizeLoudnessMode" = ?, "TargetLoudness" = ?,
                "AudioChannels" = ?, "AudioSampleRate" = ?,
                "NormalizeFramerate" = ?, "NormalizeColors" = ?, "DeinterlaceVideo" = ?
               WHERE "Id" = ?"#,
        )
        .bind(&name)
        .bind(form.thread_count)
        .bind(i32::from(form.normalize_video))
        .bind(i32::from(form.normalize_audio))
        .bind(hw_accel)
        .bind(vaapi_display)
        .bind(form.vaapi_driver)
        .bind(vaapi_device)
        .bind(qsv_frames)
        .bind(form.resolution_id)
        .bind(form.scaling_behavior)
        .bind(pad_mode)
        .bind(video_format)
        .bind(video_profile)
        .bind(video_preset)
        .bind(i32::from(form.allow_b_frames))
        .bind(bit_depth)
        .bind(form.video_bitrate)
        .bind(form.video_buffer_size)
        .bind(form.tonemap_algorithm)
        .bind(audio_format)
        .bind(form.audio_bitrate)
        .bind(form.audio_buffer_size)
        .bind(form.normalize_loudness_mode)
        .bind(target_loudness)
        .bind(form.audio_channels)
        .bind(form.audio_sample_rate)
        .bind(i32::from(form.normalize_framerate))
        .bind(i32::from(form.normalize_colors))
        .bind(deinterlace)
        .bind(form.id)
        .execute(&state.pool)
        .await?;

        Ok(form.id)
    }
}

/// Deletes an `FFmpegProfile` by id.
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
///
/// Validates that `new_name` is non-empty and ≤ 50 characters. Returns an error
/// if the source profile is not found (zero rows inserted).
#[server]
pub async fn copy_ffmpeg_profile(id: i64, new_name: String) -> Result<i64, ServerFnError> {
    use crate::state::AppState;

    let name = new_name.trim().to_string();
    if name.is_empty() {
        return Err(ServerFnError::new("Profile name is required."));
    }
    if name.len() > 50 {
        return Err(ServerFnError::new(
            "Profile name must be 50 characters or fewer.",
        ));
    }

    let state = expect_context::<AppState>();
    let result = sqlx::query(
        r#"INSERT INTO "FFmpegProfile" (
               "Name", "ThreadCount", "NormalizeVideo", "NormalizeAudio",
               "HardwareAcceleration", "VaapiDisplay", "VaapiDriver", "VaapiDevice",
               "QsvExtraHardwareFrames", "ResolutionId", "ScalingBehavior", "PadMode",
               "VideoFormat", "VideoProfile", "VideoPreset", "AllowBFrames", "BitDepth",
               "VideoBitrate", "VideoBufferSize", "TonemapAlgorithm",
               "AudioFormat", "AudioBitrate", "AudioBufferSize",
               "NormalizeLoudnessMode", "TargetLoudness",
               "AudioChannels", "AudioSampleRate",
               "NormalizeFramerate", "NormalizeColors", "DeinterlaceVideo"
           )
           SELECT
               ?, "ThreadCount", "NormalizeVideo", "NormalizeAudio",
               "HardwareAcceleration", "VaapiDisplay", "VaapiDriver", "VaapiDevice",
               "QsvExtraHardwareFrames", "ResolutionId", "ScalingBehavior", "PadMode",
               "VideoFormat", "VideoProfile", "VideoPreset", "AllowBFrames", "BitDepth",
               "VideoBitrate", "VideoBufferSize", "TonemapAlgorithm",
               "AudioFormat", "AudioBitrate", "AudioBufferSize",
               "NormalizeLoudnessMode", "TargetLoudness",
               "AudioChannels", "AudioSampleRate",
               "NormalizeFramerate", "NormalizeColors", "DeinterlaceVideo"
           FROM "FFmpegProfile"
           WHERE "Id" = ?"#,
    )
    .bind(&name)
    .bind(id)
    .execute(&state.pool)
    .await?;

    if result.rows_affected() == 0 {
        return Err(ServerFnError::new("Source profile not found."));
    }

    Ok(ersatzrs_infra::db::last_insert_id(result))
}
