use axum::{
    extract::{Form, State},
    response::Redirect,
};
use serde::Deserialize;

use crate::{
    admin::server_fns::filler::{filler_form_into_admin, FillerPresetForm},
    host::state::AppState,
};

#[derive(Debug, Deserialize)]
pub struct FillerPresetSaveForm {
    id: i64,
    name: String,
    filler_kind: i32,
    filler_mode: i32,
    #[serde(default)]
    duration_hours: Option<String>,
    #[serde(default)]
    duration_minutes: Option<String>,
    #[serde(default)]
    duration_seconds: Option<String>,
    count: Option<String>,
    pad_to_nearest_minute: Option<String>,
    #[serde(default)]
    allow_watermarks: bool,
    collection_type: i32,
    collection_id: Option<String>,
    media_item_id: Option<String>,
    multi_collection_id: Option<String>,
    smart_collection_id: Option<String>,
    playlist_id: Option<String>,
    #[serde(default)]
    expression: String,
    #[serde(default)]
    use_chapters_as_media_items: bool,
}

#[derive(Debug, Deserialize)]
pub struct FillerPresetDeleteForm {
    id: i64,
}

pub async fn save_filler_preset_form(
    State(state): State<AppState>,
    Form(input): Form<FillerPresetSaveForm>,
) -> Redirect {
    let id = input.id;
    let form = FillerPresetForm {
        id,
        name: input.name,
        filler_kind: input.filler_kind,
        filler_mode: input.filler_mode,
        duration_hours: parse_i32(input.duration_hours).unwrap_or_default(),
        duration_minutes: parse_i32(input.duration_minutes).unwrap_or_default(),
        duration_seconds: parse_i32(input.duration_seconds).unwrap_or_default(),
        count: parse_i32(input.count),
        pad_to_nearest_minute: parse_i32(input.pad_to_nearest_minute),
        allow_watermarks: input.allow_watermarks,
        collection_type: input.collection_type,
        collection_id: parse_i64(input.collection_id),
        media_item_id: parse_i64(input.media_item_id),
        multi_collection_id: parse_i64(input.multi_collection_id),
        smart_collection_id: parse_i64(input.smart_collection_id),
        playlist_id: parse_i64(input.playlist_id),
        expression: input.expression,
        use_chapters_as_media_items: input.use_chapters_as_media_items,
    };

    match state
        .application
        .save_filler_preset(ersatzrs_app::commands::SaveFillerPreset {
            form: filler_form_into_admin(form),
        })
        .await
    {
        Ok(_) => Redirect::to("/media/filler/presets"),
        Err(error) => {
            tracing::error!(%error, filler_preset_id = id, "failed to save filler preset from HTML form");
            let target = if id == 0 {
                "/media/filler/presets/add?error=save".to_string()
            } else {
                format!("/media/filler/presets/{id}/edit?error=save")
            };
            Redirect::to(&target)
        }
    }
}

pub async fn delete_filler_preset_form(
    State(state): State<AppState>,
    Form(input): Form<FillerPresetDeleteForm>,
) -> Redirect {
    match state
        .application
        .delete_filler_preset(ersatzrs_app::commands::DeleteFillerPreset { id: input.id })
        .await
    {
        Ok(()) => Redirect::to("/media/filler/presets"),
        Err(error) => {
            tracing::error!(%error, filler_preset_id = input.id, "failed to delete filler preset from HTML form");
            Redirect::to("/media/filler/presets?error=delete")
        }
    }
}

fn parse_i32(value: Option<String>) -> Option<i32> {
    value.and_then(|value| value.trim().parse().ok())
}

fn parse_i64(value: Option<String>) -> Option<i64> {
    value
        .and_then(|value| value.trim().parse().ok())
        .filter(|value| *value != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filler_save_parser_ignores_empty_disabled_numeric_fields() {
        assert_eq!(parse_i32(Some(String::new())), None);
        assert_eq!(parse_i32(Some("  ".to_string())), None);
        assert_eq!(parse_i64(Some(String::new())), None);
        assert_eq!(parse_i64(Some("0".to_string())), None);
    }

    #[test]
    fn filler_save_parser_accepts_present_numeric_fields() {
        assert_eq!(parse_i32(Some("30".to_string())), Some(30));
        assert_eq!(parse_i64(Some("42".to_string())), Some(42));
    }
}
