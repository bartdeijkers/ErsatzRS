use crate::{InputFile, InputKind, SubtitleMethod};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FilterChain {
    pub video: Vec<String>,
    pub audio: Vec<String>,
    pub watermark: Vec<String>,
    pub subtitle: Vec<String>,
    pub graphics: Vec<String>,
    pub watermark_overlay: Vec<String>,
    pub subtitle_overlay: Vec<String>,
    pub graphics_overlay: Vec<String>,
    pub pixel_format: Vec<String>,
}

impl FilterChain {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.video.is_empty()
            && self.audio.is_empty()
            && self.watermark.is_empty()
            && self.subtitle.is_empty()
            && self.graphics.is_empty()
            && self.watermark_overlay.is_empty()
            && self.subtitle_overlay.is_empty()
            && self.graphics_overlay.is_empty()
            && self.pixel_format.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComplexFilter {
    pub filter_chain: FilterChain,
    pub options: Vec<String>,
    pub filter_graph: Option<String>,
    pub video_label: String,
    pub audio_label: String,
}

impl ComplexFilter {
    #[must_use]
    pub fn build(
        inputs: &[InputFile],
        filter_chain: FilterChain,
        is_intel_vaapi_or_qsv: bool,
    ) -> Self {
        let mut builder = ComplexFilterBuilder::new(inputs, is_intel_vaapi_or_qsv);
        let (options, filter_graph, video_label, audio_label) = builder.arguments(&filter_chain);
        Self {
            filter_chain,
            options,
            filter_graph,
            video_label,
            audio_label,
        }
    }
}

struct ComplexFilterBuilder<'a> {
    inputs: &'a [InputFile],
    distinct_paths: Vec<String>,
}

impl<'a> ComplexFilterBuilder<'a> {
    fn new(inputs: &'a [InputFile], is_intel_vaapi_or_qsv: bool) -> Self {
        let mut distinct_paths = Vec::new();
        for input in inputs {
            match input.kind {
                InputKind::Audio
                    if distinct_paths.contains(&input.path) && is_intel_vaapi_or_qsv =>
                {
                    distinct_paths.push(input.path.clone());
                }
                InputKind::Subtitle if !input.is_image_based_subtitle => {}
                _ if !distinct_paths.contains(&input.path) => {
                    distinct_paths.push(input.path.clone());
                }
                _ => {}
            }
        }

        Self {
            inputs,
            distinct_paths,
        }
    }

    #[allow(clippy::too_many_lines)]
    fn arguments(
        &mut self,
        filter_chain: &FilterChain,
    ) -> (Vec<String>, Option<String>, String, String) {
        let mut audio_label = "0:a".to_owned();
        let mut video_label = "0:v".to_owned();
        let mut watermark_label = None::<String>;
        let mut subtitle_label = None::<String>;
        let mut graphics_label = None::<String>;

        let mut graph_parts = Vec::new();

        if let Some(video) = self
            .inputs
            .iter()
            .find(|input| input.kind == InputKind::Video)
        {
            let input_index = self.input_index(video, false);
            if let Some(stream) = video.streams.first() {
                video_label = format!("{input_index}:{}", stream.index);
                let filters =
                    join_filters(video.filter_steps.iter().chain(filter_chain.video.iter()));
                if !filters.is_empty() {
                    graph_parts.push(format!("[{video_label}]{filters}[v]"));
                    "[v]".clone_into(&mut video_label);
                }
            }
        }

        if let Some(watermark) = self
            .inputs
            .iter()
            .find(|input| input.kind == InputKind::Watermark)
        {
            let input_index = self.input_index(watermark, false);
            if let Some(stream) = watermark.streams.first() {
                let label = format!("{input_index}:{}", stream.index);
                let filters = join_filters(
                    watermark
                        .filter_steps
                        .iter()
                        .chain(filter_chain.watermark.iter()),
                );
                if filters.is_empty() {
                    watermark_label = Some(format!("[{label}]"));
                } else {
                    graph_parts.push(format!("[{label}]{filters}[wm]"));
                    watermark_label = Some("[wm]".to_owned());
                }
            }
        }

        if let Some(subtitle) = self.inputs.iter().find(|input| {
            input.kind == InputKind::Subtitle
                && input.is_image_based_subtitle
                && input.subtitle_method == Some(SubtitleMethod::Burn)
        }) {
            let input_index = self.input_index(subtitle, false);
            if let Some(stream) = subtitle.streams.first() {
                let label = format!("{input_index}:{}", stream.index);
                let filters = join_filters(
                    subtitle
                        .filter_steps
                        .iter()
                        .chain(filter_chain.subtitle.iter()),
                );
                if filters.is_empty() {
                    subtitle_label = Some(format!("[{label}]"));
                } else {
                    graph_parts.push(format!("[{label}]{filters}[st]"));
                    subtitle_label = Some("[st]".to_owned());
                }
            }
        }

        if let Some(graphics) = self
            .inputs
            .iter()
            .find(|input| input.kind == InputKind::Graphics)
        {
            let input_index = self.input_index(graphics, false);
            let filters = join_filters(
                graphics
                    .filter_steps
                    .iter()
                    .chain(filter_chain.graphics.iter()),
            );
            if filters.is_empty() {
                graphics_label = Some(format!("[{input_index}:0]"));
            } else {
                graph_parts.push(format!("[{input_index}:0]{filters}[ge]"));
                graphics_label = Some("[ge]".to_owned());
            }
        }

        if let Some(label) = subtitle_label {
            let filters = join_filters(filter_chain.subtitle_overlay.iter());
            if !filters.is_empty() {
                graph_parts.push(format!(
                    "{}{}{filters}[vst]",
                    proper_label(&video_label),
                    proper_label(&label)
                ));
                "[vst]".clone_into(&mut video_label);
            }
        }

        if let Some(label) = watermark_label {
            let filters = join_filters(filter_chain.watermark_overlay.iter());
            if !filters.is_empty() {
                graph_parts.push(format!(
                    "{}{}{filters}[vwm]",
                    proper_label(&video_label),
                    proper_label(&label)
                ));
                "[vwm]".clone_into(&mut video_label);
            }
        }

        if let Some(label) = graphics_label {
            let filters = join_filters(filter_chain.graphics_overlay.iter());
            if !filters.is_empty() {
                graph_parts.push(format!(
                    "{}{}{filters}[vge]",
                    proper_label(&video_label),
                    proper_label(&label)
                ));
                "[vge]".clone_into(&mut video_label);
            }
        }

        let pixel_filters = join_filters(filter_chain.pixel_format.iter());
        if !pixel_filters.is_empty() {
            graph_parts.push(format!(
                "{}{pixel_filters}[vpf]",
                proper_label(&video_label)
            ));
            "[vpf]".clone_into(&mut video_label);
        }

        if let Some(audio) = self
            .inputs
            .iter()
            .find(|input| input.kind == InputKind::Audio)
        {
            let input_index = self.input_index(audio, true);
            if let Some(stream) = audio.streams.first() {
                audio_label = format!("{input_index}:{}", stream.index);
                let filters =
                    join_filters(audio.filter_steps.iter().chain(filter_chain.audio.iter()));
                if !filters.is_empty() {
                    graph_parts.push(format!("[{audio_label}]{filters}[a]"));
                    "[a]".clone_into(&mut audio_label);
                }
            }
        }

        let filter_graph = (!graph_parts.is_empty()).then(|| graph_parts.join(";"));
        let mut options = Vec::new();
        if let Some(graph) = &filter_graph {
            options.extend(["-filter_complex".to_owned(), graph.clone()]);
        }
        options.extend([
            "-map".to_owned(),
            video_label.clone(),
            "-map".to_owned(),
            audio_label.clone(),
        ]);

        for subtitle in self.inputs.iter().filter(|input| {
            input.kind == InputKind::Subtitle
                && matches!(
                    input.subtitle_method,
                    Some(SubtitleMethod::Copy | SubtitleMethod::Convert)
                )
        }) {
            let input_index = self.input_index(subtitle, false);
            for stream in &subtitle.streams {
                options.extend(["-map".to_owned(), format!("{input_index}:{}", stream.index)]);
            }
        }

        (options, filter_graph, video_label, audio_label)
    }

    fn input_index(&self, input: &InputFile, last: bool) -> usize {
        if last {
            self.distinct_paths
                .iter()
                .rposition(|path| path == &input.path)
                .unwrap_or_default()
        } else {
            self.distinct_paths
                .iter()
                .position(|path| path == &input.path)
                .unwrap_or_default()
        }
    }
}

fn join_filters<'a>(filters: impl Iterator<Item = &'a String>) -> String {
    filters
        .filter(|filter| !filter.trim().is_empty())
        .cloned()
        .collect::<Vec<_>>()
        .join(",")
}

fn proper_label(label: &str) -> String {
    if label.starts_with('[') {
        label.to_owned()
    } else {
        format!("[{label}]")
    }
}
