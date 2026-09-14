use crate::{
    admin::{
        i18n::{upload_error_text, UiLanguage},
        server_fns::settings::load_ui_language_for_api,
    },
    host::state::AppState,
};
use axum::{
    body::Body,
    extract::{Form, Multipart, Path, State},
    http::{header, StatusCode},
    response::{IntoResponse, Redirect, Response},
    Json,
};
use serde::Deserialize;
use serde_json::json;

pub const MAX_ADDON_UPLOAD_BYTES: usize = 256 * 1024 * 1024;

#[derive(Debug, Deserialize)]
pub struct RemoveAddonRepositoryForm {
    repository_id: String,
}

pub async fn upload_addon_package(State(state): State<AppState>, multipart: Multipart) -> Response {
    let language = error_language(&state).await;
    let bytes = match read_single_file(multipart, MAX_ADDON_UPLOAD_BYTES, language).await {
        Ok(bytes) => bytes,
        Err(response) => return response,
    };
    match state.application.install_addon_from_package(bytes).await {
        Ok(()) => Json(json!({"ok": true})).into_response(),
        Err(error) => api_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            upload_error_text(language, &error.to_string()),
        ),
    }
}

pub async fn upload_repository_bundle(
    State(state): State<AppState>,
    multipart: Multipart,
) -> Response {
    let language = error_language(&state).await;
    let bytes = match read_single_file(multipart, 512 * 1024 * 1024, language).await {
        Ok(bytes) => bytes,
        Err(response) => return response,
    };
    match state
        .application
        .install_addon_repository_bundle(bytes)
        .await
    {
        Ok(repository) => Json(json!({"ok": true, "repository": repository})).into_response(),
        Err(error) => api_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            upload_error_text(language, &error.to_string()),
        ),
    }
}

pub async fn remove_repository_bundle(
    State(state): State<AppState>,
    Form(input): Form<RemoveAddonRepositoryForm>,
) -> Redirect {
    match state
        .application
        .remove_addon_repository(input.repository_id.clone())
        .await
    {
        Ok(()) => Redirect::to("/settings/addons?view=sources"),
        Err(error) => {
            tracing::error!(
                %error,
                repository_id = %input.repository_id,
                "failed to remove add-on repository bundle from HTML form"
            );
            Redirect::to("/settings/addons?view=sources&error=delete-repository")
        }
    }
}

pub async fn installed_icon(
    State(state): State<AppState>,
    Path(addon_id): Path<String>,
) -> Response {
    let icon = match state.application.addon_installed_icon(addon_id).await {
        Ok(Some(icon)) => icon,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(error) => return api_error(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
    };
    icon_response(icon)
}

pub async fn source_icon(
    State(state): State<AppState>,
    Path((source_id, addon_id)): Path<(String, String)>,
) -> Response {
    let icon = match state
        .application
        .addon_source_icon(source_id, addon_id)
        .await
    {
        Ok(Some(icon)) => icon,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(error) => return api_error(StatusCode::CONFLICT, error.to_string()),
    };
    icon_response(icon)
}

fn icon_response(icon: ersatzrs_app::AddonIconRead) -> Response {
    let Ok(content_type) = header::HeaderValue::from_str(&icon.content_type) else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    (
        [
            (header::CONTENT_TYPE, content_type),
            (
                header::CACHE_CONTROL,
                header::HeaderValue::from_static("private, max-age=3600"),
            ),
        ],
        Body::from(icon.bytes),
    )
        .into_response()
}

async fn read_single_file(
    mut multipart: Multipart,
    limit: usize,
    language: UiLanguage,
) -> Result<Vec<u8>, Response> {
    let field = multipart
        .next_field()
        .await
        .map_err(|error| api_error(StatusCode::BAD_REQUEST, error.to_string()))?
        .ok_or_else(|| {
            api_error(
                StatusCode::BAD_REQUEST,
                upload_error_text(language, "No file field"),
            )
        })?;
    let bytes = field
        .bytes()
        .await
        .map_err(|error| api_error(StatusCode::BAD_REQUEST, error.to_string()))?;
    if bytes.is_empty() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            upload_error_text(language, "No file field"),
        ));
    }
    if bytes.len() > limit {
        return Err(api_error(
            StatusCode::PAYLOAD_TOO_LARGE,
            upload_error_text(language, "Upload exceeds maximum allowed size"),
        ));
    }
    Ok(bytes.to_vec())
}

fn api_error(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(json!({"error": message.into()}))).into_response()
}

async fn error_language(state: &AppState) -> UiLanguage {
    match load_ui_language_for_api(state.application.ports().admin_settings()).await {
        Ok(language) => language,
        Err(error) => {
            tracing::warn!(%error, "failed to load UI language for add-on upload API error");
            UiLanguage::English
        }
    }
}
