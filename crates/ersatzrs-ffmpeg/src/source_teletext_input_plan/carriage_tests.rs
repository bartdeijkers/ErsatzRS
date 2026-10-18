//! ADR 0121: exact generated Copy mapping is not source/carriage authorization.

use super::{copy_burn_request, request};
use crate::{
    build_pipeline_command, build_pipeline_command_with_source_bitmap_carriage_plan,
    build_pipeline_command_with_source_teletext_plan, EnvironmentVariable, FfmpegCommand,
    InputFile, MediaStream, PipelineRequest, SourceTeletextInputPlan, SubtitleMethod,
    SubtitleTextFormat,
};
use ersatzrs_domain::{FFmpegProfileVideoFormat, HardwareAccelerationKind, TeletextPageNumber};
use std::time::Duration;

pub(super) fn fixture() -> (PipelineRequest, InputFile) {
    let mut request = copy_burn_request();
    request.inputs.pop();
    let mut caption = InputFile::image_subtitle("private-caption.ffconcat", SubtitleMethod::Copy);
    caption.input_options.extend([
        "-f".into(),
        "concat".into(),
        "-safe".into(),
        "0".into(),
        "-protocol_whitelist".into(),
        "file".into(),
    ]);
    request.inputs.push(caption.clone());
    request.state.start = Some(Duration::from_millis(17_520));
    request.ffmpeg_executable = Some("synthetic-ffmpeg".into());
    (request, caption)
}

fn build(
    request: PipelineRequest,
    expected: &InputFile,
) -> (FfmpegCommand, Option<SourceTeletextInputPlan>) {
    build_pipeline_command_with_source_bitmap_carriage_plan(
        HardwareAccelerationKind::None,
        request,
        expected,
    )
    .unwrap()
}

fn assert_only_bitmap_mux_options_changed(command: &FfmpegCommand, ordinary: &FfmpegCommand) {
    let mut arguments = command.arguments.clone();
    let positions: Vec<_> = arguments
        .windows(2)
        .enumerate()
        .filter_map(|(index, pair)| (pair == ["-max_interleave_delta", "100000"]).then_some(index))
        .collect();
    let [position] = positions.as_slice() else {
        panic!("one explicit sparse interleave bound");
    };
    arguments.drain(*position..*position + 2);
    let landings: Vec<_> = arguments
        .windows(2)
        .enumerate()
        .filter_map(|(index, pair)| (pair == ["-copypriorss:v:0", "0"]).then_some(index))
        .collect();
    let [landing] = landings.as_slice() else {
        panic!("one explicit copied-video landing boundary");
    };
    arguments.drain(*landing..*landing + 2);
    let delays: Vec<_> = arguments
        .iter()
        .enumerate()
        .filter_map(|(index, option)| (option == "-muxdelay").then_some(index))
        .collect();
    let [delay] = delays.as_slice() else {
        panic!("one replaced bitmap mux delay");
    };
    assert_eq!(arguments[*delay + 1], "0.7");
    arguments[*delay + 1] = "0".into();
    assert_eq!(arguments, ordinary.arguments);
    assert_eq!(command.executable, ordinary.executable);
    assert_eq!(command.environment, ordinary.environment);
    let actual = command.mpegts_output_policy().unwrap();
    let prior = ordinary.mpegts_output_policy().unwrap();
    assert_eq!(actual.muxrate_bps(), prior.muxrate_bps());
    assert_eq!(actual.pcr_period_ms(), prior.pcr_period_ms());
}

#[test]
fn bitmap_carriage_copy_uses_normal_mux_selected_audio_and_distinct_actual_av_seek() {
    let (request, caption) = fixture();
    let old = build_pipeline_command(HardwareAccelerationKind::None, request.clone()).unwrap();
    let (command, plan) = build(request.clone(), &caption);
    let plan = plan.unwrap();
    assert_only_bitmap_mux_options_changed(&command, &old);
    assert!(!old
        .arguments
        .iter()
        .any(|argument| argument == "-max_interleave_delta"));
    assert!(plan.matches_command(&command));
    assert!(plan.is_bitmap_carriage());
    assert_eq!(plan.caption_input(), &caption);
    assert_eq!(plan.caption_physical_input_index(), 1);
    assert_eq!(plan.video_stream(), Some((0, 0)));
    assert_eq!(plan.audio_stream(), Some((0, 3)));
    assert_eq!(plan.enabled_caption_input_streams(), [0]);
    assert_eq!(
        plan.first_input_timing()
            .effective_input_seek_microseconds(),
        Some(17_520_000)
    );
    assert_eq!(plan.timing().input_seek(), None);
    assert_eq!(plan.timing().effective_input_seek_microseconds(), None);
    assert!(!plan.first_input_timing().has_opaque_input_options());
    assert!(plan.timing().has_opaque_input_options());
    assert_eq!(
        plan.first_input_timing().duration(),
        Some(Duration::from_secs(2))
    );
    for pair in [["-codec:v", "copy"], ["-codec:s", "copy"], ["-map", "1:0"]] {
        assert!(command.arguments.windows(2).any(|actual| actual == pair));
    }
    assert!(
        command.arguments.iter().any(|arg| arg.contains("[0:3]"))
            || command
                .arguments
                .windows(2)
                .any(|pair| pair == ["-map", "0:3"])
    );
    assert!(!command.arguments.iter().any(|arg| arg.starts_with("-txt_")));
    let inputs: Vec<_> = command
        .arguments
        .iter()
        .enumerate()
        .filter_map(|(index, arg)| (arg == "-i").then_some(index))
        .collect();
    assert_eq!(inputs.len(), 2);
    assert_eq!(plan.first_input_options(), &command.arguments[..inputs[0]]);
    assert_eq!(
        plan.physical_input_options(),
        &command.arguments[inputs[0] + 2..inputs[1]]
    );
    assert!(build_pipeline_command_with_source_teletext_plan(
        HardwareAccelerationKind::None,
        request
    )
    .unwrap()
    .1
    .is_none());
    insta::assert_snapshot!(
        "bitmap_carriage_copy_command",
        format!("{}\n{}", command.executable, command.arguments.join("\n"))
    );
}

#[test]
fn bitmap_carriage_requires_the_exact_expected_input_and_the_owned_bitmap_shape() {
    let (request, caption) = fixture();
    let mut wrong_path = caption.clone();
    wrong_path.path = "other-private-caption.ffconcat".into();
    let mut wrong_options = caption.clone();
    wrong_options
        .input_options
        .extend(["-ss".into(), "9".into()]);
    let mut wrong_instance = caption.clone();
    wrong_instance.input_instance = 1;
    for expected in [wrong_path, wrong_options, wrong_instance] {
        assert!(build(request.clone(), &expected).1.is_none());
    }
    let mut shapes = Vec::new();
    let mut wrong = caption.clone();
    wrong.streams[0].index = 1;
    shapes.push(wrong);
    let mut wrong = caption.clone();
    wrong.streams.push(MediaStream::new(1));
    shapes.push(wrong);
    let mut wrong = caption.clone();
    wrong.streams[0].source_teletext_page = Some(TeletextPageNumber::new(888).unwrap());
    shapes.push(wrong);
    let mut wrong = caption.clone();
    wrong.subtitle_text_format = Some(SubtitleTextFormat::Srt);
    shapes.push(wrong);
    let mut wrong = caption.clone();
    wrong.filter_steps.push("null".into());
    shapes.push(wrong);
    let mut wrong = caption;
    wrong.is_image_based_subtitle = false;
    shapes.push(wrong);
    for expected in shapes {
        let mut changed = request.clone();
        changed.inputs[2] = expected.clone();
        assert!(build(changed, &expected).1.is_none());
    }
}

#[test]
fn bitmap_carriage_refuses_duplicate_maps_and_observes_the_actual_global_subtitle_encoder() {
    let (request, caption) = fixture();
    let mut duplicate = request.clone();
    duplicate.inputs.push(caption.clone());
    assert!(build(duplicate, &caption).1.is_none());
    let mut alias = caption.clone();
    alias.input_options.clear();
    let mut duplicate = request.clone();
    duplicate.inputs.push(alias);
    assert!(build(duplicate, &caption).1.is_none());

    let convert = InputFile::image_subtitle("other-private-caption.sup", SubtitleMethod::Convert);
    let mut earlier_convert = request.clone();
    earlier_convert.inputs.insert(2, convert.clone());
    let old =
        build_pipeline_command(HardwareAccelerationKind::None, earlier_convert.clone()).unwrap();
    let (command, plan) = build(earlier_convert, &caption);
    assert_eq!(command, old);
    assert!(plan.is_none());
    assert!(command
        .arguments
        .windows(2)
        .any(|pair| pair == ["-codec:s", "dvdsub"]));
    let mut later_convert = request;
    later_convert.inputs.push(convert);
    let (command, plan) = build(later_convert, &caption);
    assert!(plan.unwrap().is_bitmap_carriage());
    assert!(command
        .arguments
        .windows(2)
        .any(|pair| pair == ["-codec:s", "copy"]));
}

#[test]
fn bitmap_carriage_keeps_first_owner_opaque_options_separate_from_caption_demux_options() {
    let (mut request, caption) = fixture();
    request.inputs[0]
        .input_options
        .extend(["-ss".into(), "99".into()]);
    request.state.output_start = Some(Duration::from_secs(3));
    request.state.output_ts_offset = Some(Duration::from_secs(10));
    request.state.retained_stream_timestamps = true;
    let (_, plan) = build(request, &caption);
    let plan = plan.unwrap();
    assert_eq!(
        plan.first_input_timing().input_seek(),
        Some(Duration::from_millis(17_520))
    );
    assert!(plan.first_input_timing().has_opaque_input_options());
    assert_eq!(
        plan.first_input_timing()
            .effective_input_seek_microseconds(),
        None
    );
    assert!(plan
        .first_input_options()
        .windows(2)
        .any(|pair| pair == ["-ss", "99"]));
    assert!(!plan
        .physical_input_options()
        .windows(2)
        .any(|pair| pair == ["-ss", "99"]));
    for timing in [plan.timing(), plan.first_input_timing()] {
        assert_eq!(timing.output_seek(), Some(Duration::from_secs(3)));
        assert!(timing.retained_stream_timestamps());
    }
}

#[test]
fn bitmap_carriage_does_not_change_burn_plans_or_the_copy_burn_refusal() {
    let request = request();
    let (command, plan) = build_pipeline_command_with_source_teletext_plan(
        HardwareAccelerationKind::None,
        request.clone(),
    )
    .unwrap();
    let plan = plan.unwrap();
    assert_eq!(
        command,
        build_pipeline_command(HardwareAccelerationKind::None, request).unwrap()
    );
    assert!(!plan.is_bitmap_carriage());
    assert!(!command
        .arguments
        .iter()
        .any(|argument| argument == "-max_interleave_delta"));
    assert_eq!(plan.first_input_timing(), plan.timing());
    assert_eq!(plan.first_input_options(), plan.physical_input_options());

    let request = copy_burn_request();
    let ordinary =
        build_pipeline_command(HardwareAccelerationKind::None, request.clone()).unwrap_err();
    let (_, expected) = fixture();
    let explicit = build_pipeline_command_with_source_bitmap_carriage_plan(
        HardwareAccelerationKind::None,
        request,
        &expected,
    )
    .unwrap_err();
    assert_eq!(explicit, ordinary);
}

#[test]
fn bitmap_carriage_seal_covers_the_complete_command_environment_and_generated_hints() {
    let (request, caption) = fixture();
    let (command, plan) = build(request, &caption);
    let plan = plan.unwrap();
    let mut arguments = command.clone();
    arguments.arguments.push("-changed".into());
    let mut executable = command.clone();
    executable.executable = "other-ffmpeg".into();
    let mut environment = command.clone();
    environment.environment.push(EnvironmentVariable::new(
        "SYNTHETIC_CARRIAGE_TEST",
        "changed",
    ));
    assert!(command.mpegts_output.is_some());
    let mut hints = command.clone();
    hints.mpegts_output = None;
    let mut copy_prior = command.clone();
    let landing = copy_prior
        .arguments
        .iter()
        .position(|option| option == "-copypriorss:v:0")
        .unwrap();
    copy_prior.arguments[landing + 1] = "1".into();
    assert!(copy_prior.mpegts_output_policy().is_none());
    let mut mux_delay = command.clone();
    let delay = mux_delay
        .arguments
        .iter()
        .position(|option| option == "-muxdelay")
        .unwrap();
    mux_delay.arguments[delay + 1] = "0".into();
    assert!(mux_delay.mpegts_output_policy().is_none());
    for changed in [
        arguments,
        executable,
        environment,
        hints,
        mux_delay,
        copy_prior,
    ] {
        assert!(!plan.matches_command(&changed));
    }
    assert!(plan.matches_command(&command));
    assert!(!format!("{plan:?}").contains("private-caption"));
}

#[test]
fn bitmap_carriage_transcoded_video_keeps_ordinary_landing_policy() {
    let (mut request, caption) = fixture();
    request.desired.video_format = FFmpegProfileVideoFormat::H264;
    let (command, plan) = build(request, &caption);
    assert!(plan.unwrap().is_bitmap_carriage());
    assert!(!command
        .arguments
        .iter()
        .any(|option| option.starts_with("-copypriorss")));
}
