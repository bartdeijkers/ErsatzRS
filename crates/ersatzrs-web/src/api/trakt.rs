use axum::{
    extract::{Form, State},
    response::Redirect,
};
use serde::Deserialize;

use crate::{
    admin::server_fns::trakt::{
        add_trakt_list_with_pool, delete_trakt_list_with_pool, match_trakt_list_with_pool,
        update_trakt_list_with_pool, TraktListForm,
    },
    host::state::AppState,
};

#[derive(Debug, Deserialize)]
pub struct TraktListAddForm {
    url: String,
}

#[derive(Debug, Deserialize)]
pub struct TraktListIdForm {
    id: i64,
}

#[derive(Debug, Deserialize)]
pub struct TraktListSaveForm {
    id: i64,
    slug: String,
    #[serde(default)]
    auto_refresh: bool,
    #[serde(default)]
    generate_playlist: bool,
}

pub async fn add_trakt_list_form(
    State(state): State<AppState>,
    Form(input): Form<TraktListAddForm>,
) -> Redirect {
    match add_trakt_list_with_pool(&state.pool, input.url).await {
        Ok(_) => Redirect::to("/media/trakt/lists"),
        Err(error) => {
            tracing::error!(%error, "failed to add trakt list from HTML form");
            Redirect::to("/media/trakt/lists?error=add")
        }
    }
}

pub async fn match_trakt_list_form(
    State(state): State<AppState>,
    Form(input): Form<TraktListIdForm>,
) -> Redirect {
    match match_trakt_list_with_pool(&state.pool, input.id).await {
        Ok(()) => Redirect::to("/media/trakt/lists"),
        Err(error) => {
            tracing::error!(%error, trakt_list_id = input.id, "failed to match trakt list from HTML form");
            Redirect::to("/media/trakt/lists?error=match")
        }
    }
}

pub async fn save_trakt_list_form(
    State(state): State<AppState>,
    Form(input): Form<TraktListSaveForm>,
) -> Redirect {
    let id = input.id;
    let form = TraktListForm {
        id,
        slug: input.slug,
        auto_refresh: input.auto_refresh,
        generate_playlist: input.generate_playlist,
    };

    match update_trakt_list_with_pool(&state.pool, form).await {
        Ok(()) => Redirect::to("/media/trakt/lists"),
        Err(error) => {
            tracing::error!(%error, trakt_list_id = id, "failed to save trakt list from HTML form");
            Redirect::to(&format!("/media/trakt/lists/{id}?error=save"))
        }
    }
}

pub async fn delete_trakt_list_form(
    State(state): State<AppState>,
    Form(input): Form<TraktListIdForm>,
) -> Redirect {
    match delete_trakt_list_with_pool(&state.pool, input.id).await {
        Ok(()) => Redirect::to("/media/trakt/lists"),
        Err(error) => {
            tracing::error!(%error, trakt_list_id = input.id, "failed to delete trakt list from HTML form");
            Redirect::to("/media/trakt/lists?error=delete")
        }
    }
}

#[cfg(test)]
mod tests {
    use axum::body::Bytes;

    use super::*;

    #[test]
    fn save_form_defaults_missing_checkboxes_to_false() {
        let body = Bytes::from_static(b"id=42&slug=user%2Flist");
        let parsed: TraktListSaveForm = serde_urlencoded::from_bytes(&body).unwrap();
        assert_eq!(parsed.id, 42);
        assert_eq!(parsed.slug, "user/list");
        assert!(!parsed.auto_refresh);
        assert!(!parsed.generate_playlist);
    }
}
