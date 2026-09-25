use ersatzrs_domain::{
    FFmpegProfile, FFmpegProfileAudioFormat, FFmpegProfileBitDepth, FFmpegProfileTonemapAlgorithm,
    FFmpegProfileVideoFormat, FilterMode, HardwareAccelerationKind, NormalizeLoudnessMode,
    OutputDynamicRange, Resolution, ScalingBehavior, StreamingMode, VideoScanKind,
};
use std::time::Duration;

use crate::{
    audio_pad_filter, audio_resample_filter, closed_gop, colorspace_filter, crop_filter,
    cuda_format_filter, cuda_software_fallback_upload_filter, deinterlace_filter,
    dolby_vision_profile_5_hdr10_filter, dolby_vision_profile_5_sdr_filter, fast_start,
    format_time_limit, graphics_overlay_vaapi_filter, hardware_download_filter,
    hardware_surface_download_format, hardware_upload_filter, hardware_upload_vaapi_filter,
    hdr10_output_support, hide_banner, hlg_to_hdr10_cuda_filter, hlg_to_hdr10_filter,
    loglevel_error, mpegts_muxrate, mpegts_output, mpegts_pcr_period, no_autoscale, no_b_frames,
    no_demux_decode_delay, no_scene_detect, no_standard_input, no_stats, normalize_loudness_filter,
    pad_filter, pad_vaapi_filter, pipe_output, preserve_hdr10_filter, progress_to_stdout,
    realtime_filter, reset_pts_filter, scale_subtitle_image_filter, sdr_to_hdr10_cuda_filter,
    sdr_to_hdr10_filter, select_forced_software_video_decoder, select_software_video_decoder,
    select_video_decoder, standard_format_flags, still_image_loop_filter,
    strip_dolby_vision_metadata_filter, subtitle_overlay_cuda_filter, subtitle_overlay_filter,
    subtitle_overlay_vaapi_filter, subtitle_scale_npp_filter, subtitles_filter, time_limit,
    tonemap_filter_for, unknown_source_scale_filter, vaapi_subtitle_pixel_format_filter,
    watermark_alpha_format_filter, watermark_opacity_expression_filter_with_clock,
    watermark_opacity_filter, watermark_overlay_cuda_filter, watermark_overlay_filter,
    watermark_scale_filter, yadif_cuda_filter, AudioFilter, ColorParams, CommandGenerator,
    ComplexFilter, ContentLightLevel, DolbyVisionConfiguration, DolbyVisionHandling,
    FfmpegCapability, FfmpegCapabilityReport, FfmpegCommand, FfmpegRuntimeReport, FilterChain,
    FrameRate, FrameSize, FrameState, GraphicsOverlayFilter, HardwareCapabilitiesFactory,
    HardwareGraphOperation, Hdr10OutputSupport, IncompleteDolbyVisionMetadata, InputFile,
    InputKind, MasteringDisplayMetadata, MpegTsMuxingOptions, PipelineStep, PixelFormat,
    SourceColorTags, SourceTransferKind, SourceVideoCodec, StreamKind, SubtitleMethod,
    TonemapAlgorithm, VaapiProfileEntrypoint, WatermarkOpacityClock, WatermarkOverlay,
    NVENC_HDR_DEVICE_PREFIX,
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
    pub discontinuity: bool,
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
    /// Preserve the already-edited common clock of the provider's NUT stream.
    pub retained_stream_timestamps: bool,
    pub output_format: OutputFormat,
    pub start: Option<Duration>,
    pub output_start: Option<Duration>,
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
            retained_stream_timestamps: false,
            output_format: OutputFormat::MpegTs,
            start: None,
            output_start: None,
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
    /// Executable path from the same immutable runtime snapshot as
    /// `runtime_capabilities`.
    pub ffmpeg_executable: Option<String>,
    /// Publication generation for runtime-learned retry/denial feedback.
    pub runtime_generation: Option<u64>,
    pub watermark: Option<WatermarkOverlay>,
    pub watermarks: Vec<WatermarkOverlay>,
    pub watermark_opacity_clock: WatermarkOpacityClock,
    pub drawvg_filters: Vec<String>,
    pub graphics_overlays: Vec<GraphicsPipelineOverlay>,
    pub subtitle_fonts_dir: Option<String>,
}

impl PipelineRequest {
    /// Use the caller's already-edited, zero-based NUT rawvideo/PCM input.
    ///
    /// The caller configures pipe:0 and fresh stream indexes before opting in.
    /// Preserve those inputs and all auxiliary inputs, including disabled audio.
    /// The provider has consumed the source seek and owns exact interval cuts.
    #[must_use]
    pub fn with_retained_stream_input(mut self) -> Self {
        self.state.retained_stream_timestamps = true;
        self.state.start = None;
        self.state.output_start = None;
        if self.desired.video_format == FFmpegProfileVideoFormat::Copy {
            self.desired.video_format = FFmpegProfileVideoFormat::H264;
        }
        if self.desired.audio_format == FFmpegProfileAudioFormat::Copy {
            self.desired.audio_format = FFmpegProfileAudioFormat::Aac;
        }
        self
    }

    #[must_use]
    pub fn new(input: InputFile, desired: DesiredVideoState) -> Self {
        let path = input.path.clone();
        let mut audio_input = InputFile::audio(path);
        audio_input.allow_readrate = input.allow_readrate;
        audio_input.seek_after_input = input.seek_after_input;
        Self {
            inputs: vec![input, audio_input],
            state: FfmpegState::default(),
            desired,
            source_video_stream: None,
            source_audio_streams: Vec::new(),
            runtime_capabilities: None,
            ffmpeg_executable: None,
            runtime_generation: None,
            watermark: None,
            watermarks: Vec::new(),
            watermark_opacity_clock: WatermarkOpacityClock::default(),
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

    fn source_metadata_is_runtime_only(&self) -> bool {
        self.inputs
            .iter()
            .find(|input| input.kind == InputKind::Video)
            .is_some_and(|input| input.path == "pipe:0")
    }

    /// Disable audio mapping for an explicitly silent playback request.
    ///
    /// The source video input may still contain audio streams, so removing the
    /// logical audio input is required to keep `FFmpeg` from selecting one.
    #[must_use]
    pub fn without_audio(mut self) -> Self {
        self.inputs.retain(|input| input.kind != InputKind::Audio);
        self.source_audio_streams.clear();
        self
    }

    #[must_use]
    pub fn with_runtime_capabilities(mut self, report: FfmpegCapabilityReport) -> Self {
        if let Some(identity) = &report.runtime_identity {
            self.ffmpeg_executable = Some(identity.ffmpeg_path.clone());
            self.runtime_generation = Some(identity.generation);
        }
        self.runtime_capabilities = Some(report);
        self
    }

    /// Bind command generation to one coherent runtime publication.
    #[must_use]
    pub fn with_runtime_report(mut self, report: &FfmpegRuntimeReport) -> Self {
        self.runtime_capabilities = Some(report.capabilities.clone());
        self.ffmpeg_executable = Some(report.ffmpeg.path.clone());
        self.runtime_generation = Some(report.generation);
        self
    }

    #[must_use]
    pub const fn with_watermark_opacity_clock(mut self, clock: WatermarkOpacityClock) -> Self {
        self.watermark_opacity_clock = clock;
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
    Drawtext {
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
    pub fn drawtext(z_index: i32, filter: impl Into<String>) -> Self {
        Self::Drawtext {
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
            Self::Drawvg { z_index, .. }
            | Self::Drawtext { z_index, .. }
            | Self::NativeImage { z_index, .. } => *z_index,
        }
    }
}

/// Source video stream metadata used for ErsatzTV-style decoder dispatch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceVideoStream {
    pub index: usize,
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
    /// Display-matrix rotation in degrees (ADR 0003, milestone `p8c-02`).
    ///
    /// `FFmpeg` 9 autorotates from this side data by default, so ErsatzRS must
    /// **not** add a transpose of its own — that would rotate twice. What it
    /// must do is agree with the rotation when computing geometry, because
    /// `frame_size` holds the *stored* dimensions while the decoded frames are
    /// already the *displayed* ones.
    pub rotation_degrees: Option<i32>,
    /// Source SMPTE ST 2086 mastering display in `FFmpeg`'s
    /// `-mastering_display` syntax (ADR 0064), or `None`.
    pub mastering_display: Option<String>,
    /// Source CTA-861.3 `MaxCLL` in cd/m².
    pub max_cll: Option<i32>,
    /// Source CTA-861.3 `MaxFALL` in cd/m².
    pub max_fall: Option<i32>,
    /// Dolby Vision configuration record (ADR 0064), when the source has one.
    pub dolby_vision: Option<DolbyVisionConfiguration>,
    /// Positive provider detection that lacked the complete configuration
    /// record required by the strict playback policy.
    pub incomplete_dolby_vision: Option<IncompleteDolbyVisionMetadata>,
}

impl SourceVideoStream {
    #[must_use]
    pub const fn new(codec: SourceVideoCodec) -> Self {
        Self {
            index: 0,
            codec,
            profile: None,
            pixel_format: None,
            color_params: SourceColorParams {
                color_range: None,
                color_space: None,
                color_transfer: None,
                color_primaries: None,
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
            rotation_degrees: None,
            mastering_display: None,
            max_cll: None,
            max_fall: None,
            dolby_vision: None,
            incomplete_dolby_vision: None,
        }
    }

    /// True when the display matrix turns the picture on its side, so stored
    /// and displayed dimensions are transposed.
    #[must_use]
    pub fn is_quarter_turned(&self) -> bool {
        self.rotation_degrees
            .map(|degrees| degrees.rem_euclid(180))
            .is_some_and(|degrees| degrees == 90)
    }

    /// Frame size as `FFmpeg` presents it to the filter graph.
    ///
    /// Autorotation happens before any filter runs, so a quarter-turned source
    /// reaches the graph transposed. Every scaling, padding, and aspect
    /// calculation has to use this rather than the stored `frame_size`, or a
    /// portrait recording is scaled as if it were landscape.
    #[must_use]
    pub fn displayed_frame_size(&self) -> Option<FrameSize> {
        let frame_size = self.frame_size?;
        if self.is_quarter_turned() {
            return Some(FrameSize::new(frame_size.height, frame_size.width));
        }
        Some(frame_size)
    }

    #[must_use]
    pub const fn with_index(mut self, index: usize) -> Self {
        self.index = index;
        self
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
        // Compared against the *stored* size on purpose: both tags describe the
        // encoded picture, before any display-matrix rotation is applied.
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

/// Square-pixel sizing for an anamorphic source that is also quarter turned.
///
/// `width`/`height` here are the *displayed* axes; the sample aspect ratio
/// applies to the stored horizontal axis, which after the turn is the displayed
/// vertical one.
fn quarter_turned_square_pixel_frame_size(
    displayed_height: f64,
    displayed_width: f64,
    sample_aspect_ratio_factor: f64,
    resolution: FrameSize,
    scale: SquarePixelScale,
) -> Option<FrameSize> {
    let width = displayed_width;
    let height = (displayed_height * sample_aspect_ratio_factor).floor();
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
        scaled_dimension(width, percent, scale),
        scaled_dimension(height, percent, scale),
        scale,
    ))
}

fn square_pixel_frame_size(
    source: &SourceVideoStream,
    resolution: FrameSize,
    scale: SquarePixelScale,
) -> Option<FrameSize> {
    // Autorotation runs before the filter graph, so scaling must reason about
    // the displayed frame, not the stored one. A portrait recording stored as
    // 1920x1080 with a 90-degree display matrix reaches the graph as 1080x1920;
    // using the stored size would scale it as if it were landscape.
    let source_size = source.displayed_frame_size()?;
    let mut width = f64::from(source_size.width);
    let height = f64::from(source_size.height);

    if source.is_anamorphic() {
        // The sample aspect ratio describes stored pixels, so on a quarter turn
        // it stretches what is now the vertical axis.
        let factor = source.sample_aspect_ratio_factor()?;
        if source.is_quarter_turned() {
            return quarter_turned_square_pixel_frame_size(
                height, width, factor, resolution, scale,
            );
        }
        width = (width * factor).floor();
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
        scaled_dimension(width, percent, scale),
        scaled_dimension(height, percent, scale),
        scale,
    ))
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn scaled_dimension(value: f64, percent: f64, scale: SquarePixelScale) -> u32 {
    let scaled = value * percent;
    if !scaled.is_finite() || scaled <= 0.0 {
        return 0;
    }
    let scaled = match scale {
        SquarePixelScale::Fit => scaled.floor(),
        SquarePixelScale::Fill => scaled.ceil(),
    };
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

const fn even_frame_size(width: u32, height: u32, scale: SquarePixelScale) -> FrameSize {
    match scale {
        SquarePixelScale::Fit => FrameSize::new(even_floor(width), even_floor(height)),
        SquarePixelScale::Fill => FrameSize::new(even_ceil(width), even_ceil(height)),
    }
}

const fn even_floor(value: u32) -> u32 {
    let even = value - (value % 2);
    if even == 0 {
        2
    } else {
        even
    }
}

const fn even_ceil(value: u32) -> u32 {
    let even = value + (value % 2);
    if even == 0 {
        2
    } else {
        even
    }
}

/// Source audio stream metadata reserved for audio decoder dispatch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceAudioStream {
    pub index: usize,
    pub codec: SourceAudioCodec,
    pub channels: Option<u32>,
    pub sample_rate: Option<u32>,
    pub default: bool,
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
            default: false,
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
///
/// Mirrors ErsatzTV's `ColorParams` (range/space/transfer/primaries) plus the
/// HDR flags ErsatzRS already uses for tonemap dispatch. The raw tags feed the
/// `colorspace`/`colormatrix` normalization filter ported from
/// `*PipelineBuilder.SetPixelFormat`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SourceColorParams {
    pub color_range: Option<String>,
    pub color_space: Option<String>,
    pub color_transfer: Option<String>,
    pub color_primaries: Option<String>,
    pub is_hdr: bool,
    pub is_bt2020_ten: bool,
}

impl SourceColorParams {
    /// Mirror of ErsatzTV `ColorParams.IsBt709`: already-bt709 sources need no
    /// colorspace normalization. True when the tags equal the fully-specified
    /// bt709 default, or when range is `tv`/space is `bt709` with transfer and
    /// primaries unset (some sources omit those two tags).
    #[must_use]
    pub fn is_bt709(&self) -> bool {
        let range = trimmed_tag(self.color_range.as_deref());
        let space = trimmed_tag(self.color_space.as_deref());
        let transfer = trimmed_tag(self.color_transfer.as_deref());
        let primaries = trimmed_tag(self.color_primaries.as_deref());

        if range == "tv" && space == "bt709" && transfer == "bt709" && primaries == "bt709" {
            return true;
        }

        range == "tv" && space == "bt709" && transfer.is_empty() && primaries.is_empty()
    }

    /// Build the filter-facing [`ColorParams`], deriving the ErsatzTV
    /// classification flags (`IsUnknown`/`IsMixed`/`Bt470Bg`/`IsBt2020Ten`)
    /// directly from the raw tags so `colorspace_filter` matches ErsatzTV's
    /// `ColorspaceFilter` branch-for-branch.
    #[must_use]
    fn to_filter_color_params(&self) -> ColorParams {
        let range = trimmed_tag(self.color_range.as_deref());
        let space = trimmed_tag(self.color_space.as_deref());
        let transfer = trimmed_tag(self.color_transfer.as_deref());
        let primaries = trimmed_tag(self.color_primaries.as_deref());

        // ErsatzTV: ColorParams.Bt470Bg == ("tv", "bt470bg", "bt470bg", "bt470bg").
        let is_bt470_bg =
            range == "tv" && space == "bt470bg" && transfer == "bt470bg" && primaries == "bt470bg";
        // ErsatzTV: IsUnknown == space/transfer/primaries all blank.
        let is_unknown = space.is_empty() && transfer.is_empty() && primaries.is_empty();
        // ErsatzTV: IsMixed == space != transfer || transfer != primaries || range blank.
        let is_mixed = space != transfer || transfer != primaries || range.is_empty();
        // ErsatzTV: IsBt2020Ten == transfer is "bt2020-10".
        let is_bt2020_ten = transfer == "bt2020-10";

        ColorParams {
            color_range: self.color_range.clone(),
            color_space: self.color_space.clone(),
            color_transfer: self.color_transfer.clone(),
            color_primaries: self.color_primaries.clone(),
            is_unknown,
            is_mixed,
            is_bt470_bg,
            is_bt2020_ten,
        }
    }
}

fn trimmed_tag(value: Option<&str>) -> &str {
    value.map(str::trim).unwrap_or_default()
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

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq)]
pub struct DesiredVideoState {
    pub scaled_size: FrameSize,
    pub frame_size: FrameSize,
    pub cropped_size: Option<FrameSize>,
    pub scaling_behavior: ScalingBehavior,
    pub pad_mode: FilterMode,
    pub frame_rate: Option<FrameRate>,
    pub video_format: FFmpegProfileVideoFormat,
    pub audio_format: FFmpegProfileAudioFormat,
    pub bit_depth: FFmpegProfileBitDepth,
    pub tonemap_algorithm: FFmpegProfileTonemapAlgorithm,
    /// ADR 0064 output dynamic-range contract. Describes the output stream, so
    /// it does not change when an SDR item follows an HDR item.
    pub output_dynamic_range: OutputDynamicRange,
    /// Operator override for the HDR10 mastering display ErsatzRS signals when
    /// it has to synthesize metadata, or `None` for the documented default
    /// (ADR 0064). Never overrides metadata the source itself carried.
    pub hdr_mastering_display: Option<MasteringDisplayMetadata>,
    /// Operator override for the HDR10 content light levels, or `None` for the
    /// documented default (ADR 0064).
    pub hdr_content_light: Option<ContentLightLevel>,
    pub video_profile: Option<String>,
    pub video_preset: Option<String>,
    pub video_tune: Option<String>,
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
    pub deinterlace: bool,
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
            scaling_behavior: ScalingBehavior::ScaleAndPad,
            pad_mode: FilterMode::Software,
            frame_rate: Some(FrameRate::DEFAULT),
            video_format: FFmpegProfileVideoFormat::H264,
            audio_format: FFmpegProfileAudioFormat::Aac,
            bit_depth: FFmpegProfileBitDepth::EightBit,
            tonemap_algorithm: FFmpegProfileTonemapAlgorithm::Hable,
            output_dynamic_range: OutputDynamicRange::SdrBt709,
            hdr_mastering_display: None,
            hdr_content_light: None,
            video_profile: Some("high".to_owned()),
            video_preset: Some("veryfast".to_owned()),
            video_tune: None,
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
            // Matches ErsatzTV `FFmpegProfile.New` (NormalizeColors = true) and
            // the ErsatzRS DB seed (`db.rs`), so transcodes normalize colors to
            // bt709 by default via the colorspace filter above.
            normalize_colors: true,
            deinterlace: true,
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
            scaling_behavior: profile.scaling_behavior,
            pad_mode: profile.pad_mode,
            frame_rate,
            video_format: profile.video_format,
            audio_format: profile.audio_format,
            bit_depth: effective_bit_depth(profile),
            tonemap_algorithm: profile.tonemap_algorithm,
            output_dynamic_range: profile.output_dynamic_range,
            hdr_mastering_display: profile.hdr_mastering_display,
            hdr_content_light: profile.hdr_content_light,
            video_profile: non_empty_string(profile.video_profile.as_deref()),
            video_preset: normalized_video_preset(profile.video_preset.as_deref()),
            video_tune: normalized_video_tune(
                profile.video_preset.as_deref(),
                profile.video_tune.as_deref(),
            ),
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
            deinterlace: profile.deinterlace_video == Some(true),
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
            scaling_behavior: profile.scaling_behavior,
            pad_mode: profile.pad_mode,
            frame_rate: None,
            video_format: FFmpegProfileVideoFormat::Copy,
            audio_format: FFmpegProfileAudioFormat::Copy,
            bit_depth: effective_bit_depth(profile),
            tonemap_algorithm: profile.tonemap_algorithm,
            // Video copy preserves the source elementary stream, so ADR 0064's
            // output contract does not apply and must not drive filters.
            output_dynamic_range: OutputDynamicRange::SdrBt709,
            hdr_mastering_display: None,
            hdr_content_light: None,
            video_profile: None,
            video_preset: None,
            video_tune: None,
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
            deinterlace: false,
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
        // HDR10 output is 10-bit by definition; profile validation already
        // rejects an 8-bit HDR10 profile, so this only guards the in-memory
        // states that tests and callers can still construct by hand.
        if self.output_dynamic_range.is_hdr10() {
            return PixelFormat::Yuv420P10Le;
        }
        match self.bit_depth {
            FFmpegProfileBitDepth::EightBit => PixelFormat::Yuv420P,
            FFmpegProfileBitDepth::TenBit => PixelFormat::Yuv420P10Le,
        }
    }

    /// True when this state must produce BT.2020/PQ HDR10 output.
    ///
    /// Stream copy never transforms the picture, so the contract is inert.
    #[must_use]
    pub const fn produces_hdr10(&self) -> bool {
        self.output_dynamic_range.is_hdr10()
            && !matches!(self.video_format, FFmpegProfileVideoFormat::Copy)
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
    apply_source_stream_indexes(&mut request);
    let source_metadata_is_runtime_only = request.source_metadata_is_runtime_only();

    let effective_acceleration = effective_video_acceleration(&request, acceleration);
    let effective_decode_acceleration =
        effective_decode_acceleration(&request, acceleration, effective_acceleration);

    // On an encoder-only FFmpeg build (NVENC encoders + `cuda` hwaccel but no
    // CUDA filter kernels) the on-GPU filter graph is unrunnable, so when
    // decode falls back to software the whole deinterlace/scale/subtitle/
    // overlay graph and pixel-format conversion must run in software too —
    // there is no `scale_cuda` to receive uploaded frames. Demote the graph
    // acceleration to software while keeping the NVENC encoder (it accepts
    // software frames directly), so the build still gets GPU-accelerated
    // encode instead of dropping to libx264. Full builds keep the landed
    // upload-fallback path: software-decoded frames are uploaded and filtered
    // on the GPU.
    let encoder_acceleration = effective_acceleration;
    let cuda_filters_available = request
        .runtime_capabilities
        .as_ref()
        .is_none_or(crate::capabilities::nvenc_cuda_filters_available);
    let effective_acceleration = if effective_acceleration == HardwareAccelerationKind::Nvenc
        && effective_decode_acceleration != HardwareAccelerationKind::Nvenc
        && !cuda_filters_available
    {
        HardwareAccelerationKind::None
    } else {
        effective_acceleration
    };

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

    if request.state.retained_stream_timestamps {
        // ADR 0087: a failed retained media read must not become successful,
        // shortened EOF that advances the programme past unplayed content.
        steps.push(PipelineStep::global(
            "retained_stream_timestamps",
            ["-copyts", "-xerror"],
        ));
    }

    if let Some(thread_count) = request.state.thread_count {
        steps.insert(
            0,
            PipelineStep::global("threads", ["-threads".to_owned(), thread_count.to_string()]),
        );
    }

    if let Some(start) = request.state.start {
        steps.push(PipelineStep::stream_seek(start));
    }
    if let Some(start) = request.state.output_start {
        steps.push(PipelineStep::generic(
            "output_seek",
            ["-ss".to_owned(), format_time_limit(start)],
        ));
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
    // The decoder is selected from the requested encoder acceleration, not the
    // (possibly demoted) graph acceleration: under an NVENC profile a software
    // decode is forced explicit (`-hwaccel none -c:v …`), mirroring ErsatzTV.
    add_video_decoder(&request, &mut steps, encoder_acceleration);
    add_audio_decoder_input_options(&mut request);
    add_hdr10_static_metadata_input_options(&mut request);
    add_realtime_input_options(&mut request);
    let uploaded_cuda_fallback =
        add_cuda_software_fallback_upload(&mut request, effective_acceleration);
    apply_source_video_filters(
        &request,
        effective_acceleration,
        uploaded_cuda_fallback,
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

    // Pair each watermark input with its overlay in attachment order before
    // sorting, so the z-index sort cannot mismatch an input's scale/opacity
    // filters with another watermark's overlay.
    let mut watermark_pairs: Vec<(InputFile, WatermarkOverlay)> = {
        let mut watermark_inputs = Vec::new();
        let mut remaining = Vec::with_capacity(request.inputs.len());
        for input in request.inputs.drain(..) {
            if input.kind == InputKind::Watermark {
                watermark_inputs.push(input);
            } else {
                remaining.push(input);
            }
        }
        request.inputs = remaining;
        watermark_inputs
            .into_iter()
            .zip(std::mem::take(&mut request.watermarks))
            .collect()
    };
    watermark_pairs.sort_by_key(|(_, overlay)| overlay.z_index);

    // Port of `FFmpegLibraryProcessService`: "use ffmpeg for single permanent
    // watermark, graphics engine for all others". A permanent watermark has no
    // intermittent fade window and no opacity expression. Dynamic watermarks
    // (intermittent/opacity-expression/multiple, or any combined with graphics
    // elements — including resource-backed song-progress overlays) join the
    // z-ordered graphics overlay plan so they composite with graphics elements
    // the way ErsatzTV's graphics engine does; per ADR 0005 ErsatzRS composites
    // them with native overlay/fade/opacity filters instead of a raster frame
    // loop, and drawvg remains the vector path.
    let single_permanent_watermark = request.graphics_overlays.is_empty()
        && request.drawvg_filters.is_empty()
        && watermark_pairs.len() == 1
        && watermark_pairs.iter().all(|(_, overlay)| {
            overlay.fade_points.is_empty()
                && overlay
                    .opacity_expression
                    .as_deref()
                    .is_none_or(|expression| expression.trim().is_empty())
        });
    if single_permanent_watermark {
        for (input, overlay) in watermark_pairs {
            request.inputs.push(input);
            request.watermarks.push(overlay);
        }
    } else {
        for (mut input, overlay) in watermark_pairs {
            input.kind = InputKind::GraphicsImage;
            request
                .graphics_overlays
                .push(GraphicsPipelineOverlay::native_image(
                    overlay.z_index,
                    input,
                    overlay,
                ));
        }
    }
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
    let has_drawtext_graphics = request
        .graphics_overlays
        .iter()
        .any(|overlay| matches!(overlay, GraphicsPipelineOverlay::Drawtext { .. }));
    let has_text_subtitle_burn = request.inputs.iter().any(|input| {
        input.kind == InputKind::Subtitle
            && !input.is_image_based_subtitle
            && input.subtitle_method == Some(SubtitleMethod::Burn)
    });
    download_hardware_frames_for_drawtext(
        has_drawtext_graphics,
        &request.desired,
        &mut target_state,
        &mut filter_chain,
    );
    let use_cuda_overlay = uses_cuda_overlay(effective_acceleration, &request.desired)
        && profile_5_operation(&request).is_none()
        && !has_text_subtitle_burn
        && !has_drawtext_graphics;
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
    // Mirror ErsatzTV `NvidiaPipelineBuilder` (lines 239-271): for an 8-bit CUDA
    // overlay the main surface must be `yuv420p` before compositing.
    // `overlay_cuda` refuses to overlay a `yuva420p` watermark/graphic onto an
    // `nv12` surface ("Can't overlay yuva420p on nv12") — both inputs need the
    // matching 4:2:0 planar layout. Convert the main on the GPU
    // (`scale_cuda=format=yuv420p`) so the alpha overlay composites and nvenc
    // still encodes a CUDA surface directly. 10-bit overlays take the
    // software-download path above instead, so this is gated on 8-bit hardware
    // frames.
    let has_cuda_overlay_input =
        has_watermark_overlay || has_image_subtitle_burn || has_native_graphics_overlay;
    if use_cuda_overlay
        && has_cuda_overlay_input
        && request.desired.bit_depth == FFmpegProfileBitDepth::EightBit
        && target_state.frame_data_location == crate::FrameDataLocation::Hardware
        && target_state.pixel_format != PixelFormat::Yuv420P
    {
        filter_chain.video.push(format!(
            "scale_cuda=format={}",
            PixelFormat::Yuv420P.ffmpeg_name()
        ));
        target_state.pixel_format = PixelFormat::Yuv420P;
    }
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
                .push(watermark_opacity_expression_filter_with_clock(
                    expression,
                    &request.watermark_opacity_clock,
                ));
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
    let mut emit_pixel_format_output_option =
        should_emit_pixel_format_output_option(&request, effective_acceleration, &target_state);

    apply_colorspace_normalization(
        &request,
        effective_acceleration,
        &mut target_state,
        &mut filter_chain,
        &mut emit_pixel_format_output_option,
    );

    let runtime_capabilities = request.runtime_capabilities.clone();
    let mut graphics_image_index = 0usize;
    for overlay in std::mem::take(&mut request.graphics_overlays) {
        match overlay {
            GraphicsPipelineOverlay::Drawvg { filter, .. } => {
                filter_chain
                    .graphics_overlay
                    .push(GraphicsOverlayFilter::Drawvg(filter));
            }
            GraphicsPipelineOverlay::Drawtext { filter, .. } => {
                filter_chain
                    .graphics_overlay
                    .push(GraphicsOverlayFilter::Drawtext(filter));
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
                        .push(watermark_opacity_expression_filter_with_clock(
                            expression,
                            &request.watermark_opacity_clock,
                        ));
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
    if has_drawtext_graphics && request.desired.video_format != FFmpegProfileVideoFormat::Copy {
        upload_software_frames_for_hardware_encoder_filters(
            effective_acceleration,
            &mut target_state,
            &mut filter_chain.pixel_format,
        );
        emit_pixel_format_output_option =
            should_emit_pixel_format_output_option(&request, effective_acceleration, &target_state);
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
        encoder_acceleration,
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
    if request
        .inputs
        .iter()
        .any(|input| input.kind == InputKind::Audio)
    {
        steps.push(audio_encoder(
            &request.desired,
            selected_source_audio_stream(
                request.inputs.as_slice(),
                request.source_audio_streams.as_slice(),
            ),
            source_metadata_is_runtime_only,
        ));
    }
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

fn apply_source_stream_indexes(request: &mut PipelineRequest) {
    // Fresh NUT indexes were explicitly selected by the caller and may differ
    // from historical source metadata. Never override that selection.
    if request.state.retained_stream_timestamps {
        return;
    }
    if let Some(source_video_stream) = &request.source_video_stream {
        if let Some(video_input) = request
            .inputs
            .iter_mut()
            .find(|input| input.kind == InputKind::Video)
        {
            video_input.streams = vec![crate::MediaStream::new(source_video_stream.index)];
        }
    }

    let Some(audio_input) = request
        .inputs
        .iter_mut()
        .find(|input| input.kind == InputKind::Audio)
    else {
        return;
    };

    let selected_audio_index = audio_input.streams.first().map(|stream| stream.index);
    let selected_audio_exists = selected_audio_index.is_some_and(|index| {
        request
            .source_audio_streams
            .iter()
            .any(|stream| stream.index == index)
    });
    if !selected_audio_exists {
        if let Some(source_audio_stream) = request.source_audio_streams.first() {
            audio_input.streams = vec![crate::MediaStream::new(source_audio_stream.index)];
        }
    }
}

fn apply_source_video_filters(
    request: &PipelineRequest,
    acceleration: HardwareAccelerationKind,
    uploaded_cuda_fallback: bool,
    target_state: &mut FrameState,
    filter_chain: &mut FilterChain,
) {
    let is_transcode = request.desired.video_format != FFmpegProfileVideoFormat::Copy;
    let profile_5_operation = profile_5_operation(request);
    initialize_source_video_state(request, is_transcode, target_state);

    if let Some(operation) = profile_5_operation {
        apply_profile_5_conversion(request, operation, target_state, filter_chain);
    }

    if profile_5_operation.is_none()
        && target_state.should_deinterlace
        && !target_state.deinterlaced
    {
        if acceleration == HardwareAccelerationKind::Nvenc
            && uploaded_cuda_fallback
            && target_state.frame_data_location == crate::FrameDataLocation::Software
        {
            let mut uploaded_state = target_state.clone();
            uploaded_state.frame_data_location = crate::FrameDataLocation::Hardware;
            filter_chain
                .video
                .push(yadif_cuda_filter(&uploaded_state).to_owned());
        } else {
            filter_chain.video.push(deinterlace_filter(
                acceleration,
                target_state,
                request.desired.qsv_extra_hardware_frames,
            ));
        }
        target_state.deinterlaced = true;
        target_state.frame_data_location = deinterlace_output_location(acceleration);
    }

    // The cuda software-decode fallback uploads frames to the GPU as the
    // first input filter step, so past the deinterlace decision the surface
    // is hardware whether or not a deinterlace filter consumed the upload.
    if acceleration == HardwareAccelerationKind::Nvenc && uploaded_cuda_fallback {
        target_state.frame_data_location = crate::FrameDataLocation::Hardware;
    }

    // ADR 0064: the profile's output dynamic range decides what happens to the
    // source's dynamic range. `SdrBt709` keeps ErsatzTV's tonemap-to-BT.709
    // behavior exactly; `Hdr10Bt2020Pq` is the beyond-parity branch and applies
    // to every item so a linear stream never changes its signalling mid-flight.
    let used_hdr_tonemap = profile_5_operation
        == Some(HardwareGraphOperation::NvencDolbyVisionProfile5Sdr)
        || target_state.is_hdr && is_transcode && !request.desired.produces_hdr10();
    if profile_5_operation.is_none() && is_transcode && request.desired.produces_hdr10() {
        apply_hdr10_output_conversion(request, acceleration, target_state, filter_chain);
    } else if profile_5_operation.is_none() && used_hdr_tonemap {
        filter_chain.video.push(tonemap_filter_for(
            acceleration,
            target_state,
            request.desired.pixel_format(),
            tonemap_algorithm(request.desired.tonemap_algorithm),
            nvidia_vulkan_hdr_tonemap(request),
        ));
        target_state.is_hdr = false;
        // ErsatzTV resets the stream color params to `ColorParams.Default`
        // after tonemap, so the later SetPixelFormat colorspace gate sees
        // BT.709 and does not stack a second conversion on the tonemapped path.
        target_state.colors_are_bt709 = true;
        target_state.pixel_format = request.desired.pixel_format();
        target_state.frame_data_location = tonemap_output_location(acceleration);
    }

    if !is_transcode {
        return;
    }

    apply_source_video_geometry(
        request,
        acceleration,
        profile_5_operation,
        used_hdr_tonemap,
        target_state,
        filter_chain,
    );
}

fn apply_source_video_geometry(
    request: &PipelineRequest,
    acceleration: HardwareAccelerationKind,
    profile_5_operation: Option<HardwareGraphOperation>,
    used_hdr_tonemap: bool,
    target_state: &mut FrameState,
    filter_chain: &mut FilterChain,
) {
    if let Some(cropped_size) = request.desired.cropped_size {
        target_state.cropped_size = Some(cropped_size);
    }

    let source_frame_size_unknown = request.source_metadata_is_runtime_only()
        && request
            .source_video_stream
            .as_ref()
            .and_then(|source| source.frame_size)
            .is_none();

    if source_frame_size_unknown {
        if profile_5_operation.is_none() {
            apply_unknown_source_video_geometry(request, acceleration, target_state, filter_chain);
        }
        return;
    }

    if profile_5_operation.is_none() {
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
            track_hardware_scale_output(acceleration, request.desired.bit_depth, target_state);
        }
    }

    if request.desired.cropped_size.is_none()
        && target_state.padded_size != request.desired.frame_size
    {
        apply_pad_filter(
            request,
            acceleration,
            used_hdr_tonemap,
            target_state,
            filter_chain,
        );
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
        if profile_5_operation.is_none() {
            upload_software_frames_for_hardware_encoder(acceleration, target_state, filter_chain);
        }
    }
}

fn apply_unknown_source_video_geometry(
    request: &PipelineRequest,
    acceleration: HardwareAccelerationKind,
    target_state: &mut FrameState,
    filter_chain: &mut FilterChain,
) {
    filter_chain.video.push(unknown_source_scale_filter(
        target_state,
        request.desired.frame_size,
        request.desired.scaling_behavior,
    ));
    target_state.scaled_size = request.desired.frame_size;
    target_state.padded_size = request.desired.frame_size;
    target_state.cropped_size = None;
    target_state.frame_data_location = crate::FrameDataLocation::Software;
    target_state.pixel_format = request.desired.pixel_format();
    upload_software_frames_for_hardware_encoder(acceleration, target_state, filter_chain);
}

fn initialize_source_video_state(
    request: &PipelineRequest,
    is_transcode: bool,
    target_state: &mut FrameState,
) {
    let Some(source) = &request.source_video_stream else {
        return;
    };
    target_state.should_deinterlace = request.desired.deinterlace
        && source.scan_kind == VideoScanKind::Interlaced
        && is_transcode;
    target_state.is_hdr = source.color_params.is_hdr || source.color_params.is_bt2020_ten;
    target_state.colors_are_bt709 = source.color_params.is_bt709();

    // Track the source pixel format while frames live in software so the
    // output conversion is added only when needed. Hardware setup owns its
    // surface format and must not be overwritten here.
    if target_state.frame_data_location == crate::FrameDataLocation::Software {
        if let Some(pixel_format) = source.pixel_format {
            target_state.pixel_format = pixel_format;
        }
    }
}

fn apply_profile_5_conversion(
    request: &PipelineRequest,
    operation: HardwareGraphOperation,
    target_state: &mut FrameState,
    filter_chain: &mut FilterChain,
) {
    let filter = match operation {
        HardwareGraphOperation::NvencDolbyVisionProfile5Sdr => {
            dolby_vision_profile_5_sdr_filter(request.desired.scaled_size)
        }
        HardwareGraphOperation::NvencDolbyVisionProfile5Hdr10 => {
            dolby_vision_profile_5_hdr10_filter(request.desired.scaled_size)
        }
        HardwareGraphOperation::NvencHdrVulkanLibplacebo
        | HardwareGraphOperation::NvencCudaScale => unreachable!("profile 5 operation"),
    };
    filter_chain.video.push(filter);
    target_state.frame_data_location = crate::FrameDataLocation::Software;
    target_state.pixel_format = request.desired.pixel_format();
    target_state.scaled_size = request.desired.scaled_size;
    target_state.padded_size = request.desired.scaled_size;
    target_state.is_hdr = request.desired.produces_hdr10();
    target_state.colors_are_bt709 = !request.desired.produces_hdr10();
}

/// Establish the ADR 0064 `Hdr10Bt2020Pq` output contract for one item.
///
/// The three source cases are deliberately distinct rather than a single
/// "convert to PQ" chain, because they have different costs and different
/// metadata consequences:
///
/// * PQ source: the picture already satisfies the contract, so only the output
///   tags are pinned and the encoder carries the source's own mastering-display
///   and content-light side data through untouched.
/// * HLG source: converted to PQ, because one linear stream must not switch
///   transfer function between items.
/// * SDR source: mapped into PQ at the documented BT.2408 reference white.
///
/// Both converting cases change what the source's static metadata means, so the
/// caller emits validated profile metadata for them instead of copying the
/// source's — see [`hdr10_static_metadata_input_options`].
/// NVENC keeps the frames on the GPU: `setparams` works on CUDA surfaces, and
/// the converting cases re-upload with `hwupload_cuda` so `hevc_nvenc` still
/// encodes. Every other acceleration has already been demoted to software by
/// [`effective_video_acceleration`], so only these two branches exist.
fn apply_hdr10_output_conversion(
    request: &PipelineRequest,
    acceleration: HardwareAccelerationKind,
    target_state: &mut FrameState,
    filter_chain: &mut FilterChain,
) {
    let transfer = source_transfer_kind(request);
    let uses_cuda = acceleration == HardwareAccelerationKind::Nvenc;
    // Only frames that are actually in hardware need a download, and the format
    // has to be the source surface's own — see `sdr_to_hdr10_filter`.
    let download =
        (target_state.frame_data_location == crate::FrameDataLocation::Hardware).then(|| {
            hardware_surface_download_format(
                request
                    .source_video_stream
                    .as_ref()
                    .and_then(|source| source.pixel_format),
            )
        });

    let source_tags = request
        .source_video_stream
        .as_ref()
        .map(|source| SourceColorTags {
            color_range: source.color_params.color_range.clone(),
            color_space: source.color_params.color_space.clone(),
            color_transfer: source.color_params.color_transfer.clone(),
            color_primaries: source.color_params.color_primaries.clone(),
        })
        .unwrap_or_default();

    let mut filter = match (transfer, uses_cuda) {
        (SourceTransferKind::Pq, _) => preserve_hdr10_filter(),
        (SourceTransferKind::Hlg, false) => hlg_to_hdr10_filter(download),
        (SourceTransferKind::Hlg, true) => hlg_to_hdr10_cuda_filter(download),
        (SourceTransferKind::Sdr, false) => sdr_to_hdr10_filter(download, &source_tags),
        (SourceTransferKind::Sdr, true) => sdr_to_hdr10_cuda_filter(download, &source_tags),
    };

    // ADR 0064: a transcoded HDR10 stream is HDR10, not Dolby Vision. Its base
    // layer has been re-encoded and no RPU accompanies it, so retaining the
    // Dolby Vision record would describe a stream that no longer exists.
    // HDR10's own mastering-display and content-light metadata stays.
    if request
        .source_video_stream
        .as_ref()
        .is_some_and(|source| source.dolby_vision.is_some())
    {
        filter = format!("{filter},{}", strip_dolby_vision_metadata_filter());
    }
    let converts_frames = transfer != SourceTransferKind::Pq;

    filter_chain.video.push(filter);
    target_state.is_hdr = true;
    // The output is BT.2020/PQ, never BT.709. Marking it "bt709" would be a
    // lie, but the colorspace normalization gate keys off this flag, so HDR10
    // output is excluded there explicitly instead.
    target_state.colors_are_bt709 = false;
    if converts_frames {
        target_state.pixel_format = PixelFormat::Yuv420P10Le;
        target_state.frame_data_location = if uses_cuda {
            // The chain ends in `hwupload_cuda`.
            crate::FrameDataLocation::Hardware
        } else {
            crate::FrameDataLocation::Software
        };
    }
}

/// The hardware graph a built command actually selected.
///
/// Derived from the generated argv rather than tracked separately, so it cannot
/// drift from what was really spawned. Used by the stream supervisors to
/// attribute a runtime failure to the right graph.
#[must_use]
pub fn selected_hardware_graph(command: &FfmpegCommand) -> Option<HardwareGraphOperation> {
    let applies_dolby_vision = command
        .arguments
        .iter()
        .any(|argument| argument.contains("apply_dolbyvision=true"));
    if applies_dolby_vision {
        let produces_hdr10 = command
            .arguments
            .iter()
            .any(|argument| argument.contains("format=rgba64le") || argument == "hevc_nvenc");
        return Some(if produces_hdr10 {
            HardwareGraphOperation::NvencDolbyVisionProfile5Hdr10
        } else {
            HardwareGraphOperation::NvencDolbyVisionProfile5Sdr
        });
    }

    let uses_vulkan_libplacebo = command
        .arguments
        .iter()
        .any(|argument| argument.contains("libplacebo"))
        && command
            .arguments
            .windows(2)
            .any(|pair| pair[0] == "-init_hw_device" && pair[1].starts_with("vulkan"));
    if uses_vulkan_libplacebo {
        return Some(HardwareGraphOperation::NvencHdrVulkanLibplacebo);
    }

    command
        .arguments
        .iter()
        .any(|argument| argument.contains("scale_cuda") || argument.contains("hwupload_cuda"))
        .then_some(HardwareGraphOperation::NvencCudaScale)
}

/// A source ErsatzRS refuses to transcode, carrying the operator-facing reason.
///
/// ADR 0064 requires an unproven Dolby Vision path to fail visibly. Returning
/// this from the command builder is what makes the refusal real: a caller
/// cannot accidentally spawn `FFmpeg` on a source that would decode to
/// plausible but incorrect colour, because there is no command to spawn.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{reason}")]
pub struct UnsupportedSource {
    pub reason: String,
}

/// Build a playback command, refusing sources ErsatzRS cannot render correctly.
///
/// Prefer this over [`build_pipeline_for_acceleration`] at any boundary that
/// goes on to spawn a child process.
///
/// # Errors
///
/// Returns [`UnsupportedSource`] when the selected source carries a Dolby
/// Vision configuration with no characterized conversion.
pub fn build_pipeline_command(
    acceleration: HardwareAccelerationKind,
    request: PipelineRequest,
) -> Result<FfmpegCommand, UnsupportedSource> {
    if let Some(reason) = unsupported_source_reason_for_acceleration(Some(acceleration), &request) {
        return Err(UnsupportedSource { reason });
    }
    let inputs = request.inputs.clone();
    let executable = request.ffmpeg_executable.clone();
    let pipeline = build_pipeline_for_acceleration(acceleration, request);
    let mut command = pipeline.command(&inputs);
    if let Some(executable) = executable {
        command.executable = executable;
    }
    Ok(command)
}

/// Why a source cannot be played, when ErsatzRS refuses rather than guessing.
///
/// ADR 0064 requires an unproven Dolby Vision path to fail visibly instead of
/// producing plausible but incorrect colour, so callers surface this to
/// troubleshooting rather than starting a transcode that would look wrong.
#[must_use]
pub fn unsupported_source_reason(request: &PipelineRequest) -> Option<String> {
    unsupported_source_reason_for_acceleration(None, request)
}

fn unsupported_source_reason_for_acceleration(
    acceleration: Option<HardwareAccelerationKind>,
    request: &PipelineRequest,
) -> Option<String> {
    if let Some(incomplete) = request
        .source_video_stream
        .as_ref()
        .and_then(|source| source.incomplete_dolby_vision.as_ref())
    {
        return Some(format!(
            "Dolby Vision metadata is incomplete: {}",
            incomplete.reason
        ));
    }
    let dolby_vision = request
        .source_video_stream
        .as_ref()
        .and_then(|source| source.dolby_vision)?;

    if request.desired.video_format == FFmpegProfileVideoFormat::Copy {
        return Some(
            "Dolby Vision stream copy is disabled until the client/container/profile compatibility matrix is proven"
                .to_owned(),
        );
    }

    if matches!(
        dolby_vision.handling(),
        DolbyVisionHandling::LibplaceboRequired
    ) {
        return profile_5_admission_reason(acceleration, request);
    }

    if matches!(
        dolby_vision.handling(),
        DolbyVisionHandling::Hdr10BaseLayerDirectDecode
    ) && !request
        .source_video_stream
        .as_ref()
        .is_some_and(is_hevc_main10_source)
    {
        return Some(
            "Dolby Vision profile 7 support requires HEVC Main10 yuv420p10le input".to_owned(),
        );
    }

    if dolby_vision.is_playable() {
        return None;
    }

    Some(dolby_vision.evidence())
}

fn profile_5_operation(request: &PipelineRequest) -> Option<HardwareGraphOperation> {
    let configuration = request
        .source_video_stream
        .as_ref()
        .and_then(|source| source.dolby_vision)?;
    if !matches!(
        configuration.handling(),
        DolbyVisionHandling::LibplaceboRequired
    ) {
        return None;
    }
    if request.desired.produces_hdr10()
        && request.desired.video_format == FFmpegProfileVideoFormat::Hevc
        && request.desired.bit_depth == FFmpegProfileBitDepth::TenBit
    {
        return Some(HardwareGraphOperation::NvencDolbyVisionProfile5Hdr10);
    }
    if !request.desired.produces_hdr10()
        && request.desired.video_format == FFmpegProfileVideoFormat::H264
        && request.desired.bit_depth == FFmpegProfileBitDepth::EightBit
    {
        return Some(HardwareGraphOperation::NvencDolbyVisionProfile5Sdr);
    }
    None
}

fn profile_5_admission_reason(
    acceleration: Option<HardwareAccelerationKind>,
    request: &PipelineRequest,
) -> Option<String> {
    if acceleration != Some(HardwareAccelerationKind::Nvenc) {
        return Some(
            "Dolby Vision profile 5 requires the characterized NVIDIA CUDA/Vulkan/libplacebo/NVENC path"
                .to_owned(),
        );
    }
    let Some(source) = request.source_video_stream.as_ref() else {
        return Some("Dolby Vision profile 5 source metadata is missing".to_owned());
    };
    if source.scan_kind == VideoScanKind::Interlaced {
        return Some(
            "interlaced Dolby Vision profile 5 is not supported because the deinterlace order has not been proven"
                .to_owned(),
        );
    }
    if !is_hevc_main10_source(source) {
        return Some(
            "Dolby Vision profile 5 support requires progressive HEVC Main10 yuv420p10le input"
                .to_owned(),
        );
    }
    let Some(operation) = profile_5_operation(request) else {
        return Some(
            "Dolby Vision profile 5 is characterized only for SDR H.264 8-bit and HDR10 HEVC Main10 output"
                .to_owned(),
        );
    };
    let Some(report) = request.runtime_capabilities.as_ref() else {
        return Some(
            "Dolby Vision profile 5 requires a positively proven runtime operation".to_owned(),
        );
    };
    let device = report.hardware_graph_device();
    if report.hardware_graph_denied(operation, &device) {
        return Some(format!(
            "Dolby Vision profile 5 operation {} is denied on the selected device",
            operation.id()
        ));
    }
    if !report.hardware_graph_proven(operation, &device) {
        return Some(format!(
            "Dolby Vision profile 5 operation {} has not been positively proven on the selected device",
            operation.id()
        ));
    }
    if !report.has_decoder("hevc_cuvid")
        || !report
            .nvidia_decoders
            .iter()
            .any(|decoder| decoder.supports("hevc", 10))
    {
        return Some(
            "Dolby Vision profile 5 requires positively reported CUDA HEVC Main10 decode"
                .to_owned(),
        );
    }
    None
}

fn is_hevc_main10_source(source: &SourceVideoStream) -> bool {
    source.codec == SourceVideoCodec::Hevc
        && source.profile.as_deref().is_some_and(|profile| {
            let normalized = profile.to_ascii_lowercase().replace([' ', '-'], "");
            normalized.contains("main10")
        })
        && source.pixel_format == Some(PixelFormat::Yuv420P10Le)
}

fn profile_7_direct_base_layer_decode(request: &PipelineRequest) -> bool {
    request.desired.video_format != FFmpegProfileVideoFormat::Copy
        && request
            .source_video_stream
            .as_ref()
            .and_then(|source| source.dolby_vision)
            .is_some_and(|configuration| {
                matches!(
                    configuration.handling(),
                    DolbyVisionHandling::Hdr10BaseLayerDirectDecode
                )
            })
}

/// Classify the selected source video stream's transfer characteristics.
fn source_transfer_kind(request: &PipelineRequest) -> SourceTransferKind {
    request
        .source_video_stream
        .as_ref()
        .map(|source| {
            SourceTransferKind::from_transfer_tag(source.color_params.color_transfer.as_deref())
        })
        .unwrap_or_default()
}

/// Per-stream HDR10 static-metadata input options for an item.
///
/// ADR 0064 requires these at the input-option boundary, not as generic output
/// options, and forbids copying source metadata onto frames whose meaning a
/// transform has changed. A PQ source therefore emits nothing — its own side
/// data survives the encode — while HLG and SDR sources emit the documented,
/// validated profile defaults.
#[must_use]
pub fn hdr10_static_metadata_input_options(request: &PipelineRequest) -> Vec<String> {
    if !request.desired.produces_hdr10() {
        return Vec::new();
    }
    if source_transfer_kind(request) == SourceTransferKind::Pq
        && profile_5_operation(request).is_none()
    {
        return Vec::new();
    }

    // ADR 0064 prefers validated source metadata over a profile default, but
    // only when the source actually carried it. A converted picture keeps the
    // source's mastering display because that describes the display the content
    // was graded on, which the transfer conversion does not change.
    //
    // When the source is silent the profile override stands in, and the
    // documented default stands in for that. The operator field is therefore
    // the seeded default made editable, not a way to relabel a real grade.
    let source = request.source_video_stream.as_ref();
    let mastering_display = source
        .and_then(|source| source.mastering_display.clone())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| {
            request
                .desired
                .hdr_mastering_display
                .unwrap_or(MasteringDisplayMetadata::BT2020_1000_NITS)
                .to_ffmpeg_value()
        });
    let content_light = match (
        source.and_then(|source| source.max_cll),
        source.and_then(|source| source.max_fall),
    ) {
        (Some(max_cll), Some(max_fall)) if max_cll > 0 => ContentLightLevel {
            max_cll: max_cll.unsigned_abs(),
            max_fall: max_fall.unsigned_abs(),
        },
        _ => request
            .desired
            .hdr_content_light
            .unwrap_or(ContentLightLevel::DEFAULT),
    }
    .to_ffmpeg_value();

    vec![
        "-mastering_display".to_owned(),
        mastering_display,
        "-content_light".to_owned(),
        content_light,
    ]
}

fn track_hardware_scale_output(
    acceleration: HardwareAccelerationKind,
    bit_depth: FFmpegProfileBitDepth,
    target_state: &mut FrameState,
) {
    if acceleration == HardwareAccelerationKind::Qsv {
        target_state.frame_data_location = crate::FrameDataLocation::Hardware;
        target_state.pixel_format = match bit_depth {
            FFmpegProfileBitDepth::EightBit => PixelFormat::Nv12,
            FFmpegProfileBitDepth::TenBit => PixelFormat::Yuv420P10Le,
        };
    } else if acceleration == HardwareAccelerationKind::Nvenc
        && target_state.frame_data_location == crate::FrameDataLocation::Hardware
    {
        // Every NVENC scale operating on hardware frames emits `scale_cuda`,
        // which normalizes the CUDA surface to nv12/p010. Track that actual
        // output even after Vulkan HDR tonemapping; stale yuv420p state would
        // omit the conversion required before a yuva420p `overlay_cuda` input.
        target_state.pixel_format = cuda_surface_pixel_format(bit_depth);
    }
}

fn apply_pad_filter(
    request: &PipelineRequest,
    acceleration: HardwareAccelerationKind,
    used_hdr_tonemap: bool,
    target_state: &mut FrameState,
    filter_chain: &mut FilterChain,
) {
    let use_vaapi_pad = acceleration == HardwareAccelerationKind::Vaapi
        && request.desired.pad_mode == FilterMode::HardwareIfPossible
        && !used_hdr_tonemap;
    if use_vaapi_pad {
        filter_chain
            .video
            .push(pad_vaapi_filter(target_state, request.desired.frame_size));
    } else {
        filter_chain
            .video
            .push(pad_filter(target_state, request.desired.frame_size));
    }
    target_state.padded_size = request.desired.frame_size;
    target_state.scaled_size = request.desired.frame_size;
    if use_vaapi_pad {
        target_state.frame_data_location = crate::FrameDataLocation::Hardware;
        target_state.pixel_format = match request.desired.bit_depth {
            FFmpegProfileBitDepth::EightBit => PixelFormat::Nv12,
            FFmpegProfileBitDepth::TenBit => PixelFormat::Yuv420P10Le,
        };
    } else {
        target_state.frame_data_location = crate::FrameDataLocation::Software;
        target_state.pixel_format = request.desired.pixel_format();
    }
    if profile_5_operation(request).is_none() {
        upload_software_frames_for_hardware_encoder(acceleration, target_state, filter_chain);
    }
}

fn apply_colorspace_normalization(
    request: &PipelineRequest,
    acceleration: HardwareAccelerationKind,
    target_state: &mut FrameState,
    filter_chain: &mut FilterChain,
    emit_pixel_format_output_option: &mut bool,
) {
    if request.desired.video_format == FFmpegProfileVideoFormat::Copy
        || !request.desired.normalize_colors
        || target_state.colors_are_bt709
        // ADR 0064: an HDR10 profile has already established BT.2020/PQ. The
        // ErsatzTV colorspace normalization converts to BT.709, so running it
        // here would silently downgrade HDR10 output to SDR.
        || request.desired.produces_hdr10()
    {
        return;
    }

    let Some(source) = request.source_video_stream.as_ref() else {
        return;
    };
    let color_params = source.color_params.to_filter_color_params();
    if target_state.frame_data_location == crate::FrameDataLocation::Hardware
        && color_params.is_unknown
    {
        return;
    }
    let filter = colorspace_filter(
        target_state,
        &color_params,
        request.desired.pixel_format(),
        false,
        acceleration == HardwareAccelerationKind::Qsv,
    );
    if filter.trim().is_empty() {
        return;
    }

    filter_chain.pixel_format.push(filter);
    if !color_params.is_unknown {
        target_state.frame_data_location = crate::FrameDataLocation::Software;
        target_state.pixel_format = request.desired.pixel_format();
        target_state.colors_are_bt709 = true;
        *emit_pixel_format_output_option =
            should_emit_pixel_format_output_option(request, acceleration, target_state);
        if acceleration == HardwareAccelerationKind::Vaapi {
            upload_software_frames_for_hardware_encoder_filters(
                acceleration,
                target_state,
                &mut filter_chain.pixel_format,
            );
        }
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
    upload_software_frames_for_hardware_encoder_filters(
        acceleration,
        target_state,
        &mut filter_chain.video,
    );
}

fn upload_software_frames_for_hardware_encoder_filters(
    acceleration: HardwareAccelerationKind,
    target_state: &mut FrameState,
    filters: &mut Vec<String>,
) {
    if target_state.frame_data_location != crate::FrameDataLocation::Software {
        return;
    }

    match acceleration {
        HardwareAccelerationKind::Nvenc => {
            filters.push(hardware_upload_filter(
                acceleration,
                QSV_EXTRA_HARDWARE_FRAMES,
            ));
            let surface_format = cuda_surface_pixel_format_for_software_upload(target_state);
            filters.push(cuda_format_filter(surface_format));
            target_state.frame_data_location = crate::FrameDataLocation::Hardware;
            target_state.pixel_format = surface_format;
        }
        HardwareAccelerationKind::Qsv => {
            filters.push(format!("format={}", PixelFormat::Nv12.ffmpeg_name()));
            filters.push(hardware_upload_filter(
                acceleration,
                QSV_EXTRA_HARDWARE_FRAMES,
            ));
            target_state.frame_data_location = crate::FrameDataLocation::Hardware;
            target_state.pixel_format = request_qsv_surface_pixel_format(target_state);
        }
        HardwareAccelerationKind::Vaapi => {
            filters.push(hardware_upload_filter(
                acceleration,
                QSV_EXTRA_HARDWARE_FRAMES,
            ));
            target_state.frame_data_location = crate::FrameDataLocation::Hardware;
            target_state.pixel_format = PixelFormat::Vaapi;
        }
        HardwareAccelerationKind::None
        | HardwareAccelerationKind::Amf
        | HardwareAccelerationKind::VideoToolbox
        | HardwareAccelerationKind::V4l2m2m
        | HardwareAccelerationKind::Rkmpp => {}
    }
}

const fn cuda_surface_pixel_format_for_software_upload(target_state: &FrameState) -> PixelFormat {
    match target_state.pixel_format {
        PixelFormat::Yuv420P10Le => PixelFormat::Cuda,
        _ => PixelFormat::Nv12,
    }
}

const fn request_qsv_surface_pixel_format(target_state: &FrameState) -> PixelFormat {
    match target_state.pixel_format {
        PixelFormat::Yuv420P10Le => PixelFormat::Yuv420P10Le,
        _ => PixelFormat::Nv12,
    }
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

fn download_hardware_frames_for_drawtext(
    has_drawtext: bool,
    desired: &DesiredVideoState,
    target_state: &mut FrameState,
    filter_chain: &mut FilterChain,
) {
    if !has_drawtext || target_state.frame_data_location != crate::FrameDataLocation::Hardware {
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

    if request.state.retained_stream_timestamps {
        return;
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
    // Profile 5 admission has already positively proved its exact operation.
    // It must not inherit the unrelated NvencCudaScale gate.
    if acceleration == HardwareAccelerationKind::Nvenc && profile_5_operation(request).is_some() {
        return HardwareAccelerationKind::Nvenc;
    }
    // ADR 0064: hardware acceleration may change performance, not colour
    // semantics. A backend with no proven HDR10 graph would otherwise emit SDR
    // from an HDR10 profile, which is the silent downgrade the ADR forbids, so
    // demote to software encoding instead. Reported by the `Output Dynamic
    // Range` health check so the cost is visible rather than mysterious.
    if request.desired.produces_hdr10()
        && hdr10_output_support(acceleration) == Hdr10OutputSupport::Unproven
    {
        return HardwareAccelerationKind::None;
    }

    // ADR 0014: a denied or unproven CUDA graph has no further hardware
    // fallback, so NVENC must demote to software rather than emit CUDA
    // upload/scale/encode arguments that are known not to run.
    if acceleration == HardwareAccelerationKind::Nvenc
        && request.desired.video_format != FFmpegProfileVideoFormat::Copy
        && nvenc_cuda_graph_unusable(request)
    {
        return HardwareAccelerationKind::None;
    }

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
    if requested_acceleration == HardwareAccelerationKind::Nvenc
        && profile_5_operation(request).is_some()
    {
        return HardwareAccelerationKind::Nvenc;
    }
    if profile_7_direct_base_layer_decode(request) {
        return HardwareAccelerationKind::None;
    }
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
            .start
            .is_none_or(|position| position.is_zero())
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

    let decode_acceleration = if acceleration == HardwareAccelerationKind::Nvenc
        && profile_5_operation(request).is_some()
    {
        HardwareAccelerationKind::Nvenc
    } else if profile_7_direct_base_layer_decode(request)
        || qsv_multiple_profile_decode_workaround(request, acceleration, source)
    {
        HardwareAccelerationKind::None
    } else if let Some(report) = &request.runtime_capabilities {
        let capabilities = HardwareCapabilitiesFactory::new(report).for_acceleration(acceleration);
        if capabilities.can_decode(source) == FfmpegCapability::Hardware {
            capabilities.acceleration()
        } else {
            HardwareAccelerationKind::None
        }
    } else {
        acceleration
    };

    let decoder = if decode_acceleration == HardwareAccelerationKind::None {
        if acceleration == HardwareAccelerationKind::Nvenc {
            select_forced_software_video_decoder(
                source.codec,
                request.runtime_capabilities.as_ref(),
            )
        } else {
            select_software_video_decoder(source.codec, request.runtime_capabilities.as_ref())
        }
    } else {
        select_video_decoder(decode_acceleration, source.codec)
    };

    if let Some(decoder) = decoder {
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

/// Attach the HDR10 static-metadata options to the video input.
///
/// ADR 0064 places `-mastering_display` and `-content_light` at the per-stream
/// input-option boundary rather than among the generic output options, so they
/// describe the decoded source stream that the filter graph then transforms.
fn add_hdr10_static_metadata_input_options(request: &mut PipelineRequest) {
    let options = hdr10_static_metadata_input_options(request);
    if options.is_empty() {
        return;
    }

    for input in request
        .inputs
        .iter_mut()
        .filter(|input| input.kind == InputKind::Video)
    {
        input.input_options.extend(options.iter().cloned());
    }
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
            .start
            .is_some_and(|position| !position.is_zero())
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
        let paced_media = match input.kind {
            InputKind::Video => !source_is_still_image && input.allow_readrate,
            InputKind::Audio => input.allow_readrate,
            _ => false,
        };
        if paced_media {
            if request.state.retained_stream_timestamps {
                // ADR 0087: let both logical legs establish output reserve before
                // normal pacing. This input burst alone does not prove readiness.
                input
                    .input_options
                    .extend(["-readrate_initial_burst".to_owned(), "8".to_owned()]);
            }
            input.add_readrate(readrate);
        }
    }
}

fn add_cuda_software_fallback_upload(
    request: &mut PipelineRequest,
    acceleration: HardwareAccelerationKind,
) -> bool {
    // Still-image sources (song/error/slug backgrounds) stay in software:
    // their chain starts with software-only filters (`loop`, subtitle burn),
    // so an upfront hwupload would feed CUDA frames into filters that cannot
    // accept them. nvenc takes the software frames directly at encode time.
    let is_still_image = request
        .source_video_stream
        .as_ref()
        .is_some_and(|source| source.is_still_image);
    if acceleration != HardwareAccelerationKind::Nvenc
        || request.desired.video_format == FFmpegProfileVideoFormat::Copy
        || request.source_video_stream.is_none()
        || is_still_image
        || profile_5_operation(request).is_some()
        || nvidia_vulkan_hdr_tonemap(request)
        // Both NVENC HDR tone-map branches end in hardware frames on their own:
        // the libplacebo branch uploads itself (handled above), and the
        // Vulkan-free software fallback ends in `hwupload_cuda`. Uploading here
        // as well would emit `hwupload,hwdownload,...,hwupload_cuda` — a
        // pointless round trip through GPU memory before a software tonemap.
        || nvenc_software_hdr_tonemap(request)
    {
        return false;
    }

    if let Some(input) = request
        .inputs
        .iter_mut()
        .find(|input| input.kind == InputKind::Video)
    {
        input
            .filter_steps
            .push(cuda_software_fallback_upload_filter());
        return true;
    }

    false
}

/// True when the NVENC chain converts colour in software and supplies its own
/// `hwupload_cuda`, so the generic CUDA software-fallback upload must not be
/// added in front of it.
///
/// Two chains qualify:
///
/// * the Vulkan-free HDR-to-SDR tone-map fallback, and
/// * an HDR10 profile converting an HLG or SDR source.
///
/// A PQ source under an HDR10 profile does *not* qualify: it only gets
/// `setparams`, which works on CUDA surfaces, so the upload is wanted and the
/// whole graph stays on the GPU.
fn nvenc_software_hdr_tonemap(request: &PipelineRequest) -> bool {
    if request.desired.video_format == FFmpegProfileVideoFormat::Copy {
        return false;
    }

    if request.desired.produces_hdr10() {
        return source_transfer_kind(request) != SourceTransferKind::Pq;
    }

    request
        .source_video_stream
        .as_ref()
        .is_some_and(|source| source.color_params.is_hdr || source.color_params.is_bt2020_ten)
        && !nvidia_vulkan_hdr_tonemap(request)
}

/// Select the Vulkan/libplacebo NVENC HDR graph only when runtime evidence
/// says the *whole* graph works (ADR 0014, milestone `p8b-01`).
///
/// The advertised `vulkan` hwaccel is necessary but nowhere near sufficient. On
/// a WSL2 host with an RTX 3080 Ti, `ffmpeg -hwaccels` lists `vulkan` and the
/// Vulkan device initializes — but it is `llvmpipe`, a software rasterizer, so
/// `hwmap=derive_device=cuda` fails and every HDR NVENC transcode dies. The
/// operation-scoped denial learned by the functional probe is what separates
/// "advertised" from "works here".
fn nvidia_vulkan_hdr_tonemap(request: &PipelineRequest) -> bool {
    if profile_5_operation(request).is_some() {
        return false;
    }
    if request.desired.vulkan_hdr_tonemap_mode != VulkanHdrTonemapMode::Auto {
        return false;
    }
    if !request
        .source_video_stream
        .as_ref()
        .is_some_and(|source| source.color_params.is_hdr)
    {
        return false;
    }

    request.runtime_capabilities.as_ref().is_some_and(|report| {
        let device = report.hardware_graph_device();
        if report.hardware_graph_denied(HardwareGraphOperation::NvencHdrVulkanLibplacebo, &device) {
            return false;
        }
        // A runtime that has probed must have *positively* proved this graph.
        // A runtime that has never probed at all (no proof and no denial
        // recorded for any operation) keeps the pre-probe behavior of trusting
        // the advertised hwaccel, so an older database is not silently demoted.
        if report.has_probed_hardware_graphs() {
            return report
                .hardware_graph_proven(HardwareGraphOperation::NvencHdrVulkanLibplacebo, &device);
        }
        report.has_hwaccel("vulkan")
    })
}

/// True when NVENC's CUDA scale/encode graph is known not to work.
///
/// `NvencCudaScale` has no further hardware fallback, so a denial or an
/// unproven result after probing means the whole NVENC path must demote to
/// software encoding rather than emit CUDA arguments that cannot run.
fn nvenc_cuda_graph_unusable(request: &PipelineRequest) -> bool {
    request.runtime_capabilities.as_ref().is_some_and(|report| {
        let device = report.hardware_graph_device();
        if report.hardware_graph_denied(HardwareGraphOperation::NvencCudaScale, &device) {
            return true;
        }
        report.has_probed_hardware_graphs()
            && !report.hardware_graph_proven(HardwareGraphOperation::NvencCudaScale, &device)
    })
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

    if acceleration == HardwareAccelerationKind::Nvenc {
        if let Some(tune) = &desired.video_tune {
            if !tune.trim().is_empty() {
                extra.extend(["-tune:v".to_owned(), tune.clone()]);
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

    // On the NVENC path the frames reaching the encoder are a CUDA hardware
    // surface whose pixel format was already normalized on the GPU
    // (`scale_cuda=format=nv12`; `overlay_cuda` preserves that surface format).
    // A software `-pix_fmt yuv420p` cannot be applied to a CUDA surface: ffmpeg
    // inserts an `auto_scale` to bridge the formats, which fails with
    // "Impossible to convert between the formats supported by the filter
    // 'overlay_cuda' and the filter 'auto_scale'" and aborts the encode (the
    // encoder receives no frames). nvenc encodes the nv12/p010le surface
    // directly, so do not emit the option while the frames remain on the GPU.
    // Mirrors ErsatzTV `NvidiaPipelineBuilder.SetPixelFormat`, which keeps the
    // format on the GPU for hardware frames instead of adding a software
    // pixel-format output option. The software-overlay path (e.g. text/image
    // subtitle burn) downloads frames first, so `target_state` is `Software`
    // there and the option is still emitted.
    if acceleration == HardwareAccelerationKind::Nvenc
        && target_state.frame_data_location == crate::FrameDataLocation::Hardware
    {
        return false;
    }

    let Some(source) = request.source_video_stream.as_ref() else {
        return false;
    };
    // Still images decode to RGB-family frames the pixel-format enum cannot
    // represent, so the state comparison below misses them; without the
    // output option libx264 rejects the frames ("high profile doesn't
    // support 4:4:4") and nvenc encodes the wrong format.
    if source.is_still_image
        && target_state.frame_data_location == crate::FrameDataLocation::Software
    {
        return true;
    }
    target_state.pixel_format != request.desired.pixel_format()
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
            return configure_nvenc_hardware_acceleration(
                request,
                steps,
                decode_acceleration,
                state,
            );
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

fn configure_nvenc_hardware_acceleration(
    request: &PipelineRequest,
    steps: &mut Vec<PipelineStep>,
    decode_acceleration: HardwareAccelerationKind,
    mut state: FrameState,
) -> FrameState {
    let profile_5 = profile_5_operation(request).is_some();
    let vulkan_hdr = nvidia_vulkan_hdr_tonemap(request);
    if profile_5 {
        steps.push(PipelineStep::global(
            "cuda_hw_device",
            crate::NVENC_PROFILE5_DEVICE_PREFIX[0..2].iter().copied(),
        ));
        steps.push(PipelineStep::global(
            "vulkan_hw_device",
            crate::NVENC_PROFILE5_DEVICE_PREFIX[2..4].iter().copied(),
        ));
        steps.push(PipelineStep::global(
            "vulkan_filter_hw_device",
            crate::NVENC_PROFILE5_DEVICE_PREFIX[4..6].iter().copied(),
        ));
    } else if vulkan_hdr {
        // Shared with the functional probe so the two cannot drift: a probe
        // that opened another device would answer for a graph playback never
        // runs.
        steps.push(PipelineStep::global(
            "cuda_hw_device",
            NVENC_HDR_DEVICE_PREFIX[0..2].iter().copied(),
        ));
        steps.push(PipelineStep::global(
            "vulkan_hw_device",
            NVENC_HDR_DEVICE_PREFIX[2..4].iter().copied(),
        ));
    } else {
        steps.push(PipelineStep::global(
            "cuda_hw_device",
            ["-init_hw_device", "cuda=nv:0"],
        ));
    }

    if decode_acceleration != HardwareAccelerationKind::Nvenc {
        return state;
    }
    let (hwaccel_step, output_format_step, hwaccel) = if profile_5 {
        ("cuda_hwaccel", "cuda_hwaccel_output_format", "cuda")
    } else if vulkan_hdr {
        ("vulkan_hwaccel", "vulkan_hwaccel_output_format", "vulkan")
    } else {
        ("cuda_hwaccel", "cuda_hwaccel_output_format", "cuda")
    };
    steps.push(PipelineStep::global(hwaccel_step, ["-hwaccel", hwaccel]));
    steps.push(PipelineStep::global(
        output_format_step,
        ["-hwaccel_output_format", hwaccel],
    ));
    steps.push(no_autoscale());

    state.frame_data_location = crate::FrameDataLocation::Hardware;
    state.pixel_format = if request.desired.bit_depth == FFmpegProfileBitDepth::EightBit {
        PixelFormat::Nv12
    } else {
        PixelFormat::Cuda
    };
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
            // `scale_cuda` only accepts CUDA surfaces. Software frames on the
            // nvenc path (still-image pipelines that skip the upload
            // fallback) scale with the software filter; nvenc accepts the
            // software frames at encode time.
            if current.frame_data_location == crate::FrameDataLocation::Hardware {
                scale_cuda_filter(current, scaled_size, padded_size, bit_depth)
            } else {
                crate::scale_filter(current, scaled_size, padded_size)
            }
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

    if current.is_anamorphic {
        // `scaled_size` already incorporates the source sample-aspect ratio.
        // Scaling there in one pass avoids an FFmpeg 9 `scale_cuda` defect
        // where a shrinking `iw*sar` intermediate can silently become a
        // solid-green frame before the second scale runs.
        return format!(
            "scale_cuda={}:{}:format={format},setsar=1",
            scaled_size.width, scaled_size.height
        );
    }

    let aspect_ratio = if scaled_size == padded_size {
        ",setsar=1"
    } else if current.cropped_size.is_some() {
        ":force_original_aspect_ratio=increase,setsar=1"
    } else {
        ":force_original_aspect_ratio=decrease,setsar=1"
    };

    format!(
        "scale_cuda={}:{}:format={format}{}",
        padded_size.width, padded_size.height, aspect_ratio
    )
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

const fn cuda_surface_pixel_format(bit_depth: FFmpegProfileBitDepth) -> PixelFormat {
    match bit_depth {
        FFmpegProfileBitDepth::EightBit => PixelFormat::Nv12,
        FFmpegProfileBitDepth::TenBit => PixelFormat::Cuda,
    }
}

fn audio_encoder(
    desired: &DesiredVideoState,
    source_audio_stream: Option<&SourceAudioStream>,
    source_metadata_is_runtime_only: bool,
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
            emit_audio_channels(
                desired.audio_format,
                source_audio_stream,
                *channels,
                source_metadata_is_runtime_only,
            )
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
    source_metadata_is_runtime_only: bool,
) -> bool {
    let Some(source_channels) = source_audio_stream.and_then(|stream| stream.channels) else {
        // Add-on-backed Remote Streams arrive through stdin and intentionally
        // skip eager probing. The explicit output profile remains authoritative
        // for that bounded case; ordinary unprobed files keep legacy behavior.
        return source_metadata_is_runtime_only;
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

fn mpegts_muxing_options(desired: &DesiredVideoState) -> MpegTsMuxingOptions {
    MpegTsMuxingOptions::from_bitrates(desired.video_bitrate, desired.audio_bitrate)
}

fn frame_rate_output_option(frame_rate: FrameRate) -> PipelineStep {
    PipelineStep::generic(
        "frame_rate_output",
        [
            "-r".to_owned(),
            frame_rate.r_frame_rate(),
            "-fps_mode:v".to_owned(),
            "cfr".to_owned(),
        ],
    )
}

fn normalized_video_preset(value: Option<&str>) -> Option<String> {
    match value.map(str::trim).filter(|value| !value.is_empty())? {
        value if value.eq_ignore_ascii_case("llhp") => Some("p1".to_owned()),
        value if value.eq_ignore_ascii_case("llhq") => Some("p7".to_owned()),
        value => Some(value.to_owned()),
    }
}

fn normalized_video_tune(legacy_preset: Option<&str>, tune: Option<&str>) -> Option<String> {
    non_empty_string(tune).or_else(|| {
        legacy_preset
            .is_some_and(|value| {
                value.trim().eq_ignore_ascii_case("llhp")
                    || value.trim().eq_ignore_ascii_case("llhq")
            })
            .then(|| "ll".to_owned())
    })
}

#[allow(clippy::too_many_lines)] // Keep ordered output options together across timestamp modes.
fn add_output_options(
    steps: &mut Vec<PipelineStep>,
    state: &FfmpegState,
    desired: &DesiredVideoState,
    acceleration: HardwareAccelerationKind,
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
        if state.retained_stream_timestamps {
            // Fractional native section boundaries can quantize to the same
            // encoder tick. Let FFmpeg resolve video collisions without
            // resetting the edited clock or changing audio timing (ADR 0088).
            steps.push(PipelineStep::generic(
                "retained_stream_frame_rate",
                ["-fps_mode:v", "vfr"],
            ));
        } else if acceleration != HardwareAccelerationKind::Qsv {
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

    if state.output_format == OutputFormat::MpegTs {
        let muxing = mpegts_muxing_options(desired);
        steps.push(mpegts_muxrate(muxing));
        steps.push(mpegts_pcr_period(muxing));
    }

    if let Some(step) = output_ts_offset_option(state, desired) {
        steps.push(step);
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

    if desired.audio_format == FFmpegProfileAudioFormat::Ac3
        && desired.audio_channels.is_some_and(|channels| channels > 2)
    {
        options.extend(["-max_delay".to_owned(), "500000".to_owned()]);
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

    let segment_options = match format {
        OutputFormat::Hls => {
            Some(mpegts_muxing_options(desired).hls_segment_options(hls.segment_options.as_deref()))
        }
        OutputFormat::HlsMp4 => hls.segment_options.clone(),
        OutputFormat::MpegTs | OutputFormat::Mp4 | OutputFormat::Mkv | OutputFormat::Nut => None,
    };

    if let Some(segment_options) = &segment_options {
        options.extend(["-hls_segment_options".to_owned(), segment_options.clone()]);
    }

    let pdt = if hls.troubleshooting {
        ""
    } else {
        "program_date_time+omit_endlist+"
    };
    let discont = if hls.discontinuity {
        "+discont_start"
    } else {
        ""
    };
    options.extend([
        "-hls_flags".to_owned(),
        format!("{pdt}append_list{discont}+independent_segments"),
        hls.playlist_path.clone(),
    ]);

    PipelineStep::generic("hls", options)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn retained_stream_request() -> PipelineRequest {
        let mut desired = DesiredVideoState::h264_1080p();
        desired.frame_rate = Some(FrameRate::DEFAULT);
        desired.realtime_mode = RealtimeMode::On;
        let mut request = PipelineRequest::new(
            InputFile::video("pipe:0").with_input_options(["-f", "nut"]),
            desired,
        );
        request.inputs[1] = InputFile::audio("pipe:0").with_stream_index(1);
        request.state.start = Some(Duration::from_secs(90));
        request.state.output_start = Some(Duration::from_secs(90));
        request.with_retained_stream_input()
    }

    // ADR 0087: yt-dlp owns source cuts; the host consumes one edited NUT clock.
    #[test]
    fn retained_stream_input_snapshot() {
        let pipeline = build_pipeline_for_acceleration(
            HardwareAccelerationKind::None,
            retained_stream_request(),
        );
        let command = pipeline.command(&[]);
        assert!(command.arguments.iter().any(|arg| arg == "-xerror"));
        assert!(!command
            .arguments
            .iter()
            .any(|arg| arg == "-ss" || arg == "-r"));
        assert_eq!(
            command.arguments.iter().filter(|arg| *arg == "-i").count(),
            1
        );
        let graph = pipeline.filter_graph.as_deref().unwrap_or_default();
        assert!(!graph.contains("setpts="), "{graph}");
        assert!(!graph.contains("select="), "{graph}");
        assert!(!command
            .arguments
            .iter()
            .any(|arg| arg == "concat" || arg == "-segment_time_metadata"));
        // Keep the timing/input projection in generated final argv order.
        let mut selected = Vec::new();
        let mut arguments = command.arguments.iter();
        while let Some(argument) = arguments.next() {
            if matches!(argument.as_str(), "-copyts" | "-xerror") {
                selected.push(argument.clone());
            } else if matches!(
                argument.as_str(),
                "-i" | "-fps_mode:v" | "-readrate_initial_burst" | "-readrate"
            ) {
                selected.push(format!(
                    "{argument} {}",
                    arguments.next().expect("option value")
                ));
            }
        }
        insta::assert_snapshot!(selected.join("\n"), @"
        -copyts
        -xerror
        -readrate_initial_burst 8
        -readrate_initial_burst 8
        -readrate 1.05
        -i pipe:0
        -fps_mode:v vfr
        ");
    }

    #[test]
    fn retained_stream_error_policy_does_not_change_ordinary_inputs() {
        let request = PipelineRequest::new(
            InputFile::video("ordinary.mp4"),
            DesiredVideoState::h264_1080p(),
        );
        let command =
            build_pipeline_for_acceleration(HardwareAccelerationKind::None, request).command(&[]);
        assert!(!command
            .arguments
            .iter()
            .any(|arg| arg == "-xerror" || arg == "-copyts"));
    }

    #[test]
    fn retained_stream_keeps_caller_selected_fresh_indexes() {
        let mut request = retained_stream_request();
        request.inputs[0] = request.inputs[0].clone().with_stream_index(2);
        request.inputs[1] = request.inputs[1].clone().with_stream_index(3);
        let inputs = request.inputs.clone();
        let mut video = SourceVideoStream::new(SourceVideoCodec::H264);
        video.index = 4;
        request.source_video_stream = Some(video);
        request.source_audio_streams =
            vec![SourceAudioStream::new(SourceAudioCodec::Aac).with_index(5)];
        apply_source_stream_indexes(&mut request);
        assert_eq!(request.inputs, inputs);
    }

    #[test]
    fn retained_stream_promotes_copy_preserves_other_inputs_and_disabled_audio() {
        let mut request = retained_stream_request().without_audio();
        request.desired.video_format = FFmpegProfileVideoFormat::Copy;
        request.desired.audio_format = FFmpegProfileAudioFormat::Copy;
        request.desired.bit_depth = FFmpegProfileBitDepth::TenBit;
        let subtitle = InputFile::new(InputKind::Subtitle, "captions.srt", 0);
        request.inputs.push(subtitle);
        let inputs = request.inputs.clone();
        let request = request.with_retained_stream_input();
        assert_eq!(request.inputs, inputs);
        assert_eq!(request.desired.video_format, FFmpegProfileVideoFormat::H264);
        assert_eq!(request.desired.audio_format, FFmpegProfileAudioFormat::Aac);
        assert_eq!(request.desired.bit_depth, FFmpegProfileBitDepth::TenBit);
        assert!(!request
            .inputs
            .iter()
            .any(|input| input.kind == InputKind::Audio));
        assert_eq!(request.state.start, None);
        assert_eq!(request.state.output_start, None);
    }

    #[test]
    fn retained_stream_qsv_preserves_download_without_video_clock_reset() {
        for retained in [false, true] {
            let mut request = retained_stream_request();
            request.state.retained_stream_timestamps = retained;
            let mut frame = FrameState::new(FrameSize::new(1920, 1080));
            frame.frame_data_location = crate::FrameDataLocation::Hardware;
            let mut chain = FilterChain::default();
            apply_qsv_download_and_reset_pts(
                &request,
                HardwareAccelerationKind::Qsv,
                &mut frame,
                &mut chain,
            );
            assert_eq!(
                frame.frame_data_location,
                crate::FrameDataLocation::Software
            );
            assert!(chain
                .video
                .iter()
                .any(|filter| filter.contains("hwdownload")));
            assert_eq!(
                chain.video.iter().any(|filter| filter.contains("setpts=")),
                !retained
            );
        }
        assert!(!FfmpegState::default().retained_stream_timestamps);
    }

    fn hls_options_for(
        audio_format: FFmpegProfileAudioFormat,
        audio_channels: Option<u32>,
    ) -> Vec<String> {
        let mut desired = DesiredVideoState::h264_1080p();
        desired.audio_format = audio_format;
        desired.audio_channels = audio_channels;
        hls_output(
            OutputFormat::Hls,
            &HlsOutput {
                playlist_path: "playlist.m3u8".to_owned(),
                segment_template: "segment-%05d.ts".to_owned(),
                init_template: None,
                segment_options: None,
                is_first_transcode: true,
                discontinuity: false,
                troubleshooting: false,
            },
            &desired,
            HardwareAccelerationKind::None,
        )
        .output_options()
        .to_vec()
    }

    #[test]
    fn hls_output_adds_max_delay_for_multichannel_ac3() {
        let options = hls_options_for(FFmpegProfileAudioFormat::Ac3, Some(6));
        let rendered = options.join(" ");

        assert!(rendered.contains("-max_delay 500000 -f hls"), "{rendered}");
    }

    #[test]
    fn hls_output_does_not_add_max_delay_for_stereo_or_non_ac3_audio() {
        let stereo_ac3 = hls_options_for(FFmpegProfileAudioFormat::Ac3, Some(2)).join(" ");
        let multichannel_aac = hls_options_for(FFmpegProfileAudioFormat::Aac, Some(6)).join(" ");

        assert!(!stereo_ac3.contains("-max_delay"), "{stereo_ac3}");
        assert!(
            !multichannel_aac.contains("-max_delay"),
            "{multichannel_aac}"
        );
    }
}
