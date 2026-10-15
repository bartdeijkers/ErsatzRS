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
    let plan = SourceTeletextInputPlan::from_generated(&command, &inputs, &[], true).unwrap();
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
