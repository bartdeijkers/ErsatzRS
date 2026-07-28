use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{
    admin::{
        i18n::{media_action_error_text, UiLanguage},
        server_fns::{
            media_browser::media_item_info_json_from_admin, settings::load_ui_language_for_api,
        },
    },
    host::state::AppState,
};

const MOVIE_COLLECTION_TYPE: i32 = 10;

#[derive(Debug, Serialize)]
pub struct CollectionOptionDto {
    id: i64,
    name: String,
}

#[derive(Debug, Serialize)]
pub struct PlaylistGroupOptionDto {
    id: i64,
    name: String,
}

#[derive(Debug, Serialize)]
pub struct PlaylistOptionDto {
    id: i64,
    name: String,
}

#[derive(Debug, Deserialize)]
pub struct PlaylistQuery {
    group_id: i64,
}

#[derive(Debug, Deserialize)]
pub struct AddToCollectionRequest {
    media_item_id: i64,
    #[serde(default)]
    collection_id: Option<i64>,
    #[serde(default)]
    new_name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AddToPlaylistRequest {
    media_item_id: i64,
    playlist_id: i64,
}

pub async fn list_collections(
    State(state): State<AppState>,
) -> Result<Json<Vec<CollectionOptionDto>>, ApiError> {
    let language = load_error_language(&state).await;
    let rows = state
        .application
        .list_all_collections()
        .await
        .map_err(|error| ApiError::application(language, &error))?;

    Ok(Json(
        rows.into_iter()
            .map(|row| CollectionOptionDto {
                id: row.id,
                name: row.name,
            })
            .collect(),
    ))
}

pub async fn add_to_collection(
    State(state): State<AppState>,
    Json(input): Json<AddToCollectionRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let language = load_error_language(&state).await;
    let collection_id = state
        .application
        .add_media_item_to_collection(ersatzrs_app::commands::AddMediaItemToCollection {
            media_item_id: input.media_item_id,
            collection_id: input.collection_id,
            new_name: input.new_name,
        })
        .await
        .map_err(|error| ApiError::application(language, &error))?;

    Ok(Json(json!({ "id": collection_id })))
}

pub async fn list_playlist_groups(
    State(state): State<AppState>,
) -> Result<Json<Vec<PlaylistGroupOptionDto>>, ApiError> {
    let language = load_error_language(&state).await;
    let groups = state
        .application
        .list_playlist_groups()
        .await
        .map_err(|error| ApiError::application(language, &error))?;

    Ok(Json(
        groups
            .into_iter()
            .map(|row| PlaylistGroupOptionDto {
                id: row.id,
                name: row.name,
            })
            .collect(),
    ))
}

pub async fn list_playlists(
    State(state): State<AppState>,
    Query(query): Query<PlaylistQuery>,
) -> Result<Json<Vec<PlaylistOptionDto>>, ApiError> {
    let language = load_error_language(&state).await;
    let groups = state
        .application
        .list_playlist_groups()
        .await
        .map_err(|error| ApiError::application(language, &error))?;
    let rows = groups
        .into_iter()
        .find(|group| group.id == query.group_id)
        .map(|group| group.playlists)
        .unwrap_or_default();

    Ok(Json(
        rows.into_iter()
            .map(|row| PlaylistOptionDto {
                id: row.id,
                name: row.name,
            })
            .collect(),
    ))
}

pub async fn add_to_playlist(
    State(state): State<AppState>,
    Json(input): Json<AddToPlaylistRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let language = load_error_language(&state).await;
    state
        .application
        .add_playlist_items(ersatzrs_app::commands::AddPlaylistItems {
            playlist_id: input.playlist_id,
            items: vec![ersatzrs_core::repositories::PlaylistItemInsert {
                collection_type: MOVIE_COLLECTION_TYPE,
                media_item_id: input.media_item_id,
                playback: ersatzrs_core::repositories::PlaybackOverrides::default(),
                pinned_stream_selection: None,
            }],
        })
        .await
        .map_err(|error| ApiError::application(language, &error))?;

    Ok(Json(json!({ "id": input.playlist_id })))
}

pub async fn media_info(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<String, ApiError> {
    let language = load_error_language(&state).await;
    let info = state
        .application
        .get_media_item_info(id)
        .await
        .map_err(|error| ApiError::application(language, &error))?
        .ok_or_else(|| ApiError::bad_request(language, "Unable to locate media item"))?;
    media_item_info_json_from_admin(info)
        .map_err(|error| ApiError::bad_request(language, error.to_string()))
}

#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn bad_request(language: UiLanguage, message: impl Into<String>) -> Self {
        let message = message.into();
        Self {
            status: StatusCode::BAD_REQUEST,
            message: media_action_error_text(language, &message),
        }
    }

    fn application(language: UiLanguage, error: &ersatzrs_app::ApplicationError) -> Self {
        match error {
            ersatzrs_app::ApplicationError::Validation { message, .. }
            | ersatzrs_app::ApplicationError::Conflict(message) => {
                Self::bad_request(language, message)
            }
            ersatzrs_app::ApplicationError::NotFound { kind, .. } => {
                Self::bad_request(language, format!("{kind} does not exist"))
            }
            _ => {
                tracing::error!(%error, "media action API failed");
                Self {
                    status: StatusCode::INTERNAL_SERVER_ERROR,
                    message: media_action_error_text(language, "Media action failed"),
                }
            }
        }
    }
}

async fn load_error_language(state: &AppState) -> UiLanguage {
    match load_ui_language_for_api(state.application.ports().admin_settings()).await {
        Ok(language) => language,
        Err(error) => {
            tracing::warn!(%error, "failed to load UI language for media actions API error");
            UiLanguage::English
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(json!({ "error": self.message }))).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn media_action_error_response_uses_selected_language() {
        let response =
            ApiError::bad_request(UiLanguage::Dutch, "Collection name is required").into_response();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body should collect");
        let body = String::from_utf8(bytes.to_vec()).expect("json should be utf8");

        assert_eq!(body, r#"{"error":"Collectienaam is verplicht."}"#);
    }

    #[tokio::test]
    async fn media_action_error_response_preserves_unknown_diagnostics() {
        let response = ApiError::bad_request(UiLanguage::PortugueseBrazil, "custom backend error")
            .into_response();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body should collect");
        let body = String::from_utf8(bytes.to_vec()).expect("json should be utf8");

        assert_eq!(body, r#"{"error":"custom backend error"}"#);
    }
}
