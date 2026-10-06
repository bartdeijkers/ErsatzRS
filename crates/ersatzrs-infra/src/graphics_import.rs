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
    let (yaml, shared) = extract_shared_fields(yaml)?;
    let mut element = match kind {
        GraphicsElementKind::Image => {
            let element: ImageYaml = deserialize_yaml(&yaml)?;
            element.into_graphics_element(id, path)
        }
        GraphicsElementKind::Text => {
            let element: TextYaml = deserialize_yaml(&yaml)?;
            element.into_graphics_element(id, path)
        }
        GraphicsElementKind::Subtitle => {
            let element: SubtitleYaml = deserialize_yaml(&yaml)?;
            element.into_graphics_element(id, path)
        }
        GraphicsElementKind::Motion => {
            let element: MotionYaml = deserialize_yaml(&yaml)?;
            element.into_graphics_element(id, path)
        }
        GraphicsElementKind::Script => {
            let element: ScriptYaml = deserialize_yaml(&yaml)?;
            element.into_graphics_element(id, path)
        }
    };
    if let serde_json::Value::Object(mut value) =
        serde_json::to_value(&element).map_err(yaml_error)?
    {
        value.extend(shared);
        element = serde_json::from_value(serde_json::Value::Object(value)).map_err(yaml_error)?;
    }
    Ok(element)
}

/// Export the native flat YAML shape using the same parser on reimport.
///
/// # Errors
/// Returns serialization errors or invalid authored values.
pub fn graphics_element_to_yaml(element: &GraphicsElement) -> Result<String> {
    ersatzrs_core::repositories::validate_authored_graphics(element)?;
    let mut value = serde_json::to_value(element).map_err(yaml_error)?;
    let fields = value
        .as_object_mut()
        .ok_or_else(|| yaml_error("invalid graphics object"))?;
    for key in ["id", "kind", "path"] {
        fields.remove(key);
    }
    let config = fields
        .remove("config")
        .ok_or_else(|| yaml_error("missing config"))?;
    if let serde_json::Value::Object(mut config) = config {
        config.remove("kind");
        fields.extend(config);
    }
    serde_yaml::to_string(&value).map_err(yaml_error)
}

fn yaml_error(error: impl std::fmt::Display) -> RepositoryError {
    RepositoryError::Decode {
        kind: "graphics element yaml",
        value: error.to_string(),
    }
}

fn extract_shared_fields(
    yaml: &str,
) -> Result<(String, serde_json::Map<String, serde_json::Value>)> {
    if yaml.len() > 256 * 1024 || yaml.contains('\0') {
        return Err(yaml_error("graphics-authoring-invalid"));
    }
    let mut value: serde_yaml::Value = serde_yaml::from_str(yaml).map_err(yaml_error)?;
    let fields = value
        .as_mapping_mut()
        .ok_or_else(|| yaml_error("expected a YAML mapping"))?;
    if let Some(styles) = fields.get(serde_yaml::Value::String("styles".into())) {
        let template = serde_yaml::to_value(StyleDefinition::default()).map_err(yaml_error)?;
        for style in styles
            .as_sequence()
            .ok_or_else(|| yaml_error("expected styles sequence"))?
        {
            reject_unknown_nested_fields(style, &template)?;
        }
    }
    if let Some(drawtext) = fields.get(serde_yaml::Value::String("drawtext".into())) {
        let template = serde_yaml::to_value(ersatzrs_domain::DrawtextGraphicsConfig {
            metadata_refresh_interval_seconds: Some(0),
            ..Default::default()
        })
        .map_err(yaml_error)?;
        reject_unknown_nested_fields(drawtext, &template)?;
    }
    let mut shared = serde_json::Map::new();
    for (key, alias) in [
        ("z_index", "zIndex"),
        ("epg_entries", "epgEntries"),
        ("opacity_percent", "opacityPercent"),
        ("opacity_expression", "opacityExpression"),
        ("location", "location"),
        ("location_x", "locationX"),
        ("location_y", "locationY"),
        ("horizontal_margin_percent", "horizontalMarginPercent"),
        ("vertical_margin_percent", "verticalMarginPercent"),
        ("scale", "scale"),
        ("scale_width_percent", "scaleWidthPercent"),
        ("place_within_source_content", "placeWithinSourceContent"),
    ] {
        let canonical = serde_yaml::Value::String(key.into());
        let alternate = serde_yaml::Value::String(alias.into());
        let raw = fields.remove(&canonical);
        let alternate = if key == alias {
            None
        } else {
            fields.remove(&alternate)
        };
        if raw.is_some() && alternate.is_some() {
            return Err(yaml_error("duplicate graphics field"));
        }
        if let Some(raw) = raw.or(alternate) {
            shared.insert(key.into(), serde_json::to_value(raw).map_err(yaml_error)?);
        }
    }
    Ok((serde_yaml::to_string(&value).map_err(yaml_error)?, shared))
}

fn reject_unknown_nested_fields(
    value: &serde_yaml::Value,
    template: &serde_yaml::Value,
) -> Result<()> {
    let supplied = value
        .as_mapping()
        .ok_or_else(|| yaml_error("expected graphics settings mapping"))?;
    let known = template
        .as_mapping()
        .ok_or_else(|| yaml_error("invalid graphics settings shape"))?;
    if supplied.keys().any(|key| !known.contains_key(key)) {
        return Err(yaml_error("unknown graphics settings field"));
    }
    Ok(())
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
    #[serde(default, alias = "epgEntries")]
    epg_entries: i32,
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
            epg_entries: self.epg_entries,
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
