#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputKind {
    Video,
    Audio,
    Watermark,
    GraphicsImage,
    Subtitle,
    Concat,
    Graphics,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubtitleMethod {
    Burn,
    Copy,
    Convert,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaStream {
    pub index: usize,
}

impl MediaStream {
    #[must_use]
    pub const fn new(index: usize) -> Self {
        Self { index }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputFile {
    pub kind: InputKind,
    pub path: String,
    pub streams: Vec<MediaStream>,
    pub input_options: Vec<String>,
    pub filter_steps: Vec<String>,
    pub subtitle_method: Option<SubtitleMethod>,
    pub is_image_based_subtitle: bool,
}

impl InputFile {
    #[must_use]
    pub fn new(kind: InputKind, path: impl Into<String>, stream_index: usize) -> Self {
        Self {
            kind,
            path: path.into(),
            streams: vec![MediaStream::new(stream_index)],
            input_options: Vec::new(),
            filter_steps: Vec::new(),
            subtitle_method: None,
            is_image_based_subtitle: false,
        }
    }

    #[must_use]
    pub fn video(path: impl Into<String>) -> Self {
        Self::new(InputKind::Video, path, 0)
    }

    #[must_use]
    pub fn audio(path: impl Into<String>) -> Self {
        Self::new(InputKind::Audio, path, 1)
    }

    #[must_use]
    pub fn lavfi_video(filter: impl Into<String>) -> Self {
        Self::new(InputKind::Video, filter, 0).with_input_options(["-f", "lavfi"])
    }

    #[must_use]
    pub fn lavfi_audio(filter: impl Into<String>) -> Self {
        Self::new(InputKind::Audio, filter, 0).with_input_options(["-f", "lavfi"])
    }

    #[must_use]
    pub fn watermark(path: impl Into<String>) -> Self {
        let mut input = Self::new(InputKind::Watermark, path, 0);
        input
            .input_options
            .extend(["-loop".to_owned(), "1".to_owned()]);
        input
    }

    #[must_use]
    pub fn graphics_image(path: impl Into<String>) -> Self {
        let mut input = Self::new(InputKind::GraphicsImage, path, 0);
        input
            .input_options
            .extend(["-loop".to_owned(), "1".to_owned()]);
        input
    }

    #[must_use]
    pub fn graphics_motion(path: impl Into<String>, loop_input: bool) -> Self {
        let mut input = Self::new(InputKind::GraphicsImage, path, 0);
        if loop_input {
            input
                .input_options
                .extend(["-stream_loop".to_owned(), "-1".to_owned()]);
        }
        input
    }

    #[must_use]
    pub fn graphics_subtitle(path: impl Into<String>) -> Self {
        Self::new(InputKind::GraphicsImage, path, 0).with_input_options(["-f", "lavfi"])
    }

    #[must_use]
    pub fn concat(path: impl Into<String>) -> Self {
        let mut input = Self::new(InputKind::Concat, path, 0);
        input.input_options.extend([
            "-f".to_owned(),
            "concat".to_owned(),
            "-safe".to_owned(),
            "0".to_owned(),
            "-protocol_whitelist".to_owned(),
            "file,http,tcp,https,tcp,tls".to_owned(),
            "-probesize".to_owned(),
            "32".to_owned(),
        ]);
        input
    }

    #[must_use]
    pub fn graphics(path: impl Into<String>) -> Self {
        Self::new(InputKind::Graphics, path, 0)
    }

    #[must_use]
    pub fn image_subtitle(path: impl Into<String>, method: SubtitleMethod) -> Self {
        let mut input = Self::new(InputKind::Subtitle, path, 0);
        input.subtitle_method = Some(method);
        input.is_image_based_subtitle = true;
        input
    }

    #[must_use]
    pub fn text_subtitle(path: impl Into<String>, method: SubtitleMethod) -> Self {
        let mut input = Self::new(InputKind::Subtitle, path, 0);
        input.subtitle_method = Some(method);
        input
    }

    #[must_use]
    pub fn with_input_options(
        mut self,
        options: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.input_options
            .extend(options.into_iter().map(Into::into));
        self
    }

    pub fn add_readrate(&mut self, readrate: &str) {
        self.input_options
            .extend(["-readrate".to_owned(), readrate.to_owned()]);
    }

    #[must_use]
    pub fn with_filter(mut self, filter: impl Into<String>) -> Self {
        self.filter_steps.push(filter.into());
        self
    }

    #[must_use]
    pub fn with_stream_index(mut self, index: usize) -> Self {
        self.streams = vec![MediaStream::new(index)];
        self
    }
}
