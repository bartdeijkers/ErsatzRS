use std::collections::BTreeMap;
use std::time::Duration;

use ersatzrs_domain::{
    FFmpegProfile, FFmpegProfileAudioFormat, FFmpegProfileBitDepth, FFmpegProfileTonemapAlgorithm,
    FFmpegProfileVideoFormat, FilterMode, GraphicsElement, GraphicsElementConfig,
    HardwareAccelerationKind, ImageGraphicsConfig, MotionEndBehavior, MotionGraphicsConfig,
    NormalizeLoudnessMode, OutputDynamicRange, Resolution, ScalingBehavior, ScriptGraphicsConfig,
    StreamingMode, StyleDefinition, SubtitleGraphicsConfig, TextAlignment, TextFit,
    TextGraphicsConfig, VaapiDriver, VideoScanKind, WatermarkLocation, WatermarkSize,
};
use ersatzrs_ffmpeg::{
    audio_pad_filter, audio_resample_filter, capability_report_for_tests, colorspace_filter,
    cuda_format_filter, cuda_software_fallback_upload_filter, deinterlace_filter,
    deinterlace_qsv_filter, deinterlace_vaapi_filter, frame_rate_filter, hardware_download_filter,
    hardware_upload_filter, lower_third_overlay, normalize_loudness_filter, pad_vaapi_filter,
    song_progress_filter, subtitle_graphics_lavfi_input, subtitle_scale_npp_filter,
    tonemap_cuda_filter, tonemap_filter, tonemap_qsv_filter, tonemap_vaapi_filter,
    translate_watermark_opacity_expression, translate_watermark_opacity_expression_with_clock,
    yadif_cuda_filter, yadif_filter, AmfPipelineBuilder, AudioFilter, ColorParams, ComplexFilter,
    ConcatPlaylist, DesiredVideoState, DrawvgOverlay, FilterChain, FrameDataLocation, FrameRate,
    FrameSize, FrameState, GraphFailureKind, GraphicsOverlayFilter, GraphicsPipelineOverlay,
    GraphicsRenderContext, GraphicsTemplateRenderer, HardwareGraphDenial, HardwareGraphOperation,
    HlsOutput, InputFile, NvencPipelineBuilder, OutputFormat, OutputMetadata, PipelineBuilder,
    PipelineRequest, PixelFormat, QsvPipelineBuilder, RealtimeMode, RkmppPipelineBuilder,
    SoftwarePipelineBuilder, SourceAudioCodec, SourceAudioStream, SourceColorParams,
    SourceVideoCodec, SourceVideoStream, SvgTemplate, TonemapAlgorithm, V4l2m2mPipelineBuilder,
    VaapiPipelineBuilder, VaapiProfileEntrypoint, VideoToolboxPipelineBuilder, WatermarkFadeKind,
    WatermarkFadePoint, WatermarkOpacityClock, WatermarkOverlay,
};

fn h264_request(desired: DesiredVideoState) -> PipelineRequest {
    PipelineRequest::new(InputFile::video("/media/movie.mkv"), desired)
        .with_source_video_stream(SourceVideoStream::new(SourceVideoCodec::H264))
}

/// Replace every spelling of `output_dir` (raw, forward-slash, and the
/// filter-escaped `X\:`/`X\\:` drive forms) with `<tmp>` so snapshots taken
/// on Linux (`/tmp/...`) and Windows (`C:\Users\...\Temp\...`) are identical.
fn normalize_temp_paths(value: &str, output_dir: &std::path::Path) -> String {
    let raw = output_dir.to_string_lossy().into_owned();
    let forward = raw.replace('\\', "/");
    // drawvg quoting doubles backslashes; subtitles paths escape the drive
    // colon. Cover every spelling the filters emit.
    let mut variants = vec![raw.replace('\\', r"\\"), raw, forward.clone()];
    if forward.len() >= 2 && forward.as_bytes()[1] == b':' {
        let (drive, rest) = forward.split_at(1);
        let rest = &rest[1..];
        variants.push(format!(r"{drive}\\:{rest}"));
        variants.push(format!(r"{drive}\:{rest}"));
    }
    variants.sort_by_key(|variant| std::cmp::Reverse(variant.len()));
    let mut normalized = value.to_owned();
    for variant in variants {
        normalized = normalized.replace(&variant, "<tmp>");
    }
    normalized
        .replace(r"<tmp>\\", "<tmp>/")
        .replace(r"<tmp>\", "<tmp>/")
}

fn source_1080p(codec: SourceVideoCodec, width: u32, height: u32) -> SourceVideoStream {
    let mut source = SourceVideoStream::new(codec);
    source.frame_size = Some(FrameSize::new(width, height));
    source.sample_aspect_ratio = Some("1:1".to_owned());
    source.display_aspect_ratio = Some(format!("{width}:{height}"));
    source.frame_rate = Some(FrameRate::new(24_000, 1_001));
    source
}

fn software_state() -> FrameState {
    FrameState::new(FrameSize::new(1920, 1080))
}

fn hardware_state(pixel_format: PixelFormat) -> FrameState {
    let mut state = software_state();
    state.frame_data_location = FrameDataLocation::Hardware;
    state.pixel_format = pixel_format;
    state
}

fn hevc_10bit_desired() -> DesiredVideoState {
    let mut desired = DesiredVideoState::h264_1080p();
    desired.video_format = FFmpegProfileVideoFormat::Hevc;
    desired.bit_depth = FFmpegProfileBitDepth::TenBit;
    desired.video_profile = None;
    desired
}

#[test]
fn software_pipeline_single_video_mpegts_snapshot() {
    let desired = DesiredVideoState::h264_1080p();
    let request = h264_request(desired);

    let pipeline = SoftwarePipelineBuilder.build(request.clone());
    let command = pipeline.command(&request.inputs);
    let arguments = command.arguments.join(" ");

    assert!(arguments.contains("-r 30000/1001 -fps_mode:v cfr"));
    assert!(!arguments.contains("-vsync"));
    assert!(arguments.contains("-maxrate:a 192k"));
    assert!(arguments.contains("-maxrate:v 8000k"));
    insta::assert_snapshot!(arguments);
}

#[test]
fn pipeline_uses_persisted_stream_indexes_when_audio_precedes_video() {
    let mut source_video = SourceVideoStream::new(SourceVideoCodec::H264).with_index(1);
    source_video.frame_size = Some(FrameSize::new(1920, 1080));
    source_video.pixel_format = Some(PixelFormat::Yuv420P);
    source_video.color_params = SourceColorParams {
        color_range: Some("tv".to_owned()),
        color_space: Some("bt709".to_owned()),
        color_transfer: Some("bt709".to_owned()),
        color_primaries: Some("bt709".to_owned()),
        is_hdr: false,
        is_bt2020_ten: false,
    };

    let mut request = PipelineRequest::new(
        InputFile::video("/media/music-video.mp4"),
        DesiredVideoState::h264_1080p(),
    )
    .with_source_video_stream(source_video)
    .with_source_audio_streams(vec![
        SourceAudioStream::new(SourceAudioCodec::Aac).with_index(0)
    ]);
    request
        .inputs
        .push(InputFile::watermark("/media/mtv-logo.png"));
    request.watermarks.push(WatermarkOverlay {
        opacity: 30,
        ..WatermarkOverlay::default()
    });

    let pipeline = SoftwarePipelineBuilder.build(request.clone());
    let args = pipeline.command(&request.inputs).arguments.join(" ");

    assert!(
        args.contains("[0:1][wm0]overlay=x=W-w-38:y=H-h-22:format=0[vwm0]"),
        "{args}"
    );
    assert!(args.contains("[0:0]aresample=async=1,apad[a]"), "{args}");
    assert!(
        args.contains("[1:0]scale=192:-1,format=yuva420p|yuva444p|yuva422p|rgba|abgr|bgra|gbrap|ya8,colorchannelmixer=aa=0.30[wm0]"),
        "{args}"
    );
    assert!(!args.contains("[0:0]setparams="), "{args}");
}

#[test]
fn software_pipeline_emits_output_metadata_snapshot() {
    let desired = DesiredVideoState::h264_1080p();
    let mut request = h264_request(desired);
    request.state.metadata = OutputMetadata {
        service_provider: Some("ErsatzTV".to_owned()),
        service_name: Some("Cinema One".to_owned()),
        audio_language: Some("eng".to_owned()),
        subtitle_language: Some("nld".to_owned()),
        subtitle_title: Some("Dutch SDH".to_owned()),
    };

    let pipeline = SoftwarePipelineBuilder.build(request.clone());
    let command = pipeline.command(&request.inputs);

    insta::assert_snapshot!(command.arguments.join(" "));
}

#[test]
fn software_pipeline_converts_ten_bit_source_to_desired_eight_bit() {
    // Port of `SoftwarePipelineBuilder.SetPixelFormat`: a 10-bit (Main 10)
    // HEVC source transcoded to the 8-bit H.264 `high` profile must emit a
    // `-pix_fmt yuv420p` output option, otherwise libx264 cannot open the
    // encoder ("high profile doesn't support a bit depth of 10").
    let mut source = SourceVideoStream::new(SourceVideoCodec::Hevc);
    source.pixel_format = Some(PixelFormat::Yuv420P10Le);
    let request = PipelineRequest::new(
        InputFile::video("/media/movie.mkv"),
        DesiredVideoState::h264_1080p(),
    )
    .with_source_video_stream(source);

    let pipeline = SoftwarePipelineBuilder.build(request.clone());
    let args = pipeline.command(&request.inputs).arguments.join(" ");

    assert!(
        args.contains("-pix_fmt yuv420p"),
        "expected 10-bit source to be converted to 8-bit, got: {args}"
    );
}

#[test]
fn software_pipeline_keeps_matching_eight_bit_source_without_pixel_format_option() {
    // When the source already matches the desired 8-bit format, no `-pix_fmt`
    // option is emitted (matching ErsatzTV's equality check).
    let mut source = SourceVideoStream::new(SourceVideoCodec::H264);
    source.pixel_format = Some(PixelFormat::Yuv420P);
    let request = PipelineRequest::new(
        InputFile::video("/media/movie.mkv"),
        DesiredVideoState::h264_1080p(),
    )
    .with_source_video_stream(source);

    let pipeline = SoftwarePipelineBuilder.build(request.clone());
    let args = pipeline.command(&request.inputs).arguments.join(" ");

    assert!(
        !args.contains("-pix_fmt"),
        "did not expect a pixel-format option for a matching source, got: {args}"
    );
}

#[test]
fn desired_video_state_from_profile_maps_transcode_fields_and_source_sizing() {
    let source = source_1080p(SourceVideoCodec::H264, 1440, 1080);
    let profile = FFmpegProfile {
        video_format: FFmpegProfileVideoFormat::Hevc,
        audio_format: FFmpegProfileAudioFormat::Ac3,
        bit_depth: FFmpegProfileBitDepth::TenBit,
        tonemap_algorithm: FFmpegProfileTonemapAlgorithm::Gamma,
        video_profile: Some("main10".to_owned()),
        video_preset: Some("slow".to_owned()),
        allow_b_frames: true,
        video_bitrate: 4_000,
        video_buffer_size: 8_000,
        audio_bitrate: 384,
        audio_buffer_size: 768,
        audio_channels: 6,
        audio_sample_rate: 44,
        normalize_loudness_mode: NormalizeLoudnessMode::LoudNorm,
        target_loudness: Some(-23.0),
        normalize_framerate: true,
        normalize_colors: true,
        pad_mode: FilterMode::Software,
        deinterlace_video: Some(true),
        qsv_extra_hardware_frames: Some(18),
        ..FFmpegProfile::default()
    };
    let resolution = Resolution {
        width: 1920,
        height: 1080,
        ..Resolution::default()
    };

    let desired = DesiredVideoState::from_profile(
        &profile,
        &resolution,
        Some(&source),
        Some(FrameRate::new(24_000, 1_001)),
        StreamingMode::TransportStream,
    );

    assert_eq!(desired.scaled_size, FrameSize::new(1440, 1080));
    assert_eq!(desired.frame_size, FrameSize::new(1920, 1080));
    assert_eq!(desired.video_format, FFmpegProfileVideoFormat::Hevc);
    assert_eq!(desired.audio_format, FFmpegProfileAudioFormat::Ac3);
    assert_eq!(desired.bit_depth, FFmpegProfileBitDepth::TenBit);
    assert_eq!(
        desired.tonemap_algorithm,
        FFmpegProfileTonemapAlgorithm::Gamma
    );
    assert_eq!(desired.video_bitrate, Some(4_000));
    assert_eq!(desired.video_buffer_size, Some(8_000));
    assert_eq!(desired.audio_bitrate, Some(384));
    assert_eq!(desired.audio_sample_rate, Some(44_000));
    assert_eq!(desired.audio_channels, Some(6));
    assert!(desired.pad_audio);
    assert_eq!(desired.target_loudness, Some(-23.0));
    assert_eq!(desired.frame_rate, Some(FrameRate::new(24_000, 1_001)));
    assert!(desired.deinterlace);
    assert!(desired.allow_b_frames);
    assert_eq!(desired.qsv_extra_hardware_frames, 18);
}

#[test]
fn desired_video_state_converts_legacy_nvenc_preset_for_ffmpeg_nine() {
    let profile = FFmpegProfile {
        hardware_acceleration: HardwareAccelerationKind::Nvenc,
        video_format: FFmpegProfileVideoFormat::H264,
        video_preset: Some("llhq".to_owned()),
        video_tune: None,
        ..FFmpegProfile::default()
    };
    let resolution = Resolution {
        width: 1920,
        height: 1080,
        ..Resolution::default()
    };
    let desired = DesiredVideoState::from_profile(
        &profile,
        &resolution,
        Some(&source_1080p(SourceVideoCodec::H264, 1920, 1080)),
        Some(FrameRate::DEFAULT),
        StreamingMode::HttpLiveStreamingSegmenter,
    );
    let request = h264_request(desired);
    let arguments = NvencPipelineBuilder
        .build(request.clone())
        .command(&request.inputs)
        .arguments
        .join(" ");

    assert!(arguments.contains("-preset:v p7 -tune:v ll"), "{arguments}");
    assert!(!arguments.contains("llhq"), "{arguments}");
}

#[test]
fn desired_video_state_uses_copy_for_hls_direct() {
    let profile = FFmpegProfile {
        video_format: FFmpegProfileVideoFormat::Hevc,
        audio_format: FFmpegProfileAudioFormat::Ac3,
        video_bitrate: 4_000,
        audio_bitrate: 384,
        ..FFmpegProfile::default()
    };
    let resolution = Resolution {
        width: 1280,
        height: 720,
        ..Resolution::default()
    };

    let desired = DesiredVideoState::from_profile(
        &profile,
        &resolution,
        None,
        Some(FrameRate::DEFAULT),
        StreamingMode::HttpLiveStreamingDirect,
    );

    assert_eq!(desired.video_format, FFmpegProfileVideoFormat::Copy);
    assert_eq!(desired.audio_format, FFmpegProfileAudioFormat::Copy);
    assert_eq!(desired.frame_rate, None);
    assert_eq!(desired.video_bitrate, None);
    assert_eq!(desired.audio_bitrate, None);
    assert!(!desired.deinterlace);
    assert!(!desired.pad_audio);
}

#[test]
fn desired_video_state_from_profile_disables_deinterlace_when_flag_is_not_true() {
    let source = source_1080p(SourceVideoCodec::H264, 1920, 1080);
    let resolution = Resolution {
        width: 1920,
        height: 1080,
        ..Resolution::default()
    };

    for deinterlace_video in [None, Some(false)] {
        let profile = FFmpegProfile {
            video_format: FFmpegProfileVideoFormat::H264,
            audio_format: FFmpegProfileAudioFormat::Aac,
            deinterlace_video,
            ..FFmpegProfile::default()
        };

        let desired = DesiredVideoState::from_profile(
            &profile,
            &resolution,
            Some(&source),
            None,
            StreamingMode::TransportStream,
        );

        assert!(
            !desired.deinterlace,
            "nullable profile flag must only enable deinterlace when true"
        );
    }
}

#[test]
fn desired_video_state_crop_scales_small_source_to_cover_target() {
    let source = source_1080p(SourceVideoCodec::H264, 1280, 960);
    let profile = FFmpegProfile {
        scaling_behavior: ScalingBehavior::Crop,
        video_format: FFmpegProfileVideoFormat::H264,
        audio_format: FFmpegProfileAudioFormat::Aac,
        ..FFmpegProfile::default()
    };
    let resolution = Resolution {
        width: 1920,
        height: 1080,
        ..Resolution::default()
    };

    let desired = DesiredVideoState::from_profile(
        &profile,
        &resolution,
        Some(&source),
        None,
        StreamingMode::TransportStream,
    );

    assert_eq!(desired.scaled_size, FrameSize::new(1920, 1440));
    assert_eq!(desired.cropped_size, Some(FrameSize::new(1920, 1080)));
}

#[test]
fn desired_video_state_scale_and_pad_rounds_fit_down_before_padding() {
    let source = source_1080p(SourceVideoCodec::H264, 1067, 600);
    let profile = FFmpegProfile {
        video_format: FFmpegProfileVideoFormat::H264,
        audio_format: FFmpegProfileAudioFormat::Aac,
        ..FFmpegProfile::default()
    };
    let resolution = Resolution {
        width: 1920,
        height: 1080,
        ..Resolution::default()
    };

    let desired = DesiredVideoState::from_profile(
        &profile,
        &resolution,
        Some(&source),
        None,
        StreamingMode::TransportStream,
    );

    assert_eq!(desired.scaled_size, FrameSize::new(1920, 1078));
    assert_eq!(desired.frame_size, FrameSize::new(1920, 1080));
    assert_eq!(desired.cropped_size, None);
}

#[test]
fn desired_video_state_stretch_uses_exact_target_size() {
    let source = source_1080p(SourceVideoCodec::H264, 1067, 600);
    let profile = FFmpegProfile {
        scaling_behavior: ScalingBehavior::Stretch,
        video_format: FFmpegProfileVideoFormat::H264,
        audio_format: FFmpegProfileAudioFormat::Aac,
        ..FFmpegProfile::default()
    };
    let resolution = Resolution {
        width: 1920,
        height: 1080,
        ..Resolution::default()
    };

    let desired = DesiredVideoState::from_profile(
        &profile,
        &resolution,
        Some(&source),
        None,
        StreamingMode::TransportStream,
    );

    assert_eq!(desired.scaled_size, FrameSize::new(1920, 1080));
    assert_eq!(desired.frame_size, FrameSize::new(1920, 1080));
    assert_eq!(desired.cropped_size, None);
}

#[test]
fn desired_video_state_crop_rounds_fill_up_before_cropping() {
    let source = source_1080p(SourceVideoCodec::H264, 1067, 600);
    let profile = FFmpegProfile {
        scaling_behavior: ScalingBehavior::Crop,
        video_format: FFmpegProfileVideoFormat::H264,
        audio_format: FFmpegProfileAudioFormat::Aac,
        ..FFmpegProfile::default()
    };
    let resolution = Resolution {
        width: 1920,
        height: 1080,
        ..Resolution::default()
    };

    let desired = DesiredVideoState::from_profile(
        &profile,
        &resolution,
        Some(&source),
        None,
        StreamingMode::TransportStream,
    );

    assert_eq!(desired.scaled_size, FrameSize::new(1922, 1080));
    assert_eq!(desired.frame_size, FrameSize::new(1920, 1080));
    assert_eq!(desired.cropped_size, Some(FrameSize::new(1920, 1080)));
}

#[test]
fn software_pipeline_hls_direct_copy_does_not_scale_source() {
    let source = source_1080p(SourceVideoCodec::H264, 1440, 1080);
    let profile = FFmpegProfile {
        video_format: FFmpegProfileVideoFormat::H264,
        audio_format: FFmpegProfileAudioFormat::Aac,
        ..FFmpegProfile::default()
    };
    let resolution = Resolution {
        width: 1920,
        height: 1080,
        ..Resolution::default()
    };
    let desired = DesiredVideoState::from_profile(
        &profile,
        &resolution,
        Some(&source),
        Some(FrameRate::DEFAULT),
        StreamingMode::HttpLiveStreamingDirect,
    );
    let mut request = PipelineRequest::new(InputFile::video("/media/movie.mkv"), desired)
        .with_source_video_stream(source);
    request.state.output_format = OutputFormat::Mp4;

    let pipeline = SoftwarePipelineBuilder.build(request.clone());
    let args = pipeline.command(&request.inputs).arguments.join(" ");

    assert!(
        args.contains("-codec:v copy"),
        "expected video copy: {args}"
    );
    assert!(
        !args.contains("scale=") && !args.contains("pad=") && !args.contains("crop="),
        "direct copy should not include resize filters: {args}"
    );
    assert!(
        !args.contains("apad"),
        "direct copy should not include audio padding: {args}"
    );
    assert!(
        args.contains(
            "+faststart+frag_keyframe+separate_moof+omit_tfhd_offset+empty_moov+delay_moov"
        ),
        "direct mp4 copy should use ErsatzTV's HLS direct mp4 movflags: {args}"
    );
}

#[test]
fn software_pipeline_fmp4_hls_uses_fragmented_mp4_movflags() {
    let mut request = h264_request(DesiredVideoState::h264_1080p());
    request.state.output_format = OutputFormat::HlsMp4;
    request.state.hls = Some(HlsOutput {
        segment_template: "/tmp/channel-1/%05d.m4s".to_owned(),
        init_template: Some("init.mp4".to_owned()),
        playlist_path: "/tmp/channel-1/index.m3u8".to_owned(),
        segment_options: None,
        is_first_transcode: true,
        discontinuity: false,
        troubleshooting: false,
    });

    let pipeline = SoftwarePipelineBuilder.build(request.clone());
    let args = pipeline.command(&request.inputs).arguments.join(" ");

    assert!(
        args.contains("+empty_moov+omit_tfhd_offset+frag_keyframe+default_base_moof"),
        "fmp4 hls should use ErsatzTV's fragmented mp4 movflags: {args}"
    );
    assert!(
        !args.contains("-movflags +faststart "),
        "fmp4 hls should not use the default faststart movflags: {args}"
    );
}

#[test]
fn vaapi_pipeline_profile_pad_uploads_after_software_pad() {
    let source = source_1080p(SourceVideoCodec::H264, 1440, 1080);
    let profile = FFmpegProfile {
        video_format: FFmpegProfileVideoFormat::H264,
        audio_format: FFmpegProfileAudioFormat::Aac,
        pad_mode: FilterMode::Software,
        ..FFmpegProfile::default()
    };
    let resolution = Resolution {
        width: 1920,
        height: 1080,
        ..Resolution::default()
    };
    let desired = DesiredVideoState::from_profile(
        &profile,
        &resolution,
        Some(&source),
        None,
        StreamingMode::TransportStream,
    );
    let request = PipelineRequest::new(InputFile::video("/media/movie.mkv"), desired)
        .with_source_video_stream(source);

    let pipeline = VaapiPipelineBuilder.build(request.clone());
    let command = pipeline.command(&request.inputs);
    let graph = pipeline.filter_graph.expect("filter graph");
    let pad_index = graph.find("pad=1920:1080").expect("software pad");
    let upload_index = graph
        .find("format=nv12|p010le|vaapi,hwupload")
        .expect("vaapi upload");

    assert!(
        pad_index < upload_index,
        "VAAPI upload must follow software padding: {graph}"
    );
    assert!(command.arguments.join(" ").contains("-codec:v h264_vaapi"));
}

#[test]
fn vaapi_pipeline_profile_hardware_pad_uses_pad_vaapi_snapshot() {
    let source = source_1080p(SourceVideoCodec::H264, 1440, 1080);
    let profile = FFmpegProfile {
        video_format: FFmpegProfileVideoFormat::H264,
        audio_format: FFmpegProfileAudioFormat::Aac,
        pad_mode: FilterMode::HardwareIfPossible,
        ..FFmpegProfile::default()
    };
    let resolution = Resolution {
        width: 1920,
        height: 1080,
        ..Resolution::default()
    };
    let desired = DesiredVideoState::from_profile(
        &profile,
        &resolution,
        Some(&source),
        None,
        StreamingMode::TransportStream,
    );
    let request = PipelineRequest::new(InputFile::video("/media/movie.mkv"), desired)
        .with_source_video_stream(source);

    let pipeline = VaapiPipelineBuilder.build(request.clone());
    let graph = pipeline.filter_graph.expect("filter graph");

    assert!(
        graph.contains("pad_vaapi=w=1920:h=1080:x=-1:y=-1:color=black"),
        "expected VAAPI padding: {graph}"
    );
    assert!(
        !graph.contains("pad=1920:1080"),
        "hardware padding should avoid software pad: {graph}"
    );
    insta::assert_snapshot!(graph);
}

#[test]
fn software_pipeline_profile_scale_pad_ac3_snapshot() {
    let source = source_1080p(SourceVideoCodec::H264, 1440, 1080);
    let profile = FFmpegProfile {
        video_format: FFmpegProfileVideoFormat::Hevc,
        audio_format: FFmpegProfileAudioFormat::Ac3,
        bit_depth: FFmpegProfileBitDepth::TenBit,
        video_profile: Some("main10".to_owned()),
        video_preset: Some("slow".to_owned()),
        video_bitrate: 4_000,
        video_buffer_size: 8_000,
        audio_bitrate: 384,
        audio_channels: 6,
        audio_sample_rate: 48,
        normalize_loudness_mode: NormalizeLoudnessMode::LoudNorm,
        target_loudness: Some(-23.0),
        ..FFmpegProfile::default()
    };
    let resolution = Resolution {
        width: 1920,
        height: 1080,
        ..Resolution::default()
    };
    let desired = DesiredVideoState::from_profile(
        &profile,
        &resolution,
        Some(&source),
        None,
        StreamingMode::TransportStream,
    );
    let request = PipelineRequest::new(InputFile::video("/media/movie.mkv"), desired)
        .with_source_video_stream(source);

    let pipeline = SoftwarePipelineBuilder.build(request.clone());
    let command = pipeline.command(&request.inputs);
    let arguments = command.arguments.join(" ");

    assert!(arguments.contains("-tag:v hvc1"));
    assert!(arguments.contains("-x265-params log-level=error"));
    insta::assert_snapshot!(arguments);
}

#[test]
fn software_pipeline_scale_and_pad_near_16_9_source_snapshot() {
    let source = source_1080p(SourceVideoCodec::H264, 1067, 600);
    let profile = FFmpegProfile {
        video_format: FFmpegProfileVideoFormat::H264,
        audio_format: FFmpegProfileAudioFormat::Aac,
        ..FFmpegProfile::default()
    };
    let resolution = Resolution {
        width: 1920,
        height: 1080,
        ..Resolution::default()
    };
    let desired = DesiredVideoState::from_profile(
        &profile,
        &resolution,
        Some(&source),
        None,
        StreamingMode::TransportStream,
    );
    let request = PipelineRequest::new(InputFile::video("/media/movie.mkv"), desired)
        .with_source_video_stream(source);

    let pipeline = SoftwarePipelineBuilder.build(request.clone());
    let arguments = pipeline.command(&request.inputs).arguments.join(" ");

    assert!(
        arguments
            .contains("scale=1920:1080:flags=fast_bilinear:force_original_aspect_ratio=decrease"),
        "scale-and-pad must preserve source aspect ratio: {arguments}"
    );
    assert!(
        arguments.contains("pad=1920:1080:-1:-1:color=black"),
        "scale-and-pad must add padding: {arguments}"
    );
    insta::assert_snapshot!(arguments);
}

#[test]
fn software_pipeline_stretch_near_16_9_source_snapshot() {
    let source = source_1080p(SourceVideoCodec::H264, 1067, 600);
    let profile = FFmpegProfile {
        scaling_behavior: ScalingBehavior::Stretch,
        video_format: FFmpegProfileVideoFormat::H264,
        audio_format: FFmpegProfileAudioFormat::Aac,
        ..FFmpegProfile::default()
    };
    let resolution = Resolution {
        width: 1920,
        height: 1080,
        ..Resolution::default()
    };
    let desired = DesiredVideoState::from_profile(
        &profile,
        &resolution,
        Some(&source),
        None,
        StreamingMode::TransportStream,
    );
    let request = PipelineRequest::new(InputFile::video("/media/movie.mkv"), desired)
        .with_source_video_stream(source);

    let pipeline = SoftwarePipelineBuilder.build(request.clone());
    let arguments = pipeline.command(&request.inputs).arguments.join(" ");

    assert!(
        arguments.contains("scale=1920:1080:flags=fast_bilinear,setsar=1"),
        "stretch must scale directly to the target frame: {arguments}"
    );
    assert!(
        !arguments.contains("force_original_aspect_ratio")
            && !arguments.contains("pad=1920:1080")
            && !arguments.contains("crop=w=1920:h=1080"),
        "stretch must not preserve aspect ratio, pad, or crop: {arguments}"
    );
    insta::assert_snapshot!(arguments);
}

#[test]
fn software_pipeline_crop_near_16_9_source_snapshot() {
    let source = source_1080p(SourceVideoCodec::H264, 1067, 600);
    let profile = FFmpegProfile {
        scaling_behavior: ScalingBehavior::Crop,
        video_format: FFmpegProfileVideoFormat::H264,
        audio_format: FFmpegProfileAudioFormat::Aac,
        ..FFmpegProfile::default()
    };
    let resolution = Resolution {
        width: 1920,
        height: 1080,
        ..Resolution::default()
    };
    let desired = DesiredVideoState::from_profile(
        &profile,
        &resolution,
        Some(&source),
        None,
        StreamingMode::TransportStream,
    );
    let request = PipelineRequest::new(InputFile::video("/media/movie.mkv"), desired)
        .with_source_video_stream(source);

    let pipeline = SoftwarePipelineBuilder.build(request.clone());
    let arguments = pipeline.command(&request.inputs).arguments.join(" ");

    assert!(
        arguments
            .contains("scale=1920:1080:flags=fast_bilinear:force_original_aspect_ratio=increase"),
        "crop must scale to cover the target frame: {arguments}"
    );
    assert!(
        arguments.contains("crop=w=1920:h=1080"),
        "crop must trim back to the target frame: {arguments}"
    );
    insta::assert_snapshot!(arguments);
}

#[test]
fn nvenc_pipeline_scale_and_pad_near_16_9_source_snapshot() {
    let mut source = source_1080p(SourceVideoCodec::H264, 1067, 600);
    source.pixel_format = Some(PixelFormat::Yuv420P);
    let profile = FFmpegProfile {
        video_format: FFmpegProfileVideoFormat::H264,
        audio_format: FFmpegProfileAudioFormat::Aac,
        ..FFmpegProfile::default()
    };
    let resolution = Resolution {
        width: 1920,
        height: 1080,
        ..Resolution::default()
    };
    let desired = DesiredVideoState::from_profile(
        &profile,
        &resolution,
        Some(&source),
        None,
        StreamingMode::HttpLiveStreamingSegmenter,
    );
    let request = PipelineRequest::new(InputFile::video("/media/movie.mkv"), desired)
        .with_source_video_stream(source);

    let pipeline = NvencPipelineBuilder.build(request.clone());
    let arguments = pipeline.command(&request.inputs).arguments.join(" ");

    assert!(
        arguments.contains("scale_cuda=1920:1080:format=nv12:force_original_aspect_ratio=decrease"),
        "NVENC scale-and-pad must preserve source aspect ratio: {arguments}"
    );
    assert!(
        arguments.contains("pad=1920:1080:-1:-1:color=black"),
        "NVENC scale-and-pad must add padding: {arguments}"
    );
    insta::assert_snapshot!(arguments);
}

#[test]
fn software_pipeline_seeking_dts_uses_core_only_decoder_snapshot() {
    let desired = DesiredVideoState::h264_1080p();
    let mut request =
        h264_request(desired)
            .with_source_audio_streams(vec![
                SourceAudioStream::new(SourceAudioCodec::Dts).with_index(1)
            ]);
    request.state.start = Some(Duration::from_mins(2));

    let pipeline = SoftwarePipelineBuilder.build(request.clone());
    let command = pipeline.command(&request.inputs);

    insta::assert_snapshot!(command.arguments.join(" "));
}

#[test]
fn software_pipeline_ac3_stereo_output_uses_decoder_downmix_snapshot() {
    let desired = DesiredVideoState::h264_1080p();
    let mut audio = SourceAudioStream::new(SourceAudioCodec::Ac3).with_index(1);
    audio.channels = Some(6);
    let request = h264_request(desired).with_source_audio_streams(vec![audio]);

    let pipeline = SoftwarePipelineBuilder.build(request.clone());
    let command = pipeline.command(&request.inputs);

    insta::assert_snapshot!(command.arguments.join(" "));
}

#[test]
fn software_pipeline_omits_audio_channels_when_source_matches_desired() {
    let desired = DesiredVideoState::h264_1080p();
    let mut audio = SourceAudioStream::new(SourceAudioCodec::Aac).with_index(1);
    audio.channels = Some(2);
    let request = h264_request(desired).with_source_audio_streams(vec![audio]);

    let pipeline = SoftwarePipelineBuilder.build(request.clone());
    let command = pipeline.command(&request.inputs);

    assert!(!command
        .arguments
        .windows(2)
        .any(|args| args == ["-ac", "2"]));
}

#[test]
fn software_pipeline_emits_aac_audio_channels_for_surround_output() {
    let mut desired = DesiredVideoState::h264_1080p();
    desired.audio_channels = Some(6);
    let mut audio = SourceAudioStream::new(SourceAudioCodec::Aac).with_index(1);
    audio.channels = Some(6);
    let request = h264_request(desired).with_source_audio_streams(vec![audio]);

    let pipeline = SoftwarePipelineBuilder.build(request.clone());
    let command = pipeline.command(&request.inputs);

    assert!(command
        .arguments
        .windows(2)
        .any(|args| args == ["-ac", "6"]));
}

#[test]
fn software_pipeline_nut_output_skips_audio_rate_and_buffer_options() {
    let mut request = h264_request(DesiredVideoState::h264_1080p());
    request.state.output_format = OutputFormat::Nut;

    let pipeline = SoftwarePipelineBuilder.build(request.clone());
    let command = pipeline.command(&request.inputs);

    assert!(!command
        .arguments
        .iter()
        .any(|arg| matches!(arg.as_str(), "-b:a" | "-bufsize:a" | "-ar")));
}

#[test]
fn software_pipeline_mpeg2_output_uses_supported_scene_detect_threshold_snapshot() {
    let mut desired = DesiredVideoState::h264_1080p();
    desired.video_format = FFmpegProfileVideoFormat::Mpeg2Video;
    desired.video_profile = None;
    let request = h264_request(desired);

    let pipeline = SoftwarePipelineBuilder.build(request.clone());
    let command = pipeline.command(&request.inputs);

    insta::assert_snapshot!(command.arguments.join(" "));
}

#[test]
fn software_pipeline_burns_text_subtitle_with_libass_filter() {
    let mut request = h264_request(DesiredVideoState::h264_1080p())
        .with_subtitle_fonts_dir("/cache/streams/fonts");
    request.inputs.push(InputFile::text_subtitle(
        "/cache/streams/subtitles/AA/BB/subtitle.srt",
        ersatzrs_ffmpeg::SubtitleMethod::Burn,
    ));

    let pipeline = SoftwarePipelineBuilder.build(request.clone());
    let args = pipeline.command(&request.inputs).arguments.join(" ");

    assert!(
        args.contains(
            "subtitles=/cache/streams/subtitles/AA/BB/subtitle.srt:fontsdir=/cache/streams/fonts"
        ),
        "expected subtitles filter in command, got: {args}"
    );
    assert!(
        !args.contains("-i /cache/streams/subtitles/AA/BB/subtitle.srt"),
        "text subtitle burn should not add a separate ffmpeg input, got: {args}"
    );
    assert!(
        !args.contains("-codec:s"),
        "burned subtitle should not map or encode a subtitle stream, got: {args}"
    );
}

#[test]
fn software_pipeline_escapes_windows_subtitle_paths_for_libass_filter() {
    let mut request = h264_request(DesiredVideoState::h264_1080p())
        .with_subtitle_fonts_dir(r"C:\ErsatzRS\cache\streams\fonts");
    request.inputs.push(InputFile::text_subtitle(
        r"C:\ErsatzRS\cache\streams\subtitles\seek\subtitle.srt",
        ersatzrs_ffmpeg::SubtitleMethod::Burn,
    ));

    let pipeline = SoftwarePipelineBuilder.build(request.clone());
    let args = pipeline.command(&request.inputs).arguments.join(" ");

    assert!(
        args.contains(
            r"subtitles=C\\:/ErsatzRS/cache/streams/subtitles/seek/subtitle.srt:fontsdir=C\\:/ErsatzRS/cache/streams/fonts"
        ),
        "expected Windows subtitles filter paths to normalize backslashes and double-escape the drive colon for both ffmpeg parse levels, got: {args}"
    );
    assert!(
        !args.contains(r"/\Users"),
        "Windows path normalization must not leave escaped path separators, got: {args}"
    );
}

#[test]
fn software_pipeline_overlays_image_subtitle_from_source_stream() {
    let mut request = h264_request(DesiredVideoState::h264_1080p());
    request.inputs.push(
        InputFile::image_subtitle("/media/movie.mkv", ersatzrs_ffmpeg::SubtitleMethod::Burn)
            .with_stream_index(2),
    );

    let pipeline = SoftwarePipelineBuilder.build(request.clone());
    let args = pipeline.command(&request.inputs).arguments.join(" ");

    assert!(
        args.contains("[0:2]scale=1920:1080:force_original_aspect_ratio=decrease"),
        "expected image subtitle scale path, got: {args}"
    );
    assert!(
        args.contains("[0:0][st]overlay=x=(W-w)/2:y=(H-h)/2:format=0[vst]"),
        "expected centered subtitle overlay path, got: {args}"
    );
}

#[test]
fn software_pipeline_hls_snapshot() {
    let desired = DesiredVideoState::h264_1080p();
    let mut request = h264_request(desired);
    request.state.output_format = OutputFormat::Hls;
    request.state.hls = Some(HlsOutput {
        segment_template: "/tmp/channel-1/%05d.ts".to_owned(),
        init_template: None,
        playlist_path: "/tmp/channel-1/index.m3u8".to_owned(),
        segment_options: None,
        is_first_transcode: true,
        discontinuity: false,
        troubleshooting: false,
    });

    let pipeline = SoftwarePipelineBuilder.build(request.clone());
    let command = pipeline.command(&request.inputs);

    insta::assert_snapshot!(command.arguments.join(" "));
}

#[test]
fn software_pipeline_hls_time_limit_snapshot() {
    let desired = DesiredVideoState::h264_1080p();
    let mut request = h264_request(desired);
    request.state.duration = Some(Duration::from_millis(12_500));
    request.state.output_format = OutputFormat::Hls;
    request.state.hls = Some(HlsOutput {
        segment_template: "/tmp/channel-1/%05d.ts".to_owned(),
        init_template: None,
        playlist_path: "/tmp/channel-1/index.m3u8".to_owned(),
        segment_options: None,
        is_first_transcode: true,
        discontinuity: false,
        troubleshooting: true,
    });

    let pipeline = SoftwarePipelineBuilder.build(request.clone());
    let command = pipeline.command(&request.inputs);

    insta::assert_snapshot!(command.arguments.join(" "));
}

#[test]
fn software_pipeline_realtime_transcode_uses_readrate_snapshot() {
    let mut desired = DesiredVideoState::h264_1080p();
    desired.realtime_mode = RealtimeMode::On;
    let request = h264_request(desired);

    let pipeline = SoftwarePipelineBuilder.build(request.clone());
    let command = pipeline.command(&request.inputs);

    insta::assert_snapshot!(command.arguments.join(" "));
}

#[test]
fn software_pipeline_realtime_copy_uses_native_readrate_snapshot() {
    let profile = FFmpegProfile {
        video_format: FFmpegProfileVideoFormat::Copy,
        audio_format: FFmpegProfileAudioFormat::Copy,
        ..FFmpegProfile::default()
    };
    let desired = DesiredVideoState::from_profile(
        &profile,
        &Resolution {
            width: 1920,
            height: 1080,
            ..Resolution::default()
        },
        Some(&SourceVideoStream::new(SourceVideoCodec::H264)),
        Some(FrameRate::DEFAULT),
        StreamingMode::HttpLiveStreamingDirect,
    );
    let request = h264_request(desired);

    let pipeline = SoftwarePipelineBuilder.build(request.clone());
    let command = pipeline.command(&request.inputs);

    insta::assert_snapshot!(command.arguments.join(" "));
}

#[test]
fn software_pipeline_still_image_source_loops_video_snapshot() {
    let desired = DesiredVideoState::h264_1080p();
    let mut source = source_1080p(SourceVideoCodec::Unknown, 1920, 1080);
    source.is_still_image = true;
    source.frame_rate = Some(FrameRate::DEFAULT);
    let request = PipelineRequest::new(InputFile::video("/media/poster.jpg"), desired)
        .with_source_video_stream(source);

    let pipeline = SoftwarePipelineBuilder.build(request.clone());
    let command = pipeline.command(&request.inputs);

    insta::assert_snapshot!(command.arguments.join(" "));
}

#[test]
fn software_pipeline_realtime_still_image_source_uses_realtime_filter_snapshot() {
    let mut desired = DesiredVideoState::h264_1080p();
    desired.realtime_mode = RealtimeMode::On;
    let mut source = source_1080p(SourceVideoCodec::Unknown, 1920, 1080);
    source.is_still_image = true;
    source.frame_rate = Some(FrameRate::DEFAULT);
    let request = PipelineRequest::new(InputFile::video("/media/poster.jpg"), desired)
        .with_source_video_stream(source);

    let pipeline = SoftwarePipelineBuilder.build(request.clone());
    let command = pipeline.command(&request.inputs);
    let arguments = command.arguments.join(" ");

    assert!(arguments.contains("loop=-1:1,realtime"));
    assert!(!arguments.contains("-readrate 1.05 -i /media/poster.jpg"));
    insta::assert_snapshot!(arguments);
}

#[test]
fn nvenc_pipeline_snapshot() {
    let desired = DesiredVideoState::h264_1080p();
    let request = h264_request(desired);

    let pipeline = NvencPipelineBuilder.build(request.clone());
    let command = pipeline.command(&request.inputs);
    let arguments = command.arguments.join(" ");

    assert!(arguments.contains("-init_hw_device cuda"));
    assert!(arguments.contains("-hwaccel cuda"));
    assert!(arguments.contains("-hwaccel_output_format cuda"));
    insta::assert_snapshot!(arguments);
}

#[test]
fn nvenc_pipeline_uses_software_decode_when_cuvid_decoder_missing_snapshot() {
    let desired = DesiredVideoState::h264_1080p();
    let mut report = capability_report_for_tests();
    report.decoders.remove("h264_cuvid");
    let request = h264_request(desired).with_runtime_capabilities(report);

    let pipeline = NvencPipelineBuilder.build(request.clone());
    let command = pipeline.command(&request.inputs);
    let arguments = command.arguments.join(" ");

    assert!(arguments.contains("-init_hw_device cuda"));
    assert!(arguments.contains("-hwaccel none -c:v h264"));
    assert!(!arguments.contains("-hwaccel cuda"));
    assert!(!arguments.contains("-hwaccel_output_format cuda"));
    insta::assert_snapshot!(arguments);
}

#[test]
fn nvenc_pipeline_uses_software_av1_decode_without_decoder_detail_snapshot() {
    let desired = DesiredVideoState::h264_1080p();
    let mut report = capability_report_for_tests();
    report.nvidia_decoders.clear();
    let request = PipelineRequest::new(InputFile::video("/media/movie.mkv"), desired)
        .with_source_video_stream(SourceVideoStream::new(SourceVideoCodec::Av1))
        .with_runtime_capabilities(report);

    let pipeline = NvencPipelineBuilder.build(request.clone());
    let command = pipeline.command(&request.inputs);
    let arguments = command.arguments.join(" ");

    assert!(arguments.contains("-init_hw_device cuda"));
    assert!(arguments.contains("-hwaccel none -c:v libdav1d"));
    assert!(!arguments.contains("-c:v av1_cuvid"));
    assert!(!arguments.contains("-hwaccel cuda"));
    assert!(!arguments.contains("-hwaccel_output_format cuda"));
    insta::assert_snapshot!(arguments);
}

#[test]
fn nvenc_pipeline_uses_software_av1_decode_after_runtime_denial_snapshot() {
    let desired = DesiredVideoState::h264_1080p();
    let mut report = capability_report_for_tests();
    report.deny_hardware_decode(ersatzrs_ffmpeg::HardwareDecodeDenial::new(
        HardwareAccelerationKind::Nvenc,
        "av1",
        8,
    ));
    let request = PipelineRequest::new(InputFile::video("/media/movie.mkv"), desired)
        .with_source_video_stream(SourceVideoStream::new(SourceVideoCodec::Av1))
        .with_runtime_capabilities(report);

    let pipeline = NvencPipelineBuilder.build(request.clone());
    let command = pipeline.command(&request.inputs);
    let arguments = command.arguments.join(" ");

    assert!(arguments.contains("-init_hw_device cuda"));
    assert!(arguments.contains("-hwaccel none -c:v libdav1d"));
    assert!(arguments.contains("-codec:v h264_nvenc"));
    assert!(!arguments.contains("-c:v av1_cuvid"));
    assert!(!arguments.contains("-hwaccel cuda"));
    assert!(!arguments.contains("-hwaccel_output_format cuda"));
    insta::assert_snapshot!(arguments);
}

#[test]
fn nvenc_pipeline_keeps_nvenc_encode_on_encoder_only_build_snapshot() {
    // Encoder-only FFmpeg build: NVENC encoders + `cuda` hwaccel but no CUDA
    // filter kernels (no `--enable-cuda-nvcc`/`--enable-cuda-llvm`). Hardware
    // decode and the software-frame CUDA upload are both gated off so no CUDA
    // filter is emitted, while the encoder stays h264_nvenc.
    let desired = DesiredVideoState::h264_1080p();
    let mut report = capability_report_for_tests();
    report.filters.remove("scale_cuda");
    report.filters.remove("yadif_cuda");
    report.filters.remove("overlay_cuda");
    let mut source = source_1080p(SourceVideoCodec::H264, 1280, 720);
    source.scan_kind = VideoScanKind::Interlaced;
    let request = PipelineRequest::new(InputFile::video("/media/movie.mkv"), desired)
        .with_source_video_stream(source)
        .with_runtime_capabilities(report);

    let pipeline = NvencPipelineBuilder.build(request.clone());
    let command = pipeline.command(&request.inputs);
    let arguments = command.arguments.join(" ");

    assert!(arguments.contains("-codec:v h264_nvenc"));
    assert!(arguments.contains("-hwaccel none -c:v h264"));
    assert!(!arguments.contains("-hwaccel cuda"));
    assert!(!arguments.contains("scale_cuda"));
    assert!(!arguments.contains("yadif_cuda"));
    assert!(!arguments.contains("overlay_cuda"));
    assert!(!arguments.contains("hwupload"));
    insta::assert_snapshot!(arguments);
}

#[test]
fn nvenc_pipeline_architecture_gate_forces_software_av1_decode_snapshot() {
    // Probe rows say AV1 decodes in hardware, but ErsatzTV's conservative
    // architecture table (ADR 0014 step 2) says Pascal cannot: the gate is an
    // additional AND, so decode goes software while NVENC encode stays.
    let desired = DesiredVideoState::h264_1080p();
    let mut report = capability_report_for_tests();
    report.nvidia_gpu_list =
        Some("GPU #0 < NVIDIA GeForce GTX 1060 > has Compute SM 6.1".to_owned());
    let request = PipelineRequest::new(InputFile::video("/media/movie.mkv"), desired)
        .with_source_video_stream(SourceVideoStream::new(SourceVideoCodec::Av1))
        .with_runtime_capabilities(report);

    let pipeline = NvencPipelineBuilder.build(request.clone());
    let command = pipeline.command(&request.inputs);
    let arguments = command.arguments.join(" ");

    assert!(arguments.contains("-hwaccel none -c:v libdav1d"));
    assert!(arguments.contains("-codec:v h264_nvenc"));
    assert!(!arguments.contains("-c:v av1_cuvid"));
    assert!(!arguments.contains("-hwaccel cuda"));
    insta::assert_snapshot!(arguments);
}

#[test]
fn nvenc_pipeline_uses_software_decode_when_decoder_detail_lacks_10bit_snapshot() {
    let mut desired = hevc_10bit_desired();
    desired.video_preset = Some("medium".to_owned());
    let mut source = source_1080p(SourceVideoCodec::Hevc, 1920, 1080);
    source.pixel_format = Some(PixelFormat::Yuv420P10Le);
    let mut report = capability_report_for_tests();
    report.nvidia_decoders = vec![ersatzrs_ffmpeg::NvidiaDecoderCapability::new("hevc", 8)];
    let request = PipelineRequest::new(InputFile::video("/media/movie.mkv"), desired)
        .with_source_video_stream(source)
        .with_runtime_capabilities(report);

    let pipeline = NvencPipelineBuilder.build(request.clone());
    let command = pipeline.command(&request.inputs);
    let arguments = command.arguments.join(" ");

    assert!(arguments.contains("-init_hw_device cuda"));
    assert!(arguments.contains("-hwaccel none -c:v hevc"));
    assert!(!arguments.contains("-hwaccel cuda"));
    assert!(!arguments.contains("-hwaccel_output_format cuda"));
    assert!(arguments.contains("-tag:v hvc1"));
    assert!(arguments.contains("-b_ref_mode 0"));
    insta::assert_snapshot!(arguments);
}

#[test]
fn nvenc_hevc_pipeline_uses_b_ref_mode_when_probe_supports_b_frames_snapshot() {
    let mut desired = hevc_10bit_desired();
    desired.allow_b_frames = true;
    desired.video_preset = Some("medium".to_owned());
    let mut report = capability_report_for_tests();
    report.nvidia_encoders = vec![ersatzrs_ffmpeg::NvidiaEncoderCapability::new(
        "hevc_nvenc",
        std::collections::BTreeSet::from(["p010le".to_owned()]),
        std::collections::BTreeSet::from(["main10".to_owned()]),
    )
    .with_b_frames()];
    let request = h264_request(desired).with_runtime_capabilities(report);

    let pipeline = NvencPipelineBuilder.build(request.clone());
    let command = pipeline.command(&request.inputs);
    let arguments = command.arguments.join(" ");

    assert!(arguments.contains("-codec:v hevc_nvenc -tag:v hvc1 -b_ref_mode 1"));
    assert!(!arguments.contains("-bf 0"));
    insta::assert_snapshot!(arguments);
}

#[test]
fn nvenc_pipeline_uses_software_encode_when_encoder_detail_lacks_10bit_snapshot() {
    let mut request = h264_request(hevc_10bit_desired());
    let mut report = capability_report_for_tests();
    report.nvidia_encoders = vec![ersatzrs_ffmpeg::NvidiaEncoderCapability::new(
        "hevc_nvenc",
        std::collections::BTreeSet::from(["nv12".to_owned()]),
        std::collections::BTreeSet::from(["main".to_owned(), "main10".to_owned()]),
    )];
    request = request.with_runtime_capabilities(report);

    let pipeline = NvencPipelineBuilder.build(request.clone());
    let command = pipeline.command(&request.inputs);
    let arguments = command.arguments.join(" ");

    assert!(arguments.contains("-tag:v hvc1"));
    assert!(arguments.contains("-x265-params log-level=error"));
    insta::assert_snapshot!(arguments);
}

#[test]
fn nvenc_pipeline_with_watermark_uses_cuda_overlay_snapshot() {
    let mut request = h264_request(DesiredVideoState::h264_1080p());
    request.inputs.push(InputFile::watermark("/media/logo.png"));
    request.watermarks.push(WatermarkOverlay {
        location: WatermarkLocation::TopRight,
        size: WatermarkSize::Scaled,
        width_percent: 12.5,
        horizontal_margin_percent: 2.0,
        vertical_margin_percent: 3.0,
        opacity: 75,
        ..WatermarkOverlay::default()
    });

    let pipeline = NvencPipelineBuilder.build(request.clone());
    let command = pipeline.command(&request.inputs);

    insta::assert_snapshot!(command.arguments.join(" "));
}

#[test]
fn nvenc_pipeline_burns_text_subtitle_before_cuda_watermark_snapshot() {
    let mut request = h264_request(DesiredVideoState::h264_1080p())
        .with_subtitle_fonts_dir("/cache/streams/fonts");
    request.inputs.push(InputFile::text_subtitle(
        "/cache/streams/subtitles/AA/BB/subtitle.srt",
        ersatzrs_ffmpeg::SubtitleMethod::Burn,
    ));
    request.inputs.push(InputFile::watermark("/media/logo.png"));
    request.watermarks.push(WatermarkOverlay {
        location: WatermarkLocation::TopRight,
        size: WatermarkSize::Scaled,
        width_percent: 12.5,
        horizontal_margin_percent: 2.0,
        vertical_margin_percent: 3.0,
        opacity: 75,
        ..WatermarkOverlay::default()
    });

    let pipeline = NvencPipelineBuilder.build(request.clone());
    let command = pipeline.command(&request.inputs);

    insta::assert_snapshot!(command.arguments.join(" "));
}

#[test]
fn nvenc_pipeline_interlaced_software_decode_does_not_double_upload_before_yadif() {
    let mut source = source_1080p(SourceVideoCodec::Hevc, 1920, 1080);
    source.pixel_format = Some(PixelFormat::Yuv420P10Le);
    source.scan_kind = VideoScanKind::Interlaced;
    let mut report = capability_report_for_tests();
    report.decoders.remove("hevc_cuvid");
    let mut request = PipelineRequest::new(
        InputFile::video("/media/movie.mkv"),
        DesiredVideoState::h264_1080p(),
    )
    .with_source_video_stream(source)
    .with_runtime_capabilities(report)
    .with_subtitle_fonts_dir("/cache/streams/fonts");
    request.inputs.push(InputFile::text_subtitle(
        "/cache/streams/subtitles/AA/BB/subtitle.srt",
        ersatzrs_ffmpeg::SubtitleMethod::Burn,
    ));
    request.inputs.push(InputFile::watermark("/media/logo.png"));
    request.watermarks.push(WatermarkOverlay {
        location: WatermarkLocation::TopRight,
        size: WatermarkSize::Scaled,
        width_percent: 12.5,
        horizontal_margin_percent: 2.0,
        vertical_margin_percent: 3.0,
        opacity: 75,
        ..WatermarkOverlay::default()
    });

    let pipeline = NvencPipelineBuilder.build(request.clone());
    let command = pipeline.command(&request.inputs);
    let arguments = command.arguments.join(" ");

    assert!(arguments.contains("[0:0]hwupload,yadif_cuda"));
    assert!(!arguments.contains("hwupload,hwupload_cuda,yadif_cuda"));
    assert!(arguments.contains("scale_cuda=format=nv12,hwdownload,format=nv12,subtitles="));
    assert!(!arguments.contains("hwdownload,format=yuv420p10le"));
    assert!(arguments.contains("subtitles=/cache/streams/subtitles/AA/BB/subtitle.srt"));
    assert!(!arguments.contains("overlay_cuda"));
    assert!(arguments.contains("overlay=x=W-w-38:y=32:format=0"));
}

#[test]
fn nvenc_pipeline_with_1080p_overlay_uses_green_line_workaround_snapshot() {
    let mut request = PipelineRequest::new(
        InputFile::video("/media/movie.mkv"),
        DesiredVideoState::h264_1080p(),
    )
    .with_source_video_stream(source_1080p(SourceVideoCodec::H264, 1920, 1080));
    request.inputs.push(InputFile::watermark("/media/logo.png"));
    request.watermarks.push(WatermarkOverlay {
        location: WatermarkLocation::TopRight,
        size: WatermarkSize::Scaled,
        width_percent: 12.5,
        horizontal_margin_percent: 2.0,
        vertical_margin_percent: 3.0,
        opacity: 75,
        ..WatermarkOverlay::default()
    });

    let pipeline = NvencPipelineBuilder.build(request.clone());
    let command = pipeline.command(&request.inputs);

    insta::assert_snapshot!(command.arguments.join(" "));
}

#[test]
fn nvenc_pipeline_with_image_subtitle_uses_cuda_overlay_snapshot() {
    let mut request = h264_request(DesiredVideoState::h264_1080p());
    request.inputs.push(InputFile::image_subtitle(
        "/media/subtitle.sup",
        ersatzrs_ffmpeg::SubtitleMethod::Burn,
    ));

    let pipeline = NvencPipelineBuilder.build(request.clone());
    let command = pipeline.command(&request.inputs);

    insta::assert_snapshot!(command.arguments.join(" "));
}

#[test]
fn nvenc_pipeline_with_scaled_image_subtitle_uses_scale_npp_when_available_snapshot() {
    let mut request = h264_request(DesiredVideoState::h264_1080p())
        .with_source_video_stream(source_1080p(SourceVideoCodec::H264, 1280, 720));
    request.inputs.push(InputFile::image_subtitle(
        "/media/subtitle.sup",
        ersatzrs_ffmpeg::SubtitleMethod::Burn,
    ));
    let mut report = capability_report_for_tests();
    report.filters.insert("scale_npp".to_owned());
    request = request.with_runtime_capabilities(report);

    let pipeline = NvencPipelineBuilder.build(request.clone());
    let command = pipeline.command(&request.inputs);

    insta::assert_snapshot!(command.arguments.join(" "));
}

#[test]
fn nvenc_pipeline_with_native_graphics_uses_cuda_overlay_snapshot() {
    let mut request = h264_request(DesiredVideoState::h264_1080p());
    request
        .graphics_overlays
        .push(GraphicsPipelineOverlay::native_image(
            17,
            InputFile::graphics_image("/media/logo.png"),
            WatermarkOverlay {
                location: WatermarkLocation::BottomLeft,
                size: WatermarkSize::Scaled,
                width_percent: 10.0,
                horizontal_margin_percent: 2.0,
                vertical_margin_percent: 3.0,
                opacity: 80,
                eof_action_pass: true,
                ..WatermarkOverlay::default()
            },
        ));

    let pipeline = NvencPipelineBuilder.build(request.clone());
    let command = pipeline.command(&request.inputs);

    insta::assert_snapshot!(command.arguments.join(" "));
}

#[test]
fn nvenc_10bit_watermark_downloads_for_software_overlay_snapshot() {
    let mut request = h264_request(hevc_10bit_desired());
    request.inputs.push(InputFile::watermark("/media/logo.png"));
    request.watermarks.push(WatermarkOverlay {
        location: WatermarkLocation::TopRight,
        size: WatermarkSize::Scaled,
        width_percent: 12.5,
        horizontal_margin_percent: 2.0,
        vertical_margin_percent: 3.0,
        opacity: 75,
        ..WatermarkOverlay::default()
    });

    let pipeline = NvencPipelineBuilder.build(request.clone());
    let command = pipeline.command(&request.inputs);

    insta::assert_snapshot!(command.arguments.join(" "));
}

#[test]
fn nvenc_10bit_image_subtitle_downloads_for_software_overlay_snapshot() {
    let mut request = h264_request(hevc_10bit_desired());
    request.inputs.push(InputFile::image_subtitle(
        "/media/subtitle.sup",
        ersatzrs_ffmpeg::SubtitleMethod::Burn,
    ));

    let pipeline = NvencPipelineBuilder.build(request.clone());
    let command = pipeline.command(&request.inputs);

    insta::assert_snapshot!(command.arguments.join(" "));
}

#[test]
fn nvenc_10bit_native_graphics_downloads_for_software_overlay_snapshot() {
    let mut request = h264_request(hevc_10bit_desired());
    request
        .graphics_overlays
        .push(GraphicsPipelineOverlay::native_image(
            17,
            InputFile::graphics_image("/media/logo.png"),
            WatermarkOverlay {
                location: WatermarkLocation::BottomLeft,
                size: WatermarkSize::Scaled,
                width_percent: 10.0,
                horizontal_margin_percent: 2.0,
                vertical_margin_percent: 3.0,
                opacity: 80,
                eof_action_pass: true,
                ..WatermarkOverlay::default()
            },
        ));

    let pipeline = NvencPipelineBuilder.build(request.clone());
    let command = pipeline.command(&request.inputs);

    insta::assert_snapshot!(command.arguments.join(" "));
}

#[test]
fn vaapi_pipeline_snapshot() {
    let desired = DesiredVideoState::h264_1080p();
    let request = h264_request(desired);

    let pipeline = VaapiPipelineBuilder.build(request.clone());
    let command = pipeline.command(&request.inputs);

    insta::assert_snapshot!(command.arguments.join(" "));
}

#[test]
fn vaapi_pipeline_uses_profile_device_and_driver_snapshot() {
    let profile = FFmpegProfile {
        hardware_acceleration: HardwareAccelerationKind::Vaapi,
        vaapi_device: Some("/dev/dri/renderD128".to_owned()),
        vaapi_driver: VaapiDriver::IHD,
        video_format: FFmpegProfileVideoFormat::H264,
        audio_format: FFmpegProfileAudioFormat::Aac,
        ..FFmpegProfile::default()
    };
    let desired = DesiredVideoState::from_profile(
        &profile,
        &Resolution {
            width: 1920,
            height: 1080,
            ..Resolution::default()
        },
        Some(&SourceVideoStream::new(SourceVideoCodec::H264)),
        Some(FrameRate::DEFAULT),
        StreamingMode::TransportStream,
    );
    let request = h264_request(desired);

    let pipeline = VaapiPipelineBuilder.build(request.clone());
    let command = pipeline.command(&request.inputs);

    insta::assert_snapshot!(format!(
        "env={:?}\nargs={}",
        command.environment,
        command.arguments.join(" ")
    ));
}

#[test]
fn vaapi_pipeline_emits_encoder_capability_options_snapshot() {
    let mut report = capability_report_for_tests();
    report.vaapi_profiles = vec![
        VaapiProfileEntrypoint::new("VAProfileH264Main", "VAEntrypointVLD"),
        VaapiProfileEntrypoint::new("VAProfileH264Main", "VAEntrypointEncSlice")
            .with_rate_control_mode("CQP")
            .with_packed_header_misc(),
    ];
    let request = h264_request(DesiredVideoState::h264_1080p()).with_runtime_capabilities(report);

    let pipeline = VaapiPipelineBuilder.build(request.clone());
    let command = pipeline.command(&request.inputs);
    let arguments = command.arguments.join(" ");

    assert!(arguments.contains("-codec:v h264_vaapi -rc_mode 1 -sei -a53_cc -profile:v high"));
    insta::assert_snapshot!(arguments);
}

#[test]
fn vaapi_radeonsi_hevc_pipeline_uses_crop_metadata_workaround_snapshot() {
    let profile = FFmpegProfile {
        hardware_acceleration: HardwareAccelerationKind::Vaapi,
        vaapi_device: Some("/dev/dri/renderD128".to_owned()),
        vaapi_driver: VaapiDriver::RadeonSI,
        video_format: FFmpegProfileVideoFormat::Hevc,
        audio_format: FFmpegProfileAudioFormat::Aac,
        bit_depth: FFmpegProfileBitDepth::EightBit,
        ..FFmpegProfile::default()
    };
    let source = source_1080p(SourceVideoCodec::H264, 1920, 1080);
    let desired = DesiredVideoState::from_profile(
        &profile,
        &Resolution {
            width: 1920,
            height: 1080,
            ..Resolution::default()
        },
        Some(&source),
        Some(FrameRate::DEFAULT),
        StreamingMode::TransportStream,
    );
    let request = PipelineRequest::new(InputFile::video("/media/movie.mkv"), desired)
        .with_source_video_stream(source);

    let pipeline = VaapiPipelineBuilder.build(request.clone());
    let command = pipeline.command(&request.inputs);

    insta::assert_snapshot!(format!(
        "env={:?}\nargs={}",
        command.environment,
        command.arguments.join(" ")
    ));
}

#[test]
fn vaapi_pipeline_with_image_subtitle_uses_vaapi_overlay_snapshot() {
    let mut request = h264_request(DesiredVideoState::h264_1080p());
    request.inputs.push(InputFile::image_subtitle(
        "/media/subtitle.sup",
        ersatzrs_ffmpeg::SubtitleMethod::Burn,
    ));

    let pipeline = VaapiPipelineBuilder.build(request.clone());
    let command = pipeline.command(&request.inputs);

    insta::assert_snapshot!(command.arguments.join(" "));
}

#[test]
fn vaapi_pipeline_with_image_subtitle_and_watermark_forces_software_overlay_snapshot() {
    let mut request = h264_request(DesiredVideoState::h264_1080p());
    request.inputs.push(InputFile::image_subtitle(
        "/media/subtitle.sup",
        ersatzrs_ffmpeg::SubtitleMethod::Burn,
    ));
    request.inputs.push(InputFile::watermark("/media/logo.png"));
    request.watermarks.push(WatermarkOverlay {
        location: WatermarkLocation::TopRight,
        size: WatermarkSize::Scaled,
        width_percent: 12.5,
        horizontal_margin_percent: 2.0,
        vertical_margin_percent: 3.0,
        opacity: 75,
        ..WatermarkOverlay::default()
    });

    let pipeline = VaapiPipelineBuilder.build(request.clone());
    let command = pipeline.command(&request.inputs);

    insta::assert_snapshot!(command.arguments.join(" "));
}

#[test]
fn vaapi_radeonsi_image_subtitle_forces_software_overlay_snapshot() {
    let mut desired = DesiredVideoState::h264_1080p();
    desired.vaapi_driver = Some("radeonsi".to_owned());
    let mut request = h264_request(desired);
    request.inputs.push(InputFile::image_subtitle(
        "/media/subtitle.sup",
        ersatzrs_ffmpeg::SubtitleMethod::Burn,
    ));

    let pipeline = VaapiPipelineBuilder.build(request.clone());
    let command = pipeline.command(&request.inputs);

    insta::assert_snapshot!(format!(
        "env={:?}\nargs={}",
        command.environment,
        command.arguments.join(" ")
    ));
}

#[test]
fn vaapi_pipeline_burns_text_subtitle_with_download_and_upload_snapshot() {
    let mut request = h264_request(DesiredVideoState::h264_1080p())
        .with_subtitle_fonts_dir("/cache/streams/fonts");
    request.inputs.push(InputFile::text_subtitle(
        "/cache/streams/subtitles/AA/BB/subtitle.srt",
        ersatzrs_ffmpeg::SubtitleMethod::Burn,
    ));

    let pipeline = VaapiPipelineBuilder.build(request.clone());
    let command = pipeline.command(&request.inputs);

    insta::assert_snapshot!(command.arguments.join(" "));
}

#[test]
fn vaapi_pipeline_with_native_graphics_uses_vaapi_overlay_snapshot() {
    let mut request = h264_request(DesiredVideoState::h264_1080p());
    request
        .graphics_overlays
        .push(GraphicsPipelineOverlay::native_image(
            17,
            InputFile::graphics_image("/media/logo.png"),
            WatermarkOverlay {
                location: WatermarkLocation::BottomLeft,
                size: WatermarkSize::Scaled,
                width_percent: 10.0,
                horizontal_margin_percent: 2.0,
                vertical_margin_percent: 3.0,
                opacity: 80,
                eof_action_pass: true,
                ..WatermarkOverlay::default()
            },
        ));

    let pipeline = VaapiPipelineBuilder.build(request.clone());
    let command = pipeline.command(&request.inputs);

    insta::assert_snapshot!(command.arguments.join(" "));
}

#[test]
fn qsv_pipeline_snapshot() {
    let desired = DesiredVideoState::h264_1080p();
    let request = h264_request(desired);

    let pipeline = QsvPipelineBuilder.build(request.clone());
    let command = pipeline.command(&request.inputs);

    insta::assert_snapshot!(format!(
        "env={:?}\nargs={}",
        command.environment,
        command.arguments.join(" ")
    ));
}

#[test]
fn qsv_pipeline_uses_profile_device_snapshot() {
    let profile = FFmpegProfile {
        hardware_acceleration: HardwareAccelerationKind::Qsv,
        vaapi_device: Some("/dev/dri/renderD129".to_owned()),
        video_format: FFmpegProfileVideoFormat::H264,
        audio_format: FFmpegProfileAudioFormat::Aac,
        ..FFmpegProfile::default()
    };
    let desired = DesiredVideoState::from_profile(
        &profile,
        &Resolution {
            width: 1920,
            height: 1080,
            ..Resolution::default()
        },
        Some(&SourceVideoStream::new(SourceVideoCodec::H264)),
        Some(FrameRate::DEFAULT),
        StreamingMode::TransportStream,
    );
    let request = h264_request(desired);

    let pipeline = QsvPipelineBuilder.build(request.clone());
    let command = pipeline.command(&request.inputs);

    let args = command.arguments.join(" ");

    // `-qsv_device` is intentionally Linux-only (the VAAPI render node has no
    // Windows/macOS equivalent), so assert it per platform and snapshot the
    // platform-neutral remainder.
    #[cfg(target_os = "linux")]
    assert!(
        args.contains("-qsv_device /dev/dri/renderD129"),
        "Linux QSV must pass the profile device, got: {args}"
    );
    #[cfg(not(target_os = "linux"))]
    assert!(
        !args.contains("-qsv_device"),
        "-qsv_device is a Linux render node option, got: {args}"
    );
    let args = args.replace("-qsv_device /dev/dri/renderD129 ", "");

    insta::assert_snapshot!(format!("env={:?}\nargs={args}", command.environment));
}

#[test]
fn qsv_pipeline_falls_back_to_software_when_encoder_is_missing_snapshot() {
    let desired = DesiredVideoState::h264_1080p();
    let mut report = capability_report_for_tests();
    report.encoders.remove("h264_qsv");
    let request = h264_request(desired).with_runtime_capabilities(report);

    let pipeline = QsvPipelineBuilder.build(request.clone());
    let command = pipeline.command(&request.inputs);

    insta::assert_snapshot!(command.arguments.join(" "));
}

#[test]
fn qsv_pipeline_falls_back_to_software_when_decoder_is_missing_snapshot() {
    let desired = DesiredVideoState::h264_1080p();
    let mut report = capability_report_for_tests();
    report.decoders.remove("h264_qsv");
    let request = h264_request(desired).with_runtime_capabilities(report);

    let pipeline = QsvPipelineBuilder.build(request.clone());
    let command = pipeline.command(&request.inputs);

    insta::assert_snapshot!(format!(
        "env={:?}\nargs={}",
        command.environment,
        command.arguments.join(" ")
    ));
}

#[test]
fn qsv_pipeline_uses_profile_extra_hardware_frames() {
    let mut desired = DesiredVideoState::h264_1080p();
    desired.qsv_extra_hardware_frames = 18;
    let mut report = capability_report_for_tests();
    report.decoders.remove("h264_qsv");
    let request = h264_request(desired).with_runtime_capabilities(report);

    let pipeline = QsvPipelineBuilder.build(request.clone());
    let command = pipeline.command(&request.inputs);

    assert!(command
        .arguments
        .join(" ")
        .contains("hwupload=extra_hw_frames=18"));
}

#[test]
fn qsv_pipeline_uses_software_decode_for_ten_bit_hevc_source_snapshot() {
    let desired = DesiredVideoState::h264_1080p();
    let mut source = SourceVideoStream::new(SourceVideoCodec::Hevc);
    source.pixel_format = Some(PixelFormat::Yuv420P10Le);
    let request = PipelineRequest::new(InputFile::video("/media/movie.mkv"), desired)
        .with_source_video_stream(source)
        .with_runtime_capabilities(capability_report_for_tests());

    let pipeline = QsvPipelineBuilder.build(request.clone());
    let command = pipeline.command(&request.inputs);

    insta::assert_snapshot!(format!(
        "env={:?}\nargs={}",
        command.environment,
        command.arguments.join(" ")
    ));
}

#[test]
fn qsv_pipeline_uses_software_decode_for_multiple_h264_profiles_without_seek_snapshot() {
    let desired = DesiredVideoState::h264_1080p();
    let mut source = SourceVideoStream::new(SourceVideoCodec::H264);
    source.has_multiple_profiles = true;
    let request = PipelineRequest::new(InputFile::video("/media/movie.mkv"), desired)
        .with_source_video_stream(source)
        .with_runtime_capabilities(capability_report_for_tests());

    let pipeline = QsvPipelineBuilder.build(request.clone());
    let command = pipeline.command(&request.inputs);

    insta::assert_snapshot!(format!(
        "env={:?}\nargs={}",
        command.environment,
        command.arguments.join(" ")
    ));
}

#[test]
fn qsv_pipeline_keeps_hardware_decode_for_multiple_h264_profiles_with_seek_snapshot() {
    let desired = DesiredVideoState::h264_1080p();
    let mut source = SourceVideoStream::new(SourceVideoCodec::H264);
    source.has_multiple_profiles = true;
    let mut request = PipelineRequest::new(InputFile::video("/media/movie.mkv"), desired)
        .with_source_video_stream(source)
        .with_runtime_capabilities(capability_report_for_tests());
    request.state.start = Some(Duration::from_secs(12));

    let pipeline = QsvPipelineBuilder.build(request.clone());
    let command = pipeline.command(&request.inputs);

    insta::assert_snapshot!(format!(
        "env={:?}\nargs={}",
        command.environment,
        command.arguments.join(" ")
    ));
}

#[test]
fn qsv_pipeline_hls_snapshot() {
    let desired = DesiredVideoState::h264_1080p();
    let mut request = h264_request(desired);
    request.state.output_format = OutputFormat::Hls;
    request.state.hls = Some(HlsOutput {
        segment_template: "/tmp/channel-1/%05d.ts".to_owned(),
        init_template: None,
        playlist_path: "/tmp/channel-1/index.m3u8".to_owned(),
        segment_options: None,
        is_first_transcode: true,
        discontinuity: false,
        troubleshooting: false,
    });

    let pipeline = QsvPipelineBuilder.build(request.clone());
    let command = pipeline.command(&request.inputs);

    insta::assert_snapshot!(command.arguments.join(" "));
}

#[test]
fn videotoolbox_pipeline_snapshot() {
    let desired = DesiredVideoState::h264_1080p();
    let request = h264_request(desired);

    let pipeline = VideoToolboxPipelineBuilder.build(request.clone());
    let command = pipeline.command(&request.inputs);

    insta::assert_snapshot!(command.arguments.join(" "));
}

#[test]
fn videotoolbox_pipeline_uses_software_decode_when_h264_10bit_source_snapshot() {
    let desired = DesiredVideoState::h264_1080p();
    let mut source = SourceVideoStream::new(SourceVideoCodec::H264);
    source.pixel_format = Some(PixelFormat::Yuv420P10Le);
    let request = PipelineRequest::new(InputFile::video("/media/movie.mkv"), desired)
        .with_source_video_stream(source)
        .with_runtime_capabilities(capability_report_for_tests());

    let pipeline = VideoToolboxPipelineBuilder.build(request.clone());
    let command = pipeline.command(&request.inputs);

    insta::assert_snapshot!(command.arguments.join(" "));
}

#[test]
fn amf_pipeline_snapshot() {
    let desired = DesiredVideoState::h264_1080p();
    let request = h264_request(desired);

    let pipeline = AmfPipelineBuilder.build(request.clone());
    let command = pipeline.command(&request.inputs);

    insta::assert_snapshot!(command.arguments.join(" "));
}

#[test]
fn v4l2m2m_pipeline_snapshot() {
    let desired = DesiredVideoState::h264_1080p();
    let request = h264_request(desired);

    let pipeline = V4l2m2mPipelineBuilder.build(request.clone());
    let command = pipeline.command(&request.inputs);

    insta::assert_snapshot!(command.arguments.join(" "));
}

#[test]
fn rkmpp_pipeline_snapshot() {
    let desired = DesiredVideoState::h264_1080p();
    let request = h264_request(desired);

    let pipeline = RkmppPipelineBuilder.build(request.clone());
    let command = pipeline.command(&request.inputs);

    insta::assert_snapshot!(command.arguments.join(" "));
}

#[test]
fn filter_graph_with_watermark_subtitle_and_drawvg_snapshot() {
    let mut inputs = vec![
        InputFile::video("/media/movie.mkv"),
        InputFile::audio("/media/movie.mkv"),
        InputFile::watermark("/media/logo.png"),
        InputFile::image_subtitle("/media/subtitle.sup", ersatzrs_ffmpeg::SubtitleMethod::Burn),
    ];
    inputs[2]
        .filter_steps
        .push(ersatzrs_ffmpeg::watermark_opacity_filter(75));

    let overlay = DrawvgOverlay {
        script_path: "/tmp/ErsatzRS/lower-third.vgs".into(),
        x: 0,
        y: 810,
        width: 1920,
        height: 270,
        enable: Some("between(t,5,12)".to_owned()),
    };
    let chain = FilterChain {
        watermark_overlay: vec![ersatzrs_ffmpeg::watermark_overlay_filter(
            &WatermarkOverlay {
                location: WatermarkLocation::TopRight,
                horizontal_margin_percent: 2.0,
                vertical_margin_percent: 3.0,
                opacity: 75,
                ..WatermarkOverlay::default()
            },
            FrameSize::new(1920, 1080),
            FrameSize::new(1920, 1080),
            ersatzrs_ffmpeg::PixelFormat::Yuv420P,
        )],
        subtitle_overlay: vec![ersatzrs_ffmpeg::subtitle_overlay_filter(
            ersatzrs_ffmpeg::PixelFormat::Yuv420P,
        )],
        graphics_overlay: vec![GraphicsOverlayFilter::Drawvg(overlay.filter())],
        ..FilterChain::default()
    };

    let filter = ComplexFilter::build(&inputs, chain, false);

    insta::assert_snapshot!(filter.options.join(" "));
}

#[test]
fn pipeline_with_scaled_permanent_watermark_snapshot() {
    let mut request = h264_request(DesiredVideoState::h264_1080p());
    request.inputs.push(InputFile::watermark("/media/logo.png"));
    request.watermarks.push(WatermarkOverlay {
        location: WatermarkLocation::TopRight,
        size: WatermarkSize::Scaled,
        width_percent: 12.5,
        horizontal_margin_percent: 2.0,
        vertical_margin_percent: 3.0,
        opacity: 75,
        ..WatermarkOverlay::default()
    });

    let pipeline = SoftwarePipelineBuilder.build(request.clone());
    let command = pipeline.command(&request.inputs);

    insta::assert_snapshot!(command.arguments.join(" "));
}

#[test]
fn pipeline_with_intermittent_watermark_snapshot() {
    let mut request = h264_request(DesiredVideoState::h264_1080p());
    // Fading a still image requires `-loop 1` (ErsatzTV `InfiniteLoopInputOption`).
    request
        .inputs
        .push(InputFile::watermark_looped("/media/logo.png"));
    request.watermarks.push(WatermarkOverlay {
        opacity: 80,
        fade_points: vec![
            WatermarkFadePoint {
                kind: WatermarkFadeKind::In,
                time: Duration::from_secs(0),
                enable_start: Duration::from_secs(0),
                enable_finish: Duration::from_secs(1),
            },
            WatermarkFadePoint {
                kind: WatermarkFadeKind::Out,
                time: Duration::from_secs(16),
                enable_start: Duration::from_secs(16),
                enable_finish: Duration::from_secs(17),
            },
        ],
        ..WatermarkOverlay::default()
    });

    let pipeline = SoftwarePipelineBuilder.build(request.clone());
    let command = pipeline.command(&request.inputs);

    insta::assert_snapshot!(command.arguments.join(" "));
}

#[test]
fn pipeline_with_opacity_expression_watermark_snapshot() {
    let mut request = h264_request(DesiredVideoState::h264_1080p()).with_watermark_opacity_clock(
        WatermarkOpacityClock::new(86_390.0, 95.0, Some(7_200.0), 3_600.0),
    );
    request.inputs.push(InputFile::watermark("/media/logo.png"));
    request.watermarks.push(WatermarkOverlay {
        opacity_expression: Some(
            "if(content_seconds < content_total_seconds && time_of_day_seconds % 60 < 1, 1, channel_seconds / 7200)".to_owned(),
        ),
        ..WatermarkOverlay::default()
    });

    let pipeline = SoftwarePipelineBuilder.build(request.clone());
    let command = pipeline.command(&request.inputs);

    insta::assert_snapshot!(command.arguments.join(" "));
}

#[test]
fn pipeline_with_multiple_watermarks_snapshot() {
    let mut request = h264_request(DesiredVideoState::h264_1080p());
    request.inputs.push(InputFile::watermark("/media/back.png"));
    request
        .inputs
        .push(InputFile::watermark("/media/front.png"));
    request.watermarks.push(WatermarkOverlay {
        location: WatermarkLocation::TopLeft,
        z_index: 10,
        ..WatermarkOverlay::default()
    });
    request.watermarks.push(WatermarkOverlay {
        location: WatermarkLocation::BottomRight,
        z_index: 20,
        opacity: 50,
        ..WatermarkOverlay::default()
    });

    let pipeline = SoftwarePipelineBuilder.build(request.clone());
    let command = pipeline.command(&request.inputs);

    insta::assert_snapshot!(command.arguments.join(" "));
}

// `FFmpegLibraryProcessService` routing parity: a single permanent watermark
// (here the resource-backed song-progress overlay shape — full-width scaled,
// top-left, no margins) stays on the native ffmpeg watermark path.
#[test]
fn pipeline_keeps_single_permanent_resource_watermark_native_snapshot() {
    let mut request = h264_request(DesiredVideoState::h264_1080p());
    request.inputs.push(InputFile::watermark(
        "/data/cache/resources/song_progress_overlay.png",
    ));
    request.watermarks.push(WatermarkOverlay {
        location: WatermarkLocation::TopLeft,
        size: WatermarkSize::Scaled,
        width_percent: 100.0,
        horizontal_margin_percent: 0.0,
        vertical_margin_percent: 0.0,
        opacity: 100,
        ..WatermarkOverlay::default()
    });

    let pipeline = SoftwarePipelineBuilder.build(request.clone());
    let command = pipeline.command(&request.inputs);

    insta::assert_snapshot!(command.arguments.join(" "));
}

// `FFmpegLibraryProcessService` routing parity: when graphics elements are
// present, even a single permanent watermark joins the graphics-engine plan and
// composites with the elements in ascending z-index (drawvg z=5 below the
// watermark image at z=10).
#[test]
fn pipeline_composites_watermark_with_graphics_elements_by_z_index_snapshot() {
    let mut request = h264_request(DesiredVideoState::h264_1080p());
    request
        .graphics_overlays
        .push(GraphicsPipelineOverlay::drawvg(
            5,
            DrawvgOverlay {
                script_path: "/tmp/ErsatzRS/title.vgs".into(),
                x: 0,
                y: 0,
                width: 1920,
                height: 1080,
                enable: None,
            }
            .filter(),
        ));
    request.inputs.push(InputFile::watermark("/media/logo.png"));
    request.watermarks.push(WatermarkOverlay {
        location: WatermarkLocation::TopRight,
        size: WatermarkSize::Scaled,
        width_percent: 12.5,
        horizontal_margin_percent: 2.0,
        vertical_margin_percent: 3.0,
        opacity: 75,
        z_index: 10,
        ..WatermarkOverlay::default()
    });

    let pipeline = SoftwarePipelineBuilder.build(request.clone());
    let command = pipeline.command(&request.inputs);

    insta::assert_snapshot!(command.arguments.join(" "));
}

// Watermark inputs must sort together with their overlays: front.png (z=10,
// 10% wide) must receive the narrow scale filter and composite below back.png
// (z=20, 25% wide) even though back.png was attached first.
#[test]
fn pipeline_orders_multiple_watermarks_by_z_index_snapshot() {
    let mut request = h264_request(DesiredVideoState::h264_1080p());
    request.inputs.push(InputFile::watermark("/media/back.png"));
    request
        .inputs
        .push(InputFile::watermark("/media/front.png"));
    request.watermarks.push(WatermarkOverlay {
        location: WatermarkLocation::BottomRight,
        size: WatermarkSize::Scaled,
        width_percent: 25.0,
        z_index: 20,
        ..WatermarkOverlay::default()
    });
    request.watermarks.push(WatermarkOverlay {
        location: WatermarkLocation::TopLeft,
        size: WatermarkSize::Scaled,
        width_percent: 10.0,
        z_index: 10,
        ..WatermarkOverlay::default()
    });

    let pipeline = SoftwarePipelineBuilder.build(request.clone());
    let command = pipeline.command(&request.inputs);

    insta::assert_snapshot!(command.arguments.join(" "));
}

// ErsatzTV `SetWatermark` loop-option parity: a plain still watermark gets no
// loop option, a fading still watermark gets `-loop 1`, and an animated
// watermark gets `-ignore_loop 0` (`DoNotIgnoreLoopInputOption`).
#[test]
fn watermark_inputs_carry_setwatermark_loop_options() {
    assert!(InputFile::watermark("/media/logo.png")
        .input_options
        .is_empty());
    assert_eq!(
        InputFile::watermark_looped("/media/logo.png").input_options,
        ["-loop", "1"]
    );
    assert_eq!(
        InputFile::watermark_animated("/media/logo.gif").input_options,
        ["-ignore_loop", "0"]
    );
}

// ErsatzTV opacity expressions are NCalc: infix `%`, comparisons, and boolean
// operators must become FFmpeg eval functions (`mod`, `lt`, `min`/`max`), and
// `geq` exposes the frame time as uppercase `T`. Raw NCalc syntax fails geq
// initialization at runtime ("Missing ')' or too many args").
#[test]
fn opacity_expression_translation_maps_ncalc_to_ffmpeg_eval() {
    assert_eq!(
        translate_watermark_opacity_expression(
            "if(time_of_day_seconds < content_seconds, content_total_seconds, channel_seconds)"
        ),
        "if(lt(mod((0+T),86400),T),NAN,T)"
    );
    assert_eq!(
        translate_watermark_opacity_expression("if(time_of_day_seconds % 60 < 1, 1, 0.25)"),
        "if(lt(mod(mod((0+T),86400),60),1),1,0.25)"
    );
    assert_eq!(
        translate_watermark_opacity_expression(
            "if(time_of_day_seconds >= 10 && time_of_day_seconds <= 20, 1, 0)"
        ),
        "if(min(gte(mod((0+T),86400),10),lte(mod((0+T),86400),20)),1,0)"
    );
    assert_eq!(
        translate_watermark_opacity_expression("1 - time_of_day_seconds / 60"),
        "(1-(mod((0+T),86400)/60))"
    );
}

#[test]
fn opacity_expression_translation_maps_ersatztv_clock_context() {
    let clock = WatermarkOpacityClock::new(86_390.0, 45.0, Some(3_661.5), 120.0);
    assert_eq!(
        translate_watermark_opacity_expression_with_clock(
            "if(time_of_day_seconds < content_seconds, content_total_seconds, channel_seconds)",
            &clock,
        ),
        "if(lt(mod((86390+T),86400),(45+T)),3661.5,(120+T))"
    );
    assert_eq!(
        translate_watermark_opacity_expression_with_clock(
            "content_seconds + content_total_seconds + channel_seconds",
            &clock,
        ),
        "(((45+T)+3661.5)+(120+T))"
    );
}

#[test]
fn opacity_expression_time_of_day_wraps_after_midnight() {
    let clock = WatermarkOpacityClock::new(86_399.25, 0.0, Some(10.0), 0.0);
    assert_eq!(
        translate_watermark_opacity_expression_with_clock("time_of_day_seconds", &clock),
        "mod((86399.25+T),86400)"
    );
}

// ErsatzTV registers `LinearFadePoints` / `LinearFadeDuration` as custom NCalc
// functions (`OpacityExpressionHelper.EvaluateFunction`); both are clipped
// trapezoids, so they translate to FFmpeg eval `clip(min(...),0,1)` forms
// instead of passing through as undefined function names.
#[test]
fn opacity_expression_translation_maps_custom_fade_functions() {
    assert_eq!(
        translate_watermark_opacity_expression("LinearFadePoints(content_seconds, 0, 2, 8, 10)"),
        "clip(min(((T)-(0))/((2)-(0)),((10)-(T))/((10)-(8))),0,1)"
    );
    assert_eq!(
        translate_watermark_opacity_expression("LinearFadeDuration(content_seconds, 0, 2, 6)"),
        "if(lte((2),0),gte((T),(0))*lt((T),(0)+(6)),clip(min(((T)-(0))/(2),((0)+2*(2)+(6)-(T))/(2)),0,1))"
    );
    let clock = WatermarkOpacityClock::new(0.0, 45.0, Some(120.0), 0.0);
    assert_eq!(
        translate_watermark_opacity_expression_with_clock(
            "LinearFadePoints(content_seconds, 0, 2, 8, 10)",
            &clock,
        ),
        "clip(min((((45+T))-(0))/((2)-(0)),((10)-((45+T)))/((10)-(8))),0,1)"
    );
}

#[test]
fn drawvg_lower_third_template_snapshot() {
    let template = SvgTemplate::new(include_str!("../../../assets/graphics/lower-third.svg"));
    let mut variables = BTreeMap::new();
    variables.insert("title".to_owned(), "Big Buck Bunny".to_owned());
    variables.insert("subtitle".to_owned(), "Now Playing".to_owned());

    insta::assert_snapshot!(template.render(&variables));

    let overlay = lower_third_overlay(
        "/tmp/ErsatzRS/lower-third.vgs",
        FrameSize::new(1920, 1080),
        Some("between(t,0,8)".to_owned()),
    );
    insta::assert_snapshot!("drawvg_filter", overlay.filter());
}

#[test]
fn concat_playlist_snapshot() {
    let playlist = ConcatPlaylist::new("http", "localhost:8409", "101", "segmenter");

    insta::assert_snapshot!(playlist.render());
}

#[test]
fn song_progress_filter_snapshot() {
    let filter = song_progress_filter(
        FrameSize::new(1920, 1080),
        Some(Duration::from_secs(30)),
        Some(Duration::from_mins(2)),
    );

    insta::assert_snapshot!(filter);
}

#[test]
fn normalize_loudness_filter_default_snapshot() {
    let filter = normalize_loudness_filter(AudioFilter::LoudNorm, None, None);

    insta::assert_snapshot!(filter);
}

#[test]
fn normalize_loudness_filter_custom_snapshot() {
    let filter = normalize_loudness_filter(AudioFilter::LoudNorm, Some(-23.0), Some(44));

    insta::assert_snapshot!(filter);
}

#[test]
fn audio_resample_filter_snapshot() {
    insta::assert_snapshot!(audio_resample_filter());
}

#[test]
fn audio_pad_filter_snapshot() {
    insta::assert_snapshot!(audio_pad_filter());
}

#[test]
fn subtitle_scale_npp_filter_snapshot() {
    insta::assert_snapshot!(subtitle_scale_npp_filter(FrameSize::new(1920, 1080)));
}

#[test]
fn yadif_filter_software_snapshot() {
    insta::assert_snapshot!(yadif_filter(&software_state()));
}

#[test]
fn yadif_filter_hardware_nv12_snapshot() {
    insta::assert_snapshot!(yadif_filter(&hardware_state(PixelFormat::Nv12)));
}

#[test]
fn yadif_cuda_filter_software_snapshot() {
    insta::assert_snapshot!(yadif_cuda_filter(&software_state()));
}

#[test]
fn yadif_cuda_filter_hardware_snapshot() {
    insta::assert_snapshot!(yadif_cuda_filter(&hardware_state(PixelFormat::Nv12)));
}

#[test]
fn deinterlace_qsv_filter_software_snapshot() {
    insta::assert_snapshot!(deinterlace_qsv_filter(&software_state(), 64));
}

#[test]
fn deinterlace_qsv_filter_hardware_snapshot() {
    insta::assert_snapshot!(deinterlace_qsv_filter(
        &hardware_state(PixelFormat::Nv12),
        64
    ));
}

#[test]
fn deinterlace_vaapi_filter_software_snapshot() {
    insta::assert_snapshot!(deinterlace_vaapi_filter(&software_state()));
}

#[test]
fn deinterlace_vaapi_filter_hardware_snapshot() {
    insta::assert_snapshot!(deinterlace_vaapi_filter(&hardware_state(
        PixelFormat::Vaapi
    )));
}

#[test]
fn deinterlace_filter_dispatch_snapshots() {
    let cases = [
        (HardwareAccelerationKind::None, software_state()),
        (
            HardwareAccelerationKind::Nvenc,
            hardware_state(PixelFormat::Nv12),
        ),
        (HardwareAccelerationKind::Qsv, software_state()),
        (
            HardwareAccelerationKind::Vaapi,
            hardware_state(PixelFormat::Vaapi),
        ),
    ];
    let rendered = cases
        .iter()
        .map(|(accel, state)| format!("{accel:?} => {}", deinterlace_filter(*accel, state, 64)))
        .collect::<Vec<_>>()
        .join("\n");

    insta::assert_snapshot!(rendered);
}

#[test]
fn tonemap_filter_software_snapshot() {
    insta::assert_snapshot!(tonemap_filter(
        &software_state(),
        PixelFormat::Yuv420P,
        TonemapAlgorithm::Hable,
    ));
}

#[test]
fn tonemap_filter_software_from_hardware_snapshot() {
    insta::assert_snapshot!(tonemap_filter(
        &hardware_state(PixelFormat::Cuda),
        PixelFormat::Yuv420P,
        TonemapAlgorithm::Hable,
    ));
}

#[test]
fn tonemap_cuda_filter_eight_bit_snapshot() {
    insta::assert_snapshot!(tonemap_cuda_filter(
        PixelFormat::Yuv420P,
        TonemapAlgorithm::Hable,
    ));
}

#[test]
fn tonemap_cuda_filter_ten_bit_snapshot() {
    insta::assert_snapshot!(tonemap_cuda_filter(
        PixelFormat::Yuv420P10Le,
        TonemapAlgorithm::Hable,
    ));
}

#[test]
fn tonemap_qsv_filter_snapshot() {
    insta::assert_snapshot!(tonemap_qsv_filter());
}

#[test]
fn tonemap_vaapi_filter_snapshot() {
    insta::assert_snapshot!(tonemap_vaapi_filter(TonemapAlgorithm::Hable));
}

#[test]
fn colorspace_filter_unknown_software_snapshot() {
    let params = ColorParams {
        is_unknown: true,
        ..ColorParams::default()
    };
    insta::assert_snapshot!(colorspace_filter(
        &software_state(),
        &params,
        PixelFormat::Yuv420P,
        false,
        false,
    ));
}

#[test]
fn colorspace_filter_bt2020_eight_bit_snapshot() {
    let params = ColorParams {
        is_bt2020_ten: true,
        color_space: Some("bt2020nc".to_owned()),
        color_transfer: Some("smpte2084".to_owned()),
        color_primaries: Some("bt2020".to_owned()),
        color_range: Some("tv".to_owned()),
        ..ColorParams::default()
    };
    insta::assert_snapshot!(colorspace_filter(
        &hardware_state(PixelFormat::Cuda),
        &params,
        PixelFormat::Yuv420P,
        false,
        false,
    ));
}

#[test]
fn colorspace_filter_bt470_bg_snapshot() {
    let params = ColorParams {
        is_bt470_bg: true,
        ..ColorParams::default()
    };
    insta::assert_snapshot!(colorspace_filter(
        &software_state(),
        &params,
        PixelFormat::Yuv420P,
        false,
        false,
    ));
}

#[test]
fn colorspace_filter_mixed_input_overrides_snapshot() {
    let params = ColorParams {
        is_mixed: true,
        color_range: Some("tv".to_owned()),
        color_space: Some("bt709".to_owned()),
        color_transfer: Some("reserved".to_owned()),
        color_primaries: Some(String::new()),
        ..ColorParams::default()
    };
    insta::assert_snapshot!(colorspace_filter(
        &software_state(),
        &params,
        PixelFormat::Yuv420P,
        true,
        false,
    ));
}

#[test]
fn hardware_upload_filter_snapshots() {
    let rendered = [
        HardwareAccelerationKind::None,
        HardwareAccelerationKind::Nvenc,
        HardwareAccelerationKind::Qsv,
        HardwareAccelerationKind::Vaapi,
        HardwareAccelerationKind::Amf,
        HardwareAccelerationKind::VideoToolbox,
        HardwareAccelerationKind::V4l2m2m,
        HardwareAccelerationKind::Rkmpp,
    ]
    .into_iter()
    .map(|accel| format!("{accel:?} => {}", hardware_upload_filter(accel, 64)))
    .collect::<Vec<_>>()
    .join("\n");

    insta::assert_snapshot!(rendered);
}

#[test]
fn cuda_software_fallback_upload_filter_snapshot() {
    insta::assert_snapshot!(cuda_software_fallback_upload_filter());
}

#[test]
fn cuda_format_filter_snapshot() {
    insta::assert_snapshot!(cuda_format_filter(PixelFormat::Nv12));
}

#[test]
fn hardware_download_filter_snapshots() {
    let rendered = [
        ("software", software_state()),
        ("vaapi", hardware_state(PixelFormat::Vaapi)),
        ("cuda", hardware_state(PixelFormat::Cuda)),
        ("nv12", hardware_state(PixelFormat::Nv12)),
    ]
    .into_iter()
    .map(|(name, state)| format!("{name} => {}", hardware_download_filter(&state)))
    .collect::<Vec<_>>()
    .join("\n");

    insta::assert_snapshot!(rendered);
}

#[test]
fn frame_rate_filter_software_snapshot() {
    insta::assert_snapshot!(frame_rate_filter(&software_state(), FrameRate::new(60, 1)));
}

#[test]
fn frame_rate_filter_hardware_snapshot() {
    insta::assert_snapshot!(frame_rate_filter(
        &hardware_state(PixelFormat::Nv12),
        FrameRate::new(60, 1),
    ));
}

#[test]
fn pad_vaapi_filter_snapshots() {
    let rendered = [
        ("software", software_state()),
        ("hardware", hardware_state(PixelFormat::Vaapi)),
    ]
    .into_iter()
    .map(|(name, state)| {
        format!(
            "{name} => {}",
            pad_vaapi_filter(&state, FrameSize::new(1920, 1080))
        )
    })
    .collect::<Vec<_>>()
    .join("\n");

    insta::assert_snapshot!(rendered);
}

#[test]
fn pipeline_emits_deinterlace_for_interlaced_source_snapshot() {
    let mut source = SourceVideoStream::new(SourceVideoCodec::H264);
    source.scan_kind = VideoScanKind::Interlaced;
    let request = PipelineRequest::new(
        InputFile::video("/media/movie.mkv"),
        DesiredVideoState::h264_1080p(),
    )
    .with_source_video_stream(source);

    let pipeline = SoftwarePipelineBuilder.build(request.clone());
    let command = pipeline.command(&request.inputs);

    insta::assert_snapshot!(command.arguments.join(" "));
}

#[test]
fn pipeline_skips_deinterlace_when_profile_flag_disabled_snapshot() {
    let mut source = source_1080p(SourceVideoCodec::H264, 1920, 1080);
    source.scan_kind = VideoScanKind::Interlaced;
    let profile = FFmpegProfile {
        video_format: FFmpegProfileVideoFormat::H264,
        audio_format: FFmpegProfileAudioFormat::Aac,
        deinterlace_video: Some(false),
        ..FFmpegProfile::default()
    };
    let resolution = Resolution {
        width: 1920,
        height: 1080,
        ..Resolution::default()
    };
    let desired = DesiredVideoState::from_profile(
        &profile,
        &resolution,
        Some(&source),
        Some(FrameRate::DEFAULT),
        StreamingMode::TransportStream,
    );
    let request = PipelineRequest::new(InputFile::video("/media/movie.mkv"), desired)
        .with_source_video_stream(source);

    let pipeline = SoftwarePipelineBuilder.build(request.clone());
    let arguments = pipeline.command(&request.inputs).arguments.join(" ");

    assert!(
        !arguments.contains("yadif"),
        "profile-disabled deinterlace must not emit yadif: {arguments}"
    );
    insta::assert_snapshot!(arguments);
}

fn sd_source_with_colors(
    range: Option<&str>,
    space: Option<&str>,
    transfer: Option<&str>,
    primaries: Option<&str>,
) -> SourceVideoStream {
    let mut source = SourceVideoStream::new(SourceVideoCodec::H264);
    source.frame_size = Some(FrameSize::new(720, 480));
    source.sample_aspect_ratio = Some("1:1".to_owned());
    source.display_aspect_ratio = Some("720:480".to_owned());
    source.scan_kind = VideoScanKind::Interlaced;
    source.pixel_format = Some(PixelFormat::Yuv420P);
    source.color_params = SourceColorParams {
        color_range: range.map(str::to_owned),
        color_space: space.map(str::to_owned),
        color_transfer: transfer.map(str::to_owned),
        color_primaries: primaries.map(str::to_owned),
        is_hdr: false,
        is_bt2020_ten: false,
    };
    source
}

fn sd_software_pipeline_args(source: SourceVideoStream) -> String {
    let request = PipelineRequest::new(
        InputFile::video("/media/dvd-source.mkv"),
        DesiredVideoState::h264_1080p(),
    )
    .with_source_video_stream(source);
    let pipeline = SoftwarePipelineBuilder.build(request.clone());
    pipeline.command(&request.inputs).arguments.join(" ")
}

fn sd_hardware_pipeline_args(
    source: SourceVideoStream,
    acceleration: HardwareAccelerationKind,
) -> String {
    let request = PipelineRequest::new(
        InputFile::video("/media/dvd-source.mkv"),
        DesiredVideoState::h264_1080p(),
    )
    .with_source_video_stream(source);

    let pipeline = match acceleration {
        HardwareAccelerationKind::Nvenc => NvencPipelineBuilder.build(request.clone()),
        HardwareAccelerationKind::Vaapi => VaapiPipelineBuilder.build(request.clone()),
        HardwareAccelerationKind::Qsv => QsvPipelineBuilder.build(request.clone()),
        _ => unreachable!("test only covers GPU-resident hardware builders"),
    };
    pipeline.command(&request.inputs).arguments.join(" ")
}

fn set_pixel_format_segment(filter_complex: &str) -> &str {
    let segment = filter_complex
        .trim()
        .split(';')
        .find(|segment| segment.ends_with("[vpf]"))
        .expect("filter graph contains SetPixelFormat segment");
    segment
        .strip_suffix("[vpf]")
        .expect("SetPixelFormat segment has vpf output label")
        .rsplit_once(']')
        .map_or(segment, |(_, filters)| filters)
}

fn command_set_pixel_format_segment(args: &str) -> &str {
    let filter_complex = args
        .split(" -filter_complex ")
        .nth(1)
        .expect("command contains filter_complex")
        .split(" -map ")
        .next()
        .expect("filter_complex is followed by maps");
    set_pixel_format_segment(filter_complex)
}

fn assert_matches_ersatztv_set_pixel_format(args: &str, fixture: &str) {
    let expected = set_pixel_format_segment(fixture);
    let actual = command_set_pixel_format_segment(args);
    assert_eq!(
        actual, expected,
        "expected SetPixelFormat segment from ErsatzTV fixture"
    );
}

#[test]
fn pipeline_normalizes_bt470bg_source_to_bt709_software_snapshot() {
    // Port of `SetPixelFormat`: a bt470bg (SD) source is normalized to bt709
    // with `colormatrix` (ErsatzTV uses colormatrix, not colorspace, for
    // bt470bg). This is the fix for the GIF-like banding/dither on DVD footage.
    let source = sd_source_with_colors(
        Some("tv"),
        Some("bt470bg"),
        Some("bt470bg"),
        Some("bt470bg"),
    );
    let args = sd_software_pipeline_args(source);

    assert!(
        args.contains("colormatrix=src=bt470bg:dst=bt709,format=yuv420p"),
        "expected bt470bg colormatrix normalization, got: {args}"
    );
    assert!(
        !args.contains("-pix_fmt"),
        "colorspace filter already pins the format, got: {args}"
    );
    insta::assert_snapshot!(args);
}

#[test]
fn nvenc_pipeline_normalizes_bt470bg_source_to_bt709_hardware_snapshot() {
    let source = sd_source_with_colors(
        Some("tv"),
        Some("bt470bg"),
        Some("bt470bg"),
        Some("bt470bg"),
    );
    let args = sd_hardware_pipeline_args(source, HardwareAccelerationKind::Nvenc);

    assert_matches_ersatztv_set_pixel_format(
        &args,
        include_str!("fixtures/ersatztv/colorspace/nvenc-bt470bg-filter-complex.txt"),
    );
    insta::assert_snapshot!(args);
}

#[test]
fn vaapi_pipeline_normalizes_bt470bg_source_to_bt709_hardware_snapshot() {
    let source = sd_source_with_colors(
        Some("tv"),
        Some("bt470bg"),
        Some("bt470bg"),
        Some("bt470bg"),
    );
    let args = sd_hardware_pipeline_args(source, HardwareAccelerationKind::Vaapi);

    assert_matches_ersatztv_set_pixel_format(
        &args,
        include_str!("fixtures/ersatztv/colorspace/vaapi-bt470bg-filter-complex.txt"),
    );
    insta::assert_snapshot!(args);
}

#[test]
fn qsv_pipeline_normalizes_mixed_source_to_bt709_snapshot() {
    let source = sd_source_with_colors(
        Some("tv"),
        Some("smpte170m"),
        Some("bt709"),
        Some("smpte170m"),
    );
    let args = sd_hardware_pipeline_args(source, HardwareAccelerationKind::Qsv);

    assert_matches_ersatztv_set_pixel_format(
        &args,
        include_str!("fixtures/ersatztv/colorspace/qsv-mixed-filter-complex.txt"),
    );
    insta::assert_snapshot!(args);
}

#[test]
fn nvenc_pipeline_skips_colorspace_for_bt709_source_hardware() {
    let source = sd_source_with_colors(Some("tv"), Some("bt709"), Some("bt709"), Some("bt709"));
    let args = sd_hardware_pipeline_args(source, HardwareAccelerationKind::Nvenc);

    assert!(!args.contains("colorspace="), "got: {args}");
    assert!(!args.contains("colormatrix="), "got: {args}");
    assert!(!args.contains("setparams="), "got: {args}");
}

#[test]
fn pipeline_normalizes_mixed_colors_source_software_snapshot() {
    // A mixed-tag SD source (smpte170m primaries/space, bt709 transfer) gets the
    // general `colorspace` filter with input overrides + bt709 output.
    let source = sd_source_with_colors(
        Some("tv"),
        Some("smpte170m"),
        Some("bt709"),
        Some("smpte170m"),
    );
    let args = sd_software_pipeline_args(source);

    assert!(
        args.contains(
            "colorspace=irange=tv:ispace=smpte170m:itrc=bt709:iprimaries=smpte170m:all=bt709:format=yuv420p:range=tv"
        ),
        "expected mixed-color colorspace normalization, got: {args}"
    );
    insta::assert_snapshot!(args);
}

#[test]
fn pipeline_normalizes_unknown_colors_source_software_snapshot() {
    // A source missing space/transfer/primaries tags is normalized with
    // `setparams` (ErsatzTV's unknown-color branch); the format is unchanged so
    // the existing `-pix_fmt` equality logic still applies.
    let source = sd_source_with_colors(Some("tv"), None, None, None);
    let args = sd_software_pipeline_args(source);

    assert!(
        args.contains("setparams=range=tv:colorspace=bt709:color_trc=bt709:color_primaries=bt709"),
        "expected unknown-color setparams normalization, got: {args}"
    );
    insta::assert_snapshot!(args);
}

#[test]
fn pipeline_skips_colorspace_for_bt709_source_software() {
    // An already-bt709 source needs no normalization (ErsatzTV `IsBt709` gate),
    // so no colorspace/colormatrix/setparams node is emitted.
    let source = sd_source_with_colors(Some("tv"), Some("bt709"), Some("bt709"), Some("bt709"));
    let args = sd_software_pipeline_args(source);

    assert!(!args.contains("colorspace="), "got: {args}");
    assert!(!args.contains("colormatrix="), "got: {args}");
    assert!(!args.contains("setparams="), "got: {args}");
}

#[test]
fn pipeline_emits_tonemap_for_hdr_source_snapshot() {
    let mut source = SourceVideoStream::new(SourceVideoCodec::Hevc);
    source.color_params = SourceColorParams {
        is_hdr: true,
        is_bt2020_ten: true,
        ..SourceColorParams::default()
    };
    let request = PipelineRequest::new(
        InputFile::video("/media/hdr-movie.mkv"),
        DesiredVideoState::h264_1080p(),
    )
    .with_source_video_stream(source);

    let pipeline = NvencPipelineBuilder.build(request.clone());
    let command = pipeline.command(&request.inputs);
    let args = command.arguments.join(" ");

    assert!(
        !args.contains("[v]colorspace=") && !args.contains("[v]colormatrix="),
        "tonemap already resets color params to BT.709 before SetPixelFormat, got: {args}"
    );
    insta::assert_snapshot!(args);
}

#[test]
fn nvenc_pipeline_uses_vulkan_hwaccel_for_hdr_tonemap_snapshot() {
    let mut source = SourceVideoStream::new(SourceVideoCodec::Hevc);
    source.color_params = SourceColorParams {
        is_hdr: true,
        is_bt2020_ten: false,
        ..SourceColorParams::default()
    };
    let mut report = capability_report_for_tests();
    report.hwaccels.insert("vulkan".to_owned());
    let request = PipelineRequest::new(
        InputFile::video("/media/hdr-movie.mkv"),
        DesiredVideoState::h264_1080p(),
    )
    .with_source_video_stream(source)
    .with_runtime_capabilities(report);

    let pipeline = NvencPipelineBuilder.build(request.clone());
    let command = pipeline.command(&request.inputs);

    insta::assert_snapshot!(command.arguments.join(" "));
}

#[test]
fn nvenc_hdr_downscale_with_watermark_normalizes_cuda_overlay_surface_snapshot() {
    let mut source = source_1080p(SourceVideoCodec::Hevc, 3840, 2160);
    source.pixel_format = Some(PixelFormat::Yuv420P10Le);
    source.color_params = SourceColorParams {
        is_hdr: true,
        is_bt2020_ten: true,
        ..SourceColorParams::default()
    };
    let mut report = capability_report_for_tests();
    report.hwaccels.insert("vulkan".to_owned());
    let mut request = PipelineRequest::new(
        InputFile::video("/media/hdr-movie.mkv"),
        DesiredVideoState::h264_1080p(),
    )
    .with_source_video_stream(source)
    .with_runtime_capabilities(report);
    request.inputs.push(InputFile::watermark("/media/logo.png"));
    request.watermarks.push(WatermarkOverlay {
        location: WatermarkLocation::TopRight,
        size: WatermarkSize::Scaled,
        width_percent: 8.0,
        opacity: 35,
        ..WatermarkOverlay::default()
    });

    let pipeline = NvencPipelineBuilder.build(request.clone());
    let command = pipeline.command(&request.inputs);
    let arguments = command.arguments.join(" ");

    let nv12_scale = arguments
        .find("scale_cuda=1920:1080:format=nv12")
        .expect("HDR downscale must produce an nv12 CUDA surface");
    let overlay_format = arguments
        .find("scale_cuda=format=yuv420p")
        .expect("CUDA overlay main input must be converted to yuv420p");
    let overlay = arguments
        .find("overlay_cuda=")
        .expect("watermark must use CUDA overlay");
    assert!(nv12_scale < overlay_format && overlay_format < overlay);
    assert!(arguments.contains("format=yuva420p,hwupload_cuda[wm0]"));
    insta::assert_snapshot!(arguments);
}

#[test]
fn software_pipeline_uses_output_frame_rate_when_source_differs_snapshot() {
    let mut source = SourceVideoStream::new(SourceVideoCodec::H264);
    source.frame_rate = Some(FrameRate::new(25, 1));
    let mut desired = DesiredVideoState::h264_1080p();
    desired.frame_rate = Some(FrameRate::new(60, 1));
    let request = PipelineRequest::new(InputFile::video("/media/movie.mkv"), desired)
        .with_source_video_stream(source);

    let pipeline = SoftwarePipelineBuilder.build(request.clone());
    let command = pipeline.command(&request.inputs);
    let arguments = command.arguments.join(" ");

    assert!(arguments.contains("-r 60/1 -fps_mode:v cfr"));
    assert!(!arguments.contains("-vsync"));
    assert!(!arguments.contains("framerate=fps=60/1"));
    insta::assert_snapshot!(arguments);
}

#[test]
fn graphics_template_renderer_writes_svg() {
    let output_dir =
        std::env::temp_dir().join(format!("ersatzrs-ffmpeg-test-{}", std::process::id()));
    let renderer = GraphicsTemplateRenderer::new(&output_dir);
    let mut variables = BTreeMap::new();
    variables.insert("title".to_owned(), "A & B".to_owned());
    variables.insert("subtitle".to_owned(), "<Tonight>".to_owned());

    let template_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../assets/graphics/lower-third.svg"
    );
    let output_path = renderer
        .render_template_file(template_path, &variables)
        .expect("template renders");

    let svg_text = std::fs::read_to_string(output_path).expect("rendered svg can be read");
    assert!(svg_text.contains("A &amp; B"));
    assert!(svg_text.contains("&lt;Tonight&gt;"));
    std::fs::remove_dir_all(output_dir).expect("test output dir can be removed");
}

#[test]
fn graphics_assets_render_drawvg_and_ass_templates() {
    let mut variables = BTreeMap::new();
    variables.insert("background_color".to_owned(), "#101820@0.88".to_owned());
    variables.insert("safe_x".to_owned(), "96".to_owned());
    variables.insert("safe_y".to_owned(), "812".to_owned());
    variables.insert("box_width".to_owned(), "880".to_owned());
    variables.insert("box_height".to_owned(), "132".to_owned());
    variables.insert("accent_color".to_owned(), "#ffcc00".to_owned());
    variables.insert("accent_width".to_owned(), "10".to_owned());
    variables.insert("subtitle_color".to_owned(), "#000000@0.35".to_owned());
    variables.insert("subtitle_y".to_owned(), "900".to_owned());
    variables.insert("subtitle_width".to_owned(), "620".to_owned());
    variables.insert("subtitle_height".to_owned(), "28".to_owned());
    variables.insert("track_color".to_owned(), "#202020@0.7".to_owned());
    variables.insert("track_x".to_owned(), "128".to_owned());
    variables.insert("track_y".to_owned(), "1010".to_owned());
    variables.insert("track_width".to_owned(), "720".to_owned());
    variables.insert("track_height".to_owned(), "12".to_owned());
    variables.insert("elapsed_color".to_owned(), "#44aaff".to_owned());
    variables.insert("elapsed_width".to_owned(), "312".to_owned());
    variables.insert("marker_color".to_owned(), "#ffffff".to_owned());
    variables.insert("marker_x".to_owned(), "438".to_owned());
    variables.insert("marker_y".to_owned(), "1006".to_owned());
    variables.insert("marker_width".to_owned(), "4".to_owned());
    variables.insert("marker_height".to_owned(), "20".to_owned());
    variables.insert("frame_width".to_owned(), "1920".to_owned());
    variables.insert("frame_height".to_owned(), "1080".to_owned());
    variables.insert("font_family".to_owned(), "Arial".to_owned());
    variables.insert("font_size".to_owned(), "54".to_owned());
    variables.insert("primary_color".to_owned(), "&H00FFFFFF".to_owned());
    variables.insert("outline_color".to_owned(), "&H80000000".to_owned());
    variables.insert("bold".to_owned(), "0".to_owned());
    variables.insert("outline_width".to_owned(), "2".to_owned());
    variables.insert("shadow_depth".to_owned(), "1".to_owned());
    variables.insert("alignment".to_owned(), "2".to_owned());
    variables.insert("margin_l".to_owned(), "80".to_owned());
    variables.insert("margin_r".to_owned(), "80".to_owned());
    variables.insert("margin_v".to_owned(), "72".to_owned());
    variables.insert("layer".to_owned(), "0".to_owned());
    variables.insert("start_time".to_owned(), "0:00:00.00".to_owned());
    variables.insert("end_time".to_owned(), "0:00:08.00".to_owned());
    variables.insert("text".to_owned(), "A & B".to_owned());

    let lower_third = SvgTemplate::new(include_str!("../../../assets/graphics/lower-third.vgs"))
        .render_raw(&variables);
    let song_progress =
        SvgTemplate::new(include_str!("../../../assets/graphics/song-progress.vgs"))
            .render_raw(&variables);
    let subtitle = SvgTemplate::new(include_str!(
        "../../../assets/graphics/subtitle-template.ass"
    ))
    .render_raw(&variables);

    assert!(!lower_third.contains("<svg"));
    assert!(!lower_third.contains("<?xml"));
    assert!(lower_third.contains("setcolor #101820@0.88"));
    assert!(lower_third.contains("rect 96 812 880 132 fill"));
    assert!(song_progress.contains("rect 128 1010 312 12 fill"));
    assert!(subtitle.contains("PlayResX: 1920"));
    assert!(subtitle.contains("Dialogue: 0,0:00:00.00,0:00:08.00,Graphics,,0,0,0,,A & B"));
}

#[test]
fn graphics_element_renderer_writes_drawvg_scripts_snapshot() {
    let output_dir = std::env::temp_dir().join("ersatzrs-ffmpeg-graphics-elements");
    let _ = std::fs::remove_dir_all(&output_dir);
    std::fs::create_dir_all(&output_dir).expect("test output dir can be created");
    let subtitle_template_path = output_dir.join("subtitle-template.ass");
    std::fs::write(
        &subtitle_template_path,
        "[Script Info]\nTitle: Graphics\n[Events]\nDialogue: 0,0:00:00.00,0:00:05.00,Default,,0,0,0,,{{title}}\n",
    )
    .expect("subtitle template can be written");
    let renderer = GraphicsTemplateRenderer::new(&output_dir);
    let mut variables = BTreeMap::new();
    variables.insert("title".to_owned(), "Late News".to_owned());

    let elements = sample_graphics_elements(subtitle_template_path.to_string_lossy().as_ref());

    let rendered_elements = renderer
        .render_graphics_elements(
            &elements,
            &variables,
            &GraphicsRenderContext::new(FrameSize::new(1920, 1080), FrameSize::new(1440, 1080)),
        )
        .expect("graphics render");

    let overlay_plan = rendered_elements
        .clone()
        .into_iter()
        .map(|element| match element.into_pipeline_overlay() {
            GraphicsPipelineOverlay::Drawvg { z_index, filter } => {
                format!("drawvg z={z_index} {filter}")
            }
            GraphicsPipelineOverlay::NativeImage {
                z_index,
                input,
                overlay,
            } => format!(
                "native z={z_index} input={} options={:?} filters={:?} location={:?} width={} opacity={} enable={:?} eof_pass={}",
                input.path,
                input.input_options,
                input.filter_steps,
                overlay.location,
                overlay.width_percent,
                overlay.opacity,
                overlay.enable,
                overlay.eof_action_pass
            ),
        })
        .collect::<Vec<_>>()
        .join("\n");
    insta::assert_snapshot!(
        "graphics_element_overlay_plan",
        normalize_temp_paths(&overlay_plan, &output_dir)
    );

    let filters = rendered_elements
        .iter()
        .filter_map(|element| match element {
            ersatzrs_ffmpeg::RenderedGraphicsElement::Drawvg { overlay, .. } => {
                Some(overlay.filter())
            }
            ersatzrs_ffmpeg::RenderedGraphicsElement::NativeImage { .. } => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    insta::assert_snapshot!(
        "graphics_element_drawvg_filters",
        normalize_temp_paths(&filters, &output_dir)
    );

    let scripts = rendered_elements
        .iter()
        .filter_map(|element| {
            let ersatzrs_ffmpeg::RenderedGraphicsElement::Drawvg { script_path, .. } = element
            else {
                return None;
            };
            Some(
                std::fs::read_to_string(script_path).expect("rendered graphics script can be read"),
            )
        })
        .collect::<Vec<_>>()
        .join("\n---\n");
    insta::assert_snapshot!("graphics_element_rendered_scripts", scripts);

    let ass_text =
        std::fs::read_to_string(output_dir.join("13-subtitle-template.ass")).expect("ass renders");
    insta::assert_snapshot!("graphics_element_rendered_ass", ass_text);

    std::fs::remove_dir_all(output_dir).expect("test output dir can be removed");
}

fn sample_graphics_elements(subtitle_template_path: &str) -> Vec<GraphicsElement> {
    vec![
        GraphicsElement {
            id: 7,
            name: "logo".to_owned(),
            kind: ersatzrs_domain::GraphicsElementKind::Image,
            path: Some("templates/graphics-elements/channel-logo.yml".to_owned()),
            z_index: 20,
            opacity_percent: Some(75),
            location: WatermarkLocation::TopRight,
            horizontal_margin_percent: Some(2.0),
            vertical_margin_percent: Some(3.0),
            scale: true,
            scale_width_percent: Some(12.5),
            config: GraphicsElementConfig::Image(ImageGraphicsConfig {
                image: Some("/media/logo.png".to_owned()),
            }),
            ..GraphicsElement::default()
        },
        GraphicsElement {
            id: 11,
            name: "bug".to_owned(),
            kind: ersatzrs_domain::GraphicsElementKind::Motion,
            path: Some("templates/graphics-elements/motion-bug.yml".to_owned()),
            z_index: 15,
            location: WatermarkLocation::BottomLeft,
            horizontal_margin_percent: Some(1.0),
            vertical_margin_percent: Some(2.0),
            scale: true,
            scale_width_percent: Some(20.0),
            config: GraphicsElementConfig::Motion(MotionGraphicsConfig {
                video_path: Some("/media/bug.webm".to_owned()),
                start_seconds: Some(2.5),
                end_behavior: MotionEndBehavior::Loop,
                ..MotionGraphicsConfig::default()
            }),
            ..GraphicsElement::default()
        },
        GraphicsElement {
            id: 13,
            name: "subtitle".to_owned(),
            kind: ersatzrs_domain::GraphicsElementKind::Subtitle,
            path: Some("templates/graphics-elements/subtitle-template.yml".to_owned()),
            z_index: 17,
            config: GraphicsElementConfig::Subtitle(SubtitleGraphicsConfig {
                template: Some(subtitle_template_path.to_owned()),
            }),
            ..GraphicsElement::default()
        },
        GraphicsElement {
            id: 9,
            name: "title".to_owned(),
            kind: ersatzrs_domain::GraphicsElementKind::Text,
            path: Some("templates/graphics-elements/now-playing.yml".to_owned()),
            z_index: 10,
            opacity_percent: Some(90),
            location: WatermarkLocation::BottomMiddle,
            horizontal_margin_percent: Some(5.0),
            vertical_margin_percent: Some(6.0),
            config: GraphicsElementConfig::Text(TextGraphicsConfig {
                text: Some("Now: {{title}}".to_owned()),
                fit: TextFit::Scale,
                align: TextAlignment::Center,
                width_percent: Some(80.0),
                base_style: Some("base".to_owned()),
                styles: vec![StyleDefinition {
                    name: Some("base".to_owned()),
                    font_family: Some("Inter".to_owned()),
                    font_size: Some(54.0),
                    font_weight: Some(700),
                    text_color: Some("#f8f8f8".to_owned()),
                    halo_color: Some("#101010".to_owned()),
                    halo_width: Some(3.0),
                    ..StyleDefinition::default()
                }],
                ..TextGraphicsConfig::default()
            }),
            ..GraphicsElement::default()
        },
    ]
}

#[test]
fn pipeline_interleaves_native_and_drawvg_graphics_by_z_index_snapshot() {
    let mut request = h264_request(DesiredVideoState::h264_1080p());
    request
        .graphics_overlays
        .push(GraphicsPipelineOverlay::drawvg(
            20,
            DrawvgOverlay {
                script_path: "/tmp/ErsatzRS/title.vgs".into(),
                x: 0,
                y: 0,
                width: 1920,
                height: 1080,
                enable: None,
            }
            .filter(),
        ));
    request
        .graphics_overlays
        .push(GraphicsPipelineOverlay::native_image(
            10,
            InputFile::graphics_image("/media/logo.png"),
            WatermarkOverlay {
                location: WatermarkLocation::TopRight,
                size: WatermarkSize::Scaled,
                width_percent: 12.5,
                horizontal_margin_percent: 2.0,
                vertical_margin_percent: 3.0,
                opacity: 75,
                ..WatermarkOverlay::default()
            },
        ));

    let pipeline = SoftwarePipelineBuilder.build(request.clone());
    let command = pipeline.command(&request.inputs);

    insta::assert_snapshot!(command.arguments.join(" "));
}

#[test]
fn pipeline_with_looped_motion_graphics_snapshot() {
    let mut request = h264_request(DesiredVideoState::h264_1080p());
    let input =
        InputFile::graphics_motion("/media/bug.webm", true).with_filter("setpts=PTS+2.5/TB");
    request
        .graphics_overlays
        .push(GraphicsPipelineOverlay::native_image(
            15,
            input,
            WatermarkOverlay {
                location: WatermarkLocation::BottomLeft,
                size: WatermarkSize::Scaled,
                width_percent: 20.0,
                horizontal_margin_percent: 1.0,
                vertical_margin_percent: 2.0,
                enable: Some("gte(t,2.5)".to_owned()),
                ..WatermarkOverlay::default()
            },
        ));

    let pipeline = SoftwarePipelineBuilder.build(request.clone());
    let command = pipeline.command(&request.inputs);

    insta::assert_snapshot!(command.arguments.join(" "));
}

#[test]
fn pipeline_with_disappearing_motion_graphics_snapshot() {
    let mut request = h264_request(DesiredVideoState::h264_1080p());
    request
        .graphics_overlays
        .push(GraphicsPipelineOverlay::native_image(
            15,
            InputFile::graphics_motion("/media/bumper.mov", false),
            WatermarkOverlay {
                location: WatermarkLocation::MiddleCenter,
                enable: Some("gte(t,5)".to_owned()),
                eof_action_pass: true,
                ..WatermarkOverlay::default()
            },
        ));

    let pipeline = SoftwarePipelineBuilder.build(request.clone());
    let command = pipeline.command(&request.inputs);

    insta::assert_snapshot!(command.arguments.join(" "));
}

#[test]
fn graphics_element_renderer_bounds_hold_motion_snapshot() {
    let output_dir = std::env::temp_dir().join("ersatzrs-ffmpeg-hold-motion");
    let _ = std::fs::remove_dir_all(&output_dir);
    let renderer = GraphicsTemplateRenderer::new(&output_dir);
    let element = GraphicsElement {
        id: 21,
        name: "hold-bug".to_owned(),
        kind: ersatzrs_domain::GraphicsElementKind::Motion,
        z_index: 15,
        location: WatermarkLocation::MiddleCenter,
        config: GraphicsElementConfig::Motion(MotionGraphicsConfig {
            video_path: Some("/media/hold-bug.webm".to_owned()),
            start_seconds: Some(5.0),
            hold_seconds: Some(3.25),
            end_behavior: MotionEndBehavior::Hold,
        }),
        ..GraphicsElement::default()
    };

    let hold_plan = renderer
        .render_graphics_elements(
            std::slice::from_ref(&element),
            &BTreeMap::new(),
            &GraphicsRenderContext::new(FrameSize::new(1920, 1080), FrameSize::new(1920, 1080))
                .with_motion_source_duration(element.id, 2.5),
        )
        .expect("hold motion renders");
    let plan = hold_plan
        .into_iter()
        .map(|element| match element.into_pipeline_overlay() {
            GraphicsPipelineOverlay::NativeImage { input, overlay, .. } => format!(
                "input={} options={:?} filters={:?} enable={:?} eof_pass={}",
                input.path,
                input.input_options,
                input.filter_steps,
                overlay.enable,
                overlay.eof_action_pass
            ),
            GraphicsPipelineOverlay::Drawvg { .. } => "drawvg".to_owned(),
        })
        .collect::<Vec<_>>()
        .join("\n");

    insta::assert_snapshot!("graphics_element_hold_motion_plan", plan);
}

#[test]
fn pipeline_with_hold_motion_graphics_snapshot() {
    let mut request = h264_request(DesiredVideoState::h264_1080p());
    request
        .graphics_overlays
        .push(GraphicsPipelineOverlay::native_image(
            15,
            InputFile::graphics_motion("/media/hold-bug.webm", false),
            WatermarkOverlay {
                location: WatermarkLocation::MiddleCenter,
                enable: Some("between(t,5,10.75)".to_owned()),
                eof_action_pass: false,
                ..WatermarkOverlay::default()
            },
        ));

    let pipeline = SoftwarePipelineBuilder.build(request.clone());
    let command = pipeline.command(&request.inputs);

    insta::assert_snapshot!(command.arguments.join(" "));
}

#[test]
fn graphics_element_renderer_writes_script_drawvg_snapshot() {
    let output_dir = std::env::temp_dir().join("ersatzrs-ffmpeg-script-graphics");
    let _ = std::fs::remove_dir_all(&output_dir);
    let renderer = GraphicsTemplateRenderer::new(&output_dir);
    let element = GraphicsElement {
        id: 31,
        name: "lua-script".to_owned(),
        kind: ersatzrs_domain::GraphicsElementKind::Script,
        path: Some("templates/graphics-elements/script-lower-third.yml".to_owned()),
        z_index: 22,
        config: GraphicsElementConfig::Script(ScriptGraphicsConfig {
            command: Some(
                r"return string.format(
  'rect 24 24 %d 64 setcolor #00ff00 fill',
  24 + string.len(variables.title .. args[1]) * 24
)"
                .to_owned(),
            ),
            args: vec!["Live".to_owned()],
            start_seconds: Some(3.0),
            duration_seconds: Some(7.5),
            ..ScriptGraphicsConfig::default()
        }),
        ..GraphicsElement::default()
    };
    let mut variables = BTreeMap::new();
    variables.insert("title".to_owned(), "Late News".to_owned());

    let script_plan = renderer
        .render_graphics_elements(
            std::slice::from_ref(&element),
            &variables,
            &GraphicsRenderContext::new(FrameSize::new(1280, 720), FrameSize::new(1280, 720)),
        )
        .expect("script graphics render");
    let overlay = script_plan
        .iter()
        .filter_map(|element| match element {
            ersatzrs_ffmpeg::RenderedGraphicsElement::Drawvg { overlay, .. } => {
                Some(overlay.filter())
            }
            ersatzrs_ffmpeg::RenderedGraphicsElement::NativeImage { .. } => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    insta::assert_snapshot!(
        "graphics_element_script_overlay",
        normalize_temp_paths(&overlay, &output_dir)
    );

    let rendered_script = std::fs::read_to_string(output_dir.join("31-script-lower-third.vgs"))
        .expect("script drawvg renders");
    insta::assert_snapshot!("graphics_element_rendered_script_drawvg", rendered_script);
    std::fs::remove_dir_all(output_dir).expect("test output dir can be removed");
}

#[test]
fn pipeline_with_subtitle_graphics_overlay_snapshot() {
    let mut request = h264_request(DesiredVideoState::h264_1080p());
    request
        .graphics_overlays
        .push(GraphicsPipelineOverlay::native_image(
            17,
            InputFile::graphics_subtitle(subtitle_graphics_lavfi_input(
                "/tmp/ErsatzRS/subtitle.ass",
                FrameSize::new(1920, 1080),
                Some("/tmp/ErsatzRS/fonts"),
            )),
            WatermarkOverlay {
                location: WatermarkLocation::MiddleCenter,
                size: WatermarkSize::ActualSize,
                width_percent: 100.0,
                ..WatermarkOverlay::default()
            },
        ));

    let pipeline = SoftwarePipelineBuilder.build(request.clone());
    let command = pipeline.command(&request.inputs);

    insta::assert_snapshot!(command.arguments.join(" "));
}

// ── ADR 0064: explicit SDR/HDR10 output dynamic range ────────────────────────

/// Build an HDR10 (`Hdr10Bt2020Pq`) HEVC 10-bit desired state.
fn hdr10_desired() -> DesiredVideoState {
    DesiredVideoState {
        video_format: FFmpegProfileVideoFormat::Hevc,
        bit_depth: FFmpegProfileBitDepth::TenBit,
        output_dynamic_range: OutputDynamicRange::Hdr10Bt2020Pq,
        video_profile: Some("main10".to_owned()),
        ..DesiredVideoState::h264_1080p()
    }
}

fn source_with_transfer(transfer: Option<&str>, is_hdr: bool) -> SourceVideoStream {
    let mut source = SourceVideoStream::new(SourceVideoCodec::Hevc);
    source.color_params = SourceColorParams {
        color_transfer: transfer.map(str::to_owned),
        color_primaries: is_hdr.then(|| "bt2020".to_owned()),
        color_space: is_hdr.then(|| "bt2020nc".to_owned()),
        color_range: Some("tv".to_owned()),
        is_hdr,
        is_bt2020_ten: is_hdr,
    };
    source
}

#[test]
fn hdr10_profile_preserves_a_pq_source_without_tonemapping() {
    let request = PipelineRequest::new(InputFile::video("/media/hdr10.mkv"), hdr10_desired())
        .with_source_video_stream(source_with_transfer(Some("smpte2084"), true));

    let args = SoftwarePipelineBuilder
        .build(request.clone())
        .command(&request.inputs)
        .arguments
        .join(" ");

    assert!(
        !args.contains("tonemap"),
        "an HDR10 profile must never tone-map a PQ source to SDR, got: {args}"
    );
    assert!(
        args.contains(
            "setparams=range=tv:colorspace=bt2020nc:color_trc=smpte2084:color_primaries=bt2020"
        ),
        "got: {args}"
    );
    // The source's own mastering-display side data survives the encode, so no
    // profile metadata is injected over it.
    assert!(!args.contains("-mastering_display"), "got: {args}");
    assert!(!args.contains("-content_light"), "got: {args}");
    // BT.709 normalization would silently downgrade the output to SDR.
    assert!(!args.contains("all=bt709"), "got: {args}");
    insta::assert_snapshot!(args);
}

#[test]
fn hdr10_profile_maps_an_sdr_source_at_the_documented_reference_white() {
    let request = PipelineRequest::new(InputFile::video("/media/sdr.mkv"), hdr10_desired())
        .with_source_video_stream(source_with_transfer(Some("bt709"), false));

    let args = SoftwarePipelineBuilder
        .build(request.clone())
        .command(&request.inputs)
        .arguments
        .join(" ");

    assert!(
        args.contains("npl=203"),
        "BT.2408 diffuse white, got: {args}"
    );
    assert!(args.contains("zscale=t=smpte2084"), "got: {args}");
    // A transform changed what static metadata means, so validated profile
    // metadata is emitted at the input boundary.
    assert!(
        args.contains("-mastering_display G(8500,39850)B(6550,2300)R(35400,14600)WP(15635,16450)L(10000000,1)"),
        "got: {args}"
    );
    assert!(args.contains("-content_light 1000,400"), "got: {args}");
    assert!(!args.contains("all=bt709"), "got: {args}");
    insta::assert_snapshot!(args);
}

#[test]
fn hdr10_profile_normalizes_an_hlg_source_to_pq() {
    let request = PipelineRequest::new(InputFile::video("/media/hlg.mkv"), hdr10_desired())
        .with_source_video_stream(source_with_transfer(Some("arib-std-b67"), true));

    let args = SoftwarePipelineBuilder
        .build(request.clone())
        .command(&request.inputs)
        .arguments
        .join(" ");

    assert!(args.contains("zscale=t=smpte2084"), "got: {args}");
    assert!(args.contains("npl=1000"), "got: {args}");
    assert!(args.contains("-mastering_display"), "got: {args}");
    insta::assert_snapshot!(args);
}

#[test]
fn sdr_profile_still_tonemaps_an_hdr_source_exactly_as_before() {
    let request = PipelineRequest::new(
        InputFile::video("/media/hdr10.mkv"),
        DesiredVideoState::h264_1080p(),
    )
    .with_source_video_stream(source_with_transfer(Some("smpte2084"), true));

    let args = SoftwarePipelineBuilder
        .build(request.clone())
        .command(&request.inputs)
        .arguments
        .join(" ");

    assert!(args.contains("tonemap="), "got: {args}");
    assert!(
        args.contains("zscale=p=bt709:t=bt709:m=bt709"),
        "got: {args}"
    );
    // SDR output must never carry HDR static metadata.
    assert!(!args.contains("-mastering_display"), "got: {args}");
    assert!(!args.contains("-content_light"), "got: {args}");
    assert!(!args.contains("smpte2084"), "got: {args}");
}

#[test]
fn video_copy_ignores_the_output_dynamic_range() {
    let desired = DesiredVideoState {
        video_format: FFmpegProfileVideoFormat::Copy,
        output_dynamic_range: OutputDynamicRange::Hdr10Bt2020Pq,
        ..DesiredVideoState::h264_1080p()
    };
    let request = PipelineRequest::new(InputFile::video("/media/hdr10.mkv"), desired)
        .with_source_video_stream(source_with_transfer(Some("smpte2084"), true));

    let args = SoftwarePipelineBuilder
        .build(request.clone())
        .command(&request.inputs)
        .arguments
        .join(" ");

    assert!(!args.contains("-mastering_display"), "got: {args}");
    assert!(!args.contains("zscale"), "got: {args}");
    assert!(!args.contains("tonemap"), "got: {args}");
}

#[test]
fn hdr10_output_signalling_is_identical_for_sdr_and_hdr_items() {
    // ADR 0064: an HDR10 profile describes the *output* stream, so a linear
    // stream must not change its signalling when an SDR item follows an HDR
    // item. This is the programme-transition contract at command level; the
    // decoded-frame proof lives in the HDR runtime gate.
    let tail = "setparams=range=tv:colorspace=bt2020nc:color_trc=smpte2084:color_primaries=bt2020";

    for (path, transfer, is_hdr) in [
        ("/media/hdr10.mkv", Some("smpte2084"), true),
        ("/media/hlg.mkv", Some("arib-std-b67"), true),
        ("/media/sdr.mkv", Some("bt709"), false),
        ("/media/untagged.mkv", None, false),
    ] {
        let request = PipelineRequest::new(InputFile::video(path), hdr10_desired())
            .with_source_video_stream(source_with_transfer(transfer, is_hdr));
        let args = SoftwarePipelineBuilder
            .build(request.clone())
            .command(&request.inputs)
            .arguments
            .join(" ");

        assert!(args.contains(tail), "{path} lost HDR10 signalling: {args}");
        assert!(
            !args.contains("zscale=p=bt709"),
            "{path} downgraded to BT.709: {args}"
        );
    }
}

// ── ADR 0014 / p8b-01: operation-scoped Vulkan graph gate ───────────────────

fn hdr_nvenc_request(report: ersatzrs_ffmpeg::FfmpegCapabilityReport) -> PipelineRequest {
    let mut source = SourceVideoStream::new(SourceVideoCodec::Hevc);
    source.color_params = SourceColorParams {
        is_hdr: true,
        is_bt2020_ten: true,
        ..SourceColorParams::default()
    };
    PipelineRequest::new(
        InputFile::video("/media/hdr-movie.mkv"),
        DesiredVideoState::h264_1080p(),
    )
    .with_source_video_stream(source)
    .with_runtime_capabilities(report)
}

#[test]
fn a_denied_vulkan_graph_falls_back_to_the_proven_cuda_path() {
    // The exact machine state that motivated p8b-01: `vulkan` is advertised
    // and initializes, but the only Vulkan device is a software rasterizer, so
    // the functional probe denied the graph.
    let mut report = capability_report_for_tests();
    report.hwaccels.insert("vulkan".to_owned());
    report.deny_hardware_graph(HardwareGraphDenial::new(
        HardwareGraphOperation::NvencHdrVulkanLibplacebo,
        "",
        GraphFailureKind::CudaInterop,
    ));
    // The fallback is only selectable because the probe proved it.
    report.prove_hardware_graph(HardwareGraphOperation::NvencCudaScale, "");
    let request = hdr_nvenc_request(report);

    let args = NvencPipelineBuilder
        .build(request.clone())
        .command(&request.inputs)
        .arguments
        .join(" ");

    // libplacebo needs a Vulkan device even without the `vulkan` hwaccel, so a
    // denied Vulkan stack must drop libplacebo entirely — not merely the
    // `-hwaccel vulkan` flag. Measured: the old "non-Vulkan" NVENC HDR chain
    // still died with `Failed initializing vulkan device`.
    assert!(
        !args.contains("-init_hw_device vulkan") && !args.contains("libplacebo"),
        "a denied Vulkan graph must not be selected, got: {args}"
    );
    // The retry must preserve hardware encoding, not demote to software.
    assert!(args.contains("h264_nvenc"), "got: {args}");
    assert!(args.contains("hwupload_cuda"), "got: {args}");
    insta::assert_snapshot!(args);
}

#[test]
fn an_unproven_vulkan_graph_is_still_selected_when_nothing_denied_it() {
    // Absence of a denial is the normal state; the gate must not require a
    // positive proof row that older databases do not have.
    let mut report = capability_report_for_tests();
    report.hwaccels.insert("vulkan".to_owned());
    let request = hdr_nvenc_request(report);

    let args = NvencPipelineBuilder
        .build(request.clone())
        .command(&request.inputs)
        .arguments
        .join(" ");

    assert!(args.contains("libplacebo"), "got: {args}");
}

#[test]
fn a_denial_learned_on_an_unidentified_device_applies_broadly() {
    // When the runtime cannot name the GPU, the denial has to apply rather than
    // be re-tried on every playback of a known-broken graph.
    let mut report = capability_report_for_tests();
    report.hwaccels.insert("vulkan".to_owned());
    report.deny_hardware_graph(HardwareGraphDenial::new(
        HardwareGraphOperation::NvencHdrVulkanLibplacebo,
        "",
        GraphFailureKind::CudaInterop,
    ));
    report.prove_hardware_graph(HardwareGraphOperation::NvencCudaScale, "");
    let request = hdr_nvenc_request(report);

    let args = NvencPipelineBuilder
        .build(request.clone())
        .command(&request.inputs)
        .arguments
        .join(" ");

    assert!(!args.contains("libplacebo"), "got: {args}");
}

#[test]
fn a_denial_for_a_different_gpu_does_not_block_this_one() {
    let mut report = capability_report_for_tests();
    report.hwaccels.insert("vulkan".to_owned());
    // A realistic `-gpu list` probe line; `hardware_graph_device` parses the
    // model out of it. A bare model string is not what the runtime reports.
    report.nvidia_gpu_list =
        Some("[ GPU #0 - < NVIDIA GeForce RTX 4090 > has Compute SM 8.9 ]".to_owned());
    report.deny_hardware_graph(HardwareGraphDenial::new(
        HardwareGraphOperation::NvencHdrVulkanLibplacebo,
        "NVIDIA GeForce RTX 3080 Ti",
        GraphFailureKind::CudaInterop,
    ));
    // This GPU probed clean, so both graphs carry positive proof for it.
    report.prove_hardware_graph(
        HardwareGraphOperation::NvencHdrVulkanLibplacebo,
        "NVIDIA GeForce RTX 4090",
    );
    report.prove_hardware_graph(
        HardwareGraphOperation::NvencCudaScale,
        "NVIDIA GeForce RTX 4090",
    );
    let request = hdr_nvenc_request(report);

    let args = NvencPipelineBuilder
        .build(request.clone())
        .command(&request.inputs)
        .arguments
        .join(" ");

    assert!(args.contains("libplacebo"), "got: {args}");
}

#[test]
fn missing_vulkan_is_a_normal_capability_result_not_a_failure() {
    // ADR 0014: a platform without Vulkan must stay healthy and simply use the
    // CUDA graph.
    let request = hdr_nvenc_request(capability_report_for_tests());

    let args = NvencPipelineBuilder
        .build(request.clone())
        .command(&request.inputs)
        .arguments
        .join(" ");

    assert!(!args.contains("libplacebo"), "got: {args}");
    assert!(args.contains("h264_nvenc"), "got: {args}");
}

// ── ADR 0064: HDR10 output on hardware backends ─────────────────────────────

fn hdr10_hevc_desired() -> DesiredVideoState {
    DesiredVideoState {
        video_format: FFmpegProfileVideoFormat::Hevc,
        bit_depth: FFmpegProfileBitDepth::TenBit,
        output_dynamic_range: OutputDynamicRange::Hdr10Bt2020Pq,
        video_profile: Some("main10".to_owned()),
        video_preset: Some("p4".to_owned()),
        ..DesiredVideoState::h264_1080p()
    }
}

fn hdr10_nvenc_args(transfer: Option<&str>, is_hdr: bool) -> String {
    let request = PipelineRequest::new(InputFile::video("/media/item.mkv"), hdr10_hevc_desired())
        .with_source_video_stream(source_with_transfer(transfer, is_hdr))
        .with_runtime_capabilities(capability_report_for_tests());

    NvencPipelineBuilder
        .build(request.clone())
        .command(&request.inputs)
        .arguments
        .join(" ")
}

#[test]
fn nvenc_hdr10_keeps_a_pq_source_on_the_gpu() {
    let args = hdr10_nvenc_args(Some("smpte2084"), true);

    assert!(args.contains("hevc_nvenc"), "got: {args}");
    assert!(
        args.contains(
            "setparams=range=tv:colorspace=bt2020nc:color_trc=smpte2084:color_primaries=bt2020"
        ),
        "got: {args}"
    );
    // A PQ source needs no conversion, so nothing leaves the GPU.
    assert!(!args.contains("hwdownload"), "got: {args}");
    assert!(!args.contains("zscale"), "got: {args}");
    assert!(!args.contains("-mastering_display"), "got: {args}");
    insta::assert_snapshot!(args);
}

#[test]
fn nvenc_hdr10_converts_an_sdr_source_and_re_uploads_to_cuda() {
    let args = hdr10_nvenc_args(Some("bt709"), false);

    assert!(args.contains("hevc_nvenc"), "got: {args}");
    assert!(args.contains("npl=203"), "got: {args}");
    // The converted frames must go back to the GPU so NVENC still encodes.
    assert!(args.contains("format=p010le,hwupload_cuda"), "got: {args}");
    assert!(args.contains("-mastering_display"), "got: {args}");
    insta::assert_snapshot!(args);
}

#[test]
fn nvenc_hdr10_normalizes_hlg_and_downloads_ten_bit_surfaces() {
    let args = hdr10_nvenc_args(Some("arib-std-b67"), true);

    assert!(args.contains("hevc_nvenc"), "got: {args}");
    assert!(args.contains("npl=1000"), "got: {args}");
    assert!(args.contains("format=p010le,hwupload_cuda"), "got: {args}");
    // HLG surfaces are 10-bit; an nv12 download is rejected by FFmpeg.
    assert!(!args.contains("hwdownload,format=nv12"), "got: {args}");
    insta::assert_snapshot!(args);
}

#[test]
fn unproven_hardware_backends_demote_to_software_for_hdr10() {
    // ADR 0064: hardware may change performance, not colour semantics. A
    // backend with no proven HDR10 graph must not quietly emit SDR.
    for (name, builder) in [
        ("vaapi", &VaapiPipelineBuilder as &dyn PipelineBuilder),
        ("qsv", &QsvPipelineBuilder),
        ("amf", &AmfPipelineBuilder),
        ("videotoolbox", &VideoToolboxPipelineBuilder),
        ("v4l2m2m", &V4l2m2mPipelineBuilder),
        ("rkmpp", &RkmppPipelineBuilder),
    ] {
        let request =
            PipelineRequest::new(InputFile::video("/media/item.mkv"), hdr10_hevc_desired())
                .with_source_video_stream(source_with_transfer(Some("smpte2084"), true))
                .with_runtime_capabilities(capability_report_for_tests());
        let args = builder
            .build(request.clone())
            .command(&request.inputs)
            .arguments
            .join(" ");

        // HDR10 signalling is still produced — just in software.
        assert!(
            args.contains(
                "setparams=range=tv:colorspace=bt2020nc:color_trc=smpte2084:color_primaries=bt2020"
            ),
            "{name} lost HDR10 signalling: {args}"
        );
        assert!(
            args.contains("-codec:v libx265"),
            "{name} must demote to the software encoder: {args}"
        );
        assert!(
            !args.contains("_vaapi") && !args.contains("_qsv") && !args.contains("_amf"),
            "{name} kept an unproven hardware encoder: {args}"
        );
    }
}

#[test]
fn unproven_backends_keep_their_hardware_path_for_sdr_profiles() {
    // The demotion is scoped to HDR10 output; ordinary SDR profiles are
    // untouched.
    let request = PipelineRequest::new(
        InputFile::video("/media/item.mkv"),
        DesiredVideoState::h264_1080p(),
    )
    .with_source_video_stream(source_with_transfer(Some("bt709"), false))
    .with_runtime_capabilities(capability_report_for_tests());

    let args = VaapiPipelineBuilder
        .build(request.clone())
        .command(&request.inputs)
        .arguments
        .join(" ");

    assert!(args.contains("h264_vaapi"), "got: {args}");
}

// ── p8c-03: animated WebP graphics and watermark inputs ─────────────────────

fn graphics_fixture(name: &str) -> String {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/graphics")
        .join(name)
        .to_string_lossy()
        .into_owned()
}

#[test]
fn animated_webp_is_detected_from_its_header() {
    // The `ANIM` chunk in an extended RIFF/WEBP file is the animation marker.
    assert!(ersatzrs_ffmpeg::image_file_is_animated(
        std::path::Path::new(&graphics_fixture("animated-alpha.webp"))
    ));
    assert!(!ersatzrs_ffmpeg::image_file_is_animated(
        std::path::Path::new(&graphics_fixture("static-alpha.webp"))
    ));
    // A missing file must not be reported as animated, or a stale graphics
    // element would pick the wrong demuxer option.
    assert!(!ersatzrs_ffmpeg::image_file_is_animated(
        std::path::Path::new(&graphics_fixture("does-not-exist.webp"))
    ));
}

#[test]
fn a_graphics_element_image_picks_the_demuxer_specific_loop_option() {
    // Measured against FFmpeg 9: `-loop 1` on an animated WebP fails with
    // `Option loop not found` and the input never opens, while
    // `-ignore_loop 0` on a still image fails with
    // `Option ignore_loop not found`. Either mismatch kills the channel.
    let animated = InputFile::graphics_image(graphics_fixture("animated-alpha.webp"));
    assert_eq!(animated.input_options, vec!["-ignore_loop", "0"]);

    let still = InputFile::graphics_image(graphics_fixture("static-alpha.webp"));
    assert_eq!(still.input_options, vec!["-loop", "1"]);
}

#[test]
fn still_graphics_images_keep_their_existing_loop_behavior() {
    // A path that does not exist (the shape every existing snapshot uses)
    // must stay on the still-image option, so this change cannot alter any
    // established graph.
    let input = InputFile::graphics_image("/media/logo.png");

    assert_eq!(input.input_options, vec!["-loop", "1"]);
}

#[test]
fn an_animated_watermark_uses_ignore_loop_and_a_still_one_does_not() {
    assert_eq!(
        InputFile::watermark_animated("/media/bug.webp").input_options,
        vec!["-ignore_loop", "0"]
    );
    assert_eq!(
        InputFile::watermark_looped("/media/logo.png").input_options,
        vec!["-loop", "1"]
    );
    assert!(InputFile::watermark("/media/logo.png")
        .input_options
        .is_empty());
}

/// Real-`FFmpeg` gate for animated WebP overlays. Ignored by default because it
/// needs an `ffmpeg` binary.
///
/// Run with:
/// `ERSATZRS_FFMPEG=<path to ffmpeg> ./run.sh test -p ersatzrs-ffmpeg \
///  --run-ignored ignored-only animated_webp_overlay`
///
/// Decoded proof rather than argv: a per-frame hash sequence distinguishes
/// correct looping (the eight distinct frames repeat) from a missing loop
/// option (the last frame freezes), which argv inspection cannot.
// Test-only opt-in path; the configuration layer is for the application.
#[allow(clippy::disallowed_methods)]
#[test]
#[ignore = "requires a real ffmpeg binary"]
fn animated_webp_overlay_loops_and_composites_alpha() {
    let Some(ffmpeg) = std::env::var_os("ERSATZRS_FFMPEG") else {
        eprintln!("set ERSATZRS_FFMPEG to run this gate");
        return;
    };
    let fixture = graphics_fixture("animated-alpha.webp");

    let frame_hashes = |loop_option: &[&str]| -> Vec<String> {
        let mut command = std::process::Command::new(&ffmpeg);
        command.args(["-nostdin", "-hide_banner", "-loglevel", "error"]);
        command.args([
            "-f",
            "lavfi",
            "-i",
            "color=c=black:size=64x64:rate=8:duration=2",
        ]);
        command.args(loop_option);
        command.args(["-i", &fixture]);
        command.args([
            "-filter_complex",
            "[0:v][1:v]overlay=0:0[v]",
            "-map",
            "[v]",
            "-frames:v",
            "16",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "gray",
            "-",
        ]);
        let output = command.output().expect("run ffmpeg");
        assert!(
            output.status.success(),
            "ffmpeg failed: {:?}",
            output.status
        );

        output
            .stdout
            .chunks_exact(64 * 64)
            .map(|frame| {
                // A cheap content signature; only equality between frames
                // matters here.
                let sum: u64 = frame.iter().map(|byte| u64::from(*byte)).sum();
                format!("{sum}-{}", frame.len())
            })
            .collect()
    };

    let looped = frame_hashes(&["-ignore_loop", "0"]);
    assert_eq!(looped.len(), 16);
    assert_eq!(
        looped[0..8],
        looped[8..16],
        "the animation must repeat for the whole programme"
    );
    assert_eq!(
        looped[0..8]
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        8,
        "all eight source frames must decode distinctly"
    );

    // Without the loop option the final frame freezes; this is what an operator
    // saw before the loop option was selected per demuxer.
    let unlooped = frame_hashes(&[]);
    assert!(
        unlooped[8..16].iter().all(|frame| *frame == unlooped[7]),
        "control case: the last frame should freeze without -ignore_loop 0"
    );

    // Alpha survived the decode: the 50%-alpha fixture over black is strictly
    // darker than the same fixture decoded on its own would be at full opacity.
    assert!(
        looped[0] != "0-4096",
        "the overlay must actually contribute pixels"
    );
}

// ── p8c-02: source orientation applied exactly once ─────────────────────────

fn rotated_source(width: u32, height: u32, rotation: Option<i32>) -> SourceVideoStream {
    let mut source = SourceVideoStream::new(SourceVideoCodec::H264);
    source.frame_size = Some(FrameSize::new(width, height));
    source.rotation_degrees = rotation;
    source
}

#[test]
fn a_quarter_turn_transposes_the_displayed_frame_size() {
    // FFmpeg autorotates from the display matrix before any filter runs, so a
    // portrait recording stored as 1920x1080 reaches the graph as 1080x1920.
    let portrait = rotated_source(1920, 1080, Some(90));
    assert!(portrait.is_quarter_turned());
    assert_eq!(
        portrait.displayed_frame_size(),
        Some(FrameSize::new(1080, 1920))
    );

    // 270 degrees is the other quarter turn; ffprobe reports it for -90.
    assert!(rotated_source(1920, 1080, Some(270)).is_quarter_turned());

    // A half turn keeps the axes, so the displayed size is the stored size.
    let upside_down = rotated_source(1920, 1080, Some(180));
    assert!(!upside_down.is_quarter_turned());
    assert_eq!(
        upside_down.displayed_frame_size(),
        Some(FrameSize::new(1920, 1080))
    );

    // No display matrix at all: unchanged, which is every pre-existing item.
    let plain = rotated_source(1920, 1080, None);
    assert!(!plain.is_quarter_turned());
    assert_eq!(
        plain.displayed_frame_size(),
        Some(FrameSize::new(1920, 1080))
    );
}

#[test]
fn a_rotated_source_is_scaled_by_its_displayed_shape_not_its_stored_one() {
    // Into a 1920x1080 channel, a portrait item must letterbox to 606x1080
    // (1080 x 0.5625 = 607.5, floored to an even width), not fill the frame as
    // if it were landscape.
    let profile = FFmpegProfile {
        scaling_behavior: ScalingBehavior::ScaleAndPad,
        ..FFmpegProfile::default()
    };
    let resolution = Resolution {
        width: 1920,
        height: 1080,
        ..Resolution::default()
    };

    let portrait = rotated_source(1920, 1080, Some(90));
    let desired = DesiredVideoState::from_profile(
        &profile,
        &resolution,
        Some(&portrait),
        None,
        StreamingMode::TransportStream,
    );
    assert_eq!(desired.scaled_size, FrameSize::new(606, 1080));
    assert_eq!(desired.frame_size, FrameSize::new(1920, 1080));

    // The same stored dimensions without a display matrix fill the frame.
    let landscape = rotated_source(1920, 1080, None);
    let desired = DesiredVideoState::from_profile(
        &profile,
        &resolution,
        Some(&landscape),
        None,
        StreamingMode::TransportStream,
    );
    assert_eq!(desired.scaled_size, FrameSize::new(1920, 1080));
}

#[test]
fn ersatzrs_never_adds_a_rotation_filter_of_its_own() {
    // FFmpeg already autorotated the frames. A transpose here would rotate the
    // picture twice, which is the exact failure this milestone exists to avoid.
    let request = PipelineRequest::new(
        InputFile::video("/media/portrait.mp4"),
        DesiredVideoState::h264_1080p(),
    )
    .with_source_video_stream(rotated_source(1920, 1080, Some(90)));

    let args = SoftwarePipelineBuilder
        .build(request.clone())
        .command(&request.inputs)
        .arguments
        .join(" ");

    assert!(!args.contains("transpose"), "got: {args}");
    assert!(!args.contains("rotate"), "got: {args}");
    assert!(!args.contains("-noautorotate"), "got: {args}");
    insta::assert_snapshot!(args);
}

#[test]
fn stream_copy_leaves_orientation_metadata_untouched() {
    // Copy preserves the elementary stream and its display matrix; the client
    // rotates. Adding a filter would force a transcode.
    let desired = DesiredVideoState {
        video_format: FFmpegProfileVideoFormat::Copy,
        ..DesiredVideoState::h264_1080p()
    };
    let request = PipelineRequest::new(InputFile::video("/media/portrait.mp4"), desired)
        .with_source_video_stream(rotated_source(1920, 1080, Some(90)));

    let args = SoftwarePipelineBuilder
        .build(request.clone())
        .command(&request.inputs)
        .arguments
        .join(" ");

    assert!(!args.contains("transpose"), "got: {args}");
    assert!(!args.contains("scale="), "got: {args}");
}

/// Real-`FFmpeg` gate for orientation. Ignored by default because it needs an
/// `ffmpeg` binary.
///
/// Run with:
/// `ERSATZRS_FFMPEG=<path to ffmpeg> ./run.sh test -p ersatzrs-ffmpeg \
///  --run-ignored ignored-only orientation_autorotation`
///
/// This pins the assumption the whole milestone rests on: `FFmpeg` autorotates
/// from the display matrix *before* the filter graph, so ErsatzRS must adjust
/// its geometry and must not add a transpose.
// Test-only opt-in path; the configuration layer is for the application.
#[allow(clippy::disallowed_methods)]
#[test]
#[ignore = "requires a real ffmpeg binary"]
fn orientation_autorotation_happens_before_the_filter_graph() {
    let Some(ffmpeg) = std::env::var_os("ERSATZRS_FFMPEG") else {
        eprintln!("set ERSATZRS_FFMPEG to run this gate");
        return;
    };
    let directory = tempfile::tempdir().expect("temp dir");
    let landscape = directory.path().join("landscape.mp4");
    let rotated = directory.path().join("rotated.mp4");

    let run = |args: &[&str]| {
        let status = std::process::Command::new(&ffmpeg)
            .args(["-nostdin", "-hide_banner", "-loglevel", "error", "-y"])
            .args(args)
            .status()
            .expect("run ffmpeg");
        assert!(status.success(), "ffmpeg failed for {args:?}");
    };
    let dimensions = |path: &std::path::Path| -> String {
        let output = std::process::Command::new(
            std::path::Path::new(&ffmpeg)
                .parent()
                .expect("ffmpeg directory")
                .join("ffprobe"),
        )
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height",
            "-of",
            "csv=p=0",
        ])
        .arg(path)
        .output()
        .expect("run ffprobe");
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    };

    // A 320x180 landscape source, then the same elementary stream tagged with a
    // 90-degree display matrix.
    run(&[
        "-f",
        "lavfi",
        "-i",
        "testsrc2=size=320x180:duration=1:rate=10",
        "-c:v",
        "libx264",
        "-pix_fmt",
        "yuv420p",
        landscape.to_str().expect("path"),
    ]);
    run(&[
        "-display_rotation:v:0",
        "90",
        "-i",
        landscape.to_str().expect("path"),
        "-c",
        "copy",
        rotated.to_str().expect("path"),
    ]);

    // Stored dimensions are unchanged by the tag.
    assert!(
        dimensions(&rotated).starts_with("320,180"),
        "{}",
        dimensions(&rotated)
    );

    // Transcoding autorotates with no filter of ours: the decoded frames are
    // already transposed, which is why `displayed_frame_size` must transpose too.
    let transcoded = directory.path().join("transcoded.mp4");
    run(&[
        "-i",
        rotated.to_str().expect("path"),
        "-c:v",
        "libx264",
        "-frames:v",
        "2",
        transcoded.to_str().expect("path"),
    ]);
    assert!(
        dimensions(&transcoded).starts_with("180,320"),
        "FFmpeg must autorotate on transcode, got {}",
        dimensions(&transcoded)
    );
}

#[test]
fn hdr10_conversion_prefers_source_static_metadata_over_the_seeded_default() {
    // ADR 0064: use the source's own mastering display when it has one. The
    // display the content was graded on does not change because the transfer
    // function was converted.
    let mut source = source_with_transfer(Some("arib-std-b67"), true);
    source.mastering_display =
        Some("G(13250,34500)B(7500,3000)R(34000,16000)WP(15635,16450)L(10000000,50)".to_owned());
    source.max_cll = Some(4000);
    source.max_fall = Some(600);

    let request = PipelineRequest::new(InputFile::video("/media/hlg.mkv"), hdr10_desired())
        .with_source_video_stream(source);
    let args = SoftwarePipelineBuilder
        .build(request.clone())
        .command(&request.inputs)
        .arguments
        .join(" ");

    assert!(
        args.contains(
            "-mastering_display G(13250,34500)B(7500,3000)R(34000,16000)WP(15635,16450)L(10000000,50)"
        ),
        "got: {args}"
    );
    assert!(args.contains("-content_light 4000,600"), "got: {args}");
}

#[test]
fn hdr10_conversion_falls_back_to_the_documented_default_when_the_source_is_silent() {
    let request = PipelineRequest::new(InputFile::video("/media/sdr.mkv"), hdr10_desired())
        .with_source_video_stream(source_with_transfer(Some("bt709"), false));
    let args = SoftwarePipelineBuilder
        .build(request.clone())
        .command(&request.inputs)
        .arguments
        .join(" ");

    assert!(
        args.contains("-mastering_display G(8500,39850)B(6550,2300)R(35400,14600)WP(15635,16450)L(10000000,1)"),
        "got: {args}"
    );
    assert!(args.contains("-content_light 1000,400"), "got: {args}");
}

#[test]
fn a_pq_source_still_emits_no_metadata_options_even_when_it_has_them() {
    // Its own side data survives the encode untouched, so injecting anything
    // could only overwrite more accurate values with less accurate ones.
    let mut source = source_with_transfer(Some("smpte2084"), true);
    source.mastering_display = Some("G(1,2)B(3,4)R(5,6)WP(7,8)L(9,10)".to_owned());
    source.max_cll = Some(4000);

    let request = PipelineRequest::new(InputFile::video("/media/hdr10.mkv"), hdr10_desired())
        .with_source_video_stream(source);
    let args = SoftwarePipelineBuilder
        .build(request.clone())
        .command(&request.inputs)
        .arguments
        .join(" ");

    assert!(!args.contains("-mastering_display"), "got: {args}");
    assert!(!args.contains("-content_light"), "got: {args}");
}

#[test]
fn every_sdr_tonemap_path_strips_stale_hdr_metadata() {
    // ADR 0064: "A pipeline must never copy source HDR metadata onto
    // transformed SDR frames." Measured against FFmpeg 9 before this was
    // added: mastering-display and content-light side data survived the
    // tone-map and was written onto BT.709 output by libx264, libx265, and
    // h264_nvenc alike, telling a display to expect a 1000-nit BT.2020 grade
    // for pictures that are neither.
    let state = FrameState::new(FrameSize::new(1920, 1080));

    for filter in [
        tonemap_filter(&state, PixelFormat::Yuv420P, TonemapAlgorithm::Hable),
        tonemap_cuda_filter(PixelFormat::Yuv420P, TonemapAlgorithm::Hable),
        tonemap_vaapi_filter(TonemapAlgorithm::Hable),
        tonemap_qsv_filter(),
    ] {
        assert!(
            filter.contains("sidedata=mode=delete:type=MASTERING_DISPLAY_METADATA"),
            "{filter}"
        );
        assert!(
            filter.contains("sidedata=mode=delete:type=CONTENT_LIGHT_LEVEL"),
            "{filter}"
        );
        // Dolby Vision metadata must go too: a tone-mapped SDR picture is not
        // Dolby Vision either.
        assert!(
            filter.contains("sidedata=mode=delete:type=DOVI_RPU_BUFFER"),
            "{filter}"
        );
    }
}

#[test]
fn hdr10_output_keeps_its_metadata_and_never_strips_it() {
    // The strip belongs to the SDR path only; an HDR10 profile must retain the
    // metadata that makes its output meaningful.
    let request = PipelineRequest::new(InputFile::video("/media/hdr10.mkv"), hdr10_desired())
        .with_source_video_stream(source_with_transfer(Some("smpte2084"), true));
    let args = SoftwarePipelineBuilder
        .build(request.clone())
        .command(&request.inputs)
        .arguments
        .join(" ");

    assert!(!args.contains("sidedata=mode=delete"), "got: {args}");
}

#[test]
fn an_unproven_dolby_vision_source_is_refused_before_transcoding() {
    // ADR 0064: reject rather than produce plausible but incorrect colour.
    let mut source = SourceVideoStream::new(SourceVideoCodec::Hevc);
    source.dolby_vision = Some(ersatzrs_ffmpeg::DolbyVisionConfiguration {
        profile: 5,
        bl_signal_compatibility_id: 0,
        rpu_present: true,
        el_present: false,
        bl_present: true,
    });
    let request = PipelineRequest::new(
        InputFile::video("/media/dv5.mkv"),
        DesiredVideoState::h264_1080p(),
    )
    .with_source_video_stream(source);

    let reason =
        ersatzrs_ffmpeg::unsupported_source_reason(&request).expect("profile 5 must be refused");
    assert!(reason.contains("no HDR10 fallback"), "{reason}");
}

#[test]
fn a_compatible_dolby_vision_source_is_played_from_its_base_layer() {
    // Profile 8 declaring HDR10 base-layer compatibility, which is what a real
    // profile 8.1 file reports.
    let mut source = SourceVideoStream::new(SourceVideoCodec::Hevc);
    source.dolby_vision = Some(ersatzrs_ffmpeg::DolbyVisionConfiguration {
        profile: 8,
        bl_signal_compatibility_id: 1,
        rpu_present: true,
        el_present: false,
        bl_present: true,
    });
    let request = PipelineRequest::new(
        InputFile::video("/media/dv81.mkv"),
        DesiredVideoState::h264_1080p(),
    )
    .with_source_video_stream(source);

    assert_eq!(ersatzrs_ffmpeg::unsupported_source_reason(&request), None);
}

#[test]
fn stream_copy_never_refuses_a_dolby_vision_source() {
    // Copy hands the elementary stream to the client untouched, so ErsatzRS is
    // not interpreting the colour and has no basis to refuse.
    let mut source = SourceVideoStream::new(SourceVideoCodec::Hevc);
    source.dolby_vision = Some(ersatzrs_ffmpeg::DolbyVisionConfiguration {
        profile: 5,
        bl_signal_compatibility_id: 0,
        rpu_present: true,
        el_present: false,
        bl_present: true,
    });
    let desired = DesiredVideoState {
        video_format: FFmpegProfileVideoFormat::Copy,
        ..DesiredVideoState::h264_1080p()
    };
    let request = PipelineRequest::new(InputFile::video("/media/dv5.mkv"), desired)
        .with_source_video_stream(source);

    assert_eq!(ersatzrs_ffmpeg::unsupported_source_reason(&request), None);
}

#[test]
fn an_unproven_cuda_graph_demotes_nvenc_to_software() {
    // ADR 0014: NvencCudaScale has no further hardware fallback, so a runtime
    // that probed and did not prove it must not emit CUDA/NVENC arguments.
    let mut report = capability_report_for_tests();
    report.deny_hardware_graph(HardwareGraphDenial::new(
        HardwareGraphOperation::NvencCudaScale,
        "",
        GraphFailureKind::CudaInterop,
    ));
    let request = hdr_nvenc_request(report);

    let args = NvencPipelineBuilder
        .build(request.clone())
        .command(&request.inputs)
        .arguments
        .join(" ");

    assert!(!args.contains("nvenc"), "got: {args}");
    assert!(!args.contains("scale_cuda"), "got: {args}");
    assert!(!args.contains("hwupload_cuda"), "got: {args}");
}

#[test]
fn a_probed_runtime_requires_positive_proof_for_the_vulkan_graph() {
    // An unclassified probe failure leaves the graph neither proven nor denied.
    // It must not be selectable just because no denial was recorded.
    let mut report = capability_report_for_tests();
    report.hwaccels.insert("vulkan".to_owned());
    // The runtime probed: CUDA proved, Vulkan produced no verdict.
    report.prove_hardware_graph(HardwareGraphOperation::NvencCudaScale, "");
    let request = hdr_nvenc_request(report);

    let args = NvencPipelineBuilder
        .build(request.clone())
        .command(&request.inputs)
        .arguments
        .join(" ");

    assert!(!args.contains("libplacebo"), "got: {args}");
    // The proven CUDA fallback still carries the encode.
    assert!(args.contains("h264_nvenc"), "got: {args}");
}

#[test]
fn a_runtime_that_never_probed_keeps_its_previous_behavior() {
    // An older database has neither proofs nor denials. Demoting it would be a
    // silent regression, so the advertised hwaccel still decides.
    let mut report = capability_report_for_tests();
    report.hwaccels.insert("vulkan".to_owned());
    assert!(!report.has_probed_hardware_graphs());
    let request = hdr_nvenc_request(report);

    let args = NvencPipelineBuilder
        .build(request.clone())
        .command(&request.inputs)
        .arguments
        .join(" ");

    assert!(args.contains("libplacebo"), "got: {args}");
}

// ── p8a-02: rejection is enforced at the command boundary ───────────────────

fn dolby_vision_request(profile: u8, compatibility: u8, copy: bool) -> PipelineRequest {
    let mut source = SourceVideoStream::new(SourceVideoCodec::Hevc);
    source.dolby_vision = Some(ersatzrs_ffmpeg::DolbyVisionConfiguration {
        profile,
        bl_signal_compatibility_id: compatibility,
        rpu_present: true,
        el_present: profile == 7,
        bl_present: true,
    });
    let desired = if copy {
        DesiredVideoState {
            video_format: FFmpegProfileVideoFormat::Copy,
            ..DesiredVideoState::h264_1080p()
        }
    } else {
        DesiredVideoState::h264_1080p()
    };
    PipelineRequest::new(InputFile::video("/media/dv.mkv"), desired)
        .with_source_video_stream(source)
}

#[test]
fn the_command_builder_refuses_unplayable_dolby_vision_profiles() {
    // There is no command to spawn, which is what makes the refusal real: a
    // caller cannot accidentally start FFmpeg on a source that would decode to
    // plausible but incorrect colour.
    for (profile, compatibility, expected) in [
        (5_u8, 0_u8, "no HDR10 fallback"),
        (7, 0, "dual-layer"),
        (9, 1, "unrecognised"),
        (8, 0, "no conventional base-layer compatibility"),
    ] {
        let request = dolby_vision_request(profile, compatibility, false);
        let error =
            ersatzrs_ffmpeg::build_pipeline_command(HardwareAccelerationKind::None, request)
                .expect_err("profile {profile} must be refused");

        assert!(
            error.reason.contains(expected),
            "profile {profile}: {}",
            error.reason
        );
    }
}

#[test]
fn the_command_builder_allows_a_compatible_dolby_vision_profile() {
    // Profile 8.1: the base layer is a conforming HDR10 signal.
    let request = dolby_vision_request(8, 1, false);

    let command = ersatzrs_ffmpeg::build_pipeline_command(HardwareAccelerationKind::None, request)
        .expect("profile 8.1 plays from its HDR10 base layer");

    assert!(command
        .arguments
        .iter()
        .any(|argument| argument == "pipe:1"));
}

#[test]
fn the_command_builder_allows_stream_copy_of_any_dolby_vision_profile() {
    // Copy hands the elementary stream to the client untouched, so ErsatzRS is
    // not interpreting the colour and has no basis to refuse.
    for profile in [5_u8, 7, 8] {
        let request = dolby_vision_request(profile, 0, true);

        assert!(
            ersatzrs_ffmpeg::build_pipeline_command(HardwareAccelerationKind::None, request)
                .is_ok(),
            "stream copy of profile {profile} must be allowed"
        );
    }
}

#[test]
fn a_source_without_dolby_vision_is_never_refused() {
    let request = PipelineRequest::new(
        InputFile::video("/media/plain.mkv"),
        DesiredVideoState::h264_1080p(),
    )
    .with_source_video_stream(SourceVideoStream::new(SourceVideoCodec::Hevc));

    assert!(
        ersatzrs_ffmpeg::build_pipeline_command(HardwareAccelerationKind::None, request).is_ok()
    );
}

#[test]
fn the_selected_graph_is_derived_from_the_command_that_actually_ran() {
    // Derived from argv rather than tracked alongside it, so a runtime failure
    // cannot be attributed to a graph the spawned command did not use.
    let mut report = capability_report_for_tests();
    report.hwaccels.insert("vulkan".to_owned());
    let vulkan = hdr_nvenc_request(report);
    let vulkan_command = NvencPipelineBuilder
        .build(vulkan.clone())
        .command(&vulkan.inputs);
    assert_eq!(
        ersatzrs_ffmpeg::selected_hardware_graph(&vulkan_command),
        Some(HardwareGraphOperation::NvencHdrVulkanLibplacebo)
    );

    // Vulkan denied: the same source falls through to the CUDA graph, and the
    // attribution follows.
    let mut report = capability_report_for_tests();
    report.hwaccels.insert("vulkan".to_owned());
    report.deny_hardware_graph(HardwareGraphDenial::new(
        HardwareGraphOperation::NvencHdrVulkanLibplacebo,
        "",
        GraphFailureKind::CudaInterop,
    ));
    report.prove_hardware_graph(HardwareGraphOperation::NvencCudaScale, "");
    let cuda = hdr_nvenc_request(report);
    let cuda_command = NvencPipelineBuilder
        .build(cuda.clone())
        .command(&cuda.inputs);
    assert_eq!(
        ersatzrs_ffmpeg::selected_hardware_graph(&cuda_command),
        Some(HardwareGraphOperation::NvencCudaScale)
    );

    // A pure software command belongs to no hardware graph.
    let software = PipelineRequest::new(
        InputFile::video("/media/movie.mkv"),
        DesiredVideoState::h264_1080p(),
    )
    .with_source_video_stream(SourceVideoStream::new(SourceVideoCodec::H264));
    let software_command = SoftwarePipelineBuilder
        .build(software.clone())
        .command(&software.inputs);
    assert_eq!(
        ersatzrs_ffmpeg::selected_hardware_graph(&software_command),
        None
    );
}

#[test]
fn a_runtime_graph_denial_makes_the_rebuild_select_the_next_proven_graph() {
    // This is what the supervisor retry relies on: folding the learned denial
    // into the capability report is enough for the rebuilt command to pick the
    // fallback, with no separate graph-selection plumbing.
    // Startup probing proved both graphs on this machine.
    let mut report = capability_report_for_tests();
    report.hwaccels.insert("vulkan".to_owned());
    report.prove_hardware_graph(HardwareGraphOperation::NvencHdrVulkanLibplacebo, "");
    report.prove_hardware_graph(HardwareGraphOperation::NvencCudaScale, "");

    let before = hdr_nvenc_request(report.clone());
    let before_args = NvencPipelineBuilder
        .build(before.clone())
        .command(&before.inputs)
        .arguments
        .join(" ");
    assert!(before_args.contains("libplacebo"), "{before_args}");

    // The supervisor learns this from the failing stderr.
    report.deny_hardware_graph(HardwareGraphDenial::new(
        HardwareGraphOperation::NvencHdrVulkanLibplacebo,
        "",
        GraphFailureKind::CudaInterop,
    ));

    let after = hdr_nvenc_request(report);
    let after_args = NvencPipelineBuilder
        .build(after.clone())
        .command(&after.inputs)
        .arguments
        .join(" ");
    assert!(!after_args.contains("libplacebo"), "{after_args}");
    assert!(after_args.contains("h264_nvenc"), "{after_args}");
}
