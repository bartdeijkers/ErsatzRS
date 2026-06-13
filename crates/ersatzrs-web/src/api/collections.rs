use axum::{
    extract::{Form, State},
    response::Redirect,
};
use serde::Deserialize;

use crate::{
    admin::server_fns::media_browser::{
        save_collection_with_state, save_smart_collection_with_state, CollectionForm,
    },
    host::state::AppState,
};

#[derive(Debug, Deserialize)]
pub struct CollectionSaveForm {
    id: i64,
    name: String,
}

#[derive(Debug, Deserialize)]
pub struct SmartCollectionSaveForm {
    id: i64,
    name: String,
    query: String,
}

pub async fn save_collection_form(
    State(state): State<AppState>,
    Form(input): Form<CollectionSaveForm>,
) -> Redirect {
    let id = input.id;
    let form = CollectionForm {
        id,
        name: input.name,
        query: String::new(),
    };

    match save_collection_with_state(&state, form).await {
        Ok(_) if id == 0 => Redirect::to("/media/collections"),
        Ok(saved_id) => Redirect::to(&format!("/media/collections/{saved_id}")),
        Err(error) => {
            tracing::error!(%error, collection_id = id, "failed to save collection from HTML form");
            let target = if id == 0 {
                "/media/collections/add?error=save".to_string()
            } else {
                format!("/media/collections/{id}/edit?error=save")
            };
            Redirect::to(&target)
        }
    }
}

pub async fn save_smart_collection_form(
    State(state): State<AppState>,
    Form(input): Form<SmartCollectionSaveForm>,
) -> Redirect {
    let id = input.id;
    let form = CollectionForm {
        id,
        name: input.name,
        query: input.query,
    };

    match save_smart_collection_with_state(&state, form).await {
        Ok(_) => Redirect::to("/media/smart-collections"),
        Err(error) => {
            tracing::error!(%error, smart_collection_id = id, "failed to save smart collection from HTML form");
            Redirect::to(&format!("/media/smart-collections/{id}/edit?error=save"))
        }
    }
}
