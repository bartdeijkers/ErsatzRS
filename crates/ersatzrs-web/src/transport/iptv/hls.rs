use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use chrono::{DateTime, NaiveDateTime, Utc};
use ersatzrs_core::repositories::{ChannelRepository, ConfigRepository};
use ersatzrs_domain::StreamingMode;
use ersatzrs_ffmpeg::OutputFormat;
use ersatzrs_infra::hls_worker::HlsSessionWorker;
use serde::Deserialize;
use std::{collections::HashMap, sync::Arc};

use crate::host::state::AppState;
use crate::transport::iptv::access_token::AccessToken;
use crate::transport::iptv::stream;
use crate::transport::iptv::streaming_specs;
use crate::transport::iptv::variant::build_multi_variant_playlist;

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
const CONFIG_KEY_HLS_DIRECT_OUTPUT_FORMAT: &str = "ffmpeg.hls_direct.output_format";

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

async fn serve_hls_direct_playlist(
    state: &AppState,
    channel_id: i64,
    channel_number: &str,
    scheme: &str,
    host: &str,
    access_token: &AccessToken,
) -> Response {
    let active = match stream::load_active_media_path(&state.pool, channel_id, Utc::now()).await {
        Ok(Some(active)) => active,
        Ok(None) => {
            tracing::info!(channel_number, "no active playout item for hls-direct");
            return (StatusCode::SERVICE_UNAVAILABLE, "no active playout item").into_response();
        }
        Err(error) => {
            tracing::error!(%error, channel_number, "failed to load active playout item for hls-direct");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let output_format = load_hls_direct_output_format(state).await;
    let index = next_hls_direct_index(state, channel_id, &active.playout_start).await;
    let time_remaining =
        format_time_remaining(&active.playout_finish).unwrap_or_else(|| "10.00".to_string());
    let body = build_hls_direct_playlist(
        scheme,
        host,
        channel_number,
        stream::extension_for_output_format(output_format),
        index,
        &time_remaining,
        access_token,
    );
    hls_response(body)
}

async fn load_hls_direct_output_format(state: &AppState) -> OutputFormat {
    let configured = state
        .repositories
        .config
        .get(CONFIG_KEY_HLS_DIRECT_OUTPUT_FORMAT)
        .await
        .ok()
        .flatten();
    parse_hls_direct_output_format(configured.as_deref())
}

fn parse_hls_direct_output_format(value: Option<&str>) -> OutputFormat {
    let normalized = value.unwrap_or_default().trim().to_ascii_lowercase();
    match normalized.as_str() {
        "1" | "mkv" | "matroska" => OutputFormat::Mkv,
        "3" | "mp4" => OutputFormat::Mp4,
        _ => OutputFormat::MpegTs,
    }
}

async fn next_hls_direct_index(state: &AppState, channel_id: i64, playout_start: &str) -> u64 {
    let mut indices = state.hls_direct_indices.write().await;
    next_channel_index(&mut indices, channel_id, playout_start)
}

fn next_channel_index(
    indices: &mut HashMap<i64, (String, u64)>,
    channel_id: i64,
    playout_start: &str,
) -> u64 {
    match indices.get_mut(&channel_id) {
        Some((existing_start, index)) if existing_start == playout_start => *index,
        Some((existing_start, index)) => {
            *existing_start = playout_start.to_string();
            *index += 1;
            *index
        }
        None => {
            indices.insert(channel_id, (playout_start.to_string(), 1));
            1
        }
    }
}

fn format_time_remaining(playout_finish: &str) -> Option<String> {
    parse_sqlite_utc_timestamp(playout_finish).map(|finish| {
        let milliseconds = (finish - Utc::now()).num_milliseconds().unsigned_abs();
        format!(
            "{}.{:02}",
            milliseconds / 1_000,
            (milliseconds % 1_000) / 10
        )
    })
}

fn parse_sqlite_utc_timestamp(value: &str) -> Option<DateTime<Utc>> {
    for format in ["%Y-%m-%d %H:%M:%S%.f", "%Y-%m-%d %H:%M:%S"] {
        if let Ok(parsed) = NaiveDateTime::parse_from_str(value, format) {
            return Some(DateTime::<Utc>::from_naive_utc_and_offset(parsed, Utc));
        }
    }
    None
}

fn build_hls_direct_playlist(
    scheme: &str,
    host: &str,
    channel_number: &str,
    extension: &str,
    index: u64,
    time_remaining: &str,
    access_token: &AccessToken,
) -> String {
    format!(
        "#EXTM3U\n#EXT-X-VERSION:3\n#EXT-X-TARGETDURATION:10\n#EXT-X-MEDIA-SEQUENCE:{index}\n#EXT-X-DISCONTINUITY\n#EXTINF:{time_remaining},\n{scheme}://{host}/iptv/hls-direct/{channel_number}{extension}?index={index}{}\n",
        access_token.query_amp()
    )
}

fn parse_hls_direct_file(file: &str) -> &str {
    file.strip_suffix(".ts")
        .or_else(|| file.strip_suffix(".mp4"))
        .or_else(|| file.strip_suffix(".mkv"))
        .unwrap_or(file)
}

/// `GET /iptv/channel/{number}.m3u8` — mode-dispatched HLS entry point.
///
/// Ports `IptvController.GetHttpLiveStreamingVideo`: resolves `mode=mixed`
/// against the channel's configured streaming mode, then either:
///  * returns the multi-variant playlist for `segmenter`,
///  * returns the single-entry direct playlist for `hls-direct`,
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
            let scheme = request_scheme(headers);
            let host = request_host(headers);
            serve_hls_direct_playlist(
                state,
                channel.id,
                &channel.number,
                &scheme,
                &host,
                &access_token,
            )
            .await
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

pub async fn get_hls_direct_stream(
    State(state): State<AppState>,
    Path(file): Path<String>,
) -> Response {
    let channel_number = parse_hls_direct_file(&file);
    let output_format = load_hls_direct_output_format(&state).await;
    stream::serve_channel_stream(&state, channel_number, output_format).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_hls_direct_playlist_matches_expected_shape() {
        let playlist = build_hls_direct_playlist(
            "https",
            "example.test",
            "42",
            ".mp4",
            7,
            "91.25",
            &AccessToken::new(Some("abc123")),
        );

        insta::assert_snapshot!("hls_direct_playlist_mp4", playlist);
    }

    #[test]
    fn parse_hls_direct_output_format_supports_numeric_values() {
        assert_eq!(parse_hls_direct_output_format(Some("1")), OutputFormat::Mkv);
        assert_eq!(
            parse_hls_direct_output_format(Some("2")),
            OutputFormat::MpegTs
        );
        assert_eq!(parse_hls_direct_output_format(Some("3")), OutputFormat::Mp4);
        assert_eq!(
            parse_hls_direct_output_format(Some("999")),
            OutputFormat::MpegTs
        );
    }

    #[test]
    fn next_channel_index_advances_only_when_playout_changes() {
        let mut indices = HashMap::new();
        assert_eq!(
            next_channel_index(&mut indices, 5, "2026-04-20 20:00:00"),
            1
        );
        assert_eq!(
            next_channel_index(&mut indices, 5, "2026-04-20 20:00:00"),
            1
        );
        assert_eq!(
            next_channel_index(&mut indices, 5, "2026-04-20 20:30:00"),
            2
        );
        assert_eq!(
            next_channel_index(&mut indices, 6, "2026-04-20 20:00:00"),
            1
        );
    }

    #[test]
    fn parse_hls_direct_file_strips_supported_suffixes() {
        assert_eq!(parse_hls_direct_file("99.ts"), "99");
        assert_eq!(parse_hls_direct_file("99.mp4"), "99");
        assert_eq!(parse_hls_direct_file("99.mkv"), "99");
        assert_eq!(parse_hls_direct_file("99"), "99");
    }
}
