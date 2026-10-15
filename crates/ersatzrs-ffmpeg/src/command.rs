use crate::{
    closed_gop, copy_all_streams, fast_start, format_time_limit, hide_banner, loglevel_error,
    map_all_streams, mpegts_muxrate, mpegts_output, mpegts_pcr_period, no_demux_decode_delay,
    no_scene_detect, no_standard_input, no_stats, pipe_output, standard_format_flags,
    EnvironmentVariable, InputFile, InputKind, MpegTsMuxingOptions, MpegTsOutputPolicy,
    OutputMetadata, PipelineStep, MPEGTS_PCR_PERIOD_MS,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FfmpegCommand {
    pub executable: String,
    pub environment: Vec<EnvironmentVariable>,
    pub arguments: Vec<String>,
    /// Optional generated output hints; use the getter to check mutable argv.
    pub mpegts_output: Option<MpegTsOutputPolicy>,
}

impl FfmpegCommand {
    #[must_use]
    pub fn new(environment: Vec<EnvironmentVariable>, arguments: Vec<String>) -> Self {
        Self {
            executable: crate::ffmpeg_path(),
            environment,
            arguments,
            mpegts_output: None,
        }
    }

    /// Build a command with an explicit `ffmpeg` executable path, bypassing the
    /// process-wide resolved-path holder. Used where the caller has already
    /// resolved the path (e.g. from DB config).
    #[must_use]
    pub fn new_with_executable(
        executable: impl Into<String>,
        environment: Vec<EnvironmentVariable>,
        arguments: Vec<String>,
    ) -> Self {
        Self {
            executable: executable.into(),
            environment,
            arguments,
            mpegts_output: None,
        }
    }

    /// Borrow generated final-TS hints only while argv still matches its seal.
    /// Executable/environment identity and actual source PSI/PCR remain the
    /// caller's responsibility; mutations never re-infer policy from raw argv.
    #[must_use]
    pub fn mpegts_output_policy(&self) -> Option<&MpegTsOutputPolicy> {
        self.mpegts_output
            .as_ref()
            .filter(|policy| policy.matches(&self.arguments))
    }
}

/// Build the MPEG-TS hybrid wrapper command used by ErsatzTV's
/// `FFmpegLibraryProcessService.WrapSegmenter`.
///
/// The input is the live HLS segmenter URL. The wrapper does not transcode; it
/// paces reads from the segmenter playlist, copies every stream, and may use a
/// channel-derived constant mux rate so the MPEG-TS muxer emits regular PCR.
#[must_use]
pub fn build_mpegts_segmenter_wrapper_command(
    hls_url: impl Into<String>,
    metadata: &OutputMetadata,
    aac_latm_audio: bool,
    muxrate_bps: Option<i64>,
) -> FfmpegCommand {
    let mut input = InputFile::video(hls_url);
    if aac_latm_audio {
        input
            .input_options
            .extend(["-c:a".to_owned(), "aac_latm".to_owned()]);
    }
    input.add_readrate("1.0");

    let mut steps = vec![
        no_standard_input(),
        PipelineStep::global("threads", ["-threads", "1"]),
        hide_banner(),
        loglevel_error(),
        no_stats(),
        standard_format_flags(),
        map_all_streams(),
        copy_all_streams(),
    ];
    if let Some(muxrate_bps) = muxrate_bps {
        let muxing = MpegTsMuxingOptions {
            muxrate_bps,
            pcr_period_ms: MPEGTS_PCR_PERIOD_MS,
        };
        steps.push(mpegts_muxrate(muxing));
        steps.push(mpegts_pcr_period(muxing));
    }
    add_service_metadata_steps(&mut steps, metadata);
    steps.push(mpegts_output(false));
    steps.push(pipe_output());

    CommandGenerator::generate_command(&[input], &steps, false)
}

/// Build the MPEG-TS legacy concat command used by ErsatzTV's
/// `FFmpegLibraryProcessService.ConcatChannel`.
///
/// The input URL is the internal `ffconcat` playlist endpoint. `FFmpeg` loops the
/// concat playlist, paces reads in realtime, copies every stream, and emits an
/// MPEG-TS stdout pipe.
#[must_use]
pub fn build_mpegts_concat_command(
    concat_url: impl Into<String>,
    metadata: &OutputMetadata,
    muxrate_bps: Option<i64>,
) -> FfmpegCommand {
    let mut input = InputFile::concat(concat_url);
    input.add_readrate("1.0");
    input
        .input_options
        .extend(["-stream_loop".to_owned(), "-1".to_owned()]);

    let mut steps = vec![
        no_standard_input(),
        hide_banner(),
        no_stats(),
        loglevel_error(),
        standard_format_flags(),
        no_demux_decode_delay(),
        fast_start(),
        closed_gop(),
        no_scene_detect(0),
        // ADR 0121 requires source captions and alternate audio to survive the
        // final remux. EncoderCopyAll alone still uses FFmpeg's default stream
        // selection in ErsatzTV's legacy Concat pipeline.
        map_all_streams(),
        copy_all_streams(),
        // Keep the channel's global metadata separate from source language and
        // subtitle descriptors; FFmpeg 9's unscoped negative map drops both.
        PipelineStep::generic("no_metadata", ["-map_metadata:g", "-1"]),
    ];
    add_service_metadata_steps(&mut steps, metadata);
    if let Some(muxrate_bps) = muxrate_bps {
        let muxing = MpegTsMuxingOptions {
            muxrate_bps,
            pcr_period_ms: MPEGTS_PCR_PERIOD_MS,
        };
        steps.push(mpegts_muxrate(muxing));
        steps.push(mpegts_pcr_period(muxing));
    }
    steps.push(mpegts_output(true));
    steps.push(pipe_output());

    CommandGenerator::generate_command(&[input], &steps, false)
}

fn add_service_metadata_steps(steps: &mut Vec<PipelineStep>, metadata: &OutputMetadata) {
    if let Some(service_provider) = metadata.service_provider.as_deref() {
        steps.push(PipelineStep::generic(
            "metadata_service_provider",
            [
                "-metadata".to_owned(),
                format!("service_provider=\"{service_provider}\""),
            ],
        ));
    }

    if let Some(service_name) = metadata.service_name.as_deref() {
        steps.push(PipelineStep::generic(
            "metadata_service_name",
            [
                "-metadata".to_owned(),
                format!("service_name=\"{service_name}\""),
            ],
        ));
    }
}

pub struct CommandGenerator;

impl CommandGenerator {
    #[must_use]
    pub fn generate_environment_variables(
        pipeline_steps: &[PipelineStep],
    ) -> Vec<EnvironmentVariable> {
        pipeline_steps
            .iter()
            .flat_map(PipelineStep::environment_variables)
            .cloned()
            .collect()
    }

    #[must_use]
    pub fn generate_arguments(
        inputs: &[InputFile],
        pipeline_steps: &[PipelineStep],
        is_intel_vaapi_or_qsv: bool,
    ) -> Vec<String> {
        let mut arguments = Vec::new();

        for step in pipeline_steps {
            arguments.extend(step.global_options().iter().cloned());
        }

        for step in pipeline_steps {
            if let PipelineStep::StreamSeek { position } = step {
                arguments.extend(["-ss".to_owned(), format_time_limit(*position)]);
            }
        }

        let mut included_sources = Vec::new();
        let same_audio_and_video = same_audio_and_video(inputs);

        for kind in [
            InputKind::Video,
            InputKind::Audio,
            InputKind::Watermark,
            InputKind::GraphicsImage,
            InputKind::Concat,
            InputKind::Graphics,
            InputKind::Subtitle,
        ] {
            for input in inputs.iter().filter(|input| input.kind == kind) {
                if kind == InputKind::Subtitle && !input.is_image_based_subtitle {
                    continue;
                }

                let should_include = match kind {
                    InputKind::Audio => {
                        !included_sources.contains(&input.source_key()) || is_intel_vaapi_or_qsv
                    }
                    InputKind::Watermark | InputKind::GraphicsImage | InputKind::Subtitle => {
                        !included_sources.contains(&input.source_key())
                    }
                    InputKind::Video | InputKind::Concat | InputKind::Graphics => true,
                };

                if should_include {
                    // Match ComplexFilter's first physical owner. Decoder
                    // options must survive logical subtitle deduplication, but
                    // must not follow an Intel audio-only duplicate/reopen.
                    let first_source_owner = !included_sources.contains(&input.source_key());
                    included_sources.push(input.source_key());
                    if kind == InputKind::Video && same_audio_and_video && !is_intel_vaapi_or_qsv {
                        for audio_input in inputs.iter().filter(|audio_input| {
                            audio_input.kind == InputKind::Audio
                                && audio_input.source_key() == input.source_key()
                                && !audio_input.input_options.is_empty()
                        }) {
                            arguments.extend(shared_video_audio_input_options(audio_input));
                        }
                    }
                    arguments.extend(input.input_options.iter().cloned());
                    if first_source_owner {
                        arguments.extend(source_teletext_caption_input_options(inputs, input));
                    }
                    arguments.extend(["-i".to_owned(), input.path.clone()]);
                } else if same_audio_and_video && kind == InputKind::Audio && is_intel_vaapi_or_qsv
                {
                    arguments.extend(input.input_options.iter().cloned());
                }
            }
        }

        for step in pipeline_steps {
            arguments.extend(step.filter_options().iter().cloned());
        }

        let mut sorted_steps: Vec<&PipelineStep> = pipeline_steps
            .iter()
            .filter(|step| !step.is_reordered_out_of_output_pass())
            .collect();
        if let Some(complex) = pipeline_steps
            .iter()
            .find(|step| matches!(step, PipelineStep::ComplexFilter { .. }))
        {
            let insert_index = sorted_steps
                .iter()
                .position(|step| step.is_video_encoder())
                .map_or(sorted_steps.len(), |index| index + 1);
            sorted_steps.insert(insert_index, complex);
        }

        for step in sorted_steps {
            arguments.extend(step.output_options().iter().cloned());
        }

        arguments
    }

    #[must_use]
    pub fn generate_command(
        inputs: &[InputFile],
        pipeline_steps: &[PipelineStep],
        is_intel_vaapi_or_qsv: bool,
    ) -> FfmpegCommand {
        let mut command = FfmpegCommand::new(
            Self::generate_environment_variables(pipeline_steps),
            Self::generate_arguments(inputs, pipeline_steps, is_intel_vaapi_or_qsv),
        );
        command.mpegts_output =
            MpegTsOutputPolicy::from_generated(pipeline_steps, &command.arguments);
        command
    }
}

fn source_teletext_caption_input_options(inputs: &[InputFile], owner: &InputFile) -> Vec<String> {
    // ComplexFilter uses one selected bitmap Burn input and its first stream.
    // An unused later logical input must never override that decoder's page.
    let Some(subtitle) = inputs.iter().find(|subtitle| {
        subtitle.kind == InputKind::Subtitle
            && subtitle.is_image_based_subtitle
            && subtitle.subtitle_method == Some(crate::SubtitleMethod::Burn)
    }) else {
        return Vec::new();
    };
    if subtitle.source_key() != owner.source_key() {
        return Vec::new();
    }
    let Some(stream) = subtitle.streams.first() else {
        return Vec::new();
    };
    let Some(page) = stream.source_teletext_page else {
        return Vec::new();
    };
    // Absolute source stream index, not subtitle ordinal or PID.
    // Unknown charset provenance uses the decoder's automatic region.
    vec![
        format!("-txt_page:{}", stream.index),
        page.get().to_string(),
        format!("-txt_format:{}", stream.index),
        "bitmap".to_owned(),
        format!("-txt_default_region:{}", stream.index),
        "-1".to_owned(),
    ]
}

fn same_audio_and_video(inputs: &[InputFile]) -> bool {
    let video_path = inputs
        .iter()
        .find(|input| input.kind == InputKind::Video)
        .map(InputFile::source_key);
    let audio_path = inputs
        .iter()
        .find(|input| input.kind == InputKind::Audio)
        .map(InputFile::source_key);
    video_path.is_some() && video_path == audio_path
}

fn shared_video_audio_input_options(input: &InputFile) -> Vec<String> {
    let mut result = Vec::new();
    let mut options = input.input_options.iter();
    while let Some(option) = options.next() {
        if option == "-readrate" {
            let _ = options.next();
            continue;
        }

        result.push(option.clone());
        if let Some(value) = options.next() {
            result.push(value.clone());
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn teletext_caption(path: &str) -> InputFile {
        InputFile::teletext_subtitle(
            path,
            2,
            ersatzrs_domain::TeletextPageNumber::new(888).unwrap(),
        )
    }

    #[test]
    fn source_teletext_caption_keeps_one_shared_pipe_and_stream_scoped_options() {
        let inputs = [
            InputFile::video("pipe:0").with_input_options(["-f", "mpegts"]),
            InputFile::audio("pipe:0").with_input_options(["-c:a", "aac_latm"]),
            teletext_caption("pipe:0"),
        ];
        let args = CommandGenerator::generate_arguments(&inputs, &[], false);
        insta::assert_snapshot!(args.join(" "));
        assert_eq!(args.iter().filter(|arg| *arg == "-i").count(), 1);
        let input_position = args.iter().position(|arg| arg == "-i").unwrap();
        for key in ["-txt_page:2", "-txt_format:2", "-txt_default_region:2"] {
            assert!(args.iter().position(|arg| arg == key).unwrap() < input_position);
        }
        let filter = crate::ComplexFilter::build(&inputs, crate::FilterChain::default(), false);
        assert_eq!(filter.video_label, "0:0");
        assert_eq!(filter.audio_label, "0:1");
    }

    #[test]
    fn source_teletext_caption_uses_first_owner_with_intel_audio_reopen() {
        let inputs = [
            InputFile::video("pipe:0"),
            InputFile::audio("pipe:0"),
            teletext_caption("pipe:0"),
        ];
        let args = CommandGenerator::generate_arguments(&inputs, &[], true);
        insta::assert_snapshot!(args.join(" "));
        assert_eq!(args.iter().filter(|arg| *arg == "-i").count(), 2);
        assert_eq!(args.iter().filter(|arg| *arg == "-txt_page:2").count(), 1);
        assert!(
            args.iter().position(|arg| arg == "-txt_page:2").unwrap()
                < args.iter().position(|arg| arg == "-i").unwrap()
        );
    }

    #[test]
    fn source_teletext_caption_respects_separate_input_instance() {
        let mut subtitle = teletext_caption("same.ts");
        subtitle.input_instance = 1;
        let inputs = [InputFile::video("same.ts"), subtitle];
        let args = CommandGenerator::generate_arguments(&inputs, &[], false);
        insta::assert_snapshot!(args.join(" "));
        assert_eq!(&args[..2], ["-i", "same.ts"]);
        assert_eq!(args.iter().filter(|arg| *arg == "-i").count(), 2);
    }

    #[test]
    fn source_teletext_caption_does_not_modify_an_unrelated_source() {
        let inputs = [
            InputFile::video("other.ts"),
            teletext_caption("captions.ts"),
        ];
        let args = CommandGenerator::generate_arguments(&inputs, &[], false);
        insta::assert_snapshot!(args.join(" "));
        assert_eq!(&args[..2], ["-i", "other.ts"]);
    }

    #[test]
    fn source_teletext_caption_flags_follow_the_current_stream_index() {
        let inputs = [teletext_caption("caption.ts").with_stream_index(4)];
        let args = CommandGenerator::generate_arguments(&inputs, &[], false);
        assert_eq!(args[0], "-txt_page:4");
        assert!(!args.iter().any(|arg| arg.ends_with(":2")));
        assert_eq!(inputs[0].subtitle_method, Some(crate::SubtitleMethod::Burn));
        assert!(inputs[0].is_image_based_subtitle);
    }

    #[test]
    fn source_teletext_caption_leaves_ordinary_bitmap_and_text_inputs_unchanged() {
        for subtitle in [
            InputFile::image_subtitle("pipe:0", crate::SubtitleMethod::Burn),
            InputFile::text_subtitle("sidecar.srt", crate::SubtitleMethod::Burn),
        ] {
            let inputs = [InputFile::video("pipe:0"), subtitle];
            assert_eq!(
                CommandGenerator::generate_arguments(&inputs, &[], false),
                ["-i", "pipe:0"]
            );
        }
    }

    #[test]
    fn source_teletext_caption_matches_the_first_selected_bitmap_and_stream() {
        let first = teletext_caption("pipe:0");
        let mut unused = InputFile::teletext_subtitle(
            "pipe:0",
            2,
            ersatzrs_domain::TeletextPageNumber::new(200).unwrap(),
        );
        unused.streams.push(crate::MediaStream::new(4));
        let inputs = [InputFile::video("pipe:0"), first.clone(), unused];
        assert_eq!(
            CommandGenerator::generate_arguments(&inputs, &[], false),
            CommandGenerator::generate_arguments(&inputs[..2], &[], false)
        );
        let mut first = first;
        first.streams.push(crate::MediaStream::new(3));
        let args = CommandGenerator::generate_arguments(&[first], &[], false);
        assert!(!args.iter().any(|arg| arg.starts_with("-txt_page:3")));
    }

    #[test]
    fn source_teletext_caption_does_not_enable_an_unselected_later_caption() {
        let inputs = [
            InputFile::video("pipe:0"),
            InputFile::image_subtitle("pipe:0", crate::SubtitleMethod::Burn).with_stream_index(3),
            teletext_caption("pipe:0"),
        ];
        assert_eq!(
            CommandGenerator::generate_arguments(&inputs, &[], false),
            ["-i", "pipe:0"]
        );
    }

    #[test]
    fn canvas_input_identity_preserves_single_open_for_native_audio_video() {
        let video = InputFile::video("pipe:0").with_input_options(["-f", "mpegts"]);
        let mut audio = InputFile::audio("pipe:0").with_input_options(["-c:a", "aac_latm"]);
        audio.streams[0].index = 1;
        let inputs = [video, audio];
        let complex = crate::ComplexFilter::build(&inputs, crate::FilterChain::default(), false);
        assert_eq!(complex.video_label, "0:0");
        assert_eq!(complex.audio_label, "0:1");
        let arguments = CommandGenerator::generate_arguments(&inputs, &[], false);
        assert_eq!(
            arguments
                .iter()
                .filter(|argument| *argument == "-i")
                .count(),
            1
        );
        assert!(arguments
            .windows(2)
            .any(|pair| pair == ["-c:a", "aac_latm"]));
    }

    #[test]
    fn mpegts_segmenter_wrapper_uses_cbr_muxrate_for_pcr_stability() {
        let metadata = OutputMetadata {
            service_provider: Some("ErsatzTV".to_owned()),
            service_name: Some("Channel One".to_owned()),
            ..OutputMetadata::default()
        };
        let command = build_mpegts_segmenter_wrapper_command(
            "http://localhost:8409/iptv/channel/1.m3u8?mode=segmenter",
            &metadata,
            false,
            Some(9_830_400),
        );

        insta::assert_snapshot!(
            "mpegts_segmenter_wrapper_default",
            command.arguments.join(" ")
        );
    }

    #[test]
    fn mpegts_segmenter_wrapper_adds_aac_latm_decoder() {
        let metadata = OutputMetadata::default();
        let command = build_mpegts_segmenter_wrapper_command(
            "http://localhost:8409/iptv/channel/1.m3u8?mode=segmenter",
            &metadata,
            true,
            Some(9_830_400),
        );

        let arguments = command.arguments.join(" ");
        assert!(
            arguments.contains("-c:a aac_latm -readrate 1.0 -i"),
            "{arguments}"
        );
    }

    #[test]
    fn mpegts_concat_uses_cbr_muxrate_for_pcr_stability() {
        let metadata = OutputMetadata {
            service_provider: Some("ErsatzTV".to_owned()),
            service_name: Some("Channel One".to_owned()),
            ..OutputMetadata::default()
        };
        let command = build_mpegts_concat_command(
            "http://localhost:8409/ffmpeg/concat/1?mode=ts-legacy",
            &metadata,
            Some(9_830_400),
        );

        insta::assert_snapshot!("mpegts_concat_default", command.arguments.join(" "));
    }

    #[test]
    fn multi_day_seeks_and_duration_keep_input_output_order() {
        let position = std::time::Duration::new(172_861, 123_456_700);
        let arguments = CommandGenerator::generate_arguments(
            &[InputFile::video("fixture.mkv")],
            &[
                PipelineStep::stream_seek(position),
                PipelineStep::generic(
                    "output_seek",
                    ["-ss".to_owned(), format_time_limit(position)],
                ),
                crate::options::time_limit(std::time::Duration::from_hours(100)),
                pipe_output(),
            ],
            false,
        );
        insta::assert_snapshot!(arguments.join(" "), @"-ss 48:01:01.1234567 -i fixture.mkv -ss 48:01:01.1234567 -t 100:00:00.0000000 pipe:1");
    }

    #[test]
    fn output_seek_is_rendered_after_non_seekable_input() {
        let arguments = CommandGenerator::generate_arguments(
            &[InputFile::video("pipe:0").with_input_options(["-f", "mpegts"])],
            &[
                PipelineStep::generic("output_seek", ["-ss", "00:02:03.000"]),
                pipe_output(),
            ],
            false,
        );
        let input_index = arguments
            .windows(2)
            .position(|pair| pair == ["-i", "pipe:0"])
            .expect("input");
        let seek_index = arguments
            .windows(2)
            .position(|pair| pair == ["-ss", "00:02:03.000"])
            .expect("seek");

        assert!(seek_index > input_index, "{arguments:?}");
    }
}
