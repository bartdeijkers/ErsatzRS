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
use chrono::{DateTime, NaiveDateTime, Timelike, Utc};
use ersatzrs_core::hls::{HlsSessionHandle, HlsSessionRegistry};
use ersatzrs_core::path_replacement::{replace_path, PathPlatform, PathReplacement};
use ersatzrs_core::repositories::{
    ChannelRepository, ConfigRepository, MediaSourceRepository, RepositoryError,
};
use ersatzrs_domain::{
    Channel, ChannelStreamSelectorMode, ChannelSubtitleMode, EmbyMediaSource,
    FFmpegProfileAudioFormat, FFmpegProfileVideoFormat, JellyfinMediaSource, PlexMediaSource,
    StreamingMode,
};
use ersatzrs_ffmpeg::{
    build_mpegts_concat_command, build_mpegts_segmenter_wrapper_command,
    build_pipeline_for_acceleration, ConcatPlaylist, DesiredVideoState, FfmpegCapabilityReport,
    FfmpegCommand, FrameRate, FrameSize, GraphicsPipelineOverlay, InputFile, MpegTsMuxingOptions,
    OutputFormat, OutputMetadata, PipelineRequest, SourceAudioCodec, SourceAudioStream,
    SourceVideoCodec, SourceVideoStream, WatermarkOpacityClock, MPEGTS_PCR_PERIOD_MS,
};
use ersatzrs_infra::db::DbPool;
use ersatzrs_infra::ffmpeg_capabilities::{
    learned_hardware_decode_denial, persist_hardware_decode_denial,
};
use ersatzrs_infra::ffmpeg_config::playback_language_preferences;
use ersatzrs_infra::file_system_layout;
use ersatzrs_infra::hls_worker::{
    build_mpeg_ts_slate_command, MpegTsSlateCommandRequest, ALL_STREAMS_IN_USE_SLATE_MESSAGE,
};
use ersatzrs_infra::playback::{
    enrich_source_video_stream_for_playback, load_channel_playback_profile,
    load_channel_target_frame_rate, load_source_audio_streams, load_source_video_stream,
    render_playback_graphics_overlays, select_channel_playback_subtitle,
    select_custom_playback_streams, select_playback_audio_stream_index, select_playback_graphics,
    select_playback_watermarks, subtitle_fonts_dir, subtitle_input_for_stream,
    watermark_inputs_for_stream, CustomStreamSelection, PlaybackGraphicsRenderContext,
    PlaybackProfile, PlaybackSubtitle,
};
use ersatzrs_infra::subtitles::rebase_burned_text_subtitle_input;
use ersatzrs_infra::transcode_capacity::{
    load_active_transcode_limit, request_active_transcode_reclaim, try_reserve_active_transcode,
    ActiveTranscodeReservation, ActiveTranscodeSlot, DEFAULT_ACTIVE_TRANSCODE_LIMIT,
};
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
const MPEG_TS_CAPACITY_SLATE_DURATION: Duration = Duration::from_secs(4);
const MPEG_TS_CAPACITY_RETRY_INTERVAL: Duration = Duration::from_millis(250);
const MPEG_TS_HLS_RECLAIM_WATCH_WINDOW: Duration = Duration::from_secs(30);
const MPEG_TS_HLS_STALE_WATCHED_PREEMPTION_THRESHOLD: Duration = Duration::from_secs(5);
const MPEG_TS_HLS_RECLAIM_GRACE: Duration = Duration::from_secs(6);
const MPEG_TS_HLS_RECLAIMED_REACQUIRE_DELAY: Duration = Duration::from_millis(500);

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
    muxrate_bps: i64,
    pcr_period_ms: u32,
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
        TransportStreamMode::Legacy => serve_legacy_concat_stream(state, &channel, headers).await,
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

    let replacements =
        match load_required_path_replacements(state, &channel, "MPEG-TS stream").await {
            Ok(replacements) => replacements,
            Err(response) => return response,
        };

    let resolved = if replacements.is_empty() {
        active.media_path.clone()
    } else {
        replace_path(&replacements, &active.media_path, PathPlatform::Unix)
    };

    let source_stream = match load_source_video_stream(&state.pool, active.media_version_id).await {
        Ok(stream) => stream,
        Err(error) => {
            tracing::error!(
                %error,
                channel_id = channel.id,
                channel_number = %channel.number,
                media_version_id = active.media_version_id,
                "failed to load source video metadata; refusing to build FFmpeg command"
            );
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    let source_audio_streams =
        match load_source_audio_streams(&state.pool, active.media_version_id).await {
            Ok(streams) => streams,
            Err(error) => {
                tracing::error!(
                    %error,
                    channel_id = channel.id,
                    channel_number = %channel.number,
                    media_version_id = active.media_version_id,
                    "failed to load source audio metadata; refusing to build FFmpeg command"
                );
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        };
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
                tracing::error!(
                    %error,
                    channel_id = channel.id,
                    channel_number = %channel.number,
                    "failed to load channel target frame rate; refusing to build FFmpeg command"
                );
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        }
    } else {
        None
    };
    let runtime_capabilities = state.ffmpeg_runtime.read().await.capabilities.clone();
    let language_preferences = match playback_language_preferences(&state.pool).await {
        Ok(preferences) => preferences,
        Err(error) => {
            tracing::error!(
                %error,
                channel_id = channel.id,
                channel_number = %channel.number,
                "failed to load playback language preferences; refusing to build FFmpeg command"
            );
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    let subtitle_mode = active.subtitle_mode.unwrap_or(channel.subtitle_mode);
    let preferred_subtitle_languages = language_preferences.subtitle_languages(
        active.preferred_subtitle_language_code.as_deref(),
        channel.preferred_subtitle_language_code.as_deref(),
    );
    let preferred_audio_title = active
        .preferred_audio_title
        .as_deref()
        .or(channel.preferred_audio_title.as_deref());
    let preferred_audio_language = language_preferences.audio_language(
        active.preferred_audio_language_code.as_deref(),
        channel.preferred_audio_language_code.as_deref(),
        preferred_audio_title,
        effective_streaming_mode,
    );
    let start_position = active.content_seek_position(now);
    let start_seconds = duration_as_i64_seconds(start_position);
    let content_start_time = parse_sqlite_utc_timestamp(&active.playout_start).unwrap_or(now);
    let custom_selection = match select_channel_custom_streams(
        &state.pool,
        active.media_version_id,
        active.media_item_id,
        &channel,
        content_start_time,
        effective_streaming_mode,
    )
    .await
    {
        Ok(selection) => selection,
        Err(error) => {
            tracing::error!(
                %error,
                channel_id = channel.id,
                channel_number = %channel.number,
                media_item_id = active.media_item_id,
                media_version_id = active.media_version_id,
                "failed to execute channel stream selector; refusing fallback stream selection"
            );
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    let selected_audio_stream_index = custom_selection
        .as_ref()
        .and_then(|selection| selection.audio_stream_index)
        .or_else(|| {
            select_playback_audio_stream_index(
                &source_audio_streams,
                preferred_audio_language.as_deref(),
                preferred_audio_title,
            )
        });
    let subtitle = if let Some(selection) = custom_selection {
        selection.subtitle
    } else {
        match select_channel_playback_subtitle(
            &state.pool,
            active.media_item_id,
            &preferred_subtitle_languages,
            subtitle_mode,
            effective_streaming_mode,
            &channel,
            &playback_profile.resolution,
            start_seconds,
        )
        .await
        {
            Ok(subtitle) => subtitle,
            Err(error) => {
                tracing::error!(
                    %error,
                    channel_id = channel.id,
                    channel_number = %channel.number,
                    media_item_id = active.media_item_id,
                    "failed to select playback subtitle; refusing subtitle-free fallback"
                );
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        }
    };
    let metadata = playback_output_metadata(
        Some("ErsatzTV"),
        Some(&channel.name),
        preferred_audio_language.as_deref(),
        &source_audio_streams,
        subtitle.as_ref(),
        selected_audio_stream_index,
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
                    stream_seek_seconds: start_position.as_secs_f64(),
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
    let channel_start_time = parse_sqlite_utc_timestamp(&active.playout_start).unwrap_or(now);
    let stream_duration = if effective_streaming_mode == StreamingMode::TransportStream {
        active.remaining_duration(now)
    } else {
        None
    };
    let opacity_clock = watermark_opacity_clock_at_spawn(
        now,
        start_position,
        active.content_duration,
        channel_start_time,
    );
    let command = build_stream_command_with_opacity_clock(
        &resolved,
        output_format,
        start_position,
        stream_duration,
        &playback_profile,
        target_frame_rate,
        effective_streaming_mode,
        metadata.clone(),
        runtime_capabilities.clone(),
        source_stream.clone(),
        source_audio_streams.clone(),
        selected_audio_stream_index,
        subtitle_input.clone(),
        watermark_inputs.clone(),
        graphics_overlays.clone(),
        Some(opacity_clock),
    );
    let slate_metadata = metadata.clone();
    let slate_runtime_capabilities = runtime_capabilities.clone();
    let retry_playback_profile = playback_profile.clone();
    let retry = StreamDenialRetry {
        pool: state.pool.clone(),
        runtime_capabilities,
        acceleration: effective_acceleration,
        source_stream: source_stream.clone(),
        rebuild: {
            let resolved = resolved.clone();
            let channel_number = channel.number.clone();
            Box::new(move |mode| {
                let (capabilities, force_software_encode) = match mode {
                    StreamRetryMode::HardwareDecodeDenied(capabilities) => (capabilities, false),
                    StreamRetryMode::HardwareEncodeDenied(capabilities) => (capabilities, true),
                };
                let mut playback_profile = retry_playback_profile;
                if force_software_encode {
                    force_software_h264_encoding(&mut playback_profile);
                }
                tracing::debug!(
                    channel_number = %channel_number,
                    force_software_encode,
                    "rebuilding channel stream command after runtime hardware rejection"
                );
                build_stream_command_with_opacity_clock(
                    &resolved,
                    output_format,
                    start_position,
                    stream_duration,
                    &playback_profile,
                    target_frame_rate,
                    effective_streaming_mode,
                    metadata,
                    capabilities,
                    source_stream,
                    source_audio_streams,
                    selected_audio_stream_index,
                    subtitle_input,
                    watermark_inputs,
                    graphics_overlays,
                    Some(opacity_clock),
                )
            })
        },
    };
    let capacity_reservation =
        if effective_streaming_mode == ersatzrs_domain::StreamingMode::TransportStream {
            match reserve_mpeg_ts_capacity(state, &channel.number).await {
                Ok(Some(reservation)) => Some(reservation),
                Ok(None) => {
                    return mpeg_ts_capacity_wait_response(
                        state,
                        &channel,
                        command,
                        graphics_cleanup_dirs,
                        &playback_profile,
                        target_frame_rate,
                        slate_metadata,
                        slate_runtime_capabilities,
                        Some(retry),
                    )
                    .await;
                }
                Err(error) => {
                    tracing::error!(
                        %error,
                        channel_id = channel.id,
                        channel_number = %channel.number,
                        "failed to load MPEG-TS transcode capacity"
                    );
                    return StatusCode::INTERNAL_SERVER_ERROR.into_response();
                }
            }
        } else {
            None
        };

    match spawn_stream_body(
        &command,
        graphics_cleanup_dirs,
        channel.number.clone(),
        state.hls_sessions.clone(),
        capacity_reservation,
        Some(retry),
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
            StreamingMode::TransportStream | StreamingMode::TransportStreamHybrid => {
                TransportStreamMode::Legacy
            }
            StreamingMode::HttpLiveStreamingDirect | StreamingMode::HttpLiveStreamingSegmenter => {
                TransportStreamMode::RedirectToHls
            }
        },
        "ts" | "mpegts" | "mpeg-ts" | "ts-legacy" => TransportStreamMode::Legacy,
        _ => TransportStreamMode::HybridWrapper,
    }
}

#[allow(clippy::too_many_lines)]
async fn serve_segmenter_wrapper_stream(
    state: &AppState,
    channel: &Channel,
    headers: &HeaderMap,
    query: &HlsQuery,
) -> Response {
    let path_replacements =
        match load_required_path_replacements(state, channel, "wrapped segmenter").await {
            Ok(replacements) => replacements,
            Err(response) => return response,
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

    let initial_segment_count = hls::load_initial_segment_count(state).await;
    let startup_timeout = hls::load_segmenter_startup_timeout(state).await;
    let wait_result = hls::wait_for_initial_segments(
        &*state.hls_sessions,
        &channel.number,
        initial_segment_count,
        startup_timeout,
    )
    .await;
    if let Some(response) = segmenter_wrapper_wait_failure_response(
        wait_result,
        &channel.number,
        initial_segment_count,
        startup_timeout,
    ) {
        return response;
    }

    let (playback_profile, target_frame_rate, runtime_capabilities) =
        match load_capacity_slate_context(state, channel, "wrapped segmenter").await {
            Ok(context) => context,
            Err(response) => return response,
        };
    let aac_latm_audio = playback_profile.profile.audio_format == FFmpegProfileAudioFormat::AacLatm;
    let access_token = AccessToken::new(query.access_token.clone());
    let hls_url = build_segmenter_wrapper_hls_url(
        &hls::request_scheme(headers),
        &hls::request_host(headers),
        &channel.number,
        &access_token,
    );
    let metadata = channel_stream_metadata(channel);
    let wrapper_muxrate_bps = mpeg_ts_wrapper_muxrate_bps(&playback_profile);
    let (command, cleanup_paths) = segmenter_wrapper_command(
        state,
        channel,
        hls_url,
        &metadata,
        aac_latm_audio,
        wrapper_muxrate_bps,
    )
    .await;

    let capacity_reservation = match reserve_mpeg_ts_capacity(state, &channel.number).await {
        Ok(Some(reservation)) => Some(reservation),
        Ok(None) => {
            return mpeg_ts_capacity_wait_response(
                state,
                channel,
                command,
                cleanup_paths,
                &playback_profile,
                target_frame_rate,
                metadata,
                runtime_capabilities,
                None,
            )
            .await;
        }
        Err(error) => {
            tracing::error!(
                %error,
                channel_id = channel.id,
                channel_number = %channel.number,
                "failed to load wrapped-segmenter transcode capacity"
            );
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    match spawn_stream_body(
        &command,
        cleanup_paths,
        channel.number.clone(),
        state.hls_sessions.clone(),
        capacity_reservation,
        None,
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

async fn segmenter_wrapper_command(
    state: &AppState,
    channel: &Channel,
    hls_url: String,
    metadata: &OutputMetadata,
    aac_latm_audio: bool,
    wrapper_muxrate_bps: i64,
) -> (FfmpegCommand, Vec<PathBuf>) {
    match configured_mpegts_script_command(state, channel, &hls_url, wrapper_muxrate_bps).await {
        Some(command) => command,
        None => (
            build_mpegts_segmenter_wrapper_command(
                hls_url,
                metadata,
                aac_latm_audio,
                Some(wrapper_muxrate_bps),
            ),
            Vec::new(),
        ),
    }
}

fn segmenter_wrapper_wait_failure_response(
    wait_result: hls::InitialSegmentWaitResult,
    channel_number: &str,
    initial_segment_count: usize,
    startup_timeout: Duration,
) -> Option<Response> {
    match wait_result {
        hls::InitialSegmentWaitResult::Ready => None,
        hls::InitialSegmentWaitResult::AtCapacity => {
            tracing::warn!(
                channel_number,
                initial_segment_count,
                startup_timeout_seconds = startup_timeout.as_secs(),
                "hls segmenter admission denied before wrapper startup because all transcode slots are in use"
            );
            Some(
                (
                    StatusCode::SERVICE_UNAVAILABLE,
                    hls::HLS_AT_CAPACITY_MESSAGE,
                )
                    .into_response(),
            )
        }
        hls::InitialSegmentWaitResult::MissingSession | hls::InitialSegmentWaitResult::Timeout => {
            tracing::warn!(
                channel_number,
                initial_segment_count,
                startup_timeout_seconds = startup_timeout.as_secs(),
                "hls segmenter did not produce initial playlist segments before wrapper startup timeout"
            );
            Some((StatusCode::SERVICE_UNAVAILABLE, "no segments yet").into_response())
        }
    }
}

async fn serve_legacy_concat_stream(
    state: &AppState,
    channel: &Channel,
    headers: &HeaderMap,
) -> Response {
    let concat_url = build_legacy_concat_url(
        &hls::request_scheme(headers),
        &hls::request_host(headers),
        &channel.number,
    );
    let metadata = channel_stream_metadata(channel);
    let (playback_profile, target_frame_rate, runtime_capabilities) =
        match load_capacity_slate_context(state, channel, "legacy stream").await {
            Ok(context) => context,
            Err(response) => return response,
        };
    let wrapper_muxrate_bps = mpeg_ts_wrapper_muxrate_bps(&playback_profile);
    let command = build_mpegts_concat_command(concat_url, &metadata, Some(wrapper_muxrate_bps));

    let capacity_reservation = match reserve_mpeg_ts_capacity(state, &channel.number).await {
        Ok(Some(reservation)) => Some(reservation),
        Ok(None) => {
            return mpeg_ts_capacity_wait_response(
                state,
                channel,
                command,
                Vec::new(),
                &playback_profile,
                target_frame_rate,
                metadata,
                runtime_capabilities,
                None,
            )
            .await;
        }
        Err(error) => {
            tracing::error!(
                %error,
                channel_id = channel.id,
                channel_number = %channel.number,
                "failed to load legacy-stream transcode capacity"
            );
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    match spawn_stream_body(
        &command,
        Vec::new(),
        channel.number.clone(),
        state.hls_sessions.clone(),
        capacity_reservation,
        None,
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

fn channel_stream_metadata(channel: &Channel) -> OutputMetadata {
    OutputMetadata {
        service_provider: Some("ErsatzTV".to_owned()),
        service_name: Some(channel.name.clone()),
        ..OutputMetadata::default()
    }
}

async fn reserve_mpeg_ts_capacity(
    state: &AppState,
    channel_number: &str,
) -> Result<Option<ActiveTranscodeReservation>, RepositoryError> {
    let limit = load_active_transcode_limit(&state.pool)
        .await?
        .unwrap_or(DEFAULT_ACTIVE_TRANSCODE_LIMIT);
    let reservation =
        try_reserve_active_transcode(limit, ActiveTranscodeSlot::new_mpeg_ts(channel_number));
    if reservation.is_some() {
        return Ok(reservation);
    }

    let reclaim_deadline = tokio::time::Instant::now() + MPEG_TS_HLS_RECLAIM_GRACE;
    loop {
        if request_hls_capacity_reclaim_for_mpeg_ts(channel_number) {
            tokio::time::sleep(MPEG_TS_CAPACITY_RETRY_INTERVAL).await;
            let reservation = try_reserve_active_transcode(
                limit,
                ActiveTranscodeSlot::new_mpeg_ts(channel_number),
            );
            if reservation.is_some() {
                return Ok(reservation);
            }
        }

        if tokio::time::Instant::now() >= reclaim_deadline {
            break;
        }
        tokio::time::sleep(MPEG_TS_CAPACITY_RETRY_INTERVAL).await;
        let reservation =
            try_reserve_active_transcode(limit, ActiveTranscodeSlot::new_mpeg_ts(channel_number));
        if reservation.is_some() {
            return Ok(reservation);
        }
    }

    if reservation.is_none() {
        tracing::warn!(
            channel_number,
            limit,
            "mpeg-ts stream admission waiting on all-streams-in-use slate because all transcode slots are in use"
        );
    }
    Ok(reservation)
}

fn request_hls_capacity_reclaim_for_mpeg_ts(channel_number: &str) -> bool {
    let Some(reclaim) = request_active_transcode_reclaim(
        channel_number,
        MPEG_TS_HLS_RECLAIM_WATCH_WINDOW,
        MPEG_TS_HLS_STALE_WATCHED_PREEMPTION_THRESHOLD,
        MPEG_TS_HLS_RECLAIMED_REACQUIRE_DELAY,
    ) else {
        return false;
    };

    tracing::info!(
        channel_number,
        target_channel = %reclaim.channel_number,
        target_slot_id = reclaim.slot_id,
        reason = ?reclaim.reason,
        "requesting HLS transcode slot reclaim for MPEG-TS stream admission"
    );
    reclaim.notify.notify_one();
    true
}

#[allow(clippy::too_many_arguments)]
async fn mpeg_ts_capacity_wait_response(
    state: &AppState,
    channel: &Channel,
    command: FfmpegCommand,
    mut cleanup_paths: Vec<PathBuf>,
    playback_profile: &PlaybackProfile,
    target_frame_rate: Option<FrameRate>,
    metadata: OutputMetadata,
    runtime_capabilities: FfmpegCapabilityReport,
    retry: Option<StreamDenialRetry>,
) -> Response {
    let slate_dir = mpeg_ts_capacity_slate_dir(channel.id);
    let Some(slate_command) = build_mpeg_ts_slate_command(MpegTsSlateCommandRequest {
        working_dir: &slate_dir,
        duration: MPEG_TS_CAPACITY_SLATE_DURATION,
        playback_profile,
        target_frame_rate,
        metadata,
        runtime_capabilities,
        message: ALL_STREAMS_IN_USE_SLATE_MESSAGE,
    })
    .await
    else {
        tracing::warn!(
            channel_number = %channel.number,
            "failed to build MPEG-TS all-streams-in-use slate"
        );
        for path in cleanup_paths {
            cleanup_stream_artifact(&path).await;
        }
        cleanup_stream_artifact(&slate_dir).await;
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            hls::HLS_AT_CAPACITY_MESSAGE,
        )
            .into_response();
    };
    cleanup_paths.push(slate_dir);

    let body = spawn_stream_body_waiting_for_mpegts_capacity(
        command,
        slate_command,
        cleanup_paths,
        channel.number.clone(),
        state.hls_sessions.clone(),
        state.pool.clone(),
        retry,
    );
    let mut response = body.into_response();
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static("video/mp2t"));
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

async fn load_capacity_slate_context(
    state: &AppState,
    channel: &Channel,
    context: &str,
) -> Result<(PlaybackProfile, Option<FrameRate>, FfmpegCapabilityReport), Response> {
    let playback_profile = match load_channel_playback_profile(&state.pool, channel.id).await {
        Ok(Some(profile)) => profile,
        Ok(None) => {
            tracing::error!(channel_number = %channel.number, context, "channel has no playback ffmpeg profile");
            return Err(StatusCode::INTERNAL_SERVER_ERROR.into_response());
        }
        Err(error) => {
            tracing::error!(%error, channel_number = %channel.number, context, "failed to load playback ffmpeg profile");
            return Err(StatusCode::INTERNAL_SERVER_ERROR.into_response());
        }
    };
    let target_frame_rate =
        load_capacity_slate_target_frame_rate(state, channel, &playback_profile).await?;
    let runtime_capabilities = state.ffmpeg_runtime.read().await.capabilities.clone();

    Ok((playback_profile, target_frame_rate, runtime_capabilities))
}

async fn load_required_path_replacements(
    state: &AppState,
    channel: &Channel,
    context: &str,
) -> Result<Vec<PathReplacement>, Response> {
    load_path_replacements(&state.repositories.media_source)
        .await
        .map_err(|error| {
            tracing::error!(
                %error,
                channel_id = channel.id,
                channel_number = %channel.number,
                context,
                "failed to load path replacements; refusing to select an unverified FFmpeg input"
            );
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        })
}

async fn load_capacity_slate_target_frame_rate(
    state: &AppState,
    channel: &Channel,
    playback_profile: &PlaybackProfile,
) -> Result<Option<FrameRate>, Response> {
    if !playback_profile.profile.normalize_framerate {
        return Ok(None);
    }

    match load_channel_target_frame_rate(&state.pool, channel.id).await {
        Ok(frame_rate) => Ok(frame_rate),
        Err(error) => {
            tracing::error!(
                %error,
                channel_id = channel.id,
                channel_number = %channel.number,
                "failed to load channel target frame rate; refusing to build MPEG-TS capacity slate"
            );
            Err(StatusCode::INTERNAL_SERVER_ERROR.into_response())
        }
    }
}

fn mpeg_ts_capacity_slate_dir(channel_id: i64) -> PathBuf {
    let request_id = NEXT_GRAPHICS_REQUEST_ID.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir()
        .join("ersatzrs-mpegts-capacity-slate")
        .join(format!("channel-{channel_id}"))
        .join(format!("request-{request_id}"))
}

fn mpeg_ts_wrapper_muxrate_bps(playback_profile: &PlaybackProfile) -> i64 {
    let video_kbps = u32::try_from(playback_profile.profile.video_bitrate).ok();
    let audio_kbps = u32::try_from(playback_profile.profile.audio_bitrate).ok();
    MpegTsMuxingOptions::from_bitrates(video_kbps, audio_kbps).muxrate_bps
}

async fn configured_mpegts_script_command(
    state: &AppState,
    channel: &Channel,
    hls_url: &str,
    muxrate_bps: i64,
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
            muxrate_bps,
            pcr_period_ms: MPEGTS_PCR_PERIOD_MS,
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
    let muxrate = data.muxrate_bps.to_string();
    let pcr_period = data.pcr_period_ms.to_string();
    template
        .replace("{{ HlsUrl }}", &data.hls_url)
        .replace("{{HlsUrl}}", &data.hls_url)
        .replace("{{ ChannelName }}", &data.channel_name)
        .replace("{{ChannelName}}", &data.channel_name)
        .replace("{{ FFmpegPath }}", &data.ffmpeg_path)
        .replace("{{FFmpegPath}}", &data.ffmpeg_path)
        .replace("{{ MuxRate }}", &muxrate)
        .replace("{{MuxRate}}", &muxrate)
        .replace("{{ PcrPeriod }}", &pcr_period)
        .replace("{{PcrPeriod}}", &pcr_period)
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
    pub(crate) content_duration: Option<Duration>,
    pub(crate) in_point: Duration,
    pub(crate) out_point: Duration,
    pub(crate) playout_start: String,
    pub(crate) playout_finish: String,
    pub(crate) disable_watermarks: bool,
    pub(crate) preferred_audio_language_code: Option<String>,
    pub(crate) preferred_audio_title: Option<String>,
    pub(crate) preferred_subtitle_language_code: Option<String>,
    pub(crate) subtitle_mode: Option<ChannelSubtitleMode>,
}

impl ActivePlayoutItem {
    fn program_duration(&self) -> Option<std::time::Duration> {
        let start = parse_sqlite_utc_timestamp(&self.playout_start)?;
        let finish = parse_sqlite_utc_timestamp(&self.playout_finish)?;
        (finish - start).to_std().ok()
    }

    fn elapsed_since_start(&self, now: DateTime<Utc>) -> Duration {
        parse_sqlite_utc_timestamp(&self.playout_start)
            .and_then(|start| (now - start).to_std().ok())
            .unwrap_or(Duration::ZERO)
    }

    pub(crate) fn content_seek_position(&self, now: DateTime<Utc>) -> Duration {
        self.in_point.saturating_add(self.elapsed_since_start(now))
    }

    fn remaining_duration(&self, now: DateTime<Utc>) -> Option<Duration> {
        let finish_remaining = parse_sqlite_utc_timestamp(&self.playout_finish)
            .and_then(|finish| (finish - now).to_std().ok());
        let current_content_position = self.in_point.saturating_add(self.elapsed_since_start(now));
        let out_point_remaining = if self.out_point > current_content_position {
            self.out_point.checked_sub(current_content_position)
        } else {
            None
        };

        [finish_remaining, out_point_remaining]
            .into_iter()
            .flatten()
            .filter(|duration| !duration.is_zero())
            .min()
    }
}

pub(crate) async fn load_active_media_path(
    pool: &DbPool,
    channel_id: i64,
    now: DateTime<Utc>,
) -> Result<Option<ActivePlayoutItem>, RepositoryError> {
    let now_text = encode_sql_datetime(now);
    let row = sqlx::query(
        r#"SELECT mf."Path" AS "MediaPath",
                  pi."Id" AS "PlayoutItemId",
                  pi."MediaItemId" AS "MediaItemId",
                  mv."Id" AS "MediaVersionId",
                  mv."Duration" AS "ContentDuration",
                  pi."InPoint" AS "InPoint",
                  pi."OutPoint" AS "OutPoint",
                  pi."Start" AS "PlayoutStart",
                  pi."Finish" AS "PlayoutFinish",
                  pi."DisableWatermarks" AS "DisableWatermarks",
                  pi."PreferredAudioLanguageCode" AS "PreferredAudioLanguageCode",
                  pi."PreferredAudioTitle" AS "PreferredAudioTitle",
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
    let Some(row) = row else {
        return Ok(None);
    };
    let content_duration_text = decode_active_item_column::<String>(&row, "ContentDuration")?;
    let content_duration = parse_duration_text(&content_duration_text)?;
    let in_point_text = decode_active_item_column::<String>(&row, "InPoint")?;
    let out_point_text = decode_active_item_column::<String>(&row, "OutPoint")?;
    let playout_start = decode_active_item_column::<String>(&row, "PlayoutStart")?;
    let playout_finish = decode_active_item_column::<String>(&row, "PlayoutFinish")?;
    if parse_sqlite_utc_timestamp(&playout_start).is_none() {
        return Err(RepositoryError::Decode {
            kind: "active playback item",
            value: format!("PlayoutStart: invalid timestamp {playout_start:?}"),
        });
    }
    if parse_sqlite_utc_timestamp(&playout_finish).is_none() {
        return Err(RepositoryError::Decode {
            kind: "active playback item",
            value: format!("PlayoutFinish: invalid timestamp {playout_finish:?}"),
        });
    }
    let subtitle_mode_value = decode_active_item_column::<Option<i64>>(&row, "SubtitleMode")?;
    let subtitle_mode = subtitle_mode_value
        .map(|value| {
            decode_subtitle_mode(value).ok_or_else(|| RepositoryError::Decode {
                kind: "active playback item",
                value: format!("SubtitleMode: unknown value {value}"),
            })
        })
        .transpose()?;

    Ok(Some(ActivePlayoutItem {
        playout_item_id: decode_active_item_column(&row, "PlayoutItemId")?,
        media_path: decode_active_item_column(&row, "MediaPath")?,
        media_item_id: decode_active_item_column(&row, "MediaItemId")?,
        media_version_id: decode_active_item_column(&row, "MediaVersionId")?,
        content_duration: Some(content_duration),
        in_point: parse_duration_text(&in_point_text)?,
        out_point: parse_duration_text(&out_point_text)?,
        playout_start,
        playout_finish,
        disable_watermarks: decode_active_item_bool(&row, "DisableWatermarks")?,
        preferred_audio_language_code: decode_active_item_column(
            &row,
            "PreferredAudioLanguageCode",
        )?,
        preferred_audio_title: decode_active_item_column(&row, "PreferredAudioTitle")?,
        preferred_subtitle_language_code: decode_active_item_column(
            &row,
            "PreferredSubtitleLanguageCode",
        )?,
        subtitle_mode,
    }))
}

fn decode_active_item_column<T>(
    row: &ersatzrs_infra::db::DbRow,
    column: &'static str,
) -> Result<T, RepositoryError>
where
    for<'row> T: sqlx::Decode<'row, ersatzrs_infra::db::Db> + sqlx::Type<ersatzrs_infra::db::Db>,
{
    row.try_get(column)
        .map_err(|error| RepositoryError::Decode {
            kind: "active playback item",
            value: format!("{column}: {error}"),
        })
}

fn decode_active_item_bool(
    row: &ersatzrs_infra::db::DbRow,
    column: &'static str,
) -> Result<bool, RepositoryError> {
    let value = decode_active_item_column::<i64>(row, column)?;
    match value {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(RepositoryError::Decode {
            kind: "active playback item",
            value: format!("{column}: expected 0 or 1, found {value}"),
        }),
    }
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

async fn select_channel_custom_streams(
    pool: &DbPool,
    media_version_id: i64,
    media_item_id: i64,
    channel: &Channel,
    content_start_time: DateTime<Utc>,
    streaming_mode: StreamingMode,
) -> Result<Option<CustomStreamSelection>, RepositoryError> {
    if channel.stream_selector_mode != ChannelStreamSelectorMode::Custom
        || channel
            .stream_selector
            .as_deref()
            .is_none_or(|selector| selector.trim().is_empty())
    {
        return Ok(None);
    }

    select_custom_playback_streams(
        pool,
        media_version_id,
        media_item_id,
        channel,
        content_start_time,
        streaming_mode,
    )
    .await
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
#[cfg(test)]
fn build_stream_command(
    path: &str,
    output_format: OutputFormat,
    start_position: Duration,
    playback_profile: &PlaybackProfile,
    target_frame_rate: Option<FrameRate>,
    streaming_mode: ersatzrs_domain::StreamingMode,
    metadata: OutputMetadata,
    runtime_capabilities: FfmpegCapabilityReport,
    source_stream: Option<SourceVideoStream>,
    source_audio_streams: Vec<ersatzrs_ffmpeg::SourceAudioStream>,
    selected_audio_stream_index: Option<usize>,
    subtitle_input: Option<InputFile>,
    watermark_inputs: Vec<(InputFile, ersatzrs_ffmpeg::WatermarkOverlay)>,
    graphics_overlays: Vec<GraphicsPipelineOverlay>,
) -> FfmpegCommand {
    build_stream_command_with_opacity_clock(
        path,
        output_format,
        start_position,
        None,
        playback_profile,
        target_frame_rate,
        streaming_mode,
        metadata,
        runtime_capabilities,
        source_stream,
        source_audio_streams,
        selected_audio_stream_index,
        subtitle_input,
        watermark_inputs,
        graphics_overlays,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
fn build_stream_command_with_opacity_clock(
    path: &str,
    output_format: OutputFormat,
    start_position: Duration,
    duration: Option<Duration>,
    playback_profile: &PlaybackProfile,
    target_frame_rate: Option<FrameRate>,
    streaming_mode: ersatzrs_domain::StreamingMode,
    metadata: OutputMetadata,
    runtime_capabilities: FfmpegCapabilityReport,
    source_stream: Option<SourceVideoStream>,
    source_audio_streams: Vec<ersatzrs_ffmpeg::SourceAudioStream>,
    selected_audio_stream_index: Option<usize>,
    subtitle_input: Option<InputFile>,
    watermark_inputs: Vec<(InputFile, ersatzrs_ffmpeg::WatermarkOverlay)>,
    graphics_overlays: Vec<GraphicsPipelineOverlay>,
    opacity_clock: Option<WatermarkOpacityClock>,
) -> FfmpegCommand {
    let input = InputFile::video(path.to_string());
    let needs_hls_direct_mpegts_transcode = hls_direct_mpegts_needs_transcode(
        output_format,
        streaming_mode,
        source_stream.as_ref(),
        &source_audio_streams,
        selected_audio_stream_index,
    );
    let desired_streaming_mode = if needs_hls_direct_mpegts_transcode {
        StreamingMode::TransportStream
    } else {
        streaming_mode
    };
    let mut desired = DesiredVideoState::from_profile(
        &playback_profile.profile,
        &playback_profile.resolution,
        source_stream.as_ref(),
        target_frame_rate,
        desired_streaming_mode,
    );
    if needs_hls_direct_mpegts_transcode {
        if matches!(
            desired.video_format,
            FFmpegProfileVideoFormat::Copy | FFmpegProfileVideoFormat::None
        ) {
            desired.video_format = FFmpegProfileVideoFormat::H264;
        }
        if desired.audio_format == FFmpegProfileAudioFormat::Copy {
            desired.audio_format = FFmpegProfileAudioFormat::Ac3;
        }
        normalize_hls_direct_mpegts_h264_preset(&mut desired);
    }
    let mut request =
        PipelineRequest::new(input, desired).with_runtime_capabilities(runtime_capabilities);
    if let Some(clock) = opacity_clock {
        request = request.with_watermark_opacity_clock(clock);
    }
    if let Some(source_stream) = source_stream {
        request = request.with_source_video_stream(source_stream);
    }
    if let Some(stream_index) = selected_audio_stream_index {
        if let Some(audio_input) = request
            .inputs
            .iter_mut()
            .find(|input| input.kind == ersatzrs_ffmpeg::InputKind::Audio)
        {
            *audio_input = audio_input.clone().with_stream_index(stream_index);
        }
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
    if !start_position.is_zero() {
        request.state.start = Some(start_position);
    }
    request.state.duration = duration;
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

fn hls_direct_mpegts_needs_transcode(
    output_format: OutputFormat,
    streaming_mode: StreamingMode,
    source_stream: Option<&SourceVideoStream>,
    source_audio_streams: &[SourceAudioStream],
    selected_audio_stream_index: Option<usize>,
) -> bool {
    if output_format != OutputFormat::MpegTs
        || streaming_mode != StreamingMode::HttpLiveStreamingDirect
    {
        return false;
    }

    !hls_direct_mpegts_video_copy_safe(source_stream)
        || !hls_direct_mpegts_audio_copy_safe(source_audio_streams, selected_audio_stream_index)
}

fn hls_direct_mpegts_video_copy_safe(source_stream: Option<&SourceVideoStream>) -> bool {
    source_stream.is_some_and(|stream| stream.codec == SourceVideoCodec::H264)
}

fn hls_direct_mpegts_audio_copy_safe(
    source_audio_streams: &[SourceAudioStream],
    selected_audio_stream_index: Option<usize>,
) -> bool {
    let Some(stream) = selected_audio_stream_index
        .and_then(|index| {
            source_audio_streams
                .iter()
                .find(|stream| stream.index == index)
        })
        .or_else(|| source_audio_streams.first())
    else {
        return true;
    };

    matches!(
        stream.codec,
        SourceAudioCodec::Aac | SourceAudioCodec::AacLatm | SourceAudioCodec::Ac3
    )
}

fn normalize_hls_direct_mpegts_h264_preset(desired: &mut DesiredVideoState) {
    if desired.video_format != FFmpegProfileVideoFormat::H264 {
        return;
    }

    let has_x264_preset = desired.video_preset.as_deref().is_some_and(is_x264_preset);
    if !has_x264_preset {
        desired.video_preset = Some("veryfast".to_owned());
    }
}

fn force_software_h264_encoding(playback_profile: &mut PlaybackProfile) {
    playback_profile.profile.hardware_acceleration =
        ersatzrs_domain::HardwareAccelerationKind::None;
    if playback_profile.profile.video_format != FFmpegProfileVideoFormat::H264 {
        return;
    }

    if !playback_profile
        .profile
        .video_preset
        .as_deref()
        .is_some_and(is_x264_preset)
    {
        playback_profile.profile.video_preset = Some("veryfast".to_owned());
    }
}

fn is_x264_preset(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "ultrafast"
            | "superfast"
            | "veryfast"
            | "faster"
            | "fast"
            | "medium"
            | "slow"
            | "slower"
            | "veryslow"
            | "placebo"
    )
}

fn watermark_opacity_clock_at_spawn(
    spawn_time: DateTime<Utc>,
    content_seek: Duration,
    content_duration: Option<Duration>,
    channel_start_time: DateTime<Utc>,
) -> WatermarkOpacityClock {
    let time = spawn_time.time();
    let time_of_day_seconds = f64::from(time.num_seconds_from_midnight())
        + f64::from(time.nanosecond()) / 1_000_000_000.0;
    let channel_seconds = (spawn_time - channel_start_time)
        .to_std()
        .map_or(0.0, |duration| duration.as_secs_f64());
    WatermarkOpacityClock::new(
        time_of_day_seconds,
        content_seek.as_secs_f64(),
        content_duration.map(|duration| duration.as_secs_f64()),
        channel_seconds,
    )
}

pub(crate) fn playback_output_metadata(
    service_provider: Option<&str>,
    service_name: Option<&str>,
    preferred_audio_language: Option<&str>,
    source_audio_streams: &[SourceAudioStream],
    subtitle: Option<&PlaybackSubtitle>,
    selected_audio_stream_index: Option<usize>,
) -> OutputMetadata {
    let audio_language = selected_audio_stream_index
        .and_then(|index| {
            source_audio_streams
                .iter()
                .find(|stream| stream.index == index)
                .and_then(|stream| stream.language.as_deref())
                .and_then(trimmed_string)
        })
        .or_else(|| preferred_audio_language.and_then(trimmed_string))
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

#[cfg(test)]
pub(crate) fn playout_start_duration(now: DateTime<Utc>, playout_start: &str) -> Duration {
    parse_sqlite_utc_timestamp(playout_start)
        .and_then(|start| (now - start).to_std().ok())
        .unwrap_or(Duration::ZERO)
}

fn encode_sql_datetime(value: DateTime<Utc>) -> String {
    let naive = value.naive_utc();
    let base = naive.format("%Y-%m-%d %H:%M:%S").to_string();
    let ticks = naive.and_utc().timestamp_subsec_nanos() / 100;
    if ticks == 0 {
        base
    } else {
        format!("{base}.{ticks:07}")
    }
}

fn parse_sqlite_utc_timestamp(value: &str) -> Option<DateTime<Utc>> {
    for format in ["%Y-%m-%d %H:%M:%S%.f", "%Y-%m-%d %H:%M:%S"] {
        if let Ok(parsed) = NaiveDateTime::parse_from_str(value, format) {
            return Some(DateTime::<Utc>::from_naive_utc_and_offset(parsed, Utc));
        }
    }
    None
}

fn duration_as_i64_seconds(duration: Duration) -> i64 {
    i64::try_from(duration.as_secs()).unwrap_or(i64::MAX)
}

fn parse_duration_text(value: &str) -> Result<Duration, RepositoryError> {
    let decode_error = |message: String| RepositoryError::Decode {
        kind: "active playback item duration",
        value: message,
    };
    let (day_text, time_text) = value.rsplit_once('.').map_or(("", value), |(left, right)| {
        if left.contains(':') {
            ("", value)
        } else {
            (left, right)
        }
    });
    let parts: Vec<&str> = time_text.split(':').collect();
    let [hours_text, minutes_text, seconds_text] = parts.as_slice() else {
        return Err(decode_error(format!("expected HH:MM:SS, found {value:?}")));
    };
    let parse_component = |label: &str, text: &str| {
        text.parse::<u64>()
            .map_err(|error| decode_error(format!("{label}: {error}")))
    };
    let days = if day_text.is_empty() {
        0
    } else {
        parse_component("days", day_text)?
    };
    let hours = parse_component("hours", hours_text)?;
    let minutes = parse_component("minutes", minutes_text)?;
    let (seconds_text, fraction) = match seconds_text.split_once('.') {
        Some((_, "")) => {
            return Err(decode_error(
                "fraction must contain at least one decimal digit".to_owned(),
            ));
        }
        Some((whole, fraction)) => (whole, fraction),
        None => (*seconds_text, ""),
    };
    let seconds = parse_component("seconds", seconds_text)?;
    if minutes >= 60 || seconds >= 60 {
        return Err(decode_error(
            "minutes and seconds must be below 60".to_owned(),
        ));
    }
    let nanos = if fraction.is_empty() {
        0
    } else {
        if fraction.len() > 9 || !fraction.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(decode_error(
                "fraction must contain at most nine decimal digits".to_owned(),
            ));
        }
        let mut padded = fraction.to_owned();
        while padded.len() < 9 {
            padded.push('0');
        }
        padded
            .parse::<u32>()
            .map_err(|error| decode_error(format!("fraction: {error}")))?
    };
    let total_seconds = days
        .checked_mul(86_400)
        .and_then(|value| value.checked_add(hours.checked_mul(3_600)?))
        .and_then(|value| value.checked_add(minutes.checked_mul(60)?))
        .and_then(|value| value.checked_add(seconds))
        .ok_or_else(|| decode_error("duration overflows supported range".to_owned()))?;
    Ok(Duration::new(total_seconds, nanos))
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

/// Context for the ADR 0014 runtime-learned denial loop on direct IPTV
/// spawns: when the first `FFmpeg` process exits with a classified hardware
/// rejection, the supervisor restarts the stream exactly once with the rebuilt
/// command. Decode denials persist a negative capability row; encoder-open
/// failures force software encoding for this stream.
struct StreamDenialRetry {
    pool: DbPool,
    runtime_capabilities: FfmpegCapabilityReport,
    acceleration: ersatzrs_domain::HardwareAccelerationKind,
    source_stream: Option<SourceVideoStream>,
    rebuild: Box<dyn FnOnce(StreamRetryMode) -> FfmpegCommand + Send>,
}

enum StreamRetryMode {
    HardwareDecodeDenied(FfmpegCapabilityReport),
    HardwareEncodeDenied(FfmpegCapabilityReport),
}

const STREAM_BODY_BUFFER_BYTES: usize = 256 * 1024;

/// Ports `HttpContext.Response.RegisterForDispose(process)` from `ErsatzTV`'s
/// `IptvController`: when the HTTP response body is dropped (client
/// disconnected), the `FFmpeg` process is killed immediately. Without this,
/// the supervisor only notices a gone client on the next failed pipe write,
/// so a stalled or slow process lingers and concurrent transcode sessions
/// stack up across player reconnects.
struct StreamBodyDisposeGuard {
    cancel: Arc<Notify>,
}

impl Drop for StreamBodyDisposeGuard {
    fn drop(&mut self) {
        tracing::debug!("channel stream response body dropped; cancelling ffmpeg supervisor");
        // `notify_one` stores a permit, so the supervisor still observes the
        // disposal even if it is between `notified()` awaits (for example
        // while respawning after a learned hardware-decode denial).
        self.cancel.notify_one();
    }
}

enum DirectStreamOutcome {
    Completed,
    Cancelled,
    Failed(String),
}

fn spawn_stream_process(
    command: &FfmpegCommand,
) -> Result<tokio::process::Child, StreamSpawnError> {
    let mut process = Command::new(&command.executable);
    process
        .args(&command.arguments)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    for variable in &command.environment {
        process.env(&variable.name, &variable.value);
    }

    process
        .spawn()
        .map_err(|error| StreamSpawnError::Spawn(error.to_string()))
}

fn spawn_stream_body(
    command: &FfmpegCommand,
    cleanup_paths: Vec<PathBuf>,
    channel_number: String,
    session_registry: Arc<dyn HlsSessionRegistry>,
    capacity_reservation: Option<ActiveTranscodeReservation>,
    retry: Option<StreamDenialRetry>,
) -> Result<Body, StreamSpawnError> {
    let child = spawn_stream_process(command)?;
    if child.stdout.is_none() {
        return Err(StreamSpawnError::MissingStdout);
    }
    let handle = Arc::new(DirectStreamHandle::new(channel_number.clone()));
    let cancel = handle.cancel.clone();
    let handle_cancel = handle.cancel.clone();
    let (reader, mut writer) = tokio::io::duplex(STREAM_BODY_BUFFER_BYTES);

    tokio::spawn(async move {
        let _capacity_reservation = capacity_reservation;
        let registered = session_registry.try_insert(handle).await;
        let mut retry = retry;
        let _ = supervise_stream_command_with_retry(
            child,
            &mut writer,
            &cancel,
            &channel_number,
            &mut retry,
        )
        .await;
        if registered {
            session_registry.remove(&channel_number).await;
        }
        for path in cleanup_paths {
            cleanup_stream_artifact(&path).await;
        }
    });

    Ok(guarded_stream_body(reader, handle_cancel))
}

fn spawn_stream_body_waiting_for_mpegts_capacity(
    command: FfmpegCommand,
    slate_command: FfmpegCommand,
    cleanup_paths: Vec<PathBuf>,
    channel_number: String,
    session_registry: Arc<dyn HlsSessionRegistry>,
    pool: DbPool,
    retry: Option<StreamDenialRetry>,
) -> Body {
    let handle = Arc::new(DirectStreamHandle::new(channel_number.clone()));
    let cancel = handle.cancel.clone();
    let handle_cancel = handle.cancel.clone();
    let (reader, mut writer) = tokio::io::duplex(STREAM_BODY_BUFFER_BYTES);

    tokio::spawn(async move {
        let registered = session_registry.try_insert(handle).await;
        let mut retry = retry;
        loop {
            let limit = match load_active_transcode_limit(&pool).await {
                Ok(limit) => limit.unwrap_or(DEFAULT_ACTIVE_TRANSCODE_LIMIT),
                Err(error) => {
                    tracing::error!(
                        %error,
                        channel_number,
                        "failed to reload active transcode limit; ending MPEG-TS wait stream"
                    );
                    break;
                }
            };
            if let Some(reservation) = try_reserve_active_transcode(
                limit,
                ActiveTranscodeSlot::new_mpeg_ts(&channel_number),
            ) {
                let _capacity_reservation = reservation;
                match spawn_stream_process(&command) {
                    Ok(child) => {
                        let _ = supervise_stream_command_with_retry(
                            child,
                            &mut writer,
                            &cancel,
                            &channel_number,
                            &mut retry,
                        )
                        .await;
                    }
                    Err(error) => {
                        tracing::warn!(
                            %error,
                            channel_number = %channel_number,
                            "failed to spawn MPEG-TS stream after capacity became available"
                        );
                    }
                }
                break;
            }

            if request_hls_capacity_reclaim_for_mpeg_ts(&channel_number) {
                tokio::time::sleep(MPEG_TS_CAPACITY_RETRY_INTERVAL).await;
                continue;
            }

            tracing::debug!(
                channel_number = %channel_number,
                limit,
                "serving MPEG-TS all-streams-in-use slate without reserving transcode capacity"
            );
            let mut slate_child = match spawn_stream_process(&slate_command) {
                Ok(child) => child,
                Err(error) => {
                    tracing::warn!(
                        %error,
                        channel_number = %channel_number,
                        "failed to spawn MPEG-TS all-streams-in-use slate"
                    );
                    break;
                }
            };
            match supervise_stream_process(&mut slate_child, &mut writer, &cancel, &channel_number)
                .await
            {
                DirectStreamOutcome::Completed => {
                    tokio::time::sleep(MPEG_TS_CAPACITY_RETRY_INTERVAL).await;
                }
                DirectStreamOutcome::Cancelled => break,
                DirectStreamOutcome::Failed(stderr) => {
                    tracing::warn!(
                        stderr = %stderr,
                        channel_number = %channel_number,
                        "MPEG-TS all-streams-in-use slate exited unsuccessfully"
                    );
                    break;
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

    guarded_stream_body(reader, handle_cancel)
}

async fn supervise_stream_command_with_retry(
    mut child: tokio::process::Child,
    writer: &mut tokio::io::DuplexStream,
    cancel: &Arc<Notify>,
    channel_number: &str,
    retry: &mut Option<StreamDenialRetry>,
) -> DirectStreamOutcome {
    loop {
        let outcome = supervise_stream_process(&mut child, writer, cancel, channel_number).await;
        let DirectStreamOutcome::Failed(stderr) = outcome else {
            return outcome;
        };
        let Some(context) = retry.take() else {
            return DirectStreamOutcome::Failed(stderr);
        };
        let StreamDenialRetry {
            pool,
            runtime_capabilities,
            acceleration,
            source_stream,
            rebuild,
        } = context;

        if let Some(denial) =
            learned_hardware_decode_denial(false, acceleration, source_stream.as_ref(), &stderr)
        {
            tracing::warn!(
                channel_number = %channel_number,
                acceleration = ?denial.acceleration,
                codec = %denial.codec,
                bit_depth = denial.bit_depth,
                "retrying channel stream once with software input decode after runtime hardware decode rejection"
            );
            let mut capabilities = runtime_capabilities;
            if let Err(error) =
                persist_hardware_decode_denial(&pool, &mut capabilities, denial).await
            {
                tracing::warn!(
                    %error,
                    channel_number = %channel_number,
                    "failed to persist runtime-learned hardware decode denial"
                );
            }
            match spawn_stream_process(&rebuild(StreamRetryMode::HardwareDecodeDenied(
                capabilities,
            ))) {
                Ok(next) => child = next,
                Err(error) => {
                    tracing::warn!(%error, channel_number = %channel_number, "failed to respawn ffmpeg channel stream after hardware decode denial");
                    return DirectStreamOutcome::Failed(error.to_string());
                }
            }
            continue;
        }

        if acceleration == ersatzrs_domain::HardwareAccelerationKind::None
            || !ffmpeg_stderr_is_hardware_encoder_open_failure(&stderr)
        {
            return DirectStreamOutcome::Failed(stderr);
        }

        tracing::warn!(
            channel_number = %channel_number,
            acceleration = ?acceleration,
            "retrying channel stream once with software video encode after runtime hardware encoder rejection"
        );
        match spawn_stream_process(&rebuild(StreamRetryMode::HardwareEncodeDenied(
            runtime_capabilities,
        ))) {
            Ok(next) => child = next,
            Err(error) => {
                tracing::warn!(%error, channel_number = %channel_number, "failed to respawn ffmpeg channel stream after hardware encoder rejection");
                return DirectStreamOutcome::Failed(error.to_string());
            }
        }
    }
}

/// Builds the response [`Body`], tying the `FFmpeg` process lifetime to the
/// response: dropping the body fires [`StreamBodyDisposeGuard`], which cancels
/// the supervisor and kills the process (`RegisterForDispose` parity).
fn guarded_stream_body(reader: tokio::io::DuplexStream, cancel: Arc<Notify>) -> Body {
    use futures_util::StreamExt;

    let guard = StreamBodyDisposeGuard { cancel };
    Body::from_stream(ReaderStream::new(reader).map(move |chunk| {
        let _held_until_body_drop = &guard;
        chunk
    }))
}

async fn supervise_stream_process(
    child: &mut tokio::process::Child,
    writer: &mut tokio::io::DuplexStream,
    cancel: &Arc<Notify>,
    channel_number: &str,
) -> DirectStreamOutcome {
    let mut stdout = child.stdout.take();
    let stderr_task = child
        .stderr
        .take()
        .map(|stderr| tokio::spawn(read_ffmpeg_stderr_tail(stderr)));

    // Pump stdout into the response body until EOF (process exit), a body
    // write failure (client gone), or session cancellation.
    let pump = async {
        match stdout.as_mut() {
            Some(stdout) => tokio::io::copy(stdout, writer).await.map(|_| ()),
            None => Ok(()),
        }
    };
    tokio::select! {
        result = pump => {
            if result.is_err() {
                // The body side is gone; stop the process instead of letting
                // it block on a pipe nobody drains.
                tracing::debug!(channel_number = %channel_number, "channel stream body writer failed; killing ffmpeg");
                let _ = child.start_kill();
            }
        }
        () = cancel.notified() => {
            tracing::debug!(channel_number = %channel_number, "channel stream cancelled; killing ffmpeg");
            if let Err(error) = child.kill().await {
                tracing::warn!(%error, channel_number = %channel_number, "failed to kill ffmpeg channel stream");
            }
            let stderr = await_stderr_tail(stderr_task).await;
            if !stderr.is_empty() {
                tracing::debug!(stderr = %stderr, channel_number = %channel_number, "ffmpeg channel stream stderr after cancellation");
            }
            return DirectStreamOutcome::Cancelled;
        }
    }

    match child.wait().await {
        Ok(status) if status.success() => {
            let _ = await_stderr_tail(stderr_task).await;
            DirectStreamOutcome::Completed
        }
        Ok(status) => {
            let stderr = await_stderr_tail(stderr_task).await;
            if ffmpeg_stderr_is_client_disconnect(&stderr) {
                tracing::debug!(%status, stderr = %stderr, "ffmpeg channel stream ended after client disconnect");
                DirectStreamOutcome::Completed
            } else {
                tracing::warn!(%status, stderr = %stderr, "ffmpeg channel stream exited unsuccessfully");
                DirectStreamOutcome::Failed(stderr)
            }
        }
        Err(error) => {
            let stderr = await_stderr_tail(stderr_task).await;
            tracing::warn!(%error, stderr = %stderr, "failed to wait for ffmpeg channel stream");
            DirectStreamOutcome::Failed(stderr)
        }
    }
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

/// How long to wait for the stderr tail after the direct child is gone.
///
/// MPEG-TS script commands run `FFmpeg` as a grandchild of the spawned shell,
/// and the grandchild inherits the stderr pipe: when only the shell has been
/// killed, EOF never arrives. An unbounded wait here used to deadlock the
/// supervisor, which kept the stdout pipe open and therefore kept the
/// orphaned `FFmpeg` alive forever — one leaked remux per disconnected
/// viewer. `ErsatzTV` reaps such processes by closing the pipes on response
/// disposal (`FFmpegProcess` dispose), so a bounded wait plus dropping our
/// pipe ends restores that behavior: the orphan dies on its next write.
const STDERR_TAIL_TIMEOUT: Duration = Duration::from_secs(2);

async fn await_stderr_tail(stderr_task: Option<tokio::task::JoinHandle<String>>) -> String {
    let Some(mut task) = stderr_task else {
        return String::new();
    };
    match tokio::time::timeout(STDERR_TAIL_TIMEOUT, &mut task).await {
        Ok(result) => {
            result.unwrap_or_else(|error| format!("failed to join ffmpeg stderr reader: {error}"))
        }
        Err(_elapsed) => {
            // A grandchild still holds the stderr write end open; abandon the
            // reader so the pipe read ends drop and the orphan gets EPIPE.
            task.abort();
            String::new()
        }
    }
}

fn ffmpeg_stderr_is_client_disconnect(stderr: &str) -> bool {
    stderr.contains("Broken pipe")
}

fn ffmpeg_stderr_is_hardware_encoder_open_failure(stderr: &str) -> bool {
    let lower = stderr.to_ascii_lowercase();
    lower.contains("openencodesessionex failed")
        || lower.contains("error while opening encoder")
        || lower.contains("could not open encoder before eof")
        || lower.contains("no capable devices found")
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
    use std::sync::{Arc, OnceLock};
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

    async fn active_transcode_stream_test_guard() -> tokio::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
            .lock()
            .await
    }

    #[cfg(not(feature = "mysql"))]
    #[tokio::test]
    async fn active_playback_item_loader_rejects_invalid_subtitle_state() {
        let pool = active_playback_item_test_pool().await;
        let now = DateTime::parse_from_rfc3339("2026-06-08T12:00:00Z")
            .expect("timestamp")
            .with_timezone(&Utc);

        assert!(matches!(
            load_active_media_path(&pool, 7, now).await,
            Err(RepositoryError::Decode { .. })
        ));
        assert!(load_active_media_path(&pool, 99, now)
            .await
            .expect("load absent active playback item")
            .is_none());
    }

    #[test]
    fn active_playback_duration_rejects_empty_fraction() {
        assert!(matches!(
            parse_duration_text("00:00:01."),
            Err(RepositoryError::Decode { .. })
        ));
        assert_eq!(
            parse_duration_text("00:00:01").expect("parse whole-second duration"),
            Duration::from_secs(1)
        );
        assert_eq!(
            parse_duration_text("00:00:01.5").expect("parse fractional duration"),
            Duration::from_millis(1_500)
        );
    }

    #[cfg(not(feature = "mysql"))]
    async fn active_playback_item_test_pool() -> DbPool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("open active playback item test database");
        for statement in [
            r#"CREATE TABLE "Playout" (
                "Id" INTEGER PRIMARY KEY,
                "ChannelId" INTEGER NOT NULL
            )"#,
            r#"CREATE TABLE "PlayoutItem" (
                "Id" INTEGER PRIMARY KEY,
                "MediaItemId" INTEGER NOT NULL,
                "PlayoutId" INTEGER NOT NULL,
                "InPoint" TEXT NOT NULL,
                "OutPoint" TEXT NOT NULL,
                "Start" TEXT NOT NULL,
                "Finish" TEXT NOT NULL,
                "DisableWatermarks" INTEGER NOT NULL,
                "PreferredAudioLanguageCode" TEXT NULL,
                "PreferredAudioTitle" TEXT NULL,
                "PreferredSubtitleLanguageCode" TEXT NULL,
                "SubtitleMode" INTEGER NULL
            )"#,
            r#"CREATE TABLE "MediaVersion" (
                "Id" INTEGER PRIMARY KEY,
                "Duration" TEXT NOT NULL,
                "MovieId" INTEGER NULL,
                "EpisodeId" INTEGER NULL,
                "MusicVideoId" INTEGER NULL,
                "OtherVideoId" INTEGER NULL,
                "RemoteStreamId" INTEGER NULL,
                "SongId" INTEGER NULL,
                "ImageId" INTEGER NULL
            )"#,
            r#"CREATE TABLE "MediaFile" (
                "MediaVersionId" INTEGER NOT NULL,
                "Path" TEXT NULL
            )"#,
        ] {
            sqlx::query(statement)
                .execute(&pool)
                .await
                .expect("create active playback item test table");
        }
        for statement in [
            r#"INSERT INTO "Playout" ("Id", "ChannelId") VALUES (11, 7)"#,
            r#"INSERT INTO "PlayoutItem" (
                "Id", "MediaItemId", "PlayoutId", "InPoint", "OutPoint",
                "Start", "Finish", "DisableWatermarks", "SubtitleMode"
            ) VALUES (
                12, 20, 11, '00:00:00', '00:02:00',
                '2026-06-08 11:59:00', '2026-06-08 12:01:00', 0, 99
            )"#,
            r#"INSERT INTO "MediaVersion" ("Id", "Duration", "MovieId")
               VALUES (21, '00:02:00', 20)"#,
            r#"INSERT INTO "MediaFile" ("MediaVersionId", "Path")
               VALUES (21, '/media/test.mkv')"#,
        ] {
            sqlx::query(statement)
                .execute(&pool)
                .await
                .expect("seed active playback item test row");
        }
        pool
    }

    #[test]
    fn build_stream_command_pipes_mpegts_to_stdout() {
        let profile = playback_profile();
        let command = build_stream_command(
            "/media/movie.mkv",
            OutputFormat::MpegTs,
            Duration::ZERO,
            &profile,
            None,
            StreamingMode::TransportStream,
            OutputMetadata::default(),
            FfmpegCapabilityReport::default(),
            None,
            Vec::new(),
            None,
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
    fn build_stream_command_maps_selected_audio_stream() {
        let profile = playback_profile();
        let command = build_stream_command(
            "/media/movie.mkv",
            OutputFormat::MpegTs,
            Duration::ZERO,
            &profile,
            None,
            StreamingMode::TransportStream,
            OutputMetadata::default(),
            FfmpegCapabilityReport::default(),
            None,
            Vec::new(),
            Some(3),
            None,
            Vec::new(),
            Vec::new(),
        );
        let arguments = command.arguments.join(" ");

        assert!(
            arguments.contains("[0:3]aresample=async=1,apad[a]"),
            "{arguments}"
        );
        assert!(arguments.contains("-map [a]"), "{arguments}");
    }

    #[test]
    fn rebuilt_stream_command_after_learned_denial_selects_software_decode() {
        // ADR 0014 .ts retry decision: a classified decode-unsupported stderr
        // produces a denial, and the rebuilt command (what the supervisor
        // respawns) selects software input decode while keeping NVENC encode.
        let mut profile = playback_profile();
        profile.profile.hardware_acceleration = ersatzrs_domain::HardwareAccelerationKind::Nvenc;
        let mut source = SourceVideoStream::new(ersatzrs_ffmpeg::SourceVideoCodec::Av1);
        source.pixel_format = Some(ersatzrs_ffmpeg::PixelFormat::Yuv420P10Le);
        let report = ersatzrs_ffmpeg::capability_report_for_tests();

        let build = |capabilities: FfmpegCapabilityReport| {
            build_stream_command(
                "/media/movie.mkv",
                OutputFormat::MpegTs,
                Duration::ZERO,
                &profile,
                None,
                StreamingMode::TransportStream,
                OutputMetadata::default(),
                capabilities,
                Some(source.clone()),
                Vec::new(),
                None,
                None,
                Vec::new(),
                Vec::new(),
            )
        };

        // First spawn: the probe rows allow hardware AV1 decode.
        let first = build(report.clone()).arguments.join(" ");
        assert!(first.contains("-hwaccel cuda"), "{first}");
        assert!(first.contains("-codec:v h264_nvenc"), "{first}");

        // The supervisor classifies the failure and records the denial.
        let stderr = "Your platform doesn't support hardware accelerated AV1 decoding.";
        let denial = learned_hardware_decode_denial(
            false,
            ersatzrs_domain::HardwareAccelerationKind::Nvenc,
            Some(&source),
            stderr,
        )
        .expect("classified decode rejection produces a denial");

        // The retried spawn selects software input decode, NVENC encode stays.
        let mut denied = report;
        denied.deny_hardware_decode(denial);
        let second = build(denied).arguments.join(" ");
        assert!(second.contains("-hwaccel none -c:v libdav1d"), "{second}");
        assert!(second.contains("-codec:v h264_nvenc"), "{second}");
        assert!(!second.contains("-hwaccel cuda"), "{second}");
    }

    #[test]
    fn hardware_encoder_open_failure_is_classified_for_retry() {
        let stderr = "[h264_nvenc @ 0x5c328a96cf80] OpenEncodeSessionEx failed: incompatible client key (21): (no details)\n[vost#0:0/h264_nvenc @ 0x5c328a2f8f00] [enc:h264_nvenc @ 0x5c3288cf02c0] Error while opening encoder";

        assert!(ffmpeg_stderr_is_hardware_encoder_open_failure(stderr));
        assert!(!ffmpeg_stderr_is_hardware_encoder_open_failure(
            "Error opening input file /media/missing.mkv"
        ));
    }

    #[test]
    fn hardware_encode_retry_profile_selects_software_h264() {
        let mut profile = playback_profile();
        profile.profile.hardware_acceleration = ersatzrs_domain::HardwareAccelerationKind::Nvenc;
        profile.profile.video_preset = Some("llhq".to_owned());
        let source = SourceVideoStream::new(ersatzrs_ffmpeg::SourceVideoCodec::H264);
        let report = ersatzrs_ffmpeg::capability_report_for_tests();

        let first = build_stream_command(
            "/media/movie.mkv",
            OutputFormat::MpegTs,
            Duration::ZERO,
            &profile,
            None,
            StreamingMode::TransportStream,
            OutputMetadata::default(),
            report.clone(),
            Some(source.clone()),
            Vec::new(),
            None,
            None,
            Vec::new(),
            Vec::new(),
        )
        .arguments
        .join(" ");
        assert!(first.contains("-codec:v h264_nvenc"), "{first}");
        assert!(first.contains("-preset:v llhq"), "{first}");

        force_software_h264_encoding(&mut profile);
        let second = build_stream_command(
            "/media/movie.mkv",
            OutputFormat::MpegTs,
            Duration::ZERO,
            &profile,
            None,
            StreamingMode::TransportStream,
            OutputMetadata::default(),
            report,
            Some(source),
            Vec::new(),
            None,
            None,
            Vec::new(),
            Vec::new(),
        )
        .arguments
        .join(" ");
        assert!(second.contains("-codec:v libx264"), "{second}");
        assert!(second.contains("-preset:v veryfast"), "{second}");
        assert!(!second.contains("h264_nvenc"), "{second}");
        assert!(!second.contains("llhq"), "{second}");
        assert!(!second.contains("-hwaccel cuda"), "{second}");
    }

    #[test]
    fn build_stream_command_can_target_mp4() {
        let profile = playback_profile();
        let command = build_stream_command(
            "/media/movie.mkv",
            OutputFormat::Mp4,
            Duration::ZERO,
            &profile,
            None,
            StreamingMode::HttpLiveStreamingDirect,
            OutputMetadata::default(),
            FfmpegCapabilityReport::default(),
            None,
            Vec::new(),
            None,
            None,
            Vec::new(),
            Vec::new(),
        );
        let arguments = command.arguments.join(" ");
        assert!(arguments.contains("-f mp4"));
        assert!(arguments.ends_with("pipe:1"));
    }

    #[test]
    fn hls_direct_mpegts_transcodes_unsafe_copy_codecs() {
        let mut profile = playback_profile();
        profile.profile.video_preset = Some("llhp".to_owned());
        let video = SourceVideoStream::new(SourceVideoCodec::Vp9);
        let audio = SourceAudioStream::new(SourceAudioCodec::Unknown).with_index(1);

        let command = build_stream_command(
            "/media/unplugged.webm",
            OutputFormat::MpegTs,
            Duration::ZERO,
            &profile,
            None,
            StreamingMode::HttpLiveStreamingDirect,
            OutputMetadata::default(),
            FfmpegCapabilityReport::default(),
            Some(video),
            vec![audio],
            None,
            None,
            Vec::new(),
            Vec::new(),
        );
        let arguments = command.arguments.join(" ");

        assert!(arguments.contains("-codec:v libx264"), "{arguments}");
        assert!(arguments.contains("-preset:v veryfast"), "{arguments}");
        assert!(!arguments.contains("-preset:v llhp"), "{arguments}");
        assert!(arguments.contains("-codec:a aac"), "{arguments}");
        assert!(!arguments.contains("-codec:v copy"), "{arguments}");
        assert!(!arguments.contains("-codec:a copy"), "{arguments}");
        assert!(arguments.contains("-f mpegts"), "{arguments}");
    }

    #[test]
    fn hls_direct_mpegts_keeps_copy_for_ts_safe_codecs() {
        let profile = playback_profile();
        let video = SourceVideoStream::new(SourceVideoCodec::H264);
        let audio = SourceAudioStream::new(SourceAudioCodec::Aac).with_index(1);

        let command = build_stream_command(
            "/media/movie.mp4",
            OutputFormat::MpegTs,
            Duration::ZERO,
            &profile,
            None,
            StreamingMode::HttpLiveStreamingDirect,
            OutputMetadata::default(),
            FfmpegCapabilityReport::default(),
            Some(video),
            vec![audio],
            None,
            None,
            Vec::new(),
            Vec::new(),
        );
        let arguments = command.arguments.join(" ");

        assert!(arguments.contains("-codec:v copy"), "{arguments}");
        assert!(arguments.contains("-codec:a copy"), "{arguments}");
        assert!(arguments.contains("-f mpegts"), "{arguments}");
    }

    #[test]
    fn build_stream_command_seeks_to_playout_offset() {
        let profile = playback_profile();
        let command = build_stream_command(
            "/media/movie.mkv",
            OutputFormat::MpegTs,
            Duration::from_mins(15),
            &profile,
            None,
            StreamingMode::TransportStream,
            OutputMetadata::default(),
            FfmpegCapabilityReport::default(),
            None,
            Vec::new(),
            None,
            None,
            Vec::new(),
            Vec::new(),
        );
        let arguments = command.arguments.join(" ");
        // ErsatzTV emits the seek as an input-side option (`StreamSeekInputOption`,
        // added before the decoder in `PipelineBuilderBase`), so the seek precedes
        // the `-i` input with the source decoder option in between. Assert the
        // input-side seek contract by position rather than hard-coding the decoder
        // string (that argv is covered by the pipeline snapshot suite).
        let seek = arguments
            .find("-ss 00:15:00.0000000")
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
            Duration::ZERO,
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
    fn build_stream_command_burns_generated_music_video_credits_subtitle() {
        let profile = playback_profile();
        let subtitle = PlaybackSubtitle {
            id: 0,
            subtitle_kind: ersatzrs_infra::metadata::subtitle_kind::GENERATED,
            title: Some("Music Video Credits".to_owned()),
            stream_index: 0,
            codec: Some("ass".to_owned()),
            default: true,
            forced: true,
            sdh: false,
            language: None,
            is_extracted: false,
            path: Some("/tmp/ersatzrs/music-video-credits-generated.ass".to_owned()),
        };
        let subtitle_input = subtitle_input_for_stream(
            &subtitle,
            "/media/music-video.mkv",
            OutputFormat::MpegTs,
            StreamingMode::TransportStream,
        );
        let command = build_stream_command(
            "/media/music-video.mkv",
            OutputFormat::MpegTs,
            Duration::ZERO,
            &profile,
            None,
            StreamingMode::TransportStream,
            playback_output_metadata(None, None, None, &[], Some(&subtitle), None),
            FfmpegCapabilityReport::default(),
            None,
            Vec::new(),
            None,
            subtitle_input,
            Vec::new(),
            Vec::new(),
        );
        let arguments = command.arguments.join(" ");

        assert!(
            arguments.contains("subtitles=/tmp/ersatzrs/music-video-credits-generated.ass"),
            "{arguments}"
        );
        assert!(arguments.contains("fontsdir="), "{arguments}");
        assert!(
            arguments.contains("-metadata:s:s:0 title=Music Video Credits"),
            "{arguments}"
        );
    }

    #[test]
    fn build_stream_command_attaches_graphics_overlays() {
        let profile = playback_profile();
        let command = build_stream_command(
            "/media/movie.mkv",
            OutputFormat::MpegTs,
            Duration::ZERO,
            &profile,
            None,
            StreamingMode::TransportStream,
            OutputMetadata::default(),
            FfmpegCapabilityReport::default(),
            None,
            Vec::new(),
            None,
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

    fn active_playout_item(
        playout_start: &str,
        playout_finish: &str,
        in_point: Duration,
        out_point: Duration,
    ) -> ActivePlayoutItem {
        ActivePlayoutItem {
            playout_item_id: 1,
            media_path: "/media/calibration-1.mp4".to_owned(),
            media_item_id: 10,
            media_version_id: 20,
            content_duration: Some(Duration::from_secs(15)),
            in_point,
            out_point,
            playout_start: playout_start.to_owned(),
            playout_finish: playout_finish.to_owned(),
            disable_watermarks: false,
            preferred_audio_language_code: None,
            preferred_audio_title: None,
            preferred_subtitle_language_code: None,
            subtitle_mode: None,
        }
    }

    #[test]
    fn active_playout_item_remaining_duration_uses_playout_finish() {
        let now = DateTime::parse_from_rfc3339("2026-06-15T12:00:04.250Z")
            .expect("valid timestamp")
            .with_timezone(&Utc);
        let active = active_playout_item(
            "2026-06-15 12:00:00",
            "2026-06-15 12:00:06",
            Duration::ZERO,
            Duration::from_secs(15),
        );

        assert_eq!(
            active.remaining_duration(now),
            Some(Duration::from_millis(1750))
        );
    }

    #[test]
    fn active_playout_item_remaining_duration_caps_at_out_point() {
        let now = DateTime::parse_from_rfc3339("2026-06-15T12:00:04.250Z")
            .expect("valid timestamp")
            .with_timezone(&Utc);
        let active = active_playout_item(
            "2026-06-15 12:00:00",
            "2026-06-15 12:00:20",
            Duration::ZERO,
            Duration::from_secs(6),
        );

        assert_eq!(
            active.remaining_duration(now),
            Some(Duration::from_millis(1750))
        );
    }

    #[test]
    fn build_stream_command_limits_output_to_remaining_playout_item() {
        let profile = playback_profile();
        let command = build_stream_command_with_opacity_clock(
            "/media/calibration-1.mp4",
            OutputFormat::MpegTs,
            Duration::from_secs(4),
            Some(Duration::from_millis(1750)),
            &profile,
            None,
            StreamingMode::TransportStream,
            OutputMetadata::default(),
            FfmpegCapabilityReport::default(),
            None,
            Vec::new(),
            None,
            None,
            Vec::new(),
            Vec::new(),
            None,
        );

        assert!(
            command
                .arguments
                .windows(2)
                .any(|pair| pair == ["-t", "00:00:01.7500000"]),
            "{:?}",
            command.arguments
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
    fn playout_start_duration_matches_mid_item_offset() {
        let now = DateTime::parse_from_rfc3339("2026-05-31T21:45:00.517Z")
            .expect("valid timestamp")
            .with_timezone(&Utc);
        assert_eq!(
            playout_start_duration(now, "2026-05-31 21:30:00"),
            Duration::from_millis(900_517)
        );
        assert_eq!(
            playout_start_duration(now, "2026-05-31 21:45:00"),
            Duration::from_millis(517)
        );
        assert_eq!(
            playout_start_duration(now, "not a timestamp"),
            Duration::ZERO
        );
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
            resolve_transport_stream_mode(None, StreamingMode::TransportStreamHybrid),
            TransportStreamMode::Legacy
        );
        assert_eq!(
            resolve_transport_stream_mode(Some("mixed"), StreamingMode::TransportStreamHybrid),
            TransportStreamMode::Legacy
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
            TransportStreamMode::Legacy
        );
        assert_eq!(
            resolve_transport_stream_mode(Some("hybrid"), StreamingMode::TransportStream),
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
    fn mpeg_ts_wrapper_muxrate_uses_profile_bandwidth_overhead() {
        let mut profile = playback_profile();
        assert_eq!(mpeg_ts_wrapper_muxrate_bps(&profile), 9_830_400);

        profile.profile.video_bitrate = 0;
        profile.profile.audio_bitrate = 0;
        assert_eq!(mpeg_ts_wrapper_muxrate_bps(&profile), 12_000_000);
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
            r#""{{ FFmpegPath }}" -i "{{ HlsUrl }}" -muxrate {{ MuxRate }} -pcr_period {{ PcrPeriod }} -metadata service_name="{{ ChannelName }}""#,
            &MpegTsScriptRenderData {
                hls_url: "http://localhost:8409/iptv/channel/1.m3u8?mode=segmenter".to_owned(),
                channel_name: "Channel One".to_owned(),
                ffmpeg_path: "/opt/ffmpeg/bin/ffmpeg".to_owned(),
                muxrate_bps: 9_830_400,
                pcr_period_ms: 20,
            },
        );

        assert_eq!(
            rendered,
            r#""/opt/ffmpeg/bin/ffmpeg" -i "http://localhost:8409/iptv/channel/1.m3u8?mode=segmenter" -muxrate 9830400 -pcr_period 20 -metadata service_name="Channel One""#
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
            r#""{{ FFmpegPath }}" -i "{{ HlsUrl }}" -muxrate {{ MuxRate }} -pcr_period {{ PcrPeriod }} -metadata service_name="{{ ChannelName }}""#,
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
                muxrate_bps: 9_830_400,
                pcr_period_ms: 20,
            },
        )
        .await
        .expect("render command");

        let rendered_path = cleanup_paths.first().expect("cleanup path");
        let rendered = std::fs::read_to_string(rendered_path).expect("read rendered script");
        assert!(rendered.contains("/opt/ffmpeg/bin/ffmpeg"));
        assert!(rendered.contains("Channel One"));
        assert!(rendered.contains("-muxrate 9830400"));
        assert!(rendered.contains("-pcr_period 20"));
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

    #[tokio::test]
    async fn stderr_tail_wait_is_bounded_when_grandchild_holds_pipe_open() {
        // MPEG-TS script commands leave an FFmpeg grandchild holding the
        // inherited stderr pipe after the shell is killed; the tail wait must
        // give up so the supervisor drops its pipe ends and the orphan gets
        // EPIPE instead of living forever.
        let never_finishes = tokio::spawn(async {
            tokio::time::sleep(Duration::from_hours(1)).await;
            String::new()
        });

        let started = tokio::time::Instant::now();
        let tail = await_stderr_tail(Some(never_finishes)).await;

        assert!(tail.is_empty());
        assert!(started.elapsed() < STDERR_TAIL_TIMEOUT + Duration::from_secs(3));
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ignore = "requires ffmpeg on PATH (Unix-only: exercises the sh-wrapped MPEG-TS script process tree)"]
    async fn dropping_stream_body_reaps_script_grandchild_ffmpeg() {
        // Regression for the leaked-session bug: the default MPEG-TS script
        // runs FFmpeg as a grandchild of `sh`, so killing the shell alone
        // left one orphaned remux per disconnected viewer. Dropping the body
        // must reap the whole tree via pipe closure.
        let marker = format!("ersatzrs-reap-test-{}", std::process::id());
        let registry: Arc<dyn HlsSessionRegistry> =
            Arc::new(ersatzrs_infra::hls::InMemoryHlsSessionRegistry::new());
        let command = FfmpegCommand {
            executable: "/bin/sh".to_owned(),
            arguments: vec![
                "-c".to_owned(),
                format!(
                    "{} -hide_banner -loglevel error -re -f lavfi -i testsrc2=size=320x180:rate=30 -metadata service_name={marker} -f mpegts pipe:1",
                    ersatzrs_ffmpeg::ffmpeg_path()
                ),
            ],
            environment: Vec::new(),
        };

        let body = spawn_stream_body(
            &command,
            Vec::new(),
            "98".to_owned(),
            Arc::clone(&registry),
            None,
            None,
        )
        .expect("spawn stream body");
        let grandchild_running = || async {
            tokio::process::Command::new("pgrep")
                .args(["-f", &marker])
                .output()
                .await
                .is_ok_and(|output| output.status.success())
        };
        let spawn_deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        while !grandchild_running().await {
            assert!(
                tokio::time::Instant::now() < spawn_deadline,
                "grandchild ffmpeg never started"
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        drop(body);

        let reap_deadline = tokio::time::Instant::now() + Duration::from_secs(15);
        while grandchild_running().await {
            assert!(
                tokio::time::Instant::now() < reap_deadline,
                "grandchild ffmpeg was not reaped after the body was dropped"
            );
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }

    #[tokio::test]
    async fn dropping_stream_body_signals_supervisor_cancellation() {
        // `RegisterForDispose` parity: the response body going away must
        // cancel the supervisor (which kills the FFmpeg process) without
        // waiting for the next failed pipe write.
        let cancel = Arc::new(Notify::new());
        let (reader, _writer) = tokio::io::duplex(64);
        let body = guarded_stream_body(reader, cancel.clone());

        drop(body);

        tokio::time::timeout(Duration::from_secs(1), cancel.notified())
            .await
            .expect("dropping the body must signal cancellation");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn spawn_stream_body_releases_mpegts_capacity_on_process_exit() {
        let _guard = active_transcode_stream_test_guard().await;
        ersatzrs_infra::transcode_capacity::reset_active_transcode_state_for_tests();
        let registry: Arc<dyn HlsSessionRegistry> =
            Arc::new(ersatzrs_infra::hls::InMemoryHlsSessionRegistry::new());
        let reservation = try_reserve_active_transcode(1, ActiveTranscodeSlot::new_mpeg_ts("91"))
            .expect("capacity reservation");

        let body = spawn_stream_body(
            &shell_command("printf done", "echo done"),
            Vec::new(),
            "91".to_owned(),
            registry,
            Some(reservation),
            None,
        )
        .expect("spawn stream body");

        assert_eq!(
            ersatzrs_infra::transcode_capacity::active_transcode_snapshot(1).active,
            1
        );
        wait_for_capacity_active(1, 0).await;
        drop(body);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn spawn_stream_body_releases_mpegts_capacity_on_body_drop() {
        let _guard = active_transcode_stream_test_guard().await;
        ersatzrs_infra::transcode_capacity::reset_active_transcode_state_for_tests();
        let registry: Arc<dyn HlsSessionRegistry> =
            Arc::new(ersatzrs_infra::hls::InMemoryHlsSessionRegistry::new());
        let reservation = try_reserve_active_transcode(1, ActiveTranscodeSlot::new_mpeg_ts("92"))
            .expect("capacity reservation");

        let body = spawn_stream_body(
            &shell_command(
                "printf started; sleep 30",
                "echo started & ping -n 30 127.0.0.1 >NUL",
            ),
            Vec::new(),
            "92".to_owned(),
            registry,
            Some(reservation),
            None,
        )
        .expect("spawn stream body");

        assert_eq!(
            ersatzrs_infra::transcode_capacity::active_transcode_snapshot(1).active,
            1
        );
        drop(body);
        wait_for_capacity_active(1, 0).await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn mpegts_capacity_wait_body_serves_slate_without_extra_slot_then_switches() {
        let _guard = active_transcode_stream_test_guard().await;
        ersatzrs_infra::transcode_capacity::reset_active_transcode_state_for_tests();
        let pool = capacity_test_pool(1).await;
        let registry: Arc<dyn HlsSessionRegistry> =
            Arc::new(ersatzrs_infra::hls::InMemoryHlsSessionRegistry::new());
        let occupied = try_reserve_active_transcode(1, ActiveTranscodeSlot::new_mpeg_ts("busy"))
            .expect("occupy the only MPEG-TS slot");

        let body = spawn_stream_body_waiting_for_mpegts_capacity(
            shell_command("printf content", "echo content"),
            shell_command("printf slate", "echo slate"),
            Vec::new(),
            "93".to_owned(),
            registry,
            pool,
            None,
        );
        let release = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            drop(occupied);
        });

        let bytes = axum::body::to_bytes(body, 64 * 1024)
            .await
            .expect("read MPEG-TS wait body");
        release.await.expect("release occupied capacity");
        let output = String::from_utf8_lossy(&bytes);
        let slate_index = output.find("slate").expect("slate output");
        let content_index = output.find("content").expect("content output");

        assert!(
            slate_index < content_index,
            "capacity slate should precede real content, got {output:?}"
        );
        assert_eq!(
            ersatzrs_infra::transcode_capacity::active_transcode_snapshot(1).active,
            0
        );
        assert_eq!(
            ersatzrs_infra::transcode_capacity::active_transcode_snapshot(1).peak,
            1,
            "slate loop must not increment active capacity beyond the occupied real slot"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn mpegts_capacity_wait_reclaims_hls_work_ahead_before_slate() {
        let _guard = active_transcode_stream_test_guard().await;
        ersatzrs_infra::transcode_capacity::reset_active_transcode_state_for_tests();
        let pool = capacity_test_pool(1).await;
        let registry: Arc<dyn HlsSessionRegistry> =
            Arc::new(ersatzrs_infra::hls::InMemoryHlsSessionRegistry::new());
        let notify = Arc::new(Notify::new());
        let requested_slot_id = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let reacquire_deferred_until_ms = Arc::new(std::sync::atomic::AtomicI64::new(0));
        let occupied = try_reserve_active_transcode(
            1,
            ersatzrs_infra::transcode_capacity::ActiveTranscodeSlot::new_hls(
                "hls",
                ersatzrs_infra::transcode_capacity::ActiveTranscodeKind::HlsWorkAhead,
                Arc::new(std::sync::atomic::AtomicI64::new(
                    (chrono::Utc::now() - chrono::Duration::seconds(10)).timestamp_millis(),
                )),
                ersatzrs_infra::transcode_capacity::ActiveTranscodeReclaimHandle::new(
                    Arc::clone(&notify),
                    Arc::clone(&requested_slot_id),
                    Arc::clone(&reacquire_deferred_until_ms),
                    true,
                ),
            ),
        )
        .expect("occupy the only MPEG-TS slot with HLS work-ahead");

        let release = tokio::spawn(async move {
            tokio::time::timeout(Duration::from_secs(2), notify.notified())
                .await
                .expect("MPEG-TS admission should request HLS reclaim");
            drop(occupied);
        });

        let body = spawn_stream_body_waiting_for_mpegts_capacity(
            shell_command("printf content", "echo content"),
            shell_command("printf slate", "echo slate"),
            Vec::new(),
            "94".to_owned(),
            registry,
            pool,
            None,
        );

        let bytes = axum::body::to_bytes(body, 64 * 1024)
            .await
            .expect("read MPEG-TS wait body");
        release.await.expect("release occupied HLS capacity");
        let output = String::from_utf8_lossy(&bytes);

        assert_eq!(output, "content");
        assert!(!output.contains("slate"));
        assert_ne!(
            requested_slot_id.load(std::sync::atomic::Ordering::Acquire),
            0,
            "reclaim request should identify the HLS slot"
        );
        assert_eq!(
            ersatzrs_infra::transcode_capacity::active_transcode_snapshot(1).active,
            0
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ignore = "requires ffmpeg on PATH"]
    async fn spawn_stream_body_kills_ffmpeg_when_body_is_dropped() {
        // One viewing session must never leave a transcode process running
        // after its HTTP response is gone: the supervisor kills the process
        // and removes the session registration.
        let registry: Arc<dyn HlsSessionRegistry> =
            Arc::new(ersatzrs_infra::hls::InMemoryHlsSessionRegistry::new());
        let command = FfmpegCommand {
            executable: ersatzrs_ffmpeg::ffmpeg_path(),
            arguments: [
                "-hide_banner",
                "-loglevel",
                "error",
                "-re",
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=320x180:rate=30",
                "-f",
                "mpegts",
                "pipe:1",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect(),
            environment: Vec::new(),
        };

        let body = spawn_stream_body(
            &command,
            Vec::new(),
            "97".to_owned(),
            Arc::clone(&registry),
            None,
            None,
        )
        .expect("spawn stream body");
        // Wait for the fire-and-forget supervisor task to register the
        // session before dropping the response body.
        let registered_deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        while !registry.is_active("97").await {
            assert!(
                tokio::time::Instant::now() < registered_deadline,
                "session was never registered"
            );
            tokio::time::sleep(Duration::from_millis(25)).await;
        }

        drop(body);

        // The supervisor kills ffmpeg on disposal and removes the session.
        let removed_deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        while registry.is_active("97").await {
            assert!(
                tokio::time::Instant::now() < removed_deadline,
                "session was not torn down after the body was dropped"
            );
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    fn shell_command(unix_script: &str, windows_script: &str) -> FfmpegCommand {
        if cfg!(windows) {
            FfmpegCommand {
                executable: "cmd.exe".to_owned(),
                arguments: vec!["/C".to_owned(), windows_script.to_owned()],
                environment: Vec::new(),
            }
        } else {
            FfmpegCommand {
                executable: "/bin/sh".to_owned(),
                arguments: vec!["-c".to_owned(), unix_script.to_owned()],
                environment: Vec::new(),
            }
        }
    }

    async fn capacity_test_pool(limit: i32) -> DbPool {
        #[cfg(feature = "mysql")]
        let pool = {
            sqlx::any::install_default_drivers();
            sqlx::any::AnyPoolOptions::new()
                .max_connections(1)
                .connect("sqlite::memory:")
                .await
                .expect("open capacity test db")
        };
        #[cfg(not(feature = "mysql"))]
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("open capacity test db");
        sqlx::query(
            r#"CREATE TABLE "ConfigElement" (
                "Id" INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
                "Key" TEXT NOT NULL UNIQUE,
                "Value" TEXT NOT NULL
            )"#,
        )
        .execute(&pool)
        .await
        .expect("create ConfigElement");
        sqlx::query(r#"INSERT INTO "ConfigElement" ("Key", "Value") VALUES (?, ?)"#)
            .bind(ersatzrs_domain::CONFIG_KEY_FFMPEG_ACTIVE_TRANSCODE_LIMIT)
            .bind(limit.to_string())
            .execute(&pool)
            .await
            .expect("insert active transcode limit");
        pool
    }

    async fn wait_for_capacity_active(limit: i32, expected: i32) {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        while ersatzrs_infra::transcode_capacity::active_transcode_snapshot(limit).active
            != expected
        {
            assert!(
                tokio::time::Instant::now() < deadline,
                "capacity active count did not reach {expected}"
            );
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
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
            Duration::ZERO,
            &playback_profile(),
            None,
            StreamingMode::TransportStream,
            OutputMetadata::default(),
            FfmpegCapabilityReport::default(),
            None,
            Vec::new(),
            None,
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
    #[allow(clippy::too_many_lines)]
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
            Duration::ZERO,
            &playback_profile(),
            None,
            StreamingMode::TransportStream,
            OutputMetadata::default(),
            FfmpegCapabilityReport::default(),
            None,
            Vec::new(),
            None,
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
    #[ignore = "requires ffmpeg on PATH"]
    async fn stream_body_live_proves_opacity_expression_clock_bindings() {
        let ffmpeg = ersatzrs_ffmpeg::ffmpeg_path();
        let mut version = tokio::process::Command::new(&ffmpeg);
        version
            .arg("-version")
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        apply_ffmpeg_library_path(&mut version);
        if version.status().await.is_err() {
            eprintln!("skipping: ffmpeg not on PATH");
            return;
        }

        let temp = tempfile::tempdir().expect("tempdir");
        let input = temp.path().join("black.mp4");
        let logo = temp.path().join("logo.ppm");
        let stream = temp.path().join("opacity-clock.ts");
        generate_opacity_input(&ffmpeg, &input).await;
        std::fs::write(&logo, red_ppm(64, 32)).expect("write watermark fixture");

        let clock = WatermarkOpacityClock::new(86_399.5, 5.0, Some(12.0), 30.0);
        let expression = concat!(
            "if(content_seconds >= 5.5 && content_seconds < content_total_seconds && ",
            "channel_seconds >= 30.5 && channel_seconds < 31.5 && ",
            "time_of_day_seconds < 1, 1, 0)"
        );
        let command = build_stream_command_with_opacity_clock(
            input.to_string_lossy().as_ref(),
            OutputFormat::MpegTs,
            Duration::from_secs(5),
            Some(Duration::from_secs(3)),
            &playback_profile(),
            None,
            StreamingMode::TransportStream,
            OutputMetadata::default(),
            FfmpegCapabilityReport::default(),
            Some(source_320p()),
            Vec::new(),
            None,
            None,
            vec![(
                InputFile::watermark_looped(logo.to_string_lossy().into_owned()),
                opacity_clock_watermark(expression),
            )],
            Vec::new(),
            Some(clock),
        );
        let arguments = command.arguments.join(" ");
        eprintln!("IPTV opacity clock proof argv: {arguments}");
        assert!(arguments.contains("format=yuva420p|yuva444p|yuva422p,geq="));
        assert!(arguments.contains("(5+T)"), "{arguments}");
        assert!(arguments.contains("(30+T)"), "{arguments}");
        assert!(arguments.contains("mod((86399.5+T),86400)"), "{arguments}");
        assert!(arguments.contains("lt((5+T),12)"), "{arguments}");

        let output = run_ffmpeg_to_output(command).await;
        std::fs::write(&stream, &output.stdout).expect("write mpegts sample");
        let samples = decode_opacity_samples(&ffmpeg, &stream).await;
        assert_black(samples[0], "before midnight wrap and threshold");
        assert_red(samples[1], "after content/channel/time thresholds");
        assert_black(samples[2], "after one-second clock window");
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
            Duration::ZERO,
            &playback_profile(),
            None,
            StreamingMode::TransportStream,
            OutputMetadata::default(),
            FfmpegCapabilityReport::default(),
            None,
            Vec::new(),
            None,
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

    async fn run_ffmpeg_to_output(command: FfmpegCommand) -> std::process::Output {
        let mut process = Command::new(&command.executable);
        process
            .args(&command.arguments)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        for variable in command.environment {
            process.env(variable.name, variable.value);
        }
        apply_ffmpeg_library_path(&mut process);
        let output = process.output().await.expect("run ffmpeg stream");
        assert!(
            output.status.success(),
            "ffmpeg stream failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!output.stdout.is_empty(), "expected stream bytes");
        output
    }

    async fn generate_opacity_input(ffmpeg: &str, input: &Path) {
        let mut command = tokio::process::Command::new(ffmpeg);
        command.args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=c=black:s=320x180:r=25:d=8",
            "-f",
            "lavfi",
            "-i",
            "anullsrc=channel_layout=stereo:sample_rate=48000",
            "-t",
            "8",
            "-shortest",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
            input.to_string_lossy().as_ref(),
        ]);
        apply_ffmpeg_library_path(&mut command);
        let generated = command.output().await.expect("generate opacity input");
        assert!(
            generated.status.success(),
            "ffmpeg input generation failed: {}",
            String::from_utf8_lossy(&generated.stderr)
        );
    }

    async fn decode_opacity_samples(ffmpeg: &str, stream: &Path) -> Vec<[u8; 3]> {
        let mut command = tokio::process::Command::new(ffmpeg);
        command.args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-i",
            stream.to_string_lossy().as_ref(),
            "-vf",
            "select=eq(n\\,5)+eq(n\\,20)+eq(n\\,45),crop=80:80:0:0,scale=1:1,format=rgb24",
            "-vsync",
            "0",
            "-frames:v",
            "3",
            "-f",
            "rawvideo",
            "pipe:1",
        ]);
        apply_ffmpeg_library_path(&mut command);
        let decoded = command.output().await.expect("decode opacity samples");
        assert!(
            decoded.status.success(),
            "ffmpeg decode failed: {}",
            String::from_utf8_lossy(&decoded.stderr)
        );
        assert_eq!(decoded.stdout.len(), 9, "expected three RGB samples");
        decoded
            .stdout
            .chunks_exact(3)
            .map(|chunk| [chunk[0], chunk[1], chunk[2]])
            .collect()
    }

    fn opacity_clock_watermark(expression: &str) -> WatermarkOverlay {
        WatermarkOverlay {
            location: WatermarkLocation::TopLeft,
            size: WatermarkSize::Scaled,
            width_percent: 10.0,
            horizontal_margin_percent: 0.0,
            vertical_margin_percent: 0.0,
            opacity: 100,
            opacity_expression: Some(expression.to_owned()),
            ..WatermarkOverlay::default()
        }
    }

    fn assert_red(rgb: [u8; 3], context: &str) {
        assert!(
            rgb[0] > 180 && rgb[1] < 80 && rgb[2] < 80,
            "expected red sample for {context}, got rgb={rgb:?}"
        );
    }

    fn assert_black(rgb: [u8; 3], context: &str) {
        assert!(
            rgb[0] < 40 && rgb[1] < 40 && rgb[2] < 40,
            "expected black sample for {context}, got rgb={rgb:?}"
        );
    }

    fn apply_ffmpeg_library_path(command: &mut Command) {
        let ffmpeg_path = ersatzrs_ffmpeg::ffmpeg_path();
        let Some(bin_dir) = Path::new(&ffmpeg_path).parent() else {
            return;
        };
        let Some(root_dir) = bin_dir.parent() else {
            return;
        };
        let lib_dir = root_dir.join("lib");
        if lib_dir.is_dir() {
            command.env("LD_LIBRARY_PATH", lib_dir);
        }
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

    fn source_320p() -> SourceVideoStream {
        let mut source = SourceVideoStream::new(ersatzrs_ffmpeg::SourceVideoCodec::H264);
        source.frame_size = Some(ersatzrs_ffmpeg::FrameSize::new(320, 180));
        source.sample_aspect_ratio = Some("1:1".to_owned());
        source.display_aspect_ratio = Some("16:9".to_owned());
        source.pixel_format = Some(ersatzrs_ffmpeg::PixelFormat::Yuv420P);
        source
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
