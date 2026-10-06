use super::escaping::{escape_filtergraph_option, escape_filtergraph_path};
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};

/// Text input consumed by `FFmpeg`'s `drawtext` filter.
///
/// Keeping inline text and file-backed text in one enum prevents callers from
/// emitting the mutually exclusive `text` and `textfile` options together.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DrawtextTextSource {
    /// Text embedded directly in the filtergraph.
    Inline(String),
    /// UTF-8 text loaded from a file, optionally reloaded every N frames.
    TextFile {
        /// Path passed to `FFmpeg`'s `textfile` option.
        path: PathBuf,
        /// Positive frame interval passed to `FFmpeg`'s `reload` option.
        reload: Option<NonZeroU32>,
    },
}

impl DrawtextTextSource {
    /// Creates an inline text source.
    #[must_use]
    pub fn inline(text: impl Into<String>) -> Self {
        Self::Inline(text.into())
    }

    /// Creates a file-backed text source with an optional positive reload
    /// interval measured in frames.
    #[must_use]
    pub fn text_file(path: impl Into<PathBuf>, reload: Option<NonZeroU32>) -> Self {
        Self::TextFile {
            path: path.into(),
            reload,
        }
    }
}

/// Font selector consumed by `FFmpeg`'s `drawtext` filter.
///
/// `FFmpeg` 9 exposes `font` and `fontfile`; it does not expose a `fontsdir`
/// option for this filter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DrawtextFont {
    /// Fontconfig family name passed through `font`.
    Family(String),
    /// Concrete font path passed through `fontfile`.
    File(PathBuf),
}

impl DrawtextFont {
    /// Selects a font by Fontconfig family name.
    #[must_use]
    pub fn family(name: impl Into<String>) -> Self {
        Self::Family(name.into())
    }

    /// Selects a concrete font file.
    #[must_use]
    pub fn file(path: impl Into<PathBuf>) -> Self {
        Self::File(path.into())
    }
}

/// Supported non-deprecated `FFmpeg` 9 text expansion modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DrawtextExpansion {
    /// Render text literally without `%{...}` expansion.
    None,
    /// Expand FFmpeg-native `%{...}` expressions.
    #[default]
    Normal,
}

impl DrawtextExpansion {
    const fn ffmpeg_name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Normal => "normal",
        }
    }
}

/// Horizontal alignment within the configured text box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrawtextHorizontalAlignment {
    /// Align text to the left edge.
    Left,
    /// Center text horizontally.
    Center,
    /// Align text to the right edge.
    Right,
}

impl DrawtextHorizontalAlignment {
    const fn ffmpeg_flag(self) -> char {
        match self {
            Self::Left => 'L',
            Self::Center => 'C',
            Self::Right => 'R',
        }
    }
}

/// Vertical alignment within the configured text box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrawtextVerticalAlignment {
    /// Align text to the top edge.
    Top,
    /// Center text vertically.
    Middle,
    /// Align text to the bottom edge.
    Bottom,
}

impl DrawtextVerticalAlignment {
    const fn ffmpeg_flag(self) -> char {
        match self {
            Self::Top => 'T',
            Self::Middle => 'M',
            Self::Bottom => 'B',
        }
    }
}

/// Typed horizontal and vertical `text_align` flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DrawtextAlignment {
    horizontal: DrawtextHorizontalAlignment,
    vertical: DrawtextVerticalAlignment,
}

impl DrawtextAlignment {
    /// Creates a complete horizontal and vertical alignment pair.
    #[must_use]
    pub const fn new(
        horizontal: DrawtextHorizontalAlignment,
        vertical: DrawtextVerticalAlignment,
    ) -> Self {
        Self {
            horizontal,
            vertical,
        }
    }

    fn ffmpeg_flags(self) -> String {
        format!(
            "{}+{}",
            self.horizontal.ffmpeg_flag(),
            self.vertical.ffmpeg_flag()
        )
    }
}

/// Reference point used by `FFmpeg` for the `y` expression.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrawtextYAlignment {
    /// Refer `y` to the top of the first text line.
    Text,
    /// Refer `y` to the first line's baseline.
    Baseline,
    /// Refer `y` to the selected font's line metrics.
    Font,
}

impl DrawtextYAlignment {
    const fn ffmpeg_name(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Baseline => "baseline",
            Self::Font => "font",
        }
    }
}

/// Typed builder for an `FFmpeg` 9 `drawtext` filter.
///
/// String-valued options are treated as raw `FFmpeg` values and escaped once
/// for the option parser and once for the containing filtergraph. Paths are
/// additionally normalized to forward slashes before escaping, including
/// Windows drive-letter paths.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DrawtextFilterBuilder {
    text: DrawtextTextSource,
    font: DrawtextFont,
    expansion: DrawtextExpansion,
    font_size: Option<String>,
    position: Option<(String, String)>,
    font_color: Option<String>,
    alpha: Option<String>,
    alignment: Option<DrawtextAlignment>,
    y_alignment: Option<DrawtextYAlignment>,
    line_spacing: Option<i32>,
    tab_size: Option<u32>,
    border: Option<(u32, String)>,
    shadow: Option<(i32, i32, String)>,
    box_style: Option<(String, String)>,
    box_dimensions: Option<(u32, u32)>,
    text_shaping: Option<bool>,
    fix_bounds: Option<bool>,
    enable: Option<String>,
}

impl DrawtextFilterBuilder {
    /// Creates a builder with one text source and one font selector.
    #[must_use]
    pub fn new(text: DrawtextTextSource, font: DrawtextFont) -> Self {
        Self {
            text,
            font,
            expansion: DrawtextExpansion::default(),
            font_size: None,
            position: None,
            font_color: None,
            alpha: None,
            alignment: None,
            y_alignment: None,
            line_spacing: None,
            tab_size: None,
            border: None,
            shadow: None,
            box_style: None,
            box_dimensions: None,
            text_shaping: None,
            fix_bounds: None,
            enable: None,
        }
    }

    /// Sets the `FFmpeg` text expansion mode.
    #[must_use]
    pub const fn expansion(mut self, expansion: DrawtextExpansion) -> Self {
        self.expansion = expansion;
        self
    }

    /// Sets the `fontsize` expression.
    #[must_use]
    pub fn font_size(mut self, expression: impl Into<String>) -> Self {
        self.font_size = Some(expression.into());
        self
    }

    /// Sets the `x` and `y` expressions.
    #[must_use]
    pub fn position(mut self, x: impl Into<String>, y: impl Into<String>) -> Self {
        self.position = Some((x.into(), y.into()));
        self
    }

    /// Sets the foreground colour.
    #[must_use]
    pub fn font_color(mut self, color: impl Into<String>) -> Self {
        self.font_color = Some(color.into());
        self
    }

    /// Sets the alpha expression.
    #[must_use]
    pub fn alpha(mut self, expression: impl Into<String>) -> Self {
        self.alpha = Some(expression.into());
        self
    }

    /// Sets horizontal and vertical alignment within the text box.
    #[must_use]
    pub const fn alignment(mut self, alignment: DrawtextAlignment) -> Self {
        self.alignment = Some(alignment);
        self
    }

    /// Sets what the `y` expression is measured from.
    #[must_use]
    pub const fn y_alignment(mut self, alignment: DrawtextYAlignment) -> Self {
        self.y_alignment = Some(alignment);
        self
    }

    /// Sets line spacing in pixels.
    #[must_use]
    pub const fn line_spacing(mut self, pixels: i32) -> Self {
        self.line_spacing = Some(pixels);
        self
    }

    /// Sets tab width in spaces.
    #[must_use]
    pub const fn tab_size(mut self, spaces: u32) -> Self {
        self.tab_size = Some(spaces);
        self
    }

    /// Enables a text border with the supplied pixel width and colour.
    #[must_use]
    pub fn border(mut self, width: u32, color: impl Into<String>) -> Self {
        self.border = Some((width, color.into()));
        self
    }

    /// Enables a shadow with pixel offsets and colour.
    #[must_use]
    pub fn shadow(mut self, x: i32, y: i32, color: impl Into<String>) -> Self {
        self.shadow = Some((x, y, color.into()));
        self
    }

    /// Enables a background box with a colour and `FFmpeg` `boxborderw` value.
    #[must_use]
    pub fn box_style(mut self, color: impl Into<String>, border_widths: impl Into<String>) -> Self {
        self.box_style = Some((color.into(), border_widths.into()));
        self
    }

    /// Sets explicit box dimensions in pixels.
    #[must_use]
    pub const fn box_dimensions(mut self, width: u32, height: u32) -> Self {
        self.box_dimensions = Some((width, height));
        self
    }

    /// Enables or disables complex text shaping explicitly.
    #[must_use]
    pub const fn text_shaping(mut self, enabled: bool) -> Self {
        self.text_shaping = Some(enabled);
        self
    }

    /// Enables or disables `FFmpeg`'s clipping-bound correction explicitly.
    #[must_use]
    pub const fn fix_bounds(mut self, enabled: bool) -> Self {
        self.fix_bounds = Some(enabled);
        self
    }

    /// Sets the timeline `enable` expression.
    #[must_use]
    pub fn enable(mut self, expression: impl Into<String>) -> Self {
        self.enable = Some(expression.into());
        self
    }

    /// Builds the complete `drawtext=...` filtergraph fragment.
    #[must_use]
    pub fn build(&self) -> String {
        let mut options = Vec::new();
        match &self.font {
            DrawtextFont::Family(family) => push_value(&mut options, "font", family),
            DrawtextFont::File(path) => push_path(&mut options, "fontfile", path),
        }
        match &self.text {
            DrawtextTextSource::Inline(text) => push_value(&mut options, "text", text),
            DrawtextTextSource::TextFile { path, reload } => {
                push_path(&mut options, "textfile", path);
                if let Some(reload) = reload {
                    options.push(format!("reload={reload}"));
                }
            }
        }
        options.push(format!("expansion={}", self.expansion.ffmpeg_name()));
        push_optional_value(&mut options, "fontsize", self.font_size.as_deref());
        if let Some((x, y)) = &self.position {
            push_value(&mut options, "x", x);
            push_value(&mut options, "y", y);
        }
        push_optional_value(&mut options, "fontcolor", self.font_color.as_deref());
        push_optional_value(&mut options, "alpha", self.alpha.as_deref());
        if let Some(alignment) = self.alignment {
            options.push(format!("text_align={}", alignment.ffmpeg_flags()));
        }
        if let Some(alignment) = self.y_alignment {
            options.push(format!("y_align={}", alignment.ffmpeg_name()));
        }
        if let Some(line_spacing) = self.line_spacing {
            options.push(format!("line_spacing={line_spacing}"));
        }
        if let Some(tab_size) = self.tab_size {
            options.push(format!("tabsize={tab_size}"));
        }
        if let Some((width, color)) = &self.border {
            options.push(format!("borderw={width}"));
            push_value(&mut options, "bordercolor", color);
        }
        if let Some((x, y, color)) = &self.shadow {
            options.push(format!("shadowx={x}"));
            options.push(format!("shadowy={y}"));
            push_value(&mut options, "shadowcolor", color);
        }
        if let Some((color, border_widths)) = &self.box_style {
            options.push("box=1".to_owned());
            push_value(&mut options, "boxcolor", color);
            push_value(&mut options, "boxborderw", border_widths);
        }
        if let Some((width, height)) = self.box_dimensions {
            options.push(format!("boxw={width}"));
            options.push(format!("boxh={height}"));
        }
        if let Some(enabled) = self.text_shaping {
            options.push(format!("text_shaping={}", bool_flag(enabled)));
        }
        if let Some(enabled) = self.fix_bounds {
            options.push(format!("fix_bounds={}", bool_flag(enabled)));
        }
        push_optional_value(&mut options, "enable", self.enable.as_deref());
        format!("drawtext={}", options.join(":"))
    }
}

const fn bool_flag(value: bool) -> u8 {
    if value {
        1
    } else {
        0
    }
}

fn push_optional_value(options: &mut Vec<String>, name: &str, value: Option<&str>) {
    if let Some(value) = value {
        push_value(options, name, value);
    }
}

fn push_path(options: &mut Vec<String>, name: &str, path: &Path) {
    options.push(format!("{name}={}", escape_filtergraph_path(path)));
}

fn push_value(options: &mut Vec<String>, name: &str, value: &str) {
    options.push(format!("{name}={}", escape_filtergraph_option(value)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_inline_text_through_option_and_filtergraph_layers() {
        let filter = DrawtextFilterBuilder::new(
            DrawtextTextSource::inline("this is a 'string': one, [two]; C:\\temp"),
            DrawtextFont::family("Inter:style=Bold"),
        )
        .expansion(DrawtextExpansion::None)
        .position("if(gt(t,1),10,20)", "h-text_h-24")
        .enable("between(t,0,8)")
        .build();

        assert_eq!(
            filter,
            r"drawtext=font=Inter\\:style=Bold:text=this is a \\\'string\\\'\\: one\, \[two\]\; C\\:\\\\temp:expansion=none:x=if(gt(t\,1)\,10\,20):y=h-text_h-24:enable=between(t\,0\,8)"
        );
    }

    #[test]
    fn normalizes_textfile_and_fontfile_paths_and_emits_reload_interval() {
        let filter = DrawtextFilterBuilder::new(
            DrawtextTextSource::text_file(r"X:\overlay files\current.txt", NonZeroU32::new(30)),
            DrawtextFont::file(r"X:\fonts\Roboto Regular.ttf"),
        )
        .font_size("48")
        .text_shaping(true)
        .build();

        assert_eq!(
            filter,
            r"drawtext=fontfile=X\\:/fonts/Roboto Regular.ttf:textfile=X\\:/overlay files/current.txt:reload=30:expansion=normal:fontsize=48:text_shaping=1"
        );
    }
}
