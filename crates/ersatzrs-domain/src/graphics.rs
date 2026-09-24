use serde::{Deserialize, Serialize};

use crate::channel::WatermarkLocation;
use crate::EntityId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum GraphicsElementKind {
    #[default]
    Image,
    Text,
    Subtitle,
    Motion,
    Script,
}

/// A graphics element definition.
///
/// Mirrors ErsatzTV's `GraphicsElement` (`Id`/`Path`/`Name`/`Kind`) but, per the
/// ErsatzRS "DB is canonical" decision (Decision C), also persists the rich per-kind
/// configuration that ErsatzTV keeps in the YAML file at `path`. Shared fields are typed
/// columns; kind-specific extras live in [`GraphicsElement::config`], serialized to the
/// `Config` JSON column. `path` is retained as the link back to the source YAML so an
/// import can re-sync.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct GraphicsElement {
    pub id: EntityId,
    pub name: String,
    pub kind: GraphicsElementKind,
    /// Path to the source YAML the element was imported from, if any.
    pub path: Option<String>,
    pub z_index: i32,
    /// Static opacity, 0–100 (`Image`/`Text`).
    pub opacity_percent: Option<i32>,
    /// Dynamic opacity expression (`Image`/`Text`).
    pub opacity_expression: Option<String>,
    /// Anchor location (`Image`/`Text`/`Motion`); reuses the watermark location enum.
    pub location: WatermarkLocation,
    pub horizontal_margin_percent: Option<f64>,
    pub vertical_margin_percent: Option<f64>,
    /// Absolute placement override (`Image`).
    pub location_x: Option<f64>,
    pub location_y: Option<f64>,
    pub scale: bool,
    pub scale_width_percent: Option<f64>,
    pub place_within_source_content: bool,
    /// Number of EPG entries to expose to the template for any element kind.
    pub epg_entries: i32,
    /// Kind-specific configuration, persisted as JSON in the `Config` column.
    pub config: GraphicsElementConfig,
}

/// Kind-specific configuration, stored as JSON in `GraphicsElement.Config`.
///
/// Serialized internally tagged by `kind` so the JSON is self-describing; the
/// `GraphicsElement.kind` column is the denormalized discriminator and must agree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GraphicsElementConfig {
    Image(ImageGraphicsConfig),
    Text(TextGraphicsConfig),
    Subtitle(SubtitleGraphicsConfig),
    Motion(MotionGraphicsConfig),
    Script(ScriptGraphicsConfig),
}

impl Default for GraphicsElementConfig {
    fn default() -> Self {
        Self::Image(ImageGraphicsConfig::default())
    }
}

impl GraphicsElementConfig {
    /// The element kind this configuration represents.
    #[must_use]
    pub const fn kind(&self) -> GraphicsElementKind {
        match self {
            Self::Image(_) => GraphicsElementKind::Image,
            Self::Text(_) => GraphicsElementKind::Text,
            Self::Subtitle(_) => GraphicsElementKind::Subtitle,
            Self::Motion(_) => GraphicsElementKind::Motion,
            Self::Script(_) => GraphicsElementKind::Script,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ImageGraphicsConfig {
    /// Path to the image file to overlay.
    pub image: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct TextGraphicsConfig {
    pub text: Option<String>,
    /// Rendering backend. Missing persisted values remain on the historical
    /// drawvg path, so existing JSON needs no migration.
    pub renderer: TextGraphicsRenderer,
    /// Options used only when [`Self::renderer`] is [`TextGraphicsRenderer::Drawtext`].
    pub drawtext: DrawtextGraphicsConfig,
    #[serde(rename = "text_fit")]
    pub fit: TextFit,
    #[serde(rename = "text_align")]
    pub align: TextAlignment,
    pub width_percent: Option<f64>,
    pub base_style: Option<String>,
    pub include_fonts_from: Option<String>,
    pub styles: Vec<StyleDefinition>,
}

/// Backend used for text graphics elements.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TextGraphicsRenderer {
    /// Existing static VGS/drawvg rendering.
    #[default]
    Drawvg,
    /// Reloadable UTF-8 text-file rendering through `FFmpeg` drawtext.
    Drawtext,
}

/// Drawtext-specific controller and font options.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DrawtextGraphicsConfig {
    /// Exact font path. When absent, playback uses the packaged Roboto font.
    pub font_file: Option<String>,
    /// Controller refresh period. Values are bounded by the controller.
    pub refresh_interval_milliseconds: u64,
    /// `FFmpeg` text-file reload interval measured in frames.
    pub reload_interval_frames: u32,
}

impl Default for DrawtextGraphicsConfig {
    fn default() -> Self {
        Self {
            font_file: None,
            refresh_interval_milliseconds: 1_000,
            reload_interval_frames: 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct SubtitleGraphicsConfig {
    /// Path to the `.ass` subtitle template.
    pub template: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct MotionGraphicsConfig {
    pub video_path: Option<String>,
    pub start_seconds: Option<f64>,
    pub hold_seconds: Option<f64>,
    pub end_behavior: MotionEndBehavior,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ScriptGraphicsConfig {
    /// ErsatzTV external command field. ErsatzRS preserves the field for import fidelity but
    /// executes it as an in-process Lua script that returns a drawvg script; it never spawns an
    /// external graphics process (ADR 0005).
    pub command: Option<String>,
    pub args: Vec<String>,
    pub format: Option<String>,
    pub pixel_format: Option<String>,
    pub start_seconds: Option<f64>,
    pub duration_seconds: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TextFit {
    #[default]
    #[serde(alias = "none")]
    None,
    #[serde(alias = "wrap")]
    Wrap,
    #[serde(alias = "scale")]
    Scale,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TextAlignment {
    #[default]
    #[serde(alias = "left")]
    Left,
    #[serde(alias = "center")]
    Center,
    #[serde(alias = "right")]
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum MotionEndBehavior {
    #[default]
    #[serde(alias = "disappear")]
    Disappear,
    #[serde(alias = "loop")]
    Loop,
    #[serde(alias = "hold")]
    Hold,
}

/// A named text style, ported from ErsatzTV's `StyleDefinition`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct StyleDefinition {
    pub name: Option<String>,
    pub font_family: Option<String>,
    pub font_size: Option<f32>,
    pub font_weight: Option<i32>,
    pub font_italic: Option<bool>,
    pub text_color: Option<String>,
    pub letter_spacing: Option<f32>,
    pub line_height: Option<f32>,
    pub halo_color: Option<String>,
    pub halo_width: Option<f32>,
    pub halo_blur: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PlayoutItemGraphicsElement {
    pub playout_item_id: EntityId,
    pub graphics_element_id: Option<EntityId>,
    pub variables: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ProgramScheduleItemGraphicsElement {
    pub program_schedule_item_id: EntityId,
    pub graphics_element_id: EntityId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct BlockItemGraphicsElement {
    pub block_item_id: EntityId,
    pub graphics_element_id: EntityId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DecoGraphicsElement {
    pub deco_id: EntityId,
    pub graphics_element_id: EntityId,
}

#[cfg(test)]
mod tests {
    use super::{TextGraphicsConfig, TextGraphicsRenderer};

    #[test]
    fn legacy_text_graphics_json_defaults_to_drawvg() {
        let config: TextGraphicsConfig = serde_json::from_str(
            r#"{"text":"Now: {{ title }}","text_fit":"Wrap","text_align":"Center"}"#,
        )
        .expect("legacy text graphics JSON should remain readable");

        assert_eq!(config.renderer, TextGraphicsRenderer::Drawvg);
        assert_eq!(config.drawtext.refresh_interval_milliseconds, 1_000);
        assert_eq!(config.drawtext.reload_interval_frames, 1);
    }

    #[test]
    fn drawtext_renderer_round_trips_through_json() {
        let config = TextGraphicsConfig {
            renderer: TextGraphicsRenderer::Drawtext,
            ..TextGraphicsConfig::default()
        };

        let json = serde_json::to_string(&config).expect("drawtext config should serialize");
        let decoded: TextGraphicsConfig =
            serde_json::from_str(&json).expect("drawtext config should deserialize");

        assert_eq!(decoded, config);
    }
}
