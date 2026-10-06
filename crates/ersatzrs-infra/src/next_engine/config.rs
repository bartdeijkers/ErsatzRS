//! Translate the persisted channel profile into the packaged Next contract.
//!
//! Maps upstream `StartFFmpegNextSessionHandler` normalization fields. Unsupported
//! explicit output formats fail instead of inheriting its H.264/AAC fallbacks.

use std::path::Path;

use ersatzrs_domain::{
    FFmpegProfile, FFmpegProfileAudioFormat, FFmpegProfileBitDepth, FFmpegProfileTonemapAlgorithm,
    FFmpegProfileVideoFormat, HardwareAccelerationKind, NextEngineTextSubtitleMode,
    NormalizeLoudnessMode, OutputDynamicRange, ScalingBehavior, VaapiDriver,
};

use crate::playback::PlaybackProfile;

use super::protocol as wire;

/// Stable diagnostics contain no profile name, runtime path or source URL.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ConfigError {
    #[error("Next does not support the selected video output codec")]
    VideoCodec,
    #[error("Next MPEG-2 output requires a positive video bitrate")]
    VideoBitrate,
    #[error("Next does not support the selected audio output codec")]
    AudioCodec,
    #[error("Next does not support the selected HDR output contract")]
    HdrOutput,
    #[error("Next does not support the selected hardware accelerator")]
    Accelerator,
    #[error("Next does not support the selected VAAPI driver")]
    VaapiDriver,
    #[error("Next requires a positive output resolution")]
    Resolution,
    #[error("Next audio sample rate exceeds the supported integer range")]
    SampleRate,
    #[error("Next loudness target must be finite and between -70 and -5 LUFS")]
    Loudness,
    #[error("Next requires nonempty UTF-8 runtime paths")]
    RuntimePath,
}

/// Build configuration using resolved managed-runtime paths and an owned playout folder.
///
/// Audio rate is stored in kHz, whereas the Next protocol requires Hz. Like the
/// native profile mapper, nonpositive rate/bitrate/buffer/channel values mean
/// unspecified. The engine owns encoding policy not exposed by its wire API;
/// this conversion does not claim native/Next command-line equivalence.
///
/// # Errors
/// Returns a typed, redacted error for unsupported explicit output contracts,
/// invalid dimensions/loudness, rate overflow or unrepresentable runtime paths.
pub fn channel_config(
    playback: &PlaybackProfile,
    subtitle_mode: NextEngineTextSubtitleMode,
    playout_folder: &Path,
    ffmpeg_path: &Path,
    ffprobe_path: &Path,
) -> Result<wire::ChannelConfig, ConfigError> {
    Ok(wire::ChannelConfig {
        version: Some(wire::CHANNEL_VERSION.to_owned()),
        fallback: Some(wire::Fallback {
            show_error: Some(true),
        }),
        playout: wire::PlayoutFolder {
            folder: runtime_path(playout_folder)?,
            virtual_start: None,
        },
        ffmpeg: wire::Ffmpeg {
            ffmpeg_path: runtime_path(ffmpeg_path)?,
            ffprobe_path: runtime_path(ffprobe_path)?,
            reports_folder: None,
            disabled_filters: Vec::new(),
        },
        normalization: wire::Normalization {
            audio: audio(&playback.profile)?,
            video: video(playback)?,
            subtitle: wire::SubtitleNormalization {
                fonts_folder: None,
                force_style: None,
                mode: match subtitle_mode {
                    NextEngineTextSubtitleMode::Burn => wire::SubtitleMode::Burn,
                    NextEngineTextSubtitleMode::Convert => wire::SubtitleMode::Convert,
                },
            },
        },
    })
}

fn audio(profile: &FFmpegProfile) -> Result<wire::Audio, ConfigError> {
    let copy = profile.audio_format == FFmpegProfileAudioFormat::Copy;
    let format = match profile.audio_format {
        FFmpegProfileAudioFormat::Aac | FFmpegProfileAudioFormat::Copy => wire::AudioFormat::Aac,
        FFmpegProfileAudioFormat::Ac3 => wire::AudioFormat::Ac3,
        _ => return Err(ConfigError::AudioCodec),
    };
    let sample_rate_hz = positive(profile.audio_sample_rate)
        .map(|rate| rate.checked_mul(1_000).ok_or(ConfigError::SampleRate))
        .transpose()?;
    // Upstream ChannelConfigConverter suppresses loudness for audio copy;
    // retain AAC settings for items that the engine must transcode instead.
    let normalize_loudness =
        !copy && profile.normalize_loudness_mode == NormalizeLoudnessMode::LoudNorm;
    if normalize_loudness
        && profile
            .target_loudness
            .is_some_and(|target| !target.is_finite() || !(-70.0..=-5.0).contains(&target))
    {
        return Err(ConfigError::Loudness);
    }
    Ok(wire::Audio {
        mode: if copy {
            wire::StreamMode::Copy
        } else {
            wire::StreamMode::Transcode
        },
        format,
        bitrate_kbps: positive(profile.audio_bitrate),
        buffer_kbps: positive(profile.audio_buffer_size),
        channels: positive(profile.audio_channels),
        sample_rate_hz,
        normalize_loudness,
        loudness: normalize_loudness.then_some(wire::Loudness {
            integrated_target: profile.target_loudness,
            range_target: None,
            true_peak: None,
        }),
    })
}

fn video(playback: &PlaybackProfile) -> Result<wire::Video, ConfigError> {
    let profile = &playback.profile;
    if profile.output_dynamic_range != OutputDynamicRange::SdrBt709 {
        return Err(ConfigError::HdrOutput);
    }
    let format = match profile.video_format {
        FFmpegProfileVideoFormat::H264 | FFmpegProfileVideoFormat::Copy => wire::VideoFormat::H264,
        FFmpegProfileVideoFormat::Hevc => wire::VideoFormat::Hevc,
        FFmpegProfileVideoFormat::Mpeg2Video => {
            if positive(profile.video_bitrate).is_none() {
                return Err(ConfigError::VideoBitrate);
            }
            wire::VideoFormat::Mpeg2Video
        }
        _ => return Err(ConfigError::VideoCodec),
    };
    let accel = match profile.hardware_acceleration {
        HardwareAccelerationKind::None => None,
        HardwareAccelerationKind::Nvenc => Some(wire::Acceleration::Cuda),
        HardwareAccelerationKind::Qsv => Some(wire::Acceleration::Qsv),
        HardwareAccelerationKind::Vaapi => Some(wire::Acceleration::Vaapi),
        HardwareAccelerationKind::VideoToolbox => Some(wire::Acceleration::Videotoolbox),
        HardwareAccelerationKind::Amf => Some(wire::Acceleration::Amf),
        HardwareAccelerationKind::Rkmpp => Some(wire::Acceleration::Rkmpp),
        HardwareAccelerationKind::V4l2m2m => return Err(ConfigError::Accelerator),
    };
    let vaapi_driver = if profile.hardware_acceleration == HardwareAccelerationKind::Vaapi {
        match profile.vaapi_driver {
            VaapiDriver::Auto => None,
            VaapiDriver::IHD => Some(wire::VaapiDriver::Ihd),
            VaapiDriver::I965 => Some(wire::VaapiDriver::I965),
            VaapiDriver::RadeonSI => Some(wire::VaapiDriver::Radeonsi),
            VaapiDriver::Nouveau => return Err(ConfigError::VaapiDriver),
        }
    } else {
        None
    };
    let tonemap = match profile.tonemap_algorithm {
        FFmpegProfileTonemapAlgorithm::Linear => "linear",
        FFmpegProfileTonemapAlgorithm::Clip => "clip",
        FFmpegProfileTonemapAlgorithm::Gamma => "gamma",
        FFmpegProfileTonemapAlgorithm::Reinhard => "reinhard",
        FFmpegProfileTonemapAlgorithm::Mobius => "mobius",
        FFmpegProfileTonemapAlgorithm::Hable => "hable",
    };
    Ok(wire::Video {
        mode: if profile.video_format == FFmpegProfileVideoFormat::Copy {
            wire::StreamMode::Copy
        } else {
            wire::StreamMode::Transcode
        },
        format,
        bit_depth: match (profile.video_format, profile.bit_depth) {
            (FFmpegProfileVideoFormat::Mpeg2Video, _) | (_, FFmpegProfileBitDepth::EightBit) => 8,
            (_, FFmpegProfileBitDepth::TenBit) => 10,
        },
        width: positive(playback.resolution.width).ok_or(ConfigError::Resolution)?,
        height: positive(playback.resolution.height).ok_or(ConfigError::Resolution)?,
        scaling_mode: match profile.scaling_behavior {
            ScalingBehavior::ScaleAndPad => wire::ScalingMode::ScaleAndPad,
            ScalingBehavior::Stretch => wire::ScalingMode::Stretch,
            ScalingBehavior::Crop => wire::ScalingMode::Crop,
        },
        deinterlace: profile.deinterlace_video == Some(true),
        bitrate_kbps: positive(profile.video_bitrate),
        buffer_kbps: positive(profile.video_buffer_size),
        accel,
        amf_device: None,
        filters: wire::VideoFilters {
            libplacebo: wire::LibplaceboOptions {
                tonemapping: tonemap.to_owned(),
            },
            tonemap: wire::TonemapOptions {
                tonemap: tonemap.to_owned(),
            },
            tonemap_opencl: wire::TonemapOptions {
                tonemap: tonemap.to_owned(),
            },
        },
        vaapi_device: profile
            .vaapi_device
            .clone()
            .filter(|path| !path.trim().is_empty()),
        vaapi_driver,
    })
}

fn positive(value: i32) -> Option<u32> {
    u32::try_from(value).ok().filter(|value| *value > 0)
}

fn runtime_path(path: &Path) -> Result<String, ConfigError> {
    path.to_str()
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or(ConfigError::RuntimePath)
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
