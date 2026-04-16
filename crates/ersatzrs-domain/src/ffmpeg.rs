use serde::{Deserialize, Serialize};

use crate::{EntityId, FilterMode, ScalingBehavior};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum HardwareAccelerationKind {
    #[default]
    None,
    Qsv,
    Nvenc,
    Vaapi,
    VideoToolbox,
    Amf,
    V4l2m2m,
    Rkmpp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum FFmpegProfileVideoFormat {
    #[default]
    None,
    H264,
    Hevc,
    Mpeg2Video,
    Av1,
    Copy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum FFmpegProfileAudioFormat {
    #[default]
    None,
    Aac,
    Ac3,
    AacLatm,
    Copy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum FFmpegProfileBitDepth {
    #[default]
    EightBit,
    TenBit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum FFmpegProfileTonemapAlgorithm {
    #[default]
    Linear,
    Clip,
    Gamma,
    Reinhard,
    Mobius,
    Hable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum NormalizeLoudnessMode {
    #[default]
    Off,
    LoudNorm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum VaapiDriver {
    #[default]
    Auto,
    IHD,
    I965,
    RadeonSI,
    Nouveau,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Resolution {
    pub id: EntityId,
    pub name: String,
    pub is_custom: bool,
    pub width: i32,
    pub height: i32,
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct FFmpegProfile {
    pub id: EntityId,
    pub name: String,
    pub thread_count: i32,
    pub normalize_audio: bool,
    pub normalize_video: bool,
    pub hardware_acceleration: HardwareAccelerationKind,
    pub vaapi_display: Option<String>,
    pub vaapi_driver: VaapiDriver,
    pub vaapi_device: Option<String>,
    pub qsv_extra_hardware_frames: Option<i32>,
    pub resolution_id: EntityId,
    pub scaling_behavior: ScalingBehavior,
    pub pad_mode: FilterMode,
    pub video_format: FFmpegProfileVideoFormat,
    pub video_profile: Option<String>,
    pub video_preset: Option<String>,
    pub allow_b_frames: bool,
    pub bit_depth: FFmpegProfileBitDepth,
    pub video_bitrate: i32,
    pub video_buffer_size: i32,
    pub tonemap_algorithm: FFmpegProfileTonemapAlgorithm,
    pub audio_format: FFmpegProfileAudioFormat,
    pub audio_bitrate: i32,
    pub audio_buffer_size: i32,
    pub normalize_loudness_mode: NormalizeLoudnessMode,
    pub target_loudness: Option<f64>,
    pub audio_channels: i32,
    pub audio_sample_rate: i32,
    pub normalize_framerate: bool,
    pub normalize_colors: bool,
    pub deinterlace_video: Option<bool>,
}
