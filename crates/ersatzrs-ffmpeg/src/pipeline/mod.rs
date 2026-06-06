use ersatzrs_domain::{
    FFmpegProfile, FFmpegProfileAudioFormat, FFmpegProfileBitDepth, FFmpegProfileTonemapAlgorithm,
    FFmpegProfileVideoFormat, FilterMode, HardwareAccelerationKind, NormalizeLoudnessMode,
    Resolution, ScalingBehavior, StreamingMode, VideoScanKind,
};
use std::time::Duration;

use crate::{
    audio_pad_filter, audio_resample_filter, closed_gop, crop_filter,
    cuda_software_fallback_upload_filter, deinterlace_filter, fast_start,
    graphics_overlay_vaapi_filter, hardware_download_filter, hardware_upload_filter,
    hardware_upload_vaapi_filter, hide_banner, loglevel_error, mpegts_output, no_autoscale,
    no_b_frames, no_demux_decode_delay, no_scene_detect, no_standard_input, no_stats,
    normalize_loudness_filter, pad_filter, pipe_output, progress_to_stdout, realtime_filter,
    reset_pts_filter, scale_subtitle_image_filter, select_video_decoder, standard_format_flags,
    still_image_loop_filter, subtitle_overlay_cuda_filter, subtitle_overlay_filter,
    subtitle_overlay_vaapi_filter, subtitle_scale_npp_filter, subtitles_filter, time_limit,
    tonemap_filter_for, vaapi_subtitle_pixel_format_filter, watermark_alpha_format_filter,
    watermark_opacity_expression_filter, watermark_opacity_filter, watermark_overlay_cuda_filter,
    watermark_overlay_filter, watermark_scale_filter, AudioFilter, CommandGenerator, ComplexFilter,
    FfmpegCapability, FfmpegCapabilityReport, FfmpegCommand, FilterChain, FrameRate, FrameSize,
    FrameState, GraphicsOverlayFilter, HardwareCapabilitiesFactory, InputFile, InputKind,
    PipelineStep, PixelFormat, SourceVideoCodec, StreamKind, SubtitleMethod, TonemapAlgorithm,
    VaapiProfileEntrypoint, WatermarkOverlay,
};

const QSV_EXTRA_HARDWARE_FRAMES: u32 = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    MpegTs,
    Hls,
    HlsMp4,
    Mp4,
    Mkv,
    Nut,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HlsOutput {
    pub segment_template: String,
    pub init_template: Option<String>,
    pub playlist_path: String,
    pub segment_options: Option<String>,
    pub is_first_transcode: bool,
    pub troubleshooting: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OutputMetadata {
    pub service_provider: Option<String>,
    pub service_name: Option<String>,
    pub audio_language: Option<String>,
    pub subtitle_language: Option<String>,
    pub subtitle_title: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FfmpegState {
    pub output_format: OutputFormat,
    pub start_seconds: Option<i64>,
    pub duration: Option<Duration>,
    pub output_ts_offset: Option<Duration>,
    pub thread_count: Option<u32>,
    pub report_progress: bool,
    pub do_not_map_metadata: bool,
    pub metadata: OutputMetadata,
    pub hls: Option<HlsOutput>,
}

impl Default for FfmpegState {
    fn default() -> Self {
        Self {
            output_format: OutputFormat::MpegTs,
            start_seconds: None,
            duration: None,
            output_ts_offset: None,
            thread_count: None,
            report_progress: true,
            do_not_map_metadata: true,
            metadata: OutputMetadata::default(),
            hls: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PipelineRequest {
    pub inputs: Vec<InputFile>,
    pub state: FfmpegState,
    pub desired: DesiredVideoState,
    pub source_video_stream: Option<SourceVideoStream>,
    pub source_audio_streams: Vec<SourceAudioStream>,
    pub runtime_capabilities: Option<FfmpegCapabilityReport>,
    pub watermark: Option<WatermarkOverlay>,
    pub watermarks: Vec<WatermarkOverlay>,
    pub drawvg_filters: Vec<String>,
    pub graphics_overlays: Vec<GraphicsPipelineOverlay>,
    pub subtitle_fonts_dir: Option<String>,
}

impl PipelineRequest {
    #[must_use]
    pub fn new(input: InputFile, desired: DesiredVideoState) -> Self {
        let path = input.path.clone();
        Self {
            inputs: vec![input, InputFile::audio(path)],
            state: FfmpegState::default(),
            desired,
            source_video_stream: None,
            source_audio_streams: Vec::new(),
            runtime_capabilities: None,
            watermark: None,
            watermarks: Vec::new(),
            drawvg_filters: Vec::new(),
            graphics_overlays: Vec::new(),
            subtitle_fonts_dir: None,
        }
    }

    #[must_use]
    pub fn with_source_video_stream(mut self, stream: SourceVideoStream) -> Self {
        self.source_video_stream = Some(stream);
        self
    }

    #[must_use]
    pub fn with_source_audio_streams(mut self, streams: Vec<SourceAudioStream>) -> Self {
        self.source_audio_streams = streams;
        self
    }

    #[must_use]
    pub fn with_runtime_capabilities(mut self, report: FfmpegCapabilityReport) -> Self {
        self.runtime_capabilities = Some(report);
        self
    }

    #[must_use]
    pub fn with_subtitle_fonts_dir(mut self, fonts_dir: impl Into<String>) -> Self {
        self.subtitle_fonts_dir = Some(fonts_dir.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum GraphicsPipelineOverlay {
    Drawvg {
        z_index: i32,
        filter: String,
    },
    NativeImage {
        z_index: i32,
        input: InputFile,
        overlay: WatermarkOverlay,
    },
}

impl GraphicsPipelineOverlay {
    #[must_use]
    pub fn drawvg(z_index: i32, filter: impl Into<String>) -> Self {
        Self::Drawvg {
            z_index,
            filter: filter.into(),
        }
    }

    #[must_use]
    pub const fn native_image(z_index: i32, input: InputFile, overlay: WatermarkOverlay) -> Self {
        Self::NativeImage {
            z_index,
            input,
            overlay,
        }
    }

    const fn z_index(&self) -> i32 {
        match self {
            Self::Drawvg { z_index, .. } | Self::NativeImage { z_index, .. } => *z_index,
        }
    }
}

/// Source video stream metadata used for ErsatzTV-style decoder dispatch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceVideoStream {
    pub codec: SourceVideoCodec,
    pub profile: Option<String>,
    pub pixel_format: Option<PixelFormat>,
    pub color_params: SourceColorParams,
    pub has_multiple_profiles: bool,
    pub is_still_image: bool,
    pub scan_kind: VideoScanKind,
    pub frame_rate: Option<FrameRate>,
    pub frame_size: Option<FrameSize>,
    pub sample_aspect_ratio: Option<String>,
    pub display_aspect_ratio: Option<String>,
}

impl SourceVideoStream {
    #[must_use]
    pub const fn new(codec: SourceVideoCodec) -> Self {
        Self {
            codec,
            profile: None,
            pixel_format: None,
            color_params: SourceColorParams {
                is_hdr: false,
                is_bt2020_ten: false,
            },
            has_multiple_profiles: false,
            is_still_image: false,
            scan_kind: VideoScanKind::Unknown,
            frame_rate: None,
            frame_size: None,
            sample_aspect_ratio: None,
            display_aspect_ratio: None,
        }
    }

    #[must_use]
    pub fn is_anamorphic(&self) -> bool {
        let Some(size) = self.frame_size else {
            return false;
        };
        let sample_aspect_ratio = self.sample_aspect_ratio.as_deref().unwrap_or_default();
        if sample_aspect_ratio == "1:1" {
            return false;
        }
        if !sample_aspect_ratio.is_empty()
            && sample_aspect_ratio != "0:0"
            && sample_aspect_ratio != "0:1"
        {
            return true;
        }
        let display_aspect_ratio = self.display_aspect_ratio.as_deref().unwrap_or_default();
        if display_aspect_ratio.is_empty() || display_aspect_ratio == "0:1" {
            return false;
        }
        display_aspect_ratio != format!("{}:{}", size.width, size.height)
    }

    #[must_use]
    pub fn square_pixel_frame_size(&self, resolution: FrameSize) -> Option<FrameSize> {
        square_pixel_frame_size(self, resolution, SquarePixelScale::Fit)
    }

    #[must_use]
    pub fn square_pixel_frame_size_for_crop(&self, resolution: FrameSize) -> Option<FrameSize> {
        square_pixel_frame_size(self, resolution, SquarePixelScale::Fill)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SquarePixelScale {
    Fit,
    Fill,
}

fn square_pixel_frame_size(
    source: &SourceVideoStream,
    resolution: FrameSize,
    scale: SquarePixelScale,
) -> Option<FrameSize> {
    let source_size = source.frame_size?;
    let mut width = f64::from(source_size.width);
    let height = f64::from(source_size.height);

    if source.is_anamorphic() {
        width = (width * source.sample_aspect_ratio_factor()?).floor();
    }

    if width <= 0.0 || height <= 0.0 {
        return None;
    }

    let width_percent = f64::from(resolution.width) / width;
    let height_percent = f64::from(resolution.height) / height;
    let percent = match scale {
        SquarePixelScale::Fit => width_percent.min(height_percent),
        SquarePixelScale::Fill => width_percent.max(height_percent),
    };

    Some(even_frame_size(
        scaled_dimension(width, percent),
        scaled_dimension(height, percent),
    ))
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn scaled_dimension(value: f64, percent: f64) -> u32 {
    let scaled = (value * percent).floor();
    if !scaled.is_finite() || scaled <= 0.0 {
        return 0;
    }
    scaled.min(f64::from(u32::MAX)) as u32
}

impl SourceVideoStream {
    fn sample_aspect_ratio_factor(&self) -> Option<f64> {
        let sample_aspect_ratio = self.sample_aspect_ratio.as_deref().unwrap_or_default();
        if sample_aspect_ratio.is_empty() || sample_aspect_ratio == "0:0" {
            return self.display_aspect_ratio_factor();
        }
        parse_aspect_ratio(sample_aspect_ratio)
    }

    fn display_aspect_ratio_factor(&self) -> Option<f64> {
        let display_aspect_ratio = self.display_aspect_ratio.as_deref().unwrap_or_default();
        if display_aspect_ratio.is_empty() || display_aspect_ratio == "0:1" {
            return Some(1.0);
        }
        let dar = parse_aspect_ratio(display_aspect_ratio)?;
        let size = self.frame_size?;
        let source_ratio = f64::from(size.width) / f64::from(size.height);
        Some(dar / source_ratio)
    }
}

fn parse_aspect_ratio(value: &str) -> Option<f64> {
    let trimmed = value.trim();
    if let Some((left, right)) = trimmed.split_once(':') {
        let numerator = left.parse::<f64>().ok()?;
        let denominator = right.parse::<f64>().ok()?;
        if denominator == 0.0 {
            return None;
        }
        return Some(numerator / denominator);
    }
    trimmed.parse::<f64>().ok()
}

const fn even_frame_size(width: u32, height: u32) -> FrameSize {
    FrameSize::new(width + (width % 2), height + (height % 2))
}

/// Source audio stream metadata reserved for audio decoder dispatch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceAudioStream {
    pub index: usize,
    pub codec: SourceAudioCodec,
    pub channels: Option<u32>,
    pub sample_rate: Option<u32>,
    pub language: Option<String>,
    pub title: Option<String>,
}

impl SourceAudioStream {
    #[must_use]
    pub const fn new(codec: SourceAudioCodec) -> Self {
        Self {
            index: 1,
            codec,
            channels: None,
            sample_rate: None,
            language: None,
            title: None,
        }
    }

    #[must_use]
    pub const fn with_index(mut self, index: usize) -> Self {
        self.index = index;
        self
    }
}

/// Source audio codecs understood by the pipeline request model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceAudioCodec {
    Aac,
    AacLatm,
    Ac3,
    Dts,
    Unknown,
}

impl SourceAudioCodec {
    #[must_use]
    pub fn from_ffmpeg_name(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "aac" => Self::Aac,
            "aac_latm" | "aac-latm" => Self::AacLatm,
            "ac3" | "eac3" => Self::Ac3,
            "dts" => Self::Dts,
            _ => Self::Unknown,
        }
    }
}

/// Source color metadata that affects later filter and decoder choices.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SourceColorParams {
    pub is_hdr: bool,
    pub is_bt2020_ten: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VulkanHdrTonemapMode {
    #[default]
    Auto,
    Disabled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RealtimeMode {
    #[default]
    Off,
    On,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DesiredVideoState {
    pub scaled_size: FrameSize,
    pub frame_size: FrameSize,
    pub cropped_size: Option<FrameSize>,
    pub pad_mode: FilterMode,
    pub frame_rate: Option<FrameRate>,
    pub video_format: FFmpegProfileVideoFormat,
    pub audio_format: FFmpegProfileAudioFormat,
    pub bit_depth: FFmpegProfileBitDepth,
    pub tonemap_algorithm: FFmpegProfileTonemapAlgorithm,
    pub video_profile: Option<String>,
    pub video_preset: Option<String>,
    pub video_bitrate: Option<u32>,
    pub video_buffer_size: Option<u32>,
    pub video_track_timescale: Option<u32>,
    pub audio_bitrate: Option<u32>,
    pub audio_buffer_size: Option<u32>,
    pub audio_channels: Option<u32>,
    pub audio_sample_rate: Option<u32>,
    pub pad_audio: bool,
    pub realtime_mode: RealtimeMode,
    pub normalize_loudness_mode: NormalizeLoudnessMode,
    pub target_loudness: Option<f64>,
    pub normalize_colors: bool,
    pub allow_b_frames: bool,
    pub vaapi_device: Option<String>,
    pub vaapi_driver: Option<String>,
    pub qsv_device: Option<String>,
    pub qsv_extra_hardware_frames: u32,
    pub vulkan_hdr_tonemap_mode: VulkanHdrTonemapMode,
}

impl DesiredVideoState {
    #[must_use]
    pub fn h264_1080p() -> Self {
        Self {
            scaled_size: FrameSize::new(1920, 1080),
            frame_size: FrameSize::new(1920, 1080),
            cropped_size: None,
            pad_mode: FilterMode::Software,
            frame_rate: Some(FrameRate::DEFAULT),
            video_format: FFmpegProfileVideoFormat::H264,
            audio_format: FFmpegProfileAudioFormat::Aac,
            bit_depth: FFmpegProfileBitDepth::EightBit,
            tonemap_algorithm: FFmpegProfileTonemapAlgorithm::Hable,
            video_profile: Some("high".to_owned()),
            video_preset: Some("veryfast".to_owned()),
            video_bitrate: Some(8_000),
            video_buffer_size: Some(16_000),
            video_track_timescale: Some(90_000),
            audio_bitrate: Some(192),
            audio_buffer_size: Some(384),
            audio_channels: Some(2),
            audio_sample_rate: Some(48_000),
            pad_audio: true,
            realtime_mode: RealtimeMode::Off,
            normalize_loudness_mode: NormalizeLoudnessMode::Off,
            target_loudness: None,
            normalize_colors: false,
            allow_b_frames: false,
            vaapi_device: None,
            vaapi_driver: None,
            qsv_device: None,
            qsv_extra_hardware_frames: QSV_EXTRA_HARDWARE_FRAMES,
            vulkan_hdr_tonemap_mode: VulkanHdrTonemapMode::Auto,
        }
    }

    #[must_use]
    pub fn from_profile(
        profile: &FFmpegProfile,
        resolution: &Resolution,
        source: Option<&SourceVideoStream>,
        target_frame_rate: Option<FrameRate>,
        streaming_mode: StreamingMode,
    ) -> Self {
        if streaming_mode == StreamingMode::HttpLiveStreamingDirect {
            return Self::copy_from_profile(profile, resolution, streaming_mode);
        }

        let frame_size = resolution_frame_size(resolution);
        let (scaled_size, cropped_size) = desired_sizes(profile, frame_size, source);
        let frame_rate = profile
            .normalize_framerate
            .then_some(target_frame_rate)
            .flatten();
        let audio_sample_rate = positive_u32(profile.audio_sample_rate)
            .map(|sample_rate_khz| sample_rate_khz.saturating_mul(1_000));
        let normalize_loudness_mode = profile.normalize_loudness_mode;
        Self {
            scaled_size,
            frame_size,
            cropped_size,
            pad_mode: profile.pad_mode,
            frame_rate,
            video_format: profile.video_format,
            audio_format: profile.audio_format,
            bit_depth: effective_bit_depth(profile),
            tonemap_algorithm: profile.tonemap_algorithm,
            video_profile: non_empty_string(profile.video_profile.as_deref()),
            video_preset: non_empty_string(profile.video_preset.as_deref()),
            video_bitrate: positive_u32(profile.video_bitrate),
            video_buffer_size: positive_u32(profile.video_buffer_size),
            video_track_timescale: Some(90_000),
            audio_bitrate: positive_u32(profile.audio_bitrate),
            audio_buffer_size: positive_u32(profile.audio_buffer_size),
            audio_channels: positive_u32(profile.audio_channels),
            audio_sample_rate,
            pad_audio: profile.audio_format != FFmpegProfileAudioFormat::Copy,
            realtime_mode: realtime_mode_for_streaming(streaming_mode),
            normalize_loudness_mode,
            target_loudness: (normalize_loudness_mode == NormalizeLoudnessMode::LoudNorm)
                .then_some(profile.target_loudness)
                .flatten(),
            normalize_colors: profile.normalize_colors,
            allow_b_frames: profile.allow_b_frames,
            vaapi_device: non_empty_string(profile.vaapi_device.as_deref()),
            vaapi_driver: vaapi_driver_name(profile.vaapi_driver).map(str::to_owned),
            qsv_device: non_empty_string(profile.vaapi_device.as_deref()),
            qsv_extra_hardware_frames: profile
                .qsv_extra_hardware_frames
                .and_then(positive_u32)
                .unwrap_or(QSV_EXTRA_HARDWARE_FRAMES),
            vulkan_hdr_tonemap_mode: VulkanHdrTonemapMode::Auto,
        }
    }

    fn copy_from_profile(
        profile: &FFmpegProfile,
        resolution: &Resolution,
        streaming_mode: StreamingMode,
    ) -> Self {
        let frame_size = resolution_frame_size(resolution);
        Self {
            scaled_size: frame_size,
            frame_size,
            cropped_size: None,
            pad_mode: profile.pad_mode,
            frame_rate: None,
            video_format: FFmpegProfileVideoFormat::Copy,
            audio_format: FFmpegProfileAudioFormat::Copy,
            bit_depth: effective_bit_depth(profile),
            tonemap_algorithm: profile.tonemap_algorithm,
            video_profile: None,
            video_preset: None,
            video_bitrate: None,
            video_buffer_size: None,
            video_track_timescale: Some(90_000),
            audio_bitrate: None,
            audio_buffer_size: None,
            audio_channels: None,
            audio_sample_rate: None,
            pad_audio: false,
            realtime_mode: realtime_mode_for_streaming(streaming_mode),
            normalize_loudness_mode: NormalizeLoudnessMode::Off,
            target_loudness: None,
            normalize_colors: profile.normalize_colors,
            allow_b_frames: true,
            vaapi_device: non_empty_string(profile.vaapi_device.as_deref()),
            vaapi_driver: vaapi_driver_name(profile.vaapi_driver).map(str::to_owned),
            qsv_device: non_empty_string(profile.vaapi_device.as_deref()),
            qsv_extra_hardware_frames: profile
                .qsv_extra_hardware_frames
                .and_then(positive_u32)
                .unwrap_or(QSV_EXTRA_HARDWARE_FRAMES),
            vulkan_hdr_tonemap_mode: VulkanHdrTonemapMode::Auto,
        }
    }

    #[must_use]
    pub const fn pixel_format(&self) -> PixelFormat {
        match self.bit_depth {
            FFmpegProfileBitDepth::EightBit => PixelFormat::Yuv420P,
            FFmpegProfileBitDepth::TenBit => PixelFormat::Yuv420P10Le,
        }
    }
}

fn desired_sizes(
    profile: &FFmpegProfile,
    frame_size: FrameSize,
    source: Option<&SourceVideoStream>,
) -> (FrameSize, Option<FrameSize>) {
    let Some(source) = source else {
        return (frame_size, None);
    };
    if source.frame_size.is_none() {
        return (frame_size, None);
    }

    if profile.scaling_behavior == ScalingBehavior::Stretch {
        return (frame_size, None);
    }

    if profile.scaling_behavior == ScalingBehavior::Crop {
        let scaled = source
            .square_pixel_frame_size_for_crop(frame_size)
            .unwrap_or(frame_size);
        return (scaled, Some(frame_size));
    }

    let scaled = source
        .square_pixel_frame_size(frame_size)
        .unwrap_or(frame_size);
    (scaled, None)
}

fn resolution_frame_size(resolution: &Resolution) -> FrameSize {
    FrameSize::new(
        positive_u32(resolution.width).unwrap_or(1920),
        positive_u32(resolution.height).unwrap_or(1080),
    )
}

fn effective_bit_depth(profile: &FFmpegProfile) -> FFmpegProfileBitDepth {
    if profile.bit_depth == FFmpegProfileBitDepth::TenBit
        && profile.video_format != FFmpegProfileVideoFormat::Mpeg2Video
    {
        FFmpegProfileBitDepth::TenBit
    } else {
        FFmpegProfileBitDepth::EightBit
    }
}

fn positive_u32(value: i32) -> Option<u32> {
    u32::try_from(value).ok().filter(|value| *value > 0)
}

fn non_empty_string(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

const fn realtime_mode_for_streaming(streaming_mode: StreamingMode) -> RealtimeMode {
    if matches!(streaming_mode, StreamingMode::HttpLiveStreamingSegmenter) {
        RealtimeMode::Off
    } else {
        RealtimeMode::On
    }
}

const fn vaapi_driver_name(driver: ersatzrs_domain::VaapiDriver) -> Option<&'static str> {
    match driver {
        ersatzrs_domain::VaapiDriver::Auto => None,
        ersatzrs_domain::VaapiDriver::IHD => Some("iHD"),
        ersatzrs_domain::VaapiDriver::I965 => Some("i965"),
        ersatzrs_domain::VaapiDriver::RadeonSI => Some("radeonsi"),
        ersatzrs_domain::VaapiDriver::Nouveau => Some("nouveau"),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FFmpegPipeline {
    pub steps: Vec<PipelineStep>,
    pub inputs: Vec<InputFile>,
    pub is_intel_vaapi_or_qsv: bool,
    pub filter_graph: Option<String>,
}

impl FFmpegPipeline {
    #[must_use]
    pub fn command(&self, inputs: &[InputFile]) -> FfmpegCommand {
        let effective_inputs = if self.inputs.is_empty() {
            inputs
        } else {
            &self.inputs
        };
        CommandGenerator::generate_command(
            effective_inputs,
            &self.steps,
            self.is_intel_vaapi_or_qsv,
        )
    }
}

pub trait PipelineBuilder {
    fn build(&self, request: PipelineRequest) -> FFmpegPipeline;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SoftwarePipelineBuilder;

impl PipelineBuilder for SoftwarePipelineBuilder {
    fn build(&self, request: PipelineRequest) -> FFmpegPipeline {
        build_transcode_pipeline(request, HardwareAccelerationKind::None)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NvencPipelineBuilder;

impl PipelineBuilder for NvencPipelineBuilder {
    fn build(&self, request: PipelineRequest) -> FFmpegPipeline {
        build_transcode_pipeline(request, HardwareAccelerationKind::Nvenc)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VaapiPipelineBuilder;

impl PipelineBuilder for VaapiPipelineBuilder {
    fn build(&self, request: PipelineRequest) -> FFmpegPipeline {
        build_transcode_pipeline(request, HardwareAccelerationKind::Vaapi)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QsvPipelineBuilder;

impl PipelineBuilder for QsvPipelineBuilder {
    fn build(&self, request: PipelineRequest) -> FFmpegPipeline {
        build_transcode_pipeline(request, HardwareAccelerationKind::Qsv)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VideoToolboxPipelineBuilder;

impl PipelineBuilder for VideoToolboxPipelineBuilder {
    fn build(&self, request: PipelineRequest) -> FFmpegPipeline {
        build_transcode_pipeline(request, HardwareAccelerationKind::VideoToolbox)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AmfPipelineBuilder;

impl PipelineBuilder for AmfPipelineBuilder {
    fn build(&self, request: PipelineRequest) -> FFmpegPipeline {
        build_transcode_pipeline(request, HardwareAccelerationKind::Amf)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct V4l2m2mPipelineBuilder;

impl PipelineBuilder for V4l2m2mPipelineBuilder {
    fn build(&self, request: PipelineRequest) -> FFmpegPipeline {
        build_transcode_pipeline(request, HardwareAccelerationKind::V4l2m2m)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RkmppPipelineBuilder;

impl PipelineBuilder for RkmppPipelineBuilder {
    fn build(&self, request: PipelineRequest) -> FFmpegPipeline {
        build_transcode_pipeline(request, HardwareAccelerationKind::Rkmpp)
    }
}

#[must_use]
pub fn build_pipeline_for_acceleration(
    acceleration: HardwareAccelerationKind,
    request: PipelineRequest,
) -> FFmpegPipeline {
    match acceleration {
        HardwareAccelerationKind::None => SoftwarePipelineBuilder.build(request),
        HardwareAccelerationKind::Nvenc => NvencPipelineBuilder.build(request),
        HardwareAccelerationKind::Vaapi => VaapiPipelineBuilder.build(request),
        HardwareAccelerationKind::Qsv => QsvPipelineBuilder.build(request),
        HardwareAccelerationKind::VideoToolbox => VideoToolboxPipelineBuilder.build(request),
        HardwareAccelerationKind::Amf => AmfPipelineBuilder.build(request),
        HardwareAccelerationKind::V4l2m2m => V4l2m2mPipelineBuilder.build(request),
        HardwareAccelerationKind::Rkmpp => RkmppPipelineBuilder.build(request),
    }
}

#[allow(clippy::too_many_lines)]
fn build_transcode_pipeline(
    mut request: PipelineRequest,
    acceleration: HardwareAccelerationKind,
) -> FFmpegPipeline {
    let effective_acceleration = effective_video_acceleration(&request, acceleration);
    let effective_decode_acceleration =
        effective_decode_acceleration(&request, acceleration, effective_acceleration);
    let mut steps = vec![
        no_standard_input(),
        hide_banner(),
        no_stats(),
        loglevel_error(),
        standard_format_flags(),
        no_demux_decode_delay(),
        container_movflags(&request.state, &request.desired),
    ];

    if request.state.report_progress {
        steps.insert(3, progress_to_stdout());
    }

    if let Some(thread_count) = request.state.thread_count {
        steps.insert(
            0,
            PipelineStep::global("threads", ["-threads".to_owned(), thread_count.to_string()]),
        );
    }

    if let Some(start_seconds) = request.state.start_seconds {
        steps.push(PipelineStep::stream_seek(start_seconds));
    }

    let is_video_copy = request.desired.video_format == FFmpegProfileVideoFormat::Copy;
    if !is_video_copy {
        steps.push(closed_gop());
        if !request.desired.allow_b_frames {
            steps.push(no_b_frames());
        }
    }

    let mut filter_chain = FilterChain::default();
    let mut target_state = configure_hardware_acceleration(
        &mut request,
        &mut steps,
        effective_acceleration,
        effective_decode_acceleration,
    );
    if !is_video_copy {
        steps.push(no_scene_detect(scene_detect_threshold(
            &request,
            effective_decode_acceleration,
        )));
    }
    let use_cuda_overlay = uses_cuda_overlay(effective_acceleration, &request.desired);
    add_video_decoder(&request, &mut steps, effective_acceleration);
    add_audio_decoder_input_options(&mut request);
    add_realtime_input_options(&mut request);
    add_cuda_software_fallback_upload(&mut request, effective_acceleration);
    apply_source_video_filters(
        &request,
        effective_acceleration,
        &mut target_state,
        &mut filter_chain,
    );
    apply_still_image_loop_filter(&request, &mut filter_chain);
    apply_qsv_download_and_reset_pts(
        &request,
        effective_acceleration,
        &mut target_state,
        &mut filter_chain,
    );

    if let Some(watermark) = &request.watermark {
        request.watermarks.push(watermark.clone());
    }

    request
        .watermarks
        .sort_by_key(|watermark| watermark.z_index);
    let has_watermark_overlay = request
        .inputs
        .iter()
        .any(|input| input.kind == InputKind::Watermark);
    let has_image_subtitle_burn = request.inputs.iter().any(|input| {
        input.kind == InputKind::Subtitle
            && input.is_image_based_subtitle
            && input.subtitle_method == Some(SubtitleMethod::Burn)
    });
    let has_native_graphics_overlay = request
        .graphics_overlays
        .iter()
        .any(|overlay| matches!(overlay, GraphicsPipelineOverlay::NativeImage { .. }));
    let force_vaapi_software_overlay = force_vaapi_software_overlay(
        &request.desired,
        has_watermark_overlay,
        has_image_subtitle_burn,
    );
    download_cuda_frames_for_software_overlay(
        effective_acceleration,
        &request.desired,
        has_watermark_overlay || has_image_subtitle_burn || has_native_graphics_overlay,
        &mut target_state,
        &mut filter_chain,
    );
    force_vaapi_frames_to_software_for_overlay(
        effective_acceleration,
        force_vaapi_software_overlay,
        has_watermark_overlay || has_image_subtitle_burn || has_native_graphics_overlay,
        &request.desired,
        &mut target_state,
        &mut filter_chain,
    );
    let use_vaapi_overlay = uses_vaapi_overlay(
        effective_acceleration,
        &request.desired,
        &target_state,
        force_vaapi_software_overlay,
    );
    for (input, watermark) in request
        .inputs
        .iter_mut()
        .filter(|input| input.kind == InputKind::Watermark)
        .zip(request.watermarks.iter())
    {
        if watermark.size == ersatzrs_domain::WatermarkSize::Scaled {
            input.filter_steps.push(watermark_scale_filter(
                watermark,
                request.desired.frame_size,
            ));
        }
        if let Some(expression) = watermark
            .opacity_expression
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            input
                .filter_steps
                .push(watermark_opacity_expression_filter(expression));
        } else if watermark.opacity < 100 {
            input
                .filter_steps
                .push(watermark_opacity_filter(watermark.opacity));
        }
        input.filter_steps.extend(
            watermark
                .fade_points
                .iter()
                .copied()
                .map(crate::watermark_fade_filter),
        );
        let square_pixel_frame_size = request
            .source_video_stream
            .as_ref()
            .and_then(|stream| stream.square_pixel_frame_size(request.desired.frame_size))
            .unwrap_or(request.desired.frame_size);
        if use_cuda_overlay {
            input.filter_steps.push(watermark_alpha_format_filter());
            input.filter_steps.push(hardware_upload_filter(
                HardwareAccelerationKind::Nvenc,
                QSV_EXTRA_HARDWARE_FRAMES,
            ));
            filter_chain
                .watermark_overlay
                .push(watermark_overlay_cuda_filter(
                    watermark,
                    request.desired.frame_size,
                    square_pixel_frame_size,
                ));
        } else {
            filter_chain
                .watermark_overlay
                .push(watermark_overlay_filter(
                    watermark,
                    request.desired.frame_size,
                    square_pixel_frame_size,
                    request.desired.pixel_format(),
                ));
        }
    }

    if has_image_subtitle_burn {
        if use_vaapi_overlay {
            filter_chain
                .subtitle
                .push(vaapi_subtitle_pixel_format_filter());
            filter_chain
                .subtitle
                .push(scale_subtitle_image_filter(request.desired.frame_size));
            filter_chain
                .subtitle
                .push(hardware_upload_vaapi_filter(false));
            filter_chain
                .subtitle_overlay
                .push(subtitle_overlay_vaapi_filter());
        } else if use_cuda_overlay {
            if use_cuda_subtitle_scale_npp(&request) {
                filter_chain.subtitle.push(watermark_alpha_format_filter());
                filter_chain.subtitle.push(hardware_upload_filter(
                    HardwareAccelerationKind::Nvenc,
                    QSV_EXTRA_HARDWARE_FRAMES,
                ));
                filter_chain
                    .subtitle
                    .push(subtitle_scale_npp_filter(request.desired.frame_size));
            } else {
                filter_chain
                    .subtitle
                    .push(scale_subtitle_image_filter(request.desired.frame_size));
                filter_chain.subtitle.push(watermark_alpha_format_filter());
                filter_chain.subtitle.push(hardware_upload_filter(
                    HardwareAccelerationKind::Nvenc,
                    QSV_EXTRA_HARDWARE_FRAMES,
                ));
            }
            filter_chain
                .subtitle_overlay
                .push(subtitle_overlay_cuda_filter());
        } else {
            filter_chain
                .subtitle
                .push(scale_subtitle_image_filter(request.desired.frame_size));
            filter_chain
                .subtitle_overlay
                .push(subtitle_overlay_filter(request.desired.pixel_format()));
        }
    }

    apply_text_subtitle_burn_filters(
        &request,
        effective_acceleration,
        use_cuda_overlay,
        has_watermark_overlay || has_native_graphics_overlay || !request.drawvg_filters.is_empty(),
        &mut target_state,
        &mut filter_chain,
    );

    if request.desired.audio_format != FFmpegProfileAudioFormat::Copy {
        filter_chain.audio.push(normalize_loudness_filter(
            match request.desired.normalize_loudness_mode {
                NormalizeLoudnessMode::LoudNorm => AudioFilter::LoudNorm,
                NormalizeLoudnessMode::Off => AudioFilter::None,
            },
            request.desired.target_loudness,
            request
                .desired
                .audio_sample_rate
                .map(|sample_rate_hz| sample_rate_hz / 1_000),
        ));
        filter_chain.audio.push(audio_resample_filter());
        if request.desired.pad_audio {
            filter_chain.audio.push(audio_pad_filter());
        }
    }

    request.graphics_overlays.extend(
        request
            .drawvg_filters
            .iter()
            .cloned()
            .map(|filter| GraphicsPipelineOverlay::drawvg(0, filter)),
    );
    request
        .graphics_overlays
        .sort_by_key(GraphicsPipelineOverlay::z_index);
    let has_drawvg_graphics = request
        .graphics_overlays
        .iter()
        .any(|overlay| matches!(overlay, GraphicsPipelineOverlay::Drawvg { .. }));
    let emit_pixel_format_output_option =
        should_emit_pixel_format_output_option(&request, effective_acceleration, &target_state);
    let runtime_capabilities = request.runtime_capabilities.clone();
    let use_nvidia_green_line_workaround = nvidia_green_line_workaround_applies(
        &request,
        effective_acceleration,
        has_watermark_overlay,
        has_image_subtitle_burn,
        has_native_graphics_overlay || has_drawvg_graphics,
    );
    let mut graphics_image_index = 0usize;
    for overlay in request.graphics_overlays {
        match overlay {
            GraphicsPipelineOverlay::Drawvg { filter, .. } => {
                filter_chain
                    .graphics_overlay
                    .push(GraphicsOverlayFilter::Drawvg(filter));
            }
            GraphicsPipelineOverlay::NativeImage {
                mut input, overlay, ..
            } => {
                if overlay.size == ersatzrs_domain::WatermarkSize::Scaled {
                    input
                        .filter_steps
                        .push(watermark_scale_filter(&overlay, request.desired.frame_size));
                }
                if let Some(expression) = overlay
                    .opacity_expression
                    .as_deref()
                    .filter(|value| !value.trim().is_empty())
                {
                    input
                        .filter_steps
                        .push(watermark_opacity_expression_filter(expression));
                } else if overlay.opacity < 100 {
                    input
                        .filter_steps
                        .push(watermark_opacity_filter(overlay.opacity));
                }
                input.filter_steps.extend(
                    overlay
                        .fade_points
                        .iter()
                        .copied()
                        .map(crate::watermark_fade_filter),
                );
                let square_pixel_frame_size = request
                    .source_video_stream
                    .as_ref()
                    .and_then(|stream| stream.square_pixel_frame_size(request.desired.frame_size))
                    .unwrap_or(request.desired.frame_size);
                let filter = if use_vaapi_overlay {
                    input.filter_steps.push(hardware_upload_vaapi_filter(false));
                    graphics_overlay_vaapi_filter()
                } else if use_cuda_overlay {
                    input.filter_steps.push(watermark_alpha_format_filter());
                    input.filter_steps.push(hardware_upload_filter(
                        HardwareAccelerationKind::Nvenc,
                        QSV_EXTRA_HARDWARE_FRAMES,
                    ));
                    watermark_overlay_cuda_filter(
                        &overlay,
                        request.desired.frame_size,
                        square_pixel_frame_size,
                    )
                } else {
                    watermark_overlay_filter(
                        &overlay,
                        request.desired.frame_size,
                        square_pixel_frame_size,
                        request.desired.pixel_format(),
                    )
                };
                request.inputs.push(input);
                filter_chain
                    .graphics_overlay
                    .push(GraphicsOverlayFilter::NativeImage {
                        input_index: graphics_image_index,
                        filter,
                    });
                graphics_image_index = graphics_image_index.saturating_add(1);
            }
        }
    }
    if request.desired.video_format != FFmpegProfileVideoFormat::Copy
        && effective_acceleration == HardwareAccelerationKind::None
        && has_drawvg_graphics
    {
        filter_chain.pixel_format.push(format!(
            "format={}",
            request.desired.pixel_format().ffmpeg_name()
        ));
    }

    let is_intel = matches!(
        effective_acceleration,
        HardwareAccelerationKind::Vaapi | HardwareAccelerationKind::Qsv
    );
    let complex = ComplexFilter::build(&request.inputs, filter_chain, is_intel);
    let filter_graph = complex.filter_graph.clone();

    steps.push(video_encoder(
        effective_acceleration,
        &request.desired,
        runtime_capabilities.as_ref(),
    ));

    // Port of `SoftwarePipelineBuilder.SetPixelFormat`: when the source frame
    // is in software memory and its pixel format differs from the desired
    // output format, emit a `-pix_fmt` output option. This is what converts a
    // 10-bit (`yuv420p10le`) source down to 8-bit `yuv420p` so the libx264
    // `high` profile (8-bit only) can open the encoder. Gated on a known source
    // stream so pipelines built without probe metadata are unchanged.
    if emit_pixel_format_output_option {
        steps.push(pixel_format_output_option(request.desired.pixel_format()));
    }

    steps.push(PipelineStep::complex_filter(complex.options));
    steps.push(audio_encoder(
        &request.desired,
        selected_source_audio_stream(
            request.inputs.as_slice(),
            request.source_audio_streams.as_slice(),
        ),
    ));
    if let Some(encoder) = subtitle_encoder(&request.inputs) {
        steps.push(encoder);
    }

    add_output_options(
        &mut steps,
        &request.state,
        &request.desired,
        effective_acceleration,
        use_nvidia_green_line_workaround,
    );

    FFmpegPipeline {
        steps,
        inputs: request.inputs,
        is_intel_vaapi_or_qsv: is_intel,
        filter_graph,
    }
}

fn apply_source_video_filters(
    request: &PipelineRequest,
    acceleration: HardwareAccelerationKind,
    target_state: &mut FrameState,
    filter_chain: &mut FilterChain,
) {
    let is_transcode = request.desired.video_format != FFmpegProfileVideoFormat::Copy;

    if let Some(source) = &request.source_video_stream {
        target_state.should_deinterlace =
            source.scan_kind == VideoScanKind::Interlaced && is_transcode;
        target_state.is_hdr = source.color_params.is_hdr || source.color_params.is_bt2020_ten;

        // Track the true source pixel format while the frame lives in software
        // memory so `SetPixelFormat` can decide whether an output `-pix_fmt`
        // conversion is required (e.g. 10-bit source into an 8-bit H.264 high
        // profile). Hardware paths manage their own surface format in
        // `configure_hardware_acceleration`, so only override for software.
        if target_state.frame_data_location == crate::FrameDataLocation::Software {
            if let Some(pixel_format) = source.pixel_format {
                target_state.pixel_format = pixel_format;
            }
        }
    }

    if target_state.should_deinterlace && !target_state.deinterlaced {
        filter_chain.video.push(deinterlace_filter(
            acceleration,
            target_state,
            request.desired.qsv_extra_hardware_frames,
        ));
        target_state.deinterlaced = true;
        target_state.frame_data_location = deinterlace_output_location(acceleration);
    }

    if target_state.is_hdr && is_transcode {
        filter_chain.video.push(tonemap_filter_for(
            acceleration,
            target_state,
            request.desired.pixel_format(),
            tonemap_algorithm(request.desired.tonemap_algorithm),
        ));
        target_state.is_hdr = false;
        target_state.pixel_format = request.desired.pixel_format();
        target_state.frame_data_location = tonemap_output_location(acceleration);
    }

    if !is_transcode {
        return;
    }

    if let Some(cropped_size) = request.desired.cropped_size {
        target_state.cropped_size = Some(cropped_size);
    }

    let scale = video_scale_filter(
        acceleration,
        target_state,
        request.desired.scaled_size,
        request.desired.frame_size,
        request.desired.bit_depth,
        request.desired.qsv_extra_hardware_frames,
    );
    if !scale.trim().is_empty() {
        filter_chain.video.push(scale);
        target_state.scaled_size = request.desired.scaled_size;
        target_state.padded_size = request.desired.scaled_size;
        if acceleration == HardwareAccelerationKind::Qsv {
            target_state.frame_data_location = crate::FrameDataLocation::Hardware;
            target_state.pixel_format = match request.desired.bit_depth {
                FFmpegProfileBitDepth::EightBit => PixelFormat::Nv12,
                FFmpegProfileBitDepth::TenBit => PixelFormat::Yuv420P10Le,
            };
        }
    }

    if request.desired.cropped_size.is_none()
        && target_state.padded_size != request.desired.frame_size
    {
        filter_chain
            .video
            .push(pad_filter(target_state, request.desired.frame_size));
        target_state.padded_size = request.desired.frame_size;
        target_state.scaled_size = request.desired.frame_size;
        target_state.frame_data_location = crate::FrameDataLocation::Software;
        target_state.pixel_format = request.desired.pixel_format();
        upload_software_frames_for_hardware_encoder(acceleration, target_state, filter_chain);
    }

    if let Some(cropped_size) = request.desired.cropped_size {
        filter_chain
            .video
            .push(crop_filter(target_state, cropped_size));
        target_state.cropped_size = None;
        target_state.scaled_size = cropped_size;
        target_state.padded_size = cropped_size;
        target_state.frame_data_location = crate::FrameDataLocation::Software;
        target_state.pixel_format = request.desired.pixel_format();
        upload_software_frames_for_hardware_encoder(acceleration, target_state, filter_chain);
    }
}

fn apply_still_image_loop_filter(request: &PipelineRequest, filter_chain: &mut FilterChain) {
    if request.desired.video_format == FFmpegProfileVideoFormat::Copy {
        return;
    }

    if request
        .source_video_stream
        .as_ref()
        .is_some_and(|source| source.is_still_image)
    {
        filter_chain.video.push(still_image_loop_filter());
        if request.desired.realtime_mode == RealtimeMode::On {
            filter_chain.video.push(realtime_filter());
        }
    }
}

fn upload_software_frames_for_hardware_encoder(
    acceleration: HardwareAccelerationKind,
    target_state: &mut FrameState,
    filter_chain: &mut FilterChain,
) {
    if acceleration != HardwareAccelerationKind::Vaapi
        || target_state.frame_data_location != crate::FrameDataLocation::Software
    {
        return;
    }

    filter_chain.video.push(hardware_upload_filter(
        acceleration,
        QSV_EXTRA_HARDWARE_FRAMES,
    ));
    target_state.frame_data_location = crate::FrameDataLocation::Hardware;
    target_state.pixel_format = PixelFormat::Vaapi;
}

fn uses_cuda_overlay(acceleration: HardwareAccelerationKind, desired: &DesiredVideoState) -> bool {
    acceleration == HardwareAccelerationKind::Nvenc
        && desired.video_format != FFmpegProfileVideoFormat::Copy
        && desired.bit_depth == FFmpegProfileBitDepth::EightBit
}

fn uses_vaapi_overlay(
    acceleration: HardwareAccelerationKind,
    desired: &DesiredVideoState,
    target_state: &FrameState,
    force_software_overlay: bool,
) -> bool {
    acceleration == HardwareAccelerationKind::Vaapi
        && desired.video_format != FFmpegProfileVideoFormat::Copy
        && !force_software_overlay
        && target_state.frame_data_location == crate::FrameDataLocation::Hardware
}

fn force_vaapi_software_overlay(
    desired: &DesiredVideoState,
    has_watermark: bool,
    has_image_subtitle: bool,
) -> bool {
    (has_image_subtitle && has_watermark) || desired.vaapi_driver.as_deref() == Some("radeonsi")
}

fn force_vaapi_frames_to_software_for_overlay(
    acceleration: HardwareAccelerationKind,
    force_software_overlay: bool,
    has_overlay: bool,
    desired: &DesiredVideoState,
    target_state: &mut FrameState,
    filter_chain: &mut FilterChain,
) {
    if acceleration != HardwareAccelerationKind::Vaapi
        || !force_software_overlay
        || !has_overlay
        || target_state.frame_data_location != crate::FrameDataLocation::Hardware
    {
        return;
    }

    filter_chain
        .video
        .push(hardware_download_filter(target_state));
    target_state.frame_data_location = crate::FrameDataLocation::Software;
    target_state.pixel_format = desired.pixel_format();
}

fn use_cuda_subtitle_scale_npp(request: &PipelineRequest) -> bool {
    request
        .runtime_capabilities
        .as_ref()
        .is_some_and(|report| report.has_filter("scale_npp"))
        && primary_video_needs_spatial_filter(request)
}

fn primary_video_needs_spatial_filter(request: &PipelineRequest) -> bool {
    request.desired.cropped_size.is_some()
        || request.desired.scaled_size != request.desired.frame_size
        || request
            .source_video_stream
            .as_ref()
            .and_then(|source| source.frame_size)
            .is_some_and(|frame_size| frame_size != request.desired.scaled_size)
}

fn download_cuda_frames_for_software_overlay(
    acceleration: HardwareAccelerationKind,
    desired: &DesiredVideoState,
    has_software_overlay: bool,
    target_state: &mut FrameState,
    filter_chain: &mut FilterChain,
) {
    if acceleration != HardwareAccelerationKind::Nvenc
        || desired.bit_depth != FFmpegProfileBitDepth::TenBit
        || !has_software_overlay
        || target_state.frame_data_location != crate::FrameDataLocation::Hardware
    {
        return;
    }

    filter_chain
        .video
        .push(hardware_download_filter(target_state));
    target_state.frame_data_location = crate::FrameDataLocation::Software;
    target_state.pixel_format = desired.pixel_format();
}

fn apply_text_subtitle_burn_filters(
    request: &PipelineRequest,
    acceleration: HardwareAccelerationKind,
    use_cuda_overlay: bool,
    has_later_cuda_overlay: bool,
    target_state: &mut FrameState,
    filter_chain: &mut FilterChain,
) {
    for input in request.inputs.iter().filter(|input| {
        input.kind == InputKind::Subtitle
            && !input.is_image_based_subtitle
            && input.subtitle_method == Some(SubtitleMethod::Burn)
    }) {
        if matches!(
            acceleration,
            HardwareAccelerationKind::Nvenc | HardwareAccelerationKind::Vaapi
        ) && target_state.frame_data_location == crate::FrameDataLocation::Hardware
        {
            filter_chain
                .video
                .push(hardware_download_filter(target_state));
            target_state.frame_data_location = crate::FrameDataLocation::Software;
            target_state.pixel_format = request.desired.pixel_format();
        }

        filter_chain.video.push(subtitles_filter(
            &input.path,
            request.subtitle_fonts_dir.as_deref(),
        ));

        if use_cuda_overlay && has_later_cuda_overlay {
            filter_chain.video.push(hardware_upload_filter(
                HardwareAccelerationKind::Nvenc,
                QSV_EXTRA_HARDWARE_FRAMES,
            ));
            target_state.frame_data_location = crate::FrameDataLocation::Hardware;
        }

        if acceleration == HardwareAccelerationKind::Vaapi
            && target_state.frame_data_location == crate::FrameDataLocation::Software
        {
            filter_chain.video.push(hardware_upload_filter(
                HardwareAccelerationKind::Vaapi,
                QSV_EXTRA_HARDWARE_FRAMES,
            ));
            target_state.frame_data_location = crate::FrameDataLocation::Hardware;
            target_state.pixel_format = PixelFormat::Vaapi;
        }
    }
}

fn apply_qsv_download_and_reset_pts(
    request: &PipelineRequest,
    acceleration: HardwareAccelerationKind,
    target_state: &mut FrameState,
    filter_chain: &mut FilterChain,
) {
    if acceleration != HardwareAccelerationKind::Qsv
        || request.desired.video_format == FFmpegProfileVideoFormat::Copy
    {
        return;
    }

    if target_state.frame_data_location == crate::FrameDataLocation::Hardware {
        target_state.pixel_format = match request.desired.bit_depth {
            FFmpegProfileBitDepth::EightBit => PixelFormat::Nv12,
            FFmpegProfileBitDepth::TenBit => PixelFormat::Yuv420P10Le,
        };
        filter_chain
            .video
            .push(hardware_download_filter(target_state));
        target_state.frame_data_location = crate::FrameDataLocation::Software;
        target_state.pixel_format = request.desired.pixel_format();
    }

    let frame_rate = request
        .desired
        .frame_rate
        .or_else(|| {
            request
                .source_video_stream
                .as_ref()
                .and_then(|source| source.frame_rate)
        })
        .unwrap_or(FrameRate::DEFAULT);
    filter_chain.video.push(reset_pts_filter(frame_rate));
    target_state.frame_rate = Some(frame_rate);
}

const fn tonemap_algorithm(algorithm: FFmpegProfileTonemapAlgorithm) -> TonemapAlgorithm {
    match algorithm {
        FFmpegProfileTonemapAlgorithm::Linear => TonemapAlgorithm::Linear,
        FFmpegProfileTonemapAlgorithm::Clip => TonemapAlgorithm::Clip,
        FFmpegProfileTonemapAlgorithm::Gamma => TonemapAlgorithm::Gamma,
        FFmpegProfileTonemapAlgorithm::Reinhard => TonemapAlgorithm::Reinhard,
        FFmpegProfileTonemapAlgorithm::Mobius => TonemapAlgorithm::Mobius,
        FFmpegProfileTonemapAlgorithm::Hable => TonemapAlgorithm::Hable,
    }
}

const fn deinterlace_output_location(
    acceleration: HardwareAccelerationKind,
) -> crate::FrameDataLocation {
    match acceleration {
        HardwareAccelerationKind::Nvenc
        | HardwareAccelerationKind::Qsv
        | HardwareAccelerationKind::Vaapi => crate::FrameDataLocation::Hardware,
        HardwareAccelerationKind::None
        | HardwareAccelerationKind::Amf
        | HardwareAccelerationKind::VideoToolbox
        | HardwareAccelerationKind::V4l2m2m
        | HardwareAccelerationKind::Rkmpp => crate::FrameDataLocation::Software,
    }
}

const fn tonemap_output_location(
    acceleration: HardwareAccelerationKind,
) -> crate::FrameDataLocation {
    match acceleration {
        HardwareAccelerationKind::Nvenc
        | HardwareAccelerationKind::Qsv
        | HardwareAccelerationKind::Vaapi => crate::FrameDataLocation::Hardware,
        HardwareAccelerationKind::None
        | HardwareAccelerationKind::Amf
        | HardwareAccelerationKind::VideoToolbox
        | HardwareAccelerationKind::V4l2m2m
        | HardwareAccelerationKind::Rkmpp => crate::FrameDataLocation::Software,
    }
}

fn effective_video_acceleration(
    request: &PipelineRequest,
    acceleration: HardwareAccelerationKind,
) -> HardwareAccelerationKind {
    let Some(report) = &request.runtime_capabilities else {
        return acceleration;
    };

    if request.desired.video_format == FFmpegProfileVideoFormat::Copy {
        return acceleration;
    }

    let capabilities = HardwareCapabilitiesFactory::new(report).for_acceleration(acceleration);
    let can_encode = capabilities.can_encode(&request.desired) == FfmpegCapability::Hardware;
    let can_decode = request
        .source_video_stream
        .as_ref()
        .is_none_or(|source| capabilities.can_decode(source) == FfmpegCapability::Hardware);

    if matches!(
        acceleration,
        HardwareAccelerationKind::Nvenc
            | HardwareAccelerationKind::Qsv
            | HardwareAccelerationKind::VideoToolbox
    ) && can_encode
    {
        return capabilities.acceleration();
    }

    if can_encode && can_decode {
        capabilities.acceleration()
    } else {
        HardwareAccelerationKind::None
    }
}

fn effective_decode_acceleration(
    request: &PipelineRequest,
    requested_acceleration: HardwareAccelerationKind,
    effective_acceleration: HardwareAccelerationKind,
) -> HardwareAccelerationKind {
    let Some(source) = &request.source_video_stream else {
        return effective_acceleration;
    };

    if qsv_multiple_profile_decode_workaround(request, requested_acceleration, source) {
        return HardwareAccelerationKind::None;
    }

    let Some(report) = &request.runtime_capabilities else {
        return effective_acceleration;
    };

    let capabilities =
        HardwareCapabilitiesFactory::new(report).for_acceleration(requested_acceleration);
    if capabilities.can_decode(source) == FfmpegCapability::Hardware {
        capabilities.acceleration()
    } else {
        HardwareAccelerationKind::None
    }
}

fn qsv_multiple_profile_decode_workaround(
    request: &PipelineRequest,
    requested_acceleration: HardwareAccelerationKind,
    source: &SourceVideoStream,
) -> bool {
    requested_acceleration == HardwareAccelerationKind::Qsv
        && source.codec == SourceVideoCodec::H264
        && source.has_multiple_profiles
        && request
            .state
            .start_seconds
            .is_none_or(|seconds| seconds <= 0)
}

fn add_video_decoder(
    request: &PipelineRequest,
    steps: &mut Vec<PipelineStep>,
    acceleration: HardwareAccelerationKind,
) {
    if request.desired.video_format == FFmpegProfileVideoFormat::Copy {
        return;
    }

    let Some(source) = &request.source_video_stream else {
        return;
    };

    let decode_acceleration =
        if qsv_multiple_profile_decode_workaround(request, acceleration, source) {
            HardwareAccelerationKind::None
        } else if let Some(report) = &request.runtime_capabilities {
            let capabilities =
                HardwareCapabilitiesFactory::new(report).for_acceleration(acceleration);
            if capabilities.can_decode(source) == FfmpegCapability::Hardware {
                capabilities.acceleration()
            } else {
                HardwareAccelerationKind::None
            }
        } else {
            acceleration
        };

    if let Some(decoder) = select_video_decoder(decode_acceleration, source.codec) {
        let options = decoder.input_options();
        if !options.is_empty() {
            steps.push(PipelineStep::global("video_decoder", options));
        }
    }
}

fn container_movflags(state: &FfmpegState, desired: &DesiredVideoState) -> PipelineStep {
    if state.output_format == OutputFormat::Mp4
        && desired.video_format == FFmpegProfileVideoFormat::Copy
    {
        return PipelineStep::generic(
            "hls_direct_mp4",
            [
                "-movflags",
                "+faststart+frag_keyframe+separate_moof+omit_tfhd_offset+empty_moov+delay_moov",
            ],
        );
    }

    if matches!(
        state.output_format,
        OutputFormat::Hls | OutputFormat::HlsMp4
    ) && state
        .hls
        .as_ref()
        .is_some_and(|hls| hls.segment_template.contains("m4s"))
    {
        return PipelineStep::generic(
            "mp4",
            [
                "-movflags",
                "+empty_moov+omit_tfhd_offset+frag_keyframe+default_base_moof",
            ],
        );
    }

    fast_start()
}

fn add_audio_decoder_input_options(request: &mut PipelineRequest) {
    for input in request
        .inputs
        .iter_mut()
        .filter(|input| input.kind == InputKind::Audio)
    {
        let Some(stream) = request.source_audio_streams.iter().find(|stream| {
            input
                .streams
                .iter()
                .any(|input_stream| input_stream.index == stream.index)
        }) else {
            continue;
        };

        if request
            .state
            .start_seconds
            .is_some_and(|seconds| seconds > 0)
            && stream.codec == SourceAudioCodec::Dts
        {
            input.input_options.extend([
                "-c:a".to_owned(),
                "dts".to_owned(),
                "-core_only".to_owned(),
                "true".to_owned(),
            ]);
            continue;
        }

        if stream.codec == SourceAudioCodec::Ac3
            && stream.channels.is_some_and(|channels| channels >= 2)
            && request.desired.audio_channels == Some(2)
        {
            input.input_options.extend([
                "-c:a".to_owned(),
                "ac3".to_owned(),
                "-downmix".to_owned(),
                "stereo".to_owned(),
            ]);
        }
    }
}

fn add_realtime_input_options(request: &mut PipelineRequest) {
    if request.desired.realtime_mode != RealtimeMode::On {
        return;
    }

    let readrate = if request.desired.video_format == FFmpegProfileVideoFormat::Copy {
        "1.0"
    } else {
        "1.05"
    };
    let source_is_still_image = request
        .source_video_stream
        .as_ref()
        .is_some_and(|source| source.is_still_image);

    for input in &mut request.inputs {
        match input.kind {
            InputKind::Video if !source_is_still_image => input.add_readrate(readrate),
            InputKind::Audio => input.add_readrate(readrate),
            _ => {}
        }
    }
}

fn add_cuda_software_fallback_upload(
    request: &mut PipelineRequest,
    acceleration: HardwareAccelerationKind,
) {
    if acceleration != HardwareAccelerationKind::Nvenc
        || request.desired.video_format == FFmpegProfileVideoFormat::Copy
        || request.source_video_stream.is_none()
        || nvidia_vulkan_hdr_tonemap(request)
    {
        return;
    }

    if let Some(input) = request
        .inputs
        .iter_mut()
        .find(|input| input.kind == InputKind::Video)
    {
        input
            .filter_steps
            .push(cuda_software_fallback_upload_filter());
    }
}

fn nvidia_vulkan_hdr_tonemap(request: &PipelineRequest) -> bool {
    request
        .source_video_stream
        .as_ref()
        .is_some_and(|source| source.color_params.is_hdr)
        && request
            .runtime_capabilities
            .as_ref()
            .is_some_and(|report| report.has_hwaccel("vulkan"))
        && request.desired.vulkan_hdr_tonemap_mode == VulkanHdrTonemapMode::Auto
}

fn video_encoder(
    acceleration: HardwareAccelerationKind,
    desired: &DesiredVideoState,
    runtime_capabilities: Option<&FfmpegCapabilityReport>,
) -> PipelineStep {
    let name = match desired.video_format {
        FFmpegProfileVideoFormat::Copy => "copy",
        _ => crate::hardware_encoder_name(acceleration, desired.video_format)
            .unwrap_or_else(|| software_video_encoder_name(desired.video_format)),
    };

    let mut extra = Vec::new();
    if acceleration == HardwareAccelerationKind::Vaapi {
        extra.extend(vaapi_encoder_options(runtime_capabilities, desired));
    }

    if let Some(profile) = video_profile_option(acceleration, desired) {
        extra.extend(["-profile:v".to_owned(), profile]);
    }

    if acceleration == HardwareAccelerationKind::Qsv {
        extra.extend(["-low_power".to_owned(), "0".to_owned()]);
        extra.extend(["-look_ahead".to_owned(), "0".to_owned()]);
    }

    match (acceleration, desired.video_format) {
        (HardwareAccelerationKind::None, FFmpegProfileVideoFormat::Hevc) => {
            extra.extend(["-tag:v".to_owned(), "hvc1".to_owned()]);
            extra.extend(["-x265-params".to_owned(), "log-level=error".to_owned()]);
        }
        (HardwareAccelerationKind::Nvenc, FFmpegProfileVideoFormat::Hevc) => {
            extra.extend(["-tag:v".to_owned(), "hvc1".to_owned()]);
            extra.extend([
                "-b_ref_mode".to_owned(),
                nvidia_hevc_b_ref_mode(runtime_capabilities, desired).to_owned(),
            ]);
        }
        _ => {}
    }

    if acceleration != HardwareAccelerationKind::Vaapi {
        if let Some(preset) = &desired.video_preset {
            if !preset.trim().is_empty() {
                match acceleration {
                    HardwareAccelerationKind::VideoToolbox
                    | HardwareAccelerationKind::Amf
                    | HardwareAccelerationKind::V4l2m2m
                    | HardwareAccelerationKind::Rkmpp => {}
                    _ => extra.extend(["-preset:v".to_owned(), preset.clone()]),
                }
            }
        }
    }

    if acceleration == HardwareAccelerationKind::V4l2m2m
        && desired.video_format == FFmpegProfileVideoFormat::H264
    {
        extra.extend(["-num_capture_buffers".to_owned(), "16".to_owned()]);
    }

    PipelineStep::encoder(name, StreamKind::Video, extra)
}

fn vaapi_encoder_options(
    report: Option<&FfmpegCapabilityReport>,
    desired: &DesiredVideoState,
) -> Vec<String> {
    let Some(entrypoint) =
        report.and_then(|report| vaapi_encoder_profile_entry(&report.vaapi_profiles, desired))
    else {
        return Vec::new();
    };

    let mut options = Vec::new();
    if entrypoint.has_rate_control_mode("CQP")
        && !entrypoint.has_rate_control_mode("VBR")
        && !entrypoint.has_rate_control_mode("CBR")
    {
        options.extend(["-rc_mode".to_owned(), "1".to_owned()]);
    }

    if matches!(
        desired.video_format,
        FFmpegProfileVideoFormat::H264 | FFmpegProfileVideoFormat::Hevc
    ) && entrypoint.packed_header_misc
    {
        options.extend(["-sei".to_owned(), "-a53_cc".to_owned()]);
    }

    options
}

fn nvidia_hevc_b_ref_mode(
    report: Option<&FfmpegCapabilityReport>,
    desired: &DesiredVideoState,
) -> &'static str {
    if !desired.allow_b_frames {
        return "0";
    }

    let Some(details) = report.and_then(|report| {
        report
            .nvidia_encoders
            .iter()
            .find(|details| details.encoder_matches("hevc_nvenc"))
    }) else {
        return "0";
    };

    if details.supports_b_frames {
        "1"
    } else {
        "0"
    }
}

fn vaapi_encoder_profile_entry<'a>(
    profiles: &'a [VaapiProfileEntrypoint],
    desired: &DesiredVideoState,
) -> Option<&'a VaapiProfileEntrypoint> {
    let bit_depth = match desired.bit_depth {
        FFmpegProfileBitDepth::EightBit => 8,
        FFmpegProfileBitDepth::TenBit => 10,
    };

    profiles.iter().find(|entry| {
        entry.is_encode()
            && match desired.video_format {
                FFmpegProfileVideoFormat::H264 if bit_depth == 10 => false,
                FFmpegProfileVideoFormat::H264 => entry.profile_matches("VAProfileH264Main"),
                FFmpegProfileVideoFormat::Hevc if bit_depth == 10 => {
                    entry.profile_matches("VAProfileHEVCMain10")
                }
                FFmpegProfileVideoFormat::Hevc => entry.profile_matches("VAProfileHEVCMain"),
                FFmpegProfileVideoFormat::Av1 => entry.profile_matches("VAProfileAV1Profile0"),
                FFmpegProfileVideoFormat::Mpeg2Video => entry.profile_matches("VAProfileMPEG2Main"),
                FFmpegProfileVideoFormat::None | FFmpegProfileVideoFormat::Copy => false,
            }
    })
}

/// Port of `ErsatzTV`'s `PixelFormatOutputOption` for software encoders:
/// emit `-pix_fmt <name>` so the encoder receives frames in the desired format.
fn pixel_format_output_option(pixel_format: PixelFormat) -> PipelineStep {
    PipelineStep::generic(
        "pixel_format",
        ["-pix_fmt".to_owned(), pixel_format.ffmpeg_name().to_owned()],
    )
}

fn crop_metadata_workaround_filter(
    name: &'static str,
    video_format: FFmpegProfileVideoFormat,
    frame_size: FrameSize,
) -> Option<PipelineStep> {
    let (metadata_filter, crop_pixels) = match video_format {
        FFmpegProfileVideoFormat::H264 => {
            let crop_pixels = match frame_size.height {
                1080 => 8,
                720 => 16,
                _ => return None,
            };
            ("h264_metadata", crop_pixels)
        }
        FFmpegProfileVideoFormat::Hevc => {
            let crop_pixels = match frame_size.height {
                1080 => 8,
                _ => return None,
            };
            ("hevc_metadata", crop_pixels)
        }
        FFmpegProfileVideoFormat::Av1
        | FFmpegProfileVideoFormat::Mpeg2Video
        | FFmpegProfileVideoFormat::None
        | FFmpegProfileVideoFormat::Copy => return None,
    };

    Some(PipelineStep::generic(
        name,
        [
            "-bsf:v".to_owned(),
            format!("{metadata_filter}=crop_bottom={crop_pixels}"),
        ],
    ))
}

fn nvidia_green_line_workaround_filter(frame_size: FrameSize) -> Option<PipelineStep> {
    crop_metadata_workaround_filter(
        "nvidia_green_line_workaround",
        FFmpegProfileVideoFormat::H264,
        frame_size,
    )
}

fn amd_crop_metadata_workaround_filter(
    video_format: FFmpegProfileVideoFormat,
    frame_size: FrameSize,
) -> Option<PipelineStep> {
    if video_format != FFmpegProfileVideoFormat::Hevc {
        return None;
    }

    crop_metadata_workaround_filter("amd_crop_metadata_workaround", video_format, frame_size)
}

fn nvidia_green_line_workaround_applies(
    request: &PipelineRequest,
    acceleration: HardwareAccelerationKind,
    has_watermark: bool,
    has_subtitle_overlay: bool,
    has_graphics: bool,
) -> bool {
    if acceleration != HardwareAccelerationKind::Nvenc
        || request.desired.video_format != FFmpegProfileVideoFormat::H264
        || !(has_watermark || has_subtitle_overlay || has_graphics)
    {
        return false;
    }

    let source_matches_scaled = request
        .source_video_stream
        .as_ref()
        .and_then(|source| source.frame_size)
        == Some(request.desired.scaled_size);
    let subtitle_graphics_scaled = has_subtitle_overlay
        && has_graphics
        && request.desired.frame_size == request.desired.scaled_size;

    source_matches_scaled || subtitle_graphics_scaled
}

fn scene_detect_threshold(
    request: &PipelineRequest,
    acceleration: HardwareAccelerationKind,
) -> u32 {
    if acceleration == HardwareAccelerationKind::VideoToolbox
        || request.desired.video_format == FFmpegProfileVideoFormat::Mpeg2Video
        || request
            .source_video_stream
            .as_ref()
            .is_some_and(|source| source.codec == SourceVideoCodec::Mpeg2Video)
    {
        1_000_000_000
    } else {
        0
    }
}

fn should_emit_pixel_format_output_option(
    request: &PipelineRequest,
    acceleration: HardwareAccelerationKind,
    target_state: &FrameState,
) -> bool {
    if request.desired.video_format == FFmpegProfileVideoFormat::Copy {
        return false;
    }

    if matches!(
        acceleration,
        HardwareAccelerationKind::Amf
            | HardwareAccelerationKind::VideoToolbox
            | HardwareAccelerationKind::V4l2m2m
            | HardwareAccelerationKind::Rkmpp
    ) {
        return true;
    }

    request.source_video_stream.is_some()
        && target_state.pixel_format != request.desired.pixel_format()
}

const fn software_video_encoder_name(format: FFmpegProfileVideoFormat) -> &'static str {
    match format {
        FFmpegProfileVideoFormat::Hevc => "libx265",
        FFmpegProfileVideoFormat::Mpeg2Video => "mpeg2video",
        FFmpegProfileVideoFormat::Av1 => "libsvtav1",
        FFmpegProfileVideoFormat::H264
        | FFmpegProfileVideoFormat::None
        | FFmpegProfileVideoFormat::Copy => "libx264",
    }
}

fn video_profile_option(
    acceleration: HardwareAccelerationKind,
    desired: &DesiredVideoState,
) -> Option<String> {
    match acceleration {
        HardwareAccelerationKind::Qsv => match desired.video_format {
            FFmpegProfileVideoFormat::H264 => desired
                .video_profile
                .as_ref()
                .map(|profile| profile.to_lowercase()),
            FFmpegProfileVideoFormat::Hevc => None,
            _ => desired
                .video_profile
                .as_ref()
                .map(|profile| profile.to_lowercase()),
        },
        HardwareAccelerationKind::VideoToolbox | HardwareAccelerationKind::Rkmpp => {
            match desired.video_format {
                FFmpegProfileVideoFormat::H264 => desired
                    .video_profile
                    .as_ref()
                    .map(|profile| profile.to_lowercase()),
                FFmpegProfileVideoFormat::Hevc => Some(match desired.bit_depth {
                    FFmpegProfileBitDepth::EightBit => "main".to_owned(),
                    FFmpegProfileBitDepth::TenBit => "main10".to_owned(),
                }),
                _ => desired
                    .video_profile
                    .as_ref()
                    .map(|profile| profile.to_lowercase()),
            }
        }
        _ => desired
            .video_profile
            .as_ref()
            .map(|profile| profile.to_lowercase()),
    }
}

fn configure_hardware_acceleration(
    request: &mut PipelineRequest,
    steps: &mut Vec<PipelineStep>,
    acceleration: HardwareAccelerationKind,
    decode_acceleration: HardwareAccelerationKind,
) -> FrameState {
    let mut state = FrameState::new(
        request
            .source_video_stream
            .as_ref()
            .and_then(|stream| stream.frame_size)
            .unwrap_or(request.desired.frame_size),
    );
    if let Some(source) = &request.source_video_stream {
        state.is_anamorphic = source.is_anamorphic();
    }

    match acceleration {
        HardwareAccelerationKind::None
        | HardwareAccelerationKind::Amf
        | HardwareAccelerationKind::V4l2m2m => {}
        HardwareAccelerationKind::Qsv => {
            let decode_hardware = decode_acceleration == HardwareAccelerationKind::Qsv;
            add_qsv_hardware_options(steps, &request.desired, decode_hardware);

            if decode_hardware {
                state.frame_data_location = crate::FrameDataLocation::Hardware;
                state.pixel_format = request.desired.pixel_format();
            }
        }
        HardwareAccelerationKind::Nvenc => {
            if decode_acceleration != HardwareAccelerationKind::Nvenc {
                return state;
            }
            if nvidia_vulkan_hdr_tonemap(request) {
                steps.push(PipelineStep::global(
                    "cuda_hw_device",
                    ["-init_hw_device", "cuda=nv"],
                ));
                steps.push(PipelineStep::global(
                    "vulkan_hw_device",
                    ["-init_hw_device", "vulkan=vk@nv"],
                ));
                steps.push(PipelineStep::global(
                    "vulkan_hwaccel",
                    ["-hwaccel", "vulkan"],
                ));
                steps.push(PipelineStep::global(
                    "vulkan_hwaccel_output_format",
                    ["-hwaccel_output_format", "vulkan"],
                ));
            } else {
                steps.push(PipelineStep::global(
                    "cuda_hw_device",
                    ["-init_hw_device", "cuda"],
                ));
                steps.push(PipelineStep::global("cuda_hwaccel", ["-hwaccel", "cuda"]));
                steps.push(PipelineStep::global(
                    "cuda_hwaccel_output_format",
                    ["-hwaccel_output_format", "cuda"],
                ));
            }
            steps.push(no_autoscale());

            state.frame_data_location = crate::FrameDataLocation::Hardware;
            state.pixel_format = if request.desired.bit_depth == FFmpegProfileBitDepth::EightBit {
                PixelFormat::Nv12
            } else {
                PixelFormat::Cuda
            };
        }
        HardwareAccelerationKind::Vaapi => {
            add_vaapi_hardware_options(steps, &request.desired);
            steps.push(no_autoscale());

            state.frame_data_location = crate::FrameDataLocation::Hardware;
            state.pixel_format = if request.desired.bit_depth == FFmpegProfileBitDepth::EightBit {
                PixelFormat::Nv12
            } else {
                PixelFormat::Vaapi
            };
        }
        HardwareAccelerationKind::VideoToolbox => {
            if decode_acceleration != HardwareAccelerationKind::VideoToolbox {
                return state;
            }
            steps.push(PipelineStep::global(
                "videotoolbox_hwaccel",
                ["-hwaccel", "videotoolbox"],
            ));
        }
        HardwareAccelerationKind::Rkmpp => {
            steps.push(PipelineStep::global("rkmpp_hwaccel", ["-hwaccel", "rkmpp"]));
            state.frame_data_location = crate::FrameDataLocation::Hardware;
            state.pixel_format = if request.desired.bit_depth == FFmpegProfileBitDepth::EightBit {
                PixelFormat::Nv12
            } else {
                request.desired.pixel_format()
            };
        }
    }

    state
}

fn add_qsv_hardware_options(
    steps: &mut Vec<PipelineStep>,
    desired: &DesiredVideoState,
    decode_hardware: bool,
) {
    steps.push(PipelineStep::env(
        "cuda_visible_devices",
        crate::EnvironmentVariable::new("CUDA_VISIBLE_DEVICES", "999"),
    ));
    if decode_hardware {
        steps.push(PipelineStep::global("qsv_hwaccel", ["-hwaccel", "qsv"]));
        steps.push(PipelineStep::global(
            "qsv_hwaccel_output_format",
            ["-hwaccel_output_format", "qsv"],
        ));
    }
    if cfg!(target_os = "linux") {
        if let Some(device) = desired.qsv_device.as_deref() {
            steps.push(PipelineStep::global(
                "qsv_device",
                ["-qsv_device".to_owned(), device.to_owned()],
            ));
        }
    }
    steps.push(PipelineStep::global(
        "qsv_hw_device",
        ["-init_hw_device", "qsv=hw", "-filter_hw_device", "hw"],
    ));
}

fn add_vaapi_hardware_options(steps: &mut Vec<PipelineStep>, desired: &DesiredVideoState) {
    if let Some(device) = desired.vaapi_device.as_deref() {
        steps.push(PipelineStep::global(
            "vaapi_hw_device",
            ["-vaapi_device".to_owned(), device.to_owned()],
        ));
    } else {
        steps.push(PipelineStep::global(
            "vaapi_hw_device",
            ["-init_hw_device", "vaapi"],
        ));
    }
    if let Some(driver) = desired.vaapi_driver.as_deref() {
        steps.push(PipelineStep::env(
            "libva_driver_name",
            crate::EnvironmentVariable::new("LIBVA_DRIVER_NAME", driver),
        ));
    }
    steps.push(PipelineStep::global("vaapi_hwaccel", ["-hwaccel", "vaapi"]));
    steps.push(PipelineStep::global(
        "vaapi_hwaccel_output_format",
        ["-hwaccel_output_format", "vaapi"],
    ));
}

fn video_scale_filter(
    acceleration: HardwareAccelerationKind,
    current: &FrameState,
    scaled_size: FrameSize,
    padded_size: FrameSize,
    bit_depth: FFmpegProfileBitDepth,
    qsv_extra_hardware_frames: u32,
) -> String {
    match acceleration {
        HardwareAccelerationKind::Qsv => scale_qsv_filter(
            current,
            scaled_size,
            padded_size,
            bit_depth,
            qsv_extra_hardware_frames,
        ),
        HardwareAccelerationKind::Nvenc => {
            scale_cuda_filter(current, scaled_size, padded_size, bit_depth)
        }
        HardwareAccelerationKind::Vaapi => {
            scale_vaapi_filter(current, scaled_size, padded_size, bit_depth)
        }
        HardwareAccelerationKind::None
        | HardwareAccelerationKind::VideoToolbox
        | HardwareAccelerationKind::Amf
        | HardwareAccelerationKind::V4l2m2m
        | HardwareAccelerationKind::Rkmpp => crate::scale_filter(current, scaled_size, padded_size),
    }
}

fn scale_qsv_filter(
    current: &FrameState,
    scaled_size: FrameSize,
    padded_size: FrameSize,
    bit_depth: FFmpegProfileBitDepth,
    qsv_extra_hardware_frames: u32,
) -> String {
    let format = hardware_surface_format(bit_depth);
    let scale = if current.scaled_size == scaled_size {
        format!("vpp_qsv=format={format}")
    } else {
        let format_segment = format!(":format={format}");
        if current.is_anamorphic {
            format!(
                "vpp_qsv=w=iw*sar:h=ih{format_segment},setsar=1,vpp_qsv=w={}:h={}{format_segment},setsar=1",
                padded_size.width, padded_size.height
            )
        } else {
            let target = if current.cropped_size.is_some() {
                padded_size
            } else {
                scaled_size
            };
            format!(
                "vpp_qsv=w={}:h={}{format_segment},setsar=1",
                target.width, target.height
            )
        }
    };

    if current.frame_data_location == crate::FrameDataLocation::Hardware {
        return scale;
    }

    let initial_pixel_format = current.pixel_format.ffmpeg_name();
    format!(
        "format={initial_pixel_format},hwupload=extra_hw_frames={qsv_extra_hardware_frames},{scale}"
    )
}

fn scale_cuda_filter(
    current: &FrameState,
    scaled_size: FrameSize,
    padded_size: FrameSize,
    bit_depth: FFmpegProfileBitDepth,
) -> String {
    let format = hardware_surface_format(bit_depth);
    if current.scaled_size == scaled_size {
        return format!("scale_cuda=format={format}");
    }

    let aspect_ratio = if scaled_size == padded_size {
        ",setsar=1"
    } else if current.cropped_size.is_some() {
        ":force_original_aspect_ratio=increase,setsar=1"
    } else {
        ":force_original_aspect_ratio=decrease,setsar=1"
    };

    if current.is_anamorphic {
        format!(
            "scale_cuda=iw*sar:ih:format={format},setsar=1,scale_cuda={}:{}:format={format}{}",
            padded_size.width, padded_size.height, aspect_ratio
        )
    } else {
        format!(
            "scale_cuda={}:{}:format={format}{}",
            padded_size.width, padded_size.height, aspect_ratio
        )
    }
}

fn scale_vaapi_filter(
    current: &FrameState,
    scaled_size: FrameSize,
    padded_size: FrameSize,
    bit_depth: FFmpegProfileBitDepth,
) -> String {
    let format = hardware_surface_format(bit_depth);
    if current.scaled_size == scaled_size {
        return format!("scale_vaapi=format={format}");
    }

    let aspect_ratio = if scaled_size == padded_size {
        ",setsar=1"
    } else if current.cropped_size.is_some() {
        ":force_original_aspect_ratio=increase,setsar=1"
    } else {
        ":force_original_aspect_ratio=decrease,setsar=1"
    };

    if current.is_anamorphic {
        format!(
            "scale_vaapi=iw*sar:ih:format={format},setsar=1,scale_vaapi={}:{}:force_divisible_by=2:format={format}{}",
            padded_size.width, padded_size.height, aspect_ratio
        )
    } else {
        format!(
            "scale_vaapi={}:{}:force_divisible_by=2:format={format}{}",
            padded_size.width, padded_size.height, aspect_ratio
        )
    }
}

const fn hardware_surface_format(bit_depth: FFmpegProfileBitDepth) -> &'static str {
    match bit_depth {
        FFmpegProfileBitDepth::EightBit => "nv12",
        FFmpegProfileBitDepth::TenBit => "p010le",
    }
}

fn audio_encoder(
    desired: &DesiredVideoState,
    source_audio_stream: Option<&SourceAudioStream>,
) -> PipelineStep {
    let name = match desired.audio_format {
        FFmpegProfileAudioFormat::Copy => "copy",
        FFmpegProfileAudioFormat::Ac3 => "ac3",
        FFmpegProfileAudioFormat::Aac
        | FFmpegProfileAudioFormat::AacLatm
        | FFmpegProfileAudioFormat::None => "aac",
    };

    let mut extra = Vec::new();
    if desired.audio_format != FFmpegProfileAudioFormat::Copy {
        if let Some(channels) = desired.audio_channels.filter(|channels| {
            emit_audio_channels(desired.audio_format, source_audio_stream, *channels)
        }) {
            extra.extend(["-ac".to_owned(), channels.to_string()]);
        }
    }

    PipelineStep::encoder(name, StreamKind::Audio, extra)
}

fn selected_source_audio_stream<'a>(
    inputs: &[InputFile],
    source_audio_streams: &'a [SourceAudioStream],
) -> Option<&'a SourceAudioStream> {
    let stream_index = inputs
        .iter()
        .find(|input| input.kind == InputKind::Audio)?
        .streams
        .first()?
        .index;
    source_audio_streams
        .iter()
        .find(|stream| stream.index == stream_index)
}

fn emit_audio_channels(
    audio_format: FFmpegProfileAudioFormat,
    source_audio_stream: Option<&SourceAudioStream>,
    desired_channels: u32,
) -> bool {
    let Some(source_channels) = source_audio_stream.and_then(|stream| stream.channels) else {
        return false;
    };
    source_channels != desired_channels
        || matches!(
            audio_format,
            FFmpegProfileAudioFormat::Aac | FFmpegProfileAudioFormat::AacLatm
        ) && desired_channels > 2
}

fn audio_sample_rate_output(sample_rate_hz: u32) -> String {
    if sample_rate_hz.is_multiple_of(1_000) {
        format!("{}k", sample_rate_hz / 1_000)
    } else {
        sample_rate_hz.to_string()
    }
}

fn format_duration_seconds(duration: Duration) -> String {
    let seconds = duration.as_secs();
    let nanos = duration.subsec_nanos();
    if nanos == 0 {
        return seconds.to_string();
    }

    let mut fractional = format!("{nanos:09}");
    while fractional.ends_with('0') {
        fractional.pop();
    }
    format!("{seconds}.{fractional}")
}

fn subtitle_encoder(inputs: &[InputFile]) -> Option<PipelineStep> {
    let method = inputs
        .iter()
        .find(|input| {
            input.kind == InputKind::Subtitle
                && matches!(
                    input.subtitle_method,
                    Some(SubtitleMethod::Copy | SubtitleMethod::Convert)
                )
        })?
        .subtitle_method?;
    let encoder = match method {
        SubtitleMethod::Copy => "copy",
        SubtitleMethod::Convert => "dvdsub",
        SubtitleMethod::Burn => return None,
    };
    Some(PipelineStep::encoder(
        encoder,
        StreamKind::Subtitle,
        Vec::<String>::new(),
    ))
}

fn output_ts_offset_option(
    state: &FfmpegState,
    desired: &DesiredVideoState,
) -> Option<PipelineStep> {
    let offset = state
        .output_ts_offset
        .filter(|_| state.output_format != OutputFormat::HlsMp4)
        .filter(|_| desired.video_format != FFmpegProfileVideoFormat::Copy)?;

    Some(PipelineStep::generic(
        "output_ts_offset",
        [
            "-output_ts_offset".to_owned(),
            format!("{}s", format_duration_seconds(offset)),
        ],
    ))
}

fn audio_bitrate_option(bitrate: u32) -> PipelineStep {
    PipelineStep::generic(
        "audio_bitrate",
        [
            "-b:a".to_owned(),
            format!("{bitrate}k"),
            "-maxrate:a".to_owned(),
            format!("{bitrate}k"),
        ],
    )
}

fn video_bitrate_option(bitrate: u32) -> PipelineStep {
    PipelineStep::generic(
        "video_bitrate",
        [
            "-b:v".to_owned(),
            format!("{bitrate}k"),
            "-maxrate:v".to_owned(),
            format!("{bitrate}k"),
        ],
    )
}

fn frame_rate_output_option(frame_rate: FrameRate) -> PipelineStep {
    PipelineStep::generic(
        "frame_rate_output",
        [
            "-r".to_owned(),
            frame_rate.r_frame_rate(),
            "-vsync".to_owned(),
            "cfr".to_owned(),
        ],
    )
}

fn add_output_options(
    steps: &mut Vec<PipelineStep>,
    state: &FfmpegState,
    desired: &DesiredVideoState,
    acceleration: HardwareAccelerationKind,
    nvidia_green_line_workaround: bool,
) {
    if state.output_format != OutputFormat::Nut
        && desired.audio_format != FFmpegProfileAudioFormat::Copy
    {
        if let Some(bitrate) = desired.audio_bitrate {
            steps.push(audio_bitrate_option(bitrate));
        }
        if let Some(buffer_size) = desired.audio_buffer_size {
            steps.push(PipelineStep::generic(
                "audio_buffer_size",
                ["-bufsize:a".to_owned(), format!("{buffer_size}k")],
            ));
        }
        if let Some(sample_rate) = desired.audio_sample_rate {
            steps.push(PipelineStep::generic(
                "audio_sample_rate",
                ["-ar".to_owned(), audio_sample_rate_output(sample_rate)],
            ));
        }
    }

    if desired.video_format != FFmpegProfileVideoFormat::Copy {
        if acceleration != HardwareAccelerationKind::Qsv {
            if let Some(frame_rate) = desired.frame_rate {
                steps.push(frame_rate_output_option(frame_rate));
            }
        }
        if let Some(timescale) = desired.video_track_timescale {
            steps.push(PipelineStep::generic(
                "video_track_timescale",
                ["-video_track_timescale".to_owned(), timescale.to_string()],
            ));
        }
        if let Some(bitrate) = desired.video_bitrate {
            steps.push(video_bitrate_option(bitrate));
        }
        if let Some(buffer_size) = desired.video_buffer_size {
            steps.push(PipelineStep::generic(
                "video_buffer_size",
                ["-bufsize:v".to_owned(), format!("{buffer_size}k")],
            ));
        }
    }

    if let Some(step) = output_ts_offset_option(state, desired) {
        steps.push(step);
    }

    if nvidia_green_line_workaround {
        if let Some(step) =
            nvidia_green_line_workaround_filter(desired.cropped_size.unwrap_or(desired.frame_size))
        {
            steps.push(step);
        }
    }

    if acceleration == HardwareAccelerationKind::Vaapi
        && desired.vaapi_driver.as_deref() == Some("radeonsi")
    {
        if let Some(step) = amd_crop_metadata_workaround_filter(
            desired.video_format,
            desired.cropped_size.unwrap_or(desired.frame_size),
        ) {
            steps.push(step);
        }
    }

    if state.do_not_map_metadata {
        steps.push(PipelineStep::generic(
            "no_metadata",
            ["-map_metadata", "-1"],
        ));
    }

    add_metadata_output_options(steps, &state.metadata);

    if let Some(duration) = state.duration {
        steps.push(time_limit(duration));
    }

    match state.output_format {
        OutputFormat::MpegTs => {
            steps.push(mpegts_output(true));
            steps.push(pipe_output());
        }
        OutputFormat::Mp4 => {
            steps.push(PipelineStep::generic("mp4", ["-f", "mp4"]));
            steps.push(pipe_output());
        }
        OutputFormat::Mkv => {
            steps.push(PipelineStep::generic("matroska", ["-f", "matroska"]));
            steps.push(pipe_output());
        }
        OutputFormat::Nut => {
            steps.push(PipelineStep::generic("nut", ["-f", "nut"]));
            steps.push(pipe_output());
        }
        OutputFormat::Hls | OutputFormat::HlsMp4 => {
            if let Some(hls) = &state.hls {
                steps.push(hls_output(state.output_format, hls, desired, acceleration));
            }
        }
    }
}

fn add_metadata_output_options(steps: &mut Vec<PipelineStep>, metadata: &OutputMetadata) {
    if let Some(service_provider) = metadata.service_provider.as_deref() {
        steps.push(PipelineStep::generic(
            "metadata_service_provider",
            [
                "-metadata".to_owned(),
                format!("service_provider=\"{service_provider}\""),
            ],
        ));
    }

    if let Some(service_name) = metadata.service_name.as_deref() {
        steps.push(PipelineStep::generic(
            "metadata_service_name",
            [
                "-metadata".to_owned(),
                format!("service_name=\"{service_name}\""),
            ],
        ));
    }

    if let Some(audio_language) = metadata.audio_language.as_deref() {
        steps.push(PipelineStep::generic(
            "metadata_audio_language",
            [
                "-metadata:s:a:0".to_owned(),
                format!("language={audio_language}"),
            ],
        ));
    }

    if let Some(subtitle_language) = metadata.subtitle_language.as_deref() {
        steps.push(PipelineStep::generic(
            "metadata_subtitle_language",
            [
                "-metadata:s:s:0".to_owned(),
                format!("language={subtitle_language}"),
            ],
        ));
    }

    if let Some(subtitle_title) = metadata.subtitle_title.as_deref() {
        steps.push(PipelineStep::generic(
            "metadata_subtitle_title",
            [
                "-metadata:s:s:0".to_owned(),
                format!("title={subtitle_title}"),
            ],
        ));
    }
}

fn hls_output(
    format: OutputFormat,
    hls: &HlsOutput,
    desired: &DesiredVideoState,
    acceleration: HardwareAccelerationKind,
) -> PipelineStep {
    let frame_rate = desired.frame_rate.unwrap_or(FrameRate::DEFAULT);
    let one_second_gop = acceleration == HardwareAccelerationKind::Qsv;
    let keyframe_interval_seconds = 2.0;
    let gop = if one_second_gop {
        frame_rate.parsed().round()
    } else {
        (frame_rate.parsed() * keyframe_interval_seconds).round()
    };

    let mut options = Vec::new();
    if desired.video_format != FFmpegProfileVideoFormat::Copy {
        options.extend([
            "-g".to_owned(),
            gop.to_string(),
            "-keyint_min".to_owned(),
            (frame_rate.parsed() * keyframe_interval_seconds)
                .round()
                .to_string(),
            "-force_key_frames".to_owned(),
            "expr:gte(t,n_forced*2)".to_owned(),
        ]);
    }

    options.extend([
        "-f".to_owned(),
        "hls".to_owned(),
        "-hls_time".to_owned(),
        "4".to_owned(),
        "-hls_list_size".to_owned(),
        "0".to_owned(),
        "-segment_list_flags".to_owned(),
        "+live".to_owned(),
        "-hls_segment_filename".to_owned(),
        hls.segment_template.clone(),
    ]);

    match format {
        OutputFormat::Hls => options.extend(["-hls_segment_type".to_owned(), "mpegts".to_owned()]),
        OutputFormat::HlsMp4 => options.extend([
            "-hls_segment_type".to_owned(),
            "fmp4".to_owned(),
            "-hls_fmp4_init_filename".to_owned(),
            hls.init_template
                .clone()
                .unwrap_or_else(|| "init.mp4".to_owned()),
        ]),
        OutputFormat::MpegTs | OutputFormat::Mp4 | OutputFormat::Mkv | OutputFormat::Nut => {}
    }

    if let Some(segment_options) = &hls.segment_options {
        options.extend(["-hls_segment_options".to_owned(), segment_options.clone()]);
    }

    let pdt = if hls.troubleshooting {
        ""
    } else {
        "program_date_time+omit_endlist+"
    };
    let discont = if hls.is_first_transcode {
        ""
    } else {
        "+discont_start"
    };
    options.extend([
        "-hls_flags".to_owned(),
        format!("{pdt}append_list{discont}+independent_segments"),
        hls.playlist_path.clone(),
    ]);

    PipelineStep::generic("hls", options)
}
