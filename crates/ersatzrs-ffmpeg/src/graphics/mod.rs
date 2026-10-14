use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use ersatzrs_domain::{
    GraphicsElement, GraphicsElementConfig, MotionEndBehavior, TextAlignment, TextFit,
    WatermarkLocation,
};
use mlua::{Lua, Table, Value};

use crate::{
    DrawtextAlignment, DrawtextExpansion, DrawtextFilterBuilder, DrawtextFont,
    DrawtextHorizontalAlignment, DrawtextTextSource, DrawtextVerticalAlignment, DrawtextYAlignment,
    FrameSize, GraphicsPipelineOverlay, InputFile, WatermarkOverlay,
};

#[derive(Debug, thiserror::Error)]
pub enum GraphicsTemplateError {
    #[error("graphics template has no file name: {0}")]
    MissingFileName(String),
    #[error("graphics template I/O error for {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("graphics element {id} has no renderable {field}")]
    MissingField { id: i64, field: &'static str },
    #[error("graphics element {id} script failed: {source}")]
    Script { id: i64, source: mlua::Error },
    #[error("graphics element {id} script did not return drawvg script")]
    InvalidScriptResult { id: i64 },
    #[error("graphics element {id} has invalid or unbounded JPEG XL timing")]
    InvalidJpegXlTiming { id: i64 },
    #[error("graphics element {id} has invalid or unbounded image timing")]
    InvalidImageTiming { id: i64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DrawvgOverlay {
    pub script_path: PathBuf,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub enable: Option<String>,
}

impl DrawvgOverlay {
    #[must_use]
    pub fn filter(&self) -> String {
        let mut filter = format!(
            "drawvg=file={}",
            crate::filter::escaping::escape_filtergraph_path(&self.script_path)
        );
        if let Some(enable) = &self.enable {
            filter.push_str(":enable='");
            filter.push_str(&escape_filter_value(enable));
            filter.push('\'');
        }
        filter
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SvgTemplate {
    source: String,
}

impl SvgTemplate {
    #[must_use]
    pub fn new(source: impl Into<String>) -> Self {
        Self {
            source: source.into(),
        }
    }

    #[must_use]
    pub fn render(&self, variables: &BTreeMap<String, String>) -> String {
        render_template(&self.source, variables, true)
    }

    #[must_use]
    pub fn render_raw(&self, variables: &BTreeMap<String, String>) -> String {
        render_template(&self.source, variables, false)
    }
}

fn render_template(
    source: &str,
    variables: &BTreeMap<String, String>,
    escape_values: bool,
) -> String {
    let mut rendered = source.to_owned();
    for (name, value) in variables {
        let value = if escape_values {
            escape_xml(value)
        } else {
            value.clone()
        };
        rendered = rendered.replace(&format!("{{{{{name}}}}}"), &value);
    }
    rendered
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphicsTemplateRenderer {
    output_dir: PathBuf,
}

impl GraphicsTemplateRenderer {
    #[must_use]
    pub fn new(output_dir: impl Into<PathBuf>) -> Self {
        Self {
            output_dir: output_dir.into(),
        }
    }

    /// Renders a template file into the renderer output directory.
    ///
    /// # Errors
    ///
    /// Returns an error when the template cannot be read, the output directory
    /// cannot be created, the template path has no file name, or the rendered
    /// template cannot be written.
    pub fn render_template_file(
        &self,
        template_path: impl AsRef<Path>,
        variables: &BTreeMap<String, String>,
    ) -> Result<PathBuf, GraphicsTemplateError> {
        let template_path = template_path.as_ref();
        let template =
            std::fs::read_to_string(template_path).map_err(|source| GraphicsTemplateError::Io {
                path: template_path.to_path_buf(),
                source,
            })?;
        let rendered = SvgTemplate::new(template).render(variables);

        std::fs::create_dir_all(&self.output_dir).map_err(|source| GraphicsTemplateError::Io {
            path: self.output_dir.clone(),
            source,
        })?;

        let file_name = template_path.file_name().ok_or_else(|| {
            GraphicsTemplateError::MissingFileName(template_path.display().to_string())
        })?;
        let output_path = self.output_dir.join(file_name);
        std::fs::write(&output_path, rendered).map_err(|source| GraphicsTemplateError::Io {
            path: output_path.clone(),
            source,
        })?;
        Ok(output_path)
    }

    /// Renders DB-backed graphics elements into generated inputs and `drawvg`
    /// overlays.
    ///
    /// This is the FFmpeg-native counterpart to ErsatzTV's Skia
    /// `GraphicsEngine`: static image elements become native overlay inputs and
    /// text elements become VGS draw scripts, while timing, opacity, z-order,
    /// and source-content placement are represented in the generated
    /// overlay/filter metadata.
    ///
    /// Source map:
    /// - `$ERSATZTV_SRC/ErsatzTV.Infrastructure/Streaming/Graphics/GraphicsEngine.cs`
    /// - `$ERSATZTV_SRC/ErsatzTV.Infrastructure/Streaming/Graphics/Image/ImageElement.cs`
    /// - `$ERSATZTV_SRC/ErsatzTV.Infrastructure/Streaming/Graphics/Image/ImageElementBase.cs`
    /// - `$ERSATZTV_SRC/ErsatzTV.Infrastructure/Streaming/Graphics/Text/TextElement.cs`
    ///
    /// # Errors
    ///
    /// Returns an error when an element is missing a required image/text field,
    /// the output directory cannot be created, or a generated artifact cannot
    /// be written.
    pub fn render_graphics_elements(
        &self,
        elements: &[GraphicsElement],
        variables: &BTreeMap<String, String>,
        context: &GraphicsRenderContext,
    ) -> Result<Vec<RenderedGraphicsElement>, GraphicsTemplateError> {
        std::fs::create_dir_all(&self.output_dir).map_err(|source| GraphicsTemplateError::Io {
            path: self.output_dir.clone(),
            source,
        })?;

        let mut rendered = Vec::new();
        for element in elements {
            if let Some(rendered_element) =
                render_graphics_element(element, variables, &self.output_dir, context)?
            {
                rendered.push(rendered_element);
            }
        }

        rendered.sort_by_key(RenderedGraphicsElement::z_index);
        Ok(rendered)
    }
}

#[must_use]
pub fn lower_third_overlay(
    script_path: impl Into<PathBuf>,
    frame_size: FrameSize,
    enable: Option<String>,
) -> DrawvgOverlay {
    DrawvgOverlay {
        script_path: script_path.into(),
        x: 0,
        y: i32::try_from(frame_size.height.saturating_mul(3) / 4).unwrap_or(i32::MAX),
        width: frame_size.width,
        height: frame_size.height / 4,
        enable,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct GraphicsRenderContext {
    pub frame_size: FrameSize,
    pub square_pixel_frame_size: FrameSize,
    motion_source_durations: BTreeMap<i64, f64>,
    stream_offset: std::time::Duration,
    stream_duration: Option<std::time::Duration>,
    image_sources: BTreeMap<i64, ImageTiming>,
}

/// Actual decoded frame timing; a single decoded frame is a still image.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageTiming {
    pub frame_count: usize,
    pub duration_seconds: f64,
    pub last_frame_seconds: f64,
    pub frame_timestamps: Vec<f64>,
}

/// Compatibility name for the original JPEG XL timing API.
pub type JpegXlTiming = ImageTiming;

impl GraphicsRenderContext {
    #[must_use]
    pub fn new(frame_size: FrameSize, square_pixel_frame_size: FrameSize) -> Self {
        Self {
            frame_size,
            square_pixel_frame_size,
            motion_source_durations: BTreeMap::new(),
            stream_offset: std::time::Duration::ZERO,
            stream_duration: None,
            image_sources: BTreeMap::new(),
        }
    }

    /// Programme-relative source phase for native playback and canvas chunks.
    #[must_use]
    pub fn with_stream_offset(mut self, offset: std::time::Duration) -> Self {
        self.stream_offset = offset;
        self
    }

    /// Finite remaining output duration, independent of the motion start time.
    #[must_use]
    pub fn with_stream_duration(mut self, duration: std::time::Duration) -> Self {
        self.stream_duration = Some(duration);
        self
    }

    #[must_use]
    pub fn with_motion_source_duration(mut self, element_id: i64, duration_seconds: f64) -> Self {
        if duration_seconds.is_finite() && duration_seconds >= 0.0 {
            self.motion_source_durations
                .insert(element_id, duration_seconds);
        }
        self
    }

    fn motion_source_duration(&self, element_id: i64) -> Option<f64> {
        self.motion_source_durations.get(&element_id).copied()
    }

    #[must_use]
    pub fn with_jpeg_xl_timing(self, element_id: i64, timing: JpegXlTiming) -> Self {
        self.with_image_timing(element_id, timing)
    }

    /// Decoded timing for image inputs that need bounded file reopening.
    #[must_use]
    pub fn with_image_timing(mut self, element_id: i64, timing: ImageTiming) -> Self {
        let duration = timing.duration_seconds;
        self.image_sources.insert(element_id, timing);
        self.with_motion_source_duration(element_id, duration)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum RenderedGraphicsElement {
    Drawvg {
        id: i64,
        z_index: i32,
        script_path: PathBuf,
        overlay: DrawvgOverlay,
    },
    NativeImage {
        id: i64,
        z_index: i32,
        input: InputFile,
        overlay: WatermarkOverlay,
    },
}

impl RenderedGraphicsElement {
    #[must_use]
    pub const fn z_index(&self) -> i32 {
        match self {
            Self::Drawvg { z_index, .. } | Self::NativeImage { z_index, .. } => *z_index,
        }
    }

    #[must_use]
    pub fn into_pipeline_overlay(self) -> GraphicsPipelineOverlay {
        match self {
            Self::Drawvg {
                z_index, overlay, ..
            } => GraphicsPipelineOverlay::drawvg(z_index, overlay.filter()),
            Self::NativeImage {
                z_index,
                input,
                overlay,
                ..
            } => GraphicsPipelineOverlay::native_image(z_index, input, overlay),
        }
    }
}

#[must_use]
pub fn graphics_element_drawvg_filter(
    script_path: impl Into<PathBuf>,
    element: &GraphicsElement,
    context: &GraphicsRenderContext,
) -> DrawvgOverlay {
    DrawvgOverlay {
        script_path: script_path.into(),
        x: 0,
        y: 0,
        width: context.frame_size.width,
        height: context.frame_size.height,
        enable: graphics_enable_expression(element),
    }
}

/// Build the reloadable drawtext overlay for a text graphics element.
///
/// The infrastructure controller owns and atomically refreshes `text_file`;
/// this function maps the persisted text style and placement snapshot onto the
/// typed `FFmpeg` 9 filter contract. An exact packaged font file is used by
/// default so rendering does not depend on host Fontconfig state.
///
/// # Errors
///
/// Returns an error when `element` is not a text element or has no usable font
/// selector.
pub fn graphics_element_drawtext_overlay(
    element: &GraphicsElement,
    text_file: impl Into<PathBuf>,
    default_font_file: impl Into<PathBuf>,
    context: &GraphicsRenderContext,
) -> Result<GraphicsPipelineOverlay, GraphicsTemplateError> {
    let GraphicsElementConfig::Text(config) = &element.config else {
        return Err(GraphicsTemplateError::MissingField {
            id: element.id,
            field: "text config",
        });
    };
    let style = config
        .styles
        .iter()
        .find(|style| style.name.as_deref() == config.base_style.as_deref())
        .or_else(|| config.styles.first());
    let font = config
        .drawtext
        .font_file
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(DrawtextFont::file)
        .or_else(|| {
            style
                .and_then(|style| style.font_family.as_deref())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(DrawtextFont::family)
        })
        .unwrap_or_else(|| DrawtextFont::file(default_font_file.into()));
    let reload = std::num::NonZeroU32::new(config.drawtext.reload_interval_frames.max(1));
    let mut builder = DrawtextFilterBuilder::new(
        DrawtextTextSource::text_file(text_file.into(), reload),
        font,
    )
    .expansion(DrawtextExpansion::None)
    .text_shaping(true)
    .fix_bounds(true)
    .y_alignment(DrawtextYAlignment::Text);

    let font_size = style.and_then(|style| style.font_size).unwrap_or(48.0);
    builder = builder.font_size(trim_float(f64::from(font_size.max(1.0))));
    let (x, y, horizontal_alignment) = drawtext_position(element, config.align, context);
    builder = builder.position(x, y).alignment(DrawtextAlignment::new(
        horizontal_alignment,
        DrawtextVerticalAlignment::Top,
    ));

    if let Some(color) = style
        .and_then(|style| style.text_color.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        builder = builder.font_color(color);
    }
    if let Some(style) = style {
        if let (Some(width), Some(color)) = (style.halo_width, style.halo_color.as_deref()) {
            if width.is_finite() && width > 0.0 && !color.trim().is_empty() {
                if let Ok(width) = format!("{width:.0}").parse::<u32>() {
                    builder = builder.border(width, color.trim());
                }
            }
        }
        if let Some(line_height) = style.line_height.filter(|value| value.is_finite()) {
            let line_spacing = (line_height - font_size).round();
            if line_spacing >= f32::from(i16::MIN) && line_spacing <= f32::from(i16::MAX) {
                if let Ok(line_spacing) = format!("{line_spacing:.0}").parse::<i32>() {
                    builder = builder.line_spacing(line_spacing);
                }
            }
        }
    }
    let alpha = element
        .opacity_expression
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map_or_else(|| static_opacity(element), str::to_owned);
    builder = builder.alpha(alpha);
    if let Some(enable) = graphics_enable_expression(element) {
        builder = builder.enable(enable);
    }

    Ok(GraphicsPipelineOverlay::drawtext(
        element.z_index,
        builder.build(),
    ))
}

fn drawtext_position(
    element: &GraphicsElement,
    requested_alignment: TextAlignment,
    context: &GraphicsRenderContext,
) -> (String, String, DrawtextHorizontalAlignment) {
    if let (Some(x), Some(y)) = (element.location_x, element.location_y) {
        let x = match requested_alignment {
            TextAlignment::Left => trim_float(x),
            TextAlignment::Center => format!("{}-text_w/2", trim_float(x)),
            TextAlignment::Right => format!("{}-text_w", trim_float(x)),
        };
        return (x, trim_float(y), drawtext_alignment(requested_alignment));
    }

    let (horizontal_margin, vertical_margin) = margins(element, context);
    let x_margin = parse_percent(&horizontal_margin) / 100.0 * f64::from(context.frame_size.width);
    let y_margin = parse_percent(&vertical_margin) / 100.0 * f64::from(context.frame_size.height);
    let (x, alignment) = match element.location {
        WatermarkLocation::TopLeft
        | WatermarkLocation::LeftMiddle
        | WatermarkLocation::BottomLeft => {
            (trim_float(x_margin), DrawtextHorizontalAlignment::Left)
        }
        WatermarkLocation::TopMiddle
        | WatermarkLocation::MiddleCenter
        | WatermarkLocation::BottomMiddle => (
            "(w-text_w)/2".to_owned(),
            DrawtextHorizontalAlignment::Center,
        ),
        WatermarkLocation::TopRight
        | WatermarkLocation::RightMiddle
        | WatermarkLocation::BottomRight => (
            format!("w-text_w-{}", trim_float(x_margin)),
            DrawtextHorizontalAlignment::Right,
        ),
    };
    let y = match element.location {
        WatermarkLocation::TopLeft | WatermarkLocation::TopMiddle | WatermarkLocation::TopRight => {
            trim_float(y_margin)
        }
        WatermarkLocation::LeftMiddle
        | WatermarkLocation::MiddleCenter
        | WatermarkLocation::RightMiddle => "(h-text_h)/2".to_owned(),
        WatermarkLocation::BottomLeft
        | WatermarkLocation::BottomMiddle
        | WatermarkLocation::BottomRight => format!("h-text_h-{}", trim_float(y_margin)),
    };
    (x, y, alignment)
}

const fn drawtext_alignment(alignment: TextAlignment) -> DrawtextHorizontalAlignment {
    match alignment {
        TextAlignment::Left => DrawtextHorizontalAlignment::Left,
        TextAlignment::Center => DrawtextHorizontalAlignment::Center,
        TextAlignment::Right => DrawtextHorizontalAlignment::Right,
    }
}

#[must_use]
pub fn graphics_image_overlay(element: &GraphicsElement) -> Option<WatermarkOverlay> {
    if !matches!(element.config, GraphicsElementConfig::Image(_)) {
        return None;
    }

    Some(graphics_native_overlay(element, None, false))
}

#[must_use]
pub fn graphics_motion_overlay(
    element: &GraphicsElement,
    context: &GraphicsRenderContext,
) -> Option<WatermarkOverlay> {
    let GraphicsElementConfig::Motion(config) = &element.config else {
        return None;
    };
    let mut overlay = graphics_native_overlay(
        element,
        Some(graphics_motion_enable_expression(
            config,
            context.motion_source_duration(element.id),
            context.stream_offset.as_secs_f64(),
        )),
        config.end_behavior == MotionEndBehavior::Disappear,
    );
    apply_native_pixel_location(&mut overlay, element, context.frame_size);
    Some(overlay)
}

fn apply_native_pixel_location(
    overlay: &mut WatermarkOverlay,
    element: &GraphicsElement,
    frame_size: FrameSize,
) {
    if let (Some(x), Some(y)) = (element.location_x, element.location_y) {
        overlay.location = ersatzrs_domain::WatermarkLocation::TopLeft;
        overlay.horizontal_margin_percent = x * 100.0 / f64::from(frame_size.width);
        overlay.vertical_margin_percent = y * 100.0 / f64::from(frame_size.height);
        overlay.place_within_source_content = false;
    }
}

fn graphics_subtitle_overlay(element: &GraphicsElement) -> WatermarkOverlay {
    WatermarkOverlay {
        location: WatermarkLocation::MiddleCenter,
        size: ersatzrs_domain::WatermarkSize::ActualSize,
        width_percent: 100.0,
        horizontal_margin_percent: 0.0,
        vertical_margin_percent: 0.0,
        opacity: 100,
        place_within_source_content: false,
        fade_points: Vec::new(),
        opacity_expression: None,
        enable: graphics_enable_expression(element),
        eof_action_pass: false,
        z_index: element.z_index,
    }
}

fn graphics_native_overlay(
    element: &GraphicsElement,
    enable: Option<String>,
    eof_action_pass: bool,
) -> WatermarkOverlay {
    WatermarkOverlay {
        location: element.location,
        size: if element.scale {
            ersatzrs_domain::WatermarkSize::Scaled
        } else {
            ersatzrs_domain::WatermarkSize::ActualSize
        },
        width_percent: element.scale_width_percent.unwrap_or(100.0),
        horizontal_margin_percent: element.horizontal_margin_percent.unwrap_or(0.0),
        vertical_margin_percent: element.vertical_margin_percent.unwrap_or(0.0),
        opacity: u8::try_from(element.opacity_percent.unwrap_or(100).clamp(0, 100)).unwrap_or(100),
        place_within_source_content: element.place_within_source_content,
        fade_points: Vec::new(),
        opacity_expression: element.opacity_expression.clone(),
        enable,
        eof_action_pass,
        z_index: element.z_index,
    }
}

fn render_graphics_element(
    element: &GraphicsElement,
    variables: &BTreeMap<String, String>,
    output_dir: &Path,
    context: &GraphicsRenderContext,
) -> Result<Option<RenderedGraphicsElement>, GraphicsTemplateError> {
    match &element.config {
        GraphicsElementConfig::Image(config) => {
            render_image_graphics_element(element, config, output_dir, context)
        }
        GraphicsElementConfig::Text(_) => {
            render_text_graphics_element(element, variables, output_dir, context)
        }
        GraphicsElementConfig::Motion(config) => {
            render_motion_graphics_element(element, config, output_dir, context)
        }
        GraphicsElementConfig::Subtitle(config) => {
            render_subtitle_graphics_element(element, config, variables, output_dir, context)
        }
        GraphicsElementConfig::Script(config) => {
            render_script_graphics_element(element, config, variables, output_dir, context)
        }
    }
}

fn render_image_graphics_element(
    element: &GraphicsElement,
    config: &ersatzrs_domain::ImageGraphicsConfig,
    output_dir: &Path,
    context: &GraphicsRenderContext,
) -> Result<Option<RenderedGraphicsElement>, GraphicsTemplateError> {
    let image = config
        .image
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or(GraphicsTemplateError::MissingField {
            id: element.id,
            field: "image",
        })?;
    let mut overlay =
        graphics_image_overlay(element).ok_or(GraphicsTemplateError::MissingField {
            id: element.id,
            field: "image overlay",
        })?;
    apply_native_pixel_location(&mut overlay, element, context.frame_size);
    Ok(Some(RenderedGraphicsElement::NativeImage {
        id: element.id,
        z_index: element.z_index,
        input: if image_needs_timing(image) {
            finite_image_input(
                element.id,
                image,
                output_dir,
                context,
                true,
                context.stream_offset.as_secs_f64(),
            )?
        } else {
            InputFile::graphics_image(image)
        },
        overlay,
    }))
}

fn render_text_graphics_element(
    element: &GraphicsElement,
    variables: &BTreeMap<String, String>,
    output_dir: &Path,
    context: &GraphicsRenderContext,
) -> Result<Option<RenderedGraphicsElement>, GraphicsTemplateError> {
    let output_path = output_dir.join(graphics_drawvg_file_name(element));
    std::fs::write(
        &output_path,
        text_drawvg_for_element(element, variables, context)?,
    )
    .map_err(|source| GraphicsTemplateError::Io {
        path: output_path.clone(),
        source,
    })?;
    Ok(Some(RenderedGraphicsElement::Drawvg {
        id: element.id,
        z_index: element.z_index,
        script_path: output_path.clone(),
        overlay: DrawvgOverlay {
            script_path: output_path,
            x: 0,
            y: 0,
            width: context.frame_size.width,
            height: context.frame_size.height,
            enable: graphics_enable_expression(element),
        },
    }))
}

fn render_motion_graphics_element(
    element: &GraphicsElement,
    config: &ersatzrs_domain::MotionGraphicsConfig,
    output_dir: &Path,
    context: &GraphicsRenderContext,
) -> Result<Option<RenderedGraphicsElement>, GraphicsTemplateError> {
    let video_path = config
        .video_path
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or(GraphicsTemplateError::MissingField {
            id: element.id,
            field: "video_path",
        })?;
    let overlay =
        graphics_motion_overlay(element, context).ok_or(GraphicsTemplateError::MissingField {
            id: element.id,
            field: "motion overlay",
        })?;
    let timed_image = image_needs_timing(video_path);
    let mut input =
        InputFile::graphics_motion(video_path, config.end_behavior == MotionEndBehavior::Loop);
    let start = config.start_seconds.unwrap_or_default().max(0.0);
    let offset = context.stream_offset.as_secs_f64();
    if context
        .stream_duration
        .is_some_and(|duration| start >= offset + duration.as_secs_f64())
    {
        return Ok(None);
    }
    let mut seek = (offset - start).max(0.0);
    if let Some(duration) = context
        .motion_source_duration(element.id)
        .filter(|duration| *duration > 0.0)
    {
        match config.end_behavior {
            MotionEndBehavior::Loop => seek %= duration,
            MotionEndBehavior::Disappear if seek >= duration => return Ok(None),
            MotionEndBehavior::Hold
                if seek >= duration + config.hold_seconds.unwrap_or_default().max(0.0) =>
            {
                return Ok(None)
            }
            MotionEndBehavior::Disappear | MotionEndBehavior::Hold => {}
        }
    }
    let held_frame = config.end_behavior == MotionEndBehavior::Hold
        && context
            .motion_source_duration(element.id)
            .is_some_and(|duration| seek >= duration && duration > 0.0);
    if timed_image {
        input = finite_image_input(
            element.id,
            video_path,
            output_dir,
            context,
            config.end_behavior == MotionEndBehavior::Loop,
            if held_frame { 0.0 } else { seek },
        )?;
        if held_frame {
            let timing = &context.image_sources[&element.id];
            input.filter_steps.push(format!(
                "trim=start={},trim=end_frame=1,setpts=PTS-STARTPTS",
                jpeg_xl_time(timing.last_frame_seconds)
            ));
        }
    } else if held_frame {
        hold_final_motion_frame(
            &mut input,
            context
                .motion_source_duration(element.id)
                .unwrap_or_default(),
        );
    } else if seek > 0.0 {
        let input_seek = if config.end_behavior == MotionEndBehavior::Hold {
            context
                .motion_source_duration(element.id)
                .filter(|duration| *duration > 0.0)
                .map_or(seek, |duration| seek.min((duration - 1.0).max(0.0)))
        } else {
            seek
        };
        input
            .input_options
            .extend(["-ss".into(), trim_float(input_seek)]);
        // Keep a decoded EOF frame when the requested phase falls between the
        // final source timestamp and duration. Clamp decoding to the last second;
        // extending that frame before trimming avoids an empty sought input.
        // Retain input seek timestamps until trimming; normalizing earlier can
        // advance a frame when the clamped seek falls between source frames.
        if input_seek < seek {
            input.filter_steps.push(format!(
                "tpad=stop_mode=clone:stop_duration=1,trim=start={},setpts=PTS-STARTPTS",
                trim_float(seek - input_seek)
            ));
        } else {
            // Fractional input seeks can leave a positive first timestamp.
            input.filter_steps.push("setpts=PTS-STARTPTS".into());
        }
    }
    let delay = (start - offset).max(0.0);
    if delay > 0.0 {
        input
            .filter_steps
            .push(format!("setpts=PTS+{}/TB", trim_float(delay)));
    }
    Ok(Some(RenderedGraphicsElement::NativeImage {
        id: element.id,
        z_index: element.z_index,
        input,
        overlay,
    }))
}

fn hold_final_motion_frame(input: &mut InputFile, duration: f64) {
    input
        .input_options
        .extend(["-ss".into(), trim_float((duration - 1.0).max(0.0))]);
    // Decode at most the final second, then retain the exact last frame.
    input
        .filter_steps
        .push("reverse,trim=end_frame=1,setpts=PTS-STARTPTS".into());
}

fn jpeg_xl_time(value: f64) -> String {
    // FFprobe frame timestamps and FFmpeg's AVTB use microseconds. Rounding a
    // 24 fps frame up to milliseconds would trim away the in-progress frame.
    format!("{value:.6}")
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_owned()
}

fn image_needs_timing(source: &str) -> bool {
    crate::input::image_file_is_jpeg_xl(Path::new(source))
        || crate::input::image_file_is_apng(Path::new(source))
}

fn repeated_image_input(
    id: i64,
    source: &str,
    output_dir: &Path,
    duration: f64,
    cycles: f64,
    apng: bool,
) -> Result<InputFile, GraphicsTemplateError> {
    let invalid_timing = || {
        if apng {
            GraphicsTemplateError::InvalidImageTiming { id }
        } else {
            GraphicsTemplateError::InvalidJpegXlTiming { id }
        }
    };
    let source_path =
        std::fs::canonicalize(source).map_err(|source_error| GraphicsTemplateError::Io {
            path: source.into(),
            source: source_error,
        })?;
    let source = source_path.to_string_lossy();
    if source.contains(['\n', '\r', '\0']) {
        return Err(invalid_timing());
    }
    // ffconcat quoting is independent of filtergraph escaping. Native
    // Windows paths use forward slashes; POSIX literal backslashes survive.
    let source = if cfg!(windows) {
        source
            .strip_prefix(r"\\?\UNC\")
            .map_or_else(
                || source.strip_prefix(r"\\?\").unwrap_or(&source).to_owned(),
                |rest| format!("//{rest}"),
            )
            .replace('\\', "/")
    } else {
        source.into_owned()
    };
    let mut record = format!(
        "file '{}'\nduration {}\n",
        source.replace('\'', "'\\''"),
        jpeg_xl_time(duration)
    );
    if apng {
        record.push_str("option ignore_loop 1\n");
    }
    let mut manifest = String::from("ffconcat version 1.0\n");
    let mut cycle = 0.0;
    while cycle < cycles {
        if manifest.len().saturating_add(record.len()) > 8 * 1024 * 1024 {
            return Err(invalid_timing());
        }
        manifest.push_str(&record);
        cycle += 1.0;
    }
    let codec = if apng { "apng" } else { "jpeg-xl" };
    let path = output_dir.join(format!("{id}-{codec}.ffconcat"));
    std::fs::create_dir_all(output_dir)
        .and_then(|()| std::fs::write(&path, manifest))
        .map_err(|source| GraphicsTemplateError::Io {
            path: path.clone(),
            source,
        })?;
    Ok(
        InputFile::new(crate::InputKind::GraphicsImage, path.to_string_lossy(), 0)
            .with_input_options(["-f", "concat", "-safe", "0", "-protocol_whitelist", "file"]),
    )
}

fn finite_image_input(
    id: i64,
    source: &str,
    output_dir: &Path,
    context: &GraphicsRenderContext,
    repeat: bool,
    seek: f64,
) -> Result<InputFile, GraphicsTemplateError> {
    let apng = crate::input::image_file_is_apng(Path::new(source));
    let invalid_timing = || {
        if apng {
            GraphicsTemplateError::InvalidImageTiming { id }
        } else {
            GraphicsTemplateError::InvalidJpegXlTiming { id }
        }
    };
    let loop_options = if apng {
        vec!["-ignore_loop", "1"]
    } else {
        vec![]
    };
    let timing = context
        .image_sources
        .get(&id)
        .filter(|value| {
            value.frame_count > 0
                && value.duration_seconds.is_finite()
                && value.duration_seconds > 0.0
        })
        .ok_or_else(invalid_timing)?;
    if timing.frame_count == 1 {
        let mut input = InputFile::new(crate::InputKind::GraphicsImage, source, 0)
            .with_input_options(loop_options);
        input.filter_steps.push(format!(
            "loop=loop=-1:size=1:start=0,setpts=N*{}/TB",
            jpeg_xl_time(timing.duration_seconds)
        ));
        return Ok(input);
    }
    let seek = if repeat {
        seek.rem_euclid(timing.duration_seconds)
    } else {
        seek
    };
    let mut input = if repeat {
        let remaining = context
            .stream_duration
            .ok_or_else(invalid_timing)?
            .as_secs_f64();
        let cycles = ((remaining + seek) / timing.duration_seconds).ceil() + 1.0;
        if !cycles.is_finite() || !(1.0..=100_000.0).contains(&cycles) {
            return Err(invalid_timing());
        }
        repeated_image_input(
            id,
            source,
            output_dir,
            timing.duration_seconds,
            cycles,
            apng,
        )?
    } else {
        InputFile::new(crate::InputKind::GraphicsImage, source, 0).with_input_options(loop_options)
    };
    // Decode the compact source from its beginning. Reopening each concat
    // entry bounds decoded memory and supplies monotonic packet timestamps.
    let first_timestamp = timing
        .frame_timestamps
        .iter()
        .rev()
        .find(|timestamp| **timestamp <= seek + 0.000_001)
        .ok_or_else(invalid_timing)?;
    if !repeat {
        input.filter_steps.push(format!(
            "tpad=stop_mode=clone:stop_duration={}",
            jpeg_xl_time(timing.duration_seconds - timing.last_frame_seconds)
        ));
    }
    input.filter_steps.push(format!(
        "settb=AVTB,trim=start={},setpts=PTS-{}/TB",
        jpeg_xl_time(*first_timestamp),
        jpeg_xl_time(seek)
    ));
    Ok(input)
}

fn render_subtitle_graphics_element(
    element: &GraphicsElement,
    config: &ersatzrs_domain::SubtitleGraphicsConfig,
    variables: &BTreeMap<String, String>,
    output_dir: &Path,
    context: &GraphicsRenderContext,
) -> Result<Option<RenderedGraphicsElement>, GraphicsTemplateError> {
    let template_path = config
        .template
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or(GraphicsTemplateError::MissingField {
            id: element.id,
            field: "template",
        })?;
    let output_path = output_dir.join(graphics_ass_file_name(element));
    render_subtitle_template_file(template_path, &output_path, variables)?;
    let input = InputFile::graphics_subtitle(crate::subtitle_graphics_lavfi_input_at(
        &output_path.to_string_lossy(),
        context.frame_size,
        None,
        context.stream_offset,
    ));
    Ok(Some(RenderedGraphicsElement::NativeImage {
        id: element.id,
        z_index: element.z_index,
        input,
        overlay: graphics_subtitle_overlay(element),
    }))
}

fn render_script_graphics_element(
    element: &GraphicsElement,
    config: &ersatzrs_domain::ScriptGraphicsConfig,
    variables: &BTreeMap<String, String>,
    output_dir: &Path,
    context: &GraphicsRenderContext,
) -> Result<Option<RenderedGraphicsElement>, GraphicsTemplateError> {
    let command = config
        .command
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or(GraphicsTemplateError::MissingField {
            id: element.id,
            field: "script command",
        })?;
    let output_path = output_dir.join(graphics_script_drawvg_file_name(element));
    let script = render_script_drawvg(element, config, variables, command, context)?;
    std::fs::write(&output_path, script).map_err(|source| GraphicsTemplateError::Io {
        path: output_path.clone(),
        source,
    })?;
    Ok(Some(RenderedGraphicsElement::Drawvg {
        id: element.id,
        z_index: element.z_index,
        script_path: output_path.clone(),
        overlay: DrawvgOverlay {
            script_path: output_path,
            x: 0,
            y: 0,
            width: context.frame_size.width,
            height: context.frame_size.height,
            enable: Some(graphics_script_enable_expression(config)),
        },
    }))
}

fn bounded_graphics_lua(id: i64) -> Result<Lua, GraphicsTemplateError> {
    // ADRs 0005/0114: scripts produce native drawing commands, never processes.
    let lua = Lua::new_with(
        mlua::StdLib::TABLE
            | mlua::StdLib::STRING
            | mlua::StdLib::MATH
            | mlua::StdLib::UTF8
            | mlua::StdLib::COROUTINE
            | mlua::StdLib::OS,
        mlua::LuaOptions::default(),
    )
    .map_err(|source| GraphicsTemplateError::Script { id, source })?;
    let source = lua
        .globals()
        .get::<Table>("os")
        .map_err(|source| GraphicsTemplateError::Script { id, source })?;
    let clocks = lua
        .create_table()
        .map_err(|source| GraphicsTemplateError::Script { id, source })?;
    for name in ["date", "time", "clock", "difftime"] {
        let function = source
            .get::<Value>(name)
            .map_err(|source| GraphicsTemplateError::Script { id, source })?;
        clocks
            .set(name, function)
            .map_err(|source| GraphicsTemplateError::Script { id, source })?;
    }
    lua.globals()
        .set("os", clocks)
        .map_err(|source| GraphicsTemplateError::Script { id, source })?;
    for name in ["dofile", "loadfile"] {
        lua.globals()
            .set(name, Value::Nil)
            .map_err(|source| GraphicsTemplateError::Script { id, source })?;
    }
    lua.set_memory_limit(16 * 1024 * 1024)
        .map_err(|source| GraphicsTemplateError::Script { id, source })?;
    let instructions = std::sync::atomic::AtomicUsize::new(0);
    lua.set_global_hook(
        mlua::HookTriggers::new().every_nth_instruction(10_000),
        move |_, _| {
            if instructions.fetch_add(10_000, std::sync::atomic::Ordering::Relaxed) >= 1_000_000 {
                return Err(mlua::Error::RuntimeError(
                    "graphics script instruction limit exceeded".into(),
                ));
            }
            Ok(mlua::VmState::Continue)
        },
    )
    .map_err(|source| GraphicsTemplateError::Script { id, source })?;
    Ok(lua)
}

fn render_script_drawvg(
    element: &GraphicsElement,
    config: &ersatzrs_domain::ScriptGraphicsConfig,
    variables: &BTreeMap<String, String>,
    command: &str,
    context: &GraphicsRenderContext,
) -> Result<String, GraphicsTemplateError> {
    let lua = bounded_graphics_lua(element.id)?;
    let globals = lua.globals();
    let variables_table =
        lua_table_from_map(&lua, variables).map_err(|source| GraphicsTemplateError::Script {
            id: element.id,
            source,
        })?;
    let args_table = lua_table_from_args(&lua, &config.args).map_err(|source| {
        GraphicsTemplateError::Script {
            id: element.id,
            source,
        }
    })?;
    let frame_table =
        lua_frame_table(&lua, context).map_err(|source| GraphicsTemplateError::Script {
            id: element.id,
            source,
        })?;
    let element_table = lua_element_table(&lua, element, config).map_err(|source| {
        GraphicsTemplateError::Script {
            id: element.id,
            source,
        }
    })?;
    globals
        .set("variables", variables_table)
        .map_err(|source| GraphicsTemplateError::Script {
            id: element.id,
            source,
        })?;
    globals
        .set("args", args_table)
        .map_err(|source| GraphicsTemplateError::Script {
            id: element.id,
            source,
        })?;
    globals
        .set("frame", frame_table)
        .map_err(|source| GraphicsTemplateError::Script {
            id: element.id,
            source,
        })?;
    globals
        .set("element", element_table)
        .map_err(|source| GraphicsTemplateError::Script {
            id: element.id,
            source,
        })?;

    let source = script_lua_source(command).map_err(|source| GraphicsTemplateError::Io {
        path: PathBuf::from(command),
        source,
    })?;
    let value =
        lua.load(&source)
            .eval::<Value>()
            .map_err(|source| GraphicsTemplateError::Script {
                id: element.id,
                source,
            })?;
    script_drawvg_from_lua_value(element.id, value)
}

fn lua_table_from_map(lua: &Lua, values: &BTreeMap<String, String>) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    for (key, value) in values {
        table.set(key.as_str(), value.as_str())?;
    }
    Ok(table)
}

fn lua_table_from_args(lua: &Lua, args: &[String]) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    for (index, value) in args.iter().enumerate() {
        table.set(index + 1, value.as_str())?;
    }
    Ok(table)
}

fn lua_frame_table(lua: &Lua, context: &GraphicsRenderContext) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("width", context.frame_size.width)?;
    table.set("height", context.frame_size.height)?;
    table.set("scaled_width", context.square_pixel_frame_size.width)?;
    table.set("scaled_height", context.square_pixel_frame_size.height)?;
    Ok(table)
}

fn lua_element_table(
    lua: &Lua,
    element: &GraphicsElement,
    config: &ersatzrs_domain::ScriptGraphicsConfig,
) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("id", element.id)?;
    table.set("name", element.name.as_str())?;
    table.set("z_index", element.z_index)?;
    if let Some(start) = config.start_seconds {
        table.set("start_seconds", start)?;
    }
    if let Some(duration) = config.duration_seconds {
        table.set("duration_seconds", duration)?;
    }
    Ok(table)
}

fn script_lua_source(command: &str) -> std::io::Result<String> {
    use std::io::Read as _;
    const MAX_SOURCE_BYTES: usize = 64 * 1024;
    let path = Path::new(command);
    let source = if path.is_file() {
        let mut source = String::new();
        std::fs::File::open(path)?
            .take((MAX_SOURCE_BYTES + 1) as u64)
            .read_to_string(&mut source)?;
        source
    } else {
        command.to_owned()
    };
    if source.len() > MAX_SOURCE_BYTES {
        Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "graphics script exceeds 64 KiB",
        ))
    } else {
        Ok(source)
    }
}

fn script_drawvg_from_lua_value(
    element_id: i64,
    value: Value,
) -> Result<String, GraphicsTemplateError> {
    match value {
        Value::String(value) => value
            .to_str()
            .map(|value| value.to_owned())
            .map_err(|source| GraphicsTemplateError::Script {
                id: element_id,
                source,
            }),
        Value::Table(table) => table
            .get::<String>("drawvg")
            .or_else(|_| table.get::<String>("script"))
            .map_err(|source| GraphicsTemplateError::Script {
                id: element_id,
                source,
            }),
        _ => Err(GraphicsTemplateError::InvalidScriptResult { id: element_id }),
    }
}

fn render_subtitle_template_file(
    template_path: &str,
    output_path: &Path,
    variables: &BTreeMap<String, String>,
) -> Result<(), GraphicsTemplateError> {
    let template_path = Path::new(template_path);
    let template =
        std::fs::read_to_string(template_path).map_err(|source| GraphicsTemplateError::Io {
            path: template_path.to_path_buf(),
            source,
        })?;
    let rendered = SvgTemplate::new(template).render_raw(variables);
    std::fs::write(output_path, rendered).map_err(|source| GraphicsTemplateError::Io {
        path: output_path.to_path_buf(),
        source,
    })
}

fn text_drawvg_for_element(
    element: &GraphicsElement,
    variables: &BTreeMap<String, String>,
    context: &GraphicsRenderContext,
) -> Result<String, GraphicsTemplateError> {
    let GraphicsElementConfig::Text(config) = &element.config else {
        return Err(GraphicsTemplateError::MissingField {
            id: element.id,
            field: "text config",
        });
    };
    let text = config
        .text
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or(GraphicsTemplateError::MissingField {
            id: element.id,
            field: "text",
        })?;
    Ok(text_drawvg(element, config, variables, text, context))
}

fn text_drawvg(
    element: &GraphicsElement,
    config: &ersatzrs_domain::TextGraphicsConfig,
    variables: &BTreeMap<String, String>,
    text: &str,
    context: &GraphicsRenderContext,
) -> String {
    let text = SvgTemplate::new(text).render(variables);
    let style = config
        .styles
        .iter()
        .find(|style| style.name.as_deref() == config.base_style.as_deref())
        .or_else(|| config.styles.first());
    let font_size = f64::from(style.and_then(|style| style.font_size).unwrap_or(48.0));
    let fill = style
        .and_then(|style| style.text_color.as_deref())
        .unwrap_or("white");
    let (x, y, align) = drawvg_text_position(element, config.align, context);
    let max_width = config
        .width_percent
        .map(|percent| percent / 100.0 * f64::from(context.frame_size.width));
    let opacity = static_opacity(element);
    let color = drawvg_color(fill, &opacity);
    let lines = text.lines().collect::<Vec<_>>();
    let cell = (font_size / 7.0).max(1.0);
    let glyph_width = cell * 5.0;
    let glyph_gap = cell;
    let line_height = font_size * 1.2;
    let scale = if config.fit == TextFit::Scale {
        max_width.map_or(1.0, |width| {
            lines
                .iter()
                .map(|line| text_line_width(line, glyph_width, glyph_gap))
                .fold(1.0_f64, |scale, line_width| {
                    if line_width > width && line_width > 0.0 {
                        scale.min(width / line_width)
                    } else {
                        scale
                    }
                })
        })
    } else {
        1.0
    };
    let cell = cell * scale;
    let glyph_width = glyph_width * scale;
    let glyph_gap = glyph_gap * scale;
    let line_height = line_height * scale;

    let mut script = String::from("// ErsatzRS generated text graphics\n");
    let _ = writeln!(script, "setcolor {color}");
    for (line_index, line) in lines.iter().enumerate() {
        let line_width = text_line_width(line, glyph_width, glyph_gap);
        let line_x = match align {
            TextAlignment::Left => x,
            TextAlignment::Center => x - line_width / 2.0,
            TextAlignment::Right => x - line_width,
        };
        let line_y = y + (f64_from_usize(line_index) * line_height);
        append_drawvg_text_line(&mut script, line, line_x, line_y, cell, glyph_gap);
    }
    script
}

fn drawvg_text_position(
    element: &GraphicsElement,
    align: TextAlignment,
    context: &GraphicsRenderContext,
) -> (f64, f64, TextAlignment) {
    if let (Some(x), Some(y)) = (element.location_x, element.location_y) {
        return (x, y, align);
    }

    let (horizontal_margin, vertical_margin) = margins(element, context);
    let x_margin = parse_percent(&horizontal_margin) / 100.0 * f64::from(context.frame_size.width);
    let y_margin = parse_percent(&vertical_margin) / 100.0 * f64::from(context.frame_size.height);
    match element.location {
        WatermarkLocation::TopLeft
        | WatermarkLocation::LeftMiddle
        | WatermarkLocation::BottomLeft => (x_margin, y_margin, TextAlignment::Left),
        WatermarkLocation::TopMiddle
        | WatermarkLocation::MiddleCenter
        | WatermarkLocation::BottomMiddle => (
            f64::from(context.frame_size.width) / 2.0,
            y_margin,
            TextAlignment::Center,
        ),
        WatermarkLocation::TopRight
        | WatermarkLocation::RightMiddle
        | WatermarkLocation::BottomRight => (
            f64::from(context.frame_size.width) - x_margin,
            y_margin,
            TextAlignment::Right,
        ),
    }
}

fn margins(element: &GraphicsElement, context: &GraphicsRenderContext) -> (String, String) {
    if element.place_within_source_content {
        let horizontal_padding = context
            .frame_size
            .width
            .saturating_sub(context.square_pixel_frame_size.width);
        let vertical_padding = context
            .frame_size
            .height
            .saturating_sub(context.square_pixel_frame_size.height);
        return (
            format_percentage(
                element.horizontal_margin_percent.unwrap_or(0.0)
                    * f64::from(context.square_pixel_frame_size.width)
                    / f64::from(context.frame_size.width)
                    + f64::from(horizontal_padding) * 50.0 / f64::from(context.frame_size.width),
            ),
            format_percentage(
                element.vertical_margin_percent.unwrap_or(0.0)
                    * f64::from(context.square_pixel_frame_size.height)
                    / f64::from(context.frame_size.height)
                    + f64::from(vertical_padding) * 50.0 / f64::from(context.frame_size.height),
            ),
        );
    }

    (
        format_percentage(element.horizontal_margin_percent.unwrap_or(0.0)),
        format_percentage(element.vertical_margin_percent.unwrap_or(0.0)),
    )
}

fn drawvg_color(color: &str, opacity: &str) -> String {
    let color = color.trim();
    if color.contains('@') || opacity == "1.00" {
        color.to_owned()
    } else {
        format!("{color}@{opacity}")
    }
}

fn text_line_width(line: &str, glyph_width: f64, glyph_gap: f64) -> f64 {
    let count = line.chars().count();
    if count == 0 {
        0.0
    } else {
        f64_from_usize(count) * glyph_width + f64_from_usize(count.saturating_sub(1)) * glyph_gap
    }
}

fn append_drawvg_text_line(
    script: &mut String,
    line: &str,
    x: f64,
    y: f64,
    cell: f64,
    glyph_gap: f64,
) {
    let glyph_width = cell * 5.0;
    for (char_index, ch) in line.chars().enumerate() {
        let glyph_x = x + f64_from_usize(char_index) * (glyph_width + glyph_gap);
        append_drawvg_glyph(script, ch, glyph_x, y, cell);
    }
}

fn append_drawvg_glyph(script: &mut String, ch: char, x: f64, y: f64, cell: f64) {
    let pattern = glyph_pattern(ch);
    for (row, bits) in pattern.iter().enumerate() {
        for (column, bit) in bits.chars().enumerate() {
            if bit != '1' {
                continue;
            }
            let _ = writeln!(
                script,
                "rect {} {} {} {} fill",
                trim_float(x + f64_from_usize(column) * cell),
                trim_float(y + f64_from_usize(row) * cell),
                trim_float(cell),
                trim_float(cell),
            );
        }
    }
}

type GlyphPattern = [&'static str; 7];

const QUESTION_GLYPH: GlyphPattern = [
    "01110", "10001", "00001", "00010", "00100", "00000", "00100",
];

const GLYPHS: &[(char, GlyphPattern)] = &[
    (
        'A',
        [
            "01110", "10001", "10001", "11111", "10001", "10001", "10001",
        ],
    ),
    (
        'B',
        [
            "11110", "10001", "10001", "11110", "10001", "10001", "11110",
        ],
    ),
    (
        'C',
        [
            "01111", "10000", "10000", "10000", "10000", "10000", "01111",
        ],
    ),
    (
        'D',
        [
            "11110", "10001", "10001", "10001", "10001", "10001", "11110",
        ],
    ),
    (
        'E',
        [
            "11111", "10000", "10000", "11110", "10000", "10000", "11111",
        ],
    ),
    (
        'F',
        [
            "11111", "10000", "10000", "11110", "10000", "10000", "10000",
        ],
    ),
    (
        'G',
        [
            "01111", "10000", "10000", "10111", "10001", "10001", "01111",
        ],
    ),
    (
        'H',
        [
            "10001", "10001", "10001", "11111", "10001", "10001", "10001",
        ],
    ),
    (
        'I',
        [
            "11111", "00100", "00100", "00100", "00100", "00100", "11111",
        ],
    ),
    (
        'J',
        [
            "00111", "00010", "00010", "00010", "10010", "10010", "01100",
        ],
    ),
    (
        'K',
        [
            "10001", "10010", "10100", "11000", "10100", "10010", "10001",
        ],
    ),
    (
        'L',
        [
            "10000", "10000", "10000", "10000", "10000", "10000", "11111",
        ],
    ),
    (
        'M',
        [
            "10001", "11011", "10101", "10101", "10001", "10001", "10001",
        ],
    ),
    (
        'N',
        [
            "10001", "11001", "10101", "10011", "10001", "10001", "10001",
        ],
    ),
    (
        'O',
        [
            "01110", "10001", "10001", "10001", "10001", "10001", "01110",
        ],
    ),
    (
        'P',
        [
            "11110", "10001", "10001", "11110", "10000", "10000", "10000",
        ],
    ),
    (
        'Q',
        [
            "01110", "10001", "10001", "10001", "10101", "10010", "01101",
        ],
    ),
    (
        'R',
        [
            "11110", "10001", "10001", "11110", "10100", "10010", "10001",
        ],
    ),
    (
        'S',
        [
            "01111", "10000", "10000", "01110", "00001", "00001", "11110",
        ],
    ),
    (
        'T',
        [
            "11111", "00100", "00100", "00100", "00100", "00100", "00100",
        ],
    ),
    (
        'U',
        [
            "10001", "10001", "10001", "10001", "10001", "10001", "01110",
        ],
    ),
    (
        'V',
        [
            "10001", "10001", "10001", "10001", "10001", "01010", "00100",
        ],
    ),
    (
        'W',
        [
            "10001", "10001", "10001", "10101", "10101", "10101", "01010",
        ],
    ),
    (
        'X',
        [
            "10001", "10001", "01010", "00100", "01010", "10001", "10001",
        ],
    ),
    (
        'Y',
        [
            "10001", "10001", "01010", "00100", "00100", "00100", "00100",
        ],
    ),
    (
        'Z',
        [
            "11111", "00001", "00010", "00100", "01000", "10000", "11111",
        ],
    ),
    (
        '0',
        [
            "01110", "10001", "10011", "10101", "11001", "10001", "01110",
        ],
    ),
    (
        '1',
        [
            "00100", "01100", "00100", "00100", "00100", "00100", "01110",
        ],
    ),
    (
        '2',
        [
            "01110", "10001", "00001", "00010", "00100", "01000", "11111",
        ],
    ),
    (
        '3',
        [
            "11110", "00001", "00001", "01110", "00001", "00001", "11110",
        ],
    ),
    (
        '4',
        [
            "00010", "00110", "01010", "10010", "11111", "00010", "00010",
        ],
    ),
    (
        '5',
        [
            "11111", "10000", "10000", "11110", "00001", "00001", "11110",
        ],
    ),
    (
        '6',
        [
            "01110", "10000", "10000", "11110", "10001", "10001", "01110",
        ],
    ),
    (
        '7',
        [
            "11111", "00001", "00010", "00100", "01000", "01000", "01000",
        ],
    ),
    (
        '8',
        [
            "01110", "10001", "10001", "01110", "10001", "10001", "01110",
        ],
    ),
    (
        '9',
        [
            "01110", "10001", "10001", "01111", "00001", "00001", "01110",
        ],
    ),
    (
        ' ',
        [
            "00000", "00000", "00000", "00000", "00000", "00000", "00000",
        ],
    ),
    (
        '&',
        [
            "01100", "10010", "10100", "01000", "10101", "10010", "01101",
        ],
    ),
    (
        '/',
        [
            "00001", "00010", "00010", "00100", "01000", "01000", "10000",
        ],
    ),
    (
        '<',
        [
            "00010", "00100", "01000", "10000", "01000", "00100", "00010",
        ],
    ),
    (
        '>',
        [
            "01000", "00100", "00010", "00001", "00010", "00100", "01000",
        ],
    ),
    (
        '-',
        [
            "00000", "00000", "00000", "11111", "00000", "00000", "00000",
        ],
    ),
    (
        '_',
        [
            "00000", "00000", "00000", "00000", "00000", "00000", "11111",
        ],
    ),
    (
        ':',
        [
            "00000", "00100", "00100", "00000", "00100", "00100", "00000",
        ],
    ),
    (
        '.',
        [
            "00000", "00000", "00000", "00000", "00000", "01100", "01100",
        ],
    ),
    (
        ',',
        [
            "00000", "00000", "00000", "00000", "00100", "00100", "01000",
        ],
    ),
    ('?', QUESTION_GLYPH),
];

fn glyph_pattern(ch: char) -> GlyphPattern {
    GLYPHS
        .iter()
        .find_map(|(glyph, pattern)| (*glyph == ch.to_ascii_uppercase()).then_some(*pattern))
        .unwrap_or(QUESTION_GLYPH)
}

fn f64_from_usize(value: usize) -> f64 {
    f64::from(u32::try_from(value).unwrap_or(u32::MAX))
}

fn graphics_enable_expression(element: &GraphicsElement) -> Option<String> {
    match &element.config {
        GraphicsElementConfig::Motion(config) => {
            Some(graphics_motion_enable_expression(config, None, 0.0))
        }
        _ => None,
    }
}

fn graphics_motion_enable_expression(
    config: &ersatzrs_domain::MotionGraphicsConfig,
    source_duration_seconds: Option<f64>,
    offset: f64,
) -> String {
    let start = config.start_seconds.unwrap_or(0.0).max(0.0);
    let time = if offset > 0.0 {
        format!("(t+{})", trim_float(offset))
    } else {
        "t".into()
    };
    match config.end_behavior {
        MotionEndBehavior::Disappear
            if source_duration_seconds.is_some_and(|duration| duration > 0.0) =>
        {
            format!(
                "gte({time},{})*lt({time},{})",
                trim_float(start),
                trim_float(start + source_duration_seconds.unwrap_or_default())
            )
        }
        MotionEndBehavior::Disappear | MotionEndBehavior::Loop => {
            format!("gte({time},{})", trim_float(start))
        }
        MotionEndBehavior::Hold => {
            let source_duration = source_duration_seconds.unwrap_or_default().max(0.0);
            let hold_duration = config.hold_seconds.unwrap_or_default().max(0.0);
            let end = start + source_duration + hold_duration;
            if end <= start {
                return format!("gte({time},{})", trim_float(start));
            }
            format!(
                "gte({time},{})*lt({time},{})",
                trim_float(start),
                trim_float(end)
            )
        }
    }
}

fn graphics_script_enable_expression(config: &ersatzrs_domain::ScriptGraphicsConfig) -> String {
    let start = config.start_seconds.unwrap_or_default().max(0.0);
    let Some(duration) = config.duration_seconds.filter(|value| *value > 0.0) else {
        return format!("gte(t,{})", trim_float(start));
    };
    format!(
        "between(t,{},{})",
        trim_float(start),
        trim_float(start + duration)
    )
}

fn static_opacity(element: &GraphicsElement) -> String {
    if element
        .opacity_expression
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
    {
        return "1".to_owned();
    }

    format!(
        "{:.2}",
        f64::from(element.opacity_percent.unwrap_or(100).clamp(0, 100)) / 100.0
    )
}

fn graphics_drawvg_file_name(element: &GraphicsElement) -> String {
    let stem = element
        .path
        .as_deref()
        .and_then(|path| Path::new(path).file_stem())
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.trim().is_empty())
        .unwrap_or("graphics-element");
    format!("{}-{}.vgs", element.id, sanitize_file_stem(stem))
}

fn graphics_ass_file_name(element: &GraphicsElement) -> String {
    let stem = element
        .path
        .as_deref()
        .and_then(|path| Path::new(path).file_stem())
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.trim().is_empty())
        .unwrap_or("graphics-subtitle");
    format!("{}-{}.ass", element.id, sanitize_file_stem(stem))
}

fn graphics_script_drawvg_file_name(element: &GraphicsElement) -> String {
    let stem = element
        .path
        .as_deref()
        .and_then(|path| Path::new(path).file_stem())
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.trim().is_empty())
        .unwrap_or("graphics-script");
    format!("{}-{}.vgs", element.id, sanitize_file_stem(stem))
}

fn sanitize_file_stem(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect()
}

fn format_percentage(value: f64) -> String {
    format!("{}%", trim_float(value.clamp(0.0, 100.0)))
}

fn parse_percent(value: &str) -> f64 {
    value.trim_end_matches('%').parse::<f64>().unwrap_or(0.0)
}

fn trim_float(value: f64) -> String {
    let rounded = format!("{value:.3}");
    rounded
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_owned()
}

fn escape_filter_value(value: &str) -> String {
    value.replace('\'', "\\'")
}

fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
