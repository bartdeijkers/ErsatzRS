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
    /// Inputs normally share one decoder by path. A distinct instance keeps
    /// canvas layers with independent seek positions on separate decoders.
    pub input_instance: usize,
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
            input_instance: 0,
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

    pub(crate) fn source_key(&self) -> (&str, usize) {
        (&self.path, self.input_instance)
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
        if image_file_is_jpeg_xl(std::path::Path::new(&path)) {
            // JPEG XL's demuxer has no image2 loop option. The render context
            // supplies decoded timing and a finite concat input for animations.
            return Self::new(InputKind::GraphicsImage, path, 0);
        }
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
        if loop_input && !image_file_is_jpeg_xl(std::path::Path::new(&input.path)) {
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

/// Identify JPEG XL by its codestream/container signature, or its extension
/// when the source has not yet been opened. Animation timing is decoded by
/// the bounded infrastructure probe rather than guessed from codec support.
#[must_use]
pub fn image_file_is_jpeg_xl(path: &std::path::Path) -> bool {
    use std::io::Read as _;
    let mut signature = [0_u8; 12];
    if let Ok(mut file) = std::fs::File::open(path) {
        if let Ok(count) = file.read(&mut signature) {
            if count >= 2 && signature[..2] == [0xff, 0x0a] {
                return true;
            }
            if count == 12 && signature == *b"\x00\x00\x00\x0cJXL \x0d\x0a\x87\x0a" {
                return true;
            }
        }
    }
    path.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("jxl"))
        && !is_image_sequence_path(&path.to_string_lossy())
}

fn is_image_sequence_path(path: &str) -> bool {
    let mut characters = path.chars().peekable();
    while let Some(character) = characters.next() {
        if character != '%' {
            continue;
        }
        if characters.peek() == Some(&'%') {
            characters.next();
            continue;
        }
        while characters.peek().is_some_and(char::is_ascii_digit) {
            characters.next();
        }
        if characters.peek() == Some(&'d') {
            return true;
        }
    }
    false
}

/// Whether an image file contains an animation.
///
/// `ffprobe -count_frames` is the authoritative check and is what the watermark
/// path uses, but it is async and costs a process spawn. Graphics elements are
/// built synchronously, so the container structure is parsed directly instead.
///
/// The answer matters: the loop option is demuxer-specific and a mismatch is
/// fatal. `-loop 1` on an animated `GIF`/`WebP` fails with
/// `Option loop not found` and the input never opens, taking the channel down.
///
/// Reading is bounded and fails closed — anything unreadable, truncated,
/// malformed, or unrecognised is treated as a still image, which is the
/// pre-existing behavior and the safe direction: a still misclassified as
/// animated would break, a genuinely animated file misclassified as still only
/// fails to loop.
#[must_use]
pub fn image_file_is_animated(path: &std::path::Path) -> bool {
    let Ok(mut file) = std::fs::File::open(path) else {
        return false;
    };
    let mut header = [0_u8; 16];
    if std::io::Read::read_exact(&mut file, &mut header).is_err() {
        return false;
    }

    if header.starts_with(b"GIF87a") || header.starts_with(b"GIF89a") {
        return gif_is_animated(&mut file, &header);
    }
    if header.starts_with(b"RIFF") && header[8..12] == *b"WEBP" {
        return webp_is_animated(&mut file, &header);
    }
    false
}

/// Total bytes any single reader will pull from a file.
///
/// Generous enough for a large colour table plus many small frames, small
/// enough that a hostile or corrupt file cannot be used to read a whole disk
/// into memory. Hitting it means "stop looking", not "animated".
const MAX_STRUCTURE_BYTES: u64 = 1 << 20;

/// Count `GIF` image descriptors, stopping at the second.
///
/// Deliberately does not rely on the `NETSCAPE2.0` application extension: that
/// carries the loop count, and a valid multi-frame `GIF` need not have one, so
/// keying on it under-detects. Extension blocks are skipped through their
/// sub-block chain, which is the part a naive scan gets wrong — sub-block data
/// can contain any byte, including the `0x2C` image separator.
fn gif_is_animated(file: &mut std::fs::File, header: &[u8; 16]) -> bool {
    use std::io::{Read as _, Seek as _, SeekFrom};

    // Logical screen descriptor is bytes 6..13; bit 7 of the packed field marks
    // a global colour table whose size is 3 * 2^(N+1).
    let packed = header[10];
    let mut position = 13_u64;
    if packed & 0x80 != 0 {
        position += 3 * (1_u64 << ((packed & 0x07) + 1));
    }
    if file.seek(SeekFrom::Start(position)).is_err() {
        return false;
    }

    let mut reader = std::io::BufReader::new(file.take(MAX_STRUCTURE_BYTES));
    let mut frames = 0_u32;
    loop {
        let mut marker = [0_u8; 1];
        if reader.read_exact(&mut marker).is_err() {
            return false; // truncated: fail closed
        }
        match marker[0] {
            // Image descriptor: 9 bytes, then an optional local colour table,
            // then LZW-compressed sub-blocks.
            0x2C => {
                frames += 1;
                if frames > 1 {
                    return true;
                }
                let mut descriptor = [0_u8; 9];
                if reader.read_exact(&mut descriptor).is_err() {
                    return false;
                }
                let local = descriptor[8];
                if local & 0x80 != 0 {
                    let table = 3 * (1_usize << ((local & 0x07) + 1));
                    if skip_exact(&mut reader, table).is_err() {
                        return false;
                    }
                }
                // LZW minimum code size, then the sub-block chain.
                if skip_exact(&mut reader, 1).is_err() || skip_sub_blocks(&mut reader).is_err() {
                    return false;
                }
            }
            // Extension: label byte, then a sub-block chain.
            0x21 => {
                if skip_exact(&mut reader, 1).is_err() || skip_sub_blocks(&mut reader).is_err() {
                    return false;
                }
            }
            // Trailer, or anything else — a byte that is neither an image
            // descriptor nor an extension means the structure is not what it
            // claims, and a trailer means we reached the end having seen at
            // most one frame. Both are "not animated".
            _ => return false,
        }
    }
}

/// Skip a `GIF` sub-block chain: length-prefixed runs ending with a zero length.
fn skip_sub_blocks(reader: &mut impl std::io::Read) -> std::io::Result<()> {
    loop {
        let mut length = [0_u8; 1];
        reader.read_exact(&mut length)?;
        if length[0] == 0 {
            return Ok(());
        }
        skip_exact(reader, length[0] as usize)?;
    }
}

fn skip_exact(reader: &mut impl std::io::Read, count: usize) -> std::io::Result<()> {
    // Read in bounded chunks rather than allocating `count` bytes: a corrupt
    // file can declare a chunk size far larger than the file itself.
    let mut remaining = count;
    let mut scratch = [0_u8; 4096];
    while remaining > 0 {
        let want = remaining.min(scratch.len());
        reader.read_exact(&mut scratch[..want])?;
        remaining -= want;
    }
    Ok(())
}

/// Walk `RIFF`/`WebP` chunks looking for the animation flag or an `ANIM` chunk.
///
/// Chunk-accurate rather than a byte scan: each chunk declares its size and is
/// padded to an even boundary, so `ANIM` can legitimately sit beyond any fixed
/// prefix when a file carries large `EXIF`, `XMP`, or `ICCP` metadata first.
/// The `VP8X` extended-format chunk also states animation in a flag bit, which
/// is authoritative and usually appears first.
fn webp_is_animated(file: &mut std::fs::File, header: &[u8; 16]) -> bool {
    use std::io::{Read as _, Seek as _, SeekFrom};

    // A plain `VP8 ` or `VP8L` file is a single still image by definition.
    if header[12..16] == *b"VP8 " || header[12..16] == *b"VP8L" {
        return false;
    }
    if file.seek(SeekFrom::Start(12)).is_err() {
        return false;
    }

    let mut reader = std::io::BufReader::new(file.take(MAX_STRUCTURE_BYTES));
    loop {
        let mut chunk = [0_u8; 8];
        if reader.read_exact(&mut chunk).is_err() {
            return false; // truncated or no more chunks
        }
        let fourcc = &chunk[0..4];
        let size = u32::from_le_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]);

        if fourcc == b"VP8X" {
            let mut flags = [0_u8; 1];
            if reader.read_exact(&mut flags).is_err() {
                return false;
            }
            // Bit 1 of the VP8X flags is the animation flag.
            if flags[0] & 0x02 != 0 {
                return true;
            }
            if skip_exact(&mut reader, size.saturating_sub(1) as usize).is_err() {
                return false;
            }
        } else if fourcc == b"ANIM" || fourcc == b"ANMF" {
            return true;
        } else if skip_exact(&mut reader, size as usize).is_err() {
            return false;
        }

        // Chunks are padded to an even length.
        if size % 2 == 1 && skip_exact(&mut reader, 1).is_err() {
            return false;
        }
    }
}
