use axum::{
    body::Body,
    extract::{Multipart, Path},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use std::path::PathBuf;
use tokio::fs;
use uuid::Uuid;

const WATERMARK_DIR: &str = "data/watermarks";

pub async fn upload_watermark_image(mut multipart: Multipart) -> Response {
    if let Err(e) = fs::create_dir_all(WATERMARK_DIR).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": format!("Cannot create upload directory: {e}")})),
        )
            .into_response();
    }

    if let Ok(Some(field)) = multipart.next_field().await {
        let content_type = field
            .content_type()
            .unwrap_or("application/octet-stream")
            .to_string();

        let ext = match content_type.as_str() {
            "image/png" => "png",
            "image/jpeg" | "image/jpg" => "jpg",
            "image/gif" => "gif",
            "image/webp" => "webp",
            _ => "bin",
        };

        let filename = format!("{}.{ext}", Uuid::new_v4());
        let dest = PathBuf::from(WATERMARK_DIR).join(&filename);

        match field.bytes().await {
            Ok(bytes) => {
                if let Err(e) = fs::write(&dest, &bytes).await {
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({"error": format!("Write failed: {e}")})),
                    )
                        .into_response();
                }
                return Json(json!({
                    "path": filename,
                    "content_type": content_type,
                }))
                .into_response();
            }
            Err(e) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({"error": format!("Read field: {e}")})),
                )
                    .into_response();
            }
        }
    }

    (
        StatusCode::BAD_REQUEST,
        Json(json!({"error": "No file field"})),
    )
        .into_response()
}

pub async fn serve_watermark_image(Path(filename): Path<String>) -> Response {
    let path = PathBuf::from(WATERMARK_DIR).join(&filename);

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
