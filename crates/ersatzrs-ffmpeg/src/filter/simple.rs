use std::fmt::Write as _;
use std::time::Duration;

use ersatzrs_domain::{WatermarkLocation, WatermarkSize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameSize {
    pub width: u32,
    pub height: u32,
}

impl FrameSize {
    #[must_use]
    pub const fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelFormat {
    Yuv420P,
    Yuv420P10Le,
    Nv12,
    Vaapi,
    Cuda,
}

impl PixelFormat {
    #[must_use]
    pub const fn ffmpeg_name(self) -> &'static str {
        match self {
            Self::Yuv420P => "yuv420p",
            Self::Yuv420P10Le => "yuv420p10le",
            Self::Nv12 => "nv12",
            Self::Vaapi => "vaapi",
            Self::Cuda => "cuda",
        }
    }

    /// Parse an FFmpeg/ffprobe pixel-format name into the enum, mirroring the
    /// `ErsatzTV` `PixelFormat` lookup. Returns `None` for formats outside the
    /// set the pipeline reasons about (callers then leave the current state
    /// unchanged). `yuvj420p` is folded into `yuv420p` exactly as ErsatzTV's
    /// `PixelFormatYuvJ420P` does.
    #[must_use]
    pub fn from_ffmpeg_name(name: &str) -> Option<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "yuv420p" | "yuvj420p" => Some(Self::Yuv420P),
            "yuv420p10le" | "yuv420p10" => Some(Self::Yuv420P10Le),
            "nv12" => Some(Self::Nv12),
            "vaapi" => Some(Self::Vaapi),
            "cuda" => Some(Self::Cuda),
            _ => None,
        }
    }

    #[must_use]
    pub const fn overlay_format(self) -> u8 {
        match self {
            Self::Yuv420P10Le => 1,
            Self::Yuv420P | Self::Nv12 | Self::Vaapi | Self::Cuda => 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameRate {
    pub numerator: u32,
    pub denominator: u32,
}

impl FrameRate {
    pub const DEFAULT: Self = Self {
        numerator: 30_000,
        denominator: 1_001,
    };

    #[must_use]
    pub const fn new(numerator: u32, denominator: u32) -> Self {
        Self {
            numerator,
            denominator,
        }
    }

    #[must_use]
    pub fn parsed(self) -> f64 {
        f64::from(self.numerator) / f64::from(self.denominator)
    }

    #[must_use]
    pub fn r_frame_rate(self) -> String {
        format!("{}/{}", self.numerator, self.denominator)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameDataLocation {
    Software,
    Hardware,
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameState {
    pub scaled_size: FrameSize,
    pub padded_size: FrameSize,
    pub cropped_size: Option<FrameSize>,
    pub pixel_format: PixelFormat,
    pub frame_data_location: FrameDataLocation,
    pub is_anamorphic: bool,
    pub deinterlaced: bool,
    pub frame_rate: Option<FrameRate>,
    pub is_hdr: bool,
    pub colors_are_bt709: bool,
    pub should_deinterlace: bool,
}

impl FrameState {
    #[must_use]
    pub const fn new(size: FrameSize) -> Self {
        Self {
            scaled_size: size,
            padded_size: size,
            cropped_size: None,
            pixel_format: PixelFormat::Yuv420P,
            frame_data_location: FrameDataLocation::Software,
            is_anamorphic: false,
            deinterlaced: false,
            frame_rate: None,
            is_hdr: false,
            colors_are_bt709: false,
            should_deinterlace: false,
        }
    }
}

#[must_use]
pub fn scale_filter(
    current: &FrameState,
    scaled_size: FrameSize,
    padded_size: FrameSize,
) -> String {
    if current.scaled_size == scaled_size {
        return String::new();
    }

    let aspect_ratio = if scaled_size == padded_size {
        ""
    } else if current.cropped_size.is_some() {
        ":force_original_aspect_ratio=increase"
    } else {
        ":force_original_aspect_ratio=decrease"
    };

    let scale = if current.is_anamorphic {
        format!(
            "scale=iw*sar:ih,setsar=1,scale={}:{}:flags=fast_bilinear{}",
            padded_size.width, padded_size.height, aspect_ratio
        )
    } else {
        format!(
            "scale={}:{}:flags=fast_bilinear{},setsar=1",
            padded_size.width, padded_size.height, aspect_ratio
        )
    };

    with_hwdownload(current, &scale)
}

#[must_use]
pub fn pad_filter(current: &FrameState, padded_size: FrameSize) -> String {
    with_hwdownload(
        current,
        &format!(
            "pad={}:{}:-1:-1:color=black",
            padded_size.width, padded_size.height
        ),
    )
}

#[must_use]
pub fn crop_filter(current: &FrameState, cropped_size: FrameSize) -> String {
    with_hwdownload(
        current,
        &format!("crop=w={}:h={}", cropped_size.width, cropped_size.height),
    )
}

#[must_use]
pub fn reset_pts_filter(frame_rate: FrameRate) -> String {
    format!("setpts=PTS-STARTPTS,fps={}", frame_rate.r_frame_rate())
}

#[must_use]
pub fn subtitle_overlay_filter(output_pixel_format: PixelFormat) -> String {
    format!(
        "overlay=x=(W-w)/2:y=(H-h)/2:format={}",
        output_pixel_format.overlay_format()
    )
}

#[must_use]
pub fn subtitle_overlay_cuda_filter() -> String {
    "overlay_cuda=x=(W-w)/2:y=(H-h)/2".to_owned()
}

#[must_use]
pub fn subtitle_overlay_vaapi_filter() -> String {
    "overlay_vaapi".to_owned()
}

#[must_use]
pub fn vaapi_subtitle_pixel_format_filter() -> String {
    "format=vaapi|yuva420p|yuva444p|yuva422p|rgba|abgr|bgra|gbrap|ya8".to_owned()
}

#[must_use]
pub fn scale_subtitle_image_filter(size: FrameSize) -> String {
    format!(
        "scale={}:{}:force_original_aspect_ratio=decrease,pad=w={}:h={}:x=-1:y=-1:color=black@0",
        size.width, size.height, size.width, size.height
    )
}

#[must_use]
pub fn subtitle_scale_npp_filter(size: FrameSize) -> String {
    format!(
        "scale_npp={}:{}:force_original_aspect_ratio=1",
        size.width, size.height
    )
}

#[must_use]
pub fn subtitles_filter(path: &str, fonts_dir: Option<&str>) -> String {
    let effective_file = escape_subtitles_filter_path(path);
    match fonts_dir.filter(|value| !value.trim().is_empty()) {
        Some(fonts_dir) => format!(
            "subtitles={effective_file}:fontsdir={}",
            escape_subtitles_filter_path(fonts_dir)
        ),
        None => format!("subtitles={effective_file}"),
    }
}

#[must_use]
pub fn subtitle_graphics_lavfi_input(
    path: &str,
    size: FrameSize,
    fonts_dir: Option<&str>,
) -> String {
    let mut subtitles = subtitles_filter(path, fonts_dir);
    subtitles.push_str(":alpha=1");
    format!(
        "color=c=black@0.0:s={}x{},format=rgba,{subtitles}",
        size.width, size.height
    )
}

fn escape_subtitles_filter_path(path: &str) -> String {
    let mut effective = path.replace('\\', "/");
    if looks_like_windows_absolute_path(&effective) {
        // The drive colon passes through two ffmpeg parsers: the filtergraph
        // parser consumes one backslash, and the filter option parser needs
        // the remaining `\:` to keep the colon out of option splitting.
        effective.replace_range(1..2, r"\\:");
    }
    escape_subtitles_filter_common(&effective)
}

fn looks_like_windows_absolute_path(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && bytes[2] == b'/'
}

fn escape_subtitles_filter_common(path: &str) -> String {
    path.replace('[', r"\[")
        .replace(']', r"\]")
        .replace("http://localhost:", r"http\\://localhost\\:")
}

#[derive(Debug, Clone, PartialEq)]
pub struct WatermarkOverlay {
    pub location: WatermarkLocation,
    pub size: WatermarkSize,
    pub width_percent: f64,
    pub horizontal_margin_percent: f64,
    pub vertical_margin_percent: f64,
    pub opacity: u8,
    pub place_within_source_content: bool,
    pub fade_points: Vec<WatermarkFadePoint>,
    pub opacity_expression: Option<String>,
    pub enable: Option<String>,
    pub eof_action_pass: bool,
    pub z_index: i32,
}

impl Default for WatermarkOverlay {
    fn default() -> Self {
        Self {
            location: WatermarkLocation::BottomRight,
            size: WatermarkSize::Scaled,
            width_percent: 10.0,
            horizontal_margin_percent: 2.0,
            vertical_margin_percent: 2.0,
            opacity: 100,
            place_within_source_content: false,
            fade_points: Vec::new(),
            opacity_expression: None,
            enable: None,
            eof_action_pass: false,
            z_index: 0,
        }
    }
}

#[must_use]
pub fn watermark_scale_filter(watermark: &WatermarkOverlay, resolution: FrameSize) -> String {
    let width = watermark.width_percent / 100.0 * f64::from(resolution.width);
    format!("scale={}:-1", rounded_number(width))
}

#[must_use]
pub fn watermark_overlay_filter(
    watermark: &WatermarkOverlay,
    resolution: FrameSize,
    square_pixel_frame_size: FrameSize,
    output_pixel_format: PixelFormat,
) -> String {
    let position = watermark_overlay_position(watermark, resolution, square_pixel_frame_size);
    let mut filter = format!(
        "overlay={position}:format={}",
        output_pixel_format.overlay_format()
    );
    append_overlay_timing_options(&mut filter, watermark);
    filter
}

#[must_use]
pub fn watermark_overlay_cuda_filter(
    watermark: &WatermarkOverlay,
    resolution: FrameSize,
    square_pixel_frame_size: FrameSize,
) -> String {
    let position = watermark_overlay_position(watermark, resolution, square_pixel_frame_size);
    let mut filter = format!("overlay_cuda={position}");
    append_overlay_timing_options(&mut filter, watermark);
    filter
}

#[must_use]
pub fn still_image_loop_filter() -> String {
    "loop=-1:1".to_owned()
}

#[must_use]
pub fn realtime_filter() -> String {
    "realtime".to_owned()
}

#[must_use]
pub fn graphics_overlay_vaapi_filter() -> String {
    "overlay_vaapi".to_owned()
}

fn watermark_overlay_position(
    watermark: &WatermarkOverlay,
    resolution: FrameSize,
    square_pixel_frame_size: FrameSize,
) -> String {
    let (horizontal_margin, vertical_margin) = if watermark.place_within_source_content {
        source_content_margins(watermark, resolution, square_pixel_frame_size)
    } else {
        normal_margins(watermark, resolution)
    };

    match watermark.location {
        WatermarkLocation::BottomLeft => format!("x={horizontal_margin}:y=H-h-{vertical_margin}"),
        WatermarkLocation::TopLeft => format!("x={horizontal_margin}:y={vertical_margin}"),
        WatermarkLocation::TopRight => format!("x=W-w-{horizontal_margin}:y={vertical_margin}"),
        WatermarkLocation::TopMiddle => format!("x=(W-w)/2:y={vertical_margin}"),
        WatermarkLocation::RightMiddle => format!("x=W-w-{horizontal_margin}:y=(H-h)/2"),
        WatermarkLocation::BottomMiddle => format!("x=(W-w)/2:y=H-h-{vertical_margin}"),
        WatermarkLocation::LeftMiddle => format!("x={horizontal_margin}:y=(H-h)/2"),
        WatermarkLocation::MiddleCenter => "x=(W-w)/2:y=(H-h)/2".to_owned(),
        WatermarkLocation::BottomRight => {
            format!("x=W-w-{horizontal_margin}:y=H-h-{vertical_margin}")
        }
    }
}

fn append_overlay_timing_options(filter: &mut String, watermark: &WatermarkOverlay) {
    if watermark.eof_action_pass {
        filter.push_str(":eof_action=pass");
    }
    if let Some(enable) = watermark
        .enable
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        filter.push_str(":enable='");
        filter.push_str(&escape_overlay_filter_value(enable));
        filter.push('\'');
    }
}

#[must_use]
pub fn watermark_opacity_filter(opacity: u8) -> String {
    let opacity = f64::from(opacity) / 100.0;
    format!(
        "format=yuva420p|yuva444p|yuva422p|rgba|abgr|bgra|gbrap|ya8,colorchannelmixer=aa={opacity:.2}"
    )
}

#[must_use]
pub fn watermark_alpha_format_filter() -> String {
    "format=yuva420p".to_owned()
}

#[must_use]
pub fn watermark_opacity_expression_filter(expression: &str) -> String {
    let expression = translate_watermark_opacity_expression(expression);
    // `geq` requires a luminance (or RGB) expression — alpha alone fails at
    // runtime with "A luminance or RGB expression is mandatory". Copy the
    // color planes through and rewrite only alpha.
    format!(
        "format=yuva420p|yuva444p|yuva422p,geq=lum='lum(X,Y)':cb='cb(X,Y)':cr='cr(X,Y)':a='clip(255*({expression}),0,255)'"
    )
}

/// Translate an `ErsatzTV` (`NCalc`) watermark opacity expression into
/// `FFmpeg` `geq` eval syntax.
///
/// `ErsatzTV` evaluates these expressions per frame with `NCalc`, which
/// accepts infix `%`, comparison, and boolean operators. The `FFmpeg`
/// expression evaluator has no infix `%`/`<`/`&&` — they must become `mod()`,
/// `lt()`, `min()` (boolean results are exactly 0/1, so `min`/`max` are
/// and/or). `geq` exposes the frame time as uppercase `T`. An expression that
/// cannot be parsed is returned with symbol substitution only, preserving the
/// previous behavior.
#[must_use]
pub fn translate_watermark_opacity_expression(expression: &str) -> String {
    OpacityExpressionParser::translate(expression).unwrap_or_else(|| {
        expression
            .replace("time_of_day_seconds", "T")
            .replace("content_total_seconds", "NAN")
            .replace("content_seconds", "T")
            .replace("channel_seconds", "T")
    })
}

/// Recursive-descent parser for the `NCalc` operator subset `ErsatzTV`
/// opacity expressions use, emitting `FFmpeg` eval syntax.
struct OpacityExpressionParser {
    tokens: Vec<OpacityToken>,
    position: usize,
}

#[derive(Debug, Clone, PartialEq)]
enum OpacityToken {
    Number(String),
    Identifier(String),
    Operator(&'static str),
    OpenParen,
    CloseParen,
    Comma,
}

impl OpacityExpressionParser {
    fn translate(expression: &str) -> Option<String> {
        let tokens = Self::tokenize(expression)?;
        let mut parser = Self {
            tokens,
            position: 0,
        };
        let translated = parser.parse_or()?;
        (parser.position == parser.tokens.len()).then_some(translated)
    }

    fn tokenize(expression: &str) -> Option<Vec<OpacityToken>> {
        let mut tokens = Vec::new();
        let chars: Vec<char> = expression.chars().collect();
        let mut index = 0;
        while index < chars.len() {
            let c = chars[index];
            match c {
                ' ' | '\t' | '\r' | '\n' => index += 1,
                '(' => {
                    tokens.push(OpacityToken::OpenParen);
                    index += 1;
                }
                ')' => {
                    tokens.push(OpacityToken::CloseParen);
                    index += 1;
                }
                ',' => {
                    tokens.push(OpacityToken::Comma);
                    index += 1;
                }
                '+' | '-' | '*' | '/' | '%' => {
                    tokens.push(OpacityToken::Operator(match c {
                        '+' => "+",
                        '-' => "-",
                        '*' => "*",
                        '/' => "/",
                        _ => "%",
                    }));
                    index += 1;
                }
                '<' | '>' | '=' | '!' | '&' | '|' => {
                    let next = chars.get(index + 1).copied();
                    let (operator, len) = match (c, next) {
                        ('<', Some('=')) => ("<=", 2),
                        ('<', _) => ("<", 1),
                        ('>', Some('=')) => (">=", 2),
                        ('>', _) => (">", 1),
                        ('=', Some('=')) => ("==", 2),
                        ('=', _) => ("==", 1),
                        ('!', Some('=')) => ("!=", 2),
                        ('!', _) => ("!", 1),
                        ('&', Some('&')) => ("&&", 2),
                        ('|', Some('|')) => ("||", 2),
                        _ => return None,
                    };
                    tokens.push(OpacityToken::Operator(operator));
                    index += len;
                }
                _ if c.is_ascii_digit() || c == '.' => {
                    let start = index;
                    while index < chars.len()
                        && (chars[index].is_ascii_digit() || chars[index] == '.')
                    {
                        index += 1;
                    }
                    tokens.push(OpacityToken::Number(chars[start..index].iter().collect()));
                }
                _ if c.is_ascii_alphabetic() || c == '_' => {
                    let start = index;
                    while index < chars.len()
                        && (chars[index].is_ascii_alphanumeric() || chars[index] == '_')
                    {
                        index += 1;
                    }
                    tokens.push(OpacityToken::Identifier(
                        chars[start..index].iter().collect(),
                    ));
                }
                _ => return None,
            }
        }
        Some(tokens)
    }

    fn peek_operator(&self) -> Option<&'static str> {
        match self.tokens.get(self.position) {
            Some(OpacityToken::Operator(operator)) => Some(operator),
            _ => None,
        }
    }

    fn consume(&mut self) -> Option<OpacityToken> {
        let token = self.tokens.get(self.position).cloned();
        if token.is_some() {
            self.position += 1;
        }
        token
    }

    fn parse_or(&mut self) -> Option<String> {
        let mut left = self.parse_and()?;
        while self.peek_operator() == Some("||") {
            self.position += 1;
            let right = self.parse_and()?;
            left = format!("max({left},{right})");
        }
        Some(left)
    }

    fn parse_and(&mut self) -> Option<String> {
        let mut left = self.parse_comparison()?;
        while self.peek_operator() == Some("&&") {
            self.position += 1;
            let right = self.parse_comparison()?;
            left = format!("min({left},{right})");
        }
        Some(left)
    }

    fn parse_comparison(&mut self) -> Option<String> {
        let left = self.parse_additive()?;
        let Some(operator) = self
            .peek_operator()
            .filter(|op| matches!(*op, "<" | "<=" | ">" | ">=" | "==" | "!="))
        else {
            return Some(left);
        };
        self.position += 1;
        let right = self.parse_additive()?;
        Some(match operator {
            "<" => format!("lt({left},{right})"),
            "<=" => format!("lte({left},{right})"),
            ">" => format!("gt({left},{right})"),
            ">=" => format!("gte({left},{right})"),
            "==" => format!("eq({left},{right})"),
            _ => format!("not(eq({left},{right}))"),
        })
    }

    fn parse_additive(&mut self) -> Option<String> {
        let mut left = self.parse_multiplicative()?;
        while let Some(operator) = self.peek_operator().filter(|op| matches!(*op, "+" | "-")) {
            self.position += 1;
            let right = self.parse_multiplicative()?;
            left = format!("({left}{operator}{right})");
        }
        Some(left)
    }

    fn parse_multiplicative(&mut self) -> Option<String> {
        let mut left = self.parse_unary()?;
        while let Some(operator) = self
            .peek_operator()
            .filter(|op| matches!(*op, "*" | "/" | "%"))
        {
            self.position += 1;
            let right = self.parse_unary()?;
            left = if operator == "%" {
                format!("mod({left},{right})")
            } else {
                format!("({left}{operator}{right})")
            };
        }
        Some(left)
    }

    fn parse_unary(&mut self) -> Option<String> {
        match self.peek_operator() {
            Some("-") => {
                self.position += 1;
                let operand = self.parse_unary()?;
                Some(format!("(0-{operand})"))
            }
            Some("!") => {
                self.position += 1;
                let operand = self.parse_unary()?;
                Some(format!("not({operand})"))
            }
            _ => self.parse_primary(),
        }
    }

    fn parse_primary(&mut self) -> Option<String> {
        match self.consume()? {
            OpacityToken::Number(number) => Some(number),
            OpacityToken::OpenParen => {
                let inner = self.parse_or()?;
                match self.consume()? {
                    OpacityToken::CloseParen => Some(format!("({inner})")),
                    _ => None,
                }
            }
            OpacityToken::Identifier(name) => {
                if matches!(
                    self.tokens.get(self.position),
                    Some(OpacityToken::OpenParen)
                ) {
                    self.position += 1;
                    let mut arguments = Vec::new();
                    if matches!(
                        self.tokens.get(self.position),
                        Some(OpacityToken::CloseParen)
                    ) {
                        self.position += 1;
                    } else {
                        loop {
                            arguments.push(self.parse_or()?);
                            match self.consume()? {
                                OpacityToken::Comma => {}
                                OpacityToken::CloseParen => break,
                                _ => return None,
                            }
                        }
                    }
                    Self::translate_function_call(&name, &arguments)
                } else {
                    Some(Self::translate_identifier(&name))
                }
            }
            _ => None,
        }
    }

    /// Translate a function call, mapping `ErsatzTV`'s custom `NCalc` fade
    /// helpers (`OpacityExpressionHelper.EvaluateFunction`) to equivalent
    /// `FFmpeg` eval trapezoid expressions; other functions pass through
    /// lowercased (`NCalc` built-ins like `Sin` match `FFmpeg` eval names).
    fn translate_function_call(name: &str, arguments: &[String]) -> Option<String> {
        if name.eq_ignore_ascii_case("LinearFadePoints") {
            // LinearFadePoints(time, start, peakStart, peakEnd, end):
            // 0 outside [start, end), linear ramp up to peakStart, solid
            // until peakEnd, linear ramp down to end — a clipped trapezoid.
            let [time, start, peak_start, peak_end, end] = arguments else {
                return None;
            };
            return Some(format!(
                "clip(min((({time})-({start}))/(({peak_start})-({start})),(({end})-({time}))/(({end})-({peak_end}))),0,1)"
            ));
        }
        if name.eq_ignore_ascii_case("LinearFadeDuration") {
            // LinearFadeDuration(time, start, fadeSeconds, peakSeconds):
            // rectangle when fadeSeconds <= 0, otherwise the trapezoid
            // start..start+fade..+peak..+fade.
            let [time, start, fade, peak] = arguments else {
                return None;
            };
            return Some(format!(
                "if(lte(({fade}),0),gte(({time}),({start}))*lt(({time}),({start})+({peak})),clip(min((({time})-({start}))/({fade}),(({start})+2*({fade})+({peak})-({time}))/({fade})),0,1))"
            ));
        }
        Some(format!(
            "{}({})",
            name.to_ascii_lowercase(),
            arguments.join(",")
        ))
    }

    fn translate_identifier(name: &str) -> String {
        // `geq` exposes the frame time as uppercase `T` (lowercase `t` is an
        // undefined constant there and fails filter initialization).
        match name {
            "time_of_day_seconds" | "content_seconds" | "channel_seconds" => "T".to_owned(),
            "content_total_seconds" => "NAN".to_owned(),
            "true" | "True" => "1".to_owned(),
            "false" | "False" => "0".to_owned(),
            _ => name.to_owned(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatermarkFadeKind {
    In,
    Out,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WatermarkFadePoint {
    pub kind: WatermarkFadeKind,
    pub time: Duration,
    pub enable_start: Duration,
    pub enable_finish: Duration,
}

#[must_use]
pub fn watermark_fade_filter(point: WatermarkFadePoint) -> String {
    let kind = match point.kind {
        WatermarkFadeKind::In => "in",
        WatermarkFadeKind::Out => "out",
    };
    format!(
        "fade={kind}:st={}:d=1:alpha=1:enable='between(t,{},{})'",
        seconds(point.time),
        seconds(point.enable_start),
        seconds(point.enable_finish)
    )
}

#[must_use]
pub fn song_progress_filter(
    frame_size: FrameSize,
    start: Option<Duration>,
    duration: Option<Duration>,
) -> String {
    let Some(duration) = duration else {
        return String::new();
    };

    let start = start.unwrap_or_default();
    let finish = start + duration;
    let width = f64::from(frame_size.width) * 0.9;
    let height = f64::from(frame_size.height) * 0.025;
    let seconds = duration.as_secs_f64();
    let already_played = start.as_secs_f64() / finish.as_secs_f64();
    let scale = 1.0 - already_played;

    let generate_gray_bar =
        format!("color=c=#323232:s={width}x{height},format=rgba,colorchannelmixer=aa=0.4");
    let generate_white_bar = format!("color=c=white:s={width}x{height}");
    let scale_to_full_width =
        format!("scale=iw*{already_played}+iw*(t/{seconds})*{scale}:ih:eval=frame");
    let overlay_bar = "overlay=W*0.05:H-h-H*0.05:shortest=1:enable='gt(t,0.1)'";

    format!(
        "loop=-1:1[si],{generate_gray_bar}[gray];{generate_white_bar},format=rgba,colorchannelmixer=aa=0.9,{scale_to_full_width}[sbar];[si][gray]{overlay_bar}[sgray];[sgray][sbar]{overlay_bar}"
    )
}

fn with_hwdownload(current: &FrameState, filter: &str) -> String {
    if current.frame_data_location == FrameDataLocation::Hardware {
        if current.pixel_format == PixelFormat::Nv12 {
            return format!("hwdownload,format=nv12,{filter}");
        }
        return format!(
            "hwdownload,format={},{filter}",
            current.pixel_format.ffmpeg_name()
        );
    }
    filter.to_owned()
}

fn normal_margins(watermark: &WatermarkOverlay, resolution: FrameSize) -> (String, String) {
    (
        rounded_number(watermark.horizontal_margin_percent / 100.0 * f64::from(resolution.width)),
        rounded_number(watermark.vertical_margin_percent / 100.0 * f64::from(resolution.height)),
    )
}

fn source_content_margins(
    watermark: &WatermarkOverlay,
    resolution: FrameSize,
    square_pixel_frame_size: FrameSize,
) -> (String, String) {
    let horizontal_padding = resolution
        .width
        .saturating_sub(square_pixel_frame_size.width);
    let vertical_padding = resolution
        .height
        .saturating_sub(square_pixel_frame_size.height);
    (
        rounded_number(
            watermark.horizontal_margin_percent / 100.0 * f64::from(square_pixel_frame_size.width)
                + f64::from(horizontal_padding) / 2.0,
        ),
        rounded_number(
            watermark.vertical_margin_percent / 100.0 * f64::from(square_pixel_frame_size.height)
                + f64::from(vertical_padding) / 2.0,
        ),
    )
}

fn seconds(duration: Duration) -> String {
    let seconds = duration.as_secs_f64();
    if seconds.fract() == 0.0 {
        return format!("{seconds:.0}");
    }

    let mut value = String::new();
    write!(&mut value, "{seconds:.3}").expect("writing to String cannot fail");
    value.trim_end_matches('0').trim_end_matches('.').to_owned()
}

fn rounded_number(value: f64) -> String {
    format!("{value:.0}")
}

fn escape_overlay_filter_value(value: &str) -> String {
    value.replace('\'', "\\'")
}
