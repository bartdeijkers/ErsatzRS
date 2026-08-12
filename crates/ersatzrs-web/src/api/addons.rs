use axum::{
    body::Body,
    extract::{Multipart, Path, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use sha2::{Digest as _, Sha256};
use std::fmt::Write as _;

use ersatzrs_addon_contract::AddonIconMediaType;
use ersatzrs_infra::file_system_layout::addon_cache_folder;

use crate::{
    admin::{
        i18n::{upload_error_text, UiLanguage},
        server_fns::settings::load_ui_language_for_api,
    },
    host::addons::addon_repository_client,
    host::state::AppState,
};

pub const MAX_ADDON_UPLOAD_BYTES: usize = 256 * 1024 * 1024;

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

pub async fn upload_repository_package(
    State(state): State<AppState>,
    multipart: Multipart,
) -> Response {
    let language = error_language(&state).await;
    let bytes = match read_single_file(multipart, 256 * 1024, language).await {
        Ok(bytes) => bytes,
        Err(response) => return response,
    };
    match state
        .application
        .install_addon_repository_from_package(bytes)
        .await
    {
        Ok(()) => Json(json!({"ok": true})).into_response(),
        Err(error) => api_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            upload_error_text(language, &error.to_string()),
        ),
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

pub async fn catalog_icon(
    State(state): State<AppState>,
    Path((repository_id, addon_id)): Path<(String, String)>,
) -> Response {
    let overview = match state.application.addon_overview().await {
        Ok(overview) => overview,
        Err(error) => return api_error(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
    };
    let Some(icon) = overview
        .available
        .into_iter()
        .find(|item| item.repository_id == repository_id && item.addon.id == addon_id)
        .and_then(|item| item.addon.icon)
    else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let cache_path = addon_cache_folder(&repository_id, &addon_id)
        .join("icons")
        .join(&icon.sha256);
    match tokio::fs::read(&cache_path).await {
        Ok(bytes)
            if bytes.len() as u64 == icon.size
                && sha256_hex(&bytes) == icon.sha256
                && valid_icon_bytes(&bytes, icon.media_type) =>
        {
            return icon_response(bytes, icon.media_type.as_mime_type());
        }
        Ok(_) | Err(_) => {}
    }
    let response = match addon_repository_client().get(&icon.url).send().await {
        Ok(response) if response.status().is_success() && response.url().scheme() == "https" => {
            response
        }
        Ok(_) => return StatusCode::BAD_GATEWAY.into_response(),
        Err(error) => return api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    };
    if response
        .content_length()
        .is_some_and(|length| length > icon.size)
    {
        return StatusCode::BAD_GATEWAY.into_response();
    }
    let bytes = match response.bytes().await {
        Ok(bytes) if bytes.len() as u64 == icon.size => bytes,
        _ => return StatusCode::BAD_GATEWAY.into_response(),
    };
    let actual = sha256_hex(&bytes);
    if actual != icon.sha256 || !valid_icon_bytes(&bytes, icon.media_type) {
        return StatusCode::BAD_GATEWAY.into_response();
    }
    if let Some(parent) = cache_path.parent() {
        if tokio::fs::create_dir_all(parent).await.is_ok() {
            let temporary = cache_path.with_extension("tmp");
            if tokio::fs::write(&temporary, &bytes).await.is_ok() {
                let _ = tokio::fs::rename(temporary, &cache_path).await;
            }
        }
    }
    icon_response(bytes.to_vec(), icon.media_type.as_mime_type())
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

fn icon_response(bytes: Vec<u8>, content_type: &'static str) -> Response {
    (
        [
            (header::CONTENT_TYPE, content_type),
            (header::CACHE_CONTROL, "private, max-age=3600"),
        ],
        Body::from(bytes),
    )
        .into_response()
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        let _ = write!(&mut output, "{byte:02x}");
    }
    output
}

fn valid_icon_bytes(bytes: &[u8], media_type: AddonIconMediaType) -> bool {
    match media_type {
        AddonIconMediaType::Png => bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
        AddonIconMediaType::Webp => {
            bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP"
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_bytes_must_match_the_signed_media_type() {
        let png = b"\x89PNG\r\n\x1a\nfixture";
        let webp = b"RIFF\x04\x00\x00\x00WEBPfixture";
        assert!(valid_icon_bytes(png, AddonIconMediaType::Png));
        assert!(valid_icon_bytes(webp, AddonIconMediaType::Webp));
        assert!(!valid_icon_bytes(png, AddonIconMediaType::Webp));
    }

    #[test]
    fn icon_hash_is_stable_lowercase_hex() {
        assert_eq!(
            sha256_hex(b"fixture"),
            "f16d05ec6b29248d2c61adb1e9263f78e4f7bace1b955014a2d17872cfe4064d"
        );
    }
}
