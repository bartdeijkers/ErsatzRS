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
