use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::Row;

use crate::{
    admin::{
        i18n::{media_action_error_text, UiLanguage},
        server_fns::{
            media_browser::get_media_item_info_json_with_pool, settings::load_ui_language_for_api,
        },
    },
    host::state::AppState,
};

const MOVIE_COLLECTION_TYPE: i32 = 10;
const PLAYBACK_ORDER_SHUFFLE: i32 = 1;

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
    let rows = sqlx::query(
        r#"SELECT "Id", COALESCE("Name", '') AS "Name"
           FROM "Collection"
           ORDER BY "Name" COLLATE NOCASE"#,
    )
    .fetch_all(&state.pool)
    .await
    .map_err(|error| ApiError::database(language, &error))?;

    Ok(Json(
        rows.iter()
            .map(|row| CollectionOptionDto {
                id: row.try_get("Id").unwrap_or(0),
                name: row.try_get("Name").unwrap_or_default(),
            })
            .collect(),
    ))
}

pub async fn add_to_collection(
    State(state): State<AppState>,
    Json(input): Json<AddToCollectionRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let language = load_error_language(&state).await;
    ensure_media_item_exists(&state, input.media_item_id, language).await?;
    let collection_id = match input.collection_id {
        Some(id) if id > 0 => {
            ensure_collection_exists(&state, id, language).await?;
            id
        }
        _ => create_collection(&state, input.new_name.unwrap_or_default(), language).await?,
    };

    // Default add: idempotent per (collection, media item, no-override) row;
    // ADR 0048 duplicate rows are created only via the override add flow.
    let result = sqlx::query(
        r#"INSERT INTO "CollectionItem" ("CollectionId", "MediaItemId")
           SELECT ?, ?
           WHERE NOT EXISTS (
               SELECT 1 FROM "CollectionItem"
               WHERE "CollectionId" = ? AND "MediaItemId" = ?
                 AND "PreferredAudioLanguageCode" IS NULL
                 AND "PreferredAudioTitle" IS NULL
                 AND "PreferredSubtitleLanguageCode" IS NULL
                 AND "SubtitleMode" IS NULL
           )"#,
    )
    .bind(collection_id)
    .bind(input.media_item_id)
    .bind(collection_id)
    .bind(input.media_item_id)
    .execute(&state.pool)
    .await
    .map_err(|error| ApiError::database(language, &error))?;
    if result.rows_affected() > 0 {
        enqueue_collection_playout_refresh(&state, collection_id).await?;
    }

    Ok(Json(json!({ "id": collection_id })))
}

pub async fn list_playlist_groups(
    State(state): State<AppState>,
) -> Result<Json<Vec<PlaylistGroupOptionDto>>, ApiError> {
    let language = load_error_language(&state).await;
    let rows = sqlx::query(
        r#"SELECT "Id", COALESCE("Name", '') AS "Name"
           FROM "PlaylistGroup"
           ORDER BY COALESCE("IsSystem", 0) DESC, "Name" COLLATE NOCASE"#,
    )
    .fetch_all(&state.pool)
    .await
    .map_err(|error| ApiError::database(language, &error))?;

    Ok(Json(
        rows.iter()
            .map(|row| PlaylistGroupOptionDto {
                id: row.try_get("Id").unwrap_or(0),
                name: row.try_get("Name").unwrap_or_default(),
            })
            .collect(),
    ))
}

pub async fn list_playlists(
    State(state): State<AppState>,
    Query(query): Query<PlaylistQuery>,
) -> Result<Json<Vec<PlaylistOptionDto>>, ApiError> {
    let language = load_error_language(&state).await;
    let rows = sqlx::query(
        r#"SELECT "Id", COALESCE("Name", '') AS "Name"
           FROM "Playlist"
           WHERE "PlaylistGroupId" = ?
           ORDER BY "Name" COLLATE NOCASE"#,
    )
    .bind(query.group_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|error| ApiError::database(language, &error))?;

    Ok(Json(
        rows.iter()
            .map(|row| PlaylistOptionDto {
                id: row.try_get("Id").unwrap_or(0),
                name: row.try_get("Name").unwrap_or_default(),
            })
            .collect(),
    ))
}

pub async fn add_to_playlist(
    State(state): State<AppState>,
    Json(input): Json<AddToPlaylistRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let language = load_error_language(&state).await;
    ensure_media_item_exists(&state, input.media_item_id, language).await?;
    ensure_playlist_exists(&state, input.playlist_id, language).await?;

    let max_index: Option<i64> =
        sqlx::query_scalar(r#"SELECT MAX("Index") FROM "PlaylistItem" WHERE "PlaylistId" = ?"#)
            .bind(input.playlist_id)
            .fetch_one(&state.pool)
            .await
            .map_err(|error| ApiError::database(language, &error))?;
    let next_index = max_index.map_or(0, |index| index + 1);

    sqlx::query(
        r#"INSERT INTO "PlaylistItem"
               ("Index", "PlaylistId", "CollectionType", "MediaItemId",
                "IncludeInProgramGuide", "PlaybackOrder", "PlayAll")
           VALUES (?, ?, ?, ?, 1, ?, 0)"#,
    )
    .bind(next_index)
    .bind(input.playlist_id)
    .bind(MOVIE_COLLECTION_TYPE)
    .bind(input.media_item_id)
    .bind(PLAYBACK_ORDER_SHUFFLE)
    .execute(&state.pool)
    .await
    .map_err(|error| ApiError::database(language, &error))?;

    Ok(Json(json!({ "id": input.playlist_id })))
}

pub async fn media_info(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<String, ApiError> {
    let language = load_error_language(&state).await;
    get_media_item_info_json_with_pool(&state.pool, id)
        .await
        .map_err(|error| ApiError::bad_request(language, error.to_string()))
}

async fn create_collection(
    state: &AppState,
    name: String,
    language: UiLanguage,
) -> Result<i64, ApiError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(ApiError::bad_request(
            language,
            "Collection name is required",
        ));
    }
    if trimmed.chars().count() > 50 {
        return Err(ApiError::bad_request(
            language,
            "Collection name must be 50 characters or less",
        ));
    }

    let duplicate: i64 =
        sqlx::query_scalar(r#"SELECT COUNT(*) FROM "Collection" WHERE "Name" = ? COLLATE NOCASE"#)
            .bind(trimmed)
            .fetch_one(&state.pool)
            .await
            .map_err(|error| ApiError::database(language, &error))?;
    if duplicate > 0 {
        return Err(ApiError::bad_request(
            language,
            "Collection name must be unique",
        ));
    }

    let result =
        sqlx::query(r#"INSERT INTO "Collection" ("Name", "UseCustomPlaybackOrder") VALUES (?, 0)"#)
            .bind(trimmed)
            .execute(&state.pool)
            .await
            .map_err(|error| ApiError::database(language, &error))?;
    Ok(ersatzrs_infra::db::last_insert_id(result))
}

async fn ensure_media_item_exists(
    state: &AppState,
    id: i64,
    language: UiLanguage,
) -> Result<(), ApiError> {
    ensure_exists(
        state,
        r#"SELECT COUNT(*) FROM "MediaItem" WHERE "Id" = ?"#,
        id,
        "Media item does not exist",
        language,
    )
    .await
}

async fn ensure_collection_exists(
    state: &AppState,
    id: i64,
    language: UiLanguage,
) -> Result<(), ApiError> {
    ensure_exists(
        state,
        r#"SELECT COUNT(*) FROM "Collection" WHERE "Id" = ?"#,
        id,
        "Collection does not exist",
        language,
    )
    .await
}

async fn ensure_playlist_exists(
    state: &AppState,
    id: i64,
    language: UiLanguage,
) -> Result<(), ApiError> {
    ensure_exists(
        state,
        r#"SELECT COUNT(*) FROM "Playlist" WHERE "Id" = ?"#,
        id,
        "Playlist does not exist",
        language,
    )
    .await
}

async fn ensure_exists(
    state: &AppState,
    sql: &str,
    id: i64,
    missing_message: &'static str,
    language: UiLanguage,
) -> Result<(), ApiError> {
    let count: i64 = sqlx::query_scalar(ersatzrs_infra::db::assert_sql_safe(sql))
        .bind(id)
        .fetch_one(&state.pool)
        .await
        .map_err(|error| ApiError::database(language, &error))?;
    if count == 0 {
        return Err(ApiError::bad_request(language, missing_message));
    }
    Ok(())
}

async fn enqueue_collection_playout_refresh(
    state: &AppState,
    collection_id: i64,
) -> Result<(), ApiError> {
    let language = load_error_language(state).await;
    let playout_ids: Vec<i64> = sqlx::query_scalar(
        r#"SELECT DISTINCT "PlayoutId"
           FROM "PlayoutProgramScheduleAnchor"
           WHERE "CollectionId" = ?
           ORDER BY "PlayoutId""#,
    )
    .bind(collection_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|error| ApiError::database(language, &error))?;

    for playout_id in playout_ids {
        let request = ersatzrs_core::scheduling::PlayoutBuildRequest {
            playout_id,
            mode: ersatzrs_core::scheduling::PlayoutBuildMode::Refresh,
        };
        if let Err(error) = state.playout_build_tx.try_send(request) {
            tracing::warn!(
                playout_id,
                %error,
                "dropping requested playout refresh; periodic scheduler will retry later"
            );
        }
    }

    Ok(())
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

    fn database(language: UiLanguage, error: &sqlx::Error) -> Self {
        tracing::error!(%error, "media action API failed");
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: media_action_error_text(language, "Media action failed"),
        }
    }
}

async fn load_error_language(state: &AppState) -> UiLanguage {
    load_ui_language_for_api(&state.pool)
        .await
        .unwrap_or(UiLanguage::English)
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
