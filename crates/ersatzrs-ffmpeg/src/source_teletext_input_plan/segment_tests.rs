//! A private segment list is never enabled by a generic or Burn builder.

use super::carriage::fixture;
use crate::{
    build_pipeline_command, build_pipeline_command_with_source_bitmap_carriage_plan,
    build_pipeline_command_with_source_teletext_plan, build_pipeline_for_acceleration,
    HlsMpegTsSegments, HlsOutput, InputFile, OutputFormat, PipelineRequest,
};
use ersatzrs_domain::HardwareAccelerationKind;

fn segmented() -> (PipelineRequest, InputFile) {
    let (mut request, caption) = fixture();
    request.state.output_format = OutputFormat::Hls;
    request.state.hls = Some(HlsOutput {
        mpegts_segments: Some(HlsMpegTsSegments {
            producer_playlist: ".private-producer/producer.m3u8".into(),
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
    (request, caption)
}

#[test]
fn bitmap_carriage_mpegts_segments_are_exact_typed_and_preserve_av_and_mux_options() {
    let (request, caption) = segmented();
    let expected = request.state.hls.as_ref().unwrap().mpegts_segments.clone();
    let (command, plan) = build_pipeline_command_with_source_bitmap_carriage_plan(
        HardwareAccelerationKind::None,
        request,
        &caption,
    )
    .unwrap();
    let plan = plan.unwrap();
    assert_eq!(plan.mpegts_segments(), expected.as_ref());
    assert!(plan.matches_command(&command));
    assert_eq!(plan.video_stream(), Some((0, 0)));
    assert_eq!(plan.audio_stream(), Some((0, 3)));
    assert_eq!(
        plan.first_input_timing()
            .effective_input_seek_microseconds(),
        Some(17_520_000)
    );
    for pair in [
        ["-f", "segment"],
        ["-segment_format", "mpegts"],
        ["-codec:s", "copy"],
        ["-max_interleave_delta", "100000"],
        ["-muxdelay", "0.7"],
        ["-copypriorss:v:0", "0"],
        ["-segment_start_number", "17"],
    ] {
        assert!(command.arguments.windows(2).any(|actual| actual == pair));
    }
    assert_eq!(
        command
            .arguments
            .iter()
            .filter(|arg| *arg == "-muxdelay")
            .count(),
        1
    );
    let mut changed_delay = command.clone();
    let delay = changed_delay
        .arguments
        .iter()
        .position(|arg| arg == "-muxdelay")
        .unwrap();
    changed_delay.arguments[delay + 1] = "0".into();
    assert!(!plan.matches_command(&changed_delay));
    assert!(!command
        .arguments
        .iter()
        .any(|arg| arg == "-hls_flags" || arg == "live.m3u8"));
    let options = command
        .arguments
        .windows(2)
        .find(|pair| pair[0] == "-segment_format_options")
        .unwrap();
    assert!(options[1].starts_with("mpegts_service_id=37:mpegts_start_pid=512:mpegts_copyts=1:"));
    assert!(options[1].contains("muxrate="));
    assert!(options[1].contains("pcr_period=20"));
    insta::assert_snapshot!(
        "bitmap_carriage_private_mpegts_segments",
        format!("{}\n{}", command.executable, command.arguments.join("\n"))
    );
}

#[test]
fn bitmap_carriage_mpegts_segments_cannot_activate_via_generic_burn_or_raw_builders() {
    let (request, _) = segmented();
    assert!(build_pipeline_command(HardwareAccelerationKind::None, request.clone()).is_err());
    assert!(build_pipeline_command_with_source_teletext_plan(
        HardwareAccelerationKind::None,
        request.clone()
    )
    .is_err());
    // The low-level legacy builder has no refusal result; its ordinary HLS output
    // remains unchanged and cannot turn this option into a private segment mux.
    let pipeline = build_pipeline_for_acceleration(HardwareAccelerationKind::None, request.clone());
    let command = pipeline.command(&request.inputs);
    assert!(command
        .arguments
        .windows(2)
        .any(|pair| pair == ["-f", "hls"]));
    assert!(!command.arguments.iter().any(|arg| arg == "-segment_list"));
}

#[test]
fn bitmap_carriage_mpegts_segments_reject_non_hls_invalid_bitmap_and_invalid_options() {
    let (request, caption) = segmented();
    for format in [
        OutputFormat::HlsMp4,
        OutputFormat::MpegTs,
        OutputFormat::Mkv,
    ] {
        let mut changed = request.clone();
        changed.state.output_format = format;
        assert!(build_pipeline_command_with_source_bitmap_carriage_plan(
            HardwareAccelerationKind::None,
            changed,
            &caption
        )
        .is_err());
    }
    let mut wrong_caption = caption.clone();
    wrong_caption.path = "different-private-caption.ffconcat".into();
    assert!(build_pipeline_command_with_source_bitmap_carriage_plan(
        HardwareAccelerationKind::None,
        request.clone(),
        &wrong_caption
    )
    .is_err());
    for path in ["", "private\nproducer.m3u8"] {
        let mut changed = request.clone();
        changed
            .state
            .hls
            .as_mut()
            .unwrap()
            .mpegts_segments
            .as_mut()
            .unwrap()
            .producer_playlist = path.into();
        assert!(build_pipeline_command_with_source_bitmap_carriage_plan(
            HardwareAccelerationKind::None,
            changed,
            &caption
        )
        .is_err());
    }
    let mut changed = request;
    changed
        .state
        .hls
        .as_mut()
        .unwrap()
        .mpegts_segments
        .as_mut()
        .unwrap()
        .start_number = i32::MAX as u64 + 1;
    assert!(build_pipeline_command_with_source_bitmap_carriage_plan(
        HardwareAccelerationKind::None,
        changed,
        &caption
    )
    .is_err());
}
