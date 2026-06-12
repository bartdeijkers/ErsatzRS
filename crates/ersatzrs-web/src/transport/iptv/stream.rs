use std::{
    path::PathBuf,
    process::Stdio,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};

use axum::{
    body::Body,
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use chrono::{DateTime, NaiveDateTime, Utc};
use ersatzrs_core::hls::{HlsSessionHandle, HlsSessionRegistry};
use ersatzrs_core::path_replacement::{replace_path, PathPlatform, PathReplacement};
use ersatzrs_core::repositories::{ChannelRepository, ConfigRepository, MediaSourceRepository};
use ersatzrs_domain::{
    Channel, ChannelSubtitleMode, EmbyMediaSource, FFmpegProfileAudioFormat, JellyfinMediaSource,
    PlexMediaSource, StreamingMode,
};
use ersatzrs_ffmpeg::{
    build_mpegts_concat_command, build_mpegts_segmenter_wrapper_command,
    build_pipeline_for_acceleration, ConcatPlaylist, DesiredVideoState, FfmpegCapabilityReport,
    FfmpegCommand, FrameRate, FrameSize, GraphicsPipelineOverlay, InputFile, OutputFormat,
    OutputMetadata, PipelineRequest, SourceAudioStream, SourceVideoStream,
};
use ersatzrs_infra::db::DbPool;
use ersatzrs_infra::file_system_layout;
use ersatzrs_infra::playback::{
    enrich_source_video_stream_for_playback, load_channel_playback_profile,
    load_channel_target_frame_rate, load_source_audio_streams, load_source_video_stream,
    render_playback_graphics_overlays, select_playback_graphics, select_playback_subtitle,
    select_playback_watermarks, subtitle_fonts_dir, subtitle_input_for_stream,
    watermark_inputs_for_stream, PlaybackGraphicsRenderContext, PlaybackProfile, PlaybackSubtitle,
};
use ersatzrs_infra::subtitles::rebase_burned_text_subtitle_input;
use serde::Deserialize;
use sqlx::Row;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::sync::Notify;
use tokio_util::io::ReaderStream;

use crate::host::state::AppState;
use crate::transport::iptv::access_token::AccessToken;
use crate::transport::iptv::{hls, hls::HlsQuery};

static NEXT_GRAPHICS_REQUEST_ID: AtomicU64 = AtomicU64::new(1);
const FFMPEG_STDERR_TAIL_BYTES: usize = 16 * 1024;
const DEFAULT_MPEG_TS_SCRIPT_ID: &str = "default";

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct MpegTsScriptDefinition {
    name: String,
    #[serde(default)]
    linux_script: Option<String>,
    #[serde(default)]
    windows_script: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MpegTsScript {
    id: String,
    folder: PathBuf,
    definition: MpegTsScriptDefinition,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MpegTsScriptRenderData {
    hls_url: String,
    channel_name: String,
    ffmpeg_path: String,
}

pub async fn serve_transport_stream(
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
            tracing::debug!(%error, channel_number, "channel not found for transport stream");
            return StatusCode::NOT_FOUND.into_response();
        }
    };

    match resolve_transport_stream_mode(query.mode.as_deref(), channel.streaming_mode) {
        TransportStreamMode::RedirectToHls => {
            let access_token = AccessToken::new(query.access_token.clone());
            let target = format!(
                "/iptv/channel/{channel_number}.m3u8{}",
                access_token.query()
            );
            axum::response::Redirect::temporary(&target).into_response()
        }
        TransportStreamMode::Legacy => serve_legacy_concat_stream(state, &channel, headers),
        TransportStreamMode::HybridWrapper => {
            serve_segmenter_wrapper_stream(state, &channel, headers, query).await
        }
    }
}

#[allow(clippy::too_many_lines)]
pub async fn serve_channel_stream(
    state: &AppState,
    channel_number: &str,
    output_format: OutputFormat,
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
            tracing::debug!(%error, channel_number, "channel not found for stream");
            return StatusCode::NOT_FOUND.into_response();
        }
    };

    let effective_streaming_mode = channel.streaming_mode;
    serve_channel_stream_with_channel(state, channel, output_format, effective_streaming_mode).await
}

pub async fn serve_concat_playlist(
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
            tracing::debug!(%error, channel_number, "channel not found for concat playlist");
            return StatusCode::NOT_FOUND.into_response();
        }
    };
    let mode = query
        .mode
        .as_deref()
        .map(str::trim)
        .filter(|mode| !mode.is_empty())
        .unwrap_or("ts-legacy");
    let playlist = ConcatPlaylist::new(
        hls::request_scheme(headers),
        hls::request_host(headers),
        channel.number,
        mode,
    );

    let mut response = playlist.render().into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

pub async fn serve_internal_legacy_stream(state: &AppState, channel_number: &str) -> Response {
    let channel = match state
        .repositories
        .channel
        .get_by_number(channel_number)
        .await
    {
        Ok(channel) if channel.is_enabled => channel,
        Ok(_) => return StatusCode::NOT_FOUND.into_response(),
        Err(error) => {
            tracing::debug!(%error, channel_number, "channel not found for internal legacy stream");
            return StatusCode::NOT_FOUND.into_response();
        }
    };

    serve_channel_stream_with_channel(
        state,
        channel,
        OutputFormat::MpegTs,
        StreamingMode::TransportStream,
    )
    .await
}

#[allow(clippy::too_many_lines)]
async fn serve_channel_stream_with_channel(
    state: &AppState,
    channel: Channel,
    output_format: OutputFormat,
    effective_streaming_mode: StreamingMode,
) -> Response {
    let now = Utc::now();
    let active = match load_active_media_path(&state.pool, channel.id, now).await {
        Ok(Some(path)) => path,
        Ok(None) => {
            tracing::info!(
                channel_number = %channel.number,
                "no active playout item; returning 503"
            );
            return (StatusCode::SERVICE_UNAVAILABLE, "no active playout item").into_response();
        }
        Err(error) => {
            tracing::error!(%error, channel_number = %channel.number, "failed to load active playout item");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let replacements = match load_path_replacements(&state.repositories.media_source).await {
        Ok(replacements) => replacements,
        Err(error) => {
            tracing::warn!(%error, "failed to load path replacements; continuing with raw path");
            Vec::new()
        }
    };

    let resolved = if replacements.is_empty() {
        active.media_path.clone()
    } else {
        replace_path(&replacements, &active.media_path, PathPlatform::Unix)
    };

    let source_stream = load_source_video_stream(&state.pool, active.media_version_id).await;
    let source_audio_streams =
        load_source_audio_streams(&state.pool, active.media_version_id).await;
    let playback_profile = match load_channel_playback_profile(&state.pool, channel.id).await {
        Ok(Some(profile)) => profile,
        Ok(None) => {
            tracing::error!(channel_number = %channel.number, "channel has no playback ffmpeg profile");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
        Err(error) => {
            tracing::error!(%error, channel_number = %channel.number, "failed to load playback ffmpeg profile");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    let effective_acceleration =
        if effective_streaming_mode == ersatzrs_domain::StreamingMode::HttpLiveStreamingDirect {
            ersatzrs_domain::HardwareAccelerationKind::None
        } else {
            playback_profile.profile.hardware_acceleration
        };
    let source_stream =
        enrich_source_video_stream_for_playback(source_stream, effective_acceleration, &resolved)
            .await;
    let target_frame_rate = if playback_profile.profile.normalize_framerate {
        match load_channel_target_frame_rate(&state.pool, channel.id).await {
            Ok(frame_rate) => frame_rate,
            Err(error) => {
                tracing::warn!(%error, channel_number = %channel.number, "failed to load channel target frame rate");
                None
            }
        }
    } else {
        None
    };
    let runtime_capabilities = state.ffmpeg_runtime.read().await.capabilities.clone();
    let subtitle_mode = active.subtitle_mode.unwrap_or(channel.subtitle_mode);
    let preferred_subtitle_language = active
        .preferred_subtitle_language_code
        .as_deref()
        .or(channel.preferred_subtitle_language_code.as_deref());
    let subtitle = match select_playback_subtitle(
        &state.pool,
        active.media_item_id,
        preferred_subtitle_language,
        subtitle_mode,
        effective_streaming_mode,
    )
    .await
    {
        Ok(subtitle) => subtitle,
        Err(error) => {
            tracing::warn!(%error, media_item_id = active.media_item_id, "failed to select playback subtitle");
            None
        }
    };
    let start_seconds = playout_start_seconds(now, &active.playout_start);
    let metadata = playback_output_metadata(
        Some("ErsatzTV"),
        Some(&channel.name),
        active
            .preferred_audio_language_code
            .as_deref()
            .or(channel.preferred_audio_language_code.as_deref()),
        &source_audio_streams,
        subtitle.as_ref(),
    );
    let subtitle_input = subtitle.as_ref().and_then(|subtitle| {
        subtitle_input_for_stream(subtitle, &resolved, output_format, effective_streaming_mode)
    });
    let subtitle_input = rebase_burned_text_subtitle_input(subtitle_input, start_seconds).await;
    let watermark_inputs = match select_playback_watermarks(
        &state.pool,
        &channel,
        active.playout_item_id,
        active.disable_watermarks,
        now,
    )
    .await
    {
        Ok(watermarks) => watermark_inputs_for_stream(watermarks, active.program_duration()).await,
        Err(error) => {
            tracing::warn!(%error, channel_number = %channel.number, "failed to select playback watermarks");
            Vec::new()
        }
    };
    let mut graphics_cleanup_dirs = Vec::new();
    let graphics_overlays = match select_playback_graphics(
        &state.pool,
        &channel,
        active.playout_item_id,
        now,
    )
    .await
    {
        Ok(graphics) => {
            let frame_size = playback_frame_size(&playback_profile);
            let square_pixel_frame_size = source_stream
                .as_ref()
                .and_then(|stream| stream.square_pixel_frame_size(frame_size))
                .unwrap_or(frame_size);
            let graphics_dir = playback_graphics_dir(channel.id, active.playout_item_id);
            let graphics_cleanup_dir = graphics_dir.clone();
            let start_time = parse_sqlite_utc_timestamp(&active.playout_start).unwrap_or(now);
            match render_playback_graphics_overlays(
                &state.pool,
                &graphics,
                &PlaybackGraphicsRenderContext {
                    channel_id: channel.id,
                    channel_number: channel.number.clone(),
                    media_item_id: active.media_item_id,
                    media_path: resolved.clone(),
                    channel_start_time: start_time,
                    content_start_time: start_time,
                    stream_seek_seconds: seconds_as_f64(start_seconds),
                    duration_seconds: active
                        .program_duration()
                        .map_or(0.0, |duration| duration.as_secs_f64()),
                    frame_size,
                    square_pixel_frame_size,
                    frame_rate: target_frame_rate
                        .or_else(|| source_stream.as_ref().and_then(|stream| stream.frame_rate)),
                },
                graphics_dir,
            )
            .await
            {
                Ok(overlays) => {
                    graphics_cleanup_dirs.push(graphics_cleanup_dir);
                    overlays
                }
                Err(error) => {
                    let _ = tokio::fs::remove_dir_all(&graphics_cleanup_dir).await;
                    tracing::warn!(%error, channel_number = %channel.number, "failed to render playback graphics");
                    Vec::new()
                }
            }
        }
        Err(error) => {
            tracing::warn!(%error, channel_number = %channel.number, "failed to select playback graphics");
            Vec::new()
        }
    };
    let command = build_stream_command(
        &resolved,
        output_format,
        start_seconds,
        &playback_profile,
        target_frame_rate,
        effective_streaming_mode,
        metadata,
        runtime_capabilities,
        source_stream,
        source_audio_streams,
        subtitle_input,
        watermark_inputs,
        graphics_overlays,
    );
    match spawn_stream_body(
        command,
        graphics_cleanup_dirs,
        channel.number.clone(),
        state.hls_sessions.clone(),
    ) {
        Ok(body) => {
            let mut response = body.into_response();
            response.headers_mut().insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static(content_type_for_output_format(output_format)),
            );
            response
                .headers_mut()
                .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
            response
        }
        Err(error) => {
            tracing::error!(%error, "failed to spawn ffmpeg for channel stream");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TransportStreamMode {
    RedirectToHls,
    Legacy,
    HybridWrapper,
}

fn resolve_transport_stream_mode(
    requested_mode: Option<&str>,
    channel_streaming_mode: StreamingMode,
) -> TransportStreamMode {
    match requested_mode.unwrap_or_default().trim() {
        "" | "mixed" => match channel_streaming_mode {
            StreamingMode::TransportStream => TransportStreamMode::Legacy,
            StreamingMode::TransportStreamHybrid => TransportStreamMode::HybridWrapper,
            StreamingMode::HttpLiveStreamingDirect | StreamingMode::HttpLiveStreamingSegmenter => {
                TransportStreamMode::RedirectToHls
            }
        },
        "ts-legacy" => TransportStreamMode::Legacy,
        _ => TransportStreamMode::HybridWrapper,
    }
}

async fn serve_segmenter_wrapper_stream(
    state: &AppState,
    channel: &Channel,
    headers: &HeaderMap,
    query: &HlsQuery,
) -> Response {
    let path_replacements = match load_path_replacements(&state.repositories.media_source).await {
        Ok(replacements) => replacements,
        Err(error) => {
            tracing::warn!(%error, channel_number = %channel.number, "failed to load path replacements for wrapped segmenter; continuing with raw path");
            Vec::new()
        }
    };

    hls::ensure_segmenter_worker_running(
        state,
        &channel.number,
        channel.id,
        channel.idle_behavior,
        OutputFormat::Hls,
        path_replacements,
    )
    .await;

    let aac_latm_audio = match load_channel_playback_profile(&state.pool, channel.id).await {
        Ok(Some(profile)) => profile.profile.audio_format == FFmpegProfileAudioFormat::AacLatm,
        Ok(None) => {
            tracing::error!(channel_number = %channel.number, "channel has no playback ffmpeg profile");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
        Err(error) => {
            tracing::error!(%error, channel_number = %channel.number, "failed to load playback ffmpeg profile for wrapped segmenter");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    let access_token = AccessToken::new(query.access_token.clone());
    let hls_url = build_segmenter_wrapper_hls_url(
        &hls::request_scheme(headers),
        &hls::request_host(headers),
        &channel.number,
        &access_token,
    );
    let metadata = OutputMetadata {
        service_provider: Some("ErsatzTV".to_owned()),
        service_name: Some(channel.name.clone()),
        ..OutputMetadata::default()
    };
    let (command, cleanup_paths) =
        match configured_mpegts_script_command(state, channel, &hls_url).await {
            Some(command) => command,
            None => (
                build_mpegts_segmenter_wrapper_command(hls_url, &metadata, aac_latm_audio),
                Vec::new(),
            ),
        };

    match spawn_stream_body(
        command,
        cleanup_paths,
        channel.number.clone(),
        state.hls_sessions.clone(),
    ) {
        Ok(body) => {
            let mut response = body.into_response();
            response
                .headers_mut()
                .insert(header::CONTENT_TYPE, HeaderValue::from_static("video/mp2t"));
            response
                .headers_mut()
                .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
            response
        }
        Err(error) => {
            tracing::error!(%error, channel_number = %channel.number, "failed to spawn ffmpeg for wrapped segmenter");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

fn serve_legacy_concat_stream(
    state: &AppState,
    channel: &Channel,
    headers: &HeaderMap,
) -> Response {
    let concat_url = build_legacy_concat_url(
        &hls::request_scheme(headers),
        &hls::request_host(headers),
        &channel.number,
    );
    let metadata = OutputMetadata {
        service_provider: Some("ErsatzTV".to_owned()),
        service_name: Some(channel.name.clone()),
        ..OutputMetadata::default()
    };
    let command = build_mpegts_concat_command(concat_url, &metadata);

    match spawn_stream_body(
        command,
        Vec::new(),
        channel.number.clone(),
        state.hls_sessions.clone(),
    ) {
        Ok(body) => {
            let mut response = body.into_response();
            response
                .headers_mut()
                .insert(header::CONTENT_TYPE, HeaderValue::from_static("video/mp2t"));
            response
                .headers_mut()
                .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
            response
        }
        Err(error) => {
            tracing::error!(%error, channel_number = %channel.number, "failed to spawn ffmpeg for legacy concat stream");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

fn build_legacy_concat_url(scheme: &str, host: &str, channel_number: &str) -> String {
    format!("{scheme}://{host}/ffmpeg/concat/{channel_number}?mode=ts-legacy")
}

fn build_segmenter_wrapper_hls_url(
    scheme: &str,
    host: &str,
    channel_number: &str,
    access_token: &AccessToken,
) -> String {
    format!(
        "{scheme}://{host}/iptv/channel/{channel_number}.m3u8?mode=segmenter{}",
        access_token.query_amp()
    )
}

async fn configured_mpegts_script_command(
    state: &AppState,
    channel: &Channel,
    hls_url: &str,
) -> Option<(FfmpegCommand, Vec<PathBuf>)> {
    let configured = match state
        .repositories
        .config
        .get(ersatzrs_domain::CONFIG_KEY_FFMPEG_DEFAULT_MPEG_TS_SCRIPT)
        .await
    {
        Ok(value) => value,
        Err(error) => {
            tracing::warn!(%error, channel_number = %channel.number, "failed to load default MPEG-TS script config");
            None
        }
    }
    .unwrap_or_else(|| DEFAULT_MPEG_TS_SCRIPT_ID.to_owned());
    let configured = configured.trim();
    if configured.is_empty() {
        return None;
    }

    let Some(script) = find_mpegts_script(configured).await else {
        if should_warn_missing_mpegts_script(configured) {
            tracing::warn!(
                script_id = configured,
                "unable to locate MPEG-TS script folder"
            );
        }
        return None;
    };

    match render_mpegts_script_command(
        &script,
        &MpegTsScriptRenderData {
            hls_url: hls_url.to_owned(),
            channel_name: channel.name.clone(),
            ffmpeg_path: ersatzrs_ffmpeg::ffmpeg_path(),
        },
    )
    .await
    {
        Ok(command) => Some(command),
        Err(error) => {
            tracing::warn!(%error, script_id = %script.id, "failed to prepare MPEG-TS script command; falling back to default wrapper");
            None
        }
    }
}

async fn find_mpegts_script(script_id: &str) -> Option<MpegTsScript> {
    find_mpegts_script_in(&file_system_layout::mpegts_scripts_folder(), script_id).await
}

fn should_warn_missing_mpegts_script(script_id: &str) -> bool {
    script_id != DEFAULT_MPEG_TS_SCRIPT_ID
}

async fn find_mpegts_script_in(root: &std::path::Path, script_id: &str) -> Option<MpegTsScript> {
    let mut entries = tokio::fs::read_dir(&root).await.ok()?;
    while let Ok(Some(entry)) = entries.next_entry().await {
        let Ok(file_type) = entry.file_type().await else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }
        let id = entry.file_name().to_string_lossy().into_owned();
        if id != script_id {
            continue;
        }
        let folder = entry.path();
        let yaml = tokio::fs::read_to_string(folder.join("mpegts.yml"))
            .await
            .ok()?;
        let definition = match serde_yaml::from_str::<MpegTsScriptDefinition>(&yaml) {
            Ok(definition) => definition,
            Err(error) => {
                tracing::warn!(%error, script_id, "failed to parse MPEG-TS script YAML definition");
                return None;
            }
        };
        return Some(MpegTsScript {
            id,
            folder,
            definition,
        });
    }
    None
}

async fn render_mpegts_script_command(
    script: &MpegTsScript,
    data: &MpegTsScriptRenderData,
) -> Result<(FfmpegCommand, Vec<PathBuf>), std::io::Error> {
    let script_name = platform_mpegts_script_name(&script.definition);
    let Some(script_name) = script_name
        .as_deref()
        .filter(|name| !name.trim().is_empty())
    else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "mpegts script definition has no script for this platform",
        ));
    };
    let template_path = script.folder.join(script_name);
    let template = tokio::fs::read_to_string(&template_path).await?;
    let rendered = render_mpegts_script_template(&template, data);
    let temp_path = temp_mpegts_script_path(cfg!(windows));
    tokio::fs::write(&temp_path, rendered).await?;

    if cfg!(windows) {
        Ok((
            FfmpegCommand::new_with_executable(
                temp_path.to_string_lossy().into_owned(),
                Vec::new(),
                Vec::new(),
            ),
            vec![temp_path],
        ))
    } else {
        Ok((
            FfmpegCommand::new_with_executable(
                "bash",
                Vec::new(),
                vec![temp_path.to_string_lossy().into_owned()],
            ),
            vec![temp_path],
        ))
    }
}

fn platform_mpegts_script_name(definition: &MpegTsScriptDefinition) -> Option<String> {
    if cfg!(windows) {
        definition.windows_script.clone()
    } else {
        definition.linux_script.clone()
    }
}

fn temp_mpegts_script_path(windows: bool) -> PathBuf {
    let extension = if windows { "bat" } else { "sh" };
    std::env::temp_dir().join(format!(
        "ersatzrs-mpegts-script-{}.{}",
        uuid::Uuid::new_v4(),
        extension
    ))
}

fn render_mpegts_script_template(template: &str, data: &MpegTsScriptRenderData) -> String {
    template
        .replace("{{ HlsUrl }}", &data.hls_url)
        .replace("{{HlsUrl}}", &data.hls_url)
        .replace("{{ ChannelName }}", &data.channel_name)
        .replace("{{ChannelName}}", &data.channel_name)
        .replace("{{ FFmpegPath }}", &data.ffmpeg_path)
        .replace("{{FFmpegPath}}", &data.ffmpeg_path)
}

#[derive(Debug, Clone, thiserror::Error)]
enum StreamSpawnError {
    #[error("failed to spawn ffmpeg: {0}")]
    Spawn(String),
    #[error("ffmpeg did not expose stdout")]
    MissingStdout,
}

#[derive(Debug, Clone)]
pub(crate) struct ActivePlayoutItem {
    pub(crate) playout_item_id: i64,
    pub(crate) media_path: String,
    pub(crate) media_item_id: i64,
    pub(crate) media_version_id: i64,
    pub(crate) playout_start: String,
    pub(crate) playout_finish: String,
    pub(crate) disable_watermarks: bool,
    pub(crate) preferred_audio_language_code: Option<String>,
    pub(crate) preferred_subtitle_language_code: Option<String>,
    pub(crate) subtitle_mode: Option<ChannelSubtitleMode>,
}

impl ActivePlayoutItem {
    fn program_duration(&self) -> Option<std::time::Duration> {
        let start = parse_sqlite_utc_timestamp(&self.playout_start)?;
        let finish = parse_sqlite_utc_timestamp(&self.playout_finish)?;
        (finish - start).to_std().ok()
    }
}

pub(crate) async fn load_active_media_path(
    pool: &DbPool,
    channel_id: i64,
    now: DateTime<Utc>,
) -> Result<Option<ActivePlayoutItem>, sqlx::Error> {
    let now_text = now.format("%Y-%m-%d %H:%M:%S").to_string();
    let row = sqlx::query(
        r#"SELECT mf."Path" AS "MediaPath",
                  pi."Id" AS "PlayoutItemId",
                  pi."MediaItemId" AS "MediaItemId",
                  mv."Id" AS "MediaVersionId",
                  pi."Start" AS "PlayoutStart",
                  pi."Finish" AS "PlayoutFinish",
                  pi."DisableWatermarks" AS "DisableWatermarks",
                  pi."PreferredAudioLanguageCode" AS "PreferredAudioLanguageCode",
                  pi."PreferredSubtitleLanguageCode" AS "PreferredSubtitleLanguageCode",
                  pi."SubtitleMode" AS "SubtitleMode"
           FROM "PlayoutItem" pi
           JOIN "Playout" p ON p."Id" = pi."PlayoutId"
           JOIN "MediaVersion" mv
               ON mv."MovieId" = pi."MediaItemId"
               OR mv."EpisodeId" = pi."MediaItemId"
               OR mv."MusicVideoId" = pi."MediaItemId"
               OR mv."OtherVideoId" = pi."MediaItemId"
               OR mv."RemoteStreamId" = pi."MediaItemId"
               OR mv."SongId" = pi."MediaItemId"
               OR mv."ImageId" = pi."MediaItemId"
           JOIN "MediaFile" mf ON mf."MediaVersionId" = mv."Id"
           WHERE p."ChannelId" = ?
             AND pi."Start" <= ?
             AND pi."Finish" > ?
             AND mf."Path" IS NOT NULL
           ORDER BY pi."Start" DESC, pi."Id" DESC
           LIMIT 1"#,
    )
    .bind(channel_id)
    .bind(&now_text)
    .bind(&now_text)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|row| ActivePlayoutItem {
        playout_item_id: row.get::<i64, _>("PlayoutItemId"),
        media_path: row.get::<String, _>("MediaPath"),
        media_item_id: row.get::<i64, _>("MediaItemId"),
        media_version_id: row.get::<i64, _>("MediaVersionId"),
        playout_start: row.get::<String, _>("PlayoutStart"),
        playout_finish: row.get::<String, _>("PlayoutFinish"),
        disable_watermarks: row
            .try_get::<i64, _>("DisableWatermarks")
            .is_ok_and(|value| value != 0),
        preferred_audio_language_code: row
            .try_get::<Option<String>, _>("PreferredAudioLanguageCode")
            .ok()
            .flatten(),
        preferred_subtitle_language_code: row
            .try_get::<Option<String>, _>("PreferredSubtitleLanguageCode")
            .ok()
            .flatten(),
        subtitle_mode: row
            .try_get::<Option<i64>, _>("SubtitleMode")
            .ok()
            .flatten()
            .and_then(decode_subtitle_mode),
    }))
}

pub(crate) async fn load_path_replacements(
    media_sources: &impl MediaSourceRepository,
) -> Result<Vec<PathReplacement>, ersatzrs_core::repositories::RepositoryError> {
    let all = media_sources.list_all().await?;
    let mut replacements = Vec::new();
    for source in all {
        match source.kind {
            ersatzrs_domain::MediaSourceKind::Plex => {
                let PlexMediaSource {
                    path_replacements, ..
                } = media_sources.get_plex(source.id).await?;
                push_replacements(
                    path_replacements
                        .into_iter()
                        .map(|r| (r.path, r.replacement_path)),
                    &mut replacements,
                );
            }
            ersatzrs_domain::MediaSourceKind::Jellyfin => {
                let JellyfinMediaSource {
                    path_replacements, ..
                } = media_sources.get_jellyfin(source.id).await?;
                push_replacements(
                    path_replacements
                        .into_iter()
                        .map(|r| (r.path, r.replacement_path)),
                    &mut replacements,
                );
            }
            ersatzrs_domain::MediaSourceKind::Emby => {
                let EmbyMediaSource {
                    path_replacements, ..
                } = media_sources.get_emby(source.id).await?;
                push_replacements(
                    path_replacements
                        .into_iter()
                        .map(|r| (r.path, r.replacement_path)),
                    &mut replacements,
                );
            }
            ersatzrs_domain::MediaSourceKind::Local => {}
        }
    }
    Ok(replacements)
}

fn push_replacements(
    source: impl IntoIterator<Item = (String, String)>,
    out: &mut Vec<PathReplacement>,
) {
    for (path, replacement_path) in source {
        let source_platform = detect_platform(&path);
        out.push(PathReplacement {
            source_platform,
            source_path: path,
            replacement_path,
        });
    }
}

fn detect_platform(path: &str) -> PathPlatform {
    if path.contains('\\') || path.contains(':') {
        PathPlatform::Windows
    } else {
        PathPlatform::Unix
    }
}

pub(crate) fn content_type_for_output_format(output_format: OutputFormat) -> &'static str {
    match output_format {
        OutputFormat::MpegTs => "video/mp2t",
        OutputFormat::Mp4 => "video/mp4",
        OutputFormat::Mkv => "video/x-matroska",
        OutputFormat::Nut | OutputFormat::Hls | OutputFormat::HlsMp4 => "application/octet-stream",
    }
}

pub(crate) fn extension_for_output_format(output_format: OutputFormat) -> &'static str {
    match output_format {
        OutputFormat::MpegTs => ".ts",
        OutputFormat::Mp4 => ".mp4",
        OutputFormat::Mkv => ".mkv",
        OutputFormat::Nut | OutputFormat::Hls | OutputFormat::HlsMp4 => "",
    }
}

#[allow(clippy::too_many_arguments)]
fn build_stream_command(
    path: &str,
    output_format: OutputFormat,
    start_seconds: i64,
    playback_profile: &PlaybackProfile,
    target_frame_rate: Option<FrameRate>,
    streaming_mode: ersatzrs_domain::StreamingMode,
    metadata: OutputMetadata,
    runtime_capabilities: FfmpegCapabilityReport,
    source_stream: Option<SourceVideoStream>,
    source_audio_streams: Vec<ersatzrs_ffmpeg::SourceAudioStream>,
    subtitle_input: Option<InputFile>,
    watermark_inputs: Vec<(InputFile, ersatzrs_ffmpeg::WatermarkOverlay)>,
    graphics_overlays: Vec<GraphicsPipelineOverlay>,
) -> FfmpegCommand {
    let input = InputFile::video(path.to_string());
    let desired = DesiredVideoState::from_profile(
        &playback_profile.profile,
        &playback_profile.resolution,
        source_stream.as_ref(),
        target_frame_rate,
        streaming_mode,
    );
    let mut request =
        PipelineRequest::new(input, desired).with_runtime_capabilities(runtime_capabilities);
    if let Some(source_stream) = source_stream {
        request = request.with_source_video_stream(source_stream);
    }
    request = request.with_source_audio_streams(source_audio_streams);
    if let Some(subtitle_input) = subtitle_input {
        request.inputs.push(subtitle_input);
        request = request.with_subtitle_fonts_dir(subtitle_fonts_dir());
    }
    for (input, overlay) in watermark_inputs {
        request.inputs.push(input);
        request.watermarks.push(overlay);
    }
    request.graphics_overlays.extend(graphics_overlays);
    request.state.output_format = output_format;
    if start_seconds > 0 {
        request.state.start_seconds = Some(start_seconds);
    }
    request.state.metadata = metadata;
    request.state.report_progress = false;
    if let Ok(thread_count) = u32::try_from(playback_profile.profile.thread_count) {
        if thread_count > 0 {
            request.state.thread_count = Some(thread_count);
        }
    }

    let acceleration = if streaming_mode == ersatzrs_domain::StreamingMode::HttpLiveStreamingDirect
    {
        ersatzrs_domain::HardwareAccelerationKind::None
    } else {
        playback_profile.profile.hardware_acceleration
    };
    let pipeline = build_pipeline_for_acceleration(acceleration, request.clone());
    pipeline.command(&request.inputs)
}

pub(crate) fn playback_output_metadata(
    service_provider: Option<&str>,
    service_name: Option<&str>,
    preferred_audio_language: Option<&str>,
    source_audio_streams: &[SourceAudioStream],
    subtitle: Option<&PlaybackSubtitle>,
) -> OutputMetadata {
    let audio_language = preferred_audio_language
        .and_then(trimmed_string)
        .or_else(|| first_audio_language(source_audio_streams));

    OutputMetadata {
        service_provider: service_provider.and_then(trimmed_string),
        service_name: service_name.and_then(trimmed_string),
        audio_language,
        subtitle_language: subtitle
            .and_then(|subtitle| subtitle.language.as_deref())
            .and_then(trimmed_string),
        subtitle_title: subtitle
            .and_then(|subtitle| subtitle.title.as_deref())
            .and_then(trimmed_string),
    }
}

fn first_audio_language(source_audio_streams: &[SourceAudioStream]) -> Option<String> {
    source_audio_streams
        .iter()
        .filter_map(|stream| stream.language.as_deref())
        .find_map(trimmed_string)
}

fn trimmed_string(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_owned())
    }
}

fn playback_frame_size(playback_profile: &PlaybackProfile) -> FrameSize {
    FrameSize::new(
        u32::try_from(playback_profile.resolution.width)
            .ok()
            .filter(|value| *value > 0)
            .unwrap_or(1920),
        u32::try_from(playback_profile.resolution.height)
            .ok()
            .filter(|value| *value > 0)
            .unwrap_or(1080),
    )
}

pub(crate) fn playout_start_seconds(now: DateTime<Utc>, playout_start: &str) -> i64 {
    parse_sqlite_utc_timestamp(playout_start).map_or(0, |start| (now - start).num_seconds().max(0))
}

fn seconds_as_f64(seconds: i64) -> f64 {
    u64::try_from(seconds)
        .map(Duration::from_secs)
        .map_or(0.0, |duration| duration.as_secs_f64())
}

fn parse_sqlite_utc_timestamp(value: &str) -> Option<DateTime<Utc>> {
    for format in ["%Y-%m-%d %H:%M:%S%.f", "%Y-%m-%d %H:%M:%S"] {
        if let Ok(parsed) = NaiveDateTime::parse_from_str(value, format) {
            return Some(DateTime::<Utc>::from_naive_utc_and_offset(parsed, Utc));
        }
    }
    None
}

const fn decode_subtitle_mode(value: i64) -> Option<ChannelSubtitleMode> {
    match value {
        0 => Some(ChannelSubtitleMode::None),
        1 => Some(ChannelSubtitleMode::Forced),
        2 => Some(ChannelSubtitleMode::Default),
        3 => Some(ChannelSubtitleMode::Any),
        _ => None,
    }
}

fn playback_graphics_dir(channel_id: i64, playout_item_id: i64) -> PathBuf {
    let request_id = NEXT_GRAPHICS_REQUEST_ID.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir()
        .join("ersatzrs-graphics")
        .join(format!("channel-{channel_id}"))
        .join(format!("item-{playout_item_id}"))
        .join(format!("request-{request_id}"))
}

fn spawn_stream_body(
    command: FfmpegCommand,
    cleanup_paths: Vec<PathBuf>,
    channel_number: String,
    session_registry: Arc<dyn HlsSessionRegistry>,
) -> Result<Body, StreamSpawnError> {
    let mut process = Command::new(&command.executable);
    process
        .args(&command.arguments)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    for variable in command.environment {
        process.env(variable.name, variable.value);
    }

    let mut child = process
        .spawn()
        .map_err(|error| StreamSpawnError::Spawn(error.to_string()))?;
    let stdout = child.stdout.take().ok_or(StreamSpawnError::MissingStdout)?;
    let stderr = child.stderr.take();
    let handle = Arc::new(DirectStreamHandle::new(channel_number.clone()));
    let cancel = handle.cancel.clone();

    tokio::spawn(async move {
        let registered = session_registry.try_insert(handle).await;
        let stderr_task = stderr.map(|stderr| tokio::spawn(read_ffmpeg_stderr_tail(stderr)));
        tokio::select! {
            result = child.wait() => {
                match result {
                    Ok(status) if status.success() => {}
                    Ok(status) => {
                        let stderr = await_stderr_tail(stderr_task).await;
                        if ffmpeg_stderr_is_client_disconnect(&stderr) {
                            tracing::debug!(%status, stderr = %stderr, "ffmpeg channel stream ended after client disconnect");
                        } else {
                            tracing::warn!(%status, stderr = %stderr, "ffmpeg channel stream exited unsuccessfully");
                        }
                    }
                    Err(error) => {
                        let stderr = await_stderr_tail(stderr_task).await;
                        tracing::warn!(%error, stderr = %stderr, "failed to wait for ffmpeg channel stream");
                    }
                }
            }
            () = cancel.notified() => {
                if let Err(error) = child.kill().await {
                    tracing::warn!(%error, channel_number = %channel_number, "failed to kill ffmpeg channel stream");
                }
                let stderr = await_stderr_tail(stderr_task).await;
                if !stderr.is_empty() {
                    tracing::debug!(stderr = %stderr, channel_number = %channel_number, "ffmpeg channel stream stderr after cancellation");
                }
            }
        }
        if registered {
            session_registry.remove(&channel_number).await;
        }
        for path in cleanup_paths {
            cleanup_stream_artifact(&path).await;
        }
    });

    Ok(Body::from_stream(ReaderStream::new(stdout)))
}

async fn cleanup_stream_artifact(path: &std::path::Path) {
    let result = match tokio::fs::metadata(path).await {
        Ok(metadata) if metadata.is_dir() => tokio::fs::remove_dir_all(path).await,
        Ok(_) => tokio::fs::remove_file(path).await,
        Err(error) => Err(error),
    };
    if let Err(error) = result {
        if error.kind() != std::io::ErrorKind::NotFound {
            tracing::debug!(%error, path = %path.display(), "failed to clean up IPTV stream artifact");
        }
    }
}

struct DirectStreamHandle {
    channel_number: String,
    cancel: Arc<Notify>,
}

impl DirectStreamHandle {
    fn new(channel_number: String) -> Self {
        Self {
            channel_number,
            cancel: Arc::new(Notify::new()),
        }
    }
}

#[async_trait::async_trait]
impl HlsSessionHandle for DirectStreamHandle {
    fn channel_number(&self) -> &str {
        &self.channel_number
    }

    fn touch(&self) {}

    async fn trim_playlist(&self) -> Option<String> {
        None
    }

    async fn cancel(&self) {
        self.cancel.notify_waiters();
    }

    fn playout_updated(&self) {}
}

async fn read_ffmpeg_stderr_tail(mut stderr: tokio::process::ChildStderr) -> String {
    let mut tail = Vec::new();
    let mut buffer = [0_u8; 4096];
    loop {
        match stderr.read(&mut buffer).await {
            Ok(0) => break,
            Ok(read) => {
                tail.extend_from_slice(&buffer[..read]);
                if tail.len() > FFMPEG_STDERR_TAIL_BYTES {
                    let remove = tail.len() - FFMPEG_STDERR_TAIL_BYTES;
                    tail.drain(..remove);
                }
            }
            Err(error) => return format!("failed to read ffmpeg stderr: {error}"),
        }
    }

    String::from_utf8_lossy(&tail).trim().to_owned()
}

async fn await_stderr_tail(stderr_task: Option<tokio::task::JoinHandle<String>>) -> String {
    match stderr_task {
        Some(task) => task
            .await
            .unwrap_or_else(|error| format!("failed to join ffmpeg stderr reader: {error}")),
        None => String::new(),
    }
}

fn ffmpeg_stderr_is_client_disconnect(stderr: &str) -> bool {
    stderr.contains("Broken pipe")
}

#[cfg(test)]
mod tests {
    use super::*;
    use ersatzrs_domain::{
        FFmpegProfile, FFmpegProfileAudioFormat, FFmpegProfileVideoFormat, Resolution,
        StreamingMode, WatermarkLocation, WatermarkSize,
    };
    use ersatzrs_ffmpeg::{FfmpegCapabilityReport, GraphicsPipelineOverlay, WatermarkOverlay};
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    const TESTDATA_CANDIDATES: &[&str] = &[
        "testdata/bbb_sunflower_1080p_30fps_normal.mp4",
        "testdata/bbb_sunflower_1080p_60fps_normal.mp4",
        "testdata/bbb_sunflower_2160p_30fps_normal.mp4",
        "testdata/bbb_sunflower_2160p_60fps_normal.mp4",
        "../../testdata/bbb_sunflower_1080p_30fps_normal.mp4",
        "../../testdata/bbb_sunflower_1080p_60fps_normal.mp4",
        "../../testdata/bbb_sunflower_2160p_30fps_normal.mp4",
        "../../testdata/bbb_sunflower_2160p_60fps_normal.mp4",
    ];

    #[test]
    fn build_stream_command_pipes_mpegts_to_stdout() {
        let profile = playback_profile();
        let command = build_stream_command(
            "/media/movie.mkv",
            OutputFormat::MpegTs,
            0,
            &profile,
            None,
            StreamingMode::TransportStream,
            OutputMetadata::default(),
            FfmpegCapabilityReport::default(),
            None,
            Vec::new(),
            None,
            Vec::new(),
            Vec::new(),
        );
        let arguments = command.arguments.join(" ");
        assert!(arguments.contains("-f mpegts"));
        assert!(arguments.ends_with("pipe:1"));
        assert!(!arguments.contains("-progress pipe:1"));
    }

    #[test]
    fn build_stream_command_can_target_mp4() {
        let profile = playback_profile();
        let command = build_stream_command(
            "/media/movie.mkv",
            OutputFormat::Mp4,
            0,
            &profile,
            None,
            StreamingMode::HttpLiveStreamingDirect,
            OutputMetadata::default(),
            FfmpegCapabilityReport::default(),
            None,
            Vec::new(),
            None,
            Vec::new(),
            Vec::new(),
        );
        let arguments = command.arguments.join(" ");
        assert!(arguments.contains("-f mp4"));
        assert!(arguments.ends_with("pipe:1"));
    }

    #[test]
    fn build_stream_command_seeks_to_playout_offset() {
        let profile = playback_profile();
        let command = build_stream_command(
            "/media/movie.mkv",
            OutputFormat::MpegTs,
            900,
            &profile,
            None,
            StreamingMode::TransportStream,
            OutputMetadata::default(),
            FfmpegCapabilityReport::default(),
            None,
            Vec::new(),
            None,
            Vec::new(),
            Vec::new(),
        );
        let arguments = command.arguments.join(" ");
        // ErsatzTV emits the seek as an input-side option (`StreamSeekInputOption`,
        // added before the decoder in `PipelineBuilderBase`), so `-ss 900` precedes
        // the `-i` input with the source decoder option in between. Assert the
        // input-side seek contract by position rather than hard-coding the decoder
        // string (that argv is covered by the pipeline snapshot suite).
        let seek = arguments
            .find("-ss 900")
            .unwrap_or_else(|| panic!("expected stream seek in arguments: {arguments}"));
        let input = arguments
            .find("-i /media/movie.mkv")
            .unwrap_or_else(|| panic!("expected input file in arguments: {arguments}"));
        assert!(
            seek < input,
            "stream seek must precede the input file: {arguments}"
        );
    }

    #[test]
    fn build_stream_command_emits_channel_output_metadata() {
        let profile = playback_profile();
        let command = build_stream_command(
            "/media/movie.mkv",
            OutputFormat::MpegTs,
            0,
            &profile,
            None,
            StreamingMode::TransportStream,
            OutputMetadata {
                service_provider: Some("ErsatzTV".to_owned()),
                service_name: Some("Cinema One".to_owned()),
                audio_language: Some("eng".to_owned()),
                subtitle_language: Some("nld".to_owned()),
                subtitle_title: Some("Dutch SDH".to_owned()),
            },
            FfmpegCapabilityReport::default(),
            None,
            Vec::new(),
            None,
            Vec::new(),
            Vec::new(),
        );
        let arguments = command.arguments.join(" ");

        assert!(arguments.contains("-metadata service_provider=\"ErsatzTV\""));
        assert!(arguments.contains("-metadata service_name=\"Cinema One\""));
        assert!(arguments.contains("-metadata:s:a:0 language=eng"));
        assert!(arguments.contains("-metadata:s:s:0 language=nld"));
        assert!(arguments.contains("-metadata:s:s:0 title=Dutch SDH"));
    }

    #[test]
    fn build_stream_command_attaches_graphics_overlays() {
        let profile = playback_profile();
        let command = build_stream_command(
            "/media/movie.mkv",
            OutputFormat::MpegTs,
            0,
            &profile,
            None,
            StreamingMode::TransportStream,
            OutputMetadata::default(),
            FfmpegCapabilityReport::default(),
            None,
            Vec::new(),
            None,
            Vec::new(),
            vec![GraphicsPipelineOverlay::drawvg(
                8,
                "drawvg=file=/tmp/iptv-graphics/title.vgs",
            )],
        );
        let arguments = command.arguments.join(" ");

        assert!(arguments.contains("-filter_complex"));
        assert!(
            arguments.contains("drawvg=file=/tmp/iptv-graphics/title.vgs"),
            "{arguments}"
        );
    }

    #[test]
    fn playback_graphics_dir_uses_unique_request_subdirectories() {
        let first = playback_graphics_dir(10, 20);
        let second = playback_graphics_dir(10, 20);

        assert_ne!(first, second);
        assert!(first
            .parent()
            .is_some_and(|path| path.ends_with(PathBuf::from("channel-10/item-20"))));
        assert!(first
            .file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with("request-")));
    }

    #[test]
    fn playout_start_seconds_matches_mid_item_offset() {
        let now = DateTime::parse_from_rfc3339("2026-05-31T21:45:00Z")
            .expect("valid timestamp")
            .with_timezone(&Utc);
        assert_eq!(playout_start_seconds(now, "2026-05-31 21:30:00"), 900);
        assert_eq!(playout_start_seconds(now, "2026-05-31 21:45:00"), 0);
        assert_eq!(playout_start_seconds(now, "not a timestamp"), 0);
    }

    #[test]
    fn output_format_helpers_match_hls_direct_variants() {
        assert_eq!(extension_for_output_format(OutputFormat::MpegTs), ".ts");
        assert_eq!(extension_for_output_format(OutputFormat::Mp4), ".mp4");
        assert_eq!(extension_for_output_format(OutputFormat::Mkv), ".mkv");
        assert_eq!(
            content_type_for_output_format(OutputFormat::MpegTs),
            "video/mp2t"
        );
        assert_eq!(
            content_type_for_output_format(OutputFormat::Mp4),
            "video/mp4"
        );
        assert_eq!(
            content_type_for_output_format(OutputFormat::Mkv),
            "video/x-matroska"
        );
    }

    #[test]
    fn transport_stream_mode_resolves_channel_setting_for_mixed_requests() {
        assert_eq!(
            resolve_transport_stream_mode(None, StreamingMode::TransportStream),
            TransportStreamMode::Legacy
        );
        assert_eq!(
            resolve_transport_stream_mode(Some("mixed"), StreamingMode::TransportStreamHybrid),
            TransportStreamMode::HybridWrapper
        );
        assert_eq!(
            resolve_transport_stream_mode(None, StreamingMode::HttpLiveStreamingSegmenter),
            TransportStreamMode::RedirectToHls
        );
    }

    #[test]
    fn transport_stream_mode_explicit_query_overrides_channel_setting() {
        assert_eq!(
            resolve_transport_stream_mode(Some("ts-legacy"), StreamingMode::TransportStreamHybrid),
            TransportStreamMode::Legacy
        );
        assert_eq!(
            resolve_transport_stream_mode(Some("ts"), StreamingMode::TransportStream),
            TransportStreamMode::HybridWrapper
        );
    }

    #[test]
    fn segmenter_wrapper_url_preserves_access_token() {
        let token = AccessToken::new(Some("abc123".to_owned()));
        assert_eq!(
            build_segmenter_wrapper_hls_url("http", "localhost:8409", "7", &token),
            "http://localhost:8409/iptv/channel/7.m3u8?mode=segmenter&access_token=abc123"
        );
    }

    #[test]
    fn mpegts_script_yaml_uses_ersatztv_field_names() {
        let script: MpegTsScriptDefinition = serde_yaml::from_str(
            "name: Default\nlinux_script: mpegts.sh\nwindows_script: mpegts.bat\n",
        )
        .expect("parse script yaml");

        assert_eq!(script.name, "Default");
        assert_eq!(script.linux_script.as_deref(), Some("mpegts.sh"));
        assert_eq!(script.windows_script.as_deref(), Some("mpegts.bat"));
    }

    #[test]
    fn mpegts_script_renderer_substitutes_known_scriban_variables() {
        let rendered = render_mpegts_script_template(
            r#""{{ FFmpegPath }}" -i "{{ HlsUrl }}" -metadata service_name="{{ ChannelName }}""#,
            &MpegTsScriptRenderData {
                hls_url: "http://localhost:8409/iptv/channel/1.m3u8?mode=segmenter".to_owned(),
                channel_name: "Channel One".to_owned(),
                ffmpeg_path: "/opt/ffmpeg/bin/ffmpeg".to_owned(),
            },
        );

        assert_eq!(
            rendered,
            r#""/opt/ffmpeg/bin/ffmpeg" -i "http://localhost:8409/iptv/channel/1.m3u8?mode=segmenter" -metadata service_name="Channel One""#
        );
    }

    #[test]
    fn mpegts_script_platform_selection_is_cfg_guarded() {
        let definition = MpegTsScriptDefinition {
            name: "Default".to_owned(),
            linux_script: Some("mpegts.sh".to_owned()),
            windows_script: Some("mpegts.bat".to_owned()),
        };

        if cfg!(windows) {
            assert_eq!(
                platform_mpegts_script_name(&definition).as_deref(),
                Some("mpegts.bat")
            );
        } else {
            assert_eq!(
                platform_mpegts_script_name(&definition).as_deref(),
                Some("mpegts.sh")
            );
        }
    }

    #[test]
    fn mpegts_script_temp_path_uses_platform_extension() {
        assert_eq!(
            temp_mpegts_script_path(false)
                .extension()
                .and_then(|extension| extension.to_str()),
            Some("sh")
        );
        assert_eq!(
            temp_mpegts_script_path(true)
                .extension()
                .and_then(|extension| extension.to_str()),
            Some("bat")
        );
    }

    #[test]
    fn missing_builtin_mpegts_script_falls_back_quietly() {
        assert!(!should_warn_missing_mpegts_script("default"));
        assert!(should_warn_missing_mpegts_script("custom"));
    }

    #[test]
    fn ffmpeg_broken_pipe_stderr_is_client_disconnect() {
        assert!(ffmpeg_stderr_is_client_disconnect(
            "[out#0/mpegts @ 0x57fc78188a80] Error writing trailer: Broken pipe"
        ));
        assert!(!ffmpeg_stderr_is_client_disconnect(
            "Error opening input files: Server returned 5XX Server Error reply"
        ));
    }

    #[tokio::test]
    async fn mpegts_script_discovery_reads_ersatztv_folder_shape() {
        let root = tempfile::tempdir().expect("temp script root");
        let folder = root.path().join("custom");
        std::fs::create_dir_all(&folder).expect("create script folder");
        std::fs::write(
            folder.join("mpegts.yml"),
            "name: Custom\nlinux_script: run.sh\nwindows_script: run.bat\n",
        )
        .expect("write script yaml");

        let script = find_mpegts_script_in(root.path(), "custom")
            .await
            .expect("script discovered");

        assert_eq!(script.id, "custom");
        assert_eq!(script.definition.name, "Custom");
        assert_eq!(script.definition.linux_script.as_deref(), Some("run.sh"));
    }

    #[tokio::test]
    async fn mpegts_script_command_writes_rendered_platform_script() {
        let root = tempfile::tempdir().expect("temp script root");
        let folder = root.path().join("default");
        std::fs::create_dir_all(&folder).expect("create script folder");
        let script_name = if cfg!(windows) { "run.bat" } else { "run.sh" };
        std::fs::write(
            folder.join(script_name),
            r#""{{ FFmpegPath }}" -i "{{ HlsUrl }}" -metadata service_name="{{ ChannelName }}""#,
        )
        .expect("write script template");
        let script = MpegTsScript {
            id: "default".to_owned(),
            folder,
            definition: MpegTsScriptDefinition {
                name: "Default".to_owned(),
                linux_script: Some("run.sh".to_owned()),
                windows_script: Some("run.bat".to_owned()),
            },
        };

        let (command, cleanup_paths) = render_mpegts_script_command(
            &script,
            &MpegTsScriptRenderData {
                hls_url: "http://localhost:8409/iptv/channel/1.m3u8?mode=segmenter".to_owned(),
                channel_name: "Channel One".to_owned(),
                ffmpeg_path: "/opt/ffmpeg/bin/ffmpeg".to_owned(),
            },
        )
        .await
        .expect("render command");

        let rendered_path = cleanup_paths.first().expect("cleanup path");
        let rendered = std::fs::read_to_string(rendered_path).expect("read rendered script");
        assert!(rendered.contains("/opt/ffmpeg/bin/ffmpeg"));
        assert!(rendered.contains("Channel One"));
        if cfg!(windows) {
            assert_eq!(
                command.executable,
                rendered_path.to_string_lossy().into_owned()
            );
            assert!(command.arguments.is_empty());
        } else {
            assert_eq!(command.executable, "bash");
            assert_eq!(
                command.arguments,
                vec![rendered_path.to_string_lossy().into_owned()]
            );
        }
        cleanup_stream_artifact(rendered_path).await;
    }

    #[test]
    fn legacy_concat_url_matches_internal_endpoint_shape() {
        assert_eq!(
            build_legacy_concat_url("http", "localhost:8409", "7"),
            "http://localhost:8409/ffmpeg/concat/7?mode=ts-legacy"
        );
    }

    #[test]
    fn detect_platform_picks_windows_for_drive_letters() {
        assert_eq!(detect_platform(r"D:\Movies"), PathPlatform::Windows);
        assert_eq!(detect_platform("/media/movies"), PathPlatform::Unix);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ignore = "requires ffmpeg on PATH and the Big Buck Bunny fixture under testdata/"]
    async fn stream_body_emits_non_empty_bytes_from_fixture() {
        use tokio::io::AsyncReadExt;
        use tokio::time::timeout;

        let Some(fixture) = TESTDATA_CANDIDATES
            .iter()
            .map(PathBuf::from)
            .find(|p| p.is_file())
        else {
            eprintln!("skipping: no Big Buck Bunny fixture found in testdata/");
            return;
        };
        if tokio::process::Command::new(ersatzrs_ffmpeg::ffmpeg_path())
            .arg("-version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await
            .is_err()
        {
            eprintln!("skipping: ffmpeg not on PATH");
            return;
        }

        let command = build_stream_command(
            fixture.to_string_lossy().as_ref(),
            OutputFormat::MpegTs,
            0,
            &playback_profile(),
            None,
            StreamingMode::TransportStream,
            OutputMetadata::default(),
            FfmpegCapabilityReport::default(),
            None,
            Vec::new(),
            None,
            Vec::new(),
            Vec::new(),
        );
        let mut process = Command::new(&command.executable);
        process
            .args(&command.arguments)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        let mut child = process.spawn().expect("spawn ffmpeg");
        let mut stdout = child.stdout.take().expect("stdout pipe");

        let mut buffer = vec![0u8; 64 * 1024];
        let read = timeout(Duration::from_secs(10), stdout.read(&mut buffer))
            .await
            .expect("timed out waiting for ffmpeg output")
            .expect("read ffmpeg stdout");
        assert!(read > 0, "expected some bytes from ffmpeg");

        child.kill().await.ok();
        let _ = child.wait().await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ignore = "requires ffmpeg on PATH and the Big Buck Bunny fixture under testdata/"]
    async fn stream_body_composites_watermark_from_fixture() {
        use tokio::io::AsyncReadExt;
        use tokio::time::timeout;

        let Some(fixture) = TESTDATA_CANDIDATES
            .iter()
            .map(PathBuf::from)
            .find(|p| p.is_file())
        else {
            eprintln!("skipping: no Big Buck Bunny fixture found in testdata/");
            return;
        };
        if tokio::process::Command::new(ersatzrs_ffmpeg::ffmpeg_path())
            .arg("-version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await
            .is_err()
        {
            eprintln!("skipping: ffmpeg not on PATH");
            return;
        }

        let temp = tempfile::tempdir().expect("tempdir");
        let logo = temp.path().join("logo.ppm");
        std::fs::write(&logo, red_ppm(64, 32)).expect("write watermark fixture");

        let command = build_stream_command(
            fixture.to_string_lossy().as_ref(),
            OutputFormat::MpegTs,
            0,
            &playback_profile(),
            None,
            StreamingMode::TransportStream,
            OutputMetadata::default(),
            FfmpegCapabilityReport::default(),
            None,
            Vec::new(),
            None,
            vec![(
                InputFile::watermark(logo.to_string_lossy().into_owned()),
                WatermarkOverlay {
                    location: WatermarkLocation::TopLeft,
                    size: WatermarkSize::Scaled,
                    width_percent: 10.0,
                    horizontal_margin_percent: 0.0,
                    vertical_margin_percent: 0.0,
                    opacity: 100,
                    ..WatermarkOverlay::default()
                },
            )],
            Vec::new(),
        );
        let mut process = Command::new(&command.executable);
        process
            .args(&command.arguments)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        for variable in command.environment {
            process.env(variable.name, variable.value);
        }
        let mut child = process.spawn().expect("spawn ffmpeg");
        let mut stdout = child.stdout.take().expect("stdout pipe");

        let mut buffer = vec![0u8; 2 * 1024 * 1024];
        let read = timeout(Duration::from_secs(20), stdout.read(&mut buffer))
            .await
            .expect("timed out waiting for ffmpeg output")
            .expect("read ffmpeg stdout");
        assert!(read > 0, "expected some bytes from ffmpeg");
        child.kill().await.ok();
        let _ = child.wait().await;

        let stream = temp.path().join("watermarked.ts");
        std::fs::write(&stream, &buffer[..read]).expect("write mpegts sample");

        let output = tokio::process::Command::new(ersatzrs_ffmpeg::ffmpeg_path())
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-i",
                stream.to_string_lossy().as_ref(),
                "-vf",
                "crop=160:80:0:0,scale=1:1,format=rgb24",
                "-frames:v",
                "1",
                "-f",
                "rawvideo",
                "pipe:1",
            ])
            .output()
            .await
            .expect("decode watermarked frame");
        assert!(
            output.status.success(),
            "ffmpeg decode failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout.len(), 3);
        assert!(
            output.stdout[0] > 180 && output.stdout[1] < 80 && output.stdout[2] < 80,
            "expected red watermark crop, got rgb={:?}",
            output.stdout
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ignore = "requires ffmpeg with drawvg on PATH"]
    async fn stream_body_composites_drawvg_graphics_from_generated_input() {
        let ffmpeg = ersatzrs_ffmpeg::ffmpeg_path();
        if !ffmpeg_has_drawvg(&ffmpeg).await {
            eprintln!("skipping: ffmpeg drawvg filter not available");
            return;
        }

        let temp = tempfile::tempdir().expect("tempdir");
        let input = temp.path().join("black.mp4");
        let script = temp.path().join("green.vgs");
        std::fs::write(&script, green_drawvg_script()).expect("write graphics script");
        generate_graphics_input(&ffmpeg, &input).await;

        let output = run_iptv_graphics_stream(&input, &script).await;
        assert!(!output.is_empty(), "expected MPEG-TS bytes");

        let stream = temp.path().join("graphics.ts");
        std::fs::write(&stream, &output).expect("write mpegts sample");
        assert_stream_has_green_graphics(&ffmpeg, &stream).await;
    }

    async fn ffmpeg_has_drawvg(ffmpeg: &str) -> bool {
        tokio::process::Command::new(ffmpeg)
            .args(["-hide_banner", "-filters"])
            .output()
            .await
            .is_ok_and(|output| String::from_utf8_lossy(&output.stdout).contains("drawvg"))
    }

    async fn generate_graphics_input(ffmpeg: &str, input: &Path) {
        let generated = tokio::process::Command::new(ffmpeg)
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "lavfi",
                "-i",
                "color=c=black:s=320x180:r=25:d=1",
                "-f",
                "lavfi",
                "-i",
                "anullsrc=channel_layout=stereo:sample_rate=48000",
                "-t",
                "1",
                "-shortest",
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
                "-c:a",
                "aac",
                input.to_string_lossy().as_ref(),
            ])
            .output()
            .await
            .expect("generate input video");
        assert!(
            generated.status.success(),
            "ffmpeg input generation failed: {}",
            String::from_utf8_lossy(&generated.stderr)
        );
    }

    async fn run_iptv_graphics_stream(input: &Path, script: &Path) -> Vec<u8> {
        let command = build_stream_command(
            input.to_string_lossy().as_ref(),
            OutputFormat::MpegTs,
            0,
            &playback_profile(),
            None,
            StreamingMode::TransportStream,
            OutputMetadata::default(),
            FfmpegCapabilityReport::default(),
            None,
            Vec::new(),
            None,
            Vec::new(),
            vec![GraphicsPipelineOverlay::drawvg(
                0,
                format!("drawvg=file={}", script.to_string_lossy()),
            )],
        );
        let mut process = Command::new(&command.executable);
        process
            .args(&command.arguments)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        for variable in command.environment {
            process.env(variable.name, variable.value);
        }
        let output = process.output().await.expect("run graphics stream");
        assert!(
            output.status.success(),
            "ffmpeg stream failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        output.stdout
    }

    async fn assert_stream_has_green_graphics(ffmpeg: &str, stream: &Path) {
        let decoded = tokio::process::Command::new(ffmpeg)
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-i",
                stream.to_string_lossy().as_ref(),
                "-vf",
                "crop=80:80:0:0,scale=1:1,format=rgb24",
                "-frames:v",
                "1",
                "-f",
                "rawvideo",
                "pipe:1",
            ])
            .output()
            .await
            .expect("decode graphics frame");
        assert!(
            decoded.status.success(),
            "ffmpeg decode failed: {}",
            String::from_utf8_lossy(&decoded.stderr)
        );
        assert_eq!(decoded.stdout.len(), 3);
        assert!(
            decoded.stdout[1] > 180 && decoded.stdout[0] < 80 && decoded.stdout[2] < 80,
            "expected green graphics crop, got rgb={:?}",
            decoded.stdout
        );
    }

    fn red_ppm(width: usize, height: usize) -> Vec<u8> {
        let mut ppm = format!("P6\n{width} {height}\n255\n").into_bytes();
        ppm.extend(std::iter::repeat_n([255, 0, 0], width * height).flatten());
        ppm
    }

    fn green_drawvg_script() -> &'static str {
        "rect 0 0 480 270 setcolor #00ff00 fill\n"
    }

    fn playback_profile() -> PlaybackProfile {
        PlaybackProfile {
            profile: FFmpegProfile {
                video_format: FFmpegProfileVideoFormat::H264,
                audio_format: FFmpegProfileAudioFormat::Aac,
                video_profile: Some("high".to_owned()),
                video_preset: Some("veryfast".to_owned()),
                video_bitrate: 8_000,
                video_buffer_size: 16_000,
                audio_bitrate: 192,
                audio_buffer_size: 384,
                audio_channels: 2,
                audio_sample_rate: 48,
                ..FFmpegProfile::default()
            },
            resolution: Resolution {
                width: 1920,
                height: 1080,
                ..Resolution::default()
            },
        }
    }
}
