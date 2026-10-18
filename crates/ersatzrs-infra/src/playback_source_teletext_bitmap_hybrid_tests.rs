//! Typed Hybrid preparation stays bound to the private actual-child directory.
use super::*;
use ersatzrs_domain::HardwareAccelerationKind;
use ersatzrs_ffmpeg::{HlsMpegTsSegments, HlsOutput, OutputFormat, SubtitleMethod};

#[test]
fn bitmap_hybrid_requires_owned_directory_and_preserves_exact_segment_plan_on_rebuild() {
    let directory = Arc::new(tempfile::tempdir().unwrap());
    let mut input = InputFile::image_subtitle("private-artifact.mks", SubtitleMethod::Copy);
    input.allow_readrate = false;
    let mut pipeline = super::super::tests::fixture(Some(3), Some(800));
    pipeline.inputs.pop();
    pipeline.inputs.push(input.clone());
    pipeline.state.output_format = OutputFormat::Hls;
    pipeline.state.hls = Some(HlsOutput {
        mpegts_segments: Some(HlsMpegTsSegments {
            producer_playlist: format!(
                "{}/producer.m3u8",
                directory.path().file_name().unwrap().to_str().unwrap()
            ),
            start_number: 17,
        }),
        segment_template: "live%06d.ts".into(),
        init_template: None,
        playlist_path: "live.m3u8".into(),
        segment_options: Some("mpegts_service_id=37:mpegts_start_pid=512:mpegts_copyts=1".into()),
        is_first_transcode: false,
        discontinuity: true,
        troubleshooting: false,
    });
    let request = SourceTeletextConsumerRequest::new_bitmap_carriage(
        HardwareAccelerationKind::None,
        pipeline,
        input.clone(),
    );
    let (_, plan) = request.build().unwrap();
    let plan = plan.unwrap();
    assert!(!request.bitmap_output_matches(&plan));
    let wrong = request
        .clone()
        .with_hls_producer_directory(Arc::new(tempfile::tempdir().unwrap()));
    assert!(!wrong.bitmap_output_matches(&plan));
    let request = request.with_hls_producer_directory(Arc::clone(&directory));
    assert!(request.bitmap_output_matches(&plan));
    let mut playlist = InputFile::image_subtitle("private.ffconcat", SubtitleMethod::Copy);
    playlist.allow_readrate = false;
    playlist.input_options = vec![
        "-f".into(),
        "concat".into(),
        "-safe".into(),
        "0".into(),
        "-protocol_whitelist".into(),
        "file".into(),
    ];
    let command = request
        .initialized_bitmap_command(&plan, &playlist)
        .unwrap();
    assert!(command.arguments.windows(2).any(|pair| pair
        == [
            "-segment_list",
            &plan.mpegts_segments().unwrap().producer_playlist
        ]));
    assert!(command
        .arguments
        .windows(2)
        .any(|pair| pair == ["-codec:s", "copy"]));
    assert!(!command.arguments.contains(&input.path));
    assert!(!format!("{request:?}").contains(directory.path().to_str().unwrap()));
}
