use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Redirect, Response},
    Form,
};
use ersatzrs_app::{AdminTroubleshootingRuntimeError, PlaybackPreviewRequest};
use ersatzrs_domain::StreamingEngine;
use serde::Deserialize;

use crate::host::state::AppState;

#[derive(Debug, Default)]
pub struct PlaybackPreviewQuery {
    streaming_engine: Option<StreamingEngine>,
    media_item: Option<i64>,
    channel: Option<i64>,
    ffmpeg_profile: Option<i64>,
    streaming_mode: Option<String>,
    watermark: Vec<i64>,
    graphics_element: Vec<i64>,
    stream_selector: Option<String>,
    preferred_audio_title: Option<String>,
    subtitle_id: Option<i64>,
    music_video_credits_template: Option<String>,
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
                    query.streaming_engine = Some(match value.as_str() {
                        "Legacy" | "0" => StreamingEngine::Legacy,
                        "Next" | "1" => StreamingEngine::Next,
                        _ => return Err(serde::de::Error::custom("invalid streaming engine")),
                    });
                }
                "mediaItem" => query.media_item = integer()?,
                "channel" => query.channel = integer()?,
                "ffmpegProfile" => query.ffmpeg_profile = integer()?,
                "subtitleId" => query.subtitle_id = integer()?,
                "seekSeconds" => query.seek_seconds = integer()?,
                "watermark" => query.watermark.extend(integer()?),
                "graphicsElement" => query.graphics_element.extend(integer()?),
                "streamingMode" => query.streaming_mode = Some(value),
                "musicVideoCreditsTemplate" => query.music_video_credits_template = Some(value),
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

impl PlaybackPreviewQuery {
    async fn into_request(
        mut self,
        state: &AppState,
    ) -> Result<PlaybackPreviewRequest, AdminTroubleshootingRuntimeError> {
        // Explicit request choices win. Only an omitted channel-bound engine
        // reads the owning channel; standalone preview defaults to Next.
        if self.streaming_engine.is_none() {
            if let Some(id) = self.channel.filter(|id| *id > 0) {
                let data = state
                    .application
                    .ports()
                    .troubleshooting()
                    .load_playback_context_admin(0, id)
                    .await
                    .map_err(|_| {
                        AdminTroubleshootingRuntimeError::unavailable(
                            "channel preview configuration could not be loaded",
                        )
                    })?;
                let channel = data.channel.ok_or_else(|| {
                    AdminTroubleshootingRuntimeError::NotFound("channel does not exist".to_owned())
                })?;
                self.streaming_engine = Some(channel.streaming_engine);
            }
        }
        Ok(self.into())
    }
}

impl From<PlaybackPreviewQuery> for PlaybackPreviewRequest {
    fn from(query: PlaybackPreviewQuery) -> Self {
        Self {
            streaming_engine: query.streaming_engine.unwrap_or(StreamingEngine::Next),
            media_item_id: query.media_item,
            channel_id: query.channel,
            ffmpeg_profile_id: query.ffmpeg_profile,
            streaming_mode: query.streaming_mode,
            watermark_ids: query.watermark,
            graphics_element_ids: query.graphics_element,
            stream_selector: query.stream_selector,
            preferred_audio_title: query.preferred_audio_title,
            subtitle_id: query.subtitle_id,
            music_video_credits_template: query.music_video_credits_template,
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
    let request = match query.into_request(&state).await {
        Ok(request) => request,
        Err(error) => return playback_preview_response(Err(error)),
    };
    playback_preview_response(state.application.start_playback_preview(request).await)
}

fn playback_preview_response(result: Result<(), AdminTroubleshootingRuntimeError>) -> Response {
    match result {
        Ok(()) => Redirect::temporary("/iptv/session/.troubleshooting/live.m3u8").into_response(),
        Err(AdminTroubleshootingRuntimeError::NotFound(error)) => {
            tracing::warn!(%error, "playback troubleshooting preview cannot be started");
            preview_startup_failure(StatusCode::NOT_FOUND, &error)
        }
        Err(AdminTroubleshootingRuntimeError::Unavailable(error)) => {
            tracing::warn!(%error, "failed to start playback troubleshooting preview");
            preview_startup_failure(StatusCode::INTERNAL_SERVER_ERROR, &error)
        }
    }
}

pub async fn queue_preview(
    State(state): State<AppState>,
    Query(query): Query<PlaybackPreviewQuery>,
) -> Response {
    let request = match query.into_request(&state).await {
        Ok(request) => request,
        Err(error) => return playback_preview_response(Err(error)),
    };
    match state.application.queue_playback_preview(request).await {
        Ok(status) => (
            StatusCode::ACCEPTED,
            [(header::CACHE_CONTROL, "no-store")],
            axum::Json(status),
        )
            .into_response(),
        Err(error) => preview_startup_failure(StatusCode::CONFLICT, &error.to_string()),
    }
}

pub async fn owned_preview_status(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    match state.application.playback_preview_status(id).await {
        Ok(Some(status)) => {
            ([(header::CACHE_CONTROL, "no-store")], axum::Json(status)).into_response()
        }
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub async fn stop_owned_preview(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    match state.application.stop_playback_preview(id).await {
        Ok(Some(status)) => {
            ([(header::CACHE_CONTROL, "no-store")], axum::Json(status)).into_response()
        }
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub async fn owned_preview_file(
    State(state): State<AppState>,
    Path((id, filename)): Path<(String, String)>,
) -> Response {
    match state
        .application
        .playback_preview_file(id, filename.clone())
        .await
    {
        Ok(Some(bytes)) => {
            let mut response = crate::transport::iptv::hls::hls_file_response(&filename, bytes);
            response
                .headers_mut()
                .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
            response
        }
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub async fn start_preview_form(
    State(state): State<AppState>,
    Form(query): Form<PlaybackPreviewQuery>,
) -> Response {
    let request = match query.into_request(&state).await {
        Ok(request) => request,
        Err(error) => return playback_preview_response(Err(error)),
    };
    match state.application.queue_playback_preview(request).await {
        Ok(status) => Redirect::to(&status.return_url).into_response(),
        Err(error) => preview_startup_failure(StatusCode::CONFLICT, &error.to_string()),
    }
}

pub async fn stop_preview_form(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    match state.application.stop_playback_preview(id).await {
        Ok(Some(status)) => Redirect::to(&status.return_url).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

// Return only a stable diagnostic code. Runtime errors may contain provider
// credentials or private paths; keep their details in the host diagnostic log.
fn preview_startup_failure(status: StatusCode, error: &str) -> Response {
    let code = if error.contains("troubleshooting playback is already running") {
        "busy"
    } else if error.contains("cleanup is unconfirmed") {
        "cleanup"
    } else if error.starts_with("failed to decode channel streaming_mode:") {
        "configuration"
    } else {
        "startup"
    };
    (
        status,
        [(header::CACHE_CONTROL, "no-store")],
        axum::Json(serde_json::json!({"code": code})),
    )
        .into_response()
}

pub async fn playback_log(State(state): State<AppState>) -> Response {
    match state.application.playback_preview_log().await {
        Ok(log) => ([(header::CACHE_CONTROL, "no-store")], axum::Json(log)).into_response(),
        Err(error) => {
            tracing::warn!(%error, "failed to read playback troubleshooting log");
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
    fn successful_preview_response_redirects_to_generated_live_playlist() {
        let response = playback_preview_response(Ok(()));
        assert_eq!(response.status(), StatusCode::TEMPORARY_REDIRECT);
        assert_eq!(
            response.headers()[header::LOCATION],
            "/iptv/session/.troubleshooting/live.m3u8"
        );
    }

    #[tokio::test]
    async fn preview_startup_failure_exposes_only_safe_action_codes() {
        for (error, code) in [
            (
                "failed to decode channel streaming_mode: unknown enum value 99",
                "configuration",
            ),
            ("troubleshooting playback is already running", "busy"),
            (
                "troubleshooting process cleanup is unconfirmed; restart is required",
                "cleanup",
            ),
            (
                "provider URL https://user:private@example.invalid/?token=private",
                "startup",
            ),
        ] {
            let response = preview_startup_failure(StatusCode::INTERNAL_SERVER_ERROR, error);
            assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
            assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
            let body = axum::body::to_bytes(response.into_body(), 1024)
                .await
                .unwrap();
            let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(value, serde_json::json!({"code": code}));
        }
    }

    #[test]
    fn preview_query_preserves_optional_unicode_credits_and_rejects_duplicates() {
        for engine in ["Legacy", "Next"] {
            let query: PlaybackPreviewQuery = serde_urlencoded::from_str(&format!(
                "streamingEngine={engine}&musicVideoCreditsTemplate=Caf%C3%A9+%26+Title&subtitleId=9"
            )).unwrap();
            let request: PlaybackPreviewRequest = query.into();
            assert_eq!(
                request.music_video_credits_template.as_deref(),
                Some("Café & Title")
            );
            assert_eq!(request.subtitle_id, Some(9));
        }
        let query: PlaybackPreviewQuery =
            serde_urlencoded::from_str("musicVideoCreditsTemplate=").unwrap();
        assert_eq!(query.music_video_credits_template.as_deref(), Some(""));
        assert!(serde_urlencoded::from_str::<PlaybackPreviewQuery>(
            "musicVideoCreditsTemplate=a&musicVideoCreditsTemplate=b"
        )
        .is_err());
    }

    #[tokio::test]
    async fn preview_request_resolves_stored_channel_engine_and_preserves_explicit_and_standalone_choices(
    ) {
        use ersatzrs_core::repositories::{ChannelRepository, FfmpegProfileRepository};
        use sqlx::Executor;
        let state = AppState::for_tests(crate::host::state::IptvAccessTokenVerifier::disabled());
        sqlx::migrate!("../../migrations")
            .run(&state.pool)
            .await
            .unwrap();
        state.pool.execute(r#"INSERT OR IGNORE INTO "Resolution" ("Id", "Name", "Width", "Height") VALUES (1, '1920x1080', 1920, 1080)"#).await.unwrap();
        let repositories = ersatzrs_infra::repositories::make_repositories(state.pool.clone());
        repositories
            .ffmpeg_profile
            .upsert(&ersatzrs_domain::FFmpegProfile {
                id: 1,
                name: "Preview".to_owned(),
                resolution_id: 1,
                ..ersatzrs_domain::FFmpegProfile::default()
            })
            .await
            .unwrap();
        for (id, engine) in [(10, StreamingEngine::Legacy), (11, StreamingEngine::Next)] {
            repositories
                .channel
                .upsert(&ersatzrs_domain::Channel {
                    id,
                    number: id.to_string(),
                    name: "Preview".to_owned(),
                    ffmpeg_profile_id: 1,
                    unique_id: uuid::Uuid::from_u128(u128::try_from(id).unwrap()),
                    streaming_engine: engine,
                    preferred_audio_language_code: Some("nld".to_owned()),
                    preferred_subtitle_language_code: Some("eng".to_owned()),
                    ..ersatzrs_domain::Channel::default()
                })
                .await
                .unwrap();
            for media in ["", "&mediaItem=20"] {
                let query: PlaybackPreviewQuery = serde_urlencoded::from_str(&format!(
                    "channel={id}{media}&subtitleId=5&watermark=1&watermark=2"
                ))
                .unwrap();
                let request = query.into_request(&state).await.unwrap();
                assert_eq!(request.streaming_engine, engine);
                assert_eq!(request.channel_id, Some(id));
                assert_eq!(
                    request.media_item_id,
                    if media.is_empty() { None } else { Some(20) }
                );
                assert_eq!(request.subtitle_id, Some(5));
                assert_eq!(request.watermark_ids, [1, 2]);
                for (value, explicit_engine) in [
                    ("Legacy", StreamingEngine::Legacy),
                    ("Next", StreamingEngine::Next),
                ] {
                    let explicit: PlaybackPreviewQuery = serde_urlencoded::from_str(&format!(
                        "channel={id}{media}&streamingEngine={value}"
                    ))
                    .unwrap();
                    assert_eq!(
                        explicit
                            .into_request(&state)
                            .await
                            .unwrap()
                            .streaming_engine,
                        explicit_engine
                    );
                }
            }
        }
        for (query, engine) in [
            ("mediaItem=20", StreamingEngine::Next),
            (
                "mediaItem=20&streamingEngine=Legacy",
                StreamingEngine::Legacy,
            ),
        ] {
            let query: PlaybackPreviewQuery = serde_urlencoded::from_str(query).unwrap();
            assert_eq!(
                query.into_request(&state).await.unwrap().streaming_engine,
                engine
            );
        }
        let missing: PlaybackPreviewQuery = serde_urlencoded::from_str("channel=999").unwrap();
        assert!(matches!(
            missing.into_request(&state).await,
            Err(AdminTroubleshootingRuntimeError::NotFound(_))
        ));
    }

    #[test]
    fn preview_engine_defaults_to_next_and_explicit_engines_retain_all_watermarks() {
        let legacy: PlaybackPreviewQuery =
            serde_urlencoded::from_str("watermark=1&watermark=2").unwrap();
        let legacy: PlaybackPreviewRequest = legacy.into();
        assert_eq!(legacy.streaming_engine, StreamingEngine::Next);
        assert_eq!(legacy.watermark_ids, [1, 2]);
        for value in ["Next", "1", "Legacy", "0"] {
            let next: PlaybackPreviewQuery = serde_urlencoded::from_str(&format!(
                "streamingEngine={value}&watermark=1&watermark=2"
            ))
            .unwrap();
            let next: PlaybackPreviewRequest = next.into();
            assert_eq!(
                next.streaming_engine,
                if matches!(value, "Legacy" | "0") {
                    StreamingEngine::Legacy
                } else {
                    StreamingEngine::Next
                }
            );
            assert_eq!(next.watermark_ids, [1, 2]);
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
