#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EnvironmentVariable {
    pub name: String,
    pub value: String,
}

impl EnvironmentVariable {
    #[must_use]
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamKind {
    Video,
    Audio,
    Subtitle,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PipelineStep {
    Generic {
        name: &'static str,
        global_options: Vec<String>,
        filter_options: Vec<String>,
        output_options: Vec<String>,
        environment_variables: Vec<EnvironmentVariable>,
    },
    Encoder {
        name: String,
        kind: StreamKind,
        output_options: Vec<String>,
    },
    ComplexFilter {
        options: Vec<String>,
    },
    StreamSeek {
        seconds: i64,
    },
}

impl PipelineStep {
    #[must_use]
    pub fn generic(
        name: &'static str,
        output_options: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self::Generic {
            name,
            global_options: Vec::new(),
            filter_options: Vec::new(),
            output_options: output_options.into_iter().map(Into::into).collect(),
            environment_variables: Vec::new(),
        }
    }

    #[must_use]
    pub fn global(
        name: &'static str,
        options: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self::Generic {
            name,
            global_options: options.into_iter().map(Into::into).collect(),
            filter_options: Vec::new(),
            output_options: Vec::new(),
            environment_variables: Vec::new(),
        }
    }

    #[must_use]
    pub fn filter(
        name: &'static str,
        options: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self::Generic {
            name,
            global_options: Vec::new(),
            filter_options: options.into_iter().map(Into::into).collect(),
            output_options: Vec::new(),
            environment_variables: Vec::new(),
        }
    }

    #[must_use]
    pub fn env(name: &'static str, variable: EnvironmentVariable) -> Self {
        Self::Generic {
            name,
            global_options: Vec::new(),
            filter_options: Vec::new(),
            output_options: Vec::new(),
            environment_variables: vec![variable],
        }
    }

    #[must_use]
    pub fn encoder(
        name: impl Into<String>,
        kind: StreamKind,
        extra: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        let name = name.into();
        let mut output_options = vec![format!("-codec:{}", stream_suffix(kind)), name.clone()];
        output_options.extend(extra.into_iter().map(Into::into));
        Self::Encoder {
            name,
            kind,
            output_options,
        }
    }

    #[must_use]
    pub fn complex_filter(options: Vec<String>) -> Self {
        Self::ComplexFilter { options }
    }

    #[must_use]
    pub fn stream_seek(seconds: i64) -> Self {
        Self::StreamSeek { seconds }
    }

    #[must_use]
    pub fn global_options(&self) -> &[String] {
        match self {
            Self::Generic { global_options, .. } => global_options,
            Self::StreamSeek { .. } | Self::Encoder { .. } | Self::ComplexFilter { .. } => &[],
        }
    }

    #[must_use]
    pub fn filter_options(&self) -> &[String] {
        match self {
            Self::Generic { filter_options, .. } => filter_options,
            Self::ComplexFilter { options } => options,
            Self::StreamSeek { .. } | Self::Encoder { .. } => &[],
        }
    }

    #[must_use]
    pub fn output_options(&self) -> &[String] {
        match self {
            Self::Generic { output_options, .. } | Self::Encoder { output_options, .. } => {
                output_options
            }
            Self::StreamSeek { .. } | Self::ComplexFilter { .. } => &[],
        }
    }

    #[must_use]
    pub fn environment_variables(&self) -> &[EnvironmentVariable] {
        match self {
            Self::Generic {
                environment_variables,
                ..
            } => environment_variables,
            Self::StreamSeek { .. } | Self::Encoder { .. } | Self::ComplexFilter { .. } => &[],
        }
    }

    #[must_use]
    pub fn is_video_encoder(&self) -> bool {
        matches!(
            self,
            Self::Encoder {
                kind: StreamKind::Video,
                ..
            }
        )
    }

    #[must_use]
    pub fn is_reordered_out_of_output_pass(&self) -> bool {
        matches!(self, Self::StreamSeek { .. } | Self::ComplexFilter { .. })
    }
}

const fn stream_suffix(kind: StreamKind) -> &'static str {
    match kind {
        StreamKind::Video => "v",
        StreamKind::Audio => "a",
        StreamKind::Subtitle => "s",
        StreamKind::Other => "d",
    }
}

#[must_use]
pub fn no_standard_input() -> PipelineStep {
    PipelineStep::global("nostdin", ["-nostdin"])
}

#[must_use]
pub fn hide_banner() -> PipelineStep {
    PipelineStep::global("hide_banner", ["-hide_banner"])
}

#[must_use]
pub fn no_stats() -> PipelineStep {
    PipelineStep::global("nostats", ["-nostats"])
}

#[must_use]
pub fn progress_to_stdout() -> PipelineStep {
    PipelineStep::global("progress", ["-progress", "pipe:1"])
}

#[must_use]
pub fn loglevel_error() -> PipelineStep {
    PipelineStep::global("loglevel_error", ["-loglevel", "error"])
}

#[must_use]
pub fn standard_format_flags() -> PipelineStep {
    PipelineStep::global(
        "standard_format_flags",
        ["-fflags", "+genpts+discardcorrupt+igndts"],
    )
}

#[must_use]
pub fn map_all_streams() -> PipelineStep {
    PipelineStep::generic("map_all_streams", ["-map", "0"])
}

#[must_use]
pub fn copy_all_streams() -> PipelineStep {
    PipelineStep::generic("copy_all_streams", ["-c", "copy"])
}

#[must_use]
pub fn closed_gop() -> PipelineStep {
    PipelineStep::generic("closed_gop", ["-flags", "cgop"])
}

#[must_use]
pub fn no_b_frames() -> PipelineStep {
    PipelineStep::generic("no_b_frames", ["-bf", "0"])
}

#[must_use]
pub fn fast_start() -> PipelineStep {
    PipelineStep::generic("fast_start", ["-movflags", "+faststart"])
}

#[must_use]
pub fn no_demux_decode_delay() -> PipelineStep {
    PipelineStep::generic(
        "no_demux_decode_delay",
        ["-muxdelay", "0", "-muxpreload", "0"],
    )
}

#[must_use]
pub fn no_scene_detect(value: u32) -> PipelineStep {
    PipelineStep::generic(
        "no_scene_detect",
        ["-sc_threshold".to_owned(), value.to_string()],
    )
}

#[must_use]
pub fn time_limit(duration: Duration) -> PipelineStep {
    PipelineStep::generic("time_limit", ["-t".to_owned(), format_time_limit(duration)])
}

#[must_use]
pub fn format_time_limit(duration: Duration) -> String {
    let total_seconds = duration.as_secs();
    let hours = total_seconds / 3_600;
    let minutes = (total_seconds % 3_600) / 60;
    let seconds = total_seconds % 60;
    let ticks = duration.subsec_nanos() / 100;
    format!("{hours:02}:{minutes:02}:{seconds:02}.{ticks:07}")
}

#[must_use]
pub fn no_autoscale() -> PipelineStep {
    PipelineStep::generic("no_autoscale", ["-noautoscale"])
}

#[must_use]
pub fn mpegts_output(initial_discontinuity: bool) -> PipelineStep {
    if initial_discontinuity {
        PipelineStep::generic(
            "mpegts",
            ["-f", "mpegts", "-mpegts_flags", "+initial_discontinuity"],
        )
    } else {
        PipelineStep::generic("mpegts", ["-f", "mpegts"])
    }
}

#[must_use]
pub fn pipe_output() -> PipelineStep {
    PipelineStep::generic("pipe", ["pipe:1"])
}
use std::time::Duration;
