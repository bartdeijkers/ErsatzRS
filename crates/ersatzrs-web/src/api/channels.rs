use axum::{
    extract::{Form, State},
    response::Redirect,
};
use serde::Deserialize;

use crate::{
    admin::server_fns::channels::{save_channel_with_pool, ChannelForm},
    host::state::AppState,
};

#[derive(Debug, Deserialize)]
pub struct ChannelSaveForm {
    id: i64,
    number: String,
    name: String,
    #[serde(default)]
    group: String,
    ffmpeg_profile_id: i64,
    streaming_mode: i32,
    #[serde(default)]
    is_enabled: bool,
    #[serde(default)]
    show_in_epg: bool,
    #[serde(default)]
    fallback_filler_id: i64,
}

pub async fn save_channel_form(
    State(state): State<AppState>,
    Form(input): Form<ChannelSaveForm>,
) -> Redirect {
    let form = ChannelForm {
        id: input.id,
        number: input.number,
        name: input.name,
        group: input.group,
        ffmpeg_profile_id: input.ffmpeg_profile_id,
        streaming_mode: input.streaming_mode,
        is_enabled: input.is_enabled,
        show_in_epg: input.show_in_epg,
        fallback_filler_id: input.fallback_filler_id,
    };

    match save_channel_with_pool(&state.pool, form).await {
        Ok(_) => Redirect::to("/channels"),
        Err(error) => {
            tracing::error!(%error, "failed to save channel from HTML form");
            Redirect::to("/channels/add?error=save")
        }
    }
}
