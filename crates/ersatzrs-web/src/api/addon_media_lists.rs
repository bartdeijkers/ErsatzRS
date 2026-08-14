use axum::{
    extract::{Form, State},
    response::Redirect,
};
use serde::Deserialize;

use crate::{
    admin::server_fns::addon_media_lists::{
        add_addon_media_list_with_state, delete_addon_media_list_with_state,
        match_addon_media_list_with_state, update_addon_media_list_with_state, AddonMediaListForm,
    },
    host::state::AppState,
};

#[derive(Debug, Deserialize)]
pub struct AddonMediaListAddForm {
    addon_id: String,
    source_url: String,
}

#[derive(Debug, Deserialize)]
pub struct AddonMediaListIdForm {
    id: i64,
}

#[derive(Debug, Deserialize)]
pub struct AddonMediaListSaveForm {
    id: i64,
    addon_id: String,
    source_url: String,
    #[serde(default)]
    auto_refresh: bool,
    #[serde(default)]
    generate_playlist: bool,
}

pub async fn add_addon_media_list_form(
    State(state): State<AppState>,
    Form(input): Form<AddonMediaListAddForm>,
) -> Redirect {
    match add_addon_media_list_with_state(&state, input.addon_id, input.source_url).await {
        Ok(_) => Redirect::to("/media/addon-lists"),
        Err(error) => {
            tracing::error!(%error, "failed to add add-on media list from HTML form");
            Redirect::to("/media/addon-lists?error=add")
        }
    }
}

pub async fn match_addon_media_list_form(
    State(state): State<AppState>,
    Form(input): Form<AddonMediaListIdForm>,
) -> Redirect {
    match match_addon_media_list_with_state(&state, input.id).await {
        Ok(()) => Redirect::to("/media/addon-lists"),
        Err(error) => {
            tracing::error!(%error, addon_media_list_id = input.id, "failed to match add-on media list from HTML form");
            Redirect::to("/media/addon-lists?error=match")
        }
    }
}

pub async fn save_addon_media_list_form(
    State(state): State<AppState>,
    Form(input): Form<AddonMediaListSaveForm>,
) -> Redirect {
    let id = input.id;
    let form = AddonMediaListForm {
        id,
        addon_id: input.addon_id,
        source_url: input.source_url,
        auto_refresh: input.auto_refresh,
        generate_playlist: input.generate_playlist,
    };

    match update_addon_media_list_with_state(&state, form).await {
        Ok(()) => Redirect::to("/media/addon-lists"),
        Err(error) => {
            tracing::error!(%error, addon_media_list_id = id, "failed to save add-on media list from HTML form");
            Redirect::to(&format!("/media/addon-lists/{id}?error=save"))
        }
    }
}

pub async fn delete_addon_media_list_form(
    State(state): State<AppState>,
    Form(input): Form<AddonMediaListIdForm>,
) -> Redirect {
    match delete_addon_media_list_with_state(&state, input.id).await {
        Ok(()) => Redirect::to("/media/addon-lists"),
        Err(error) => {
            tracing::error!(%error, addon_media_list_id = input.id, "failed to delete add-on media list from HTML form");
            Redirect::to("/media/addon-lists?error=delete")
        }
    }
}

#[cfg(test)]
mod tests {
    use axum::body::Bytes;

    use super::*;

    #[test]
    fn save_form_defaults_missing_checkboxes_to_false() {
        let body = Bytes::from_static(
            b"id=42&addon_id=org.example.list&source_url=https%3A%2F%2Fexample.invalid%2Flist",
        );
        let parsed: AddonMediaListSaveForm = serde_urlencoded::from_bytes(&body).unwrap();
        assert_eq!(parsed.id, 42);
        assert_eq!(parsed.addon_id, "org.example.list");
        assert!(!parsed.auto_refresh);
        assert!(!parsed.generate_playlist);
    }
}
