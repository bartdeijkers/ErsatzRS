use std::time::Duration;

use ersatzrs_domain::{HardwareAccelerationKind, TeletextPageNumber};

use super::*;
use crate::{
    build_pipeline_command, build_pipeline_command_with_source_teletext_plan, ComplexFilter,
    DesiredVideoState, EnvironmentVariable, FilterChain, FrameSize, PipelineRequest, PixelFormat,
    SourceAudioCodec, SourceAudioStream, SourceVideoCodec, SourceVideoStream, SubtitleMethod,
};

fn request() -> PipelineRequest {
    let mut video = SourceVideoStream::new(SourceVideoCodec::H264).with_index(4);
    video.frame_size = Some(FrameSize::new(640, 360));
    video.pixel_format = Some(PixelFormat::Yuv420P);
    let mut request = PipelineRequest::new(
        InputFile::video("source.ts"),
        DesiredVideoState::h264_1080p(),
    )
    .with_source_video_stream(video)
    .with_source_audio_streams(vec![
        SourceAudioStream::new(SourceAudioCodec::Aac).with_index(0),
        SourceAudioStream::new(SourceAudioCodec::Aac).with_index(3),
    ]);
    request.inputs.push(InputFile::teletext_subtitle(
        "source.ts",
        7,
        TeletextPageNumber::new(888).unwrap(),
    ));
    request.ffmpeg_executable = Some("synthetic-ffmpeg".into());
    request
}

fn build(
    acceleration: HardwareAccelerationKind,
    request: PipelineRequest,
) -> (FfmpegCommand, SourceTeletextInputPlan) {
    let (command, plan) =
        build_pipeline_command_with_source_teletext_plan(acceleration, request).unwrap();
    (command, plan.unwrap())
}

#[test]
fn shared_plan_uses_final_normalized_streams_and_preserves_the_old_command() {
    let mut request = request();
    request.state.start = Some(Duration::from_millis(525));
    let old = build_pipeline_command(HardwareAccelerationKind::None, request.clone()).unwrap();
    let (command, plan) = build(HardwareAccelerationKind::None, request);
    assert_eq!(command, old);
    assert!(plan.matches_command(&command));
    assert_eq!(command.executable, "synthetic-ffmpeg");
    assert_eq!(plan.caption_physical_input_index(), 0);
    assert_eq!(plan.video_stream(), Some((0, 4)));
    // Default logical audio index1 was absent; final pipeline selected index0.
    assert_eq!(plan.audio_stream(), Some((0, 0)));
    assert_eq!(plan.enabled_caption_input_streams(), [4, 0, 7]);
    assert_eq!(plan.caption_input().streams[0].index, 7);
    assert_eq!(
        plan.caption_input().streams[0]
            .source_teletext_page
            .unwrap()
            .get(),
        888
    );
    let input = command
        .arguments
        .iter()
        .position(|arg| arg == "-i")
        .unwrap();
    assert_eq!(plan.physical_input_options(), &command.arguments[..input]);
    assert!(plan
        .physical_input_options()
        .windows(2)
        .any(|pair| pair == ["-ss", "00:00:00.5250000"]));
}

#[test]
fn explicit_valid_audio_index_survives_instead_of_selecting_first_metadata() {
    let mut request = request();
    request.inputs[1] = request.inputs[1].clone().with_stream_index(3);
    let (_, plan) = build(HardwareAccelerationKind::None, request);
    assert_eq!(plan.audio_stream(), Some((0, 3)));
    assert_eq!(plan.enabled_caption_input_streams(), [4, 3, 7]);
}

fn copy_burn_request() -> PipelineRequest {
    let frame_size = FrameSize::new(492, 250);
    let mut desired = DesiredVideoState::h264_1080p();
    desired.video_format = ersatzrs_domain::FFmpegProfileVideoFormat::Copy;
    desired.scaled_size = frame_size;
    desired.frame_size = frame_size;
    desired.frame_rate = Some(crate::FrameRate::new(25, 1));
    desired.normalize_colors = false;
    desired.deinterlace = false;
    let mut video = SourceVideoStream::new(SourceVideoCodec::Mpeg2Video).with_index(0);
    video.frame_size = Some(frame_size);
    video.frame_rate = Some(crate::FrameRate::new(25, 1));
    video.pixel_format = Some(PixelFormat::Yuv420P);
    video.sample_aspect_ratio = Some("1:1".to_owned());
    let mut request = PipelineRequest::new(InputFile::video("source.ts"), desired)
        .with_source_video_stream(video)
        .with_source_audio_streams(vec![
            SourceAudioStream::new(SourceAudioCodec::Aac).with_index(1),
            SourceAudioStream::new(SourceAudioCodec::Aac).with_index(3),
        ]);
    request.inputs[1] = request.inputs[1].clone().with_stream_index(3);
    request.inputs.push(InputFile::teletext_subtitle(
        "source.ts",
        2,
        TeletextPageNumber::new(888).unwrap(),
    ));
    request.state.duration = Some(Duration::from_secs(2));
    request.state.report_progress = false;
    request.state.thread_count = Some(1);

    request
}

#[test]
fn video_copy_with_selected_source_teletext_burn_requires_explicit_refusal() {
    let request = copy_burn_request();
    // ADR 0121 requires verified carriage or a source-bound sidecar for Copy.
    // Safe refusal does not fulfill that native caption delivery obligation.
    let reason = crate::unsupported_source_reason(&request).unwrap();
    assert!(reason.contains("teletext Burn"));
    assert!(reason.contains("video Copy"));
    assert!(reason.contains("carriage") && reason.contains("sidecar"));
    for acceleration in [
        HardwareAccelerationKind::None,
        HardwareAccelerationKind::Vaapi,
        HardwareAccelerationKind::Qsv,
        HardwareAccelerationKind::Nvenc,
    ] {
        let direct = build_pipeline_command(acceleration, request.clone()).unwrap_err();
        let planned =
            build_pipeline_command_with_source_teletext_plan(acceleration, request.clone())
                .unwrap_err();
        assert_eq!(direct.reason, reason);
        assert_eq!(planned, direct);
    }
}

#[test]
fn copy_burn_refusal_preserves_copy_without_captions_and_encoded_caption_selection() {
    let mut copy = copy_burn_request();
    copy.inputs.pop();
    assert!(crate::unsupported_source_reason(&copy).is_none());
    let (command, plan) =
        build_pipeline_command_with_source_teletext_plan(HardwareAccelerationKind::None, copy)
            .unwrap();
    assert!(plan.is_none());
    assert!(command
        .arguments
        .windows(2)
        .any(|pair| pair == ["-codec:v", "copy"]));

    let mut encoded = copy_burn_request();
    encoded.desired.video_format = ersatzrs_domain::FFmpegProfileVideoFormat::H264;
    assert!(crate::unsupported_source_reason(&encoded).is_none());
    let (command, plan) = build(HardwareAccelerationKind::None, encoded);
    assert!(command
        .arguments
        .windows(2)
        .any(|pair| pair == ["-txt_page:2", "888"]));
    assert_eq!(plan.video_stream(), Some((0, 0)));
    assert_eq!(plan.audio_stream(), Some((0, 3)));
    assert_eq!(plan.enabled_caption_input_streams(), [0, 3, 2]);
}

#[test]
fn explicit_source_teletext_copy_mapping_does_not_gain_burn_consumer_authority() {
    let mut request = copy_burn_request();
    request.inputs[2].subtitle_method = Some(SubtitleMethod::Copy);
    assert!(crate::unsupported_source_reason(&request).is_none());
    let (command, plan) =
        build_pipeline_command_with_source_teletext_plan(HardwareAccelerationKind::None, request)
            .unwrap();
    for pair in [["-codec:v", "copy"], ["-codec:s", "copy"], ["-map", "0:2"]] {
        assert!(command.arguments.windows(2).any(|actual| actual == pair));
    }
    assert!(!command.arguments.iter().any(|arg| arg == "-txt_page:2"));
    // Generic raw stream mapping is possible, but does not certify selected
    // page delivery or the application owner's exact-source carriage contract.
    assert!(plan.is_none());
}

#[test]
fn intel_reopened_audio_is_not_enabled_on_the_caption_physical_input() {
    for acceleration in [
        HardwareAccelerationKind::Vaapi,
        HardwareAccelerationKind::Qsv,
    ] {
        let request = request();
        let old = build_pipeline_command(acceleration, request.clone()).unwrap();
        let (command, plan) = build(acceleration, request);
        assert_eq!(command, old);
        assert_eq!(
            command.arguments.iter().filter(|arg| *arg == "-i").count(),
            2
        );
        assert_eq!(plan.video_stream(), Some((0, 4)));
        assert_eq!(plan.audio_stream(), Some((1, 0)));
        assert_eq!(plan.caption_physical_input_index(), 0);
        assert_eq!(plan.enabled_caption_input_streams(), [4, 7]);
        assert_eq!(
            command
                .arguments
                .iter()
                .filter(|arg| *arg == "-txt_page:7")
                .count(),
            1
        );
    }
}

#[test]
fn first_bitmap_burn_without_source_page_cannot_enable_a_later_source_plan() {
    for subtitle in [
        InputFile::text_subtitle("captions.srt", SubtitleMethod::Burn),
        InputFile::image_subtitle("captions.sup", SubtitleMethod::Burn),
    ] {
        let mut request = request();
        request.inputs.pop();
        request.inputs.push(subtitle);
        let (command, plan) = build_pipeline_command_with_source_teletext_plan(
            HardwareAccelerationKind::None,
            request.clone(),
        )
        .unwrap();
        assert!(plan.is_none());
        assert_eq!(
            command,
            build_pipeline_command(HardwareAccelerationKind::None, request).unwrap()
        );
    }
    let mut request = request();
    request.inputs.insert(
        2,
        InputFile::image_subtitle("external.sup", SubtitleMethod::Burn),
    );
    assert!(build_pipeline_command_with_source_teletext_plan(
        HardwareAccelerationKind::None,
        request
    )
    .unwrap()
    .1
    .is_none());
}

#[test]
fn independent_caption_instance_retains_only_its_own_physical_options_and_stream() {
    let mut request = request();
    request.inputs[2].input_instance = 1;
    request.inputs[2]
        .input_options
        .extend(["-custom-caption-option".into(), "unqualified".into()]);
    let (command, plan) = build(HardwareAccelerationKind::None, request);
    assert_eq!(plan.caption_physical_input_index(), 1);
    assert_eq!(plan.caption_input().input_instance, 1);
    assert_eq!(plan.enabled_caption_input_streams(), [7]);
    assert_eq!(plan.video_stream(), Some((0, 4)));
    assert_eq!(plan.audio_stream(), Some((0, 0)));
    assert!(plan
        .physical_input_options()
        .windows(2)
        .any(|pair| pair == ["-custom-caption-option", "unqualified"]));
    assert!(!plan
        .physical_input_options()
        .iter()
        .any(|option| option == "-fflags"));
    let input_positions = command
        .arguments
        .iter()
        .enumerate()
        .filter_map(|(index, arg)| (arg == "-i").then_some(index))
        .collect::<Vec<_>>();
    assert_eq!(
        plan.physical_input_options(),
        &command.arguments[input_positions[0] + 2..input_positions[1]]
    );
}

#[test]
fn duplicate_sources_share_the_same_generator_and_filter_ownership() {
    let video = InputFile::video("source.ts").with_stream_index(4);
    let audio = InputFile::audio("source.ts").with_stream_index(3);
    let caption =
        InputFile::teletext_subtitle("source.ts", 7, TeletextPageNumber::new(888).unwrap());
    let inputs = [video.clone(), video, audio, caption];
    let complex = ComplexFilter::build(&inputs, FilterChain::default(), true);
    assert_eq!(complex.video_label, "0:4");
    assert_eq!(complex.audio_label, "2:3");
    let command = crate::CommandGenerator::generate_command(&inputs, &[], true);
    let plan = SourceTeletextInputPlan::from_generated(
        &command,
        &inputs,
        &inputs,
        &[],
        true,
        SourceTeletextInputTiming::from_state(&FfmpegState::default()),
    )
    .unwrap();
    assert_eq!(
        command.arguments.iter().filter(|arg| *arg == "-i").count(),
        3
    );
    assert_eq!(plan.caption_physical_input_index(), 0);
    assert_eq!(plan.video_stream(), Some((0, 4)));
    assert_eq!(plan.audio_stream(), Some((2, 3)));
    assert_eq!(plan.enabled_caption_input_streams(), [4, 7]);
    assert_eq!(
        command
            .arguments
            .iter()
            .filter(|arg| *arg == "-txt_page:7")
            .count(),
        1
    );
}

#[test]
fn full_command_seal_rejects_argument_executable_and_environment_mutations() {
    let (command, plan) = build(HardwareAccelerationKind::None, request());
    let mut arguments = command.clone();
    arguments.arguments.push("-changed".into());
    let mut executable = command.clone();
    executable.executable = "different-ffmpeg".into();
    let mut environment = command.clone();
    environment
        .environment
        .push(EnvironmentVariable::new("SYNTHETIC_INPUT_TEST", "changed"));
    for changed in [arguments, executable, environment] {
        assert!(!plan.matches_command(&changed));
    }
    assert!(plan.matches_command(&command.clone()));
    assert!(!format!("{plan:?}").contains("source.ts"));
}

#[test]
fn hardware_fallback_requires_its_fresh_final_plan() {
    let request = request();
    let (hardware, hardware_plan) = build(HardwareAccelerationKind::Qsv, request.clone());
    let (software, software_plan) = build(HardwareAccelerationKind::None, request);
    assert!(hardware_plan.matches_command(&hardware));
    assert!(software_plan.matches_command(&software));
    assert!(!hardware_plan.matches_command(&software));
    assert!(!software_plan.matches_command(&hardware));
    assert_eq!(hardware_plan.audio_stream(), Some((1, 0)));
    assert_eq!(software_plan.audio_stream(), Some((0, 0)));
}

#[test]
fn disabled_audio_and_same_index_aliases_do_not_invent_enabled_streams() {
    let (_, plan) = build(HardwareAccelerationKind::None, request().without_audio());
    assert_eq!(plan.audio_stream(), None);
    assert_eq!(plan.enabled_caption_input_streams(), [4, 7]);
    let mut request = request();
    request.inputs[2] = request.inputs[2].clone().with_stream_index(4);
    let (_, alias) = build(HardwareAccelerationKind::None, request);
    assert_eq!(alias.enabled_caption_input_streams(), [4, 0]);
}

#[test]
fn timing_preserves_separate_generated_seeks_duration_and_transport_offset() {
    let mut request = request();
    let seek = Duration::new(90_061, 123_456_789);
    let output_seek = Duration::new(3, 987_654_321);
    let duration = Duration::new(40, 999_999_999);
    request.state.start = Some(seek);
    request.state.output_start = Some(output_seek);
    request.state.duration = Some(duration);
    request.state.output_ts_offset = Some(Duration::from_millis(456));
    let old = build_pipeline_command(HardwareAccelerationKind::None, request.clone()).unwrap();
    let (command, plan) = build(HardwareAccelerationKind::None, request);
    assert_eq!(command, old);
    let timing = plan.timing();
    assert_eq!(timing.input_seek(), Some(seek));
    assert_eq!(
        timing.effective_input_seek_microseconds(),
        Some(90_061_123_456)
    );
    assert_eq!(timing.output_seek(), Some(output_seek));
    assert_eq!(timing.duration(), Some(duration));
    assert_eq!(timing.output_ts_offset(), Some(Duration::from_millis(456)));
    assert!(!timing.retained_stream_timestamps());
    assert!(!timing.has_opaque_input_options());
}

#[test]
fn later_caption_input_does_not_inherit_first_input_seek_or_video_options() {
    let mut request = request();
    request.state.start = Some(Duration::from_secs(40));
    request.state.output_start = Some(Duration::from_secs(2));
    request.inputs[0]
        .input_options
        .extend(["-itsoffset".into(), "5".into()]);
    request.inputs[2].input_instance = 1;
    let (command, plan) = build(HardwareAccelerationKind::None, request.clone());
    assert_eq!(
        command,
        build_pipeline_command(HardwareAccelerationKind::None, request).unwrap()
    );
    assert_eq!(plan.caption_physical_input_index(), 1);
    let timing = plan.timing();
    assert_eq!(timing.input_seek(), None);
    assert_eq!(timing.effective_input_seek_microseconds(), Some(0));
    assert_eq!(timing.output_seek(), Some(Duration::from_secs(2)));
    assert!(!timing.has_opaque_input_options());
}

#[test]
fn only_original_options_emitted_for_the_actual_caption_owner_make_seek_opaque() {
    let mut request = request();
    request.inputs[2]
        .input_options
        .extend(["-ss".into(), "15".into()]);
    // Deduplicated logical caption options never reach the shared physical input.
    let (_, shared) = build(HardwareAccelerationKind::None, request.clone());
    assert!(!shared.timing().has_opaque_input_options());
    assert_eq!(shared.timing().effective_input_seek_microseconds(), Some(0));
    request.inputs[2].input_instance = 1;
    let (_, independent) = build(HardwareAccelerationKind::None, request.clone());
    assert!(independent.timing().has_opaque_input_options());
    assert_eq!(independent.timing().input_seek(), None);
    assert_eq!(
        independent.timing().effective_input_seek_microseconds(),
        None
    );
    request.inputs[2].input_instance = 0;
    request.inputs[0]
        .input_options
        .extend(["-ss".into(), "25".into()]);
    let (_, owner) = build(HardwareAccelerationKind::None, request);
    assert!(owner.timing().has_opaque_input_options());
    assert_eq!(owner.timing().effective_input_seek_microseconds(), None);
}

#[test]
fn intel_reopen_audio_options_do_not_override_caption_owner_seek() {
    let mut request = request();
    request.state.start = Some(Duration::from_millis(500));
    request.inputs[1]
        .input_options
        .extend(["-ss".into(), "7".into()]);
    let (_, shared) = build(HardwareAccelerationKind::None, request.clone());
    assert!(shared.timing().has_opaque_input_options());
    assert_eq!(shared.timing().effective_input_seek_microseconds(), None);
    for acceleration in [
        HardwareAccelerationKind::Vaapi,
        HardwareAccelerationKind::Qsv,
    ] {
        let (_, reopened) = build(acceleration, request.clone());
        assert!(!reopened.timing().has_opaque_input_options());
        assert_eq!(
            reopened.timing().input_seek(),
            Some(Duration::from_millis(500))
        );
        assert_eq!(
            reopened.timing().effective_input_seek_microseconds(),
            Some(500_000)
        );
    }
}

#[test]
fn retained_timestamps_and_unparseable_seeks_never_disappear_from_timing() {
    let mut request = request();
    request.state.retained_stream_timestamps = true;
    request.state.start = Some(Duration::from_hours(10_000));
    let (_, plan) = build(HardwareAccelerationKind::None, request);
    assert!(plan.timing().retained_stream_timestamps());
    assert_eq!(
        plan.timing().input_seek(),
        Some(Duration::from_hours(10_000))
    );
    assert_eq!(plan.timing().effective_input_seek_microseconds(), None);
}

#[test]
fn song_background_conversion_keeps_moved_per_input_seek_unknown() {
    let mut request = request();
    request.state.start = Some(Duration::from_secs(40));
    let request = request.with_song_background(
        InputFile::video("song.png"),
        SourceVideoStream::new(SourceVideoCodec::Unknown),
        false,
    );
    assert_eq!(request.state.start, None);
    assert_eq!(request.state.output_start, None);
    let old = build_pipeline_command(HardwareAccelerationKind::None, request.clone()).unwrap();
    let (command, plan) = build(HardwareAccelerationKind::None, request);
    assert_eq!(command, old);
    assert_eq!(plan.video_stream(), Some((0, 0)));
    assert_eq!(plan.audio_stream(), Some((1, 0)));
    assert_eq!(plan.caption_physical_input_index(), 1);
    assert_eq!(plan.timing().input_seek(), None);
    assert!(plan.timing().has_opaque_input_options());
    assert_eq!(plan.timing().effective_input_seek_microseconds(), None);
}

#[test]
fn suppressed_fmp4_transport_offset_is_not_reported_as_emitted() {
    let mut request = request();
    request.state.output_ts_offset = Some(Duration::from_secs(5));
    request.state.output_format = crate::OutputFormat::HlsMp4;
    request.state.hls = Some(crate::HlsOutput {
        segment_template: "segment-%d.m4s".into(),
        init_template: Some("init.mp4".into()),
        playlist_path: "stream.m3u8".into(),
        segment_options: None,
        is_first_transcode: true,
        discontinuity: false,
        troubleshooting: false,
    });
    let (command, plan) = build(HardwareAccelerationKind::None, request.clone());
    assert_eq!(
        command,
        build_pipeline_command(HardwareAccelerationKind::None, request).unwrap()
    );
    assert!(!command
        .arguments
        .iter()
        .any(|arg| arg == "-output_ts_offset"));
    assert_eq!(plan.timing().output_ts_offset(), None);
}

#[test]
fn typed_realtime_burst_keeps_caption_seek_and_both_audio_selections_qualified() {
    for audio in [1, 5] {
        let mut request = request();
        request.source_audio_streams = vec![
            SourceAudioStream::new(SourceAudioCodec::Aac).with_index(1),
            SourceAudioStream::new(SourceAudioCodec::Aac).with_index(5),
        ];
        request.inputs[1] = request.inputs[1].clone().with_stream_index(audio);
        request.desired.realtime_mode = crate::RealtimeMode::On;
        request.state.start = Some(Duration::from_millis(17_500));
        request.state.realtime_initial_burst = Some(Duration::from_secs(12));
        let (command, plan) = build(HardwareAccelerationKind::None, request);
        assert!(plan.matches_command(&command));
        assert_eq!(plan.audio_stream(), Some((0, audio)));
        assert_eq!(plan.enabled_caption_input_streams(), [4, audio, 7]);
        assert_eq!(
            plan.caption_input().streams[0]
                .source_teletext_page
                .unwrap()
                .get(),
            888
        );
        assert!(!plan.timing().has_opaque_input_options());
        assert_eq!(
            plan.timing().effective_input_seek_microseconds(),
            Some(17_500_000)
        );
        let generated = plan
            .physical_input_options()
            .windows(2)
            .filter(|pair| {
                matches!(
                    pair[0].as_str(),
                    "-ss" | "-readrate_initial_burst" | "-readrate"
                )
            })
            .map(|pair| format!("{} {}", pair[0], pair[1]))
            .collect::<Vec<_>>();
        insta::allow_duplicates! {
            insta::assert_snapshot!(generated.join("\n"), @"
            -ss 00:00:17.5000000
            -readrate_initial_burst 12
            -readrate_initial_burst 12
            -readrate 1.05
            ");
        }
    }
}

#[test]
fn raw_burst_and_seek_remain_opaque_even_with_generated_realtime_reserve() {
    for options in [["-readrate_initial_burst", "12"], ["-ss", "99"]] {
        let mut request = request();
        request.inputs[0]
            .input_options
            .extend(options.map(str::to_owned));
        request.desired.realtime_mode = crate::RealtimeMode::On;
        request.state.start = Some(Duration::from_millis(17_500));
        request.state.realtime_initial_burst = Some(Duration::from_secs(12));
        let (command, plan) = build(HardwareAccelerationKind::None, request);
        assert!(plan.matches_command(&command));
        assert!(plan.timing().has_opaque_input_options());
        assert_eq!(plan.timing().effective_input_seek_microseconds(), None);
        assert!(plan
            .physical_input_options()
            .windows(2)
            .any(|pair| pair == options));
    }
}
