use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Redirect, Response},
};
use ersatzrs_app::{AdminTroubleshootingRuntimeError, PlaybackPreviewRequest};
use serde::Deserialize;

use crate::host::state::AppState;

#[derive(Debug, Deserialize)]
pub struct PlaybackPreviewQuery {
    #[serde(rename = "mediaItem")]
    media_item: Option<i64>,
    channel: Option<i64>,
    #[serde(rename = "ffmpegProfile")]
    ffmpeg_profile: Option<i64>,
    #[serde(default, rename = "streamingMode")]
    streaming_mode: Option<String>,
    #[serde(default)]
    watermark: Vec<i64>,
    #[serde(default, rename = "graphicsElement")]
    graphics_element: Vec<i64>,
    #[serde(default, rename = "streamSelector")]
    stream_selector: Option<String>,
    #[serde(default, rename = "preferredAudioTitle")]
    preferred_audio_title: Option<String>,
    #[serde(default, rename = "subtitleId")]
    subtitle_id: Option<i64>,
    #[serde(default, rename = "seekSeconds")]
    seek_seconds: Option<i64>,
    #[serde(default)]
    start: Option<String>,
}

impl From<PlaybackPreviewQuery> for PlaybackPreviewRequest {
    fn from(query: PlaybackPreviewQuery) -> Self {
        Self {
            media_item_id: query.media_item,
            channel_id: query.channel,
            ffmpeg_profile_id: query.ffmpeg_profile,
            streaming_mode: query.streaming_mode,
            watermark_ids: query.watermark,
            graphics_element_ids: query.graphics_element,
            stream_selector: query.stream_selector,
            preferred_audio_title: query.preferred_audio_title,
            subtitle_id: query.subtitle_id,
            seek_seconds: query.seek_seconds,
            start: query.start,
        }
    }
}

pub async fn playback_preview(
    State(state): State<AppState>,
    Query(query): Query<PlaybackPreviewQuery>,
) -> Response {
    match state.application.start_playback_preview(query.into()).await {
        Ok(()) => Redirect::temporary("/iptv/session/.troubleshooting/live.m3u8").into_response(),
        Err(AdminTroubleshootingRuntimeError::NotFound(error)) => {
            tracing::warn!(%error, "playback troubleshooting preview cannot be started");
            StatusCode::NOT_FOUND.into_response()
        }
        Err(AdminTroubleshootingRuntimeError::Unavailable(error)) => {
            tracing::warn!(%error, "failed to start playback troubleshooting preview");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

pub async fn playback_archive(State(state): State<AppState>) -> Response {
    match state.application.build_troubleshooting_archive().await {
        Ok(Some(bytes)) => zip_response(
            bytes,
            &format!(
                "ersatzrs-troubleshooting-{}.zip",
                chrono::Utc::now().timestamp()
            ),
        ),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => {
            tracing::warn!(%error, "failed to build troubleshooting archive");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

pub async fn playback_sample(
    State(state): State<AppState>,
    Path(media_item_id): Path<i64>,
) -> Response {
    match state
        .application
        .build_media_sample_archive(media_item_id)
        .await
    {
        Ok(Some(bytes)) => zip_response(
            bytes,
            &format!(
                "ersatzrs-media-sample-{}.zip",
                chrono::Utc::now().timestamp()
            ),
        ),
        Ok(None) | Err(AdminTroubleshootingRuntimeError::NotFound(_)) => {
            StatusCode::NOT_FOUND.into_response()
        }
        Err(AdminTroubleshootingRuntimeError::Unavailable(error)) => {
            tracing::warn!(%error, media_item_id, "failed to build troubleshooting media sample");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

fn zip_response(bytes: Vec<u8>, filename: &str) -> Response {
    let mut response = Body::from(bytes).into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/zip"),
    );
    if let Ok(value) = HeaderValue::from_str(&format!("attachment; filename=\"{filename}\"")) {
        response
            .headers_mut()
            .insert(header::CONTENT_DISPOSITION, value);
    }
    response
}
