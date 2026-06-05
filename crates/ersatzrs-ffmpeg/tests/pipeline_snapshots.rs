use std::collections::BTreeMap;
use std::time::Duration;

use ersatzrs_domain::{
    FFmpegProfile, FFmpegProfileAudioFormat, FFmpegProfileBitDepth, FFmpegProfileTonemapAlgorithm,
    FFmpegProfileVideoFormat, FilterMode, GraphicsElement, GraphicsElementConfig,
    HardwareAccelerationKind, ImageGraphicsConfig, MotionEndBehavior, MotionGraphicsConfig,
    NormalizeLoudnessMode, Resolution, ScalingBehavior, ScriptGraphicsConfig, StreamingMode,
    StyleDefinition, SubtitleGraphicsConfig, TextAlignment, TextFit, TextGraphicsConfig,
    VaapiDriver, VideoScanKind, WatermarkLocation, WatermarkSize,
};
use ersatzrs_ffmpeg::{
    audio_pad_filter, audio_resample_filter, capability_report_for_tests, colorspace_filter,
    cuda_software_fallback_upload_filter, deinterlace_filter, deinterlace_qsv_filter,
    deinterlace_vaapi_filter, frame_rate_filter, hardware_download_filter, hardware_upload_filter,
    lower_third_overlay, normalize_loudness_filter, song_progress_filter,
    subtitle_graphics_lavfi_input, subtitle_scale_npp_filter, tonemap_cuda_filter, tonemap_filter,
    tonemap_qsv_filter, tonemap_vaapi_filter, translate_watermark_opacity_expression,
    yadif_cuda_filter, yadif_filter, AmfPipelineBuilder, AudioFilter, ColorParams, ComplexFilter,
    ConcatPlaylist, DesiredVideoState, DrawvgOverlay, FilterChain, FrameDataLocation, FrameRate,
    FrameSize, FrameState, GraphicsOverlayFilter, GraphicsPipelineOverlay, GraphicsRenderContext,
    GraphicsTemplateRenderer, HlsOutput, InputFile, NvencPipelineBuilder, OutputFormat,
    OutputMetadata, PipelineBuilder, PipelineRequest, PixelFormat, QsvPipelineBuilder,
    RealtimeMode, RkmppPipelineBuilder, SoftwarePipelineBuilder, SourceAudioCodec,
    SourceAudioStream, SourceColorParams, SourceVideoCodec, SourceVideoStream, SvgTemplate,
    TonemapAlgorithm, V4l2m2mPipelineBuilder, VaapiPipelineBuilder, VideoToolboxPipelineBuilder,
    WatermarkFadeKind, WatermarkFadePoint, WatermarkOverlay,
};

fn h264_request(desired: DesiredVideoState) -> PipelineRequest {
    PipelineRequest::new(InputFile::video("/media/movie.mkv"), desired)
        .with_source_video_stream(SourceVideoStream::new(SourceVideoCodec::H264))
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

    insta::assert_snapshot!(command.arguments.join(" "));
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
    assert!(desired.allow_b_frames);
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
    assert!(!desired.pad_audio);
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
    let request = PipelineRequest::new(InputFile::video("/media/movie.mkv"), desired)
        .with_source_video_stream(source);

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
}

#[test]
fn vaapi_pipeline_profile_pad_uploads_after_software_pad() {
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

    insta::assert_snapshot!(command.arguments.join(" "));
}

#[test]
fn software_pipeline_seeking_dts_uses_core_only_decoder_snapshot() {
    let desired = DesiredVideoState::h264_1080p();
    let mut request =
        h264_request(desired)
            .with_source_audio_streams(vec![
                SourceAudioStream::new(SourceAudioCodec::Dts).with_index(1)
            ]);
    request.state.start_seconds = Some(120);

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
fn nvenc_pipeline_snapshot() {
    let desired = DesiredVideoState::h264_1080p();
    let request = h264_request(desired);

    let pipeline = NvencPipelineBuilder.build(request.clone());
    let command = pipeline.command(&request.inputs);

    insta::assert_snapshot!(command.arguments.join(" "));
}

#[test]
fn nvenc_pipeline_uses_software_decode_when_cuvid_decoder_missing_snapshot() {
    let desired = DesiredVideoState::h264_1080p();
    let mut report = capability_report_for_tests();
    report.decoders.remove("h264_cuvid");
    let request = h264_request(desired).with_runtime_capabilities(report);

    let pipeline = NvencPipelineBuilder.build(request.clone());
    let command = pipeline.command(&request.inputs);

    insta::assert_snapshot!(command.arguments.join(" "));
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

    insta::assert_snapshot!(command.arguments.join(" "));
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

    insta::assert_snapshot!(command.arguments.join(" "));
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

    insta::assert_snapshot!(format!(
        "env={:?}\nargs={}",
        command.environment,
        command.arguments.join(" ")
    ));
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
    request.state.start_seconds = Some(12);

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
    request.inputs.push(InputFile::watermark("/media/logo.png"));
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
    let mut request = h264_request(DesiredVideoState::h264_1080p());
    request.inputs.push(InputFile::watermark("/media/logo.png"));
    request.watermarks.push(WatermarkOverlay {
        opacity_expression: Some("if(time_of_day_seconds % 60 < 1, 1, 0.25)".to_owned()),
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

#[test]
fn watermark_input_loops_still_images() {
    let input = InputFile::watermark("/media/logo.png");
    assert_eq!(input.input_options, ["-loop", "1"]);
}

#[test]
fn opacity_expression_translation_maps_ersatztv_time_symbols() {
    assert_eq!(
        translate_watermark_opacity_expression(
            "if(time_of_day_seconds < content_time_seconds, content_total_seconds, channel_time_seconds)"
        ),
        "if(t < t, NAN, t)"
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
fn pipeline_emits_tonemap_for_hdr_source_snapshot() {
    let mut source = SourceVideoStream::new(SourceVideoCodec::Hevc);
    source.color_params = SourceColorParams {
        is_hdr: true,
        is_bt2020_ten: true,
    };
    let request = PipelineRequest::new(
        InputFile::video("/media/hdr-movie.mkv"),
        DesiredVideoState::h264_1080p(),
    )
    .with_source_video_stream(source);

    let pipeline = NvencPipelineBuilder.build(request.clone());
    let command = pipeline.command(&request.inputs);

    insta::assert_snapshot!(command.arguments.join(" "));
}

#[test]
fn nvenc_pipeline_uses_vulkan_hwaccel_for_hdr_tonemap_snapshot() {
    let mut source = SourceVideoStream::new(SourceVideoCodec::Hevc);
    source.color_params = SourceColorParams {
        is_hdr: true,
        is_bt2020_ten: false,
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
fn pipeline_emits_frame_rate_filter_when_source_differs_snapshot() {
    let mut source = SourceVideoStream::new(SourceVideoCodec::H264);
    source.frame_rate = Some(FrameRate::new(25, 1));
    let mut desired = DesiredVideoState::h264_1080p();
    desired.frame_rate = Some(FrameRate::new(60, 1));
    let request = PipelineRequest::new(InputFile::video("/media/movie.mkv"), desired)
        .with_source_video_stream(source);

    let pipeline = SoftwarePipelineBuilder.build(request.clone());
    let command = pipeline.command(&request.inputs);

    insta::assert_snapshot!(command.arguments.join(" "));
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
    insta::assert_snapshot!("graphics_element_overlay_plan", overlay_plan);

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
    insta::assert_snapshot!("graphics_element_drawvg_filters", filters);

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
    insta::assert_snapshot!("graphics_element_script_overlay", overlay);

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
