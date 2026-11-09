use super::tests::source_stream_test_pool;
use super::*;
use ersatzrs_ffmpeg::{
    capability_report_for_tests, NvencPipelineBuilder, PipelineBuilder, SoftwarePipelineBuilder,
};

async fn source(format: &str) -> SourceVideoStream {
    let pool = source_stream_test_pool().await;
    sqlx::query(r#"INSERT INTO "MediaVersion" ("Id","VideoScanKind","Width","Height","RFrameRate") VALUES (10,1,320,180,'25/1')"#).execute(&pool).await.unwrap();
    sqlx::query(r#"INSERT INTO "MediaStream" ("Id","Index","Codec","MediaVersionId","MediaStreamKind","Channels","Default","PixelFormat") VALUES (1,3,?,10,1,0,0,?)"#).bind(if format.contains("12") { "hevc" } else { "h264" }).bind(format).execute(&pool).await.unwrap();
    let source = load_source_video_stream(&pool, 10).await.unwrap().unwrap();
    assert_eq!(source.pixel_format_name.as_deref(), Some(format));
    source
}

fn request(source: SourceVideoStream) -> PipelineRequest {
    PipelineRequest::new(
        InputFile::video("/media/synthetic.mkv"),
        DesiredVideoState::h264_1080p(),
    )
    .with_source_video_stream(source)
    .with_runtime_capabilities(capability_report_for_tests())
}

#[tokio::test]
async fn native_non420_and_twelve_bit_metadata_cannot_select_hardware_decode() {
    for format in [
        "yuv422p",
        "yuv444p",
        "yuv422p10le",
        "yuv444p10le",
        "yuv420p12le",
        "yuv444p12le",
    ] {
        let request = request(source(format).await);
        let command = NvencPipelineBuilder
            .build(request.clone())
            .command(&request.inputs);
        assert!(
            !command
                .arguments
                .windows(2)
                .any(|args| args == ["-hwaccel", "cuda"]),
            "{format} incorrectly selected CUDA decode: {:?}",
            command.arguments
        );
        assert!(
            command
                .arguments
                .windows(2)
                .any(|args| args == ["-codec:v", "h264_nvenc"]),
            "software decoding retains the available encoder"
        );
    }
}

#[tokio::test]
async fn native_non420_and_twelve_bit_software_output_requires_explicit_profile_format() {
    for format in ["yuv444p", "yuv420p12le"] {
        let request = request(source(format).await);
        let command = SoftwarePipelineBuilder
            .build(request.clone())
            .command(&request.inputs);
        assert!(
            command
                .arguments
                .windows(2)
                .any(|args| args == ["-pix_fmt", "yuv420p"]),
            "{format} did not normalize the8-bit4:2:0 profile: {:?}",
            command.arguments
        );
    }
}

#[test]
fn native_fresh_probe_keeps_unmodeled_formats_and_selected_metadata() {
    for format in ["yuv444p", "yuv420p12le", "gbrp12le"] {
        let mut probe = super::tests::fresh_playback_probe();
        let video = probe.streams.iter_mut().find(|s| s.index == 3).unwrap();
        video.pixel_format = Some(format.into());
        let source = source_video_stream_from_probe(&probe).unwrap().unwrap();
        assert_eq!(source.pixel_format_name.as_deref(), Some(format));
        assert_eq!(source.index, 3);
        assert!(source.color_params.is_hdr);
        assert_eq!(
            source_audio_streams_from_probe(&probe).unwrap()[0]
                .language
                .as_deref(),
            Some("jpn")
        );
    }
}

#[path = "playback_pixel_format_runtime_tests.rs"]
mod runtime;
