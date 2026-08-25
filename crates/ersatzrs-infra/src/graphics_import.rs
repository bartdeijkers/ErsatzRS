//! ErsatzTV graphics-element YAML import.
//!
//! Source map:
//! - `$ERSATZTV_SRC/ErsatzTV.Core/Graphics/*GraphicsElement.cs` defines the YAML
//!   fields and `YamlMember` aliases.
//! - `$ERSATZTV_SRC/ErsatzTV.Application/Graphics/Commands/RefreshGraphicsElementsHandler.cs`
//!   discovers files by kind folder and persists `Path`/`Kind`/`Name`.
//! - ErsatzRS persists the fully populated import in `GraphicsElement` columns plus
//!   the kind-specific `Config` JSON field.

use ersatzrs_core::repositories::{GraphicsRepository, RepositoryError, Result};
use ersatzrs_domain::{
    EntityId, GraphicsElement, GraphicsElementConfig, GraphicsElementKind, ImageGraphicsConfig,
    MotionEndBehavior, MotionGraphicsConfig, ScriptGraphicsConfig, StyleDefinition,
    SubtitleGraphicsConfig, TextAlignment, TextFit, TextGraphicsConfig, WatermarkLocation,
};
use serde::Deserialize;

/// Parse an ErsatzTV graphics-element YAML document into the canonical ErsatzRS
/// database/domain shape. `path` is retained as the source link for future refreshes.
///
/// # Errors
///
/// Returns [`RepositoryError::Decode`] when the YAML does not match the upstream
/// graphics-element shape for `kind`.
pub fn graphics_element_from_yaml(
    id: EntityId,
    kind: GraphicsElementKind,
    path: impl Into<String>,
    yaml: &str,
) -> Result<GraphicsElement> {
    let path = path.into();
    match kind {
        GraphicsElementKind::Image => {
            let element: ImageYaml = deserialize_yaml(yaml)?;
            Ok(element.into_graphics_element(id, path))
        }
        GraphicsElementKind::Text => {
            let element: TextYaml = deserialize_yaml(yaml)?;
            Ok(element.into_graphics_element(id, path))
        }
        GraphicsElementKind::Subtitle => {
            let element: SubtitleYaml = deserialize_yaml(yaml)?;
            Ok(element.into_graphics_element(id, path))
        }
        GraphicsElementKind::Motion => {
            let element: MotionYaml = deserialize_yaml(yaml)?;
            Ok(element.into_graphics_element(id, path))
        }
        GraphicsElementKind::Script => {
            let element: ScriptYaml = deserialize_yaml(yaml)?;
            Ok(element.into_graphics_element(id, path))
        }
    }
}

/// Parse and upsert one YAML document through the graphics repository.
///
/// # Errors
///
/// Returns [`RepositoryError::Decode`] when YAML parsing fails, or any error returned
/// by [`GraphicsRepository::upsert_element`].
pub async fn upsert_graphics_element_from_yaml<R>(
    repository: &R,
    id: EntityId,
    kind: GraphicsElementKind,
    path: impl Into<String>,
    yaml: &str,
) -> Result<GraphicsElement>
where
    R: GraphicsRepository + Sync,
{
    let element = graphics_element_from_yaml(id, kind, path, yaml)?;
    repository.upsert_element(&element).await?;
    Ok(element)
}

fn deserialize_yaml<T>(yaml: &str) -> Result<T>
where
    T: for<'de> Deserialize<'de>,
{
    serde_yaml::from_str(yaml).map_err(|error| RepositoryError::Decode {
        kind: "graphics element yaml",
        value: error.to_string(),
    })
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ImageYaml {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    image: Option<String>,
    #[serde(default, alias = "opacityPercent")]
    opacity_percent: Option<i32>,
    #[serde(default, alias = "opacityExpression")]
    opacity_expression: Option<String>,
    #[serde(default)]
    location: WatermarkLocation,
    #[serde(default, alias = "horizontalMarginPercent")]
    horizontal_margin_percent: Option<f64>,
    #[serde(default, alias = "verticalMarginPercent")]
    vertical_margin_percent: Option<f64>,
    #[serde(default, alias = "locationX")]
    location_x: Option<f64>,
    #[serde(default, alias = "locationY")]
    location_y: Option<f64>,
    #[serde(default, alias = "zIndex")]
    z_index: Option<i32>,
    #[serde(default)]
    scale: bool,
    #[serde(default, alias = "scaleWidthPercent")]
    scale_width_percent: Option<f64>,
    #[serde(default, alias = "placeWithinSourceContent")]
    place_within_source_content: bool,
}

impl ImageYaml {
    fn into_graphics_element(self, id: EntityId, path: String) -> GraphicsElement {
        GraphicsElement {
            id,
            name: self.name.unwrap_or_default(),
            kind: GraphicsElementKind::Image,
            path: Some(path),
            z_index: self.z_index.unwrap_or_default(),
            opacity_percent: self.opacity_percent,
            opacity_expression: self.opacity_expression,
            location: self.location,
            horizontal_margin_percent: self.horizontal_margin_percent,
            vertical_margin_percent: self.vertical_margin_percent,
            location_x: self.location_x,
            location_y: self.location_y,
            scale: self.scale,
            scale_width_percent: self.scale_width_percent,
            place_within_source_content: self.place_within_source_content,
            config: GraphicsElementConfig::Image(ImageGraphicsConfig { image: self.image }),
            ..GraphicsElement::default()
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TextYaml {
    #[serde(default)]
    name: Option<String>,
    #[serde(default, alias = "opacityPercent")]
    opacity_percent: Option<i32>,
    #[serde(default, alias = "opacityExpression")]
    opacity_expression: Option<String>,
    #[serde(default)]
    location: WatermarkLocation,
    #[serde(default, alias = "horizontalMarginPercent")]
    horizontal_margin_percent: Option<f64>,
    #[serde(default, alias = "verticalMarginPercent")]
    vertical_margin_percent: Option<f64>,
    #[serde(default, alias = "locationX")]
    location_x: Option<f64>,
    #[serde(default, alias = "locationY")]
    location_y: Option<f64>,
    #[serde(default, alias = "widthPercent")]
    width_percent: Option<f64>,
    #[serde(default, alias = "textFit")]
    text_fit: TextFit,
    #[serde(default, alias = "textAlign")]
    text_align: TextAlignment,
    #[serde(default, alias = "zIndex")]
    z_index: Option<i32>,
    #[serde(default)]
    styles: Vec<StyleDefinition>,
    #[serde(default, alias = "baseStyle")]
    base_style: Option<String>,
    #[serde(default, alias = "includeFontsFrom")]
    include_fonts_from: Option<String>,
    #[serde(default, alias = "epgEntries")]
    epg_entries: i32,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    renderer: ersatzrs_domain::TextGraphicsRenderer,
    #[serde(default)]
    drawtext: ersatzrs_domain::DrawtextGraphicsConfig,
}

impl TextYaml {
    fn into_graphics_element(self, id: EntityId, path: String) -> GraphicsElement {
        GraphicsElement {
            id,
            name: self.name.unwrap_or_default(),
            kind: GraphicsElementKind::Text,
            path: Some(path),
            z_index: self.z_index.unwrap_or_default(),
            opacity_percent: self.opacity_percent,
            opacity_expression: self.opacity_expression,
            location: self.location,
            horizontal_margin_percent: self.horizontal_margin_percent,
            vertical_margin_percent: self.vertical_margin_percent,
            location_x: self.location_x,
            location_y: self.location_y,
            epg_entries: self.epg_entries,
            config: GraphicsElementConfig::Text(TextGraphicsConfig {
                text: self.text,
                renderer: self.renderer,
                drawtext: self.drawtext,
                fit: self.text_fit,
                align: self.text_align,
                width_percent: self.width_percent,
                base_style: self.base_style,
                include_fonts_from: self.include_fonts_from,
                styles: self.styles,
            }),
            ..GraphicsElement::default()
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SubtitleYaml {
    #[serde(default)]
    name: Option<String>,
    #[serde(default, alias = "zIndex")]
    z_index: Option<i32>,
    #[serde(default, alias = "epgEntries")]
    epg_entries: i32,
    #[serde(default)]
    template: Option<String>,
}

impl SubtitleYaml {
    fn into_graphics_element(self, id: EntityId, path: String) -> GraphicsElement {
        GraphicsElement {
            id,
            name: self.name.unwrap_or_default(),
            kind: GraphicsElementKind::Subtitle,
            path: Some(path),
            z_index: self.z_index.unwrap_or_default(),
            epg_entries: self.epg_entries,
            config: GraphicsElementConfig::Subtitle(SubtitleGraphicsConfig {
                template: self.template,
            }),
            ..GraphicsElement::default()
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MotionYaml {
    #[serde(default)]
    name: Option<String>,
    #[serde(default, alias = "videoPath")]
    video_path: Option<String>,
    #[serde(default, alias = "startSeconds")]
    start_seconds: Option<f64>,
    #[serde(default, alias = "endBehavior")]
    end_behavior: MotionEndBehavior,
    #[serde(default, alias = "holdSeconds")]
    hold_seconds: Option<f64>,
    #[serde(default)]
    location: WatermarkLocation,
    #[serde(default, alias = "horizontalMarginPercent")]
    horizontal_margin_percent: Option<f64>,
    #[serde(default, alias = "verticalMarginPercent")]
    vertical_margin_percent: Option<f64>,
    #[serde(default, alias = "zIndex")]
    z_index: Option<i32>,
    #[serde(default, alias = "epgEntries")]
    epg_entries: i32,
    #[serde(default)]
    scale: bool,
    #[serde(default, alias = "scaleWidthPercent")]
    scale_width_percent: Option<f64>,
}

impl MotionYaml {
    fn into_graphics_element(self, id: EntityId, path: String) -> GraphicsElement {
        GraphicsElement {
            id,
            name: self.name.unwrap_or_default(),
            kind: GraphicsElementKind::Motion,
            path: Some(path),
            z_index: self.z_index.unwrap_or_default(),
            location: self.location,
            horizontal_margin_percent: self.horizontal_margin_percent,
            vertical_margin_percent: self.vertical_margin_percent,
            scale: self.scale,
            scale_width_percent: self.scale_width_percent,
            epg_entries: self.epg_entries,
            config: GraphicsElementConfig::Motion(MotionGraphicsConfig {
                video_path: self.video_path,
                start_seconds: self.start_seconds,
                hold_seconds: self.hold_seconds,
                end_behavior: self.end_behavior,
            }),
            ..GraphicsElement::default()
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ScriptYaml {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    command: Option<String>,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default, alias = "zIndex")]
    z_index: Option<i32>,
    #[serde(default, alias = "epgEntries")]
    epg_entries: i32,
    #[serde(default, alias = "startSeconds")]
    start_seconds: Option<f64>,
    #[serde(default, alias = "durationSeconds")]
    duration_seconds: Option<f64>,
    #[serde(default, alias = "pixelFormat")]
    pixel_format: Option<String>,
    #[serde(default)]
    format: Option<ScriptGraphicsFormat>,
}

impl ScriptYaml {
    fn into_graphics_element(self, id: EntityId, path: String) -> GraphicsElement {
        GraphicsElement {
            id,
            name: self.name.unwrap_or_default(),
            kind: GraphicsElementKind::Script,
            path: Some(path),
            z_index: self.z_index.unwrap_or_default(),
            epg_entries: self.epg_entries,
            config: GraphicsElementConfig::Script(ScriptGraphicsConfig {
                command: self.command,
                args: self.args,
                format: self.format.map(|format| format.as_str().to_owned()),
                pixel_format: self.pixel_format,
                start_seconds: self.start_seconds,
                duration_seconds: self.duration_seconds,
            }),
            ..GraphicsElement::default()
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
enum ScriptGraphicsFormat {
    Raw,
    Packet,
}

impl ScriptGraphicsFormat {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Raw => "Raw",
            Self::Packet => "Packet",
        }
    }
}
