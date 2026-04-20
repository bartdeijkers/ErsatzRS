use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use ersatzrs_core::repositories::ChannelRepository;
use ersatzrs_domain::StreamingMode;
use ersatzrs_ffmpeg::OutputFormat;
use ersatzrs_infra::hls_worker::HlsSessionWorker;
use serde::Deserialize;
use std::sync::Arc;

use crate::iptv::access_token::AccessToken;
use crate::iptv::stream;
use crate::iptv::streaming_specs;
use crate::iptv::variant::build_multi_variant_playlist;
use crate::state::AppState;

#[derive(Debug, Deserialize, Default)]
pub struct HlsQuery {
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub access_token: Option<String>,
}

/// HLS content type emitted by every playlist response, matching the
/// `Content(playlist, "application/vnd.apple.mpegurl")` call sites in
/// `ErsatzTV`'s `IptvController`.
const HLS_CONTENT_TYPE: HeaderValue = HeaderValue::from_static("application/vnd.apple.mpegurl");

fn request_scheme(headers: &HeaderMap) -> String {
    headers
        .get("x-forwarded-proto")
        .and_then(|value| value.to_str().ok())
        .map_or_else(
            || "http".to_string(),
            |value| value.split(',').next().unwrap_or(value).trim().to_string(),
        )
}

fn request_host(headers: &HeaderMap) -> String {
    headers
        .get("x-forwarded-host")
        .or_else(|| headers.get(header::HOST))
        .and_then(|value| value.to_str().ok())
        .map_or_else(
            || "localhost".to_string(),
            |value| value.split(',').next().unwrap_or(value).trim().to_string(),
        )
}

fn hls_response(body: String) -> Response {
    let mut response = body.into_response();
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, HLS_CONTENT_TYPE);
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

async fn ensure_segmenter_worker_running(
    state: &AppState,
    channel_number: &str,
    channel_id: i64,
    output_format: OutputFormat,
    path_replacements: Vec<ersatzrs_core::path_replacement::PathReplacement>,
) {
    if let Some(worker) = state.hls_sessions.try_get(channel_number).await {
        worker.touch();
        return;
    }

    let working_dir = std::env::temp_dir()
        .join("ersatzrs-hls")
        .join(channel_number);
    let worker = HlsSessionWorker::new(
        channel_number.to_string(),
        channel_id,
        working_dir,
        output_format,
        path_replacements,
    );
    if state.hls_sessions.try_insert(worker.clone()).await {
        worker.start(
            state.pool.clone(),
            Arc::clone(&state.hls_sessions),
            Some(std::time::Duration::from_secs(30)),
        );
    } else if let Some(existing_worker) = state.hls_sessions.try_get(channel_number).await {
        existing_worker.touch();
    }
}

/// `GET /iptv/channel/{number}.m3u8` — mode-dispatched HLS entry point.
///
/// Ports `IptvController.GetHttpLiveStreamingVideo`: resolves `mode=mixed`
/// against the channel's configured streaming mode, then either:
///  * returns the multi-variant playlist for `segmenter`,
///  * returns `501 Not Implemented` for `hls-direct` (landing in a later slice),
///  * or redirects to the MPEG-TS variant for TS modes.
pub async fn serve_channel_m3u8(
    state: &AppState,
    channel_number: &str,
    headers: &HeaderMap,
    query: &HlsQuery,
) -> Response {
    let channel = match state
        .repositories
        .channel
        .get_by_number(channel_number)
        .await
    {
        Ok(channel) if channel.is_enabled => channel,
        Ok(_) => return StatusCode::NOT_FOUND.into_response(),
        Err(error) => {
            tracing::debug!(%error, channel_number, "channel not found for m3u8 request");
            return StatusCode::NOT_FOUND.into_response();
        }
    };

    let access_token = AccessToken::new(query.access_token.clone());
    let mut mode = query.mode.clone().unwrap_or_default();
    if mode.is_empty() || mode == "mixed" {
        mode = match channel.streaming_mode {
            StreamingMode::HttpLiveStreamingDirect => "hls-direct".to_string(),
            StreamingMode::HttpLiveStreamingSegmenter => "segmenter".to_string(),
            StreamingMode::TransportStream | StreamingMode::TransportStreamHybrid => {
                let target = format!("/iptv/channel/{channel_number}.ts{}", access_token.query());
                return axum::response::Redirect::temporary(&target).into_response();
            }
        };
    }

    match mode.as_str() {
        "segmenter" | "segmenter-v2" | "segmenter-fmp4" => {
            let output_format = if mode == "segmenter-fmp4" {
                OutputFormat::HlsMp4
            } else {
                OutputFormat::Hls
            };
            let scheme = request_scheme(headers);
            let host = request_host(headers);
            let spec = match streaming_specs::load_spec(&state.pool, channel.id).await {
                Ok(spec) => spec,
                Err(error) => {
                    tracing::warn!(
                        %error,
                        channel_number = %channel.number,
                        "failed to load channel streaming spec; emitting playlist with default bandwidth"
                    );
                    None
                }
            };

            let path_replacements = match stream::load_path_replacements(
                &state.repositories.media_source,
            )
            .await
            {
                Ok(replacements) => replacements,
                Err(error) => {
                    tracing::warn!(%error, channel_number = %channel.number, "failed to load path replacements for hls worker; continuing with raw path");
                    Vec::new()
                }
            };

            ensure_segmenter_worker_running(
                state,
                &channel.number,
                channel.id,
                output_format,
                path_replacements,
            )
            .await;

            let body = build_multi_variant_playlist(
                &scheme,
                &host,
                channel_number,
                spec.as_ref(),
                &access_token,
            );
            hls_response(body)
        }
        "hls-direct" => {
            tracing::info!(
                channel_number = %channel.number,
                "hls-direct mode requested; handler arrives in a later Phase 7 slice"
            );
            (
                StatusCode::NOT_IMPLEMENTED,
                "hls-direct output is not available yet",
            )
                .into_response()
        }
        _ => {
            let target = format!("/iptv/channel/{channel_number}.ts{}", access_token.query());
            axum::response::Redirect::temporary(&target).into_response()
        }
    }
}

/// `GET /iptv/session/{number}/hls.m3u8` — trimmed variant playlist.
///
/// Ports `IptvController.GetLivePlaylist`: the session worker owns the
/// real playlist, so this handler looks up the worker in the
/// [`HlsSessionRegistry`] and reports 503 until Slice 2 lands the worker.
///
/// [`HlsSessionRegistry`]: ersatzrs_core::hls::HlsSessionRegistry
pub async fn get_session_playlist(
    State(state): State<AppState>,
    Path(channel_number): Path<String>,
) -> Response {
    if let Some(handle) = state.hls_sessions.try_get(&channel_number).await {
        handle.touch();
        return match handle.trim_playlist().await {
            Some(playlist) => hls_response(playlist),
            None => (StatusCode::SERVICE_UNAVAILABLE, "no segments yet").into_response(),
        };
    }

    (StatusCode::SERVICE_UNAVAILABLE, "hls session not running").into_response()
}

pub async fn get_session_segment(
    State(_state): State<AppState>,
    Path((channel_number, filename)): Path<(String, String)>,
) -> Response {
    if filename.contains("..") || filename.contains('/') || filename.contains('\\') {
        return (StatusCode::BAD_REQUEST, "invalid segment filename").into_response();
    }

    let path = std::env::temp_dir()
        .join("ersatzrs-hls")
        .join(&channel_number)
        .join(&filename);

    match tokio::fs::read(path).await {
        Ok(body) => {
            let mut response = body.into_response();
            match std::path::Path::new(&filename)
                .extension()
                .and_then(|extension| extension.to_str())
                .map(str::to_ascii_lowercase)
                .as_deref()
            {
                Some("ts") => {
                    response
                        .headers_mut()
                        .insert(header::CONTENT_TYPE, HeaderValue::from_static("video/mp2t"));
                }
                Some("mp4") => {
                    response
                        .headers_mut()
                        .insert(header::CONTENT_TYPE, HeaderValue::from_static("video/mp4"));
                }
                Some("m4s") => {
                    response.headers_mut().insert(
                        header::CONTENT_TYPE,
                        HeaderValue::from_static("video/iso.segment"),
                    );
                }
                Some(_) | None => {}
            }
            response
                .headers_mut()
                .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
            response
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            StatusCode::NOT_FOUND.into_response()
        }
        Err(error) => {
            tracing::warn!(%error, channel_number, filename, "failed to read hls segment");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}
