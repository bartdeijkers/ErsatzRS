//! JSON exchanged with `ersatztv-channel`, not a second scheduling model.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub const PLAYOUT_VERSION: &str = "https://ersatztv.org/playout/version/0.0.1";

#[derive(Clone, Serialize, Deserialize)]
pub struct ChannelConfig {
    pub playout: PlayoutFolder,
    pub ffmpeg: Ffmpeg,
    pub normalization: Normalization,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct PlayoutFolder {
    pub folder: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub virtual_start: Option<DateTime<Utc>>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Ffmpeg {
    pub ffmpeg_path: String,
    pub ffprobe_path: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub disabled_filters: Vec<String>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Normalization {
    pub audio: Audio,
    pub video: Video,
    #[serde(default)]
    pub subtitle: SubtitleNormalization,
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct SubtitleNormalization {
    pub mode: SubtitleMode,
}

#[derive(Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubtitleMode {
    #[default]
    Burn,
    Convert,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Audio {
    pub format: AudioFormat,
    pub bitrate_kbps: Option<u32>,
    pub buffer_kbps: Option<u32>,
    pub channels: Option<u32>,
    pub sample_rate_hz: Option<u32>,
    pub normalize_loudness: bool,
    pub loudness: Option<Loudness>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Loudness {
    pub integrated_target: Option<f64>,
    pub range_target: Option<f64>,
    pub true_peak: Option<f64>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Video {
    pub format: VideoFormat,
    pub bit_depth: u8,
    pub width: u32,
    pub height: u32,
    #[serde(default)]
    pub scaling_mode: ScalingMode,
    #[serde(default)]
    pub deinterlace: bool,
    pub bitrate_kbps: Option<u32>,
    pub buffer_kbps: Option<u32>,
    pub accel: Option<Acceleration>,
    pub filters: VideoFilters,
    pub vaapi_device: Option<String>,
    pub vaapi_driver: Option<VaapiDriver>,
}

// Next 946aa52 consumes these per-filter options. The original integration's
// top-level tonemap_algorithm is ignored by that packaged runtime.
#[derive(Clone, Serialize, Deserialize)]
pub struct VideoFilters {
    pub libplacebo: LibplaceboOptions,
    pub tonemap: TonemapOptions,
    pub tonemap_opencl: TonemapOptions,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct LibplaceboOptions {
    pub tonemapping: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct TonemapOptions {
    pub tonemap: String,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AudioFormat {
    Aac,
    Ac3,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VideoFormat {
    H264,
    Hevc,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Acceleration {
    Amf,
    Cuda,
    Qsv,
    Rkmpp,
    Vaapi,
    Videotoolbox,
    Vulkan,
}

#[derive(Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScalingMode {
    #[default]
    ScaleAndPad,
    Stretch,
    Crop,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VaapiDriver {
    I965,
    Ihd,
    Radeonsi,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Playout {
    pub version: String,
    pub items: Vec<PlayoutItem>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct PlayoutItem {
    pub id: String,
    pub start: DateTime<Utc>,
    pub finish: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tracks: Option<Tracks>,
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Tracks {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio: Option<Track>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video: Option<Track>,
    // The current packaged Next contract adds explicit subtitle selection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<Track>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Track {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream_index: Option<u32>,
}

// Deliberately no Debug: HTTP sources can carry private URLs and credentials.
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "source_type", rename_all = "lowercase")]
pub enum Source {
    Local {
        path: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        in_point_ms: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        out_point_ms: Option<u64>,
    },
    Http {
        uri: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        headers: Vec<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        user_agent: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        reconnect: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        reconnect_delay_max: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        timeout_us: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        in_point_ms: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        out_point_ms: Option<u64>,
    },
    /// Resolve one complete concrete item before Next probes its media inputs.
    /// Dynamic is only supported at the item's top-level source, not on a track.
    Dynamic {
        uri: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        headers: Option<Vec<String>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        user_agent: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        timeout_us: Option<u64>,
    },
    Lavfi {
        params: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        probe_hint: Option<AudioOnlyProbeHint>,
    },
}

/// Finite metadata for generated silence; Next must not probe an infinite lavfi input.
#[derive(Clone, Serialize, Deserialize)]
pub struct AudioOnlyProbeHint {
    pub video: [(); 0],
    pub audio: Vec<AudioProbeHint>,
    pub subtitle: [(); 0],
    pub format_name: String,
    pub duration_ms: u64,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct AudioProbeHint {
    pub stream_index: u32,
    pub codec: String,
    pub channels: u32,
}

#[cfg(test)]
#[path = "protocol_tests.rs"]
mod tests;
