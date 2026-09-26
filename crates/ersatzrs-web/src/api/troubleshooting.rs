use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Redirect, Response},
};
use ersatzrs_app::{AdminTroubleshootingRuntimeError, PlaybackPreviewRequest};
use ersatzrs_domain::StreamingEngine;
use serde::Deserialize;

use crate::host::state::AppState;

#[derive(Debug, Default)]
pub struct PlaybackPreviewQuery {
    streaming_engine: StreamingEngine,
    media_item: Option<i64>,
    channel: Option<i64>,
    ffmpeg_profile: Option<i64>,
    streaming_mode: Option<String>,
    watermark: Vec<i64>,
    graphics_element: Vec<i64>,
    stream_selector: Option<String>,
    preferred_audio_title: Option<String>,
    subtitle_id: Option<i64>,
    seek_seconds: Option<i64>,
    start: Option<String>,
    start_from_beginning: bool,
}

impl<'de> Deserialize<'de> for PlaybackPreviewQuery {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // HTML multi-selects submit repeated keys; an unselected subtitle is empty.
        let fields = Vec::<(String, String)>::deserialize(deserializer)?;
        let mut query = Self::default();
        let mut seen = std::collections::HashSet::new();
        for (key, value) in fields {
            if !matches!(key.as_str(), "watermark" | "graphicsElement") && !seen.insert(key.clone())
            {
                return Err(serde::de::Error::custom("duplicate preview query field"));
            }
            let integer = || -> Result<Option<i64>, D::Error> {
                if value.is_empty() {
                    Ok(None)
                } else {
                    value.parse().map(Some).map_err(serde::de::Error::custom)
                }
            };
            match key.as_str() {
                "streamingEngine" => {
                    query.streaming_engine = match value.as_str() {
                        "Legacy" | "0" => StreamingEngine::Legacy,
                        "Next" | "1" => StreamingEngine::Next,
                        _ => return Err(serde::de::Error::custom("invalid streaming engine")),
                    }
                }
                "mediaItem" => query.media_item = integer()?,
                "channel" => query.channel = integer()?,
                "ffmpegProfile" => query.ffmpeg_profile = integer()?,
                "subtitleId" => query.subtitle_id = integer()?,
                "seekSeconds" => query.seek_seconds = integer()?,
                "watermark" => query.watermark.extend(integer()?),
                "graphicsElement" => query.graphics_element.extend(integer()?),
                "streamingMode" => query.streaming_mode = Some(value),
                "streamSelector" => query.stream_selector = Some(value),
                "preferredAudioTitle" => query.preferred_audio_title = Some(value),
                "start" => query.start = Some(value),
                "startFromBeginning" => {
                    query.start_from_beginning = value.parse().map_err(serde::de::Error::custom)?;
                }
                _ => {}
            }
        }
        Ok(query)
    }
}

impl From<PlaybackPreviewQuery> for PlaybackPreviewRequest {
    fn from(mut query: PlaybackPreviewQuery) -> Self {
        if query.streaming_engine == StreamingEngine::Next {
            query.watermark.truncate(1);
        }
        Self {
            streaming_engine: query.streaming_engine,
            media_item_id: query.media_item,
            channel_id: query.channel,
            ffmpeg_profile_id: query.ffmpeg_profile,
            streaming_mode: query.streaming_mode,
            watermark_ids: query.watermark,
            graphics_element_ids: query.graphics_element,
            stream_selector: query.stream_selector,
            preferred_audio_title: query.preferred_audio_title,
            subtitle_id: query.subtitle_id,
            seek_seconds: if query.start_from_beginning {
                Some(0)
            } else {
                query.seek_seconds
            },
            start: query.start,
        }
    }
}

pub async fn playback_preview(
    State(state): State<AppState>,
    Query(query): Query<PlaybackPreviewQuery>,
) -> Response {
    let playlist = match query.streaming_engine {
        StreamingEngine::Legacy => "/iptv/session/.troubleshooting/live.m3u8",
        StreamingEngine::Next => "/iptv/session/.troubleshooting/ffmpeg.m3u8",
    };
    match state.application.start_playback_preview(query.into()).await {
        Ok(()) => Redirect::temporary(playlist).into_response(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_engine_defaults_to_legacy_and_next_retains_only_the_first_watermark() {
        let legacy: PlaybackPreviewQuery =
            serde_urlencoded::from_str("watermark=1&watermark=2").unwrap();
        let legacy: PlaybackPreviewRequest = legacy.into();
        assert_eq!(legacy.streaming_engine, StreamingEngine::Legacy);
        assert_eq!(legacy.watermark_ids, [1, 2]);
        for value in ["Next", "1"] {
            let next: PlaybackPreviewQuery = serde_urlencoded::from_str(&format!(
                "streamingEngine={value}&watermark=1&watermark=2"
            ))
            .unwrap();
            let next: PlaybackPreviewRequest = next.into();
            assert_eq!(next.streaming_engine, StreamingEngine::Next);
            assert_eq!(next.watermark_ids, [1]);
        }
        assert!(
            serde_urlencoded::from_str::<PlaybackPreviewQuery>("streamingEngine=unknown").is_err()
        );
    }

    #[test]
    fn preview_query_accepts_empty_subtitle_and_repeated_watermarks_from_html_forms() {
        let query: PlaybackPreviewQuery = serde_urlencoded::from_str(
            "mediaItem=10&ffmpegProfile=1&subtitleId=&watermark=1&watermark=2",
        )
        .unwrap();
        assert_eq!(query.subtitle_id, None);
        assert_eq!(query.watermark, [1, 2]);
    }

    #[test]
    fn preview_query_start_from_beginning_overrides_the_default_seek() {
        let query: PlaybackPreviewQuery = serde_urlencoded::from_str(
            "mediaItem=10&ffmpegProfile=1&seekSeconds=15&startFromBeginning=true",
        )
        .unwrap();
        let request: PlaybackPreviewRequest = query.into();
        assert_eq!(request.seek_seconds, Some(0));
    }

    #[test]
    fn preview_query_preserves_track_titles_and_multiple_graphics() {
        let query: PlaybackPreviewQuery = serde_urlencoded::from_str(
            "preferredAudioTitle=Dutch+%26+English&graphicsElement=3&graphicsElement=4&seekSeconds=15",
        )
        .unwrap();
        let request: PlaybackPreviewRequest = query.into();
        assert_eq!(
            request.preferred_audio_title.as_deref(),
            Some("Dutch & English")
        );
        assert_eq!(request.graphics_element_ids, [3, 4]);
        assert_eq!(request.seek_seconds, Some(15));
    }

    #[test]
    fn preview_query_rejects_invalid_numbers_and_ambiguous_scalar_fields() {
        for query in [
            "subtitleId=invalid",
            "watermark=invalid",
            "mediaItem=1&mediaItem=2",
        ] {
            assert!(serde_urlencoded::from_str::<PlaybackPreviewQuery>(query).is_err());
        }
    }
}
