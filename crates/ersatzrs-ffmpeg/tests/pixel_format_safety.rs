//! UCU15 row277: positive source format metadata must survive decoder dispatch.
use ersatzrs_domain::{FFmpegProfileVideoFormat, HardwareAccelerationKind};
use ersatzrs_ffmpeg::{
    build_pipeline_for_acceleration, capability_report_for_tests, DesiredVideoState,
    FfmpegCapability, FrameRate, FrameSize, HardwareCapabilitiesFactory, InputFile,
    PipelineRequest, PixelFormat, SourceVideoCodec, SourceVideoStream,
};

fn request(format: &str) -> PipelineRequest {
    let mut source = SourceVideoStream::new(SourceVideoCodec::Hevc);
    source.pixel_format = PixelFormat::from_ffmpeg_name(format);
    source.pixel_format_name = Some(format.into());
    source.frame_size = Some(FrameSize::new(320, 180));
    source.frame_rate = Some(FrameRate::new(25, 1));
    let mut desired = DesiredVideoState::h264_1080p();
    desired.video_preset = None;
    PipelineRequest::new(InputFile::video("/media/synthetic.mkv"), desired)
        .with_source_video_stream(source)
        .with_runtime_capabilities(capability_report_for_tests())
}

#[test]
fn native_source_format_gate_covers_sampling_depth_unknown_names_and_missing_metadata() {
    let report = capability_report_for_tests();
    for format in [
        "yuv422p",
        "yuv444p",
        "yuv422p10le",
        "yuv444p10le",
        "yuv420p12le",
        "yuv422p12le",
        "yuv444p12le",
        "yuv420p16le",
        "rgb24",
        "gbrp12le",
        "unknown",
    ] {
        let req = request(format);
        let source = req.source_video_stream.as_ref().unwrap();
        assert!(source.requires_software_pixel_format(), "{format}");
        for acceleration in [
            HardwareAccelerationKind::Nvenc,
            HardwareAccelerationKind::Qsv,
            HardwareAccelerationKind::Vaapi,
            HardwareAccelerationKind::Amf,
            HardwareAccelerationKind::V4l2m2m,
            HardwareAccelerationKind::Rkmpp,
        ] {
            assert_eq!(
                HardwareCapabilitiesFactory::new(&report)
                    .for_acceleration(acceleration)
                    .can_decode(source),
                FfmpegCapability::Software,
                "{format}/{acceleration:?}"
            );
        }
    }
    for format in ["yuv420p", "yuvj420p", "yuv420p10le", "NV12", " yuv420p10 "] {
        let req = request(format);
        assert!(
            !req.source_video_stream
                .unwrap()
                .requires_software_pixel_format(),
            "{format}"
        );
    }
    assert!(!SourceVideoStream::new(SourceVideoCodec::H264).requires_software_pixel_format());
}

#[test]
fn native_known_formats_keep_the_same_commands_and_copy_never_adds_conversion() {
    for format in ["yuv420p", "yuvj420p", "yuv420p10le", "nv12"] {
        for acceleration in [
            HardwareAccelerationKind::None,
            HardwareAccelerationKind::Nvenc,
            HardwareAccelerationKind::Qsv,
            HardwareAccelerationKind::Vaapi,
        ] {
            let req = request(format);
            let expected = build_pipeline_for_acceleration(acceleration, req.clone())
                .command(&req.inputs)
                .arguments;
            let mut old = req.clone();
            old.source_video_stream.as_mut().unwrap().pixel_format_name = None;
            assert_eq!(
                build_pipeline_for_acceleration(acceleration, old)
                    .command(&req.inputs)
                    .arguments,
                expected,
                "{format}/{acceleration:?}"
            );
        }
    }
    let mut req = request("yuv444p12le");
    req.desired.video_format = FFmpegProfileVideoFormat::Copy;
    let args = build_pipeline_for_acceleration(HardwareAccelerationKind::None, req.clone())
        .command(&req.inputs)
        .arguments;
    assert!(!args.iter().any(|arg| arg == "-pix_fmt"));
    assert!(args.windows(2).any(|a| a == ["-codec:v", "copy"]));
}

#[test]
fn native_unmodeled_format_decode_gate_survives_missing_runtime_reports() {
    for acceleration in [
        HardwareAccelerationKind::Nvenc,
        HardwareAccelerationKind::Qsv,
        HardwareAccelerationKind::Vaapi,
        HardwareAccelerationKind::Rkmpp,
        HardwareAccelerationKind::V4l2m2m,
    ] {
        let mut req = request("yuv444p12le");
        req.runtime_capabilities = None;
        let args = build_pipeline_for_acceleration(acceleration, req.clone())
            .command(&req.inputs)
            .arguments;
        assert!(
            !args.iter().any(|arg| arg == "-hwaccel"
                && args
                    .windows(2)
                    .any(|pair| pair[0] == "-hwaccel" && pair[1] != "none")),
            "{acceleration:?}: {args:?}"
        );
        assert!(
            !args.iter().any(|arg| arg.contains("_cuvid")),
            "{acceleration:?}"
        );
    }
}

#[test]
fn native_nvenc_unmodeled_source_uses_software_filters_and_retains_encoder_snapshot() {
    let mut req = request("yuv444p12le");
    req.desired.video_preset = Some("p1".into());
    let command = build_pipeline_for_acceleration(HardwareAccelerationKind::Nvenc, req.clone())
        .command(&req.inputs);
    assert!(command
        .arguments
        .windows(2)
        .any(|a| a == ["-codec:v", "h264_nvenc"]));
    assert!(command
        .arguments
        .windows(2)
        .any(|a| a == ["-hwaccel", "none"]));
    assert!(command
        .arguments
        .windows(2)
        .any(|a| a == ["-pix_fmt", "yuv420p"]));
    assert!(!command
        .arguments
        .iter()
        .any(|a| a.contains("hwupload") || a.contains("scale_cuda")));
    insta::assert_snapshot!(command.arguments.join(" "));
}

#[test]
fn native_twelve_bit_source_software_output_normalizes_to_profile_snapshot() {
    let req = request("yuv420p12le");
    let command = build_pipeline_for_acceleration(HardwareAccelerationKind::None, req.clone())
        .command(&req.inputs);
    assert!(command
        .arguments
        .windows(2)
        .any(|a| a == ["-pix_fmt", "yuv420p"]));
    insta::assert_snapshot!(command.arguments.join(" "));
}
