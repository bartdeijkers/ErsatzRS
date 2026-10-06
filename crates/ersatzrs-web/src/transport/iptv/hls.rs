use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use chrono::{DateTime, NaiveDateTime, Utc};
use ersatzrs_core::hls::{
    HlsAdmissionStatus, HlsSessionHandle, HlsSessionRegistry, HlsStartupReadiness,
};
use ersatzrs_core::repositories::{ChannelRepository, ConfigRepository};
use ersatzrs_domain::{
    Channel, ChannelIdleBehavior, FFmpegProfileAudioFormat, FFmpegProfileVideoFormat, StreamingMode,
};
use ersatzrs_ffmpeg::OutputFormat;
use ersatzrs_infra::{
    file_system_layout::{transcode_troubleshooting_folder, TRANSCODE_TROUBLESHOOTING_CHANNEL},
    hls_worker::{HlsPlaybackProfileOverride, HlsSessionOptions, HlsSessionWorker},
};
use serde::Deserialize;
use std::{collections::HashMap, sync::Arc, time::Duration};

use crate::host::state::AppState;
use crate::transport::iptv::access_token::AccessToken;
use crate::transport::iptv::stream;
use crate::transport::iptv::streaming_specs;
use crate::transport::iptv::variant::{
    build_multi_variant_playlist, build_preview_variant_playlist, with_next_subtitles,
};

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
const DEFAULT_SEGMENTER_TIMEOUT_SECONDS_USIZE: usize = 60;
const DEFAULT_INITIAL_SEGMENT_COUNT: usize = 1;
const MIN_STABLE_INITIAL_SEGMENT_COUNT: usize = 3;
const INITIAL_SEGMENT_POLL_INTERVAL: Duration = Duration::from_millis(250);
pub(crate) const HLS_AT_CAPACITY_MESSAGE: &str = "all HLS transcode slots are in use";
const HLS_DIRECT_LOOKAHEAD_SEGMENTS: u64 = 6;

pub(crate) fn request_scheme(headers: &HeaderMap) -> String {
    headers
        .get("x-forwarded-proto")
        .and_then(|value| value.to_str().ok())
        .map_or_else(
            || "http".to_string(),
            |value| value.split(',').next().unwrap_or(value).trim().to_string(),
        )
}

pub(crate) fn request_host(headers: &HeaderMap) -> String {
    headers
        .get("x-forwarded-host")
        .or_else(|| headers.get(header::HOST))
        .and_then(|value| value.to_str().ok())
        .map_or_else(
            || "localhost".to_string(),
            |value| value.split(',').next().unwrap_or(value).trim().to_string(),
        )
}

pub(super) fn hls_response(body: String) -> Response {
    let mut response = body.into_response();
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, HLS_CONTENT_TYPE);
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

fn hls_at_capacity_response() -> Response {
    (StatusCode::SERVICE_UNAVAILABLE, HLS_AT_CAPACITY_MESSAGE).into_response()
}

async fn startup_read<T>(
    deadline: std::time::Instant,
    read: impl std::future::Future<Output = ersatzrs_core::repositories::Result<T>>,
) -> ersatzrs_core::repositories::Result<T> {
    tokio::time::timeout_at(deadline.into(), read)
        .await
        .unwrap_or_else(|_| {
            Err(ersatzrs_core::repositories::RepositoryError::unavailable(
                "HLS startup",
                "deadline expired",
            ))
        })
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn ensure_segmenter_worker_running_before(
    state: &AppState,
    channel_number: &str,
    channel_id: i64,
    idle_behavior: ChannelIdleBehavior,
    output_format: OutputFormat,
    path_replacements: Vec<ersatzrs_core::path_replacement::PathReplacement>,
    deadline: std::time::Instant,
) {
    if let Some(worker) = state.hls_sessions.try_get(channel_number).await {
        worker.touch();
        return;
    }

    let Ok(segmenter_timeout) = tokio::time::timeout_at(
        deadline.into(),
        load_segmenter_idle_timeout(state, idle_behavior),
    )
    .await
    else {
        return;
    };
    let Ok(channel) = startup_read(
        deadline,
        state.repositories.channel.get_with_profile(channel_id),
    )
    .await
    else {
        return;
    };
    if channel.streaming_engine == ersatzrs_domain::StreamingEngine::Next {
        let executable = match ersatzrs_infra::next_engine::host::channel_binary() {
            Ok(executable) => executable,
            Err(error) => {
                tracing::warn!(channel_id, kind = ?error.kind(), "Next executable unavailable");
                return;
            }
        };
        let _task = state
            .next_engine
            .start(
                Arc::clone(&state.hls_sessions),
                ersatzrs_infra::next_engine::host::NextStart {
                    channel_id,
                    preview: false,
                    number: channel_number.to_owned(),
                    executable,
                    output: hls_working_dir(channel_number),
                    ffmpeg: ersatzrs_ffmpeg::ffmpeg_path().into(),
                    ffprobe: ersatzrs_ffmpeg::ffprobe_path().into(),
                    replacements: path_replacements,
                    idle_timeout: segmenter_timeout,
                    deadline: deadline.into(),
                },
            )
            .await;
        return;
    }
    let working_dir = hls_working_dir(channel_number);
    let worker = HlsSessionWorker::new_with_options(
        channel_number.to_string(),
        channel_id,
        HlsSessionOptions::standard(channel_number, working_dir),
        output_format,
        path_replacements,
        Arc::clone(&state.ffmpeg_runtime),
    );
    let Ok(minimum_segments) =
        tokio::time::timeout_at(deadline.into(), load_initial_segment_count(state)).await
    else {
        return;
    };
    if worker
        .configure_media_relay(Arc::clone(&state.next_engine.media))
        .is_err()
    {
        return;
    }
    worker.initialize_startup(deadline, minimum_segments);
    if state.hls_sessions.try_insert(worker.clone()).await {
        worker.start(
            state.pool.clone(),
            Arc::clone(&state.hls_sessions),
            segmenter_timeout,
        );
    } else if let Some(existing_worker) = state.hls_sessions.try_get(channel_number).await {
        existing_worker.touch();
    }
}

async fn load_segmenter_idle_timeout(
    state: &AppState,
    idle_behavior: ChannelIdleBehavior,
) -> Option<Duration> {
    if idle_behavior == ChannelIdleBehavior::KeepRunning {
        return None;
    }

    let seconds = load_config_usize(
        state,
        ersatzrs_domain::CONFIG_KEY_FFMPEG_SEGMENTER_TIMEOUT,
        DEFAULT_SEGMENTER_TIMEOUT_SECONDS_USIZE,
    )
    .await;
    segmenter_idle_timeout_for(idle_behavior, seconds)
}

fn segmenter_idle_timeout_for(
    idle_behavior: ChannelIdleBehavior,
    timeout_seconds: usize,
) -> Option<Duration> {
    if idle_behavior == ChannelIdleBehavior::KeepRunning {
        None
    } else {
        Some(Duration::from_secs(timeout_seconds as u64))
    }
}

pub(crate) async fn load_segmenter_startup_timeout(state: &AppState) -> Duration {
    let seconds = load_config_usize(
        state,
        ersatzrs_domain::CONFIG_KEY_FFMPEG_SEGMENTER_TIMEOUT,
        DEFAULT_SEGMENTER_TIMEOUT_SECONDS_USIZE,
    )
    .await;
    Duration::from_secs(seconds as u64)
}

pub(crate) async fn load_initial_segment_count(state: &AppState) -> usize {
    normalize_initial_segment_count(
        load_config_usize(
            state,
            ersatzrs_domain::CONFIG_KEY_FFMPEG_INITIAL_SEGMENT_COUNT,
            DEFAULT_INITIAL_SEGMENT_COUNT,
        )
        .await,
    )
}

fn normalize_initial_segment_count(segment_count: usize) -> usize {
    segment_count.max(MIN_STABLE_INITIAL_SEGMENT_COUNT)
}

async fn load_config_usize(state: &AppState, key: &str, default: usize) -> usize {
    state
        .repositories
        .config
        .get(key)
        .await
        .ok()
        .flatten()
        .and_then(|value| parse_positive_usize(&value))
        .unwrap_or(default)
}

fn parse_positive_usize(value: &str) -> Option<usize> {
    value
        .trim()
        .parse::<usize>()
        .ok()
        .filter(|value| *value > 0)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InitialSegmentWaitResult {
    Ready,
    AtCapacity,
    MissingSession,
    Timeout,
}

/// Admission is atomic in the registry; each viewer owns only its readiness wait.
/// Recover once when an existing worker ends, never after a timeout or denial.
pub(crate) async fn start_and_wait_for_initial_segments<F, Fut>(
    registry: &dyn HlsSessionRegistry,
    channel_number: &str,
    segment_count: usize,
    startup_timeout: Duration,
    deadline: std::time::Instant,
    mut start: F,
) -> InitialSegmentWaitResult
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    for attempt in 0..2 {
        let Ok(existing) =
            tokio::time::timeout_at(deadline.into(), registry.try_get(channel_number)).await
        else {
            return InitialSegmentWaitResult::Timeout;
        };
        if tokio::time::timeout_at(deadline.into(), start())
            .await
            .is_err()
        {
            return InitialSegmentWaitResult::Timeout;
        }
        let result = wait_for_initial_segments(
            registry,
            channel_number,
            segment_count,
            startup_timeout.min(deadline.saturating_duration_since(std::time::Instant::now())),
        )
        .await;
        if attempt == 0 && existing.is_some() && result == InitialSegmentWaitResult::MissingSession
        {
            continue;
        }
        return result;
    }
    InitialSegmentWaitResult::MissingSession
}

pub(crate) async fn wait_for_initial_segments(
    registry: &dyn HlsSessionRegistry,
    channel_number: &str,
    segment_count: usize,
    startup_timeout: Duration,
) -> InitialSegmentWaitResult {
    let deadline = tokio::time::Instant::now() + startup_timeout;
    let mut saw_at_capacity = false;
    let mut expected = None;
    while tokio::time::Instant::now() < deadline {
        let mut poll_at =
            (tokio::time::Instant::now() + INITIAL_SEGMENT_POLL_INTERVAL).min(deadline);
        let Ok(handle) = tokio::time::timeout_at(deadline, registry.try_get(channel_number)).await
        else {
            break;
        };
        if let Some(handle) = handle {
            if expected
                .as_ref()
                .is_some_and(|previous| !Arc::ptr_eq(previous, &handle))
            {
                return InitialSegmentWaitResult::MissingSession;
            }
            expected = Some(handle.clone());
            if handle.admission_status() == HlsAdmissionStatus::AtCapacity {
                saw_at_capacity = true;
            }
            match handle.startup_readiness() {
                HlsStartupReadiness::Ready => {
                    return publish_initial_readiness(registry, &handle, deadline).await;
                }
                HlsStartupReadiness::Failed => {
                    return if handle.admission_status() == HlsAdmissionStatus::AtCapacity {
                        InitialSegmentWaitResult::AtCapacity
                    } else {
                        InitialSegmentWaitResult::Timeout
                    }
                }
                HlsStartupReadiness::Pending { deadline } => {
                    poll_at = poll_at.min(deadline.into());
                    if std::time::Instant::now() >= deadline {
                        return if handle.admission_status() == HlsAdmissionStatus::AtCapacity {
                            InitialSegmentWaitResult::AtCapacity
                        } else {
                            InitialSegmentWaitResult::Timeout
                        };
                    }
                }
                HlsStartupReadiness::Legacy => {
                    match tokio::time::timeout_at(
                        deadline,
                        registry.initial_segments_ready(&handle),
                    )
                    .await
                    {
                        Ok(Some(true)) => return InitialSegmentWaitResult::Ready,
                        Ok(Some(false)) => {}
                        Ok(None) => return InitialSegmentWaitResult::MissingSession,
                        Err(_) => break,
                    }
                    let playlist = match read_initial_playlist(registry, &handle, deadline).await {
                        Ok(playlist) => playlist,
                        Err(InitialSegmentWaitResult::Timeout) => break,
                        Err(result) => return result,
                    };
                    if let Some(playlist) = playlist {
                        if count_playlist_segments(&playlist) >= segment_count.max(1) {
                            return publish_initial_readiness(registry, &handle, deadline).await;
                        }
                    }
                }
            }
        } else {
            return InitialSegmentWaitResult::MissingSession;
        }
        tokio::time::sleep_until(poll_at).await;
    }

    if saw_at_capacity {
        InitialSegmentWaitResult::AtCapacity
    } else {
        InitialSegmentWaitResult::Timeout
    }
}

async fn read_initial_playlist(
    registry: &dyn HlsSessionRegistry,
    handle: &Arc<dyn HlsSessionHandle>,
    deadline: tokio::time::Instant,
) -> Result<Option<String>, InitialSegmentWaitResult> {
    let removed = async {
        loop {
            if registry.initial_segments_ready(handle).await.is_none() {
                return;
            }
            tokio::time::sleep(INITIAL_SEGMENT_POLL_INTERVAL).await;
        }
    };
    tokio::select! {
        biased;
        () = tokio::time::sleep_until(deadline) => Err(InitialSegmentWaitResult::Timeout),
        () = removed => Err(InitialSegmentWaitResult::MissingSession),
        playlist = handle.trim_playlist() => Ok(playlist),
    }
}

async fn publish_initial_readiness(
    registry: &dyn HlsSessionRegistry,
    handle: &Arc<dyn HlsSessionHandle>,
    deadline: tokio::time::Instant,
) -> InitialSegmentWaitResult {
    match tokio::time::timeout_at(deadline, registry.mark_initial_segments_ready(handle)).await {
        Ok(true) => InitialSegmentWaitResult::Ready,
        Ok(false) => InitialSegmentWaitResult::MissingSession,
        Err(_) => InitialSegmentWaitResult::Timeout,
    }
}

fn count_playlist_segments(playlist: &str) -> usize {
    playlist
        .lines()
        .filter(|line| line.trim_start().starts_with("#EXTINF:"))
        .count()
}

/// Waiters never own cancellation of a potentially shared healthy worker.
async fn wait_for_retained_startup(handle: &dyn HlsSessionHandle) -> bool {
    loop {
        match handle.startup_readiness() {
            HlsStartupReadiness::Legacy | HlsStartupReadiness::Ready => return true,
            HlsStartupReadiness::Failed => return false,
            HlsStartupReadiness::Pending { deadline } => {
                if std::time::Instant::now() >= deadline {
                    return false;
                }
                tokio::time::sleep_until(
                    (std::time::Instant::now() + INITIAL_SEGMENT_POLL_INTERVAL)
                        .min(deadline)
                        .into(),
                )
                .await;
            }
        }
    }
}

pub(super) fn hls_working_dir(channel_number: &str) -> std::path::PathBuf {
    std::env::temp_dir()
        .join("ersatzrs-hls")
        .join(channel_number)
}

pub(super) fn preview_hls_working_dir(channel_number: &str) -> std::path::PathBuf {
    std::env::temp_dir()
        .join("ersatzrs-hls-preview")
        .join(channel_number)
}

pub(super) fn is_safe_channel_number_path_component(value: &str) -> bool {
    let mut parts = value.split('.');
    let Some(main) = parts.next() else {
        return false;
    };
    !main.is_empty()
        && main.chars().all(|character| character.is_ascii_digit())
        && parts.next().is_none_or(|subchannel| {
            !subchannel.is_empty()
                && subchannel
                    .chars()
                    .all(|character| character.is_ascii_digit())
                && parts.next().is_none()
        })
}

fn preview_hls_session_options(channel_number: &str) -> HlsSessionOptions {
    HlsSessionOptions {
        registry_key: channel_number.to_owned(),
        capacity_key: format!("preview:{channel_number}"),
        working_dir: preview_hls_working_dir(channel_number),
        playback_profile_override: Some(HlsPlaybackProfileOverride::BrowserCompatible),
    }
}

async fn ensure_preview_worker_running(
    state: &AppState,
    channel: &Channel,
    path_replacements: Vec<ersatzrs_core::path_replacement::PathReplacement>,
    deadline: std::time::Instant,
) {
    if let Some(worker) = state.preview_hls_sessions.try_get(&channel.number).await {
        worker.touch();
        return;
    }

    let Ok(timeout) =
        tokio::time::timeout_at(deadline.into(), load_segmenter_startup_timeout(state)).await
    else {
        return;
    };
    let idle_timeout = Some(timeout);
    if channel.streaming_engine == ersatzrs_domain::StreamingEngine::Next {
        let executable = match ersatzrs_infra::next_engine::host::channel_binary() {
            Ok(executable) => executable,
            Err(error) => {
                tracing::warn!(channel_id = channel.id, kind = ?error.kind(), "Next executable unavailable");
                return;
            }
        };
        let _task = state
            .next_engine
            .start(
                Arc::clone(&state.preview_hls_sessions),
                ersatzrs_infra::next_engine::host::NextStart {
                    channel_id: channel.id,
                    preview: true,
                    number: channel.number.clone(),
                    executable,
                    output: preview_hls_working_dir(&channel.number),
                    ffmpeg: ersatzrs_ffmpeg::ffmpeg_path().into(),
                    ffprobe: ersatzrs_ffmpeg::ffprobe_path().into(),
                    replacements: path_replacements,
                    idle_timeout,
                    deadline: deadline.into(),
                },
            )
            .await;
        return;
    }
    let worker = HlsSessionWorker::new_with_options(
        channel.number.clone(),
        channel.id,
        preview_hls_session_options(&channel.number),
        OutputFormat::Hls,
        path_replacements,
        Arc::clone(&state.ffmpeg_runtime),
    );
    let Ok(minimum_segments) =
        tokio::time::timeout_at(deadline.into(), load_initial_segment_count(state)).await
    else {
        return;
    };
    if worker
        .configure_media_relay(Arc::clone(&state.next_engine.media))
        .is_err()
    {
        return;
    }
    worker.initialize_startup(deadline, minimum_segments);
    if state.preview_hls_sessions.try_insert(worker.clone()).await {
        worker.start(
            state.pool.clone(),
            Arc::clone(&state.preview_hls_sessions),
            idle_timeout,
        );
    } else if let Some(existing) = state.preview_hls_sessions.try_get(&channel.number).await {
        existing.touch();
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
    let target_duration = hls_direct_target_duration(time_remaining);
    let mut playlist = format!(
        "#EXTM3U\n#EXT-X-VERSION:3\n#EXT-X-TARGETDURATION:{target_duration}\n#EXT-X-MEDIA-SEQUENCE:{index}\n#EXT-X-DISCONTINUITY\n"
    );
    let token = access_token.query_amp();
    let _ = std::fmt::Write::write_fmt(
        &mut playlist,
        format_args!(
            "#EXTINF:{time_remaining},\n{scheme}://{host}/iptv/hls-direct/{channel_number}{extension}?index={index}{token}\n"
        ),
    );
    for offset in 1..=HLS_DIRECT_LOOKAHEAD_SEGMENTS {
        let next_index = index + offset;
        let _ = std::fmt::Write::write_fmt(
            &mut playlist,
            format_args!(
                "#EXTINF:10.00,\n{scheme}://{host}/iptv/hls-direct/{channel_number}{extension}?index={next_index}{token}\n"
            ),
        );
    }
    playlist
}

fn hls_direct_target_duration(time_remaining: &str) -> u64 {
    time_remaining
        .parse::<f64>()
        .ok()
        .and_then(|value| Duration::try_from_secs_f64(value).ok())
        .map_or(10, |duration| {
            (duration.as_secs() + u64::from(duration.subsec_nanos() > 0)).max(10)
        })
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
///  * returns a segmenter-backed playlist for `hls-direct`,
///  * or redirects to the MPEG-TS variant for TS modes.
pub async fn serve_channel_m3u8(
    state: &AppState,
    channel_number: &str,
    headers: &HeaderMap,
    query: &HlsQuery,
) -> Response {
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    let channel = match startup_read(
        deadline,
        state.repositories.channel.get_by_number(channel_number),
    )
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
    if channel.streaming_engine == ersatzrs_domain::StreamingEngine::Next {
        return serve_next_channel_m3u8(
            state,
            channel_number,
            headers,
            &channel,
            &access_token,
            query,
            deadline,
        )
        .await;
    }
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
            serve_hls_segmenter_playlist(
                state,
                channel_number,
                headers,
                &channel,
                &access_token,
                output_format,
                DEFAULT_INITIAL_SEGMENT_COUNT,
                deadline,
            )
            .await
        }
        "hls-direct" => {
            serve_hls_segmenter_playlist(
                state,
                channel_number,
                headers,
                &channel,
                &access_token,
                OutputFormat::Hls,
                MIN_STABLE_INITIAL_SEGMENT_COUNT,
                deadline,
            )
            .await
        }
        "hls-direct-raw" => {
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

async fn serve_next_channel_m3u8(
    state: &AppState,
    channel_number: &str,
    headers: &HeaderMap,
    channel: &Channel,
    access_token: &AccessToken,
    query: &HlsQuery,
    deadline: std::time::Instant,
) -> Response {
    if let Err(response) =
        super::validate_profile_output(state, channel.id, OutputFormat::Hls, deadline.into()).await
    {
        return response;
    }
    if channel.streaming_mode == StreamingMode::TransportStreamHybrid
        && query
            .mode
            .as_deref()
            .is_none_or(|mode| mode.trim().is_empty() || mode.trim() == "mixed")
    {
        let target = format!("/iptv/channel/{channel_number}.ts{}", access_token.query());
        return axum::response::Redirect::temporary(&target).into_response();
    }
    return serve_hls_segmenter_playlist(
        state,
        channel_number,
        headers,
        channel,
        access_token,
        OutputFormat::Hls,
        MIN_STABLE_INITIAL_SEGMENT_COUNT,
        deadline,
    )
    .await;
}

// Keep admission and its single request deadline visible in one route owner.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
async fn serve_hls_segmenter_playlist(
    state: &AppState,
    channel_number: &str,
    headers: &HeaderMap,
    channel: &Channel,
    access_token: &AccessToken,
    output_format: OutputFormat,
    min_initial_segment_count: usize,
    deadline: std::time::Instant,
) -> Response {
    // ADR 0106: retain AV1 through native fMP4, never MPEG-TS segments.
    let output_format = if channel.streaming_engine == ersatzrs_domain::StreamingEngine::Legacy {
        match tokio::time::timeout_at(
            deadline.into(),
            ersatzrs_infra::playback::load_channel_playback_profile(&state.pool, channel.id),
        )
        .await
        {
            Ok(Ok(Some(profile))) if ersatzrs_ffmpeg::profile_encodes_av1(&profile.profile) => {
                OutputFormat::HlsMp4
            }
            Ok(Ok(Some(_))) => output_format,
            Ok(Ok(None) | Err(_)) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
            Err(_) => return StatusCode::SERVICE_UNAVAILABLE.into_response(),
        }
    } else {
        output_format
    };
    let scheme = request_scheme(headers);
    let host = request_host(headers);
    let spec = match startup_read(deadline, async {
        streaming_specs::load_spec(&state.pool, channel.id)
            .await
            .map_err(|_| {
                ersatzrs_core::repositories::RepositoryError::unavailable(
                    "HLS startup",
                    "streaming spec unavailable",
                )
            })
    })
    .await
    {
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

    let path_replacements = match startup_read(
        deadline,
        stream::load_path_replacements(&state.repositories.media_source),
    )
    .await
    {
        Ok(replacements) => replacements,
        Err(error) => {
            tracing::error!(
                %error,
                channel_id = channel.id,
                channel_number = %channel.number,
                "failed to load path replacements for hls worker"
            );
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let Ok((initial_segment_count, startup_timeout)) =
        tokio::time::timeout_at(deadline.into(), async {
            (
                load_initial_segment_count(state)
                    .await
                    .max(min_initial_segment_count),
                load_segmenter_startup_timeout(state).await,
            )
        })
        .await
    else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "HLS startup deadline expired",
        )
            .into_response();
    };
    match start_and_wait_for_initial_segments(
        &*state.hls_sessions,
        &channel.number,
        initial_segment_count,
        startup_timeout,
        deadline,
        || {
            ensure_segmenter_worker_running_before(
                state,
                &channel.number,
                channel.id,
                channel.idle_behavior,
                output_format,
                path_replacements.clone(),
                deadline,
            )
        },
    )
    .await
    {
        InitialSegmentWaitResult::Ready => {}
        InitialSegmentWaitResult::AtCapacity => {
            tracing::warn!(
                channel_number = %channel.number,
                initial_segment_count,
                startup_timeout_seconds = startup_timeout.as_secs(),
                "hls segmenter admission denied because all transcode slots are in use"
            );
            return hls_at_capacity_response();
        }
        InitialSegmentWaitResult::MissingSession | InitialSegmentWaitResult::Timeout => {
            tracing::warn!(
                channel_number = %channel.number,
                initial_segment_count,
                startup_timeout_seconds = startup_timeout.as_secs(),
                "hls segmenter did not produce initial playlist segments before timeout"
            );
            return (StatusCode::SERVICE_UNAVAILABLE, "no segments yet").into_response();
        }
    }

    let body = build_multi_variant_playlist(
        &scheme,
        &host,
        channel_number,
        spec.as_ref(),
        channel.streaming_engine,
        access_token,
    );
    hls_response(with_next_subtitles(body, channel, false, access_token))
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
    Query(query): Query<HlsQuery>,
) -> Response {
    if channel_number == TRANSCODE_TROUBLESHOOTING_CHANNEL {
        return serve_troubleshooting_hls_file("live.m3u8").await;
    }

    if let Some(handle) = state.hls_sessions.try_get(&channel_number).await {
        handle.touch();
        if !wait_for_retained_startup(&*handle).await {
            return (StatusCode::SERVICE_UNAVAILABLE, "HLS startup is not ready").into_response();
        }
        let admission_status = handle.admission_status();
        let access_token = AccessToken::new(query.access_token);
        return match handle.trim_playlist().await {
            Some(playlist) => {
                hls_response(propagate_playlist_access_token(&playlist, &access_token))
            }
            None if admission_status == HlsAdmissionStatus::AtCapacity => {
                let startup_timeout = load_segmenter_startup_timeout(&state).await;
                match wait_for_capacity_playlist(handle, startup_timeout).await {
                    Some(playlist) => {
                        hls_response(propagate_playlist_access_token(&playlist, &access_token))
                    }
                    None => hls_at_capacity_response(),
                }
            }
            None => (StatusCode::SERVICE_UNAVAILABLE, "no segments yet").into_response(),
        };
    }

    (StatusCode::SERVICE_UNAVAILABLE, "hls session not running").into_response()
}

/// `GET /iptv/preview/{number}/master.m3u8` — isolated browser-preview master.
pub async fn get_preview_master_playlist(
    State(state): State<AppState>,
    Path(channel_number): Path<String>,
    headers: HeaderMap,
    Query(query): Query<HlsQuery>,
) -> Response {
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    let channel = match startup_read(
        deadline,
        state.repositories.channel.get_by_number(&channel_number),
    )
    .await
    {
        Ok(channel) if channel.is_enabled => channel,
        Ok(_) | Err(_) => return StatusCode::NOT_FOUND.into_response(),
    };
    let access_token = AccessToken::new(query.access_token);
    let mut spec = match startup_read(deadline, async {
        streaming_specs::load_spec(&state.pool, channel.id)
            .await
            .map_err(|_| {
                ersatzrs_core::repositories::RepositoryError::unavailable(
                    "HLS startup",
                    "streaming spec unavailable",
                )
            })
    })
    .await
    {
        Ok(spec) => spec,
        Err(error) => {
            tracing::warn!(
                %error,
                channel_number = %channel.number,
                "failed to load browser-preview streaming spec"
            );
            None
        }
    };
    if let Some(spec) = &mut spec {
        spec.video_format = FFmpegProfileVideoFormat::H264;
        spec.audio_format = FFmpegProfileAudioFormat::Aac;
    }
    let path_replacements = match startup_read(
        deadline,
        stream::load_path_replacements(&state.repositories.media_source),
    )
    .await
    {
        Ok(replacements) => replacements,
        Err(error) => {
            tracing::error!(
                %error,
                channel_id = channel.id,
                channel_number = %channel.number,
                "failed to load path replacements for browser preview"
            );
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let Ok((initial_segment_count, startup_timeout)) =
        tokio::time::timeout_at(deadline.into(), async {
            (
                load_initial_segment_count(&state).await,
                load_segmenter_startup_timeout(&state).await,
            )
        })
        .await
    else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "HLS startup deadline expired",
        )
            .into_response();
    };
    match start_and_wait_for_initial_segments(
        &*state.preview_hls_sessions,
        &channel.number,
        initial_segment_count,
        startup_timeout,
        deadline,
        || ensure_preview_worker_running(&state, &channel, path_replacements.clone(), deadline),
    )
    .await
    {
        InitialSegmentWaitResult::Ready => {}
        InitialSegmentWaitResult::AtCapacity => return hls_at_capacity_response(),
        InitialSegmentWaitResult::MissingSession | InitialSegmentWaitResult::Timeout => {
            return (StatusCode::SERVICE_UNAVAILABLE, "no preview segments yet").into_response();
        }
    }

    hls_response(with_next_subtitles(
        build_preview_variant_playlist(
            &request_scheme(&headers),
            &request_host(&headers),
            &channel.number,
            spec.as_ref(),
            channel.streaming_engine,
            &access_token,
        ),
        &channel,
        true,
        &access_token,
    ))
}

/// `GET /iptv/preview/{number}/hls.m3u8` — isolated preview variant playlist.
pub async fn get_preview_session_playlist(
    State(state): State<AppState>,
    Path(channel_number): Path<String>,
    Query(query): Query<HlsQuery>,
) -> Response {
    if let Some(handle) = state.preview_hls_sessions.try_get(&channel_number).await {
        handle.touch();
        if !wait_for_retained_startup(&*handle).await {
            return (StatusCode::SERVICE_UNAVAILABLE, "HLS startup is not ready").into_response();
        }
        let admission_status = handle.admission_status();
        let access_token = AccessToken::new(query.access_token);
        return match handle.trim_playlist().await {
            Some(playlist) => {
                hls_response(propagate_playlist_access_token(&playlist, &access_token))
            }
            None if admission_status == HlsAdmissionStatus::AtCapacity => {
                let startup_timeout = load_segmenter_startup_timeout(&state).await;
                match wait_for_capacity_playlist(handle, startup_timeout).await {
                    Some(playlist) => {
                        hls_response(propagate_playlist_access_token(&playlist, &access_token))
                    }
                    None => hls_at_capacity_response(),
                }
            }
            None => (StatusCode::SERVICE_UNAVAILABLE, "no preview segments yet").into_response(),
        };
    }
    (
        StatusCode::SERVICE_UNAVAILABLE,
        "preview session not running",
    )
        .into_response()
}

/// `GET /iptv/preview/{number}/{segment}` — isolated preview segment.
pub async fn get_preview_session_segment(
    State(state): State<AppState>,
    Path((channel_number, filename)): Path<(String, String)>,
) -> Response {
    if !is_safe_channel_number_path_component(&channel_number) {
        return (StatusCode::BAD_REQUEST, "invalid channel number").into_response();
    }
    if filename.contains("..") || filename.contains('/') || filename.contains('\\') {
        return (StatusCode::BAD_REQUEST, "invalid segment filename").into_response();
    }
    touch_session_segment(&*state.preview_hls_sessions, &channel_number).await;
    serve_hls_segment_file(
        preview_hls_working_dir(&channel_number).join(&filename),
        &channel_number,
        &filename,
    )
    .await
}

pub(super) fn propagate_playlist_access_token(
    playlist: &str,
    access_token: &AccessToken,
) -> String {
    let query = access_token.query();
    if query.is_empty() {
        return playlist.to_owned();
    }
    let query_amp = access_token.query_amp();

    let mut result = String::with_capacity(playlist.len() + 128);
    for line in playlist.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            result.push_str(line);
        } else {
            result.push_str(line);
            result.push_str(if line.contains('?') {
                &query_amp
            } else {
                &query
            });
        }
        result.push('\n');
    }
    result
}

pub async fn get_session_segment(
    State(state): State<AppState>,
    Path((channel_number, filename)): Path<(String, String)>,
) -> Response {
    if channel_number != TRANSCODE_TROUBLESHOOTING_CHANNEL
        && !is_safe_channel_number_path_component(&channel_number)
    {
        return (StatusCode::BAD_REQUEST, "invalid channel number").into_response();
    }
    if filename.contains("..") || filename.contains('/') || filename.contains('\\') {
        return (StatusCode::BAD_REQUEST, "invalid segment filename").into_response();
    }

    if channel_number == TRANSCODE_TROUBLESHOOTING_CHANNEL {
        return serve_troubleshooting_hls_file(&filename).await;
    }

    touch_session_segment(&*state.hls_sessions, &channel_number).await;

    let path = std::env::temp_dir()
        .join("ersatzrs-hls")
        .join(&channel_number)
        .join(&filename);

    serve_hls_segment_file(path, &channel_number, &filename).await
}

async fn serve_hls_segment_file(
    path: std::path::PathBuf,
    channel_number: &str,
    filename: &str,
) -> Response {
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
                Some("vtt") => {
                    response.headers_mut().insert(
                        header::CONTENT_TYPE,
                        HeaderValue::from_static("text/vtt; charset=utf-8"),
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

async fn touch_session_segment(registry: &dyn HlsSessionRegistry, channel_number: &str) {
    if let Some(handle) = registry.try_get(channel_number).await {
        handle.touch();
    }
}

async fn wait_for_capacity_playlist(
    handle: Arc<dyn HlsSessionHandle>,
    startup_timeout: Duration,
) -> Option<String> {
    let deadline = tokio::time::Instant::now() + startup_timeout;
    while tokio::time::Instant::now() < deadline {
        handle.touch();
        if let Some(playlist) = tokio::time::timeout_at(deadline, handle.trim_playlist())
            .await
            .ok()?
        {
            return Some(playlist);
        }
        if handle.admission_status() != HlsAdmissionStatus::AtCapacity {
            return None;
        }
        tokio::time::sleep_until(
            (tokio::time::Instant::now() + INITIAL_SEGMENT_POLL_INTERVAL).min(deadline),
        )
        .await;
    }
    None
}

async fn serve_troubleshooting_hls_file(filename: &str) -> Response {
    if filename.contains("..") || filename.contains('/') || filename.contains('\\') {
        return (StatusCode::BAD_REQUEST, "invalid troubleshooting filename").into_response();
    }

    match tokio::fs::read(transcode_troubleshooting_folder().join(filename)).await {
        Ok(body) => hls_file_response(filename, body),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            StatusCode::NOT_FOUND.into_response()
        }
        Err(error) => {
            tracing::warn!(%error, filename, "failed to read troubleshooting hls file");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

pub(crate) fn hls_file_response(filename: &str, body: Vec<u8>) -> Response {
    let mut response = body.into_response();
    match std::path::Path::new(filename)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("m3u8") => {
            response
                .headers_mut()
                .insert(header::CONTENT_TYPE, HLS_CONTENT_TYPE);
        }
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
    use async_trait::async_trait;
    use ersatzrs_core::hls::HlsSessionHandle;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Mutex;

    struct FakeHandle {
        channel: String,
        touched: AtomicUsize,
        admission_status: HlsAdmissionStatus,
        playlist: Mutex<Option<String>>,
        readiness: Mutex<HlsStartupReadiness>,
        cancelled: AtomicUsize,
        stall_playlist: AtomicBool,
        playlist_reads: AtomicUsize,
        playlist_gate: Mutex<Option<(Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>)>>,
    }

    impl FakeHandle {
        fn new(channel: &str) -> Arc<Self> {
            Self::with_admission_status(channel, HlsAdmissionStatus::Waiting)
        }

        fn with_admission_status(channel: &str, admission_status: HlsAdmissionStatus) -> Arc<Self> {
            Arc::new(Self {
                channel: channel.to_string(),
                touched: AtomicUsize::new(0),
                admission_status,
                playlist: Mutex::new(None),
                readiness: Mutex::new(HlsStartupReadiness::Legacy),
                cancelled: AtomicUsize::new(0),
                stall_playlist: AtomicBool::new(false),
                playlist_reads: AtomicUsize::new(0),
                playlist_gate: Mutex::new(None),
            })
        }

        fn set_playlist(&self, playlist: &str) {
            *self
                .playlist
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(playlist.to_owned());
        }
    }

    #[async_trait]
    impl HlsSessionHandle for FakeHandle {
        fn startup_readiness(&self) -> HlsStartupReadiness {
            *self.readiness.lock().unwrap()
        }
        fn channel_number(&self) -> &str {
            &self.channel
        }

        fn touch(&self) {
            self.touched.fetch_add(1, Ordering::SeqCst);
        }

        fn admission_status(&self) -> HlsAdmissionStatus {
            self.admission_status
        }

        async fn trim_playlist(&self) -> Option<String> {
            self.playlist_reads.fetch_add(1, Ordering::SeqCst);
            let gate = self.playlist_gate.lock().unwrap().clone();
            if let Some((entered, release)) = gate {
                entered.notify_one();
                release.notified().await;
            }
            if self.stall_playlist.load(Ordering::SeqCst) {
                std::future::pending::<()>().await;
            }
            self.playlist
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone()
        }

        async fn cancel(&self) {
            self.cancelled.fetch_add(1, Ordering::SeqCst);
        }

        fn playout_updated(&self) {}
    }

    #[tokio::test]
    async fn retained_variant_waiter_never_bypasses_gate_or_cancels_shared_worker() {
        let handle = FakeHandle::new("42");
        handle.set_playlist("#EXTM3U\n#EXTINF:4,\na.ts\n#EXTINF:4,\nb.ts\n#EXTINF:4,\nc.ts\n");
        *handle.readiness.lock().unwrap() = HlsStartupReadiness::Pending {
            deadline: std::time::Instant::now(),
        };
        assert!(!wait_for_retained_startup(&*handle).await);
        assert_eq!(handle.cancelled.load(Ordering::SeqCst), 0);
        *handle.readiness.lock().unwrap() = HlsStartupReadiness::Ready;
        assert!(wait_for_retained_startup(&*handle).await);
        *handle.readiness.lock().unwrap() = HlsStartupReadiness::Legacy;
        assert!(wait_for_retained_startup(&*handle).await);
    }

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

    #[test]
    fn hls_working_dir_uses_channel_scoped_temp_folder() {
        assert_eq!(
            hls_working_dir("1"),
            std::env::temp_dir().join("ersatzrs-hls").join("1")
        );
    }

    #[test]
    fn channel_session_paths_accept_numbers_and_reject_traversal_components() {
        for valid in ["1", "001", "7.2", "0.540000"] {
            assert!(is_safe_channel_number_path_component(valid), "{valid}");
        }
        for invalid in [
            "",
            ".",
            "..",
            "/etc",
            "/etc/passwd",
            r"..\etc",
            "1/../../etc",
            "1.2.3",
            "channel",
        ] {
            assert!(!is_safe_channel_number_path_component(invalid), "{invalid}");
        }
    }

    #[test]
    fn preview_session_uses_distinct_capacity_identity() {
        let standard = HlsSessionOptions::standard("42", hls_working_dir("42"));
        let preview = preview_hls_session_options("42");

        assert_eq!(standard.registry_key, preview.registry_key);
        assert_eq!(standard.capacity_key, "42");
        assert_eq!(preview.capacity_key, "preview:42");
        assert_ne!(standard.working_dir, preview.working_dir);
    }

    #[test]
    fn troubleshooting_channel_is_reserved_session_number() {
        assert_eq!(TRANSCODE_TROUBLESHOOTING_CHANNEL, ".troubleshooting");
    }

    #[test]
    fn parse_positive_usize_rejects_empty_zero_and_negative_values() {
        assert_eq!(parse_positive_usize(" 3 "), Some(3));
        assert_eq!(parse_positive_usize("0"), None);
        assert_eq!(parse_positive_usize("-1"), None);
        assert_eq!(parse_positive_usize(""), None);
        assert_eq!(parse_positive_usize("abc"), None);
    }

    #[test]
    fn initial_segment_count_is_clamped_to_stable_floor() {
        assert_eq!(normalize_initial_segment_count(1), 3);
        assert_eq!(normalize_initial_segment_count(2), 3);
        assert_eq!(normalize_initial_segment_count(4), 4);
    }

    #[test]
    fn segmenter_idle_timeout_respects_keep_running_channels() {
        assert_eq!(
            segmenter_idle_timeout_for(ChannelIdleBehavior::StopOnDisconnect, 45),
            Some(Duration::from_secs(45))
        );
        assert_eq!(
            segmenter_idle_timeout_for(ChannelIdleBehavior::KeepRunning, 45),
            None
        );
    }

    #[tokio::test]
    async fn session_segment_access_touches_registered_worker() {
        let registry = ersatzrs_infra::hls::InMemoryHlsSessionRegistry::new();
        let handle = FakeHandle::new("12");
        registry.insert(handle.clone()).await;

        touch_session_segment(&registry, "12").await;

        assert_eq!(handle.touched.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn startup_http_disconnect_drops_wait_without_cancelling_shared_worker() {
        use tokio::io::AsyncWriteExt;

        struct Dropped(Arc<tokio::sync::Notify>);
        impl Drop for Dropped {
            fn drop(&mut self) {
                self.0.notify_one();
            }
        }

        let registry = Arc::new(ersatzrs_infra::hls::InMemoryHlsSessionRegistry::new());
        let handle = FakeHandle::new("12");
        handle.stall_playlist.store(true, Ordering::SeqCst);
        registry.insert(handle.clone()).await;
        let entered = Arc::new(tokio::sync::Notify::new());
        let dropped = Arc::new(tokio::sync::Notify::new());
        let route = {
            let registry = registry.clone();
            let entered = entered.clone();
            let dropped = dropped.clone();
            axum::routing::get(move || {
                let registry = registry.clone();
                let entered = entered.clone();
                let dropped = dropped.clone();
                async move {
                    let _guard = Dropped(dropped);
                    entered.notify_one();
                    wait_for_initial_segments(&*registry, "12", 1, Duration::from_secs(30)).await;
                    StatusCode::SERVICE_UNAVAILABLE
                }
            })
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, axum::Router::new().route("/startup", route))
                .await
                .unwrap();
        });
        let mut client = tokio::net::TcpStream::connect(address).await.unwrap();
        client
            .write_all(b"GET /startup HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(2), entered.notified())
            .await
            .unwrap();
        drop(client);
        let result = tokio::time::timeout(Duration::from_secs(2), dropped.notified()).await;
        server.abort();
        let _ = server.await;
        result.expect("disconnect must drop the pending startup wait before its deadline");
        assert_eq!(handle.cancelled.load(Ordering::SeqCst), 0);
        let expected: Arc<dyn HlsSessionHandle> = handle;
        assert!(Arc::ptr_eq(
            &registry.try_get("12").await.unwrap(),
            &expected
        ));
    }

    #[tokio::test]
    async fn initial_segment_wait_handles_ready_failed_and_missing_workers() {
        let registry = ersatzrs_infra::hls::InMemoryHlsSessionRegistry::new();
        assert_eq!(
            wait_for_initial_segments(&registry, "12", 1, Duration::from_secs(1)).await,
            InitialSegmentWaitResult::MissingSession
        );
        let handle = FakeHandle::new("12");
        handle.set_playlist("#EXTM3U\n#EXTINF:4,\na.ts\n");
        registry.insert(handle.clone()).await;
        assert_eq!(
            wait_for_initial_segments(&registry, "12", 1, Duration::from_secs(1)).await,
            InitialSegmentWaitResult::Ready
        );
        *handle.readiness.lock().unwrap() = HlsStartupReadiness::Failed;
        assert_eq!(
            wait_for_initial_segments(&registry, "12", 1, Duration::from_secs(1)).await,
            InitialSegmentWaitResult::Timeout
        );
        assert_eq!(handle.cancelled.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn initial_segment_wait_rejects_removed_worker_playlist() {
        let registry = ersatzrs_infra::hls::InMemoryHlsSessionRegistry::new();
        let handle = FakeHandle::new("12");
        handle.set_playlist("#EXTM3U\n#EXTINF:4,\na.ts\n");
        let entered = Arc::new(tokio::sync::Notify::new());
        let release = Arc::new(tokio::sync::Notify::new());
        *handle.playlist_gate.lock().unwrap() = Some((entered.clone(), release.clone()));
        registry.insert(handle.clone()).await;
        let wait = wait_for_initial_segments(&registry, "12", 1, Duration::from_secs(1));
        let removal = async {
            entered.notified().await;
            let identity: Arc<dyn HlsSessionHandle> = handle.clone();
            registry.remove_if_current(&identity).await;
            assert!(registry.try_get("12").await.is_none());
            release.notify_one();
        };
        let (result, ()) = tokio::join!(wait, removal);
        assert_eq!(result, InitialSegmentWaitResult::MissingSession);
        assert_eq!(handle.cancelled.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn initial_segment_readiness_is_cached_only_for_current_worker() {
        let registry = ersatzrs_infra::hls::InMemoryHlsSessionRegistry::new();
        let handle = FakeHandle::new("12");
        handle.set_playlist("#EXTM3U\n#EXTINF:4,\na.ts\n");
        registry.insert(handle.clone()).await;
        assert_eq!(
            wait_for_initial_segments(&registry, "12", 1, Duration::from_secs(1)).await,
            InitialSegmentWaitResult::Ready
        );
        handle.stall_playlist.store(true, Ordering::SeqCst);
        assert_eq!(
            wait_for_initial_segments(&registry, "12", 1, Duration::from_millis(20)).await,
            InitialSegmentWaitResult::Ready
        );
        let replacement = FakeHandle::new("12");
        replacement.set_playlist("#EXTM3U\n");
        registry.insert(replacement.clone()).await;
        let previous: Arc<dyn HlsSessionHandle> = handle;
        assert_eq!(registry.initial_segments_ready(&previous).await, None);
        assert!(!registry.mark_initial_segments_ready(&previous).await);
        assert_eq!(
            wait_for_initial_segments(&registry, "12", 0, Duration::from_millis(20)).await,
            InitialSegmentWaitResult::Timeout
        );
        replacement.set_playlist("#EXTM3U\n#EXTINF:4,\nb.ts\n");
        assert_eq!(
            wait_for_initial_segments(&registry, "12", 0, Duration::from_secs(1)).await,
            InitialSegmentWaitResult::Ready
        );
    }

    #[tokio::test]
    async fn startup_recovery_replaces_ended_existing_worker_only_once() {
        for replacement_ends in [false, true] {
            let registry = ersatzrs_infra::hls::InMemoryHlsSessionRegistry::new();
            let old = FakeHandle::new("12");
            let entered = Arc::new(tokio::sync::Notify::new());
            *old.playlist_gate.lock().unwrap() =
                Some((entered.clone(), Arc::new(tokio::sync::Notify::new())));
            registry.insert(old.clone()).await;
            let replacement = FakeHandle::new("12");
            let replacement_entered = Arc::new(tokio::sync::Notify::new());
            if replacement_ends {
                *replacement.playlist_gate.lock().unwrap() = Some((
                    replacement_entered.clone(),
                    Arc::new(tokio::sync::Notify::new()),
                ));
            } else {
                replacement.set_playlist("#EXTM3U\n#EXTINF:4,\nb.ts\n");
            }
            let created = AtomicUsize::new(0);
            let start = || async {
                if registry.try_get("12").await.is_none() {
                    assert!(registry.try_insert(replacement.clone()).await);
                    created.fetch_add(1, Ordering::SeqCst);
                }
            };
            let wait = start_and_wait_for_initial_segments(
                &registry,
                "12",
                1,
                Duration::from_secs(3),
                std::time::Instant::now() + Duration::from_secs(3),
                start,
            );
            let remove = async {
                entered.notified().await;
                let identity: Arc<dyn HlsSessionHandle> = old.clone();
                registry.remove_if_current(&identity).await;
                if replacement_ends {
                    replacement_entered.notified().await;
                    let identity: Arc<dyn HlsSessionHandle> = replacement.clone();
                    registry.remove_if_current(&identity).await;
                }
            };
            let (result, ()) =
                tokio::time::timeout(Duration::from_secs(2), async { tokio::join!(wait, remove) })
                    .await
                    .expect("removal must interrupt pending playlist I/O");
            assert_eq!(
                result,
                if replacement_ends {
                    InitialSegmentWaitResult::MissingSession
                } else {
                    InitialSegmentWaitResult::Ready
                }
            );
            assert_eq!(created.load(Ordering::SeqCst), 1);
            assert_eq!(old.cancelled.load(Ordering::SeqCst), 0);
            assert_eq!(replacement.cancelled.load(Ordering::SeqCst), 0);
        }
    }

    #[tokio::test]
    async fn simultaneous_startup_waiters_share_worker_and_cancel_independently() {
        for cancel_first in [false, true] {
            let registry = ersatzrs_infra::hls::InMemoryHlsSessionRegistry::new();
            let handle = FakeHandle::new("12");
            let release = Arc::new(tokio::sync::Notify::new());
            *handle.playlist_gate.lock().unwrap() =
                Some((Arc::new(tokio::sync::Notify::new()), release.clone()));
            let created = AtomicUsize::new(0);
            let start = || async {
                if registry.try_insert(handle.clone()).await {
                    created.fetch_add(1, Ordering::SeqCst);
                }
            };
            let deadline = std::time::Instant::now() + Duration::from_secs(3);
            let (cancel, cancelled) = tokio::sync::oneshot::channel();
            let mut cancel = Some(cancel);
            let first = async {
                tokio::select! {
                    biased;
                    _ = cancelled => None,
                    result = start_and_wait_for_initial_segments(&registry, "12", 1, Duration::from_secs(3), deadline, start) => Some(result),
                }
            };
            let second = start_and_wait_for_initial_segments(
                &registry,
                "12",
                1,
                Duration::from_secs(3),
                deadline,
                start,
            );
            let producer = async {
                while handle.playlist_reads.load(Ordering::SeqCst) < 2 {
                    tokio::task::yield_now().await;
                }
                if cancel_first {
                    cancel.take().unwrap().send(()).unwrap();
                }
                handle.set_playlist("#EXTM3U\n#EXTINF:4,\na.ts\n");
                release.notify_waiters();
            };
            let (first, second, ()) = tokio::time::timeout(Duration::from_secs(2), async {
                tokio::join!(first, second, producer)
            })
            .await
            .expect("viewers must reach readiness concurrently");
            assert_eq!(
                first,
                if cancel_first {
                    None
                } else {
                    Some(InitialSegmentWaitResult::Ready)
                }
            );
            assert_eq!(second, InitialSegmentWaitResult::Ready);
            assert_eq!(created.load(Ordering::SeqCst), 1);
            assert_eq!(handle.cancelled.load(Ordering::SeqCst), 0);
        }
    }

    #[tokio::test]
    async fn startup_recovery_uses_another_viewers_replacement() {
        let registry = ersatzrs_infra::hls::InMemoryHlsSessionRegistry::new();
        let old = FakeHandle::new("12");
        let entered = Arc::new(tokio::sync::Notify::new());
        *old.playlist_gate.lock().unwrap() =
            Some((entered.clone(), Arc::new(tokio::sync::Notify::new())));
        registry.insert(old).await;
        let replacement = FakeHandle::new("12");
        replacement.set_playlist("#EXTM3U\n#EXTINF:4,\nb.ts\n");
        let start = || async {
            assert!(
                registry.try_get("12").await.is_some(),
                "must reuse registered replacement"
            );
        };
        let wait = start_and_wait_for_initial_segments(
            &registry,
            "12",
            1,
            Duration::from_secs(3),
            std::time::Instant::now() + Duration::from_secs(3),
            start,
        );
        let replace = async {
            entered.notified().await;
            registry.insert(replacement.clone()).await;
        };
        let (result, ()) = tokio::time::timeout(Duration::from_secs(2), async {
            tokio::join!(wait, replace)
        })
        .await
        .unwrap();
        assert_eq!(result, InitialSegmentWaitResult::Ready);
        let identity: Arc<dyn HlsSessionHandle> = replacement;
        assert!(Arc::ptr_eq(
            &registry.try_get("12").await.unwrap(),
            &identity
        ));
    }

    #[tokio::test]
    async fn startup_timeout_does_not_restart_existing_worker() {
        let registry = ersatzrs_infra::hls::InMemoryHlsSessionRegistry::new();
        let handle = FakeHandle::new("12");
        handle.stall_playlist.store(true, Ordering::SeqCst);
        registry.insert(handle.clone()).await;
        let attempts = AtomicUsize::new(0);
        let result = start_and_wait_for_initial_segments(
            &registry,
            "12",
            1,
            Duration::from_millis(20),
            std::time::Instant::now() + Duration::from_secs(1),
            || async {
                attempts.fetch_add(1, Ordering::SeqCst);
            },
        )
        .await;
        assert_eq!(result, InitialSegmentWaitResult::Timeout);
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
        let identity: Arc<dyn HlsSessionHandle> = handle.clone();
        assert!(Arc::ptr_eq(
            &registry.try_get("12").await.unwrap(),
            &identity
        ));
        assert_eq!(handle.cancelled.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn initial_segment_wait_bounds_stalled_playlist_reads() {
        let registry = ersatzrs_infra::hls::InMemoryHlsSessionRegistry::new();
        for admission in [HlsAdmissionStatus::Waiting, HlsAdmissionStatus::AtCapacity] {
            let handle = FakeHandle::with_admission_status("12", admission);
            handle.stall_playlist.store(true, Ordering::SeqCst);
            registry.insert(handle.clone()).await;
            let result = tokio::time::timeout(
                Duration::from_secs(1),
                wait_for_initial_segments(&registry, "12", 1, Duration::from_millis(20)),
            )
            .await
            .expect("startup deadline must bound playlist I/O");
            assert!(tokio::time::timeout(
                Duration::from_secs(1),
                wait_for_capacity_playlist(handle.clone(), Duration::from_millis(20)),
            )
            .await
            .expect("capacity deadline must bound playlist I/O")
            .is_none());
            assert_eq!(
                result,
                if admission == HlsAdmissionStatus::AtCapacity {
                    InitialSegmentWaitResult::AtCapacity
                } else {
                    InitialSegmentWaitResult::Timeout
                }
            );
            assert_eq!(handle.cancelled.load(Ordering::SeqCst), 0);
        }
    }

    #[tokio::test]
    async fn initial_segment_wait_gives_capacity_slate_time_to_publish() {
        let registry = ersatzrs_infra::hls::InMemoryHlsSessionRegistry::new();
        registry
            .insert(FakeHandle::with_admission_status(
                "12",
                HlsAdmissionStatus::AtCapacity,
            ))
            .await;

        let result = wait_for_initial_segments(&registry, "12", 1, Duration::from_millis(1)).await;

        assert_eq!(result, InitialSegmentWaitResult::AtCapacity);
    }

    #[tokio::test]
    async fn capacity_session_playlist_wait_returns_generated_slate() {
        let handle = FakeHandle::with_admission_status("12", HlsAdmissionStatus::AtCapacity);
        let updater = handle.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(20)).await;
            updater.set_playlist("#EXTM3U\n#EXTINF:4.000,\nslate.ts\n");
        });

        let playlist = wait_for_capacity_playlist(handle, Duration::from_secs(1))
            .await
            .expect("capacity slate playlist");

        assert!(playlist.contains("slate.ts"));
    }

    #[test]
    fn count_playlist_segments_counts_extinf_lines_only() {
        let playlist = "#EXTM3U\n#EXT-X-TARGETDURATION:4\n#EXTINF:4.000,\nsegment-1.ts\n#EXT-X-DISCONTINUITY\n  #EXTINF:3.500,\nsegment-2.ts\n";

        assert_eq!(count_playlist_segments(playlist), 2);
    }

    #[test]
    fn session_playlist_propagates_access_token_to_segments() {
        let playlist =
            "#EXTM3U\n#EXTINF:4.000,\nsegment-1.ts\n#EXTINF:4.000,\nsegment-2.ts?part=1\n";
        let propagated =
            propagate_playlist_access_token(playlist, &AccessToken::new(Some("preview-token")));

        assert!(propagated.contains("segment-1.ts?access_token=preview-token"));
        assert!(propagated.contains("segment-2.ts?part=1&access_token=preview-token"));
        assert!(!propagated.contains("#EXTM3U?"));
    }

    #[tokio::test]
    async fn converted_subtitle_segments_use_webvtt_content_type_and_no_store() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("live000001.vtt");
        tokio::fs::write(&path, "WEBVTT\n\n00:00.000 --> 00:01.000\nSynthetic cue\n")
            .await
            .unwrap();
        let response = serve_hls_segment_file(path, "1", "live000001.vtt").await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers()[header::CONTENT_TYPE],
            "text/vtt; charset=utf-8"
        );
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        let playlist = propagate_playlist_access_token(
            "#EXTM3U\n#EXTINF:1.0,\nlive000001.vtt\n",
            &AccessToken::new(Some("synthetic-token")),
        );
        assert!(playlist.contains("live000001.vtt?access_token=synthetic-token"));
    }
    async fn assert_next_hybrid_routes(state: &AppState) {
        sqlx::query(r#"UPDATE "Channel" SET "StreamingMode"=5 WHERE "Id"=1"#)
            .execute(&state.pool)
            .await
            .unwrap();
        for mode in [None, Some("mixed".into())] {
            let query = HlsQuery {
                mode,
                access_token: Some("synthetic-token".into()),
            };
            let response = serve_channel_m3u8(state, "1", &HeaderMap::new(), &query).await;
            assert_eq!(response.status(), StatusCode::TEMPORARY_REDIRECT);
            assert_eq!(
                response.headers()[header::LOCATION],
                "/iptv/channel/1.ts?access_token=synthetic-token"
            );
        }
        let explicit = HlsQuery {
            mode: Some("segmenter".into()),
            access_token: Some("synthetic-token".into()),
        };
        assert_eq!(
            serve_channel_m3u8(state, "1", &HeaderMap::new(), &explicit)
                .await
                .status(),
            StatusCode::OK
        );
        let legacy = HlsQuery {
            mode: Some("ts-legacy".into()),
            ..Default::default()
        };
        assert_eq!(
            stream::serve_transport_stream(state, "1", &HeaderMap::new(), &legacy)
                .await
                .status(),
            StatusCode::NOT_FOUND
        );
        sqlx::query(r#"UPDATE "Channel" SET "IsEnabled"=0 WHERE "Id"=1"#)
            .execute(&state.pool)
            .await
            .unwrap();
        assert_eq!(
            stream::serve_transport_stream(state, "1", &HeaderMap::new(), &HlsQuery::default())
                .await
                .status(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            serve_channel_m3u8(state, "1", &HeaderMap::new(), &explicit)
                .await
                .status(),
            StatusCode::NOT_FOUND
        );
        sqlx::query(r#"UPDATE "Channel" SET "IsEnabled"=1 WHERE "Id"=1"#)
            .execute(&state.pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn next_engine_routes_preserve_hybrid_hls_tokens_and_reject_native_playback() {
        use ersatzrs_core::repositories::FfmpegProfileRepository;
        use ersatzrs_domain::{FFmpegProfile, StreamingEngine};
        let state = AppState::for_tests(
            crate::host::state::IptvAccessTokenVerifier::from_optional_secret(None),
        );
        sqlx::migrate!("../../migrations")
            .run(&state.pool)
            .await
            .unwrap();
        sqlx::query(r#"INSERT INTO "Resolution" ("Id", "Name", "Width", "Height") VALUES (1, 'Synthetic', 320, 180)"#).execute(&state.pool).await.unwrap();
        state
            .repositories
            .ffmpeg_profile
            .upsert(&FFmpegProfile {
                id: 1,
                resolution_id: 1,
                video_format: FFmpegProfileVideoFormat::H264,
                audio_format: FFmpegProfileAudioFormat::Aac,
                ..Default::default()
            })
            .await
            .unwrap();
        state
            .repositories
            .channel
            .upsert(&Channel {
                id: 1,
                number: "1".into(),
                name: "Synthetic".into(),
                ffmpeg_profile_id: 1,
                streaming_engine: StreamingEngine::Next,
                next_engine_text_subtitle_mode:
                    ersatzrs_domain::NextEngineTextSubtitleMode::Convert,
                streaming_mode: StreamingMode::HttpLiveStreamingSegmenter,
                is_enabled: true,
                ..Default::default()
            })
            .await
            .unwrap();
        let worker = FakeHandle::new("1");
        worker.set_playlist("#EXTM3U\n#EXTINF:4.0,\nnext-segment.ts\n");
        *worker.readiness.lock().unwrap() = HlsStartupReadiness::Ready;
        state.hls_sessions.try_insert(worker.clone()).await;
        let query = HlsQuery {
            mode: Some("ts".into()),
            access_token: Some("synthetic-token".into()),
        };
        let master = serve_channel_m3u8(&state, "1", &HeaderMap::new(), &query).await;
        assert_eq!(master.status(), StatusCode::OK);
        let body = axum::body::to_bytes(master.into_body(), 64 * 1024)
            .await
            .unwrap();
        let text = std::str::from_utf8(&body).unwrap();
        assert!(text.contains("/iptv/session/1/hls.m3u8?access_token=synthetic-token"));
        assert!(text.contains("/iptv/session/1/live_sub.m3u8?access_token=synthetic-token"));
        assert!(text.contains("SUBTITLES=\"subs\""));
        let variant =
            get_session_playlist(State(state.clone()), Path("1".into()), Query(query)).await;
        assert_eq!(variant.status(), StatusCode::OK);
        let body = axum::body::to_bytes(variant.into_body(), 64 * 1024)
            .await
            .unwrap();
        assert!(std::str::from_utf8(&body)
            .unwrap()
            .contains("next-segment.ts?access_token=synthetic-token"));
        let redirect = stream::serve_transport_stream(
            &state,
            "1",
            &HeaderMap::new(),
            &HlsQuery {
                access_token: Some("synthetic-token".into()),
                ..Default::default()
            },
        )
        .await;
        assert_eq!(redirect.status(), StatusCode::TEMPORARY_REDIRECT);
        assert_eq!(
            redirect.headers()[header::LOCATION],
            "/iptv/channel/1.m3u8?access_token=synthetic-token"
        );
        assert_next_hybrid_routes(&state).await;
        assert_eq!(
            stream::serve_channel_stream(&state, "1", OutputFormat::MpegTs)
                .await
                .status(),
            StatusCode::NOT_FOUND
        );
        assert!(worker.touched.load(Ordering::SeqCst) >= 2);
    }
}
