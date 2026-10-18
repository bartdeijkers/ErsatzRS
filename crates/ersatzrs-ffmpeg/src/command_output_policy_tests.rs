use ersatzrs_domain::HardwareAccelerationKind;

use crate::{
    build_mpegts_concat_command, build_mpegts_segmenter_wrapper_command,
    build_pipeline_for_acceleration, mpegts_muxrate, mpegts_output, mpegts_pcr_period, pipe_output,
    CommandGenerator, DesiredVideoState, FfmpegCommand, InputFile, MpegTsMuxingOptions,
    OutputFormat, OutputMetadata, PipelineRequest, PipelineStep,
};

fn steps() -> Vec<PipelineStep> {
    let muxing = MpegTsMuxingOptions {
        muxrate_bps: 5_000_000,
        pcr_period_ms: 20,
    };
    vec![
        mpegts_muxrate(muxing),
        mpegts_pcr_period(muxing),
        mpegts_output(true),
        pipe_output(),
    ]
}

fn generate(steps: &[PipelineStep]) -> FfmpegCommand {
    CommandGenerator::generate_command(&[InputFile::video("synthetic.mkv")], steps, false)
}

#[test]
fn native_pipeline_records_only_generated_final_ts_hints_without_changing_argv() {
    let mut request = PipelineRequest::new(
        InputFile::video("synthetic.mkv"),
        DesiredVideoState::h264_1080p(),
    );
    request.state.report_progress = false;
    let pipeline = build_pipeline_for_acceleration(HardwareAccelerationKind::None, request);
    let command = pipeline.command(&[]);
    assert_eq!(
        command.arguments,
        CommandGenerator::generate_arguments(&pipeline.inputs, &pipeline.steps, false)
    );
    let policy = command.mpegts_output_policy().unwrap();
    assert_eq!(policy.muxrate_bps(), 9_830_400);
    assert_eq!(policy.pcr_period_ms(), 20);
    assert_eq!(command.arguments.last().unwrap(), "pipe:1");
}

#[test]
fn concat_and_segmenter_wrappers_keep_input_container_scope_and_explicit_rate() {
    let metadata = OutputMetadata::default();
    for command in [
        build_mpegts_concat_command(
            "http://synthetic.invalid/concat",
            &metadata,
            Some(5_000_000),
        ),
        build_mpegts_segmenter_wrapper_command(
            "http://synthetic.invalid/segments.m3u8",
            &metadata,
            true,
            Some(5_000_000),
        ),
    ] {
        let policy = command.mpegts_output_policy().unwrap();
        assert_eq!(policy.muxrate_bps(), 5_000_000);
        assert_eq!(policy.pcr_period_ms(), 20);
    }
    assert!(build_mpegts_concat_command("synthetic", &metadata, None)
        .mpegts_output_policy()
        .is_none());
    assert!(
        build_mpegts_segmenter_wrapper_command("synthetic", &metadata, false, None)
            .mpegts_output_policy()
            .is_none()
    );
}

#[test]
fn bounded_sparse_interleave_keeps_generated_rate_policy_and_full_argument_seal() {
    let mut candidate = steps();
    candidate.insert(0, crate::command::sparse_subtitle_interleave());
    let command = generate(&candidate);
    let policy = command.mpegts_output_policy().unwrap();
    assert_eq!(policy.muxrate_bps(), 5_000_000);
    assert_eq!(policy.pcr_period_ms(), 20);
    let mut changed = command.clone();
    let index = changed
        .arguments
        .iter()
        .position(|value| value == "-max_interleave_delta")
        .unwrap();
    changed.arguments[index + 1] = "200000".into();
    assert!(changed.mpegts_output_policy().is_none());
    candidate[0] = PipelineStep::generic("sparse_subtitle_interleave", ["-max_interleave_delta"]);
    assert!(generate(&candidate).mpegts_output_policy().is_none());
}

#[test]
fn opaque_commands_do_not_infer_policy_even_for_identical_generated_argv() {
    let generated = generate(&steps());
    assert!(generated.mpegts_output_policy().is_some());
    for opaque in [
        FfmpegCommand::new(Vec::new(), generated.arguments.clone()),
        FfmpegCommand::new_with_executable("synthetic-shell", Vec::new(), generated.arguments),
    ] {
        assert!(opaque.mpegts_output.is_none());
        assert!(opaque.mpegts_output_policy().is_none());
    }
}

#[test]
fn hls_and_other_generated_containers_do_not_acquire_final_ts_policy() {
    for format in [
        OutputFormat::Hls,
        OutputFormat::HlsMp4,
        OutputFormat::Mp4,
        OutputFormat::Nut,
    ] {
        let mut request = PipelineRequest::new(
            InputFile::video("synthetic.mkv"),
            DesiredVideoState::h264_1080p(),
        );
        request.state.output_format = format;
        let pipeline = build_pipeline_for_acceleration(HardwareAccelerationKind::None, request);
        assert!(pipeline.command(&[]).mpegts_output_policy().is_none());
    }
}

#[test]
fn malformed_or_duplicate_named_steps_never_produce_policy() {
    for replacement in [
        PipelineStep::generic("mpegts_muxrate", ["-muxrate", "0"]),
        PipelineStep::generic("mpegts_muxrate", ["-muxrate", "-1"]),
        PipelineStep::generic("mpegts_muxrate", ["-muxrate", "05000000"]),
        PipelineStep::generic("mpegts_muxrate", ["-muxrate", "9223372036854775808"]),
        PipelineStep::generic("mpegts_muxrate", ["-muxrate", "18446744073709551616"]),
        PipelineStep::generic("mpegts_muxrate", ["-muxrate"]),
        PipelineStep::global("mpegts_muxrate", ["-muxrate", "5000000"]),
    ] {
        let mut candidate = steps();
        candidate[0] = replacement;
        assert!(generate(&candidate).mpegts_output.is_none());
    }
    for index in 0..4 {
        let mut duplicate = steps();
        duplicate.push(duplicate[index].clone());
        assert!(generate(&duplicate).mpegts_output.is_none());
        let mut missing = steps();
        missing.remove(index);
        assert!(generate(&missing).mpegts_output.is_none());
    }
}

#[test]
fn duplicate_flags_wrong_containers_periods_and_multiple_output_pipes_are_rejected() {
    for additional in [
        PipelineStep::generic("other_rate", ["-muxrate", "5000000"]),
        PipelineStep::generic("other_period", ["-pcr_period", "20"]),
        PipelineStep::generic("other_container", ["-f", "mpegts"]),
        PipelineStep::generic("other_output", ["pipe:2"]),
    ] {
        let mut candidate = steps();
        candidate.insert(0, additional);
        assert!(generate(&candidate).mpegts_output.is_none());
    }
    for period in ["0", "-1", "4294967296", "twenty"] {
        let mut candidate = steps();
        candidate[1] = PipelineStep::generic("mpegts_pcr_period", ["-pcr_period", period]);
        assert!(generate(&candidate).mpegts_output.is_none());
    }
    let mut candidate = steps();
    candidate[2] = PipelineStep::generic("mpegts", ["-f", "hls"]);
    assert!(generate(&candidate).mpegts_output.is_none());
    candidate = steps();
    candidate.push(PipelineStep::generic("trailing_output", ["extra.ts"]));
    assert!(generate(&candidate).mpegts_output.is_none());
}

#[test]
fn input_pipe_is_allowed_but_every_public_argv_mutation_invalidates_the_seal() {
    let original = CommandGenerator::generate_command(
        &[InputFile::video("pipe:0").with_input_options(["-f", "nut"])],
        &steps(),
        false,
    );
    assert!(original.mpegts_output_policy().is_some());
    for (flag, value) in [
        ("-muxrate", "6000000"),
        ("-pcr_period", "40"),
        ("-f", "matroska"),
    ] {
        let mut changed = original.clone();
        let index = changed
            .arguments
            .iter()
            .position(|arg| arg == flag)
            .unwrap();
        changed.arguments[index + 1] = value.to_owned();
        assert!(changed.mpegts_output.is_some());
        assert!(changed.mpegts_output_policy().is_none());
    }
    let mut changed = original.clone();
    changed
        .arguments
        .splice(0..0, ["-muxrate".to_owned(), "5000000".to_owned()]);
    assert!(changed.mpegts_output_policy().is_none());
    let mut changed = original.clone();
    changed.arguments.push("pipe:2".to_owned());
    assert!(changed.mpegts_output_policy().is_none());
    let mut changed = original.clone();
    changed.arguments[0] = "different-input-scope".to_owned();
    assert!(changed.mpegts_output_policy().is_none());
    let mut override_path = original;
    override_path.executable = "another-managed-runtime".to_owned();
    assert!(override_path.mpegts_output_policy().is_some());
}

#[test]
fn transplanted_policy_is_bound_to_argv_and_debug_reveals_only_numeric_hints() {
    let generated = generate(&steps());
    let policy = generated.mpegts_output_policy().unwrap();
    let debug = format!("{policy:?}");
    assert!(debug.contains("5000000"));
    for private in ["synthetic.mkv", "arguments_digest", "pipe:1"] {
        assert!(!debug.contains(private));
    }
    let mut other = FfmpegCommand::new(Vec::new(), vec!["synthetic-script".to_owned()]);
    other.mpegts_output = generated.mpegts_output;
    assert!(other.mpegts_output_policy().is_none());
}

#[test]
fn earlier_positional_sinks_cannot_consume_the_final_muxer_hints() {
    for position in 0..4 {
        let mut candidate = steps();
        candidate.insert(
            position,
            PipelineStep::generic("intermediate_output", ["extra.ts"]),
        );
        assert!(generate(&candidate).mpegts_output.is_none());
    }
    for options in [
        vec!["-map", "0", "extra.ts"],
        vec!["-unknown-option", "0"],
        vec!["-map"],
    ] {
        let mut candidate = steps();
        candidate.insert(2, PipelineStep::generic("additional_options", options));
        assert!(generate(&candidate).mpegts_output.is_none());
    }
    let mut candidate = steps();
    candidate.insert(2, PipelineStep::filter("intermediate_output", ["extra.ts"]));
    assert!(generate(&candidate).mpegts_output.is_none());
}

#[test]
fn progress_sharing_stdout_is_not_a_qualified_final_ts_pipe() {
    let request = PipelineRequest::new(
        InputFile::video("synthetic.mkv"),
        DesiredVideoState::h264_1080p(),
    );
    let pipeline = build_pipeline_for_acceleration(HardwareAccelerationKind::None, request);
    assert!(pipeline.command(&[]).mpegts_output.is_none());
}
