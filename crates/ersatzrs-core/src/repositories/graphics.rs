use async_trait::async_trait;
use ersatzrs_domain::{EntityId, GraphicsElement};

use super::{RepositoryError, Result};

/// A read-only text preview at an explicit broadcast instant.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GraphicsTextPreviewRequest {
    pub channel_id: EntityId,
    pub element_id: Option<EntityId>,
    pub render_time: chrono::DateTime<chrono::Utc>,
    pub template: String,
    pub renderer: ersatzrs_domain::TextGraphicsRenderer,
    pub epg_entries: usize,
    pub width: u32,
    pub height: u32,
}

/// Text resolved with the native context; no playback producer is started.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GraphicsTextPreview {
    pub text: String,
    pub local_time: String,
    pub programme_context: serde_json::Value,
}

/// Unsaved definition rendered at an explicit programme instant.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GraphicsRenderPreviewRequest {
    pub element: GraphicsElement,
    pub channel_id: Option<EntityId>,
    pub seek_seconds: f64,
    pub render_time: chrono::DateTime<chrono::Utc>,
    pub width: u32,
    pub height: u32,
    /// Explicit preview overrides, separate from persisted attachment variables.
    pub variables: Option<String>,
}

/// Actual native canvas output, independent of channel delivery or producers.
#[derive(Debug, Clone)]
pub struct GraphicsRenderPreview {
    pub png: Vec<u8>,
    pub local_time: String,
}

/// Validate persisted geometry and kind-specific values before any side effect.
///
/// # Errors
/// Returns a static validation code for invalid or unbounded input.
pub fn validate_authored_graphics(element: &GraphicsElement) -> Result<()> {
    use ersatzrs_domain::GraphicsElementConfig as C;
    let invalid = || RepositoryError::Validation {
        field: "graphics element",
        message: "graphics-authoring-invalid".into(),
    };
    let bounded = |value: Option<f64>, minimum: f64, maximum: f64| {
        value.is_none_or(|n| n.is_finite() && (minimum..=maximum).contains(&n))
    };
    if element.id < 0
        || element.name.trim().is_empty()
        || element.name.chars().count() > 50
        || element.kind != element.config.kind()
        || !(0..=256).contains(&element.epg_entries)
        || !(-10000..=10000).contains(&element.z_index)
        || element
            .opacity_percent
            .is_some_and(|n| !(0..=100).contains(&n))
        || !bounded(element.horizontal_margin_percent, -100.0, 100.0)
        || !bounded(element.vertical_margin_percent, -100.0, 100.0)
        || !bounded(element.location_x, -8192.0, 8192.0)
        || !bounded(element.location_y, -8192.0, 8192.0)
        || !bounded(element.scale_width_percent, 0.01, 1000.0)
        || serde_json::to_vec(element).map_err(|_| invalid())?.len() > 256 * 1024
    {
        return Err(invalid());
    }
    let required = |value: Option<&str>| -> Result<()> {
        if value.is_none_or(|s| s.trim().is_empty() || s.len() > 64 * 1024 || s.contains('\0')) {
            return Err(invalid());
        }
        Ok(())
    };
    match &element.config {
        C::Image(c) => required(c.image.as_deref())?,
        C::Subtitle(c) => required(c.template.as_deref())?,
        C::Text(c) => {
            required(c.text.as_deref())?;
            if !bounded(c.width_percent, 0.01, 100.0)
                || !(250..=60000).contains(&c.drawtext.refresh_interval_milliseconds)
                || c.drawtext.reload_interval_frames == 0
                || c.drawtext
                    .metadata_refresh_interval_seconds
                    .is_some_and(|n| n != 0 && !(5..=60).contains(&n))
                || c.styles.len() > 32
                || c.styles.iter().any(|s| {
                    s.font_size
                        .is_some_and(|n| !n.is_finite() || !(1.0..=2048.0).contains(&n))
                })
            {
                return Err(invalid());
            }
        }
        C::Motion(c) => {
            required(c.video_path.as_deref())?;
            if !bounded(c.start_seconds, 0.0, 86400.0) || !bounded(c.hold_seconds, 0.0, 86400.0) {
                return Err(invalid());
            }
        }
        C::Script(c) => {
            required(c.command.as_deref())?;
            if !bounded(c.start_seconds, 0.0, 86400.0)
                || !bounded(c.duration_seconds, 0.001, 86400.0)
                || c.args.len() > 32
            {
                return Err(invalid());
            }
        }
    }
    Ok(())
}

/// Parse editor overrides keyed by selected graphics-element identity.
///
/// # Errors
/// Rejects malformed/non-object data or an aggregate larger than 256 KiB.
pub fn parse_graphics_attachment_variables(
    value: &str,
) -> Result<std::collections::BTreeMap<i64, String>> {
    if value.trim().is_empty() {
        return Ok(std::collections::BTreeMap::new());
    }
    if value.len() > 256 * 1024 {
        return Err(invalid_graphics_variables());
    }
    let values: std::collections::BTreeMap<i64, serde_json::Value> =
        serde_json::from_str(value).map_err(|_| invalid_graphics_variables())?;
    values
        .into_iter()
        .map(|(id, value)| {
            if id <= 0 || !value.is_object() {
                return Err(invalid_graphics_variables());
            }
            let value = serde_json::to_string(&value).map_err(|_| invalid_graphics_variables())?;
            if value.len() > 64 * 1024 {
                return Err(invalid_graphics_variables());
            }
            Ok((id, value))
        })
        .collect()
}

/// Validate explicit overrides without changing their occurrence or source order.
///
/// # Errors
/// Rejects overrides for unselected elements or values outside the object bounds.
pub fn validate_graphics_attachment_variables(
    ids: &[i64],
    variables: &std::collections::BTreeMap<i64, String>,
) -> Result<()> {
    if variables.values().map(String::len).sum::<usize>() > 256 * 1024 {
        return Err(invalid_graphics_variables());
    }
    for (id, value) in variables {
        if !ids.contains(id)
            || *id <= 0
            || value.len() > 64 * 1024
            || serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(value).is_err()
        {
            return Err(invalid_graphics_variables());
        }
    }
    Ok(())
}

fn invalid_graphics_variables() -> RepositoryError {
    RepositoryError::Decode {
        kind: "graphics attachment variables",
        value: "overrides must be bounded JSON objects for selected graphics elements".into(),
    }
}

#[async_trait]
pub trait GraphicsRepository: Send + Sync {
    async fn preview_text(
        &self,
        _request: &GraphicsTextPreviewRequest,
    ) -> Result<GraphicsTextPreview> {
        Err(RepositoryError::unavailable(
            "graphics text preview",
            "not supported by this adapter",
        ))
    }
    async fn parse_yaml(
        &self,
        _kind: ersatzrs_domain::GraphicsElementKind,
        _yaml: &str,
    ) -> Result<GraphicsElement> {
        Err(RepositoryError::unavailable(
            "graphics YAML",
            "not supported by this adapter",
        ))
    }
    async fn export_yaml(&self, _element: &GraphicsElement) -> Result<String> {
        Err(RepositoryError::unavailable(
            "graphics YAML",
            "not supported by this adapter",
        ))
    }
    async fn validate_authoring(&self, request: &GraphicsRenderPreviewRequest) -> Result<()> {
        validate_authored_graphics(&request.element)
    }
    async fn render_preview(
        &self,
        _request: &GraphicsRenderPreviewRequest,
    ) -> Result<GraphicsRenderPreview> {
        Err(RepositoryError::unavailable(
            "graphics rendered preview",
            "not supported by this adapter",
        ))
    }
    async fn list_elements(&self) -> Result<Vec<GraphicsElement>>;
    async fn get_element(&self, id: EntityId) -> Result<GraphicsElement>;
    async fn list_for_channel(&self, channel_id: EntityId) -> Result<Vec<GraphicsElement>>;
    async fn create_element(&self, element: &GraphicsElement) -> Result<EntityId>;
    async fn upsert_element(&self, element: &GraphicsElement) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authored_graphics_rejects_invalid_geometry_kind_and_timing() {
        use ersatzrs_domain::{GraphicsElementConfig, GraphicsElementKind, ImageGraphicsConfig};
        let valid = GraphicsElement {
            name: "Valid".into(),
            config: GraphicsElementConfig::Image(ImageGraphicsConfig {
                image: Some("logo.png".into()),
            }),
            ..Default::default()
        };
        assert!(validate_authored_graphics(&valid).is_ok());
        for invalid in [
            GraphicsElement {
                kind: GraphicsElementKind::Text,
                ..valid.clone()
            },
            GraphicsElement {
                location_x: Some(f64::NAN),
                ..valid.clone()
            },
            GraphicsElement {
                opacity_percent: Some(101),
                ..valid.clone()
            },
            GraphicsElement {
                epg_entries: 257,
                ..valid.clone()
            },
        ] {
            assert!(validate_authored_graphics(&invalid).is_err());
        }
    }

    #[test]
    fn graphics_attachment_variables_require_selected_bounded_objects() {
        let parsed =
            parse_graphics_attachment_variables(r#"{"7":{"headline":"A quoted \\\"title\\\""}}"#)
                .unwrap();
        assert!(validate_graphics_attachment_variables(&[7], &parsed).is_ok());
        assert!(validate_graphics_attachment_variables(&[8], &parsed).is_err());
        for malformed in ["[]", r#"{"7":[]}"#, r#"{"0":{}}"#, r#"{"-1":{}}"#, "{"] {
            assert!(parse_graphics_attachment_variables(malformed).is_err());
        }
        let oversized = serde_json::json!({"7": {"headline": "a".repeat(64 * 1024)}}).to_string();
        assert!(parse_graphics_attachment_variables(&oversized).is_err());
        let oversized: std::collections::BTreeMap<_, _> = (1..=5)
            .map(|id| {
                (
                    id,
                    serde_json::json!({"headline": "a".repeat(60 * 1024)}).to_string(),
                )
            })
            .collect();
        assert!(validate_graphics_attachment_variables(&[1, 2, 3, 4, 5], &oversized).is_err());
    }
}
