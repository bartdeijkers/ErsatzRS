use axum::{
    body::Body,
    extract::{Form, Multipart, Path, State},
    http::{header, StatusCode},
    response::{IntoResponse, Redirect, Response},
    Json,
};
use ersatzrs_core::repositories::{AddonMediaListArtworkRole, RepositoryError};
use ersatzrs_domain::{
    AddonMediaListContentKind, AddonMediaListPresentationKind, AddonMediaListReviewState,
};
use serde::Deserialize;
use serde_json::json;
use sqlx::Row as _;

use crate::{
    admin::server_fns::addon_media_lists::{
        add_addon_media_list_with_state, delete_addon_media_list_with_state,
        match_addon_media_list_with_state, update_addon_media_list_with_state, AddonMediaListForm,
    },
    host::state::AppState,
};

#[derive(Debug, Deserialize)]
pub struct AddonMediaListAddForm {
    #[serde(default)]
    addon_id: String,
    source_url: String,
    #[serde(default)]
    folder_name: String,
    #[serde(default)]
    content_kind_override: AddonMediaListContentKind,
    #[serde(default)]
    chapter_input: String,
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
    #[serde(default)]
    content_kind_override: AddonMediaListContentKind,
    #[serde(default)]
    chapter_input: String,
}

fn is_unresolved_addon(error: &ersatzrs_app::ApplicationError) -> bool {
    matches!(
        error,
        ersatzrs_app::ApplicationError::Validation {
            field: "addon_id",
            ..
        }
    )
}

pub async fn add_addon_media_list_form(
    State(state): State<AppState>,
    Form(input): Form<AddonMediaListAddForm>,
) -> Redirect {
    match add_addon_media_list_with_state(
        &state,
        input.addon_id,
        input.source_url,
        input.folder_name,
        input.content_kind_override,
        input.chapter_input,
    )
    .await
    {
        Ok(_) => Redirect::to("/media/addon-lists"),
        Err(error) => {
            tracing::error!(%error, "failed to add add-on media list from HTML form");
            if is_unresolved_addon(&error) {
                Redirect::to("/media/addon-lists?error=add-unresolved")
            } else {
                Redirect::to("/media/addon-lists?error=add")
            }
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
        folder_name: String::new(),
        auto_refresh: input.auto_refresh,
        generate_playlist: input.generate_playlist,
        content_kind_override: input.content_kind_override,
        legacy_chapter_input: input.chapter_input,
        presentation_kind: AddonMediaListPresentationKind::RemoteStreams,
        requested_presentation_kind: AddonMediaListPresentationKind::RemoteStreams,
        presentation_error: None,
        review_state: AddonMediaListReviewState::default(),
        review_error: None,
        published_revision: 0,
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

/// Multipart overhead allowed in addition to the validated image payload.
pub const MAX_ARTWORK_REQUEST_BYTES: usize =
    ersatzrs_infra::addon_artwork::MAX_ARTWORK_BYTES + 64 * 1024;

/// Store one role-aware artwork upload inside the metadata draft.
pub async fn upload_artwork(State(state): State<AppState>, mut multipart: Multipart) -> Response {
    let mut list_id = None;
    let mut item_id = None;
    let mut role = None;
    let mut etag = None;
    let mut image = None;
    loop {
        let field = match multipart.next_field().await {
            Ok(Some(field)) => field,
            Ok(None) => break,
            Err(error) => return api_error(StatusCode::BAD_REQUEST, error.to_string()),
        };
        let Some(name) = field.name().map(str::to_owned) else {
            continue;
        };
        if name == "file" {
            let content_type = field
                .content_type()
                .unwrap_or("application/octet-stream")
                .to_owned();
            match field.bytes().await {
                Ok(bytes) => image = Some((content_type, bytes)),
                Err(error) => return api_error(StatusCode::BAD_REQUEST, error.to_string()),
            }
            continue;
        }
        let value = match field.text().await {
            Ok(value) => value,
            Err(error) => return api_error(StatusCode::BAD_REQUEST, error.to_string()),
        };
        match name.as_str() {
            "list_id" => list_id = value.parse::<i64>().ok().filter(|id| *id > 0),
            "item_id" => item_id = value.parse::<i64>().ok().filter(|id| *id > 0),
            "role" => role = parse_role(&value),
            "etag" => etag = (!value.trim().is_empty()).then_some(value),
            _ => {}
        }
    }
    let Some(list_id) = list_id else {
        return api_error(StatusCode::BAD_REQUEST, "list_id is required");
    };
    let Some(role) = role else {
        return api_error(
            StatusCode::BAD_REQUEST,
            "a supported artwork role is required",
        );
    };
    let Some(etag) = etag else {
        return api_error(StatusCode::BAD_REQUEST, "etag is required");
    };
    let Some((content_type, bytes)) = image else {
        return api_error(StatusCode::BAD_REQUEST, "an artwork file is required");
    };
    match state
        .application
        .ports()
        .addon_media_lists()
        .upload_artwork(list_id, item_id, role, &content_type, &bytes, &etag)
        .await
    {
        Ok(form) => {
            let selected = form
                .metadata
                .artwork
                .iter()
                .find(|candidate| candidate.role == role)
                .map(|candidate| candidate.url.clone())
                .unwrap_or_default();
            Json(json!({"etag": form.etag, "url": selected})).into_response()
        }
        Err(RepositoryError::NotFound { .. }) => {
            api_error(StatusCode::NOT_FOUND, "metadata owner was not found")
        }
        Err(RepositoryError::Conflict(message)) => api_error(StatusCode::CONFLICT, message),
        Err(error) => api_error(StatusCode::UNPROCESSABLE_ENTITY, error.to_string()),
    }
}

/// Serve a database-backed draft image for the editor preview. The opaque
/// upload id is not a filesystem path and cannot escape managed storage.
pub async fn uploaded_artwork(
    State(state): State<AppState>,
    Path(upload_id): Path<i64>,
) -> Response {
    let row = sqlx::query(
        r#"SELECT "ContentType", "Bytes" FROM "AddonMediaListArtworkUpload" WHERE "Id" = ?"#,
    )
    .bind(upload_id)
    .fetch_optional(&state.pool)
    .await;
    let Ok(Some(row)) = row else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Ok(content_type) = row.try_get::<String, _>("ContentType") else {
        return StatusCode::UNPROCESSABLE_ENTITY.into_response();
    };
    let Ok(bytes) = row.try_get::<Vec<u8>, _>("Bytes") else {
        return StatusCode::UNPROCESSABLE_ENTITY.into_response();
    };
    (
        [
            (header::CONTENT_TYPE, content_type),
            (header::CACHE_CONTROL, "private, no-store".to_owned()),
        ],
        Body::from(bytes),
    )
        .into_response()
}

fn parse_role(value: &str) -> Option<AddonMediaListArtworkRole> {
    match value.trim().to_ascii_lowercase().as_str() {
        "poster" => Some(AddonMediaListArtworkRole::Poster),
        "fanart" => Some(AddonMediaListArtworkRole::Fanart),
        "thumb" | "thumbnail" => Some(AddonMediaListArtworkRole::Thumb),
        _ => None,
    }
}

fn api_error(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(json!({"error": message.into()}))).into_response()
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
        assert!(parsed.chapter_input.is_empty());
    }

    #[test]
    fn save_form_preserves_multiline_chapter_input() {
        let body = Bytes::from_static(
            b"id=42&addon_id=org.example.list&source_url=https%3A%2F%2Fexample.invalid%2Fepisode&chapter_input=0%3A00+Intro%0A1%3A35+First+topic",
        );
        let parsed: AddonMediaListSaveForm = serde_urlencoded::from_bytes(&body).unwrap();
        assert_eq!(parsed.chapter_input, "0:00 Intro\n1:35 First topic");
    }

    #[test]
    fn only_publishable_artwork_roles_are_accepted() {
        assert_eq!(
            parse_role("poster"),
            Some(AddonMediaListArtworkRole::Poster)
        );
        assert_eq!(
            parse_role("fanart"),
            Some(AddonMediaListArtworkRole::Fanart)
        );
        assert_eq!(
            parse_role("thumbnail"),
            Some(AddonMediaListArtworkRole::Thumb)
        );
        assert_eq!(parse_role("other"), None);
    }
}
