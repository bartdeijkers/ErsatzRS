use axum::{
    body::Body,
    extract::{Form, Multipart, Path, State},
    http::{header, StatusCode},
    response::{IntoResponse, Redirect, Response},
    Json,
};
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;
use tokio::fs;
use uuid::Uuid;

use ersatzrs_infra::file_system_layout::data_path;

use crate::{
    admin::{
        i18n::{upload_error_text, UiLanguage},
        server_fns::{
            channels::{
                save_channel_numbers_with_pool, save_channel_with_state, ChannelForm,
                ChannelNumberFormUpdate,
            },
            settings::load_ui_language_for_api,
        },
    },
    host::state::AppState,
};

#[derive(Debug, Deserialize)]
pub struct ChannelSaveForm {
    id: i64,
    number: String,
    name: String,
    #[serde(default)]
    group: String,
    #[serde(default)]
    categories: String,
    ffmpeg_profile_id: i64,
    #[serde(default)]
    slug_seconds: String,
    #[serde(default)]
    logo_path: String,
    #[serde(default)]
    logo_content_type: String,
    #[serde(default)]
    external_logo_url: String,
    #[serde(default)]
    stream_selector_mode: i32,
    #[serde(default)]
    stream_selector: String,
    #[serde(default)]
    preferred_audio_language_code: String,
    #[serde(default)]
    preferred_audio_title: String,
    #[serde(default)]
    playout_source: i32,
    #[serde(default)]
    playout_mode: i32,
    #[serde(default)]
    mirror_source_channel_id: i64,
    #[serde(default)]
    playout_offset_hours: i32,
    streaming_mode: i32,
    #[serde(default)]
    watermark_id: i64,
    #[serde(default)]
    is_enabled: bool,
    #[serde(default)]
    show_in_epg: bool,
    #[serde(default)]
    fallback_filler_id: i64,
    #[serde(default)]
    preferred_subtitle_language_code: String,
    #[serde(default)]
    subtitle_mode: i32,
    #[serde(default)]
    music_video_credits_mode: i32,
    #[serde(default)]
    music_video_credits_template: String,
    #[serde(default)]
    song_video_mode: i32,
    #[serde(default)]
    transcode_mode: i32,
    #[serde(default)]
    idle_behavior: i32,
    #[serde(default)]
    remove_logo: bool,
}

pub async fn save_channel_form(
    State(state): State<AppState>,
    Form(input): Form<ChannelSaveForm>,
) -> Redirect {
    let id = input.id;
    let form = input.into_channel_form();

    match save_channel_with_state(&state, form).await {
        Ok(_) => Redirect::to("/channels"),
        Err(error) => {
            tracing::error!(%error, "failed to save channel from HTML form");
            channel_save_error_redirect(id, &error)
        }
    }
}

pub async fn save_channel_multipart_form(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Redirect {
    let mut fields = HashMap::new();
    let mut uploaded_logo: Option<(String, String)> = None;

    if let Err(error) = fs::create_dir_all(channel_logo_dir()).await {
        tracing::error!(%error, "failed to create channel logo upload directory");
        return Redirect::to("/channels/add?error=save");
    }

    loop {
        match multipart.next_field().await {
            Ok(Some(field)) => {
                let Some(name) = field.name().map(str::to_owned) else {
                    continue;
                };

                if name == "logo_file" {
                    let content_type = field
                        .content_type()
                        .unwrap_or("application/octet-stream")
                        .to_string();
                    match field.bytes().await {
                        Ok(bytes) if !bytes.is_empty() => {
                            if bytes.len() > super::watermarks::max_upload_bytes() {
                                tracing::warn!(
                                    bytes = bytes.len(),
                                    "channel logo upload exceeds maximum allowed size"
                                );
                                return Redirect::to("/channels/add?error=upload");
                            }
                            match persist_channel_logo(&content_type, &bytes).await {
                                Ok(path) => uploaded_logo = Some((path, content_type)),
                                Err(error) => {
                                    tracing::error!(%error, "failed to persist channel logo upload");
                                    return Redirect::to("/channels/add?error=upload");
                                }
                            }
                        }
                        Ok(_) => {}
                        Err(error) => {
                            tracing::error!(%error, "failed to read channel logo upload field");
                            return Redirect::to("/channels/add?error=upload");
                        }
                    }
                } else {
                    match field.text().await {
                        Ok(value) => {
                            fields.insert(name, value);
                        }
                        Err(error) => {
                            tracing::error!(%error, "failed to read channel editor form field");
                            return Redirect::to("/channels/add?error=save");
                        }
                    }
                }
            }
            Ok(None) => break,
            Err(error) => {
                tracing::error!(%error, "failed to read channel editor multipart form");
                return Redirect::to("/channels/add?error=save");
            }
        }
    }

    let mut input = ChannelSaveForm::from_fields(&fields);
    if let Some((path, content_type)) = uploaded_logo {
        input.logo_path = path;
        input.logo_content_type = content_type;
        input.external_logo_url.clear();
    } else if input.remove_logo && input.external_logo_url.trim().is_empty() {
        input.logo_path.clear();
        input.logo_content_type.clear();
    }

    let id = input.id;
    let form = input.into_channel_form();
    match save_channel_with_state(&state, form).await {
        Ok(_) => Redirect::to("/channels"),
        Err(error) => {
            tracing::error!(%error, "failed to save channel from multipart HTML form");
            channel_save_error_redirect(id, &error)
        }
    }
}

pub async fn save_channel_numbers_form(
    State(state): State<AppState>,
    Form(fields): Form<HashMap<String, String>>,
) -> Redirect {
    let updates = channel_number_updates_from_fields(&fields);
    match save_channel_numbers_with_pool(&state.pool, updates).await {
        Ok(_) => Redirect::to("/channels"),
        Err(error) => {
            tracing::error!(%error, "failed to save channel numbers from HTML form");
            Redirect::to(&format!(
                "/channels/numbers?error={}",
                urlencoding::encode(&error.to_string())
            ))
        }
    }
}

fn channel_save_error_redirect(id: i64, error: &dyn std::error::Error) -> Redirect {
    let target = if id > 0 {
        format!("/channels/{id}")
    } else {
        "/channels/add".to_string()
    };
    Redirect::to(&format!(
        "{target}?error={}",
        urlencoding::encode(&error.to_string())
    ))
}

pub async fn upload_channel_logo(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Response {
    let language = load_error_language(&state).await;
    if let Err(error) = fs::create_dir_all(channel_logo_dir()).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": upload_error_text(language, &format!("Cannot create upload directory: {error}"))})),
        )
            .into_response();
    }

    if let Ok(Some(field)) = multipart.next_field().await {
        let content_type = field
            .content_type()
            .unwrap_or("application/octet-stream")
            .to_string();

        match field.bytes().await {
            Ok(bytes) => {
                if bytes.len() > super::watermarks::max_upload_bytes() {
                    return (
                        StatusCode::PAYLOAD_TOO_LARGE,
                        Json(json!({"error": upload_error_text(language, "Upload exceeds maximum allowed size")})),
                    )
                        .into_response();
                }

                match persist_channel_logo(&content_type, &bytes).await {
                    Ok(path) => Json(json!({
                        "path": path,
                        "content_type": content_type,
                    }))
                    .into_response(),
                    Err(error) => (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({"error": upload_error_text(language, &format!("Write failed: {error}"))})),
                    )
                        .into_response(),
                }
            }
            Err(error) => (
                StatusCode::BAD_REQUEST,
                Json(
                    json!({"error": upload_error_text(language, &format!("Read field: {error}"))}),
                ),
            )
                .into_response(),
        }
    } else {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": upload_error_text(language, "No file field")})),
        )
            .into_response()
    }
}

async fn load_error_language(state: &AppState) -> UiLanguage {
    load_ui_language_for_api(&state.pool)
        .await
        .unwrap_or(UiLanguage::English)
}

async fn persist_channel_logo(content_type: &str, bytes: &[u8]) -> std::io::Result<String> {
    fs::create_dir_all(channel_logo_dir()).await?;
    let filename = format!(
        "{}.{}",
        Uuid::new_v4(),
        extension_for_content_type(content_type)
    );
    let dest = channel_logo_dir().join(&filename);
    fs::write(&dest, bytes).await?;
    Ok(format!("iptv/logos/{filename}"))
}

fn extension_for_content_type(content_type: &str) -> &'static str {
    match content_type {
        "image/png" => "png",
        "image/jpeg" | "image/jpg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "image/svg+xml" => "svg",
        _ => "bin",
    }
}

impl ChannelSaveForm {
    fn from_fields(fields: &HashMap<String, String>) -> Self {
        Self {
            id: parse_i64(fields, "id"),
            number: text(fields, "number"),
            name: text(fields, "name"),
            group: text(fields, "group"),
            categories: text(fields, "categories"),
            ffmpeg_profile_id: parse_i64(fields, "ffmpeg_profile_id"),
            slug_seconds: text(fields, "slug_seconds"),
            logo_path: text(fields, "logo_path"),
            logo_content_type: text(fields, "logo_content_type"),
            external_logo_url: text(fields, "external_logo_url"),
            stream_selector_mode: parse_i32(fields, "stream_selector_mode"),
            stream_selector: text(fields, "stream_selector"),
            preferred_audio_language_code: text(fields, "preferred_audio_language_code"),
            preferred_audio_title: text(fields, "preferred_audio_title"),
            playout_source: parse_i32(fields, "playout_source"),
            playout_mode: parse_i32(fields, "playout_mode"),
            mirror_source_channel_id: parse_i64(fields, "mirror_source_channel_id"),
            playout_offset_hours: parse_i32(fields, "playout_offset_hours"),
            streaming_mode: parse_i32(fields, "streaming_mode"),
            watermark_id: parse_i64(fields, "watermark_id"),
            is_enabled: parse_bool(fields, "is_enabled"),
            show_in_epg: parse_bool(fields, "show_in_epg"),
            fallback_filler_id: parse_i64(fields, "fallback_filler_id"),
            preferred_subtitle_language_code: text(fields, "preferred_subtitle_language_code"),
            subtitle_mode: parse_i32(fields, "subtitle_mode"),
            music_video_credits_mode: parse_i32(fields, "music_video_credits_mode"),
            music_video_credits_template: text(fields, "music_video_credits_template"),
            song_video_mode: parse_i32(fields, "song_video_mode"),
            transcode_mode: parse_i32(fields, "transcode_mode"),
            idle_behavior: parse_i32(fields, "idle_behavior"),
            remove_logo: parse_bool(fields, "remove_logo"),
        }
    }

    fn into_channel_form(self) -> ChannelForm {
        let (logo_path, logo_content_type, external_logo_url) =
            if self.remove_logo && self.external_logo_url.trim().is_empty() {
                (String::new(), String::new(), String::new())
            } else {
                (
                    self.logo_path,
                    self.logo_content_type,
                    self.external_logo_url,
                )
            };

        ChannelForm {
            id: self.id,
            number: self.number,
            name: self.name,
            group: self.group,
            categories: self.categories,
            ffmpeg_profile_id: self.ffmpeg_profile_id,
            slug_seconds: self.slug_seconds.parse::<f64>().ok(),
            logo_path,
            logo_content_type,
            external_logo_url,
            stream_selector_mode: self.stream_selector_mode,
            stream_selector: self.stream_selector,
            preferred_audio_language_code: self.preferred_audio_language_code,
            preferred_audio_title: self.preferred_audio_title,
            playout_source: self.playout_source,
            playout_mode: self.playout_mode,
            mirror_source_channel_id: self.mirror_source_channel_id,
            playout_offset_hours: self.playout_offset_hours,
            streaming_mode: self.streaming_mode,
            watermark_id: self.watermark_id,
            is_enabled: self.is_enabled,
            show_in_epg: self.show_in_epg,
            fallback_filler_id: self.fallback_filler_id,
            preferred_subtitle_language_code: self.preferred_subtitle_language_code,
            subtitle_mode: self.subtitle_mode,
            music_video_credits_mode: self.music_video_credits_mode,
            music_video_credits_template: self.music_video_credits_template,
            song_video_mode: self.song_video_mode,
            transcode_mode: self.transcode_mode,
            idle_behavior: self.idle_behavior,
        }
    }
}

fn text(fields: &HashMap<String, String>, name: &str) -> String {
    fields.get(name).cloned().unwrap_or_default()
}

fn parse_i32(fields: &HashMap<String, String>, name: &str) -> i32 {
    fields
        .get(name)
        .and_then(|value| value.parse::<i32>().ok())
        .unwrap_or_default()
}

fn parse_i64(fields: &HashMap<String, String>, name: &str) -> i64 {
    fields
        .get(name)
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or_default()
}

fn parse_bool(fields: &HashMap<String, String>, name: &str) -> bool {
    fields
        .get(name)
        .is_some_and(|value| matches!(value.as_str(), "true" | "on" | "1" | "True"))
}

fn channel_number_updates_from_fields(
    fields: &HashMap<String, String>,
) -> Vec<ChannelNumberFormUpdate> {
    let mut updates = fields
        .iter()
        .filter_map(|(name, value)| {
            let id = name.strip_prefix("number_")?.parse::<i64>().ok()?;
            Some(ChannelNumberFormUpdate {
                id,
                number: value.clone(),
            })
        })
        .collect::<Vec<_>>();
    updates.sort_by_key(|update| update.id);
    updates
}

pub async fn serve_channel_logo(Path(filename): Path<String>) -> Response {
    let path = channel_logo_dir().join(&filename);
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("bin");
    let content_type = match ext {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        _ => "application/octet-stream",
    };

    match fs::read(&path).await {
        Ok(bytes) => ([(header::CONTENT_TYPE, content_type)], Body::from(bytes)).into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

fn channel_logo_dir() -> std::path::PathBuf {
    data_path("iptv/logos")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_save_form_defaults_missing_checkboxes_to_false() {
        let fields = HashMap::from([
            ("id".to_string(), "7".to_string()),
            ("number".to_string(), "101".to_string()),
            ("name".to_string(), "No Flags".to_string()),
            ("ffmpeg_profile_id".to_string(), "3".to_string()),
            ("streaming_mode".to_string(), "5".to_string()),
        ]);

        let form = ChannelSaveForm::from_fields(&fields).into_channel_form();

        assert_eq!(form.id, 7);
        assert_eq!(form.number, "101");
        assert_eq!(form.ffmpeg_profile_id, 3);
        assert_eq!(form.streaming_mode, 5);
        assert!(!form.is_enabled);
        assert!(!form.show_in_epg);
    }

    #[test]
    fn channel_save_form_parses_checked_boolean_values() {
        let fields = HashMap::from([
            ("is_enabled".to_string(), "true".to_string()),
            ("show_in_epg".to_string(), "on".to_string()),
            ("remove_logo".to_string(), "1".to_string()),
        ]);

        let input = ChannelSaveForm::from_fields(&fields);

        assert!(input.is_enabled);
        assert!(input.show_in_epg);
        assert!(input.remove_logo);
    }

    #[test]
    fn remove_logo_clears_stored_logo_when_no_external_url_is_supplied() {
        let fields = HashMap::from([
            ("logo_path".to_string(), "stored.png".to_string()),
            ("logo_content_type".to_string(), "image/png".to_string()),
            ("remove_logo".to_string(), "true".to_string()),
        ]);

        let form = ChannelSaveForm::from_fields(&fields).into_channel_form();

        assert!(form.logo_path.is_empty());
        assert!(form.logo_content_type.is_empty());
        assert!(form.external_logo_url.is_empty());
    }

    #[test]
    fn remove_logo_does_not_clear_external_logo_replacement() {
        let fields = HashMap::from([
            ("logo_path".to_string(), "stored.png".to_string()),
            ("logo_content_type".to_string(), "image/png".to_string()),
            (
                "external_logo_url".to_string(),
                "https://example.test/logo.png".to_string(),
            ),
            ("remove_logo".to_string(), "true".to_string()),
        ]);

        let form = ChannelSaveForm::from_fields(&fields).into_channel_form();

        assert_eq!(form.logo_path, "stored.png");
        assert_eq!(form.logo_content_type, "image/png");
        assert_eq!(form.external_logo_url, "https://example.test/logo.png");
    }

    #[test]
    fn channel_number_form_collects_sorted_number_updates() {
        let fields = HashMap::from([
            ("number_12".to_string(), "12.1".to_string()),
            ("number_4".to_string(), "4".to_string()),
            ("not_a_number".to_string(), "ignored".to_string()),
        ]);

        let updates = channel_number_updates_from_fields(&fields);

        assert_eq!(
            updates
                .iter()
                .map(|update| (update.id, update.number.as_str()))
                .collect::<Vec<_>>(),
            vec![(4, "4"), (12, "12.1")]
        );
    }
}
