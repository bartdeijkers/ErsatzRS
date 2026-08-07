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

/// Output dynamic-range contract of an [`FFmpegProfile`] (ADR 0064).
///
/// The profile describes the *output* stream, not a per-item preference, so an
/// `Hdr10Bt2020Pq` profile stays HDR10 when an SDR item follows an HDR item.
/// Bit depth stays a separate field: 10-bit SDR is valid and must not be
/// mistaken for HDR.
///
/// Only meaningful when video is transcoded.
/// [`FFmpegProfileVideoFormat::Copy`] preserves the source elementary stream
/// and ignores this field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum OutputDynamicRange {
    /// Produce BT.709 SDR. HDR10, HLG, and supported Dolby Vision inputs are
    /// tone-mapped to SDR and all HDR/Dolby Vision signaling is removed.
    #[default]
    SdrBt709,
    /// Produce a stable BT.2020/PQ HDR10 output. HDR input is preserved or
    /// normalized; SDR input is mapped into the HDR10 signal at a fixed
    /// reference white without inventing highlight detail.
    Hdr10Bt2020Pq,
}

impl OutputDynamicRange {
    /// True when the profile must emit BT.2020/PQ HDR10 signaling.
    #[must_use]
    pub const fn is_hdr10(self) -> bool {
        matches!(self, Self::Hdr10Bt2020Pq)
    }

    /// Codecs able to carry HDR10 static metadata in the containers ErsatzRS
    /// serves (MPEG-TS and fMP4/HLS).
    ///
    /// H.264 is excluded deliberately: while HEVC, AV1, and MPEG-2 signalling
    /// differ, only HEVC and AV1 have a mastering-display/content-light SEI or
    /// OBU metadata path that `FFmpeg` emits and that clients read. Rejecting the
    /// rest is ADR 0064's "never silently downgrade" rule.
    #[must_use]
    pub const fn codec_supports_hdr10(video_format: FFmpegProfileVideoFormat) -> bool {
        matches!(
            video_format,
            FFmpegProfileVideoFormat::Hevc | FFmpegProfileVideoFormat::Av1
        )
    }
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
    pub transcode_audio: bool,
    pub transcode_video: bool,
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
    pub video_tune: Option<String>,
    pub allow_b_frames: bool,
    pub bit_depth: FFmpegProfileBitDepth,
    pub video_bitrate: i32,
    pub video_buffer_size: i32,
    pub tonemap_algorithm: FFmpegProfileTonemapAlgorithm,
    /// ADR 0064 output dynamic-range contract. Ignored when
    /// `video_format == FFmpegProfileVideoFormat::Copy`.
    pub output_dynamic_range: OutputDynamicRange,
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
