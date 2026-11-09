//! JSON exchanged with `ersatztv-channel`, not a second scheduling model.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub const PLAYOUT_VERSION: &str = "https://ersatztv.org/playout/version/0.0.7";
pub const CHANNEL_VERSION: &str = "https://ersatztv.org/channel/version/0.1.2";

#[derive(Clone, Serialize, Deserialize)]
pub struct ChannelConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback: Option<Fallback>,
    pub playout: PlayoutFolder,
    pub ffmpeg: Ffmpeg,
    pub normalization: Normalization,
}

/// Emergency fallback card policy; ordinary subtitle selection is independent.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Fallback {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub show_error: Option<bool>,
    /// Minimum fallback duration after failed resolution; Next defaults to 60s.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_seconds: Option<u32>,
    /// Custom or bundled offline background, independent of the error overlay.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background_image: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct PlayoutFolder {
    pub folder: String,
    /// Explicit private reader layout; absent retains ordinary folder loading.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub virtual_start: Option<DateTime<Utc>>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Ffmpeg {
    pub ffmpeg_path: String,
    pub ffprobe_path: String,
    /// Next f2a981b enables reports through this path, without a `save_reports` flag.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reports_folder: Option<String>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fonts_folder: Option<String>,
    /// Optional ASS style override, matching upstream Next's channel config.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub force_style: Option<String>,
}

#[derive(Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubtitleMode {
    #[default]
    Burn,
    Convert,
}

/// Copy eligible source streams and retain encoding settings for fallback items.
#[derive(Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StreamMode {
    #[default]
    Transcode,
    Copy,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Audio {
    #[serde(default)]
    pub mode: StreamMode,
    pub format: AudioFormat,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bitrate_kbps: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub buffer_kbps: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channels: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sample_rate_hz: Option<u32>,
    pub normalize_loudness: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub loudness: Option<Loudness>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Loudness {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub integrated_target: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub range_target: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub true_peak: Option<f64>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Video {
    /// Output rate: frames are dropped/repeated; copy applies only at this rate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame_rate: Option<String>,
    #[serde(default)]
    pub mode: StreamMode,
    pub format: VideoFormat,
    pub bit_depth: u8,
    pub width: u32,
    pub height: u32,
    #[serde(default)]
    pub scaling_mode: ScalingMode,
    #[serde(default)]
    pub deinterlace: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bitrate_kbps: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub buffer_kbps: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accel: Option<Acceleration>,
    /// Windows adapter index. Omission keeps Next's automatic AMD selection.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amf_device: Option<u32>,
    pub filters: VideoFilters,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vaapi_device: Option<String>,
    /// Omission lets libva choose the driver for the selected VAAPI device.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vaapi_driver: Option<VaapiDriver>,
}

// Next 946aa52 consumes these per-filter options. The original integration's
// top-level tonemap_algorithm is ignored by that packaged runtime.
#[derive(Clone, Serialize, Deserialize)]
pub struct VideoFilters {
    pub libplacebo: LibplaceboOptions,
    pub tonemap: TonemapOptions,
    pub tonemap_opencl: TonemapOptions,
    /// Software scaler options; omission preserves Next's bicubic default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale: Option<ScaleOptions>,
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct ScaleOptions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flags: Option<String>,
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
    Mpeg2Video,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub watermark: Option<Watermark>,
    /// Ordered overlays, lowest first, above the compatibility watermark.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub graphics: Vec<Watermark>,
}

/// Official Next layer semantics: canvas sources already have output geometry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GraphicsLayerKind {
    Media,
    Canvas,
}

/// A single image/video overlay, always visible unless periodic timing is set.
#[derive(Clone, Serialize, Deserialize)]
pub struct Watermark {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<CanvasLayout>,
    pub source: Source,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<GraphicsLayerKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timing: Option<WatermarkTiming>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream_index: Option<u32>,
    pub location: WatermarkLocation,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub within_source_content: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width_percent: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub horizontal_margin_percent: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vertical_margin_percent: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opacity_percent: Option<f64>,
}

/// Upstream playout0.0.7 canvas placement; media layers never emit this field.
#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum CanvasLayout {
    FullFrame,
    Region { origin: CanvasPosition },
}

#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct CanvasPosition {
    pub x: u32,
    pub y: u32,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "timing_type", rename_all = "snake_case")]
pub enum WatermarkTiming {
    Periodic {
        clock: PeriodicClock,
        frequency_ms: u64,
        hold_ms: u64,
    },
}

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PeriodicClock {
    Wall,
    Content,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WatermarkLocation {
    TopLeft,
    TopCenter,
    TopRight,
    CenterLeft,
    Center,
    CenterRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
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
        #[serde(default, skip_serializing_if = "Option::is_none")]
        probe_hint: Option<ProbeHint>,
        #[serde(skip_serializing_if = "Option::is_none")]
        in_point_ms: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        out_point_ms: Option<u64>,
    },
    Http {
        uri: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        probe_hint: Option<ProbeHint>,
        /// Live input disables source seeks and work-ahead in Next.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        is_live: Option<bool>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        headers: Vec<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        user_agent: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        keep_alive: Option<bool>,
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
    /// RTSP is a live input in Next and has no source seek fields.
    Rtsp {
        uri: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        probe_hint: Option<ProbeHint>,
        #[serde(skip_serializing_if = "Option::is_none")]
        timeout_us: Option<u64>,
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
        /// Total resolution time including requests/backoff; 0 disables retries.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        retry_budget_ms: Option<u64>,
        /// First retry delay; later delays double with runtime-owned jitter.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        retry_backoff_ms: Option<u64>,
    },
    Lavfi {
        params: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        probe_hint: Option<ProbeHint>,
    },
}

/// Official Next metadata for an input that must not be consumed by probing.
#[derive(Clone, Serialize, Deserialize)]
pub struct ProbeHint {
    /// Negotiated consume-once transport, not whole-source readiness or duration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub single_use: Option<SingleUseInputHint>,
    pub video: Vec<VideoProbeHint>,
    pub audio: Vec<AudioProbeHint>,
    pub subtitle: Vec<SubtitleProbeHint>,
    pub format_name: Option<String>,
    pub duration_ms: Option<u64>,
}

/// Fresh first-packet observations for the very same owned producer transfer.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SingleUseInputHint {
    pub version: u8,
    /// Missing video indexes remain unqualified; false means a leading non-keyframe.
    pub first_video_packets: Vec<SingleUseVideoStart>,
}

/// One observed first video packet, indexed in the fresh consuming stream.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SingleUseVideoStart {
    pub stream_index: u32,
    pub is_keyframe: bool,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct VideoProbeHint {
    pub codec: String,
    pub width: u32,
    pub height: u32,
    pub pix_fmt: String,
    pub stream_index: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frame_rate: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field_order: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sample_aspect_ratio: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_aspect_ratio: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color_range: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color_space: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color_transfer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color_primaries: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dv_profile: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_hdr10_metadata: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rotation: Option<i32>,
    /// Required only after explicit runtime capability negotiation; never a
    /// replacement for the source's original outpoint or format duration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_frame_cut: Option<SourceFrameCutHint>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceFrameCutHint {
    pub version: u8,
    pub out_point_ms: u64,
    pub first_excluded_pts_90k: i64,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct SubtitleProbeHint {
    pub codec: String,
    pub stream_index: u32,
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
