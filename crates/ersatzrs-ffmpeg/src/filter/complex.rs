use crate::{InputFile, InputKind, SubtitleMethod};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GraphicsOverlayFilter {
    Drawvg(String),
    NativeImage { input_index: usize, filter: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FilterChain {
    pub video: Vec<String>,
    pub audio: Vec<String>,
    pub watermark: Vec<String>,
    pub subtitle: Vec<String>,
    pub graphics: Vec<String>,
    pub watermark_overlay: Vec<String>,
    pub subtitle_overlay: Vec<String>,
    pub graphics_overlay: Vec<GraphicsOverlayFilter>,
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
        for kind in input_kind_order() {
            for input in inputs.iter().filter(|input| input.kind == kind) {
                match input.kind {
                    InputKind::Audio
                        if distinct_paths.contains(&input.path) && is_intel_vaapi_or_qsv =>
                    {
                        distinct_paths.push(input.path.clone());
                    }
                    InputKind::Subtitle if !input.is_image_based_subtitle => {}
                    InputKind::Video | InputKind::Concat | InputKind::Graphics => {
                        distinct_paths.push(input.path.clone());
                    }
                    _ if !distinct_paths.contains(&input.path) => {
                        distinct_paths.push(input.path.clone());
                    }
                    _ => {}
                }
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
        let mut watermark_labels = Vec::<String>::new();
        let mut subtitle_label = None::<String>;
        let mut graphics_image_labels = Vec::<String>::new();

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

        for (index, watermark) in self
            .inputs
            .iter()
            .filter(|input| input.kind == InputKind::Watermark)
            .enumerate()
        {
            let input_index = self.input_index(watermark, false);
            let Some(stream) = watermark.streams.first() else {
                continue;
            };
            let label = format!("{input_index}:{}", stream.index);
            let filters = join_filters(
                watermark
                    .filter_steps
                    .iter()
                    .chain(filter_chain.watermark.iter()),
            );
            if filters.is_empty() {
                watermark_labels.push(format!("[{label}]"));
            } else {
                let output = format!("[wm{index}]");
                graph_parts.push(format!("[{label}]{filters}{output}"));
                watermark_labels.push(output);
            }
        }

        for (index, image) in self
            .inputs
            .iter()
            .filter(|input| input.kind == InputKind::GraphicsImage)
            .enumerate()
        {
            let input_index = self.input_index(image, false);
            let Some(stream) = image.streams.first() else {
                continue;
            };
            let label = format!("{input_index}:{}", stream.index);
            let filters = join_filters(image.filter_steps.iter());
            if filters.is_empty() {
                graphics_image_labels.push(format!("[{label}]"));
            } else {
                let output = format!("[gi{index}]");
                graph_parts.push(format!("[{label}]{filters}{output}"));
                graphics_image_labels.push(output);
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

        for (index, (label, overlay_filter)) in watermark_labels
            .iter()
            .zip(filter_chain.watermark_overlay.iter())
            .enumerate()
        {
            let filters = join_filters(std::iter::once(overlay_filter));
            if !filters.is_empty() {
                let output = format!("[vwm{index}]");
                graph_parts.push(format!(
                    "{}{}{filters}{output}",
                    proper_label(&video_label),
                    proper_label(label)
                ));
                video_label = output;
            }
        }

        for (index, overlay) in filter_chain.graphics_overlay.iter().enumerate() {
            match overlay {
                GraphicsOverlayFilter::Drawvg(filter) => {
                    let filters = join_filters(std::iter::once(filter));
                    if !filters.is_empty() {
                        let output = format!("[vge{index}]");
                        graph_parts
                            .push(format!("{}{filters}{output}", proper_label(&video_label)));
                        video_label = output;
                    }
                }
                GraphicsOverlayFilter::NativeImage {
                    input_index,
                    filter,
                } => {
                    let Some(label) = graphics_image_labels.get(*input_index) else {
                        continue;
                    };
                    let filters = join_filters(std::iter::once(filter));
                    if !filters.is_empty() {
                        let output = format!("[vge{index}]");
                        graph_parts.push(format!(
                            "{}{}{filters}{output}",
                            proper_label(&video_label),
                            proper_label(label)
                        ));
                        video_label = output;
                    }
                }
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

fn input_kind_order() -> [InputKind; 7] {
    [
        InputKind::Video,
        InputKind::Audio,
        InputKind::Watermark,
        InputKind::GraphicsImage,
        InputKind::Concat,
        InputKind::Graphics,
        InputKind::Subtitle,
    ]
}
