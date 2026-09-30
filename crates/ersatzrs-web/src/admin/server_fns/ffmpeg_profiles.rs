use ersatzrs_domain::ffmpeg::{ContentLightLevel, MasteringDisplayMetadata};
use leptos::prelude::*;
use serde::{Deserialize, Serialize};

/// Lightweight row for the `FFmpegProfile` list table.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct FfmpegProfileListRow {
    pub id: i64,
    pub name: String,
    pub resolution: String,
    pub video_format: i32,
    pub audio_format: i32,
    pub hardware_acceleration: i32,
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
    pub transcode_video: bool,
    pub transcode_audio: bool,
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
    pub video_tune: String,
    pub allow_b_frames: bool,
    pub bit_depth: i32,
    pub video_bitrate: i32,
    pub video_buffer_size: i32,
    pub tonemap_algorithm: i32,
    /// ADR 0064 output dynamic range: 0 = SDR BT.709, 1 = HDR10 BT.2020/PQ.
    pub output_dynamic_range: i32,
    /// ADR 0064 mastering-display override; blank uses the documented default.
    pub hdr_mastering_display: String,
    /// ADR 0064 `MaxCLL` override in cd/m²; 0 uses the documented default.
    pub hdr_max_cll: i32,
    /// ADR 0064 `MaxFALL` override in cd/m²; 0 uses the documented default.
    pub hdr_max_fall: i32,
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
            transcode_video: true,
            transcode_audio: true,
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
            video_tune: String::new(),
            allow_b_frames: false,
            bit_depth: 0, // EightBit
            video_bitrate: 2000,
            video_buffer_size: 4000,
            tonemap_algorithm: 0,    // Linear
            output_dynamic_range: 0, // SdrBt709
            hdr_mastering_display: String::new(),
            hdr_max_cll: 0,
            hdr_max_fall: 0,
            audio_format: 1, // Aac
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

#[cfg(feature = "ssr")]
fn ffmpeg_profile_form_into_admin(
    form: FfmpegProfileForm,
) -> ersatzrs_app::commands::FfmpegProfileAdminForm {
    ersatzrs_app::commands::FfmpegProfileAdminForm {
        id: form.id,
        name: form.name,
        thread_count: form.thread_count,
        transcode_video: form.transcode_video,
        transcode_audio: form.transcode_audio,
        hardware_acceleration: form.hardware_acceleration,
        vaapi_display: form.vaapi_display,
        vaapi_driver: form.vaapi_driver,
        vaapi_device: form.vaapi_device,
        qsv_extra_hardware_frames: form.qsv_extra_hardware_frames,
        resolution_id: form.resolution_id,
        scaling_behavior: form.scaling_behavior,
        pad_mode: form.pad_mode,
        video_format: form.video_format,
        video_profile: form.video_profile,
        video_preset: form.video_preset,
        video_tune: form.video_tune,
        allow_b_frames: form.allow_b_frames,
        bit_depth: form.bit_depth,
        video_bitrate: form.video_bitrate,
        video_buffer_size: form.video_buffer_size,
        tonemap_algorithm: form.tonemap_algorithm,
        output_dynamic_range: form.output_dynamic_range,
        hdr_mastering_display: form.hdr_mastering_display,
        hdr_max_cll: form.hdr_max_cll,
        hdr_max_fall: form.hdr_max_fall,
        audio_format: form.audio_format,
        audio_bitrate: form.audio_bitrate,
        audio_buffer_size: form.audio_buffer_size,
        normalize_loudness_mode: form.normalize_loudness_mode,
        target_loudness: form.target_loudness,
        audio_channels: form.audio_channels,
        audio_sample_rate: form.audio_sample_rate,
        normalize_framerate: form.normalize_framerate,
        normalize_colors: form.normalize_colors,
        deinterlace_video: form.deinterlace_video,
    }
}

#[cfg(feature = "ssr")]
fn ffmpeg_profile_form_from_admin(
    form: ersatzrs_app::commands::FfmpegProfileAdminForm,
) -> FfmpegProfileForm {
    FfmpegProfileForm {
        id: form.id,
        name: form.name,
        thread_count: form.thread_count,
        transcode_video: form.transcode_video,
        transcode_audio: form.transcode_audio,
        hardware_acceleration: form.hardware_acceleration,
        vaapi_display: form.vaapi_display,
        vaapi_driver: form.vaapi_driver,
        vaapi_device: form.vaapi_device,
        qsv_extra_hardware_frames: form.qsv_extra_hardware_frames,
        resolution_id: form.resolution_id,
        scaling_behavior: form.scaling_behavior,
        pad_mode: form.pad_mode,
        video_format: form.video_format,
        video_profile: form.video_profile,
        video_preset: form.video_preset,
        video_tune: form.video_tune,
        allow_b_frames: form.allow_b_frames,
        bit_depth: form.bit_depth,
        video_bitrate: form.video_bitrate,
        video_buffer_size: form.video_buffer_size,
        tonemap_algorithm: form.tonemap_algorithm,
        output_dynamic_range: form.output_dynamic_range,
        hdr_mastering_display: form.hdr_mastering_display,
        hdr_max_cll: form.hdr_max_cll,
        hdr_max_fall: form.hdr_max_fall,
        audio_format: form.audio_format,
        audio_bitrate: form.audio_bitrate,
        audio_buffer_size: form.audio_buffer_size,
        normalize_loudness_mode: form.normalize_loudness_mode,
        target_loudness: form.target_loudness,
        audio_channels: form.audio_channels,
        audio_sample_rate: form.audio_sample_rate,
        normalize_framerate: form.normalize_framerate,
        normalize_colors: form.normalize_colors,
        deinterlace_video: form.deinterlace_video,
    }
}

// ---------------------------------------------------------------------------
// Label helpers (C# enum integer → display string)
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Preset helpers — ports ErsatzTV's `AvailablePresets.ForAccelAndFormat`
// ---------------------------------------------------------------------------

/// Returns the valid `FFmpeg` 9 `VideoPreset` strings for a profile.
///
/// - `hw`: `HardwareAccelerationKind` integer (0=None, 1=QSV, 2=NVENC, …)
/// - `vf`: `FFmpegProfileVideoFormat` integer (1=H264, 2=HEVC, …)
/// - `bd`: `FFmpegProfileBitDepth` integer (0=EightBit, 1=TenBit)
pub fn presets_for_profile(hw: i32, vf: i32, bd: i32) -> Vec<&'static str> {
    match (hw, vf, bd) {
        (2, 1 | 2 | 4, _) => vec!["p1", "p2", "p3", "p4", "p5", "p6", "p7"],
        // 10-bit H.264 always encodes through libx264, whatever the profile's
        // acceleration says.
        (_, 1, 1) | (0, 1 | 2, _) => vec![
            "ultrafast",
            "superfast",
            "veryfast",
            "faster",
            "fast",
            "medium",
            "slow",
            "slower",
            "veryslow",
        ],
        (1, 1 | 2, _) => vec![
            "veryfast", "faster", "fast", "medium", "slow", "slower", "veryslow",
        ],
        _ => vec![],
    }
}

/// Returns the valid `FFmpeg` 9 NVENC tune strings for a profile.
///
/// `uhq` is codec-gated: `FFmpeg` 9's `hevc_nvenc` and `av1_nvenc` advertise it,
/// `h264_nvenc` does not.
pub fn tunes_for_profile(hw: i32, vf: i32) -> Vec<&'static str> {
    match (hw, vf) {
        (2, 1) => vec!["hq", "ll", "ull", "lossless"],
        (2, 2 | 4) => vec!["hq", "uhq", "ll", "ull", "lossless"],
        _ => vec![],
    }
}

pub const VIDEO_PROFILE_MAIN: &str = "main";
pub const VIDEO_PROFILE_HIGH: &str = "high";
pub const VIDEO_PROFILE_HIGH10: &str = "high10";
pub const VIDEO_PROFILE_HIGH444P: &str = "high444p";

pub const NVENC_FORMATS_ERROR: &str = "NVENC supports formats (h264, hevc, av1)";

fn hardware_accel_supports_h264_profile(hw: i32) -> bool {
    matches!(hw, 0..=4)
}

#[must_use]
pub fn video_profile_options(hw: i32, vf: i32) -> Vec<&'static str> {
    if vf != 1 || !hardware_accel_supports_h264_profile(hw) {
        return vec![];
    }

    let mut options = vec![VIDEO_PROFILE_MAIN, VIDEO_PROFILE_HIGH];
    if hw == 2 {
        options.push(VIDEO_PROFILE_HIGH444P);
    } else {
        options.push(VIDEO_PROFILE_HIGH10);
    }
    options
}

#[must_use]
pub fn video_format_validation_error(hw: i32, vf: i32) -> Option<&'static str> {
    match hw {
        1 if !matches!(vf, 1..=4) => Some("QSV supports formats (h264, hevc, mpeg2video)"),
        2 if !matches!(vf, 1 | 2 | 4) => Some(NVENC_FORMATS_ERROR),
        3 if !matches!(vf, 1..=4) => Some("VAAPI supports formats (h264, hevc, av1, mpeg2video)"),
        4 if !matches!(vf, 1 | 2) => Some("VideoToolbox supports formats (h264, hevc)"),
        5 if !matches!(vf, 1 | 2) => Some("Amf supports formats (h264, hevc)"),
        6 if !matches!(vf, 1 | 2) => Some("V4L2 M2M supports formats (h264, hevc)"),
        7 if !matches!(vf, 1 | 2) => Some("Rkmpp supports formats (h264, hevc)"),
        _ => None,
    }
}

/// Client-side mirror of the application layer's ADR 0064 HDR10 rules.
///
/// Kept in lockstep with `validate_output_dynamic_range` in
/// `ersatzrs-app::commands` so the editor can surface the same rejection before
/// a round trip; the server remains the authority.
#[must_use]
pub fn output_dynamic_range_validation_error(
    output_dynamic_range: i32,
    video_format: i32,
    bit_depth: i32,
) -> Option<&'static str> {
    if output_dynamic_range != 1 {
        return None;
    }
    if !matches!(video_format, 2 | 4) {
        return Some(HDR10_CODEC_ERROR);
    }
    if bit_depth != 1 {
        return Some(HDR10_BIT_DEPTH_ERROR);
    }
    None
}

/// The documented ADR 0064 default, shown as the mastering-display placeholder
/// so an operator can see the effective value without having to guess what a
/// blank field means.
pub const DOCUMENTED_MASTERING_DISPLAY: &str =
    "G(8500,39850)B(6550,2300)R(35400,14600)WP(15635,16450)L(10000000,1)";

/// Client-side mirror of the application layer's mastering-display rule.
///
/// Kept in lockstep with `parse_hdr_mastering_display` in `ersatzrs-app`.
/// Blank is always accepted: it selects the documented default.
#[must_use]
pub fn hdr_mastering_display_validation_error(
    output_dynamic_range: i32,
    value: &str,
) -> Option<&'static str> {
    let trimmed = value.trim();
    if output_dynamic_range != 1 || trimmed.is_empty() {
        return None;
    }
    MasteringDisplayMetadata::parse(trimmed)
        .is_none()
        .then_some(HDR10_MASTERING_DISPLAY_ERROR)
}

/// Client-side mirror of the application layer's content-light rule.
///
/// Kept in lockstep with `parse_hdr_content_light` in `ersatzrs-app`. Both
/// levels empty is accepted: that selects the documented default.
#[must_use]
pub fn hdr_content_light_validation_error(
    output_dynamic_range: i32,
    max_cll: i32,
    max_fall: i32,
) -> Option<&'static str> {
    if output_dynamic_range != 1 || (max_cll <= 0 && max_fall <= 0) {
        return None;
    }
    // Both must be positive: in a form, a blank field and a measured zero are
    // the same value. Kept in step with `parse_hdr_content_light`.
    let valid = positive_content_light_level(max_cll)
        .zip(positive_content_light_level(max_fall))
        .is_some_and(|(max_cll, max_fall)| ContentLightLevel::new(max_cll, max_fall).is_some());
    (!valid).then_some(HDR10_CONTENT_LIGHT_ERROR)
}

/// A content light level an operator actually entered, as opposed to a blank
/// field that the form reports as `0`.
const fn positive_content_light_level(level: i32) -> Option<u32> {
    if level > 0 {
        Some(level.unsigned_abs())
    } else {
        None
    }
}

/// Shared with `ersatzrs-app` so both sides render the same message.
pub const HDR10_MASTERING_DISPLAY_ERROR: &str =
    "Mastering display must look like G(8500,39850)B(6550,2300)R(35400,14600)WP(15635,16450)L(10000000,1), with chromaticities up to 50000 and max luminance above min";
/// Shared with `ersatzrs-app` so both sides render the same message.
pub const HDR10_CONTENT_LIGHT_ERROR: &str =
    "MaxCLL and MaxFALL must both be set (or both left empty), at most 65535, with MaxFALL no greater than MaxCLL";
/// Shared with `ersatzrs-app` so both sides render the same message.
pub const HDR10_CODEC_ERROR: &str =
    "HDR10 output requires hevc or av1; h264 and mpeg2video cannot carry HDR10 static metadata";
/// Shared with `ersatzrs-app` so both sides render the same message.
pub const HDR10_BIT_DEPTH_ERROR: &str =
    "HDR10 output requires 10-bit; 8-bit cannot represent a PQ signal without banding";

#[cfg(all(test, feature = "ssr"))]
fn normalized_video_profile_for_persist(
    hw: i32,
    vf: i32,
    bit_depth: i32,
    requested: &str,
) -> Option<String> {
    if vf != 1 {
        return None;
    }

    if hw == 2 && bit_depth == 1 {
        return Some(VIDEO_PROFILE_HIGH444P.to_owned());
    }

    let requested = requested.trim();
    if requested.is_empty() {
        return None;
    }

    video_profile_options(hw, vf)
        .contains(&requested)
        .then(|| requested.to_owned())
}

#[cfg(all(test, feature = "ssr"))]
fn validate_profile_options(
    hw: i32,
    vf: i32,
    bit_depth: i32,
    video_profile: &str,
) -> Result<(), &'static str> {
    if let Some(error) = video_format_validation_error(hw, vf) {
        return Err(error);
    }

    if vf == 3 && bit_depth == 1 {
        return Err("Mpeg2Video does not support 10-bit content");
    }

    if vf == 1 && bit_depth == 1 && hw == 2 && video_profile.trim() != VIDEO_PROFILE_HIGH444P {
        return Err("VideoProfile must be high444p with NVIDIA 10-bit h264");
    }

    if vf == 1 && bit_depth == 1 && hw != 2 && video_profile.trim() != VIDEO_PROFILE_HIGH10 {
        return Err("VideoProfile must be high10 with 10-bit h264");
    }

    if vf == 1 && bit_depth == 0 && video_profile.trim() == VIDEO_PROFILE_HIGH10 {
        return Err("VideoProfile cannot be high10 with 8-bit h264");
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Server functions
// ---------------------------------------------------------------------------

/// Returns paginated `FFmpegProfile` rows (Name, Resolution, VideoFormat, AudioFormat).
#[server(prefix = "/admin-api")]
pub async fn list_ffmpeg_profile_rows(
    page: u32,
    page_size: u32,
) -> Result<Vec<FfmpegProfileListRow>, ServerFnError> {
    use crate::host::state::AppState;
    let state = expect_context::<AppState>();
    state
        .application
        .list_ffmpeg_profiles(page, page_size)
        .await
        .map(|rows| {
            rows.into_iter()
                .map(|row| FfmpegProfileListRow {
                    id: row.id,
                    name: row.name,
                    resolution: row.resolution,
                    video_format: row.video_format,
                    audio_format: row.audio_format,
                    hardware_acceleration: row.hardware_acceleration,
                })
                .collect()
        })
        .map_err(ServerFnError::new)
}

/// Returns the total `FFmpegProfile` count.
#[server(prefix = "/admin-api")]
pub async fn count_ffmpeg_profiles() -> Result<u64, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    state
        .application
        .count_ffmpeg_profiles()
        .await
        .map_err(ServerFnError::new)
}

/// Returns `(id, name)` pairs for resolutions.
#[server(prefix = "/admin-api")]
pub async fn list_resolutions_for_select() -> Result<Vec<(i64, String)>, ServerFnError> {
    use crate::host::state::AppState;
    let state = expect_context::<AppState>();
    state
        .application
        .list_ffmpeg_resolutions()
        .await
        .map_err(ServerFnError::new)
}

/// Returns all hardware acceleration kinds, annotating each with whether it is
/// detected in the local FFmpeg installation.
///
/// All standard options are always present so users can configure a profile for
/// hardware that will be available at playout time even when the current host
/// lacks that hardware. The availability suffix is rendered at the page
/// boundary so it follows the active UI language.
///
/// V4L2M2M is excluded (commented out in the upstream handler too).
#[server(prefix = "/admin-api")]
#[allow(clippy::unused_async)]
pub async fn list_supported_hw_accelerations(
) -> Result<Vec<HardwareAccelerationOption>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    let runtime = state.ffmpeg_runtime.read().await;
    Ok(hw_acceleration_select_items(&runtime.capabilities))
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct HardwareAccelerationOption {
    pub value: i32,
    pub label: String,
    pub supported: bool,
}

#[cfg(feature = "ssr")]
fn hw_acceleration_select_items(
    capabilities: &ersatzrs_ffmpeg::FfmpegCapabilityReport,
) -> Vec<HardwareAccelerationOption> {
    [
        (0i32, "None", true),
        (
            1,
            "QSV",
            capabilities.has_hwaccel("qsv")
                || capabilities.has_encoder("h264_qsv")
                || capabilities.has_encoder("hevc_qsv")
                || capabilities.has_encoder("av1_qsv"),
        ),
        (
            2,
            "NVENC",
            capabilities.has_hwaccel("cuda")
                || capabilities.has_encoder("h264_nvenc")
                || capabilities.has_encoder("hevc_nvenc")
                || capabilities.has_encoder("av1_nvenc"),
        ),
        (
            3,
            "VAAPI",
            capabilities.has_hwaccel("vaapi")
                || capabilities.has_encoder("h264_vaapi")
                || capabilities.has_encoder("hevc_vaapi")
                || capabilities.has_encoder("av1_vaapi"),
        ),
        (4, "VideoToolbox", capabilities.has_hwaccel("videotoolbox")),
        (
            5,
            "AMF",
            capabilities.has_encoder("h264_amf")
                || capabilities.has_encoder("hevc_amf")
                || capabilities.has_encoder("av1_amf"),
        ),
        (7, "RKMPP", capabilities.has_hwaccel("rkmpp")),
    ]
    .into_iter()
    .map(|(value, label, supported)| HardwareAccelerationOption {
        value,
        label: label.to_owned(),
        supported,
    })
    .collect()
}

/// Loads a profile into [`FfmpegProfileForm`]. Returns `None` if not found.
#[server(prefix = "/admin-api")]
pub async fn get_ffmpeg_profile_form(id: i64) -> Result<Option<FfmpegProfileForm>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    state
        .application
        .get_ffmpeg_profile(id)
        .await
        .map(|form| form.map(ffmpeg_profile_form_from_admin))
        .map_err(ServerFnError::new)
}

/// `QsvExtraHardwareFrames` is only meaningful for QSV acceleration, so it is
/// persisted as a value only when QSV (1) is selected and `NULL` otherwise. The
/// editor form always carries the field (see [`FfmpegProfileForm`]); this is the
/// single place that decides whether it reaches the database.
#[cfg(all(test, feature = "ssr"))]
fn qsv_extra_hardware_frames_for_persist(
    hardware_acceleration: i32,
    qsv_extra_hardware_frames: i32,
) -> Option<i32> {
    (hardware_acceleration == 1).then_some(qsv_extra_hardware_frames)
}

/// Validates and creates or updates a profile. Returns the profile id.
///
/// Server-side rules mirror `CreateFFmpegProfileHandler` / `UpdateFFmpegProfileHandler`:
/// - Name: non-empty, ≤ 50 chars, unique (excluding own id on update).
/// - `ThreadCount` ≥ 0.
/// - When `TranscodeVideo = false`, `VideoFormat` is forced to Copy (99) and
///   `HardwareAcceleration` to None (0).
/// - When `TranscodeAudio = false`, `AudioFormat` is forced to Copy (99).
/// - `PadMode` is Software (1) for None, VAAPI-controlled otherwise.
/// - `BitDepth` is forced to EightBit (0) for `Mpeg2Video`.
#[server(prefix = "/admin-api")]
pub async fn save_ffmpeg_profile(form: FfmpegProfileForm) -> Result<i64, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    state
        .application
        .save_ffmpeg_profile(ersatzrs_app::commands::SaveFfmpegProfile {
            form: ffmpeg_profile_form_into_admin(form),
        })
        .await
        .map_err(ServerFnError::new)
}

#[cfg(all(test, feature = "ssr"))]
#[allow(dead_code)]
#[allow(clippy::too_many_lines)]
async fn save_ffmpeg_profile_legacy_sql(form: FfmpegProfileForm) -> Result<i64, ServerFnError> {
    use crate::host::state::AppState;

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
    let hw_accel = if form.transcode_video {
        form.hardware_acceleration
    } else {
        0
    };
    // Copy = 99 when video transcoding is disabled.
    let video_format = if form.transcode_video {
        form.video_format
    } else {
        99
    };
    let audio_format = if form.transcode_audio {
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

    validate_profile_options(hw_accel, video_format, form.bit_depth, &form.video_profile)
        .map_err(ServerFnError::new)?;

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
    let profile_str = normalized_video_profile_for_persist(
        hw_accel,
        video_format,
        bit_depth,
        &form.video_profile,
    );
    let video_profile = profile_str.as_deref();
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
    let valid_tunes = tunes_for_profile(hw_accel, video_format);
    let video_tune = valid_tunes
        .contains(&form.video_tune.as_str())
        .then_some(form.video_tune.as_str())
        .filter(|value| !value.is_empty());
    let deinterlace: Option<i32> = Some(i32::from(form.deinterlace_video));
    let qsv_frames: Option<i32> =
        qsv_extra_hardware_frames_for_persist(hw_accel, form.qsv_extra_hardware_frames);

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
                "Name", "ThreadCount", "TranscodeVideo", "TranscodeAudio",
                "HardwareAcceleration", "VaapiDisplay", "VaapiDriver", "VaapiDevice",
                "QsvExtraHardwareFrames", "ResolutionId", "ScalingBehavior", "PadMode",
                "VideoFormat", "VideoProfile", "VideoPreset", "VideoTune", "AllowBFrames", "BitDepth",
                "VideoBitrate", "VideoBufferSize", "TonemapAlgorithm",
                "AudioFormat", "AudioBitrate", "AudioBufferSize",
                "NormalizeLoudnessMode", "TargetLoudness",
                "AudioChannels", "AudioSampleRate",
                "NormalizeFramerate", "NormalizeColors", "DeinterlaceVideo"
               ) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)"#,
        )
        .bind(&name)
        .bind(form.thread_count)
        .bind(i32::from(form.transcode_video))
        .bind(i32::from(form.transcode_audio))
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
        .bind(video_tune)
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
                "Name" = ?, "ThreadCount" = ?, "TranscodeVideo" = ?, "TranscodeAudio" = ?,
                "HardwareAcceleration" = ?, "VaapiDisplay" = ?, "VaapiDriver" = ?,
                "VaapiDevice" = ?, "QsvExtraHardwareFrames" = ?, "ResolutionId" = ?,
                "ScalingBehavior" = ?, "PadMode" = ?,
                "VideoFormat" = ?, "VideoProfile" = ?, "VideoPreset" = ?, "VideoTune" = ?,
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
        .bind(i32::from(form.transcode_video))
        .bind(i32::from(form.transcode_audio))
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
        .bind(video_tune)
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
#[server(prefix = "/admin-api")]
pub async fn delete_ffmpeg_profile(id: i64) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;
    use ersatzrs_app::commands::DeleteFfmpegProfile;

    let state = expect_context::<AppState>();
    state
        .application
        .delete_ffmpeg_profile(DeleteFfmpegProfile { id })
        .await
        .map_err(|error| match error {
            ersatzrs_app::ApplicationError::Validation {
                field: "default_profile_id",
                ..
            } => ServerFnError::new("ffmpeg-profile-delete-default"),
            ersatzrs_app::ApplicationError::Conflict(_) => {
                ServerFnError::new("ffmpeg-profile-delete-in-use")
            }
            _ => ServerFnError::new("ffmpeg-profile-delete-failed"),
        })
}

/// Copies a profile, giving the copy `new_name`. Returns the new profile's id.
///
/// Validates that `new_name` is non-empty and ≤ 50 characters. Returns an error
/// if the source profile is not found (zero rows inserted).
#[server(prefix = "/admin-api")]
pub async fn copy_ffmpeg_profile(id: i64, new_name: String) -> Result<i64, ServerFnError> {
    use crate::host::state::AppState;
    use ersatzrs_app::commands::CopyFfmpegProfile;

    let state = expect_context::<AppState>();
    state
        .application
        .copy_ffmpeg_profile(CopyFfmpegProfile { id, name: new_name })
        .await
        .map_err(ServerFnError::new)
}

#[cfg(all(test, feature = "ssr"))]
mod tests {
    use super::*;
    use ersatzrs_ffmpeg::FfmpegCapabilityReport;
    use std::collections::BTreeSet;

    /// The editor's option lists and the application-layer validation lists are
    /// separate copies — `ersatzrs-app` is excluded from the wasm32 build, so
    /// the island code in `editor.rs` cannot call it. They must still agree:
    /// `save_ffmpeg_profile` drops any preset or tune the validation list does
    /// not contain, so an option offered here but missing there is silently
    /// discarded on save with no error shown to the operator.
    #[test]
    fn editor_option_lists_match_application_validation() {
        // 0=None, 1=QSV, 2=NVENC, 3=VAAPI, 4=VideoToolbox, 5=AMF, 6=v4l2m2m, 7=RKMPP
        for hw in 0..=7 {
            // 0=None, 1=H264, 2=HEVC, 3=Mpeg2Video, 4=AV1, 99=Copy
            for vf in [0, 1, 2, 3, 4, 99] {
                assert_eq!(
                    tunes_for_profile(hw, vf),
                    ersatzrs_app::commands::ffmpeg_tunes(hw, vf),
                    "tune list drift at hw={hw} vf={vf}"
                );
                for bd in [0, 1] {
                    assert_eq!(
                        presets_for_profile(hw, vf, bd),
                        ersatzrs_app::commands::ffmpeg_presets(hw, vf, bd),
                        "preset list drift at hw={hw} vf={vf} bd={bd}"
                    );
                }
            }
        }
    }

    #[test]
    fn nvenc_uhq_tune_is_codec_gated() {
        // FFmpeg 9 advertises `uhq` on hevc_nvenc and av1_nvenc but not on
        // h264_nvenc, whose tune range is 1-4.
        assert!(!tunes_for_profile(2, 1).contains(&"uhq"));
        assert!(tunes_for_profile(2, 2).contains(&"uhq"));
        assert!(tunes_for_profile(2, 4).contains(&"uhq"));
    }

    #[test]
    fn software_and_qsv_presets_expose_the_full_ffmpeg_9_ladders() {
        // ADR 0017 v5: ErsatzTV offered only `veryfast` here.
        assert!(presets_for_profile(0, 1, 0).contains(&"slow"));
        assert!(presets_for_profile(0, 2, 0).contains(&"veryslow"));
        // `placebo` is never offered: unusable in a realtime encoder.
        assert!(!presets_for_profile(0, 1, 0).contains(&"placebo"));
        // QSV has no `ultrafast`/`superfast` target usage.
        assert!(!presets_for_profile(1, 1, 0).contains(&"ultrafast"));
        assert!(presets_for_profile(1, 2, 0).contains(&"medium"));
    }

    #[test]
    fn qsv_extra_hardware_frames_persisted_only_for_qsv() {
        // QSV (1) carries the field through to the database.
        assert_eq!(qsv_extra_hardware_frames_for_persist(1, 96), Some(96));
        // Every other acceleration persists NULL regardless of the form value.
        for hw_accel in [0, 2, 3, 4, 5, 7] {
            assert_eq!(
                qsv_extra_hardware_frames_for_persist(hw_accel, 96),
                None,
                "hw_accel {hw_accel} must not persist QsvExtraHardwareFrames"
            );
        }
    }

    #[test]
    fn h264_video_profile_options_follow_ersatztv_hardware_rules() {
        assert_eq!(
            video_profile_options(0, 1),
            vec![VIDEO_PROFILE_MAIN, VIDEO_PROFILE_HIGH, VIDEO_PROFILE_HIGH10]
        );
        assert_eq!(
            video_profile_options(2, 1),
            vec![
                VIDEO_PROFILE_MAIN,
                VIDEO_PROFILE_HIGH,
                VIDEO_PROFILE_HIGH444P
            ]
        );
        assert!(video_profile_options(5, 1).is_empty());
        assert!(video_profile_options(2, 2).is_empty());
    }

    #[test]
    fn nvenc_mpeg2_validation_matches_upstream_warning() {
        assert_eq!(
            video_format_validation_error(2, 3),
            Some(NVENC_FORMATS_ERROR)
        );
        assert!(validate_profile_options(2, 3, 0, VIDEO_PROFILE_HIGH).is_err());
        assert!(validate_profile_options(2, 1, 0, VIDEO_PROFILE_HIGH444P).is_ok());
    }

    #[test]
    fn video_profile_persist_normalizes_to_h264_profile_contract() {
        assert_eq!(
            normalized_video_profile_for_persist(2, 1, 1, VIDEO_PROFILE_HIGH),
            Some(VIDEO_PROFILE_HIGH444P.to_string())
        );
        assert_eq!(
            normalized_video_profile_for_persist(0, 1, 0, VIDEO_PROFILE_MAIN),
            Some(VIDEO_PROFILE_MAIN.to_string())
        );
        assert_eq!(
            normalized_video_profile_for_persist(0, 2, 0, VIDEO_PROFILE_HIGH),
            None
        );
        assert_eq!(
            normalized_video_profile_for_persist(5, 1, 0, VIDEO_PROFILE_HIGH),
            None
        );
    }

    #[test]
    fn hardware_acceleration_options_always_includes_all_standard_options() {
        // Even with no hardware capabilities detected, all standard options appear.
        let capabilities = FfmpegCapabilityReport::default();

        let items = hw_acceleration_select_items(&capabilities);
        let values: Vec<i32> = items.iter().map(|item| item.value).collect();

        assert!(values.contains(&0), "None must always be present");
        assert!(values.contains(&1), "QSV must always be present");
        assert!(values.contains(&2), "NVENC must always be present");
        assert!(values.contains(&3), "VAAPI must always be present");
        assert!(values.contains(&4), "VideoToolbox must always be present");
        assert!(values.contains(&5), "AMF must always be present");
        assert!(values.contains(&7), "RKMPP must always be present");
    }

    #[test]
    fn hardware_acceleration_options_label_unsupported_as_not_available() {
        // No GPU hardware detected — all non-None options are labelled accordingly.
        let capabilities = FfmpegCapabilityReport::default();

        let items = hw_acceleration_select_items(&capabilities);

        assert_eq!(
            items[0],
            HardwareAccelerationOption {
                value: 0,
                label: "None".to_string(),
                supported: true,
            }
        );
        assert!(
            items
                .iter()
                .any(|item| item.value == 2 && item.label == "NVENC" && !item.supported),
            "NVENC should be marked unsupported when no GPU is detected"
        );
    }

    #[test]
    fn hardware_acceleration_options_label_detected_hardware_without_suffix() {
        // When GPU hardware is present the label has no '(not available)' suffix.
        let capabilities = FfmpegCapabilityReport {
            hwaccels: BTreeSet::from(["vulkan".to_string()]),
            encoders: BTreeSet::from(["h264_nvenc".to_string(), "hevc_vaapi".to_string()]),
            ..FfmpegCapabilityReport::default()
        };

        let items = hw_acceleration_select_items(&capabilities);

        assert!(
            items.contains(&HardwareAccelerationOption {
                value: 2,
                label: "NVENC".to_string(),
                supported: true,
            }),
            "NVENC should be marked supported when h264_nvenc encoder is present"
        );
        assert!(
            items.contains(&HardwareAccelerationOption {
                value: 3,
                label: "VAAPI".to_string(),
                supported: true,
            }),
            "VAAPI should be marked supported when hevc_vaapi encoder is present"
        );
    }
}
