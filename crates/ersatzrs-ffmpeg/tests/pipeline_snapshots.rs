use std::collections::BTreeMap;
use std::time::Duration;

use ersatzrs_domain::{
    FFmpegProfile, FFmpegProfileAudioFormat, FFmpegProfileBitDepth, FFmpegProfileTonemapAlgorithm,
    FFmpegProfileVideoFormat, FilterMode, HardwareAccelerationKind, NormalizeLoudnessMode,
    Resolution, ScalingBehavior, StreamingMode, VideoScanKind, WatermarkLocation, WatermarkSize,
};
use ersatzrs_ffmpeg::{
    capability_report_for_tests, colorspace_filter, deinterlace_filter, deinterlace_qsv_filter,
    deinterlace_vaapi_filter, frame_rate_filter, hardware_download_filter, hardware_upload_filter,
    lower_third_overlay, normalize_loudness_filter, song_progress_filter, tonemap_cuda_filter,
    tonemap_filter, tonemap_qsv_filter, tonemap_vaapi_filter,
    translate_watermark_opacity_expression, yadif_cuda_filter, yadif_filter, AmfPipelineBuilder,
    AudioFilter, ColorParams, ComplexFilter, ConcatPlaylist, DesiredVideoState, DrawvgOverlay,
    FilterChain, FrameDataLocation, FrameRate, FrameSize, FrameState, GraphicsTemplateRenderer,
    HlsOutput, InputFile, NvencPipelineBuilder, OutputFormat, PipelineBuilder, PipelineRequest,
    PixelFormat, QsvPipelineBuilder, RkmppPipelineBuilder, SoftwarePipelineBuilder,
    SourceColorParams, SourceVideoCodec, SourceVideoStream, SvgTemplate, TonemapAlgorithm,
    V4l2m2mPipelineBuilder, VaapiPipelineBuilder, VideoToolboxPipelineBuilder, WatermarkFadeKind,
    WatermarkFadePoint, WatermarkOverlay,
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

#[test]
fn software_pipeline_single_video_mpegts_snapshot() {
    let desired = DesiredVideoState::h264_1080p();
    let request = h264_request(desired);

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
fn nvenc_pipeline_snapshot() {
    let desired = DesiredVideoState::h264_1080p();
    let request = h264_request(desired);

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
fn qsv_pipeline_snapshot() {
    let desired = DesiredVideoState::h264_1080p();
    let request = h264_request(desired);

    let pipeline = QsvPipelineBuilder.build(request.clone());
    let command = pipeline.command(&request.inputs);

    insta::assert_snapshot!(command.arguments.join(" "));
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

    insta::assert_snapshot!(command.arguments.join(" "));
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
        InputFile::graphics("color=c=black@0.0:s=16x16:d=86400")
            .with_input_options(["-f", "lavfi"]),
    ];
    inputs[2]
        .filter_steps
        .push(ersatzrs_ffmpeg::watermark_opacity_filter(75));

    let overlay = DrawvgOverlay {
        svg_path: "/tmp/ErsatzRS/lower-third.svg".into(),
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
        graphics_overlay: vec![overlay.filter()],
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
        "/tmp/ErsatzRS/lower-third.svg",
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
