use crate::{EnvironmentVariable, InputFile, InputKind, PipelineStep};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FfmpegCommand {
    pub executable: String,
    pub environment: Vec<EnvironmentVariable>,
    pub arguments: Vec<String>,
}

impl FfmpegCommand {
    #[must_use]
    pub fn new(environment: Vec<EnvironmentVariable>, arguments: Vec<String>) -> Self {
        Self {
            executable: crate::ffmpeg_path_from_env(),
            environment,
            arguments,
        }
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
            if let PipelineStep::StreamSeek { seconds } = step {
                arguments.extend(["-ss".to_owned(), seconds.to_string()]);
            }
        }

        let mut included_paths = Vec::<String>::new();
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
                        !included_paths.contains(&input.path) || is_intel_vaapi_or_qsv
                    }
                    InputKind::Watermark | InputKind::GraphicsImage | InputKind::Subtitle => {
                        !included_paths.contains(&input.path)
                    }
                    InputKind::Video | InputKind::Concat | InputKind::Graphics => true,
                };

                if should_include {
                    included_paths.push(input.path.clone());
                    arguments.extend(input.input_options.iter().cloned());
                    arguments.extend(["-i".to_owned(), input.path.clone()]);
                } else if same_audio_and_video && kind == InputKind::Audio {
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
        FfmpegCommand::new(
            Self::generate_environment_variables(pipeline_steps),
            Self::generate_arguments(inputs, pipeline_steps, is_intel_vaapi_or_qsv),
        )
    }
}

fn same_audio_and_video(inputs: &[InputFile]) -> bool {
    let video_path = inputs
        .iter()
        .find(|input| input.kind == InputKind::Video)
        .map(|input| &input.path);
    let audio_path = inputs
        .iter()
        .find(|input| input.kind == InputKind::Audio)
        .map(|input| &input.path);
    video_path.is_some() && video_path == audio_path
}
