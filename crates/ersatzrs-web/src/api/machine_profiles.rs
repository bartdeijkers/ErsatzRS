//! Upstream-compatible profile machine routes over the application boundary.
use super::machine::{parse_json, MachineError};
use crate::host::state::AppState;
use axum::{
    extract::{rejection::JsonRejection, Path, State},
    Json,
};
use ersatzrs_app::commands::{
    DeleteFfmpegProfile, FfmpegProfileAdminForm, SaveFfmpegProfileForApi,
};
use ersatzrs_domain::{
    FFmpegProfileAudioFormat, FFmpegProfileBitDepth, FFmpegProfileTonemapAlgorithm,
    FFmpegProfileVideoFormat, HardwareAccelerationKind, NormalizeLoudnessMode, ScalingBehavior,
    VaapiDriver,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

macro_rules! enum_code {
    ($function:ident, [$($name:literal => $code:literal),+ $(,)?]) => {
        fn $function<'de, D: serde::Deserializer<'de>>(input: D) -> Result<i32, D::Error> {
            let value = Value::deserialize(input)?;
            match value {
                Value::String(name) => match name.to_ascii_lowercase().as_str() {
                    $($name => Ok($code),)+
                    _ => Err(serde::de::Error::custom("unknown enum value")),
                },
                Value::Number(number) => {
                    let code = number.as_i64().and_then(|n| i32::try_from(n).ok());
                    match code {
                        $(Some($code) => Ok($code),)+
                        _ => Err(serde::de::Error::custom("unknown enum value")),
                    }
                }
                _ => Err(serde::de::Error::custom("expected enum name or integer")),
            }
        }
    };
}
enum_code!(hw, ["none"=>0,"qsv"=>1,"nvenc"=>2,"vaapi"=>3,"videotoolbox"=>4,"amf"=>5,"v4l2m2m"=>6,"rkmpp"=>7]);
enum_code!(driver, ["default"=>0,"ihd"=>1,"i965"=>2,"radeonsi"=>3,"nouveau"=>4]);
enum_code!(scaling, ["scaleandpad"=>0,"stretch"=>1,"crop"=>2]);
enum_code!(pad, ["hardwareifpossible"=>0,"software"=>1]);
enum_code!(video, ["none"=>0,"h264"=>1,"hevc"=>2,"mpeg2video"=>3,"av1"=>4,"copy"=>99]);
enum_code!(depth, ["eightbit"=>0,"tenbit"=>1]);
enum_code!(tonemap, ["linear"=>0,"clip"=>1,"gamma"=>2,"reinhard"=>3,"mobius"=>4,"hable"=>5]);
enum_code!(audio, ["none"=>0,"aac"=>1,"ac3"=>2,"aaclatm"=>3,"copy"=>99]);
enum_code!(loudness, ["off"=>0,"loudnorm"=>1]);

#[allow(clippy::struct_excessive_bools)] // Preserve independent boolean fields in the upstream machine API request.
#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ProfileMutation {
    #[serde(rename = "ffmpegProfileId", alias = "FFmpegProfileId")]
    id: i64,
    name: String,
    thread_count: i32,
    #[serde(rename = "normalizeAudio", alias = "transcodeAudio")]
    transcode_audio: bool,
    #[serde(rename = "normalizeVideo", alias = "transcodeVideo")]
    transcode_video: bool,
    #[serde(deserialize_with = "hw")]
    hardware_acceleration: i32,
    vaapi_display: Option<String>,
    #[serde(deserialize_with = "driver")]
    vaapi_driver: i32,
    vaapi_device: Option<String>,
    qsv_extra_hardware_frames: Option<i32>,
    resolution_id: i64,
    #[serde(deserialize_with = "scaling")]
    scaling_behavior: i32,
    #[serde(deserialize_with = "pad")]
    pad_mode: i32,
    #[serde(deserialize_with = "video")]
    video_format: i32,
    video_profile: Option<String>,
    video_preset: Option<String>,
    allow_b_frames: bool,
    #[serde(deserialize_with = "depth")]
    bit_depth: i32,
    video_bitrate: i32,
    video_buffer_size: i32,
    #[serde(deserialize_with = "tonemap")]
    tonemap_algorithm: i32,
    #[serde(deserialize_with = "audio")]
    audio_format: i32,
    audio_bitrate: i32,
    audio_buffer_size: i32,
    #[serde(deserialize_with = "loudness")]
    normalize_loudness_mode: i32,
    target_loudness: Option<f64>,
    audio_channels: i32,
    audio_sample_rate: i32,
    normalize_framerate: bool,
    normalize_colors: bool,
    deinterlace_video: bool,
}

impl ProfileMutation {
    fn into_command(
        self,
        is_update: bool,
        existing: Option<FfmpegProfileAdminForm>,
    ) -> SaveFfmpegProfileForApi {
        let (video_tune, output_dynamic_range, hdr_mastering_display, hdr_max_cll, hdr_max_fall) =
            existing.map_or_else(
                || (String::new(), 0, String::new(), 0, 0),
                |old| {
                    (
                        old.video_tune,
                        old.output_dynamic_range,
                        old.hdr_mastering_display,
                        old.hdr_max_cll,
                        old.hdr_max_fall,
                    )
                },
            );
        SaveFfmpegProfileForApi {
            is_update,
            qsv_extra_hardware_frames: self.qsv_extra_hardware_frames,
            form: FfmpegProfileAdminForm {
                id: if is_update { self.id } else { 0 },
                name: self.name,
                thread_count: self.thread_count,
                transcode_audio: self.transcode_audio,
                transcode_video: self.transcode_video,
                hardware_acceleration: self.hardware_acceleration,
                vaapi_driver: self.vaapi_driver,
                resolution_id: self.resolution_id,
                scaling_behavior: self.scaling_behavior,
                pad_mode: self.pad_mode,
                video_format: self.video_format,
                allow_b_frames: self.allow_b_frames,
                bit_depth: self.bit_depth,
                video_bitrate: self.video_bitrate,
                video_buffer_size: self.video_buffer_size,
                tonemap_algorithm: self.tonemap_algorithm,
                audio_format: self.audio_format,
                audio_bitrate: self.audio_bitrate,
                audio_buffer_size: self.audio_buffer_size,
                normalize_loudness_mode: self.normalize_loudness_mode,
                audio_channels: self.audio_channels,
                audio_sample_rate: self.audio_sample_rate,
                normalize_framerate: self.normalize_framerate,
                normalize_colors: self.normalize_colors,
                deinterlace_video: self.deinterlace_video,
                vaapi_display: self.vaapi_display.unwrap_or_default(),
                vaapi_device: self.vaapi_device.unwrap_or_default(),
                video_profile: self.video_profile.unwrap_or_default(),
                video_preset: self.video_preset.unwrap_or_default(),
                qsv_extra_hardware_frames: self.qsv_extra_hardware_frames.unwrap_or(64),
                target_loudness: self
                    .target_loudness
                    .map(|v| v.to_string())
                    .unwrap_or_default(),
                video_tune,
                output_dynamic_range,
                hdr_mastering_display,
                hdr_max_cll,
                hdr_max_fall,
            },
        }
    }
}

pub async fn create(
    State(state): State<AppState>,
    input: Result<Json<ProfileMutation>, JsonRejection>,
) -> Result<Json<Value>, MachineError> {
    let id = state
        .application
        .save_ffmpeg_profile_for_api(parse_json(input)?.into_command(false, None))
        .await?;
    Ok(Json(json!({"ffmpegProfileId":id, "id":id})))
}

pub async fn update(
    State(state): State<AppState>,
    input: Result<Json<ProfileMutation>, JsonRejection>,
) -> Result<Json<Value>, MachineError> {
    let input = parse_json(input)?;
    let existing = if input.id > 0 {
        state.application.get_ffmpeg_profile(input.id).await?
    } else {
        None
    };
    let id = state
        .application
        .save_ffmpeg_profile_for_api(input.into_command(true, existing))
        .await?;
    Ok(Json(json!({"ffmpegProfileId":id, "id":id})))
}

pub async fn delete(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, MachineError> {
    if id <= 0 {
        return Err(ersatzrs_app::ApplicationError::NotFound {
            kind: "ffmpeg profile",
            id,
        }
        .into());
    }
    state
        .application
        .delete_ffmpeg_profile(DeleteFfmpegProfile { id })
        .await?;
    Ok(Json(json!({})))
}

#[allow(clippy::struct_excessive_bools)] // Preserve independent boolean fields in the upstream machine API response.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileResponse {
    id: i64,
    name: String,
    thread_count: i32,
    hardware_acceleration: HardwareAccelerationKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    vaapi_display: Option<String>,
    vaapi_driver: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    vaapi_device: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    qsv_extra_hardware_frames: Option<i32>,
    resolution: String,
    scaling_behavior: ScalingBehavior,
    video_format: FFmpegProfileVideoFormat,
    #[serde(skip_serializing_if = "Option::is_none")]
    video_profile: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    video_preset: Option<String>,
    allow_b_frames: bool,
    bit_depth: FFmpegProfileBitDepth,
    video_bitrate: i32,
    video_buffer_size: i32,
    tonemap_algorithm: FFmpegProfileTonemapAlgorithm,
    audio_format: FFmpegProfileAudioFormat,
    audio_bitrate: i32,
    audio_buffer_size: i32,
    normalize_loudness_mode: NormalizeLoudnessMode,
    audio_channels: i32,
    audio_sample_rate: i32,
    normalize_framerate: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    deinterlace_video: Option<bool>,
}

pub async fn list(
    State(state): State<AppState>,
) -> Result<Json<Vec<ProfileResponse>>, MachineError> {
    let records = state.application.list_ffmpeg_profiles_for_api().await?;
    Ok(Json(
        records
            .into_iter()
            .map(|(profile, resolution)| ProfileResponse {
                resolution,
                vaapi_driver: match profile.vaapi_driver {
                    VaapiDriver::Auto => "Default",
                    VaapiDriver::IHD => "iHD",
                    VaapiDriver::I965 => "i965",
                    VaapiDriver::RadeonSI => "RadeonSI",
                    VaapiDriver::Nouveau => "Nouveau",
                },
                id: profile.id,
                name: profile.name,
                thread_count: profile.thread_count,
                hardware_acceleration: profile.hardware_acceleration,
                vaapi_display: profile.vaapi_display,
                vaapi_device: profile.vaapi_device,
                qsv_extra_hardware_frames: profile.qsv_extra_hardware_frames,
                scaling_behavior: profile.scaling_behavior,
                video_format: profile.video_format,
                video_profile: profile.video_profile,
                video_preset: profile.video_preset,
                allow_b_frames: profile.allow_b_frames,
                bit_depth: profile.bit_depth,
                video_bitrate: profile.video_bitrate,
                video_buffer_size: profile.video_buffer_size,
                tonemap_algorithm: profile.tonemap_algorithm,
                audio_format: profile.audio_format,
                audio_bitrate: profile.audio_bitrate,
                audio_buffer_size: profile.audio_buffer_size,
                normalize_loudness_mode: profile.normalize_loudness_mode,
                audio_channels: profile.audio_channels,
                audio_sample_rate: profile.audio_sample_rate,
                normalize_framerate: profile.normalize_framerate,
                deinterlace_video: profile.deinterlace_video,
            })
            .collect(),
    ))
}
