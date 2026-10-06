//! Flat native fields map to the existing graphics domain; omitted fields retain saved values.
use ersatzrs_domain::{
    GraphicsElement, GraphicsElementConfig as C, GraphicsElementKind as K, ImageGraphicsConfig,
    MotionGraphicsConfig, ScriptGraphicsConfig, StyleDefinition, SubtitleGraphicsConfig,
    TextGraphicsConfig,
};
use std::collections::BTreeMap;
pub type Fields = BTreeMap<String, String>;
const INVALID: &str = "graphics-authoring-invalid";

#[must_use]
pub fn default_element() -> GraphicsElement {
    GraphicsElement {
        kind: K::Text,
        config: C::Text(TextGraphicsConfig::default()),
        ..Default::default()
    }
}

/// Decode an editor kind without substituting an unknown discriminator.
///
/// # Errors
/// Returns the validation code for unknown kinds.
pub fn kind(value: &str) -> Result<K, &'static str> {
    match value {
        "Image" => Ok(K::Image),
        "Text" => Ok(K::Text),
        "Subtitle" => Ok(K::Subtitle),
        "Motion" => Ok(K::Motion),
        "Script" => Ok(K::Script),
        _ => Err(INVALID),
    }
}

/// Update supplied native fields while preserving omitted persisted values.
///
/// # Errors
/// Rejects invalid numbers, booleans and enum values.
#[allow(
    clippy::too_many_lines,
    reason = "one exhaustive conversion keeps all five native kinds together"
)]
pub fn from_fields(
    fields: &Fields,
    mut element: GraphicsElement,
) -> Result<GraphicsElement, &'static str> {
    fn optional(fields: &Fields, key: &str, saved: Option<String>) -> Option<String> {
        fields.get(key).map_or(saved, |value| {
            (!value.trim().is_empty()).then(|| value.trim().to_owned())
        })
    }
    fn number<T: std::str::FromStr>(
        fields: &Fields,
        key: &str,
        saved: Option<T>,
    ) -> Result<Option<T>, &'static str> {
        fields.get(key).map_or(Ok(saved), |value| {
            if value.trim().is_empty() {
                Ok(None)
            } else {
                value.trim().parse().map(Some).map_err(|_| INVALID)
            }
        })
    }
    fn choice<T: serde::de::DeserializeOwned>(
        fields: &Fields,
        key: &str,
        saved: T,
    ) -> Result<T, &'static str> {
        fields.get(key).map_or(Ok(saved), |value| {
            serde_json::from_value(serde_json::Value::String(value.clone())).map_err(|_| INVALID)
        })
    }
    if let Some(value) = fields.get("kind") {
        let selected = kind(value)?;
        if selected != element.kind {
            element.kind = selected;
            element.config = match selected {
                K::Image => C::Image(ImageGraphicsConfig::default()),
                K::Text => C::Text(TextGraphicsConfig::default()),
                K::Subtitle => C::Subtitle(SubtitleGraphicsConfig::default()),
                K::Motion => C::Motion(MotionGraphicsConfig::default()),
                K::Script => C::Script(ScriptGraphicsConfig::default()),
            };
        }
    }
    if let Some(value) = fields.get("name") {
        value.trim().clone_into(&mut element.name);
    }
    element.z_index = number(fields, "z_index", Some(element.z_index))?.ok_or(INVALID)?;
    element.epg_entries =
        number(fields, "epg_entries", Some(element.epg_entries))?.ok_or(INVALID)?;
    element.opacity_percent = number(fields, "opacity_percent", element.opacity_percent)?;
    element.opacity_expression = optional(fields, "opacity_expression", element.opacity_expression);
    element.location = choice(fields, "location", element.location)?;
    element.location_x = number(fields, "location_x", element.location_x)?;
    element.location_y = number(fields, "location_y", element.location_y)?;
    element.horizontal_margin_percent = number(
        fields,
        "horizontal_margin_percent",
        element.horizontal_margin_percent,
    )?;
    element.vertical_margin_percent = number(
        fields,
        "vertical_margin_percent",
        element.vertical_margin_percent,
    )?;
    element.scale_width_percent =
        number(fields, "scale_width_percent", element.scale_width_percent)?;
    if let Some(value) = fields.get("scale") {
        element.scale = value.parse().map_err(|_| INVALID)?;
    }
    if let Some(value) = fields.get("place_within_source_content") {
        element.place_within_source_content = value.parse().map_err(|_| INVALID)?;
    }
    match &mut element.config {
        C::Image(c) => c.image = optional(fields, "image", c.image.take()),
        C::Subtitle(c) => c.template = optional(fields, "template", c.template.take()),
        C::Motion(c) => {
            c.video_path = optional(fields, "video_path", c.video_path.take());
            c.start_seconds = number(fields, "start_seconds", c.start_seconds)?;
            c.hold_seconds = number(fields, "hold_seconds", c.hold_seconds)?;
            c.end_behavior = choice(fields, "end_behavior", c.end_behavior)?;
        }
        C::Script(c) => {
            c.command = optional(fields, "command", c.command.take());
            c.start_seconds = number(fields, "script_start_seconds", c.start_seconds)?;
            c.duration_seconds = number(fields, "duration_seconds", c.duration_seconds)?;
            c.format = optional(fields, "format", c.format.take());
            c.pixel_format = optional(fields, "pixel_format", c.pixel_format.take());
            if let Some(value) = fields.get("args") {
                // One-per-line editing cannot distinguish an unchanged empty
                // argument from no arguments, or embedded newlines from two
                // arguments. Preserve the original vector when its display did
                // not change; YAML remains the exact vector representation.
                if *value != c.args.join("\n") {
                    c.args = if value.is_empty() {
                        Vec::new()
                    } else {
                        value.split('\n').map(str::to_owned).collect()
                    };
                }
            }
        }
        C::Text(c) => {
            c.text = fields.get("text").map_or(c.text.take(), |value| {
                (!value.trim().is_empty()).then(|| value.clone())
            });
            c.renderer = choice(fields, "renderer", c.renderer)?;
            c.drawtext.font_file = optional(fields, "font_file", c.drawtext.font_file.take());
            c.drawtext.refresh_interval_milliseconds = number(
                fields,
                "refresh_interval_milliseconds",
                Some(c.drawtext.refresh_interval_milliseconds),
            )?
            .ok_or(INVALID)?;
            c.drawtext.reload_interval_frames = number(
                fields,
                "reload_interval_frames",
                Some(c.drawtext.reload_interval_frames),
            )?
            .ok_or(INVALID)?;
            c.drawtext.metadata_refresh_interval_seconds = number(
                fields,
                "metadata_refresh_interval_seconds",
                c.drawtext.metadata_refresh_interval_seconds,
            )?;
            c.fit = choice(fields, "text_fit", c.fit)?;
            c.align = choice(fields, "text_align", c.align)?;
            c.width_percent = number(fields, "width_percent", c.width_percent)?;
            c.include_fonts_from =
                optional(fields, "include_fonts_from", c.include_fonts_from.take());
            c.base_style = optional(fields, "base_style", c.base_style.take());
            let index = c.styles.iter().position(|style| style.name == c.base_style);
            let mut style = index.map_or_else(
                || StyleDefinition {
                    name: c.base_style.clone(),
                    ..Default::default()
                },
                |i| c.styles[i].clone(),
            );
            style.font_size = number(fields, "font_size", style.font_size)?;
            style.font_family = optional(fields, "font_family", style.font_family);
            style.text_color = optional(fields, "text_color", style.text_color);
            style.line_height = number(fields, "line_height", style.line_height)?;
            style.halo_width = number(fields, "halo_width", style.halo_width)?;
            style.halo_color = optional(fields, "halo_color", style.halo_color);
            if let Some(index) = index {
                c.styles[index] = style;
            } else if style != StyleDefinition::default() {
                c.styles.push(style);
            }
        }
    }
    Ok(element)
}

/// Serialize the typed definition into native form field values.
///
/// # Errors
/// Returns the validation code if the domain stops serializing as an object.
pub fn to_fields(element: &GraphicsElement) -> Result<Fields, &'static str> {
    let mut fields = Fields::new();
    let serde_json::Value::Object(mut value) =
        serde_json::to_value(element).map_err(|_| INVALID)?
    else {
        return Err(INVALID);
    };
    let config = value.remove("config");
    if let Some(serde_json::Value::Object(config)) = config {
        value.extend(config.into_iter().filter(|(key, _)| key != "kind"));
    }
    for (key, value) in &value {
        fields.insert(
            key.clone(),
            match value {
                serde_json::Value::Null => String::new(),
                serde_json::Value::String(value) => value.clone(),
                _ => value.to_string(),
            },
        );
    }
    if let C::Text(c) = &element.config {
        fields.insert("text".into(), c.text.clone().unwrap_or_default());
        fields.insert(
            "font_file".into(),
            c.drawtext.font_file.clone().unwrap_or_default(),
        );
        fields.insert(
            "refresh_interval_milliseconds".into(),
            c.drawtext.refresh_interval_milliseconds.to_string(),
        );
        fields.insert(
            "reload_interval_frames".into(),
            c.drawtext.reload_interval_frames.to_string(),
        );
        fields.insert(
            "metadata_refresh_interval_seconds".into(),
            c.drawtext
                .metadata_refresh_interval_seconds
                .map(|value| value.to_string())
                .unwrap_or_default(),
        );
        let style = c
            .styles
            .iter()
            .find(|s| s.name == c.base_style)
            .cloned()
            .unwrap_or_default();
        let serde_json::Value::Object(style) = serde_json::to_value(style).map_err(|_| INVALID)?
        else {
            return Err(INVALID);
        };
        for (key, value) in &style {
            if key == "name" {
                continue;
            }
            fields.insert(
                key.clone(),
                match value {
                    serde_json::Value::Null => String::new(),
                    serde_json::Value::String(value) => value.clone(),
                    _ => value.to_string(),
                },
            );
        }
    }
    if let C::Script(c) = &element.config {
        fields.insert("args".into(), c.args.join("\n"));
        fields.insert(
            "script_start_seconds".into(),
            c.start_seconds.map(|v| v.to_string()).unwrap_or_default(),
        );
    }
    Ok(fields)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn authored_fields_preserve_unsubmitted_style_and_shared_values() {
        let mut element = default_element();
        element.name = "Before".into();
        element.location_x = Some(17.5);
        let C::Text(c) = &mut element.config else {
            unreachable!()
        };
        c.base_style = Some("heading".into());
        c.styles = vec![StyleDefinition {
            name: Some("heading".into()),
            font_size: Some(38.0),
            font_weight: Some(700),
            halo_color: Some("black".into()),
            ..Default::default()
        }];
        let changed = from_fields(
            &Fields::from([
                ("name".into(), "After".into()),
                ("text".into(), "Épisode".into()),
            ]),
            element.clone(),
        )
        .unwrap();
        assert_eq!(changed.location_x, Some(17.5));
        let C::Text(c) = changed.config else {
            unreachable!()
        };
        assert_eq!(c.styles.len(), 1);
        assert_eq!(c.styles[0].font_weight, Some(700));
        assert_eq!(c.styles[0].font_size, Some(38.0));
        let fields = to_fields(&element).unwrap();
        assert_eq!(from_fields(&fields, element.clone()).unwrap(), element);
    }
    #[test]
    fn authored_script_arguments_preserve_existing_empty_and_multiline_values() {
        for args in [
            vec![String::new()],
            vec!["first".into(), String::new()],
            vec!["first\nsecond".into()],
        ] {
            let mut element = default_element();
            element.kind = K::Script;
            element.config = C::Script(ScriptGraphicsConfig {
                args,
                ..Default::default()
            });
            assert_eq!(
                from_fields(&to_fields(&element).unwrap(), element.clone()).unwrap(),
                element
            );
        }
    }
    #[test]
    fn authored_kind_changes_are_typed_and_numeric_errors_rejected() {
        for name in ["Image", "Text", "Subtitle", "Motion", "Script"] {
            let fields = Fields::from([("kind".into(), name.into())]);
            let element = from_fields(&fields, default_element()).unwrap();
            assert_eq!(element.kind, element.config.kind());
            assert_eq!(element.kind, kind(name).unwrap());
        }
        for (field, value) in [
            ("kind", "Other"),
            ("z_index", "oops"),
            ("location", "West"),
            ("scale", "invalid"),
            ("place_within_source_content", "invalid"),
        ] {
            assert!(from_fields(
                &Fields::from([(field.into(), value.into())]),
                default_element()
            )
            .is_err());
        }
    }
}
