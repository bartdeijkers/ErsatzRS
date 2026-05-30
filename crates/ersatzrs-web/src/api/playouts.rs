use axum::{
    extract::{Form, State},
    response::Redirect,
};
use serde::Deserialize;

use crate::{
    admin::server_fns::playouts::{create_playout_with_state, PlayoutCreateForm},
    host::state::AppState,
};

#[derive(Debug, Deserialize)]
pub struct PlayoutCreatePostForm {
    #[serde(default)]
    kind: String,
    channel_id: i64,
    #[serde(default)]
    program_schedule_id: Option<i64>,
    #[serde(default)]
    schedule_file: String,
}

pub async fn create_playout_form(
    State(state): State<AppState>,
    Form(input): Form<PlayoutCreatePostForm>,
) -> Redirect {
    let kind = input.kind.clone();
    let form = PlayoutCreateForm {
        kind: input.kind,
        channel_id: input.channel_id,
        program_schedule_id: input.program_schedule_id,
        schedule_file: input.schedule_file,
    };

    match create_playout_with_state(&state, form).await {
        Ok(_) => Redirect::to("/playouts"),
        Err(error) => {
            tracing::error!(%error, "failed to create playout from HTML form");
            let path = if kind.trim().is_empty() || kind.eq_ignore_ascii_case("classic") {
                "/playouts/add?error=create".to_owned()
            } else {
                format!("/playouts/add/{kind}?error=create")
            };
            Redirect::to(&path)
        }
    }
}
