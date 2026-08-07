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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubtitleTextFormat {
    Srt,
    Ass,
    WebVtt,
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
    pub subtitle_text_format: Option<SubtitleTextFormat>,
    pub is_image_based_subtitle: bool,
    pub allow_readrate: bool,
    pub seek_after_input: bool,
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
            subtitle_text_format: None,
            is_image_based_subtitle: false,
            allow_readrate: true,
            seek_after_input: false,
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

    /// Still watermark image. Mirrors ErsatzTV's `SetWatermark`: a plain still
    /// image gets no loop option — the overlay's default `eof_action=repeat`
    /// holds the single decoded frame.
    #[must_use]
    pub fn watermark(path: impl Into<String>) -> Self {
        Self::new(InputKind::Watermark, path, 0)
    }

    /// Still watermark image with fade points. Mirrors ErsatzTV's
    /// `InfiniteLoopInputOption` on the watermark input: "looping is required
    /// to fade a static image in and out".
    #[must_use]
    pub fn watermark_looped(path: impl Into<String>) -> Self {
        Self::new(InputKind::Watermark, path, 0).with_input_options(["-loop", "1"])
    }

    /// Animated watermark image (multi-frame GIF/APNG/WebP). Mirrors ErsatzTV's
    /// `DoNotIgnoreLoopInputOption` (`-ignore_loop 0`) so the animation loops
    /// for the stream duration.
    #[must_use]
    pub fn watermark_animated(path: impl Into<String>) -> Self {
        Self::new(InputKind::Watermark, path, 0).with_input_options(["-ignore_loop", "0"])
    }

    /// Graphics-element image input.
    ///
    /// The loop option is demuxer-specific and getting it wrong is fatal, not
    /// cosmetic. Measured against `FFmpeg` 9:
    ///
    /// | input | `-loop 1` | `-ignore_loop 0` |
    /// |---|---|---|
    /// | still image (image2) | works | `Option ignore_loop not found` |
    /// | animated `GIF` / `WebP` | `Option loop not found` | works |
    ///
    /// Either mismatch fails at `Error opening input file`, so a graphics
    /// element pointing at an animated image used to take the whole channel
    /// down. The animation flag is read from the file header rather than
    /// probed, because this builder is synchronous.
    #[must_use]
    pub fn graphics_image(path: impl Into<String>) -> Self {
        let path = path.into();
        let animated = image_file_is_animated(std::path::Path::new(&path));
        Self::graphics_image_with_animation(path, animated)
    }

    /// [`Self::graphics_image`] with the animation decision supplied, for
    /// callers that already know it and for tests.
    #[must_use]
    pub fn graphics_image_with_animation(path: impl Into<String>, animated: bool) -> Self {
        let mut input = Self::new(InputKind::GraphicsImage, path, 0);
        if animated {
            input
                .input_options
                .extend(["-ignore_loop".to_owned(), "0".to_owned()]);
        } else {
            input
                .input_options
                .extend(["-loop".to_owned(), "1".to_owned()]);
        }
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

    #[must_use]
    pub fn with_subtitle_codec(mut self, codec: &str) -> Self {
        self.subtitle_text_format = match codec.trim().to_ascii_lowercase().as_str() {
            "ass" | "ssa" => Some(SubtitleTextFormat::Ass),
            "webvtt" | "vtt" => Some(SubtitleTextFormat::WebVtt),
            "mov_text" | "subrip" | "srt" => Some(SubtitleTextFormat::Srt),
            _ => None,
        };
        self
    }

    #[must_use]
    pub const fn without_readrate(mut self) -> Self {
        self.allow_readrate = false;
        self
    }

    /// Request accurate output-side seeking for non-seekable sources such as
    /// producer pipes. Regular files and URLs keep the faster input-side seek.
    #[must_use]
    pub const fn with_seek_after_input(mut self) -> Self {
        self.seek_after_input = true;
        self
    }
}

/// Whether an image file contains an animation, decided from its header.
///
/// `ffprobe -count_frames` is the authoritative check and is what the watermark
/// path uses, but it is async and costs a process spawn. Graphics elements are
/// built synchronously, and the two formats that matter both announce animation
/// in their first few kilobytes:
///
/// * `WebP` animations are an extended (`VP8X`) RIFF file carrying an `ANIM`
///   chunk;
/// * animated `GIF`s carry the `NETSCAPE2.0` application extension that holds
///   the loop count.
///
/// Anything unreadable or unrecognised is treated as a still image, which is
/// the pre-existing behavior.
#[must_use]
pub fn image_file_is_animated(path: &std::path::Path) -> bool {
    use std::io::Read as _;

    // The markers live in the header. Reading a bounded prefix keeps this cheap
    // and stops a large image from being pulled into memory.
    const HEADER_BYTES: usize = 4_096;

    let Ok(mut file) = std::fs::File::open(path) else {
        return false;
    };
    let mut header = vec![0_u8; HEADER_BYTES];
    let Ok(read) = file.read(&mut header) else {
        return false;
    };
    header.truncate(read);

    contains(&header, b"RIFF") && contains(&header, b"WEBP") && contains(&header, b"ANIM")
        || contains(&header, b"NETSCAPE2.0")
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}
