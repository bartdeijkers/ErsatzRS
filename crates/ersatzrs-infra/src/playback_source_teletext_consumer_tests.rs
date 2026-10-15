use std::time::Duration;

use ersatzrs_domain::TeletextPageNumber;
use ersatzrs_ffmpeg::{
    DesiredVideoState, FrameSize, InputFile, PixelFormat, SourceAudioCodec, SourceAudioStream,
    SourceVideoCodec, SourceVideoStream,
};

use super::*;

fn fixture(audio_index: Option<usize>, buffer_size: Option<u32>) -> PipelineRequest {
    let mut video = SourceVideoStream::new(SourceVideoCodec::H264).with_index(4);
    video.frame_size = Some(FrameSize::new(640, 360));
    video.pixel_format = Some(PixelFormat::Yuv420P);
    let mut desired = DesiredVideoState::h264_1080p();
    desired.video_bitrate = Some(400);
    desired.video_buffer_size = buffer_size;
    let mut request = PipelineRequest::new(InputFile::video("source.ts"), desired)
        .with_source_video_stream(video)
        .with_source_audio_streams(vec![
            SourceAudioStream::new(SourceAudioCodec::Aac).with_index(0),
            SourceAudioStream::new(SourceAudioCodec::Aac).with_index(3),
        ]);
    if let Some(index) = audio_index {
        request.inputs[1] = request.inputs[1].clone().with_stream_index(index);
    }
    request.inputs.push(InputFile::teletext_subtitle(
        "source.ts",
        7,
        TeletextPageNumber::new(888).unwrap(),
    ));
    request.state.start = Some(Duration::from_millis(500));
    request.state.duration = Some(Duration::from_secs(4));
    request.state.output_ts_offset = Some(Duration::from_millis(456));
    request.state.report_progress = false;
    request.ffmpeg_executable = Some("synthetic-ffmpeg".into());
    request
}

/// Actual contiguous argument scopes ending in each physical `-i source`.
fn input_scopes(command: &FfmpegCommand) -> Vec<&[String]> {
    let mut start = 0;
    command
        .arguments
        .iter()
        .enumerate()
        .filter_map(|(index, argument)| {
            if argument != "-i" {
                return None;
            }
            let scope = &command.arguments[start..index + 2];
            start = index + 2;
            Some(scope)
        })
        .collect()
}

fn option_values<'a>(command: &'a FfmpegCommand, key: &str) -> Vec<&'a str> {
    command
        .arguments
        .windows(2)
        .filter_map(|pair| (pair[0] == key).then_some(pair[1].as_str()))
        .collect()
}

fn assert_selected_stream(command: &FfmpegCommand, physical: usize, index: usize) {
    let label = format!("{physical}:{index}");
    assert!(
        option_values(command, "-map").contains(&label.as_str())
            || option_values(command, "-filter_complex")
                .iter()
                .any(|graph| graph.contains(&format!("[{label}]"))),
        "selected stream {label} must remain mapped or consumed by the filter graph"
    );
}

#[test]
fn independent_decoder_preserves_ordinary_input_scopes_selected_audio_and_output_policy() {
    for acceleration in [
        HardwareAccelerationKind::None,
        HardwareAccelerationKind::Qsv,
        HardwareAccelerationKind::Vaapi,
    ] {
        for selected_audio in [None, Some(0), Some(3)] {
            for buffer_size in [None, Some(800)] {
                let pipeline = fixture(selected_audio, buffer_size);
                let request = SourceTeletextConsumerRequest::new(acceleration, pipeline.clone());
                let (initial_command, initial) = request.build().unwrap();
                let initial = initial.unwrap();
                let final_command = request.independent_command(&initial, -539_056).unwrap();
                let scopes = input_scopes(&final_command);
                let caption_scope = scopes.last().unwrap();
                assert_eq!(
                    *caption_scope,
                    [
                        "-fflags",
                        "+genpts+discardcorrupt+igndts",
                        "-itsoffset",
                        "-0.539056",
                        "-txt_page:7",
                        "888",
                        "-txt_format:7",
                        "bitmap",
                        "-txt_default_region:7",
                        "-1",
                        "-i",
                        "source.ts",
                    ]
                );
                let expected_audio_owner =
                    usize::from(acceleration != HardwareAccelerationKind::None);
                assert_eq!(scopes.len(), expected_audio_owner + 2);
                assert_eq!(initial.video_stream(), Some((0, 4)));
                assert_eq!(
                    initial.audio_stream(),
                    Some((expected_audio_owner, selected_audio.unwrap_or(0)))
                );
                assert_selected_stream(&final_command, 0, 4);
                assert_selected_stream(
                    &final_command,
                    expected_audio_owner,
                    selected_audio.unwrap_or(0),
                );
                assert_selected_stream(&final_command, scopes.len() - 1, 7);

                // Compare with a separately generated ordinary Off request. All
                // AV-owner flags, pre-input seek and physical ordering must be
                // byte-for-byte unchanged, including Intel's audio reopen.
                let mut ordinary = pipeline;
                ordinary.inputs.pop();
                let (ordinary, ordinary_plan) =
                    SourceTeletextConsumerRequest::new(acceleration, ordinary)
                        .build()
                        .unwrap();
                assert!(ordinary_plan.is_none());
                assert_eq!(&scopes[..scopes.len() - 1], input_scopes(&ordinary));
                assert_eq!(
                    scopes[0]
                        .windows(2)
                        .filter(|pair| pair[0] == "-ss")
                        .collect::<Vec<_>>(),
                    [&["-ss".to_owned(), "00:00:00.5000000".to_owned()][..]]
                );
                for key in ["-t", "-output_ts_offset", "-bufsize:v", "-b:v", "-muxrate"] {
                    assert_eq!(
                        option_values(&final_command, key),
                        option_values(&ordinary, key)
                    );
                    assert_eq!(
                        option_values(&final_command, key),
                        option_values(&initial_command, key)
                    );
                }
                assert_eq!(final_command.executable, ordinary.executable);
                assert_eq!(final_command.environment, ordinary.environment);
                assert_eq!(option_values(&final_command, "-itsoffset"), ["-0.539056"]);
                assert_eq!(option_values(&final_command, "-txt_page:7"), ["888"]);
                assert_eq!(
                    option_values(&final_command, "-bufsize:v"),
                    if buffer_size.is_some() {
                        vec!["800k"]
                    } else {
                        vec![]
                    }
                );
            }
        }
    }
}

#[test]
fn independent_decoder_preserves_explicit_output_seek_without_borrowing_it_as_input_seek() {
    let mut pipeline = fixture(Some(3), Some(800));
    pipeline.state.output_start = Some(Duration::from_millis(250));
    let request = SourceTeletextConsumerRequest::new(HardwareAccelerationKind::None, pipeline);
    let (initial_command, initial) = request.build().unwrap();
    let final_command = request
        .independent_command(&initial.unwrap(), 125_001)
        .unwrap();
    let scopes = input_scopes(&final_command);
    assert!(!scopes
        .last()
        .unwrap()
        .iter()
        .any(|argument| argument == "-ss"));
    assert_eq!(
        option_values(&final_command, "-ss"),
        option_values(&initial_command, "-ss")
    );
    assert_eq!(option_values(&final_command, "-itsoffset"), ["0.125001"]);
}

#[test]
fn duplicate_caption_or_exhausted_input_instance_cannot_be_rebound_ambiguously() {
    let mut duplicate = fixture(None, Some(800));
    duplicate
        .inputs
        .push(duplicate.inputs.last().unwrap().clone());
    let request = SourceTeletextConsumerRequest::new(HardwareAccelerationKind::None, duplicate);
    let (_, initial) = request.build().unwrap();
    assert!(request.independent_command(&initial.unwrap(), 0).is_err());

    let mut exhausted = fixture(None, Some(800));
    for input in &mut exhausted.inputs {
        input.input_instance = usize::MAX;
    }
    let request = SourceTeletextConsumerRequest::new(HardwareAccelerationKind::None, exhausted);
    let (_, initial) = request.build().unwrap();
    assert!(request.independent_command(&initial.unwrap(), 0).is_err());
}

#[test]
fn signed_native_offset_formatter_preserves_microseconds_without_signed_abs_overflow() {
    for (microseconds, expected) in [
        (0, "0.000000"),
        (1, "0.000001"),
        (-1, "-0.000001"),
        (539_056, "0.539056"),
        (-539_056, "-0.539056"),
        (i64::MAX, "9223372036854.775807"),
        (i64::MIN, "-9223372036854.775808"),
    ] {
        assert_eq!(signed_microseconds(microseconds), expected);
    }
}

#[test]
fn independent_sought_caption_command_snapshot() {
    // ADR 0121 requires qualification of actual consumer flags and preservation
    // of ordinary AV selection; this snapshot records the independent decoder.
    let request = SourceTeletextConsumerRequest::new(
        HardwareAccelerationKind::None,
        fixture(Some(3), Some(800)),
    );
    let (_, initial) = request.build().unwrap();
    let command = request
        .independent_command(&initial.unwrap(), -539_056)
        .unwrap();
    insta::assert_snapshot!(
        "independent_source_caption_sought_bounded_command",
        command.arguments.join(" ")
    );
}
