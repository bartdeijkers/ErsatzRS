use axum::{
    body::Body,
    extract::{Multipart, Path, State},
    http::{header, StatusCode},
    response::{IntoResponse, Redirect, Response},
    Json,
};
use serde_json::json;
use tokio::fs;
use uuid::Uuid;

use ersatzrs_infra::file_system_layout::data_path;

use crate::{
    admin::{
        i18n::{upload_error_text, UiLanguage},
        server_fns::{
            settings::load_ui_language_for_api,
            watermarks::{watermark_form_into_admin, WatermarkForm},
        },
    },
    host::state::AppState,
};

const DEFAULT_MAX_UPLOAD_MB: usize = 10;

pub fn max_upload_bytes() -> usize {
    #[allow(clippy::disallowed_methods)] // Phase 0: config crate not yet available
    let mb = std::env::var("ETV_MAXIMUM_UPLOAD_MB")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(DEFAULT_MAX_UPLOAD_MB);
    mb * 1024 * 1024
}

pub async fn upload_watermark_image(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Response {
    let language = load_error_language(&state).await;
    if let Err(e) = fs::create_dir_all(watermark_dir()).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": upload_error_text(language, &format!("Cannot create upload directory: {e}"))})),
        )
            .into_response();
    }

    let field = match multipart.next_field().await {
        Ok(Some(field)) => field,
        Ok(None) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": upload_error_text(language, "No file field")})),
            )
                .into_response();
        }
        Err(error) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(
                    json!({"error": upload_error_text(language, &format!("Read field: {error}"))}),
                ),
            )
                .into_response();
        }
    };
    let content_type = field
        .content_type()
        .unwrap_or("application/octet-stream")
        .to_string();

    match field.bytes().await {
        Ok(bytes) => {
            if bytes.len() > max_upload_bytes() {
                return (
                        StatusCode::PAYLOAD_TOO_LARGE,
                        Json(json!({"error": upload_error_text(language, "Upload exceeds maximum allowed size")})),
                    )
                        .into_response();
            }
            let filename = match persist_watermark_image(&content_type, &bytes).await {
                Ok(filename) => filename,
                Err(e) => {
                    return (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(json!({"error": upload_error_text(language, &format!("Write failed: {e}"))})),
                        )
                            .into_response();
                }
            };
            Json(json!({
                "path": filename,
                "content_type": content_type,
            }))
            .into_response()
        }
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": upload_error_text(language, &format!("Read field: {e}"))})),
        )
            .into_response(),
    }
}

async fn load_error_language(state: &AppState) -> UiLanguage {
    match load_ui_language_for_api(state.application.ports().admin_settings()).await {
        Ok(language) => language,
        Err(error) => {
            tracing::warn!(%error, "failed to load UI language for watermark API error");
            UiLanguage::English
        }
    }
}

pub async fn save_watermark_form(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Redirect {
    let mut form = WatermarkForm::default();

    loop {
        let field = match multipart.next_field().await {
            Ok(Some(field)) => field,
            Ok(None) => break,
            Err(error) => {
                tracing::warn!(%error, "failed to parse watermark multipart form");
                return Redirect::to(&watermark_editor_error_path(form.id, "save"));
            }
        };

        let Some(field_name) = field.name().map(str::to_owned) else {
            continue;
        };

        if field_name == "image_file" || field_name == "file" {
            let submitted_name = field.file_name().unwrap_or_default().to_string();
            let content_type = field
                .content_type()
                .unwrap_or("application/octet-stream")
                .to_string();

            match field.bytes().await {
                Ok(bytes) if !submitted_name.is_empty() && !bytes.is_empty() => {
                    if bytes.len() > max_upload_bytes() {
                        tracing::warn!("watermark form upload exceeded maximum allowed size");
                        return Redirect::to(&watermark_editor_error_path(form.id, "upload"));
                    }

                    match persist_watermark_image(&content_type, &bytes).await {
                        Ok(stored_name) => {
                            form.image = Some(stored_name);
                            form.original_content_type = Some(content_type);
                        }
                        Err(error) => {
                            tracing::error!(%error, "failed to persist watermark image from form");
                            return Redirect::to(&watermark_editor_error_path(form.id, "upload"));
                        }
                    }
                }
                Ok(_) => {}
                Err(error) => {
                    tracing::warn!(%error, "failed to read watermark image field");
                    return Redirect::to(&watermark_editor_error_path(form.id, "upload"));
                }
            }
            continue;
        }

        let value = match field.text().await {
            Ok(value) => value,
            Err(error) => {
                tracing::warn!(%error, field = %field_name, "failed to read watermark form field");
                return Redirect::to(&watermark_editor_error_path(form.id, "save"));
            }
        };

        apply_form_field(&mut form, &field_name, value);
    }

    let form_id = form.id;
    match state
        .application
        .save_watermark(ersatzrs_app::commands::SaveWatermark {
            form: watermark_form_into_admin(form),
        })
        .await
    {
        Ok(_) => Redirect::to("/watermarks"),
        Err(error) => {
            tracing::warn!(%error, watermark_id = form_id, "failed to save watermark from HTML form");
            Redirect::to(&watermark_editor_error_path(form_id, "save"))
        }
    }
}

async fn persist_watermark_image(content_type: &str, bytes: &[u8]) -> std::io::Result<String> {
    fs::create_dir_all(watermark_dir()).await?;
    let filename = format!(
        "{}.{}",
        Uuid::new_v4(),
        extension_for_content_type(content_type)
    );
    let dest = watermark_dir().join(&filename);
    fs::write(&dest, bytes).await?;
    Ok(filename)
}

fn extension_for_content_type(content_type: &str) -> &'static str {
    match content_type {
        "image/png" => "png",
        "image/jpeg" | "image/jpg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        _ => "bin",
    }
}

fn apply_form_field(form: &mut WatermarkForm, field: &str, value: String) {
    match field {
        "id" => form.id = parse_i64(&value, form.id),
        "name" => form.name = value,
        "mode" => form.mode = parse_i32(&value, form.mode),
        "image_source" => form.image_source = parse_i32(&value, form.image_source),
        "image" => form.image = non_empty(value),
        "original_content_type" => form.original_content_type = non_empty(value),
        "location" => form.location = parse_i32(&value, form.location),
        "size" => form.size = parse_i32(&value, form.size),
        "width_percent" => form.width_percent = parse_f64(&value, form.width_percent),
        "horizontal_margin_percent" => {
            form.horizontal_margin_percent = parse_f64(&value, form.horizontal_margin_percent);
        }
        "vertical_margin_percent" => {
            form.vertical_margin_percent = parse_f64(&value, form.vertical_margin_percent);
        }
        "frequency_minutes" => form.frequency_minutes = parse_i32(&value, form.frequency_minutes),
        "duration_seconds" => form.duration_seconds = parse_i32(&value, form.duration_seconds),
        "opacity" => form.opacity = parse_i32(&value, form.opacity),
        "place_within_source_content" => form.place_within_source_content = parse_bool(&value),
        "opacity_expression" => form.opacity_expression = value,
        "z_index" => form.z_index = parse_i32(&value, form.z_index),
        _ => {}
    }
}

fn parse_i64(value: &str, fallback: i64) -> i64 {
    value.parse().unwrap_or(fallback)
}

fn parse_i32(value: &str, fallback: i32) -> i32 {
    value.parse().unwrap_or(fallback)
}

fn parse_f64(value: &str, fallback: f64) -> f64 {
    value.parse().unwrap_or(fallback)
}

fn parse_bool(value: &str) -> bool {
    matches!(value, "true" | "True" | "1" | "on" | "ON")
}

fn non_empty(value: String) -> Option<String> {
    if value.trim().is_empty() {
        None
    } else {
        Some(value)
    }
}

fn watermark_editor_error_path(id: i64, error: &str) -> String {
    if id > 0 {
        format!("/watermarks/{id}?error={error}")
    } else {
        format!("/watermarks/add?error={error}")
    }
}

pub async fn serve_watermark_image(Path(filename): Path<String>) -> Response {
    if !super::artwork::is_cached_image_filename(&filename) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let path = watermark_dir().join(&filename);

    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("bin");
    let content_type = match ext {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        _ => "application/octet-stream",
    };

    match fs::read(&path).await {
        Ok(bytes) => ([(header::CONTENT_TYPE, content_type)], Body::from(bytes)).into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

fn watermark_dir() -> std::path::PathBuf {
    data_path("watermarks")
}
