//! UCU15 row274: Native short-GOP argv and explicit Windows NVENC qualification.
use ersatzrs_domain::{FFmpegProfileVideoFormat, HardwareAccelerationKind};
use ersatzrs_ffmpeg::{
    build_pipeline_for_acceleration, capability_report_for_tests, DesiredVideoState, FrameRate,
    FrameSize, HlsOutput, InputFile, NvencPipelineBuilder, OutputFormat, PipelineBuilder,
    PipelineRequest, SourceVideoCodec, SourceVideoStream,
};
use std::{path::PathBuf, time::Duration};

#[test]
#[ignore = "exports exact Native low-fps commands for explicit native NVIDIA qualification"]
fn native_low_fps_nvenc_exports_h264_hevc_b_frame_policy_matrix() {
    let folder = artifacts();
    std::fs::create_dir_all(&folder).unwrap();
    let input = folder.join("still.png");
    let png = include_bytes!("fixtures/graphics/static-blue.png");
    std::fs::write(&input, png).unwrap();
    let width = u32::from_be_bytes(png[16..20].try_into().unwrap());
    let height = u32::from_be_bytes(png[20..24].try_into().unwrap());
    for (codec, name) in [
        (FFmpegProfileVideoFormat::H264, "h264"),
        (FFmpegProfileVideoFormat::Hevc, "hevc"),
    ] {
        for allow_b_frames in [false, true] {
            let id = format!("{name}-b{allow_b_frames}");
            let output = folder.join(&id);
            std::fs::create_dir_all(&output).unwrap();
            let mut source = SourceVideoStream::new(SourceVideoCodec::Unknown);
            source.is_still_image = true;
            source.frame_size = Some(FrameSize::new(width, height));
            source.pixel_format_name = Some("rgb24".into());
            source.frame_rate = Some(FrameRate::new(1, 1));
            let mut desired = DesiredVideoState::h264_1080p();
            desired.frame_size = FrameSize::new(160, 90);
            desired.scaled_size = desired.frame_size;
            desired.frame_rate = Some(FrameRate::new(1, 1));
            desired.video_format = codec;
            desired.video_profile = None;
            desired.video_preset = Some("p4".into());
            desired.allow_b_frames = allow_b_frames;
            let mut image = InputFile::video(input.to_str().unwrap());
            image
                .input_options
                .extend(["-framerate".into(), "1".into()]);
            let mut request = PipelineRequest::new(image, desired)
                .without_audio()
                .with_source_video_stream(source)
                .with_runtime_capabilities(capability_report_for_tests());
            request.state.report_progress = false;
            request.state.thread_count = Some(2);
            request.state.duration = Some(Duration::from_secs(8));
            request.state.output_format = OutputFormat::HlsMp4;
            let segment = output.join("segment%03d.m4s");
            let playlist = output.join("index.m3u8");
            request.state.hls = Some(HlsOutput {
                mpegts_segments: None,
                segment_template: segment.to_string_lossy().into_owned(),
                init_template: Some("init.mp4".into()),
                playlist_path: playlist.to_string_lossy().into_owned(),
                segment_options: None,
                is_first_transcode: true,
                discontinuity: false,
                troubleshooting: true,
            });
            let command = NvencPipelineBuilder
                .build(request.clone())
                .command(&request.inputs);
            assert!(command.arguments.windows(2).any(|a| a == ["-g", "2"]));
            assert_eq!(b_frames(&command.arguments), ["0"]);
            assert!(command
                .arguments
                .windows(2)
                .any(|a| a == ["-codec:v", &format!("{name}_nvenc")]));
            let data = serde_json::json!({"id":id,"codec":name,"allow_b_frames":allow_b_frames,"input":input,"playlist":playlist,"segment_template":segment,"arguments":command.arguments,"environment":command.environment.iter().map(|e|(&e.name,&e.value)).collect::<Vec<_>>()});
            std::fs::write(
                folder.join(format!("{id}.json")),
                serde_json::to_vec_pretty(&data).unwrap(),
            )
            .unwrap();
        }
    }
}

// Explicit opt-in test artifact destination; no production configuration lookup.
#[allow(
    clippy::disallowed_methods,
    reason = "Ignored qualification uses an explicit test-only artifact directory."
)]
fn artifacts() -> PathBuf {
    std::env::var_os("ERSATZRS_NATIVE_TIMING_ARTIFACT_DIR")
        .map(PathBuf::from)
        .expect("explicit task artifact directory")
}

fn gop_request(codec: FFmpegProfileVideoFormat, rate: FrameRate, allow: bool) -> PipelineRequest {
    let mut desired = DesiredVideoState::h264_1080p();
    desired.video_format = codec;
    desired.frame_rate = Some(rate);
    desired.allow_b_frames = allow;
    desired.video_profile = None;
    desired.video_preset = Some("p4".into());
    let mut report = capability_report_for_tests();
    for encoder in &mut report.nvidia_encoders {
        encoder.max_b_frames = Some(4);
    }
    let mut request = PipelineRequest::new(InputFile::video("synthetic.png"), desired)
        .with_runtime_capabilities(report);
    request.state.output_format = OutputFormat::HlsMp4;
    request.state.hls = Some(HlsOutput {
        mpegts_segments: None,
        segment_template: "segment%03d.m4s".into(),
        init_template: Some("init.mp4".into()),
        playlist_path: "index.m3u8".into(),
        segment_options: None,
        is_first_transcode: true,
        discontinuity: false,
        troubleshooting: true,
    });
    request
}

fn args(request: PipelineRequest, acceleration: HardwareAccelerationKind) -> Vec<String> {
    let inputs = request.inputs.clone();
    build_pipeline_for_acceleration(acceleration, request)
        .command(&inputs)
        .arguments
}

fn b_frames(arguments: &[String]) -> Vec<&str> {
    arguments
        .windows(2)
        .filter(|pair| pair[0] == "-bf")
        .map(|pair| pair[1].as_str())
        .collect()
}

#[test]
fn native_nvenc_short_gop_limits_preserve_ordinary_presets_and_explicit_off() {
    for (codec, max, rate, expected) in [
        (FFmpegProfileVideoFormat::H264, 7, FrameRate::new(4, 1), "6"),
        (FFmpegProfileVideoFormat::Hevc, 5, FrameRate::new(3, 1), "4"),
    ] {
        let mut req = gop_request(codec, rate, true);
        for entry in &mut req.runtime_capabilities.as_mut().unwrap().nvidia_encoders {
            entry.max_b_frames = Some(max);
        }
        assert_eq!(
            b_frames(&args(req, HardwareAccelerationKind::Nvenc)),
            [expected]
        );
    }
    for codec in [
        FFmpegProfileVideoFormat::H264,
        FFmpegProfileVideoFormat::Hevc,
    ] {
        for (rate, expected) in [
            (FrameRate::new(1, 2), Some("0")),
            (FrameRate::new(1, 1), Some("0")),
            (FrameRate::new(3, 2), Some("1")),
            (FrameRate::new(2, 1), Some("2")),
            (FrameRate::new(5, 2), Some("3")),
            (FrameRate::new(3, 1), None),
            (FrameRate::new(25, 1), None),
        ] {
            let req = gop_request(codec, rate, true);
            let command = args(req, HardwareAccelerationKind::Nvenc);
            assert_eq!(b_frames(&command), expected.into_iter().collect::<Vec<_>>());
            let off = args(
                gop_request(codec, rate, false),
                HardwareAccelerationKind::Nvenc,
            );
            assert_eq!(b_frames(&off), ["0"]);
        }
    }
}

#[test]
fn native_nvenc_minimum_gop_is_safe_without_numeric_capability_and_other_paths_are_unchanged() {
    let mut fallback = gop_request(FFmpegProfileVideoFormat::H264, FrameRate::new(1, 1), true);
    let mut source = SourceVideoStream::new(SourceVideoCodec::Unknown);
    source.pixel_format_name = Some("rgb24".into());
    fallback.source_video_stream = Some(source);
    let command = args(fallback, HardwareAccelerationKind::Nvenc);
    assert!(command.windows(2).any(|a| a == ["-codec:v", "h264_nvenc"]));
    assert_eq!(b_frames(&command), ["0"]);
    for report in [true, false] {
        let mut req = gop_request(FFmpegProfileVideoFormat::H264, FrameRate::new(1, 1), true);
        if report {
            for entry in &mut req.runtime_capabilities.as_mut().unwrap().nvidia_encoders {
                entry.max_b_frames = None;
            }
        } else {
            req.runtime_capabilities = None;
        }
        assert_eq!(b_frames(&args(req, HardwareAccelerationKind::Nvenc)), ["0"]);
    }
    for acceleration in [
        HardwareAccelerationKind::None,
        HardwareAccelerationKind::Qsv,
        HardwareAccelerationKind::Vaapi,
    ] {
        let req = gop_request(FFmpegProfileVideoFormat::H264, FrameRate::new(1, 1), true);
        assert!(b_frames(&args(req, acceleration)).is_empty());
    }
    for format in [OutputFormat::Mp4, OutputFormat::MpegTs] {
        let mut req = gop_request(FFmpegProfileVideoFormat::H264, FrameRate::new(1, 1), true);
        req.state.output_format = format;
        assert!(b_frames(&args(req, HardwareAccelerationKind::Nvenc)).is_empty());
    }
    let mut copy = gop_request(FFmpegProfileVideoFormat::Copy, FrameRate::new(1, 1), true);
    copy.desired.video_profile = None;
    assert!(b_frames(&args(copy, HardwareAccelerationKind::Nvenc)).is_empty());
}

#[test]
fn native_nvenc_short_gop_commands_snapshot() {
    let h264 = args(
        gop_request(FFmpegProfileVideoFormat::H264, FrameRate::new(1, 1), true),
        HardwareAccelerationKind::Nvenc,
    );
    let hevc = args(
        gop_request(FFmpegProfileVideoFormat::Hevc, FrameRate::new(2, 1), true),
        HardwareAccelerationKind::Nvenc,
    );
    insta::assert_snapshot!(format!(
        "H264 1fps: {}\nHEVC 2fps: {}",
        h264.join(" "),
        hevc.join(" ")
    ));
}
