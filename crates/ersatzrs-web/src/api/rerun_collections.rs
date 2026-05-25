use axum::{
    extract::{Form, State},
    response::Redirect,
};
use serde::Deserialize;

use crate::{
    admin::server_fns::media_browser::{
        delete_rerun_collection_with_pool, save_rerun_collection_with_state, RerunCollectionForm,
    },
    host::state::AppState,
};

#[derive(Debug, Deserialize)]
pub struct RerunCollectionSaveForm {
    id: i64,
    name: String,
    collection_type: i32,
    collection_id: Option<String>,
    media_item_id: Option<String>,
    multi_collection_id: Option<String>,
    smart_collection_id: Option<String>,
    first_run_playback_order: i32,
    rerun_playback_order: i32,
}

#[derive(Debug, Deserialize)]
pub struct RerunCollectionDeleteForm {
    id: i64,
}

pub async fn save_rerun_collection_form(
    State(state): State<AppState>,
    Form(input): Form<RerunCollectionSaveForm>,
) -> Redirect {
    let id = input.id;
    let form = RerunCollectionForm {
        id,
        name: input.name,
        collection_type: input.collection_type,
        collection_id: parse_i64(input.collection_id),
        media_item_id: parse_i64(input.media_item_id),
        multi_collection_id: parse_i64(input.multi_collection_id),
        smart_collection_id: parse_i64(input.smart_collection_id),
        first_run_playback_order: input.first_run_playback_order,
        rerun_playback_order: input.rerun_playback_order,
    };

    match save_rerun_collection_with_state(&state, form).await {
        Ok(_) => Redirect::to("/media/rerun-collections"),
        Err(error) => {
            tracing::error!(%error, rerun_collection_id = id, "failed to save rerun collection from HTML form");
            let target = if id == 0 {
                "/media/rerun-collections/add?error=save".to_string()
            } else {
                format!("/media/rerun-collections/{id}/edit?error=save")
            };
            Redirect::to(&target)
        }
    }
}

pub async fn delete_rerun_collection_form(
    State(state): State<AppState>,
    Form(input): Form<RerunCollectionDeleteForm>,
) -> Redirect {
    match delete_rerun_collection_with_pool(&state.pool, input.id).await {
        Ok(()) => Redirect::to("/media/rerun-collections"),
        Err(error) => {
            tracing::error!(%error, rerun_collection_id = input.id, "failed to delete rerun collection from HTML form");
            Redirect::to("/media/rerun-collections?error=delete")
        }
    }
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
    fn rerun_collection_parser_ignores_empty_optional_ids() {
        assert_eq!(parse_i64(None), None);
        assert_eq!(parse_i64(Some(String::new())), None);
        assert_eq!(parse_i64(Some("0".to_string())), None);
    }

    #[test]
    fn rerun_collection_parser_accepts_present_ids() {
        assert_eq!(parse_i64(Some("42".to_string())), Some(42));
    }
}
