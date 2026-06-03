use ersatzrs_domain::{
    FFmpegProfile, FFmpegProfileAudioFormat, FFmpegProfileBitDepth, FFmpegProfileTonemapAlgorithm,
    FFmpegProfileVideoFormat, FilterMode, HardwareAccelerationKind, NormalizeLoudnessMode,
    Resolution, ScalingBehavior, StreamingMode, VideoScanKind,
};

use crate::{
    closed_gop, crop_filter, deinterlace_filter, fast_start, frame_rate_filter,
    hardware_upload_filter, hide_banner, loglevel_error, mpegts_output, no_autoscale, no_b_frames,
    no_standard_input, no_stats, normalize_loudness_filter, pad_filter, pipe_output,
    progress_to_stdout, scale_subtitle_image_filter, select_video_decoder, standard_format_flags,
    subtitle_overlay_filter, subtitles_filter, tonemap_filter_for,
    watermark_opacity_expression_filter, watermark_opacity_filter, watermark_overlay_filter,
    watermark_scale_filter, AudioFilter, CommandGenerator, ComplexFilter, FfmpegCapability,
    FfmpegCapabilityReport, FfmpegCommand, FilterChain, FrameRate, FrameSize, FrameState,
    GraphicsOverlayFilter, HardwareCapabilitiesFactory, InputFile, InputKind, PipelineStep,
    PixelFormat, SourceVideoCodec, StreamKind, SubtitleMethod, TonemapAlgorithm, WatermarkOverlay,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FfmpegState {
    pub output_format: OutputFormat,
    pub start_seconds: Option<i64>,
    pub thread_count: Option<u32>,
    pub report_progress: bool,
    pub do_not_map_metadata: bool,
    pub hls: Option<HlsOutput>,
}

impl Default for FfmpegState {
    fn default() -> Self {
        Self {
            output_format: OutputFormat::MpegTs,
            start_seconds: None,
            thread_count: None,
            report_progress: true,
            do_not_map_metadata: true,
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
    pub codec: SourceAudioCodec,
    pub channels: Option<u32>,
    pub sample_rate: Option<u32>,
}

impl SourceAudioStream {
    #[must_use]
    pub const fn new(codec: SourceAudioCodec) -> Self {
        Self {
            codec,
            channels: None,
            sample_rate: None,
        }
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

/// Source color metadata that affects later filter and decoder choices.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SourceColorParams {
    pub is_hdr: bool,
    pub is_bt2020_ten: bool,
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
    pub audio_bitrate: Option<u32>,
    pub audio_buffer_size: Option<u32>,
    pub audio_channels: Option<u32>,
    pub audio_sample_rate: Option<u32>,
    pub normalize_loudness_mode: NormalizeLoudnessMode,
    pub target_loudness: Option<f64>,
    pub normalize_colors: bool,
    pub allow_b_frames: bool,
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
            audio_bitrate: Some(192),
            audio_buffer_size: Some(384),
            audio_channels: Some(2),
            audio_sample_rate: Some(48_000),
            normalize_loudness_mode: NormalizeLoudnessMode::Off,
            target_loudness: None,
            normalize_colors: false,
            allow_b_frames: false,
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
            return Self::copy_from_profile(profile, resolution);
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
            audio_bitrate: positive_u32(profile.audio_bitrate),
            audio_buffer_size: positive_u32(profile.audio_buffer_size),
            audio_channels: positive_u32(profile.audio_channels),
            audio_sample_rate,
            normalize_loudness_mode,
            target_loudness: (normalize_loudness_mode == NormalizeLoudnessMode::LoudNorm)
                .then_some(profile.target_loudness)
                .flatten(),
            normalize_colors: profile.normalize_colors,
            allow_b_frames: profile.allow_b_frames,
        }
    }

    fn copy_from_profile(profile: &FFmpegProfile, resolution: &Resolution) -> Self {
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
            audio_bitrate: None,
            audio_buffer_size: None,
            audio_channels: None,
            audio_sample_rate: None,
            normalize_loudness_mode: NormalizeLoudnessMode::Off,
            target_loudness: None,
            normalize_colors: profile.normalize_colors,
            allow_b_frames: true,
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
    let mut steps = vec![
        no_standard_input(),
        hide_banner(),
        no_stats(),
        loglevel_error(),
        standard_format_flags(),
        fast_start(),
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

    if request.desired.video_format != FFmpegProfileVideoFormat::Copy {
        steps.push(closed_gop());
        if !request.desired.allow_b_frames {
            steps.push(no_b_frames());
        }
    }

    let mut filter_chain = FilterChain::default();
    let mut target_state =
        configure_hardware_acceleration(&mut request, &mut steps, effective_acceleration);
    add_video_decoder(&request, &mut steps, effective_acceleration);
    apply_source_video_filters(
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
        filter_chain
            .watermark_overlay
            .push(watermark_overlay_filter(
                watermark,
                request.desired.frame_size,
                request
                    .source_video_stream
                    .as_ref()
                    .and_then(|stream| stream.square_pixel_frame_size(request.desired.frame_size))
                    .unwrap_or(request.desired.frame_size),
                request.desired.pixel_format(),
            ));
    }

    let has_image_subtitle_burn = request.inputs.iter().any(|input| {
        input.kind == InputKind::Subtitle
            && input.is_image_based_subtitle
            && input.subtitle_method == Some(SubtitleMethod::Burn)
    });
    if has_image_subtitle_burn {
        filter_chain
            .subtitle
            .push(scale_subtitle_image_filter(request.desired.frame_size));
        filter_chain
            .subtitle_overlay
            .push(subtitle_overlay_filter(request.desired.pixel_format()));
    }

    for input in request.inputs.iter().filter(|input| {
        input.kind == InputKind::Subtitle
            && !input.is_image_based_subtitle
            && input.subtitle_method == Some(SubtitleMethod::Burn)
    }) {
        filter_chain.video.push(subtitles_filter(
            &input.path,
            request.subtitle_fonts_dir.as_deref(),
        ));
    }

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
                request.inputs.push(input);
                filter_chain
                    .graphics_overlay
                    .push(GraphicsOverlayFilter::NativeImage {
                        input_index: graphics_image_index,
                        filter: watermark_overlay_filter(
                            &overlay,
                            request.desired.frame_size,
                            request
                                .source_video_stream
                                .as_ref()
                                .and_then(|stream| {
                                    stream.square_pixel_frame_size(request.desired.frame_size)
                                })
                                .unwrap_or(request.desired.frame_size),
                            request.desired.pixel_format(),
                        ),
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

    steps.push(video_encoder(effective_acceleration, &request.desired));

    // Port of `SoftwarePipelineBuilder.SetPixelFormat`: when the source frame
    // is in software memory and its pixel format differs from the desired
    // output format, emit a `-pix_fmt` output option. This is what converts a
    // 10-bit (`yuv420p10le`) source down to 8-bit `yuv420p` so the libx264
    // `high` profile (8-bit only) can open the encoder. Gated on a known source
    // stream so pipelines built without probe metadata are unchanged.
    if request.desired.video_format != FFmpegProfileVideoFormat::Copy
        && request.source_video_stream.is_some()
        && target_state.pixel_format != request.desired.pixel_format()
    {
        steps.push(pixel_format_output_option(request.desired.pixel_format()));
    }

    steps.push(PipelineStep::complex_filter(complex.options));
    steps.push(audio_encoder(&request.desired));
    if let Some(encoder) = subtitle_encoder(&request.inputs) {
        steps.push(encoder);
    }

    add_output_options(
        &mut steps,
        &request.state,
        &request.desired,
        effective_acceleration,
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
            QSV_EXTRA_HARDWARE_FRAMES,
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
    );
    if !scale.trim().is_empty() {
        filter_chain.video.push(scale);
        target_state.scaled_size = request.desired.scaled_size;
        target_state.padded_size = request.desired.scaled_size;
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

    if let Some(desired_rate) = request.desired.frame_rate {
        let needs_fps_filter = request
            .source_video_stream
            .as_ref()
            .and_then(|source| source.frame_rate)
            .is_some_and(|source_rate| source_rate != desired_rate);
        if needs_fps_filter && is_transcode {
            filter_chain
                .video
                .push(frame_rate_filter(target_state, desired_rate));
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

    if can_encode && can_decode {
        capabilities.acceleration()
    } else {
        HardwareAccelerationKind::None
    }
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

    let decode_acceleration = if let Some(report) = &request.runtime_capabilities {
        let capabilities = HardwareCapabilitiesFactory::new(report).for_acceleration(acceleration);
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

fn video_encoder(
    acceleration: HardwareAccelerationKind,
    desired: &DesiredVideoState,
) -> PipelineStep {
    let name = match desired.video_format {
        FFmpegProfileVideoFormat::Copy => "copy",
        _ => crate::hardware_encoder_name(acceleration, desired.video_format)
            .unwrap_or_else(|| software_video_encoder_name(desired.video_format)),
    };

    let mut extra = Vec::new();
    if let Some(profile) = video_profile_option(acceleration, desired) {
        extra.extend(["-profile:v".to_owned(), profile]);
    }

    if acceleration == HardwareAccelerationKind::Qsv {
        extra.extend(["-low_power".to_owned(), "0".to_owned()]);
        extra.extend(["-look_ahead".to_owned(), "0".to_owned()]);
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

/// Port of `ErsatzTV`'s `PixelFormatOutputOption` for software encoders:
/// emit `-pix_fmt <name>` so the encoder receives frames in the desired format.
fn pixel_format_output_option(pixel_format: PixelFormat) -> PipelineStep {
    PipelineStep::generic(
        "pixel_format",
        ["-pix_fmt".to_owned(), pixel_format.ffmpeg_name().to_owned()],
    )
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
            steps.push(PipelineStep::global("qsv_hwaccel", ["-hwaccel", "qsv"]));
            steps.push(PipelineStep::global(
                "qsv_hwaccel_output_format",
                ["-hwaccel_output_format", "qsv"],
            ));
            steps.push(PipelineStep::global(
                "qsv_hw_device",
                ["-init_hw_device", "qsv=hw", "-filter_hw_device", "hw"],
            ));

            state.frame_data_location = crate::FrameDataLocation::Hardware;
            state.pixel_format = request.desired.pixel_format();
        }
        HardwareAccelerationKind::Nvenc => {
            steps.push(PipelineStep::global(
                "cuda_hw_device",
                ["-init_hw_device", "cuda"],
            ));
            steps.push(PipelineStep::global("cuda_hwaccel", ["-hwaccel", "cuda"]));
            steps.push(PipelineStep::global(
                "cuda_hwaccel_output_format",
                ["-hwaccel_output_format", "cuda"],
            ));
            steps.push(no_autoscale());

            state.frame_data_location = crate::FrameDataLocation::Hardware;
            state.pixel_format = if request.desired.bit_depth == FFmpegProfileBitDepth::EightBit {
                PixelFormat::Nv12
            } else {
                PixelFormat::Cuda
            };
        }
        HardwareAccelerationKind::Vaapi => {
            steps.push(PipelineStep::global(
                "vaapi_hw_device",
                ["-init_hw_device", "vaapi"],
            ));
            steps.push(PipelineStep::global("vaapi_hwaccel", ["-hwaccel", "vaapi"]));
            steps.push(PipelineStep::global(
                "vaapi_hwaccel_output_format",
                ["-hwaccel_output_format", "vaapi"],
            ));
            steps.push(no_autoscale());

            state.frame_data_location = crate::FrameDataLocation::Hardware;
            state.pixel_format = if request.desired.bit_depth == FFmpegProfileBitDepth::EightBit {
                PixelFormat::Nv12
            } else {
                PixelFormat::Vaapi
            };
        }
        HardwareAccelerationKind::VideoToolbox => {
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

fn video_scale_filter(
    acceleration: HardwareAccelerationKind,
    current: &FrameState,
    scaled_size: FrameSize,
    padded_size: FrameSize,
    bit_depth: FFmpegProfileBitDepth,
) -> String {
    match acceleration {
        HardwareAccelerationKind::Qsv => {
            scale_qsv_filter(current, scaled_size, padded_size, bit_depth)
        }
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
) -> String {
    let format = hardware_surface_format(bit_depth);
    if current.scaled_size == scaled_size {
        return format!("vpp_qsv=format={format}");
    }

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

fn audio_encoder(desired: &DesiredVideoState) -> PipelineStep {
    let name = match desired.audio_format {
        FFmpegProfileAudioFormat::Copy => "copy",
        FFmpegProfileAudioFormat::Ac3 => "ac3",
        FFmpegProfileAudioFormat::Aac
        | FFmpegProfileAudioFormat::AacLatm
        | FFmpegProfileAudioFormat::None => "aac",
    };

    let mut extra = Vec::new();
    if desired.audio_format != FFmpegProfileAudioFormat::Copy {
        if let Some(channels) = desired.audio_channels {
            extra.extend(["-ac".to_owned(), channels.to_string()]);
        }
        if let Some(sample_rate) = desired.audio_sample_rate {
            extra.extend(["-ar".to_owned(), sample_rate.to_string()]);
        }
        if let Some(bitrate) = desired.audio_bitrate {
            extra.extend(["-b:a".to_owned(), format!("{bitrate}k")]);
        }
    }

    PipelineStep::encoder(name, StreamKind::Audio, extra)
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

fn add_output_options(
    steps: &mut Vec<PipelineStep>,
    state: &FfmpegState,
    desired: &DesiredVideoState,
    acceleration: HardwareAccelerationKind,
) {
    if desired.video_format != FFmpegProfileVideoFormat::Copy {
        if let Some(bitrate) = desired.video_bitrate {
            steps.push(PipelineStep::generic(
                "video_bitrate",
                ["-b:v".to_owned(), format!("{bitrate}k")],
            ));
        }
        if let Some(buffer_size) = desired.video_buffer_size {
            steps.push(PipelineStep::generic(
                "video_buffer_size",
                ["-bufsize:v".to_owned(), format!("{buffer_size}k")],
            ));
        }
    }

    if state.do_not_map_metadata {
        steps.push(PipelineStep::generic(
            "no_metadata",
            ["-map_metadata", "-1"],
        ));
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
