use axum::{
    extract::{Form, State},
    response::Redirect,
};
use serde::Deserialize;

use crate::host::state::AppState;

#[derive(Debug, Deserialize)]
pub struct LocalLibrarySaveForm {
    id: i64,
    name: String,
    media_kind: i32,
    #[serde(default)]
    paths_text: String,
}

pub async fn save_local_library_form(
    State(state): State<AppState>,
    Form(input): Form<LocalLibrarySaveForm>,
) -> Redirect {
    let paths = input
        .paths_text
        .lines()
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(|path| ersatzrs_core::repositories::LocalLibraryPathAdmin {
            id: 0,
            path: path.to_string(),
        })
        .collect();

    let form = ersatzrs_core::repositories::LocalLibraryAdmin {
        id: input.id,
        name: input.name,
        media_kind: input.media_kind,
        paths,
    };

    match state.application.save_local_library(form).await {
        Ok(_) => Redirect::to("/media/sources/local"),
        Err(error) => {
            tracing::error!(%error, "failed to save local library from HTML form");
            Redirect::to("/media/sources/local/add?error=save")
        }
    }
}
