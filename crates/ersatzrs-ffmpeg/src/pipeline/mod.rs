use ersatzrs_domain::{
    FFmpegProfileAudioFormat, FFmpegProfileBitDepth, FFmpegProfileVideoFormat,
    HardwareAccelerationKind, VideoScanKind,
};

use crate::{
    closed_gop, fast_start, hide_banner, loglevel_error, mpegts_output, no_autoscale, no_b_frames,
    no_standard_input, no_stats, pipe_output, progress_to_stdout, select_video_decoder,
    standard_format_flags, subtitle_overlay_filter, watermark_opacity_filter,
    watermark_overlay_filter, CommandGenerator, ComplexFilter, FfmpegCommand, FilterChain,
    FrameRate, FrameSize, FrameState, InputFile, InputKind, PipelineStep, PixelFormat,
    SourceVideoCodec, StreamKind, WatermarkOverlay,
};

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
    pub watermark: Option<WatermarkOverlay>,
    pub drawvg_filters: Vec<String>,
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
            watermark: None,
            drawvg_filters: Vec::new(),
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
        }
    }
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesiredVideoState {
    pub frame_size: FrameSize,
    pub frame_rate: Option<FrameRate>,
    pub video_format: FFmpegProfileVideoFormat,
    pub audio_format: FFmpegProfileAudioFormat,
    pub bit_depth: FFmpegProfileBitDepth,
    pub video_profile: Option<String>,
    pub video_preset: Option<String>,
    pub video_bitrate: Option<u32>,
    pub video_buffer_size: Option<u32>,
    pub audio_bitrate: Option<u32>,
    pub audio_channels: Option<u32>,
    pub audio_sample_rate: Option<u32>,
    pub allow_b_frames: bool,
}

impl DesiredVideoState {
    #[must_use]
    pub fn h264_1080p() -> Self {
        Self {
            frame_size: FrameSize::new(1920, 1080),
            frame_rate: Some(FrameRate::DEFAULT),
            video_format: FFmpegProfileVideoFormat::H264,
            audio_format: FFmpegProfileAudioFormat::Aac,
            bit_depth: FFmpegProfileBitDepth::EightBit,
            video_profile: Some("high".to_owned()),
            video_preset: Some("veryfast".to_owned()),
            video_bitrate: Some(8_000),
            video_buffer_size: Some(16_000),
            audio_bitrate: Some(192),
            audio_channels: Some(2),
            audio_sample_rate: Some(48_000),
            allow_b_frames: false,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FFmpegPipeline {
    pub steps: Vec<PipelineStep>,
    pub is_intel_vaapi_or_qsv: bool,
    pub filter_graph: Option<String>,
}

impl FFmpegPipeline {
    #[must_use]
    pub fn command(&self, inputs: &[InputFile]) -> FfmpegCommand {
        CommandGenerator::generate_command(inputs, &self.steps, self.is_intel_vaapi_or_qsv)
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

fn build_transcode_pipeline(
    mut request: PipelineRequest,
    acceleration: HardwareAccelerationKind,
) -> FFmpegPipeline {
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
    let target_state = configure_hardware_acceleration(&mut request, &mut steps, acceleration);
    add_video_decoder(&request, &mut steps, acceleration);
    filter_chain.video.push(video_scale_filter(
        acceleration,
        &target_state,
        request.desired.frame_size,
        request.desired.frame_size,
        request.desired.bit_depth,
    ));

    if let Some(watermark) = &request.watermark {
        if let Some(input) = request
            .inputs
            .iter_mut()
            .find(|input| input.kind == InputKind::Watermark)
        {
            if watermark.opacity < 100 {
                input
                    .filter_steps
                    .push(watermark_opacity_filter(watermark.opacity));
            }
        }
        filter_chain
            .watermark_overlay
            .push(watermark_overlay_filter(
                watermark,
                request.desired.frame_size,
                request.desired.frame_size,
                request.desired.pixel_format(),
            ));
    }

    if request
        .inputs
        .iter()
        .any(|input| input.kind == InputKind::Subtitle)
    {
        filter_chain
            .subtitle_overlay
            .push(subtitle_overlay_filter(request.desired.pixel_format()));
    }

    filter_chain.graphics_overlay.extend(request.drawvg_filters);
    if !filter_chain.graphics_overlay.is_empty() {
        request.inputs.push(
            InputFile::graphics("color=c=black@0.0:s=16x16:d=86400")
                .with_input_options(["-f", "lavfi"]),
        );
    }

    let is_intel = matches!(
        acceleration,
        HardwareAccelerationKind::Vaapi | HardwareAccelerationKind::Qsv
    );
    let complex = ComplexFilter::build(&request.inputs, filter_chain, is_intel);
    let filter_graph = complex.filter_graph.clone();

    steps.push(video_encoder(acceleration, &request.desired));
    steps.push(PipelineStep::complex_filter(complex.options));
    steps.push(audio_encoder(&request.desired));

    add_output_options(&mut steps, &request.state, &request.desired, acceleration);

    FFmpegPipeline {
        steps,
        is_intel_vaapi_or_qsv: is_intel,
        filter_graph,
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

    if let Some(decoder) = select_video_decoder(acceleration, source.codec) {
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
    let name = match (acceleration, desired.video_format) {
        (_, FFmpegProfileVideoFormat::Copy) => "copy",
        (HardwareAccelerationKind::Qsv, FFmpegProfileVideoFormat::H264) => "h264_qsv",
        (HardwareAccelerationKind::Qsv, FFmpegProfileVideoFormat::Hevc) => "hevc_qsv",
        (HardwareAccelerationKind::Qsv, FFmpegProfileVideoFormat::Mpeg2Video) => "mpeg2_qsv",
        (HardwareAccelerationKind::Qsv, FFmpegProfileVideoFormat::Av1) => "av1_qsv",
        (HardwareAccelerationKind::Nvenc, FFmpegProfileVideoFormat::H264) => "h264_nvenc",
        (HardwareAccelerationKind::Nvenc, FFmpegProfileVideoFormat::Hevc) => "hevc_nvenc",
        (HardwareAccelerationKind::Nvenc, FFmpegProfileVideoFormat::Av1) => "av1_nvenc",
        (HardwareAccelerationKind::Amf, FFmpegProfileVideoFormat::H264) => "h264_amf",
        (HardwareAccelerationKind::Amf, FFmpegProfileVideoFormat::Hevc) => "hevc_amf",
        (HardwareAccelerationKind::Vaapi, FFmpegProfileVideoFormat::H264) => "h264_vaapi",
        (HardwareAccelerationKind::Vaapi, FFmpegProfileVideoFormat::Hevc) => "hevc_vaapi",
        (HardwareAccelerationKind::Vaapi, FFmpegProfileVideoFormat::Mpeg2Video) => "mpeg2_vaapi",
        (HardwareAccelerationKind::VideoToolbox, FFmpegProfileVideoFormat::H264) => {
            "h264_videotoolbox"
        }
        (HardwareAccelerationKind::VideoToolbox, FFmpegProfileVideoFormat::Hevc) => {
            "hevc_videotoolbox"
        }
        (HardwareAccelerationKind::V4l2m2m, FFmpegProfileVideoFormat::H264) => "h264_v4l2m2m",
        (HardwareAccelerationKind::V4l2m2m, FFmpegProfileVideoFormat::Hevc) => "hevc_v4l2m2m",
        (HardwareAccelerationKind::Rkmpp, FFmpegProfileVideoFormat::H264) => "h264_rkmpp",
        (HardwareAccelerationKind::Rkmpp, FFmpegProfileVideoFormat::Hevc) => "hevc_rkmpp",
        (_, FFmpegProfileVideoFormat::Hevc) => "libx265",
        (_, FFmpegProfileVideoFormat::Mpeg2Video) => "mpeg2video",
        (_, FFmpegProfileVideoFormat::Av1) => "libsvtav1",
        (_, FFmpegProfileVideoFormat::H264 | FFmpegProfileVideoFormat::None) => "libx264",
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
    let mut state = FrameState::new(request.desired.frame_size);

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
