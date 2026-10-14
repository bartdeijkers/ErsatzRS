#[path = "stream_external.rs"]
mod external;

#[path = "stream_ordinary_teletext.rs"]
mod ordinary_teletext;

// POSIX process fixtures complement, but do not replace, native Windows acceptance.
#[cfg(all(test, unix))]
#[path = "stream_teletext_tests.rs"]
mod teletext_tests;

#[cfg(all(test, unix))]
#[path = "stream_retained_teletext_tests.rs"]
mod retained_teletext_tests;

#[cfg(test)]
#[path = "stream_wrapper_teletext_runtime_tests.rs"]
mod wrapper_teletext_runtime_tests;

#[cfg(test)]
#[path = "stream_wrapper_admission_runtime_tests.rs"]
mod wrapper_admission_runtime_tests;

#[cfg(test)]
use std::process::Stdio;
use std::{
    path::PathBuf,
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
    build_mpegts_concat_command, build_mpegts_segmenter_wrapper_command, ConcatPlaylist,
    DesiredVideoState, FfmpegCapabilityReport, FfmpegCommand, FfmpegRuntimeReport, FrameRate,
    FrameSize, GraphicsPipelineOverlay, InputFile, MpegTsMuxingOptions, OutputFormat,
    OutputMetadata, PipelineRequest, SourceAudioCodec, SourceAudioStream, SourceVideoCodec,
    SourceVideoStream, WatermarkOpacityClock, MPEGTS_PCR_PERIOD_MS,
};
use ersatzrs_infra::addons::AddonRuntimeContext;
use ersatzrs_infra::channel_playout::load_channel_playout_context;
use ersatzrs_infra::db::DbPool;
use ersatzrs_infra::dynamic_text::{shutdown_dynamic_text_controllers, DynamicTextController};
use ersatzrs_infra::ffmpeg_capabilities::{
    learned_hardware_decode_denial, persist_hardware_decode_denial_into_runtime,
};
use ersatzrs_infra::ffmpeg_config::playback_language_preferences;
use ersatzrs_infra::file_system_layout;
use ersatzrs_infra::hls_worker::{
    build_mpeg_ts_slate_command, MpegTsSlateCommandRequest, ALL_STREAMS_IN_USE_SLATE_MESSAGE,
};
use ersatzrs_infra::mpegts_scripts::{MpegTsScript, MpegTsScriptDefinition};
use ersatzrs_infra::playback::{
    apply_automatic_null_audio, enrich_source_video_stream_for_playback,
    load_channel_playback_profile, load_channel_target_frame_rate, load_source_audio_streams,
    load_source_video_stream_by_index, render_playback_graphics_overlays,
    select_channel_playback_subtitle, select_custom_playback_streams,
    select_pinned_playback_subtitle, select_playback_audio_stream_index, select_playback_graphics,
    select_playback_watermarks, subtitle_fonts_dir, subtitle_input_for_stream,
    watermark_inputs_for_stream, CustomStreamSelection, PlaybackAudioSelection,
    PlaybackGraphicsArtifacts, PlaybackGraphicsRenderContext, PlaybackProfile, PlaybackSubtitle,
};
use ersatzrs_infra::published_interval_source::IntervalPlaybackContext;
use ersatzrs_infra::remote_stream::{
    decode_persisted_liveness, redact_remote_stream_output, remote_stream_input_file,
    remote_stream_playback_start, remote_stream_producer, resolve_remote_stream_with_addons,
    spawn_ffmpeg_with_remote_input, RemoteStreamCommand, RemoteStreamDefinition,
    RemoteStreamProcess, RemoteStreamSource, RemoteStreamSpawnContext, ResolvedRemoteStream,
};
use ersatzrs_infra::remote_stream::{
    spawn_ffmpeg_with_prepared_remote_input, PreparedRemoteStreamInput,
};
use ersatzrs_infra::subtitles::rebase_burned_text_subtitle_input;
use ersatzrs_infra::transcode_capacity::{
    load_active_transcode_limit, request_active_transcode_reclaim, try_reserve_active_transcode,
    try_reserve_mpeg_ts_wrapper_capacity, ActiveTranscodeReservation, ActiveTranscodeSlot,
    DEFAULT_ACTIVE_TRANSCODE_LIMIT,
};
use sqlx::Row;
use tokio::io::AsyncReadExt;
#[cfg(test)]
use tokio::process::Command;
use tokio::sync::{Notify, RwLock};
use tokio_util::io::ReaderStream;

use super::legacy_wrapper_admission::{
    LegacyWrapperCloseHandle, LegacyWrapperItemCancellation, LegacyWrapperItemGuard,
    LegacyWrapperOwner,
};
use super::retained_stream_buffer::{
    forward_retained_ts, ReservePolicy, TERMINAL_FAILURE, TERMINAL_SUCCESS,
};
use super::teletext_accepted_output::{
    forward_accepted_teletext_output, TeletextAcceptedOutputIntent,
};
use super::teletext_binding::{TeletextOutputAdmission, TeletextOutputBinding, TeletextOutputRole};
use super::teletext_output::TeletextPumpError;
use crate::host::state::AppState;
use crate::transport::iptv::access_token::AccessToken;
use crate::transport::iptv::{hls, hls::HlsQuery, LegacyWrapperQuery};

static NEXT_GRAPHICS_REQUEST_ID: AtomicU64 = AtomicU64::new(1);
const FFMPEG_STDERR_TAIL_BYTES: usize = 16 * 1024;
const DEFAULT_MPEG_TS_SCRIPT_ID: &str = "default";
const MPEG_TS_CAPACITY_SLATE_DURATION: Duration = Duration::from_secs(4);
const MPEG_TS_CAPACITY_RETRY_INTERVAL: Duration = Duration::from_millis(250);
const MPEG_TS_HLS_RECLAIM_WATCH_WINDOW: Duration = Duration::from_secs(30);
const MPEG_TS_HLS_STALE_WATCHED_PREEMPTION_THRESHOLD: Duration = Duration::from_secs(5);
const MPEG_TS_HLS_RECLAIM_GRACE: Duration = Duration::from_secs(6);
const MPEG_TS_HLS_RECLAIMED_REACQUIRE_DELAY: Duration = Duration::from_millis(500);

fn teletext_output_unavailable() -> Response {
    (
        StatusCode::UNSUPPORTED_MEDIA_TYPE,
        "teletext is unavailable for this output",
    )
        .into_response()
}

fn admit_stream_teletext(
    role: TeletextOutputRole,
    binding: Option<Arc<TeletextOutputBinding>>,
) -> Result<TeletextOutputAdmission, TeletextPumpError> {
    match binding {
        Some(binding) => TeletextOutputAdmission::with_binding(role, binding),
        None => Ok(TeletextOutputAdmission::plain(role)),
    }
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
    serve_transport_stream_with_binding(state, channel_number, headers, query, None).await
}

/// Serve a final MPEG-TS wrapper with an immutable authored-page snapshot.
/// Publication selection remains the host caller's responsibility.
pub async fn serve_transport_stream_with_teletext(
    state: &AppState,
    channel_number: &str,
    headers: &HeaderMap,
    query: &HlsQuery,
    binding: Arc<TeletextOutputBinding>,
) -> Response {
    serve_transport_stream_with_binding(state, channel_number, headers, query, Some(binding)).await
}

async fn serve_transport_stream_with_binding(
    state: &AppState,
    channel_number: &str,
    _headers: &HeaderMap,
    query: &HlsQuery,
    binding: Option<Arc<TeletextOutputBinding>>,
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

    let mode = channel_transport_stream_mode(query.mode.as_deref(), &channel);

    if binding.is_some() && matches!(mode, TransportStreamMode::RedirectToHls) {
        return teletext_output_unavailable();
    }
    let Ok(output) = admit_stream_teletext(TeletextOutputRole::FinalWrapperMpegTs, binding) else {
        return teletext_output_unavailable();
    };

    if !matches!(mode, TransportStreamMode::RedirectToHls) {
        if let Err(response) = super::validate_profile_output(
            state,
            channel.id,
            OutputFormat::MpegTs,
            tokio::time::Instant::now() + Duration::from_secs(20),
        )
        .await
        {
            return response;
        }
    }

    match mode {
        TransportStreamMode::RedirectToHls => {
            let access_token = AccessToken::new(query.access_token.clone());
            let target = format!(
                "/iptv/channel/{channel_number}.m3u8{}",
                access_token.query()
            );
            axum::response::Redirect::temporary(&target).into_response()
        }
        TransportStreamMode::Legacy => serve_legacy_concat_stream(state, &channel, output).await,
        TransportStreamMode::HybridWrapper => {
            serve_segmenter_wrapper_stream(state, &channel, query, output).await
        }
    }
}

pub async fn serve_channel_stream(
    state: &AppState,
    channel_number: &str,
    output_format: OutputFormat,
) -> Response {
    let role = if output_format == OutputFormat::MpegTs {
        TeletextOutputRole::FinalDirectMpegTs
    } else {
        TeletextOutputRole::OtherContainer
    };
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    serve_channel_stream_with_output(
        state,
        channel_number,
        output_format,
        TeletextOutputAdmission::plain(role),
        deadline,
        true,
    )
    .await
}

/// Serve a final direct stream with an immutable authored-page snapshot.
/// Container and retained-output admission are checked before producer startup.
pub async fn serve_channel_stream_with_teletext(
    state: &AppState,
    channel_number: &str,
    output_format: OutputFormat,
    binding: Arc<TeletextOutputBinding>,
) -> Response {
    serve_channel_stream_with_binding(state, channel_number, output_format, Some(binding)).await
}

/// Serve accepted native originals through the actual final direct producer.
/// No package is invoked. Ordinary routes require explicit host policy to opt in.
/// Channel identity is checked before startup; current selection, command policy
/// and actual PSI are checked again by the final stdout owner. Retained and
/// hybrid output need a separate command-provenance seam and are rejected here.
pub async fn serve_channel_stream_with_accepted_teletext(
    state: &AppState,
    channel_number: &str,
    output_format: OutputFormat,
    intent: Arc<TeletextAcceptedOutputIntent>,
) -> Response {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    let Ok(output) = TeletextOutputAdmission::with_accepted(
        TeletextOutputRole::FinalDirectMpegTs,
        intent,
        Arc::clone(&state.application),
        deadline,
    ) else {
        return teletext_output_unavailable();
    };
    serve_channel_stream_with_output(
        state,
        channel_number,
        output_format,
        output,
        deadline,
        false,
    )
    .await
}

async fn serve_channel_stream_with_binding(
    state: &AppState,
    channel_number: &str,
    output_format: OutputFormat,
    binding: Option<Arc<TeletextOutputBinding>>,
) -> Response {
    let role = if output_format == OutputFormat::MpegTs {
        TeletextOutputRole::FinalDirectMpegTs
    } else {
        TeletextOutputRole::OtherContainer
    };
    let Ok(output) = admit_stream_teletext(role, binding) else {
        return teletext_output_unavailable();
    };
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    serve_channel_stream_with_output(
        state,
        channel_number,
        output_format,
        output,
        deadline,
        false,
    )
    .await
}

async fn serve_channel_stream_with_output(
    state: &AppState,
    channel_number: &str,
    output_format: OutputFormat,
    mut output: TeletextOutputAdmission,
    deadline: tokio::time::Instant,
    recover_selected_teletext: bool,
) -> Response {
    if output.validate_format(output_format).is_err() {
        return teletext_output_unavailable();
    }
    let lookup = tokio::time::timeout_at(
        deadline,
        state.repositories.channel.get_by_number(channel_number),
    )
    .await;
    let Ok(lookup) = lookup else {
        tracing::warn!("stream channel discovery exceeded startup deadline");
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let channel = match lookup {
        Ok(channel) if channel.is_enabled => channel,
        Ok(_) => return StatusCode::NOT_FOUND.into_response(),
        Err(error) => {
            tracing::debug!(%error, channel_number, "channel not found for stream");
            return StatusCode::NOT_FOUND.into_response();
        }
    };

    if !output.matches_channel(channel.id)
        || ((output.accepted().is_some() || output.live().is_some())
            && channel.streaming_mode == StreamingMode::TransportStreamHybrid)
    {
        return teletext_output_unavailable();
    }

    let effective_streaming_mode = channel.streaming_mode;
    if recover_selected_teletext
        && output_format == OutputFormat::MpegTs
        && effective_streaming_mode != StreamingMode::TransportStreamHybrid
        && channel.streaming_engine != ersatzrs_domain::StreamingEngine::Next
    {
        if let Some(accepted) =
            ordinary_teletext::recover_optional_output(state, channel.id, deadline).await
        {
            output = accepted;
        }
    }
    serve_channel_stream_with_channel(
        state,
        channel,
        output_format,
        effective_streaming_mode,
        deadline,
        output,
    )
    .await
}

pub async fn serve_concat_playlist(
    state: &AppState,
    channel_number: &str,
    _headers: &HeaderMap,
    query: &LegacyWrapperQuery,
) -> Response {
    if let Some(id) = query.wrapper_id {
        if state.legacy_wrappers.lookup(id, channel_number).is_err() {
            return StatusCode::NOT_FOUND.into_response();
        }
    }
    let Some(internal_addr) = state.internal_http_addr.get() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let channel = match state
        .repositories
        .channel
        .get_by_number(channel_number)
        .await
    {
        Ok(channel) if channel.is_enabled => channel,
        Ok(_) => return StatusCode::NOT_FOUND.into_response(),
        Err(ersatzrs_core::repositories::RepositoryError::NotFound { .. }) => {
            return StatusCode::NOT_FOUND.into_response();
        }
        Err(_) => {
            tracing::error!("concat channel lookup failed");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    let mode = query
        .mode
        .as_deref()
        .map(str::trim)
        .filter(|mode| !mode.is_empty())
        .unwrap_or("ts-legacy");
    let playlist = ConcatPlaylist::new("http", internal_addr.to_string(), channel.number, mode);

    let rendered = query.wrapper_id.map_or_else(
        || playlist.render(),
        |id| playlist.render_with_wrapper_id(id.as_u128()),
    );
    let mut response = rendered.into_response();
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
    serve_internal_legacy_stream_with_query(state, channel_number, &LegacyWrapperQuery::default())
        .await
}

pub async fn serve_internal_legacy_stream_with_query(
    state: &AppState,
    channel_number: &str,
    query: &LegacyWrapperQuery,
) -> Response {
    let item = if let Some(id) = query.wrapper_id {
        let Ok(request) = state.legacy_wrappers.lookup(id, channel_number) else {
            return StatusCode::NOT_FOUND.into_response();
        };
        let Ok(item) = request.begin_item() else {
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        };
        Some(item)
    } else {
        None
    };
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    let lookup = tokio::time::timeout_at(
        deadline,
        state.repositories.channel.get_by_number(channel_number),
    )
    .await;
    let Ok(lookup) = lookup else {
        tracing::warn!("internal stream channel discovery exceeded startup deadline");
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let channel = match lookup {
        Ok(channel) if channel.is_enabled => channel,
        Ok(_) => return StatusCode::NOT_FOUND.into_response(),
        Err(error) => {
            tracing::debug!(%error, channel_number, "channel not found for internal legacy stream");
            return StatusCode::NOT_FOUND.into_response();
        }
    };

    if let Some((guard, first_encoder)) = item {
        let cancellation = guard.cancellation();
        if cancellation.is_cancelled() {
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        }
        let reservation = match first_encoder {
            Some(reservation) => reservation,
            None => match reserve_mpeg_ts_capacity(state, channel_number).await {
                Ok(Some(reservation)) => reservation,
                // Compatible finite slate topology is not yet established.
                // Do not open a source or feed a restarted capacity epoch.
                _ => return StatusCode::SERVICE_UNAVAILABLE.into_response(),
            },
        };
        if cancellation.is_cancelled() {
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        }
        if channel.streaming_engine == ersatzrs_domain::StreamingEngine::Next {
            return StatusCode::NOT_FOUND.into_response();
        }
        if let Err(response) =
            super::validate_profile_output(state, channel.id, OutputFormat::MpegTs, deadline).await
        {
            return response;
        }
        let close = guard.close_handle();
        let owned_state = state.clone();
        return owned_legacy_item_startup(cancellation, close, move |receiver| async move {
            serve_channel_stream_admitted(
                &owned_state,
                channel,
                OutputFormat::MpegTs,
                StreamingMode::TransportStream,
                deadline,
                Some(IntervalStreamAdmission {
                    reservation,
                    deadline,
                    cancellation: receiver,
                    teletext: None,
                    retained_required: false,
                    legacy_item: Some(guard),
                }),
                TeletextOutputAdmission::plain(TeletextOutputRole::InternalItem),
            )
            .await
        })
        .await;
    }

    serve_channel_stream_with_channel(
        state,
        channel,
        OutputFormat::MpegTs,
        StreamingMode::TransportStream,
        deadline,
        TeletextOutputAdmission::plain(TeletextOutputRole::InternalItem),
    )
    .await
}

/// Keep managed startup alive until cooperative cleanup even if its internal
/// HTTP handler disappears before it can return a response body.
async fn owned_legacy_item_startup<F, Fut>(
    cancellation: LegacyWrapperItemCancellation,
    close: LegacyWrapperCloseHandle,
    build: F,
) -> Response
where
    F: FnOnce(tokio::sync::oneshot::Receiver<()>) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = Response> + Send + 'static,
{
    let (mut response_sender, response_receiver) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        let (sender, receiver) = tokio::sync::oneshot::channel();
        let operation = build(receiver);
        tokio::pin!(operation);
        let response = tokio::select! {
            biased;
            () = cancellation.cancelled() => {
                close.close();
                drop(sender);
                operation.await
            },
            () = response_sender.closed() => {
                close.close();
                drop(sender);
                operation.await
            },
            response = &mut operation => response,
        };
        // A failed send drops the body, preserving its supervised disposal.
        let _ = response_sender.send(response);
    });
    response_receiver
        .await
        .unwrap_or_else(|_| StatusCode::SERVICE_UNAVAILABLE.into_response())
}

#[allow(clippy::too_many_lines)]
async fn serve_channel_stream_with_channel(
    state: &AppState,
    channel: Channel,
    output_format: OutputFormat,
    effective_streaming_mode: StreamingMode,
    deadline: tokio::time::Instant,
    output: TeletextOutputAdmission,
) -> Response {
    if output.validate_format(output_format).is_err() {
        return teletext_output_unavailable();
    }
    if channel.streaming_engine == ersatzrs_domain::StreamingEngine::Next {
        return StatusCode::NOT_FOUND.into_response();
    }

    if effective_streaming_mode != StreamingMode::HttpLiveStreamingDirect {
        if let Err(response) =
            super::validate_profile_output(state, channel.id, output_format, deadline).await
        {
            return response;
        }
    }

    serve_channel_stream_admitted(
        state,
        channel,
        output_format,
        effective_streaming_mode,
        deadline,
        None,
        output,
    )
    .await
}

struct IntervalStreamAdmission {
    reservation: ActiveTranscodeReservation,
    deadline: tokio::time::Instant,
    cancellation: tokio::sync::oneshot::Receiver<()>,
    teletext: Option<Arc<TeletextOutputBinding>>,
    retained_required: bool,
    legacy_item: Option<LegacyWrapperItemGuard>,
}

async fn await_retained_startup_stage<T, E>(
    cancellation: &mut tokio::sync::oneshot::Receiver<()>,
    sender: tokio::sync::oneshot::Sender<()>,
    operation: impl std::future::Future<Output = Result<T, E>>,
) -> (bool, Result<T, E>) {
    tokio::pin!(operation);
    tokio::select! {
        biased;
        _ = cancellation => {},
        result = &mut operation => return (false, result),
    }
    drop(sender);
    (true, operation.await)
}

#[derive(Clone)]
struct RetainedTsOutput {
    muxrate_bps: u64,
    remaining: Option<Duration>,
    terminal: Arc<std::sync::atomic::AtomicU8>,
    completed: Arc<Notify>,
}

impl RetainedTsOutput {
    fn from_command(
        command: &FfmpegCommand,
        remaining: Option<Duration>,
    ) -> Result<Self, &'static str> {
        let muxrate_bps = command
            .arguments
            .windows(2)
            .find(|pair| pair[0] == "-muxrate")
            .and_then(|pair| pair[1].parse::<u64>().ok())
            .ok_or("retained MPEG-TS requires a known constant mux rate")?;
        ReservePolicy::new(muxrate_bps, remaining)
            .map_err(|_| "retained MPEG-TS profile exceeds the encoded reserve bound")?;
        Ok(Self {
            muxrate_bps,
            remaining,
            terminal: Arc::new(std::sync::atomic::AtomicU8::new(0)),
            completed: Arc::new(Notify::new()),
        })
    }
}

fn serve_channel_stream_admitted(
    state: &AppState,
    channel: Channel,
    output_format: OutputFormat,
    mode: StreamingMode,
    deadline: tokio::time::Instant,
    admission: Option<IntervalStreamAdmission>,
    output: TeletextOutputAdmission,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Response> + Send + '_>> {
    Box::pin(serve_channel_stream_admitted_inner(
        state,
        channel,
        output_format,
        mode,
        deadline,
        admission,
        output,
    ))
}

/// Source-free capacity waiting; dropping the response signals the owned task.
// Keep admission, setup cancellation and response forwarding under one owner.
#[allow(clippy::too_many_lines)]
fn retained_interval_stream_response(
    state: AppState,
    channel: Channel,
    output_format: OutputFormat,
    mode: StreamingMode,
    deadline: tokio::time::Instant,
    output: TeletextOutputAdmission,
) -> Response {
    if output_format != OutputFormat::MpegTs {
        tracing::warn!(
            channel_id = channel.id,
            "retained interval byte reserve requires MPEG-TS output"
        );
        return (
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "retained interval playback currently requires MPEG-TS output",
        )
            .into_response();
    }
    let (reader, mut writer) = tokio::io::duplex(STREAM_BODY_BUFFER_BYTES);
    let cancel = Arc::new(Notify::new());
    let (body, output_completion) = stream_body_for_output(reader, cancel.clone(), &output);
    tokio::spawn(async move {
        let final_binding = output.cloned_binding();
        let setup = load_capacity_slate_context(&state, &channel, "retained interval capacity");
        let context = tokio::select! {
            biased;
            () = cancel.notified() => return,
            () = tokio::time::sleep_until(deadline) => return,
            result = setup => result,
        };
        let Ok((profile, frame_rate, capabilities)) = context else {
            return;
        };
        let slate_dir = mpeg_ts_capacity_slate_dir(channel.id);
        let mut capacity_command = None;
        let mut slate_cleanup_confirmed = true;
        let mut output_result = Err("retained output did not start");
        let reservation = loop {
            let limit = tokio::select! {
                biased;
                () = cancel.notified() => break None,
                () = tokio::time::sleep_until(deadline) => break None,
                result = load_active_transcode_limit(&state.pool) => match result {
                    Ok(limit) => limit.unwrap_or(DEFAULT_ACTIVE_TRANSCODE_LIMIT),
                    Err(_) => break None,
                },
            };
            if let Some(reservation) = try_reserve_active_transcode(
                limit,
                ActiveTranscodeSlot::new_mpeg_ts(&channel.number),
            ) {
                break Some(reservation);
            }
            request_hls_capacity_reclaim_for_mpeg_ts(&channel.number);
            if capacity_command.is_none() {
                capacity_command = build_mpeg_ts_slate_command(MpegTsSlateCommandRequest {
                    working_dir: &slate_dir,
                    duration: MPEG_TS_CAPACITY_SLATE_DURATION,
                    playback_profile: &profile,
                    target_frame_rate: frame_rate,
                    metadata: OutputMetadata::default(),
                    runtime_capabilities: capabilities.clone(),
                    message: ALL_STREAMS_IN_USE_SLATE_MESSAGE,
                })
                .await;
            }
            let Some(command) = &capacity_command else {
                break None;
            };
            let Ok(mut child) = spawn_stream_process(command, None, &channel.number) else {
                break None;
            };
            let operation =
                supervise_stream_process(&mut child, &mut writer, &cancel, &channel.number, None);
            tokio::pin!(operation);
            let outcome = tokio::select! {
                result = &mut operation => result,
                () = tokio::time::sleep_until(deadline) => {
                    cancel.notify_one();
                    operation.await
                }
            };
            if matches!(outcome, DirectStreamOutcome::CleanupUnconfirmed) {
                slate_cleanup_confirmed = false;
            }
            if !matches!(outcome, DirectStreamOutcome::Completed) {
                break None;
            }
        };
        if let Some(reservation) = reservation {
            let (sender, receiver) = tokio::sync::oneshot::channel();
            // Box this reuse of the ordinary builder: no second metadata/graphics path.
            let operation = Box::pin(serve_channel_stream_admitted(
                &state,
                channel.clone(),
                output_format,
                mode,
                deadline,
                Some(IntervalStreamAdmission {
                    reservation,
                    deadline,
                    cancellation: receiver,
                    teletext: output.cloned_binding(),
                    retained_required: true,
                    legacy_item: None,
                }),
                TeletextOutputAdmission::plain(TeletextOutputRole::InternalItem),
            ));
            tokio::pin!(operation);
            let response = tokio::select! {
                biased;
                () = cancel.notified() => {
                    drop(writer); drop(sender); drop(operation.await);
                    cleanup_stream_artifact(&slate_dir).await; return;
                },
                () = tokio::time::sleep_until(deadline) => {
                    tracing::warn!(channel_id = channel.id, "retained MPEG-TS startup deadline expired");
                    drop(writer); drop(sender); drop(operation.await);
                    cleanup_stream_artifact(&slate_dir).await; return;
                },
                response = &mut operation => Some(response),
            };
            if let Some(response) = response {
                if response.status().is_success() {
                    if let Some(output) = response.extensions().get::<RetainedTsOutput>().cloned() {
                        output_result = forward_retained_response(
                            response,
                            writer,
                            &output,
                            deadline,
                            &cancel,
                            // The inner child owns raw output; this final writer owns pages.
                            final_binding.as_deref(),
                        )
                        .await;
                        if let Err(reason) = output_result {
                            tracing::warn!(
                                channel_id = channel.id,
                                reason,
                                "retained MPEG-TS reserve ended"
                            );
                        }
                    } else {
                        tracing::warn!(
                            channel_id = channel.id,
                            "retained MPEG-TS response has no verified rate contract"
                        );
                    }
                } else {
                    tracing::warn!(channel_id = channel.id, status = %response.status(),
                        "retained interval startup was refused");
                }
            }
        }
        if slate_cleanup_confirmed {
            cleanup_stream_artifact(&slate_dir).await;
        }
        if let Some(completion) = output_completion {
            let _ = completion.send(output_result);
        }
    });
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

async fn forward_retained_response(
    response: Response,
    mut writer: tokio::io::DuplexStream,
    output: &RetainedTsOutput,
    deadline: tokio::time::Instant,
    cancel: &Notify,
    binding: Option<&TeletextOutputBinding>,
) -> Result<(), &'static str> {
    use futures_util::StreamExt as _;

    let stream = response
        .into_body()
        .into_data_stream()
        .map(|chunk| chunk.map_err(std::io::Error::other));
    let mut source = tokio_util::io::StreamReader::new(stream);
    let result = match ReservePolicy::new(output.muxrate_bps, output.remaining) {
        Ok(policy) => {
            if let Some(binding) = binding {
                super::teletext_optional_output::forward_bound_retained_teletext_output(
                    &mut source,
                    &mut writer,
                    policy,
                    &output.terminal,
                    deadline,
                    cancel,
                    binding,
                )
                .await
                .map_err(|_| "retained teletext transport failed")
            } else {
                forward_retained_ts(
                    &mut source,
                    &mut writer,
                    policy,
                    &output.terminal,
                    deadline,
                    cancel,
                )
                .await
                .map_err(|_| "retained MPEG-TS reserve failed")
            }
        }
        Err(_) => Err("retained MPEG-TS profile exceeds the encoded reserve bound"),
    };
    drop(writer);
    drop(source);
    if output.terminal.load(Ordering::Acquire) == 0 {
        output.completed.notified().await;
    }
    if output.terminal.load(Ordering::Acquire) != TERMINAL_SUCCESS {
        return Err("retained encoded output producer failed");
    }
    result
}

/// Release startup-owned artifacts only after the prepared producer stopped.
async fn cancel_retained_startup(
    prepared: Option<PreparedRemoteStreamInput>,
    reservation: Option<&ActiveTranscodeReservation>,
    subtitle_owner: Option<&mut Option<ersatzrs_infra::external_json::subtitles::PreparedSubtitle>>,
    cleanup_paths: &[PathBuf],
    controllers: Option<&mut Vec<DynamicTextController>>,
) -> bool {
    let confirmed = match prepared {
        Some(prepared) => prepared.cancel().await.is_ok(),
        None => true,
    };
    if !confirmed {
        tracing::warn!("retained startup producer cleanup could not be confirmed");
        if let Some(reservation) = reservation {
            reservation.retain_until_restart();
        }
        if let Some(owner) = subtitle_owner.and_then(Option::take) {
            owner.preserve_after_unconfirmed_cleanup();
        }
    }
    if let Some(controllers) = controllers {
        shutdown_dynamic_text_controllers(std::mem::take(controllers)).await;
    }
    if confirmed {
        cleanup_stream_artifacts(cleanup_paths).await;
    }
    confirmed
}

#[allow(clippy::too_many_lines)]
async fn serve_channel_stream_admitted_inner(
    state: &AppState,
    channel: Channel,
    output_format: OutputFormat,
    effective_streaming_mode: StreamingMode,
    startup_deadline: tokio::time::Instant,
    mut interval_admission: Option<IntervalStreamAdmission>,
    mut output: TeletextOutputAdmission,
) -> Response {
    if output.validate_format(output_format).is_err() {
        return teletext_output_unavailable();
    }
    let mut legacy_item = interval_admission
        .as_mut()
        .and_then(|admission| admission.legacy_item.take());
    let legacy_cancellation = legacy_item
        .as_ref()
        .map(LegacyWrapperItemGuard::cancellation);
    if legacy_cancellation
        .as_ref()
        .is_some_and(LegacyWrapperItemCancellation::is_cancelled)
    {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    let retained_teletext = interval_admission
        .as_ref()
        .and_then(|admission| admission.teletext.clone());
    let now = Utc::now();
    let active = tokio::time::timeout_at(
        startup_deadline,
        external::load(state, channel.id, now, startup_deadline),
    )
    .await;
    let Ok(active) = active else {
        tracing::warn!(
            channel_id = channel.id,
            "stream source discovery exceeded startup deadline"
        );
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let active = match active {
        Ok(Some(path)) => path,
        Ok(None) => {
            tracing::info!(
                channel_number = %channel.number,
                "no active playout item; returning 503"
            );
            return (StatusCode::SERVICE_UNAVAILABLE, "no active playout item").into_response();
        }
        Err(
            error @ RepositoryError::Unavailable {
                operation: "external JSON playback",
                ..
            },
        ) => {
            tracing::warn!(%error, channel_id = channel.id, "external JSON programme is unavailable");
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        }
        Err(error) => {
            tracing::error!(%error, channel_number = %channel.number, "failed to load active playout item");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    if legacy_cancellation
        .as_ref()
        .is_some_and(LegacyWrapperItemCancellation::is_cancelled)
    {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    let interval_source = active
        .remote_stream
        .as_ref()
        .and_then(|stream| match &stream.source {
            RemoteStreamSource::RetainedIntervals(source) => Some(source.clone()),
            _ => None,
        });
    if interval_admission
        .as_ref()
        .is_some_and(|admission| admission.retained_required)
        && interval_source.is_none()
    {
        tracing::warn!(
            channel_id = channel.id,
            "retained source changed during capacity admission"
        );
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    // Accepted originals need the actual retained attempt's first-byte command
    // identity. The current extension certifies only the initial rate, so never
    // attach an intent to its outer response or start a producer on that claim.
    if interval_source.is_some() {
        output = output.without_optional_accepted();
    }
    if interval_source.is_some() && (output.accepted().is_some() || output.live().is_some()) {
        return teletext_output_unavailable();
    }
    if interval_source.is_some() && interval_admission.is_some() && output.has_authored_output() {
        return teletext_output_unavailable();
    }
    if interval_source.is_some() && interval_admission.is_none() {
        return retained_interval_stream_response(
            state.clone(),
            channel,
            output_format,
            effective_streaming_mode,
            startup_deadline,
            output,
        );
    }

    let replacements = if active.external.is_some() {
        Vec::new()
    } else {
        match load_required_path_replacements(state, &channel, "MPEG-TS stream").await {
            Ok(replacements) => replacements,
            Err(response) => return response,
        }
    };

    let resolved =
        if active.external.is_some() || active.remote_stream.is_some() || replacements.is_empty() {
            active.media_path.clone()
        } else {
            replace_path(&replacements, &active.media_path, PathPlatform::Unix)
        };

    let source_stream = match async {
        if let Some(media) = &active.external {
            ersatzrs_infra::playback::source_video_stream_from_probe(&media.probe)
        } else {
            load_source_video_stream_by_index(
                &state.pool,
                active.media_version_id,
                active.video_stream_index,
            )
            .await
        }
    }
    .await
    {
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
    let mut source_audio_streams = match async {
        if let Some(media) = &active.external {
            ersatzrs_infra::playback::source_audio_streams_from_probe(&media.probe)
        } else {
            load_source_audio_streams(&state.pool, active.media_version_id).await
        }
    }
    .await
    {
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
    if let Some(stream_index) = active.audio_stream_index {
        let pinned_index = usize::try_from(stream_index).ok();
        if pinned_index.is_none_or(|index| {
            !source_audio_streams
                .iter()
                .any(|stream| stream.index == index)
        }) {
            tracing::error!(
                channel_id = channel.id,
                media_version_id = active.media_version_id,
                stream_index,
                "pinned audio stream is unavailable; refusing fallback selection"
            );
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    }
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
    let mut interval_reservation = None;
    let mut retained_playback = None;
    let mut retained_cancellation = None;
    let mut external_subtitle_owner = None;
    let previous_audio_streams = source_audio_streams.clone();
    // Fence each source-opening stage after awaiting earlier managed work.
    // Cancellation retains reservations until prepared producers are reaped.
    macro_rules! fence_cancelled_startup {
        ($paths:expr, $controllers:expr) => {
            if legacy_cancellation.as_ref().is_some_and(LegacyWrapperItemCancellation::is_cancelled) {
                cancel_retained_startup(
                    retained_playback.take().map(|playback: ersatzrs_infra::published_interval_source::RetainedIntervalPlayback| playback.prepared),
                    interval_reservation.as_ref(), Some(&mut external_subtitle_owner),
                    $paths, $controllers,
                ).await;
                return StatusCode::SERVICE_UNAVAILABLE.into_response();
            }
        };
    }
    fence_cancelled_startup!(&[], None);

    if interval_source.is_none() {
        if let Some(admission) = interval_admission.take() {
            interval_reservation = Some(admission.reservation);
            retained_cancellation = Some(admission.cancellation);
        }
    }
    let source_stream = if let (Some(source), Some(admission)) =
        (&interval_source, interval_admission)
    {
        interval_reservation = Some(admission.reservation);
        let extractor =
            ersatzrs_infra::ffprobe::FfprobeExtractor::new(ersatzrs_ffmpeg::ffprobe_path());
        let Ok(programme_ms) = u64::try_from(active.content_seek_position(now).as_millis()) else {
            return StatusCode::UNSUPPORTED_MEDIA_TYPE.into_response();
        };
        let mut cancellation = admission.cancellation;
        let (sender, receiver) = tokio::sync::oneshot::channel();
        let (cancelled, result) = Box::pin(await_retained_startup_stage(
            &mut cancellation,
            sender,
            source.prepare_playback_cancellable(
                IntervalPlaybackContext {
                    pool: &state.pool,
                    probe: &extractor,
                    spawn_context: RemoteStreamSpawnContext::mpeg_ts(&channel.number),
                },
                programme_ms,
                u32::try_from(playback_profile.resolution.height).ok(),
                admission.deadline,
                receiver,
            ),
        ))
        .await;
        retained_cancellation = Some(cancellation);
        if matches!(
            &result,
            Err(ersatzrs_infra::addon_media_list_item_runtime::ItemRuntimeError::TerminationUnconfirmed)
        ) {
            if let Some(reservation) = &interval_reservation {
                reservation.retain_until_restart();
            }
        }
        if cancelled {
            if let Ok(Some(playback)) = result {
                cancel_retained_startup(
                    Some(playback.prepared),
                    interval_reservation.as_ref(),
                    Some(&mut external_subtitle_owner),
                    &[],
                    None,
                )
                .await;
            }
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        }
        match result {
            Ok(Some(playback)) => {
                source_audio_streams.clone_from(&playback.source_audio);
                let source = Some(playback.source_video.clone());
                retained_playback = Some(playback);
                source
            }
            Ok(None) => return StatusCode::NO_CONTENT.into_response(),
            Err(error) => {
                tracing::warn!(channel_id = channel.id, %error,
                    "retained interval input preparation failed");
                return (StatusCode::SERVICE_UNAVAILABLE, error.to_string()).into_response();
            }
        }
    } else if active.external.is_some() {
        ersatzrs_infra::playback::enrich_transient_source_video_stream_for_playback(
            &state.pool,
            source_stream,
            &resolved,
            active.content_duration.unwrap_or_default(),
            playback_profile.profile.deinterlace_video == Some(true),
            effective_acceleration,
            startup_deadline,
        )
        .await
    } else {
        enrich_source_video_stream_for_playback(
            &state.pool,
            active.media_version_id,
            playback_profile.profile.deinterlace_video == Some(true),
            source_stream,
            effective_acceleration,
            &resolved,
        )
        .await
    };
    fence_cancelled_startup!(&[], None);
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
                cancel_retained_startup(
                    retained_playback.take().map(|playback| playback.prepared),
                    interval_reservation.as_ref(),
                    Some(&mut external_subtitle_owner),
                    &[],
                    None,
                )
                .await;
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
            cancel_retained_startup(
                retained_playback.take().map(|playback| playback.prepared),
                interval_reservation.as_ref(),
                Some(&mut external_subtitle_owner),
                &[],
                None,
            )
            .await;
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
    let start_position = remote_stream_playback_start(
        active.remote_stream.as_ref(),
        active.content_seek_position(now),
    );
    let start_seconds = duration_as_i64_seconds(start_position);
    let content_start_time = parse_sqlite_utc_timestamp(&active.playout_start).unwrap_or(now);
    fence_cancelled_startup!(&[], None);
    let custom_selection = if active.has_pinned_stream_selection {
        None
    } else {
        match async {
            if let Some(media) = &active.external {
                if channel.stream_selector_mode != ChannelStreamSelectorMode::Custom {
                    return Ok(None);
                }
                media
                    .tracks()
                    .select_custom(
                        &channel,
                        content_start_time,
                        external::policy(&state.pool, effective_streaming_mode, media).await?,
                    )
                    .await
            } else {
                select_channel_custom_streams(
                    &state.pool,
                    active.media_version_id,
                    active.media_item_id,
                    &channel,
                    content_start_time,
                    effective_streaming_mode,
                )
                .await
            }
        }
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
                cancel_retained_startup(
                    retained_playback.take().map(|playback| playback.prepared),
                    interval_reservation.as_ref(),
                    Some(&mut external_subtitle_owner),
                    &[],
                    None,
                )
                .await;
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        }
    };
    let script_audio = if !active.has_pinned_stream_selection
        && custom_selection
            .as_ref()
            .and_then(|selection| selection.audio_stream_index)
            .is_none()
    {
        if let Some(media) = &active.external {
            ersatzrs_infra::audio_script::transient::select(
                &channel,
                media.audio_metadata(),
                ersatzrs_infra::audio_script::AudioScriptPreferences {
                    language: preferred_audio_language.as_deref(),
                    title: preferred_audio_title,
                    streaming_mode: effective_streaming_mode,
                },
                &media.tracks(),
            )
            .await
        } else {
            ersatzrs_infra::audio_script::select(
                &state.pool,
                &channel,
                active.media_item_id,
                active.media_version_id,
                ersatzrs_infra::audio_script::AudioScriptPreferences {
                    language: preferred_audio_language.as_deref(),
                    title: preferred_audio_title,
                    streaming_mode: effective_streaming_mode,
                },
                if interval_source.is_some() {
                    &previous_audio_streams
                } else {
                    &source_audio_streams
                },
            )
            .await
        }
    } else {
        None
    };
    let explicit_audio_selection = if active.has_pinned_stream_selection {
        active
            .audio_stream_index
            .map_or(PlaybackAudioSelection::Disabled, |index| {
                usize::try_from(index).map_or(
                    PlaybackAudioSelection::Disabled,
                    PlaybackAudioSelection::Exact,
                )
            })
    } else {
        custom_selection
            .as_ref()
            .and_then(|selection| selection.audio_stream_index)
            .or(script_audio)
            .map_or(
                PlaybackAudioSelection::Automatic,
                PlaybackAudioSelection::Exact,
            )
    };
    let explicit_audio_selection = if interval_source.is_some() {
        match ersatzrs_infra::playback::remap_pinned_audio_selection(
            explicit_audio_selection,
            &previous_audio_streams,
            &source_audio_streams,
        ) {
            Ok(selection) => selection,
            Err(error) => {
                cancel_retained_startup(
                    retained_playback.take().map(|playback| playback.prepared),
                    interval_reservation.as_ref(),
                    Some(&mut external_subtitle_owner),
                    &[],
                    None,
                )
                .await;
                return (StatusCode::UNSUPPORTED_MEDIA_TYPE, error.to_string()).into_response();
            }
        }
    } else {
        explicit_audio_selection
    };
    let audio_selection = match explicit_audio_selection {
        PlaybackAudioSelection::Automatic => select_playback_audio_stream_index(
            &source_audio_streams,
            preferred_audio_language.as_deref(),
            preferred_audio_title,
        )
        .map_or(
            PlaybackAudioSelection::Automatic,
            PlaybackAudioSelection::Exact,
        ),
        selection => selection,
    };
    let audio_selection = if retained_playback.is_some() {
        retained_audio_selection(audio_selection, &source_audio_streams)
    } else {
        audio_selection
    };
    // SQL/fresh probes and retained transports describe the complete source.
    // Eager-unprobed Remote Streams must not turn unknown audio into silence.
    let source_audio_complete =
        active.external.is_some() || active.remote_stream.is_none() || retained_playback.is_some();
    let audio_selection = if source_audio_complete {
        audio_selection.with_complete_source_audio(&source_audio_streams)
    } else {
        audio_selection
    };
    fence_cancelled_startup!(&[], None);
    let subtitle = if active.has_pinned_stream_selection {
        if let Some(stream_index) = active.subtitle_stream_index {
            match select_pinned_playback_subtitle(
                &state.pool,
                active.media_item_id,
                active.media_version_id,
                stream_index,
                effective_streaming_mode,
            )
            .await
            {
                Ok(subtitle) => Some(subtitle),
                Err(error) => {
                    tracing::error!(
                        %error,
                        channel_id = channel.id,
                        media_item_id = active.media_item_id,
                        media_version_id = active.media_version_id,
                        stream_index,
                        "pinned subtitle stream is unavailable; refusing fallback selection"
                    );
                    cancel_retained_startup(
                        retained_playback.take().map(|playback| playback.prepared),
                        interval_reservation.as_ref(),
                        Some(&mut external_subtitle_owner),
                        &[],
                        None,
                    )
                    .await;
                    return StatusCode::INTERNAL_SERVER_ERROR.into_response();
                }
            }
        } else {
            None
        }
    } else if let Some(selection) = custom_selection {
        selection.subtitle
    } else {
        match async {
            if let Some(media) = &active.external {
                media
                    .tracks()
                    .select_subtitle(
                        &state.pool,
                        &preferred_subtitle_languages,
                        subtitle_mode,
                        external::policy(&state.pool, effective_streaming_mode, media).await?,
                    )
                    .await
            } else {
                select_channel_playback_subtitle(
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
            }
        }
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
                cancel_retained_startup(
                    retained_playback.take().map(|playback| playback.prepared),
                    interval_reservation.as_ref(),
                    Some(&mut external_subtitle_owner),
                    &[],
                    None,
                )
                .await;
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        }
    };
    fence_cancelled_startup!(&[], None);
    let subtitle = match subtitle {
        Some(selected) if active.external.is_some() => {
            let media = active.external.as_ref().expect("external binding checked");
            match external::prepare_subtitle(
                &state.pool,
                media,
                &resolved,
                selected,
                effective_streaming_mode,
                startup_deadline,
            )
            .await
            {
                Ok((subtitle, owner)) => {
                    external_subtitle_owner = owner;
                    Some(subtitle)
                }
                Err(error) => {
                    tracing::warn!(channel_id = channel.id, %error, "external subtitle preparation failed");
                    cancel_retained_startup(
                        retained_playback.take().map(|playback| playback.prepared),
                        interval_reservation.as_ref(),
                        Some(&mut external_subtitle_owner),
                        &[],
                        None,
                    )
                    .await;
                    return StatusCode::SERVICE_UNAVAILABLE.into_response();
                }
            }
        }
        Some(selected) => {
            let Ok(Ok(prepared)) = tokio::time::timeout_at(
                startup_deadline,
                ersatzrs_infra::provider_subtitles::prepare(
                    &state.pool,
                    &selected,
                    &ersatzrs_infra::file_system_layout::subtitle_seek_cache_folder(),
                ),
            )
            .await
            else {
                cancel_retained_startup(
                    retained_playback.take().map(|playback| playback.prepared),
                    interval_reservation.as_ref(),
                    Some(&mut external_subtitle_owner),
                    &[],
                    None,
                )
                .await;
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    "selected provider subtitle is unavailable",
                )
                    .into_response();
            };
            Some(prepared)
        }
        None => None,
    };
    fence_cancelled_startup!(&[], None);
    let mut retained_subtitle_input = None;
    let retained_subtitle_verified =
        if let (Some(source), Some(subtitle)) = (&interval_source, &subtitle) {
            match tokio::time::timeout_at(
                startup_deadline,
                ersatzrs_infra::addon_media_list_subtitle_playback::prepare_retained_subtitle_input(
                    &state.pool,
                    active.media_item_id,
                    subtitle,
                    source.plan(),
                    start_position,
                ),
            )
            .await
            {
                Ok(Ok(input)) => {
                    retained_subtitle_input = input;
                    true
                }
                _ => false,
            }
        } else {
            true
        };
    if !retained_subtitle_verified {
        cancel_retained_startup(
            retained_playback.take().map(|playback| playback.prepared),
            interval_reservation.as_ref(),
            Some(&mut external_subtitle_owner),
            &[],
            None,
        )
        .await;
        return (
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "selected source-clock subtitles are unsupported with retained intervals",
        )
            .into_response();
    }
    let metadata = playback_output_metadata(
        Some("ErsatzTV"),
        Some(&channel.name),
        preferred_audio_language.as_deref(),
        &source_audio_streams,
        subtitle.as_ref(),
        audio_selection,
    );
    let subtitle_input = subtitle.as_ref().and_then(|subtitle| {
        if active.external.is_some() {
            external::subtitle_input(subtitle, &resolved, output_format, effective_streaming_mode)
        } else {
            subtitle_input_for_stream(subtitle, &resolved, output_format, effective_streaming_mode)
        }
    });
    if active.subtitle_stream_index.is_some() && subtitle_input.is_none() {
        tracing::error!(
            channel_id = channel.id,
            media_item_id = active.media_item_id,
            stream_index = active.subtitle_stream_index,
            "pinned subtitle cannot be mapped for this output; refusing subtitle-free playback"
        );
        cancel_retained_startup(
            retained_playback.take().map(|playback| playback.prepared),
            interval_reservation.as_ref(),
            Some(&mut external_subtitle_owner),
            &[],
            None,
        )
        .await;
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    fence_cancelled_startup!(&[], None);
    let subtitle_input = if interval_source.is_some() {
        retained_subtitle_input
    } else {
        rebase_burned_text_subtitle_input(subtitle_input, start_seconds).await
    };
    fence_cancelled_startup!(&[], None);
    let watermark_inputs = match async {
        if let Some(media) = &active.external {
            if channel.streaming_mode == StreamingMode::HttpLiveStreamingDirect {
                return Ok(Vec::new());
            }
            ersatzrs_infra::playback::select_fallback_watermarks(
                &state.pool,
                &channel,
                &external::decoration_item(media),
                now,
            )
            .await
        } else {
            select_playback_watermarks(
                &state.pool,
                &channel,
                active.playout_item_id,
                active.disable_watermarks,
                now,
            )
            .await
        }
    }
    .await
    {
        Ok(watermarks) => {
            if let Some(cancellation) = retained_cancellation.as_mut() {
                let (sender, receiver) = tokio::sync::oneshot::channel();
                let (cancelled, result) = await_retained_startup_stage(
                    cancellation,
                    sender,
                    ersatzrs_infra::playback::watermark_inputs_for_stream_cancellable(
                        watermarks,
                        active.program_duration(),
                        startup_deadline,
                        receiver,
                    ),
                )
                .await;
                if cancelled {
                    cancel_retained_startup(
                        retained_playback.take().map(|playback| playback.prepared),
                        interval_reservation.as_ref(),
                        Some(&mut external_subtitle_owner),
                        &[],
                        None,
                    )
                    .await;
                    return StatusCode::SERVICE_UNAVAILABLE.into_response();
                }
                match result {
                    Ok(inputs) => inputs,
                    Err(error) => {
                        tracing::warn!(channel_id = channel.id, %error,
                            "retained interval watermark preparation failed");
                        cancel_retained_startup(
                            retained_playback.take().map(|playback| playback.prepared),
                            interval_reservation.as_ref(),
                            Some(&mut external_subtitle_owner),
                            &[],
                            None,
                        )
                        .await;
                        return StatusCode::SERVICE_UNAVAILABLE.into_response();
                    }
                }
            } else {
                watermark_inputs_for_stream(watermarks, active.program_duration()).await
            }
        }
        Err(error) => {
            tracing::warn!(%error, channel_number = %channel.number, "failed to select playback watermarks");
            Vec::new()
        }
    };
    let mut graphics_cleanup_dirs = Vec::new();
    let mut dynamic_text_controllers = Vec::new();
    fence_cancelled_startup!(&graphics_cleanup_dirs, Some(&mut dynamic_text_controllers));
    let graphics_overlays = match async {
        if let Some(media) = &active.external {
            ersatzrs_infra::playback::select_fallback_graphics(
                &state.pool,
                &external::decoration_item(media),
                now,
            )
            .await
        } else {
            select_playback_graphics(&state.pool, &channel, active.playout_item_id, now).await
        }
    }
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
            let graphics_context = PlaybackGraphicsRenderContext {
                channel_id: channel.id,
                render_time: now,
                playout_item_id: Some(active.playout_item_id),
                channel_number: channel.number.clone(),
                media: active.external.as_ref().map_or_else(
                    || {
                        ersatzrs_infra::graphics_template::GraphicsMedia::Persisted(
                            active.media_item_id,
                        )
                    },
                    |media| media.graphics_media(),
                ),
                media_path: resolved.clone(),
                channel_start_time: start_time,
                content_start_time: start_time,
                content_finish_time: parse_sqlite_utc_timestamp(&active.playout_finish),
                stream_seek_seconds: start_position.as_secs_f64(),
                duration_seconds: active
                    .program_duration()
                    .map_or(0.0, |duration| duration.as_secs_f64()),
                frame_size,
                square_pixel_frame_size,
                frame_rate: target_frame_rate
                    .or_else(|| source_stream.as_ref().and_then(|stream| stream.frame_rate))
                    .map(Into::into),
            };
            let rendered = if let Some(cancellation) = retained_cancellation.as_mut() {
                let (sender, receiver) = tokio::sync::oneshot::channel();
                let (cancelled, result) = await_retained_startup_stage(
                    cancellation,
                    sender,
                    ersatzrs_infra::playback::render_playback_graphics_overlays_cancellable(
                        &state.pool,
                        &graphics,
                        &graphics_context,
                        graphics_dir,
                        startup_deadline,
                        receiver,
                    ),
                )
                .await;
                if cancelled {
                    if let Ok(artifacts) = result {
                        artifacts.shutdown().await;
                    }
                    cancel_retained_startup(
                        retained_playback.take().map(|playback| playback.prepared),
                        interval_reservation.as_ref(),
                        Some(&mut external_subtitle_owner),
                        std::slice::from_ref(&graphics_cleanup_dir),
                        Some(&mut dynamic_text_controllers),
                    )
                    .await;
                    return StatusCode::SERVICE_UNAVAILABLE.into_response();
                }
                result
            } else {
                render_playback_graphics_overlays(
                    &state.pool,
                    &graphics,
                    &graphics_context,
                    graphics_dir,
                )
                .await
            };
            match rendered {
                Ok(PlaybackGraphicsArtifacts {
                    overlays,
                    dynamic_text_controllers: controllers,
                }) => {
                    graphics_cleanup_dirs.push(graphics_cleanup_dir);
                    dynamic_text_controllers = controllers;
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
    let remaining_duration = active.remaining_duration(now);
    let stream_duration = if effective_streaming_mode == StreamingMode::TransportStream {
        remaining_duration
    } else {
        None
    };
    let stream_duration =
        native_generated_audio_duration(audio_selection, stream_duration, remaining_duration);
    let opacity_clock = watermark_opacity_clock_at_spawn(
        now,
        start_position,
        active.content_duration,
        channel_start_time,
    );
    fence_cancelled_startup!(&graphics_cleanup_dirs, Some(&mut dynamic_text_controllers));
    let media_input = retained_playback.as_ref().map_or_else(
        || {
            active.remote_stream.as_ref().map_or_else(
                || Ok(InputFile::video(resolved.clone())),
                remote_stream_input_file,
            )
        },
        |playback| Ok(playback.video_input.clone()),
    );
    let media_input = match media_input {
        Ok(input) => input,
        Err(error) => {
            cancel_retained_startup(
                retained_playback.take().map(|playback| playback.prepared),
                interval_reservation.as_ref(),
                Some(&mut external_subtitle_owner),
                &graphics_cleanup_dirs,
                Some(&mut dynamic_text_controllers),
            )
            .await;
            return (StatusCode::SERVICE_UNAVAILABLE, error.to_string()).into_response();
        }
    };
    fence_cancelled_startup!(&graphics_cleanup_dirs, Some(&mut dynamic_text_controllers));
    let song = {
        let executable = runtime_capabilities
            .runtime_identity
            .as_ref()
            .map_or_else(ersatzrs_ffmpeg::ffmpeg_path, |identity| {
                identity.ffmpeg_path.clone()
            });
        let (sender, receiver) = tokio::sync::oneshot::channel();
        let operation = ersatzrs_infra::song_background::stage_native(
            &state.pool,
            active.media_item_id,
            channel.id,
            &playback_profile.resolution,
            active.playout_item_id,
            std::path::Path::new(&executable),
            startup_deadline.min(tokio::time::Instant::now() + Duration::from_secs(15)),
            receiver,
        );
        let result = if let Some(cancellation) = retained_cancellation.as_mut() {
            let (_, result) = await_retained_startup_stage(cancellation, sender, operation).await;
            result
        } else {
            let _sender = sender;
            operation.await
        };
        fence_cancelled_startup!(&graphics_cleanup_dirs, Some(&mut dynamic_text_controllers));
        match result {
            Ok(song) => song.map(Arc::new),
            Err(error) => {
                cancel_retained_startup(
                    retained_playback.take().map(|playback| playback.prepared),
                    interval_reservation.as_ref(),
                    Some(&mut external_subtitle_owner),
                    &graphics_cleanup_dirs,
                    Some(&mut dynamic_text_controllers),
                )
                .await;

                tracing::warn!(channel_id = channel.id, %error, "song image preparation failed");
                return StatusCode::SERVICE_UNAVAILABLE.into_response();
            }
        }
    };
    let stream_duration = if song.is_some() {
        active.remaining_duration(now)
    } else {
        stream_duration
    };
    let remote_stream = active.remote_stream.clone();
    let retained_stream = retained_playback.is_some();
    let command = match build_stream_command_with_retained_inputs(
        media_input.clone(),
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
        audio_selection,
        subtitle_input.clone(),
        watermark_inputs.clone(),
        graphics_overlays.clone(),
        Some(opacity_clock),
        retained_stream,
        song.as_deref(),
    ) {
        Ok(command) => command,
        Err(error) => {
            // ADR 0064: refuse rather than transcode a source whose colour
            // ErsatzRS cannot render correctly. No FFmpeg child is spawned and
            // the reason reaches the operator.
            tracing::error!(
                channel_id = channel.id,
                channel_number = %channel.number,
                reason = %error,
                "refusing to transcode an unsupported source"
            );
            cancel_retained_startup(
                retained_playback.take().map(|playback| playback.prepared),
                interval_reservation.as_ref(),
                Some(&mut external_subtitle_owner),
                &graphics_cleanup_dirs,
                Some(&mut dynamic_text_controllers),
            )
            .await;
            return (StatusCode::UNSUPPORTED_MEDIA_TYPE, error.to_string()).into_response();
        }
    };
    let slate_metadata = metadata.clone();
    let slate_runtime_capabilities = runtime_capabilities.clone();
    let retry_playback_profile = playback_profile.clone();
    let retry_preferred_audio_title = preferred_audio_title.map(str::to_owned);
    let retry_preferred_audio_language = preferred_audio_language.clone();
    let retry = StreamDenialRetry {
        song: song.clone(),
        pool: state.pool.clone(),
        shared_ffmpeg_runtime: Arc::clone(&state.ffmpeg_runtime),
        runtime_capabilities,
        acceleration: effective_acceleration,
        source_stream: source_stream.clone(),
        graph_operation: ersatzrs_ffmpeg::selected_hardware_graph(&command),
        ffmpeg_path: command.executable.clone(),
        rebuild: Some(StreamRetryBuilder({
            let media_input = media_input.clone();
            let song = song.clone();
            let channel_number = channel.number.clone();
            let pool = state.pool.clone();
            Box::new(move |mode, cancel| {
                Box::pin(async move {
                    let (capabilities, force_software_encode) = match mode {
                        StreamRetryMode::HardwareDecodeDenied(capabilities)
                        | StreamRetryMode::HardwareGraphDenied(capabilities) => {
                            (capabilities, false)
                        }
                        StreamRetryMode::HardwareEncodeDenied(capabilities) => (capabilities, true),
                    };
                    let mut playback_profile = retry_playback_profile;
                    if force_software_encode {
                        force_software_h264_encoding(&mut playback_profile);
                    }
                    let mut fresh = if let Some(source) = interval_source {
                        let extractor = ersatzrs_infra::ffprobe::FfprobeExtractor::new(
                            ersatzrs_ffmpeg::ffprobe_path(),
                        );
                        let Ok(programme_ms) = u64::try_from(start_position.as_millis()) else {
                            return Ok(None);
                        };
                        let (sender, receiver) = tokio::sync::oneshot::channel();
                        let operation = source.prepare_playback_cancellable(
                            IntervalPlaybackContext {
                                pool: &pool,
                                probe: &extractor,
                                spawn_context: RemoteStreamSpawnContext::mpeg_ts(&channel_number),
                            },
                            programme_ms,
                            u32::try_from(playback_profile.resolution.height).ok(),
                            startup_deadline,
                            receiver,
                        );
                        tokio::pin!(operation);
                        let result = tokio::select! {
                            biased;
                            () = cancel.notified() => {
                                drop(sender);
                                if let Some(playback) = retained_retry_input(operation.await)? {
                                    playback.prepared.cancel().await.map_err(|error| StreamSpawnError::prepared(&error))?;
                                }
                                return Ok(None);
                            },
                            result = &mut operation => result,
                        };
                        match retained_retry_input(result)? {
                            Some(playback) => Some(playback),
                            None => return Ok(None),
                        }
                    } else {
                        None
                    };
                    let audio_selection = if let Some(playback) = &fresh {
                        let explicit = ersatzrs_infra::playback::remap_pinned_audio_selection(
                            explicit_audio_selection,
                            &source_audio_streams,
                            &playback.source_audio,
                        );
                        let Ok(explicit) = explicit else {
                            if let Some(fresh) = fresh.take() {
                                fresh
                                    .prepared
                                    .cancel()
                                    .await
                                    .map_err(|error| StreamSpawnError::prepared(&error))?;
                            }
                            return Ok(None);
                        };
                        match explicit {
                            PlaybackAudioSelection::Automatic => {
                                select_playback_audio_stream_index(
                                    &playback.source_audio,
                                    retry_preferred_audio_language.as_deref(),
                                    retry_preferred_audio_title.as_deref(),
                                )
                                .map_or(
                                    PlaybackAudioSelection::Automatic,
                                    PlaybackAudioSelection::Exact,
                                )
                            }
                            selection => selection,
                        }
                    } else {
                        audio_selection
                    };
                    let audio_selection = if let Some(fresh) = &fresh {
                        retained_audio_selection(audio_selection, &fresh.source_audio)
                            .with_complete_source_audio(&fresh.source_audio)
                    } else {
                        audio_selection
                    };
                    let metadata = if let Some(fresh) = &fresh {
                        playback_output_metadata(
                            metadata.service_provider.as_deref(),
                            metadata.service_name.as_deref(),
                            retry_preferred_audio_language.as_deref(),
                            &fresh.source_audio,
                            None,
                            audio_selection,
                        )
                    } else {
                        metadata
                    };
                    let retained_stream = fresh.is_some();
                    let media_input = fresh
                        .as_ref()
                        .map_or(media_input, |fresh| fresh.video_input.clone());
                    let source_stream = fresh
                        .as_ref()
                        .map_or(source_stream, |fresh| Some(fresh.source_video.clone()));
                    let source_audio_streams = fresh
                        .as_ref()
                        .map_or(source_audio_streams, |fresh| fresh.source_audio.clone());
                    let stream_duration = native_generated_audio_duration(
                        audio_selection,
                        stream_duration,
                        remaining_duration,
                    );
                    tracing::debug!(
                        channel_number = %channel_number,
                        force_software_encode,
                        "rebuilding channel stream command after runtime hardware rejection"
                    );
                    let command = build_stream_command_with_retained_inputs(
                        media_input,
                        output_format,
                        start_position,
                        stream_duration,
                        &playback_profile,
                        target_frame_rate,
                        effective_streaming_mode,
                        metadata,
                        capabilities,
                        source_stream.clone(),
                        source_audio_streams,
                        audio_selection,
                        subtitle_input,
                        watermark_inputs,
                        graphics_overlays,
                        Some(opacity_clock),
                        retained_stream,
                        song.as_deref(),
                    );
                    let Ok(command) = command else {
                        if let Some(fresh) = fresh.take() {
                            fresh
                                .prepared
                                .cancel()
                                .await
                                .map_err(|error| StreamSpawnError::prepared(&error))?;
                        }
                        return Ok(None);
                    };
                    if fresh.is_some() && tokio::time::Instant::now() >= startup_deadline {
                        if let Some(fresh) = fresh {
                            fresh
                                .prepared
                                .cancel()
                                .await
                                .map_err(|error| StreamSpawnError::prepared(&error))?;
                        }
                        return Ok(None);
                    }
                    Ok(Some(StreamRetryAttempt {
                        command,
                        source_stream,
                        prepared: fresh.map(|fresh| fresh.prepared),
                    }))
                })
            })
        })),
    };
    let capacity_reservation = if interval_reservation.is_some() {
        interval_reservation
    } else if effective_streaming_mode == ersatzrs_domain::StreamingMode::TransportStream
        || song.is_some()
        || audio_selection == PlaybackAudioSelection::AutomaticSilence
    {
        match reserve_mpeg_ts_capacity(state, &channel.number).await {
            Ok(Some(reservation)) => Some(reservation),
            Ok(None) => {
                output = output.without_optional_accepted();
                if output.accepted().is_some() || output.live().is_some() {
                    // A slate and the later media command are distinct source
                    // epochs; this explicit accepted owner cannot join them.
                    cancel_retained_startup(
                        retained_playback.take().map(|playback| playback.prepared),
                        None,
                        Some(&mut external_subtitle_owner),
                        &graphics_cleanup_dirs,
                        Some(&mut dynamic_text_controllers),
                    )
                    .await;
                    return StatusCode::SERVICE_UNAVAILABLE.into_response();
                }
                if effective_streaming_mode != ersatzrs_domain::StreamingMode::TransportStream {
                    shutdown_dynamic_text_controllers(dynamic_text_controllers).await;
                    cleanup_stream_artifacts(&graphics_cleanup_dirs).await;
                    return (
                        StatusCode::SERVICE_UNAVAILABLE,
                        [(axum::http::header::RETRY_AFTER, "1")],
                    )
                        .into_response();
                }
                if let Some(playback) = retained_playback.take() {
                    cancel_retained_startup(
                        Some(playback.prepared),
                        None,
                        Some(&mut external_subtitle_owner),
                        &graphics_cleanup_dirs,
                        Some(&mut dynamic_text_controllers),
                    )
                    .await;

                    return StatusCode::SERVICE_UNAVAILABLE.into_response();
                }
                return mpeg_ts_capacity_wait_response(
                    state,
                    &channel,
                    command,
                    graphics_cleanup_dirs,
                    dynamic_text_controllers,
                    &playback_profile,
                    target_frame_rate,
                    slate_metadata,
                    slate_runtime_capabilities,
                    Some(retry),
                    remote_stream.clone(),
                    external_subtitle_owner,
                    output,
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
                cancel_retained_startup(
                    retained_playback.take().map(|playback| playback.prepared),
                    None,
                    Some(&mut external_subtitle_owner),
                    &graphics_cleanup_dirs,
                    Some(&mut dynamic_text_controllers),
                )
                .await;

                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        }
    } else {
        None
    };

    let cleanup_on_spawn_failure = graphics_cleanup_dirs.clone();
    if retained_playback.is_some() && tokio::time::Instant::now() >= startup_deadline {
        cancel_retained_startup(
            retained_playback.take().map(|playback| playback.prepared),
            capacity_reservation.as_ref(),
            Some(&mut external_subtitle_owner),
            &graphics_cleanup_dirs,
            Some(&mut dynamic_text_controllers),
        )
        .await;

        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "retained interval startup deadline expired",
        )
            .into_response();
    }
    let retained_output = if retained_playback.is_some() {
        match RetainedTsOutput::from_command(
            &command,
            stream_duration.or_else(|| active.remaining_duration(now)),
        )
        .and_then(|output| {
            if let Some(binding) = &retained_teletext {
                let policy = ReservePolicy::new(output.muxrate_bps, output.remaining)
                    .map_err(|_| "retained MPEG-TS profile exceeds the encoded reserve bound")?;
                super::teletext_retained_output::validate_policy(policy, binding.config())
                    .map_err(|_| "retained teletext output policies are incompatible")?;
            }
            Ok(output)
        }) {
            Ok(output) => Some(output),
            Err(reason) => {
                tracing::warn!(
                    channel_id = channel.id,
                    reason,
                    "retained MPEG-TS reserve refused"
                );
                cancel_retained_startup(
                    retained_playback.take().map(|playback| playback.prepared),
                    capacity_reservation.as_ref(),
                    Some(&mut external_subtitle_owner),
                    &graphics_cleanup_dirs,
                    Some(&mut dynamic_text_controllers),
                )
                .await;

                return (StatusCode::UNSUPPORTED_MEDIA_TYPE, reason).into_response();
            }
        }
    } else {
        None
    };
    if legacy_cancellation
        .as_ref()
        .is_some_and(LegacyWrapperItemCancellation::is_cancelled)
    {
        cancel_retained_startup(
            retained_playback.take().map(|playback| playback.prepared),
            capacity_reservation.as_ref(),
            Some(&mut external_subtitle_owner),
            &graphics_cleanup_dirs,
            Some(&mut dynamic_text_controllers),
        )
        .await;
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    match spawn_stream_body_with_lifetime(
        &command,
        graphics_cleanup_dirs,
        dynamic_text_controllers,
        channel.number.clone(),
        state.hls_sessions.clone(),
        capacity_reservation,
        Some(retry),
        remote_stream,
        retained_playback.map(|playback| playback.prepared),
        external_subtitle_owner,
        retained_output
            .as_ref()
            .map(|output| (output.terminal.clone(), output.completed.clone())),
        output,
        legacy_item.take().map(LegacyStreamLifetime::Item),
    )
    .await
    {
        Ok(body) => {
            let mut response = body.into_response();
            if let Some(output) = retained_output {
                response.extensions_mut().insert(output);
            }
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
            if error.cleanup_confirmed() {
                cleanup_stream_artifacts(&cleanup_on_spawn_failure).await;
            }
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

// Upstream IptvController coerces cached ts-legacy URLs to Next's TS wrapper.
// Resolve aliases and stale stored modes too, without starting the native engine.
fn channel_transport_stream_mode(
    requested_mode: Option<&str>,
    channel: &Channel,
) -> TransportStreamMode {
    let mode = resolve_transport_stream_mode(requested_mode, channel.streaming_mode);
    if channel.streaming_engine == ersatzrs_domain::StreamingEngine::Next
        && mode == TransportStreamMode::Legacy
    {
        tracing::info!(
            channel_id = channel.id,
            "Next channel uses hybrid MPEG-TS for a legacy transport request"
        );
        TransportStreamMode::HybridWrapper
    } else {
        mode
    }
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
        "mpegts" | "mpeg-ts" | "ts-legacy" => TransportStreamMode::Legacy,
        _ => TransportStreamMode::HybridWrapper,
    }
}

#[allow(clippy::too_many_lines)]
async fn serve_segmenter_wrapper_stream(
    state: &AppState,
    channel: &Channel,
    query: &HlsQuery,
    output: TeletextOutputAdmission,
) -> Response {
    if output.validate_format(OutputFormat::MpegTs).is_err() {
        return teletext_output_unavailable();
    }
    let Some(streaming_addr) = state.streaming_http_addr.get() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    let path_replacements = match tokio::time::timeout_at(
        tokio::time::Instant::from_std(deadline),
        load_required_path_replacements(state, channel, "wrapped segmenter"),
    )
    .await
    {
        Ok(Ok(replacements)) => replacements,
        Ok(Err(response)) => return response,
        Err(_) => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                "segmenter startup deadline expired",
            )
                .into_response();
        }
    };

    let initial_segment_count = hls::load_initial_segment_count(state).await;
    let startup_timeout = hls::load_segmenter_startup_timeout(state)
        .await
        .min(deadline.saturating_duration_since(std::time::Instant::now()));
    let wait_result = hls::start_and_wait_for_initial_segments(
        &*state.hls_sessions,
        &channel.number,
        initial_segment_count,
        startup_timeout,
        deadline,
        || {
            hls::ensure_segmenter_worker_running_before(
                state,
                &channel.number,
                channel.id,
                channel.idle_behavior,
                OutputFormat::Hls,
                path_replacements.clone(),
                deadline,
            )
        },
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
        *streaming_addr,
        &channel.number,
        channel.streaming_engine,
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
                Vec::new(),
                &playback_profile,
                target_frame_rate,
                metadata,
                runtime_capabilities,
                None,
                None,
                None,
                output,
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
        Vec::new(),
        channel.number.clone(),
        state.hls_sessions.clone(),
        capacity_reservation,
        None,
        None,
        output,
    )
    .await
    {
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
    output: TeletextOutputAdmission,
) -> Response {
    if output.validate_format(OutputFormat::MpegTs).is_err() {
        return teletext_output_unavailable();
    }
    let Some(internal_addr) = state.internal_http_addr.get() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let metadata = channel_stream_metadata(channel);
    let (playback_profile, target_frame_rate, runtime_capabilities) =
        match load_capacity_slate_context(state, channel, "legacy stream").await {
            Ok(context) => context,
            Err(response) => return response,
        };
    let wrapper_muxrate_bps = mpeg_ts_wrapper_muxrate_bps(&playback_profile);
    let limit = match load_active_transcode_limit(&state.pool).await {
        Ok(limit) => limit.unwrap_or(DEFAULT_ACTIVE_TRANSCODE_LIMIT),
        Err(error) => {
            tracing::error!(%error, "failed to load legacy wrapper transcode capacity");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    if limit < 2 {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "legacy transport requires capacity for a wrapper and encoder",
        )
            .into_response();
    }
    let Some((wrapper, first_encoder)) =
        try_reserve_mpeg_ts_wrapper_capacity(limit, &channel.number)
    else {
        if output.has_authored_output() {
            // A raw capacity-slate prefix followed by this wrapper would
            // start a new PSI/PCR epoch. Authored output must fail closed.
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        }
        return legacy_wrapper_capacity_wait_response(
            state,
            channel,
            *internal_addr,
            &playback_profile,
            target_frame_rate,
            metadata,
            runtime_capabilities,
            wrapper_muxrate_bps,
        )
        .await;
    };
    let owner = match state.legacy_wrappers.register(
        &channel.number,
        &wrapper,
        first_encoder,
        tokio::time::Instant::now() + Duration::from_secs(20),
    ) {
        Ok(owner) => owner,
        Err(error) => {
            tracing::warn!(%error, "legacy wrapper ownership registration failed");
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        }
    };
    let command = build_mpegts_concat_command(
        build_owned_legacy_concat_url(*internal_addr, &channel.number, owner.id()),
        &metadata,
        Some(wrapper_muxrate_bps),
    );

    match spawn_stream_body_with_lifetime(
        &command,
        Vec::new(),
        Vec::new(),
        channel.number.clone(),
        state.hls_sessions.clone(),
        Some(wrapper),
        None,
        None,
        None,
        None,
        None,
        output,
        Some(LegacyStreamLifetime::Wrapper(owner)),
    )
    .await
    {
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

// Plain legacy callers retain the existing capacity-slate behavior. Once
// admitted, the wrapper and first item always receive distinct atomic leases.
#[allow(
    clippy::too_many_arguments,
    reason = "Carries the existing finite slate and the independently admitted wrapper context together"
)]
#[expect(
    clippy::too_many_lines,
    reason = "Keeps plain capacity retry, exact wrapper ownership and child cleanup under one supervised response lifetime"
)]
async fn legacy_wrapper_capacity_wait_response(
    state: &AppState,
    channel: &Channel,
    address: std::net::SocketAddr,
    playback_profile: &PlaybackProfile,
    target_frame_rate: Option<FrameRate>,
    metadata: OutputMetadata,
    runtime_capabilities: FfmpegCapabilityReport,
    muxrate_bps: i64,
) -> Response {
    let slate_dir = mpeg_ts_capacity_slate_dir(channel.id);
    let Some(slate_command) = build_mpeg_ts_slate_command(MpegTsSlateCommandRequest {
        working_dir: &slate_dir,
        duration: MPEG_TS_CAPACITY_SLATE_DURATION,
        playback_profile,
        target_frame_rate,
        metadata: metadata.clone(),
        runtime_capabilities,
        message: ALL_STREAMS_IN_USE_SLATE_MESSAGE,
    })
    .await
    else {
        cleanup_stream_artifact(&slate_dir).await;
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let registry = state.legacy_wrappers.clone();
    let session_registry = state.hls_sessions.clone();
    let pool = state.pool.clone();
    let channel_number = channel.number.clone();
    let handle = Arc::new(DirectStreamHandle::new(channel_number.clone()));
    let cancel = handle.cancel.clone();
    let handle: Arc<dyn HlsSessionHandle> = handle;
    let fence = Arc::new(WaitingLegacyWrapperFence::default());
    let (reader, mut writer) = tokio::io::duplex(STREAM_BODY_BUFFER_BYTES);
    let output = TeletextOutputAdmission::plain(TeletextOutputRole::FinalWrapperMpegTs);
    let (body, completion) = stream_body_for_wrapper_wait(
        reader,
        cancel.clone(),
        &output,
        true,
        None,
        Some(fence.clone()),
    );
    tokio::spawn(async move {
        session_registry.register_direct(handle.clone()).await;
        let outcome = loop {
            if fence.closed.load(Ordering::Acquire) {
                break DirectStreamOutcome::Cancelled;
            }
            let limit = match load_active_transcode_limit(&pool).await {
                Ok(limit) => limit.unwrap_or(DEFAULT_ACTIVE_TRANSCODE_LIMIT),
                Err(error) => {
                    tracing::warn!(%error, "legacy wrapper capacity reload failed");
                    break DirectStreamOutcome::Failed(
                        "legacy capacity unavailable".to_owned().into(),
                    );
                }
            };
            if limit < 2 {
                break DirectStreamOutcome::Failed(
                    "legacy wrapper requires two transcode slots"
                        .to_owned()
                        .into(),
                );
            }
            if fence.closed.load(Ordering::Acquire) {
                break DirectStreamOutcome::Cancelled;
            }
            if let Some((wrapper, first_encoder)) =
                try_reserve_mpeg_ts_wrapper_capacity(limit, &channel_number)
            {
                let owner = match registry.register(
                    &channel_number,
                    &wrapper,
                    first_encoder,
                    tokio::time::Instant::now() + Duration::from_secs(20),
                ) {
                    Ok(owner) => owner,
                    Err(error) => {
                        tracing::warn!(%error, "legacy waiting wrapper registration failed");
                        break DirectStreamOutcome::Failed(
                            "legacy wrapper ownership unavailable".to_owned().into(),
                        );
                    }
                };
                fence.attach(owner.close_handle());
                if fence.closed.load(Ordering::Acquire) {
                    break DirectStreamOutcome::Cancelled;
                }
                let command = build_mpegts_concat_command(
                    build_owned_legacy_concat_url(address, &channel_number, owner.id()),
                    &metadata,
                    Some(muxrate_bps),
                );
                let mut child = match spawn_stream_process(&command, None, &channel_number) {
                    Ok(child) => child,
                    Err(error) => {
                        if !error.cleanup_confirmed() {
                            wrapper.retain_until_restart();
                        }
                        tracing::warn!(%error, "legacy waiting wrapper spawn failed");
                        break if error.cleanup_confirmed() {
                            DirectStreamOutcome::Failed(
                                "legacy wrapper spawn failed".to_owned().into(),
                            )
                        } else {
                            DirectStreamOutcome::CleanupUnconfirmed
                        };
                    }
                };
                if child.stdout.is_none() {
                    if reap_stream_process(&mut child).await.is_err() {
                        wrapper.retain_until_restart();
                        break DirectStreamOutcome::CleanupUnconfirmed;
                    }
                    break DirectStreamOutcome::Failed(
                        "legacy wrapper has no output".to_owned().into(),
                    );
                }
                let wrapper_cancel = owner.cancel_signal();
                let outcome = {
                    let mut retry = None;
                    let operation = supervise_stream_command_with_retry(
                        child,
                        &mut writer,
                        &cancel,
                        &channel_number,
                        &mut retry,
                        None,
                        &output,
                    );
                    tokio::pin!(operation);
                    tokio::select! {
                        biased;
                        () = wrapper_cancel.notified() => {
                            owner.close();
                            cancel.notify_one();
                            operation.await
                        },
                        () = owner.first_item_deadline_expired() => {
                            owner.close();
                            cancel.notify_one();
                            operation.await
                        },
                        outcome = &mut operation => outcome,
                    }
                };
                owner.close();
                if matches!(outcome, DirectStreamOutcome::CleanupUnconfirmed) {
                    wrapper.retain_until_restart();
                }
                drop(owner);
                drop(wrapper);
                break outcome;
            }
            if request_hls_capacity_reclaim_for_mpeg_ts(&channel_number) {
                tokio::select! {
                    () = cancel.notified() => break DirectStreamOutcome::Cancelled,
                    () = tokio::time::sleep(MPEG_TS_CAPACITY_RETRY_INTERVAL) => {},
                }
                continue;
            }
            let mut slate_child = match spawn_stream_process(&slate_command, None, &channel_number)
            {
                Ok(child) => child,
                Err(error) => {
                    tracing::warn!(%error, "legacy waiting slate spawn failed");
                    break if error.cleanup_confirmed() {
                        DirectStreamOutcome::Failed(
                            "legacy capacity slate spawn failed".to_owned().into(),
                        )
                    } else {
                        DirectStreamOutcome::CleanupUnconfirmed
                    };
                }
            };
            match supervise_stream_process(
                &mut slate_child,
                &mut writer,
                &cancel,
                &channel_number,
                None,
            )
            .await
            {
                DirectStreamOutcome::Completed => {}
                outcome => break outcome,
            }
            tokio::select! {
                () = cancel.notified() => break DirectStreamOutcome::Cancelled,
                () = tokio::time::sleep(MPEG_TS_CAPACITY_RETRY_INTERVAL) => {},
            }
        };
        fence.close();
        session_registry.unregister_direct(&handle).await;
        if !matches!(outcome, DirectStreamOutcome::CleanupUnconfirmed) {
            cleanup_stream_artifact(&slate_dir).await;
        }
        if let Some(completion) = completion {
            let _ = completion.send(stream_output_result(&outcome));
        }
        drop(writer);
    });
    let mut response = body.into_response();
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static("video/mp2t"));
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

fn build_legacy_concat_url(address: std::net::SocketAddr, channel_number: &str) -> String {
    format!("http://{address}/ffmpeg/concat/{channel_number}?mode=ts-legacy")
}

fn build_owned_legacy_concat_url(
    address: std::net::SocketAddr,
    channel_number: &str,
    wrapper_id: uuid::Uuid,
) -> String {
    format!(
        "{}&wrapper_id={wrapper_id}",
        build_legacy_concat_url(address, channel_number)
    )
}

fn build_segmenter_wrapper_hls_url(
    address: std::net::SocketAddr,
    channel_number: &str,
    engine: ersatzrs_domain::StreamingEngine,
    access_token: &AccessToken,
) -> String {
    // ADR 0099: live WebVTT in a master stalls FFmpeg's MPEG-TS copy wrapper.
    // Converted text stays on HLS; this already-started session carries video/audio.
    if engine == ersatzrs_domain::StreamingEngine::Next {
        return format!(
            "http://{address}/iptv/session/{channel_number}/hls.m3u8{}",
            access_token.query()
        );
    }
    format!(
        "http://{address}/iptv/channel/{channel_number}.m3u8?mode=segmenter{}",
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
    dynamic_text_controllers: Vec<DynamicTextController>,
    playback_profile: &PlaybackProfile,
    target_frame_rate: Option<FrameRate>,
    metadata: OutputMetadata,
    runtime_capabilities: FfmpegCapabilityReport,
    retry: Option<StreamDenialRetry>,
    remote_stream: Option<ResolvedRemoteStream>,
    subtitle_owner: Option<ersatzrs_infra::external_json::subtitles::PreparedSubtitle>,
    output: TeletextOutputAdmission,
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
        shutdown_dynamic_text_controllers(dynamic_text_controllers).await;
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
        dynamic_text_controllers,
        channel.number.clone(),
        state.hls_sessions.clone(),
        state.pool.clone(),
        retry,
        remote_stream,
        subtitle_owner,
        output,
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
    !mpegts_script_id_eq(script_id, DEFAULT_MPEG_TS_SCRIPT_ID)
}

fn mpegts_script_id_eq(left: &str, right: &str) -> bool {
    let case_mapper = icu_casemap::CaseMapper::new();
    left.chars()
        .map(|character| case_mapper.simple_uppercase(character))
        .eq(right
            .chars()
            .map(|character| case_mapper.simple_uppercase(character)))
}

async fn find_mpegts_script_in(root: &std::path::Path, script_id: &str) -> Option<MpegTsScript> {
    ersatzrs_infra::mpegts_scripts::discover_in(root)
        .await
        .ok()?
        .into_iter()
        .find(|script| mpegts_script_id_eq(&script.id, script_id))
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

    // MpegTsScriptService: environment values avoid cmd.exe's batch-file
    // code-page decoding of Unicode channel names and executable paths.
    let environment = vec![
        ersatzrs_ffmpeg::EnvironmentVariable::new("ETV_HLS_URL", &data.hls_url),
        ersatzrs_ffmpeg::EnvironmentVariable::new(
            "ETV_CHANNEL_NAME",
            data.channel_name.replace('"', ""),
        ),
        ersatzrs_ffmpeg::EnvironmentVariable::new("ETV_FFMPEG_PATH", &data.ffmpeg_path),
    ];

    if cfg!(windows) {
        Ok((
            FfmpegCommand::new_with_executable(
                temp_path.to_string_lossy().into_owned(),
                environment,
                Vec::new(),
            ),
            vec![temp_path],
        ))
    } else {
        Ok((
            FfmpegCommand::new_with_executable(
                "bash",
                environment,
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
    #[error("ffmpeg spawn cleanup could not be confirmed: {0}")]
    CleanupUnconfirmed(String),
    #[error("ffmpeg did not expose stdout")]
    MissingStdout,
}

impl StreamSpawnError {
    fn cleanup_confirmed(&self) -> bool {
        !matches!(self, Self::CleanupUnconfirmed(_))
    }

    fn prepared(error: &ersatzrs_infra::remote_stream::RemoteStreamError) -> Self {
        if matches!(
            error,
            ersatzrs_infra::remote_stream::RemoteStreamError::Spawn(_)
        ) {
            // The prepared adapter reaps its producer when consumer creation
            // fails; that is its only guaranteed no-consuming-child error.
            Self::Spawn(error.to_string())
        } else {
            Self::CleanupUnconfirmed(error.to_string())
        }
    }
}

#[derive(Clone)]
pub(crate) struct ActivePlayoutItem {
    pub(crate) external:
        Option<Arc<ersatzrs_infra::external_json::playback::ResolvedExternalMedia>>,
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
    pub(crate) has_pinned_stream_selection: bool,
    pub(crate) video_stream_index: Option<i32>,
    pub(crate) audio_stream_index: Option<i32>,
    pub(crate) subtitle_stream_index: Option<i32>,
    pub(crate) remote_stream: Option<ResolvedRemoteStream>,
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

#[allow(clippy::too_many_lines)]
pub(crate) async fn load_active_media_path(
    pool: &DbPool,
    channel_id: i64,
    now: DateTime<Utc>,
) -> Result<Option<ActivePlayoutItem>, RepositoryError> {
    let Some(playout_context) = load_channel_playout_context(pool, channel_id).await? else {
        return Ok(None);
    };
    let source_now = playout_context.source_time(now)?;
    let now_text = encode_sql_datetime(source_now);
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
                  pi."SubtitleMode" AS "SubtitleMode",
                  pi."MediaVersionId" AS "PinnedMediaVersionId",
                  pi."VideoStreamIndex" AS "VideoStreamIndex",
                  pi."AudioStreamIndex" AS "AudioStreamIndex",
                  pi."SubtitleStreamIndex" AS "SubtitleStreamIndex",
                  rs."Url" AS "RemoteUrl",
                  rs."Id" AS "RemoteStreamId",
                  rs."AddonId" AS "RemoteAddonId",
                  rs."Script" AS "RemoteLegacyScript",
                  rs."Duration" AS "RemoteDuration",
                  rs."FallbackQuery" AS "RemoteFallbackQuery",
                  rs."Liveness" AS "RemoteLiveness",
                  rs."IsLive" AS "RemoteIsLive"
           FROM "PlayoutItem" pi
           JOIN "Playout" p ON p."Id" = pi."PlayoutId"
           JOIN "MediaVersion" mv ON (
                  mv."MovieId" = pi."MediaItemId"
               OR mv."EpisodeId" = pi."MediaItemId"
               OR mv."MusicVideoId" = pi."MediaItemId"
               OR mv."OtherVideoId" = pi."MediaItemId"
               OR mv."RemoteStreamId" = pi."MediaItemId"
               OR mv."SongId" = pi."MediaItemId"
               OR mv."ImageId" = pi."MediaItemId"
           ) AND (pi."MediaVersionId" IS NULL OR mv."Id" = pi."MediaVersionId")
           JOIN "MediaFile" mf ON mf."MediaVersionId" = mv."Id"
           LEFT JOIN "RemoteStream" rs ON rs."Id" = mv."RemoteStreamId"
           WHERE p."ChannelId" = ?
             AND pi."Start" <= ?
             AND pi."Finish" > ?
             AND mf."Path" IS NOT NULL
           ORDER BY pi."Start" DESC, pi."Id" DESC, mv."Id"
           LIMIT 1"#,
    )
    .bind(playout_context.source_channel_id)
    .bind(&now_text)
    .bind(&now_text)
    .fetch_optional(pool)
    .await
    .map_err(|error| ersatzrs_infra::map_repository_error(&error))?;
    let Some(row) = row else {
        return Ok(None);
    };
    let content_duration_text = decode_active_item_column::<String>(&row, "ContentDuration")?;
    let content_duration = parse_duration_text(&content_duration_text)?;
    let in_point_text = decode_active_item_column::<String>(&row, "InPoint")?;
    let out_point_text = decode_active_item_column::<String>(&row, "OutPoint")?;
    let source_playout_start = decode_active_item_column::<String>(&row, "PlayoutStart")?;
    let source_playout_finish = decode_active_item_column::<String>(&row, "PlayoutFinish")?;
    let source_playout_start =
        parse_sqlite_utc_timestamp(&source_playout_start).ok_or_else(|| {
            RepositoryError::Decode {
                kind: "active playback item",
                value: format!("PlayoutStart: invalid timestamp {source_playout_start:?}"),
            }
        })?;
    let source_playout_finish =
        parse_sqlite_utc_timestamp(&source_playout_finish).ok_or_else(|| {
            RepositoryError::Decode {
                kind: "active playback item",
                value: format!("PlayoutFinish: invalid timestamp {source_playout_finish:?}"),
            }
        })?;
    let playout_start = encode_sql_datetime(playout_context.target_time(source_playout_start)?);
    let playout_finish = encode_sql_datetime(playout_context.target_time(source_playout_finish)?);
    let subtitle_mode_value = decode_active_item_column::<Option<i64>>(&row, "SubtitleMode")?;
    let subtitle_mode = subtitle_mode_value
        .map(|value| {
            decode_subtitle_mode(value).ok_or_else(|| RepositoryError::Decode {
                kind: "active playback item",
                value: format!("SubtitleMode: unknown value {value}"),
            })
        })
        .transpose()?;
    let media_path = decode_active_item_column::<String>(&row, "MediaPath")?;
    let remote_url = decode_active_item_column::<Option<String>>(&row, "RemoteUrl")?;
    let remote_addon = decode_active_item_column::<Option<String>>(&row, "RemoteAddonId")?;
    let remote_legacy_script =
        decode_active_item_column::<Option<String>>(&row, "RemoteLegacyScript")?;
    if remote_legacy_script.is_some() {
        return Err(RepositoryError::Decode {
            kind: "active remote stream",
            value: "legacy script source requires migration to an add-on".to_owned(),
        });
    }
    let remote_stream = if remote_url.is_some() || remote_addon.is_some() {
        let is_live = decode_active_item_column::<Option<i64>>(&row, "RemoteIsLive")?
            .is_some_and(|value| value != 0);
        let liveness = decode_persisted_liveness(
            decode_active_item_column::<Option<i64>>(&row, "RemoteLiveness")?.unwrap_or_default(),
            is_live,
        )
        .map_err(|error| RepositoryError::Decode {
            kind: "active remote stream",
            value: error.to_string(),
        })?;
        let definition = RemoteStreamDefinition {
            url: remote_url,
            addon: remote_addon,
            duration: decode_active_item_column(&row, "RemoteDuration")?,
            fallback_query: decode_active_item_column(&row, "RemoteFallbackQuery")?,
            liveness: Some(liveness),
            is_live,
            title: None,
            plot: None,
            year: None,
            content_rating: None,
            tags: Vec::new(),
            ..RemoteStreamDefinition::default()
        };
        let runtime = AddonRuntimeContext {
            ffmpeg_path: Some(ersatzrs_ffmpeg::ffmpeg_path()),
            ffprobe_path: Some(ersatzrs_ffmpeg::ffprobe_path()),
        };
        let published = ersatzrs_infra::addon_media_list_playback::resolve_published_remote_stream(
            pool,
            decode_active_item_column(&row, "RemoteStreamId")?,
            &runtime,
            &ersatzrs_infra::file_system_layout::addon_processed_media_folder(),
        )
        .await?;
        Some(match published {
            Some(stream) => stream,
            None => resolve_remote_stream_with_addons(pool, &definition, &runtime)
                .await
                .map_err(|error| RepositoryError::Decode {
                    kind: "active remote stream",
                    value: error.to_string(),
                })?,
        })
    } else {
        None
    };

    Ok(Some(ActivePlayoutItem {
        external: None,
        playout_item_id: decode_active_item_column(&row, "PlayoutItemId")?,
        media_path,
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
        has_pinned_stream_selection: decode_active_item_column::<Option<i64>>(
            &row,
            "PinnedMediaVersionId",
        )?
        .is_some(),
        video_stream_index: decode_active_item_column(&row, "VideoStreamIndex")?,
        audio_stream_index: decode_active_item_column(&row, "AudioStreamIndex")?,
        subtitle_stream_index: decode_active_item_column(&row, "SubtitleStreamIndex")?,
        remote_stream,
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
    audio_selection: impl Into<PlaybackAudioSelection>,
    subtitle_input: Option<InputFile>,
    watermark_inputs: Vec<(InputFile, ersatzrs_ffmpeg::WatermarkOverlay)>,
    graphics_overlays: Vec<GraphicsPipelineOverlay>,
) -> Result<FfmpegCommand, ersatzrs_ffmpeg::UnsupportedSource> {
    let audio_selection = audio_selection.into();
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
        audio_selection,
        subtitle_input,
        watermark_inputs,
        graphics_overlays,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
#[cfg(test)]
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
    audio_selection: PlaybackAudioSelection,
    subtitle_input: Option<InputFile>,
    watermark_inputs: Vec<(InputFile, ersatzrs_ffmpeg::WatermarkOverlay)>,
    graphics_overlays: Vec<GraphicsPipelineOverlay>,
    opacity_clock: Option<WatermarkOpacityClock>,
) -> Result<FfmpegCommand, ersatzrs_ffmpeg::UnsupportedSource> {
    build_stream_command_with_input_and_opacity_clock(
        InputFile::video(path.to_string()),
        output_format,
        start_position,
        duration,
        playback_profile,
        target_frame_rate,
        streaming_mode,
        metadata,
        runtime_capabilities,
        source_stream,
        source_audio_streams,
        audio_selection,
        subtitle_input,
        watermark_inputs,
        graphics_overlays,
        opacity_clock,
    )
}

#[allow(clippy::too_many_arguments)]
#[cfg(test)]
fn build_stream_command_with_input_and_opacity_clock(
    input: InputFile,
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
    audio_selection: PlaybackAudioSelection,
    subtitle_input: Option<InputFile>,
    watermark_inputs: Vec<(InputFile, ersatzrs_ffmpeg::WatermarkOverlay)>,
    graphics_overlays: Vec<GraphicsPipelineOverlay>,
    opacity_clock: Option<WatermarkOpacityClock>,
) -> Result<FfmpegCommand, ersatzrs_ffmpeg::UnsupportedSource> {
    build_stream_command_with_retained_inputs(
        input,
        output_format,
        start_position,
        duration,
        playback_profile,
        target_frame_rate,
        streaming_mode,
        metadata,
        runtime_capabilities,
        source_stream,
        source_audio_streams,
        audio_selection,
        subtitle_input,
        watermark_inputs,
        graphics_overlays,
        opacity_clock,
        false,
        None,
    )
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn build_stream_command_with_retained_inputs(
    input: InputFile,
    output_format: OutputFormat,
    start_position: Duration,
    duration: Option<Duration>,
    playback_profile: &PlaybackProfile,
    target_frame_rate: Option<FrameRate>,
    streaming_mode: StreamingMode,
    metadata: OutputMetadata,
    runtime_capabilities: FfmpegCapabilityReport,
    source_stream: Option<SourceVideoStream>,
    source_audio_streams: Vec<SourceAudioStream>,
    audio_selection: PlaybackAudioSelection,
    subtitle_input: Option<InputFile>,
    watermark_inputs: Vec<(InputFile, ersatzrs_ffmpeg::WatermarkOverlay)>,
    graphics_overlays: Vec<GraphicsPipelineOverlay>,
    opacity_clock: Option<WatermarkOpacityClock>,
    retained_stream: bool,
    song: Option<&ersatzrs_infra::song_background::NativeSongBackground>,
) -> Result<FfmpegCommand, ersatzrs_ffmpeg::UnsupportedSource> {
    let seek_after_input = input.seek_after_input;
    let needs_hls_direct_mpegts_transcode = hls_direct_mpegts_needs_transcode(
        output_format,
        streaming_mode,
        source_stream.as_ref(),
        &source_audio_streams,
        audio_selection,
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
        normalize_hls_direct_mpegts_h264_preset(
            &mut desired,
            playback_profile.profile.hardware_acceleration,
        );
    }
    let mut request =
        PipelineRequest::new(input, desired).with_runtime_capabilities(runtime_capabilities);
    if let Some(clock) = opacity_clock {
        request = request.with_watermark_opacity_clock(clock);
    }
    if let Some(source_stream) = source_stream {
        request = request.with_source_video_stream(source_stream);
    }
    request = request.with_source_audio_streams(source_audio_streams);
    match audio_selection {
        PlaybackAudioSelection::Exact(stream_index) => {
            if let Some(audio_input) = request
                .inputs
                .iter_mut()
                .find(|input| input.kind == ersatzrs_ffmpeg::InputKind::Audio)
            {
                *audio_input = audio_input.clone().with_stream_index(stream_index);
            }
        }
        PlaybackAudioSelection::Disabled => request = request.without_audio(),
        PlaybackAudioSelection::Automatic | PlaybackAudioSelection::AutomaticSilence => {}
    }
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
        if seek_after_input {
            request.state.output_start = Some(start_position);
        } else {
            request.state.start = Some(start_position);
        }
    }
    request.state.duration = duration;
    request.state.metadata = metadata;
    request.state.report_progress = false;
    if retained_stream {
        request = request.with_retained_stream_input();
    }
    apply_automatic_null_audio(&mut request, playback_profile, audio_selection);
    if let Ok(thread_count) = u32::try_from(playback_profile.profile.thread_count) {
        if thread_count > 0 {
            request.state.thread_count = Some(thread_count);
        }
    }

    if let Some(song) = song {
        if retained_stream {
            request.state.song_progress_start = Some(start_position);
        }
        song.apply(&mut request);
    }
    let acceleration = if streaming_mode == ersatzrs_domain::StreamingMode::HttpLiveStreamingDirect
    {
        ersatzrs_domain::HardwareAccelerationKind::None
    } else {
        playback_profile.profile.hardware_acceleration
    };
    ersatzrs_ffmpeg::build_pipeline_command(acceleration, request)
}

fn native_generated_audio_duration(
    selection: PlaybackAudioSelection,
    duration: Option<Duration>,
    remaining: Option<Duration>,
) -> Option<Duration> {
    // A fresh retained retry can acquire silence after an audio-bearing source.
    // Its infinite generated input must still end at the original programme bound.
    if selection == PlaybackAudioSelection::AutomaticSilence {
        duration.or(remaining)
    } else {
        duration
    }
}

fn retained_audio_selection(
    selection: PlaybackAudioSelection,
    audio: &[SourceAudioStream],
) -> PlaybackAudioSelection {
    match selection {
        PlaybackAudioSelection::Automatic => audio
            .iter()
            .find(|audio| audio.default)
            .or_else(|| audio.first())
            .map_or(selection, |audio| {
                PlaybackAudioSelection::Exact(audio.index)
            }),
        selection => selection,
    }
}

fn hls_direct_mpegts_needs_transcode(
    output_format: OutputFormat,
    streaming_mode: StreamingMode,
    source_stream: Option<&SourceVideoStream>,
    source_audio_streams: &[SourceAudioStream],
    audio_selection: PlaybackAudioSelection,
) -> bool {
    if output_format != OutputFormat::MpegTs
        || streaming_mode != StreamingMode::HttpLiveStreamingDirect
    {
        return false;
    }

    !hls_direct_mpegts_video_copy_safe(source_stream)
        || !hls_direct_mpegts_audio_copy_safe(source_audio_streams, audio_selection)
}

fn hls_direct_mpegts_video_copy_safe(source_stream: Option<&SourceVideoStream>) -> bool {
    source_stream.is_some_and(|stream| stream.codec == SourceVideoCodec::H264)
}

fn hls_direct_mpegts_audio_copy_safe(
    source_audio_streams: &[SourceAudioStream],
    audio_selection: PlaybackAudioSelection,
) -> bool {
    if audio_selection.is_disabled() {
        return true;
    }
    let Some(stream) = audio_selection
        .stream_index()
        .and_then(|index| {
            source_audio_streams
                .iter()
                .find(|stream| stream.index == index)
        })
        .or_else(|| {
            matches!(audio_selection, PlaybackAudioSelection::Automatic)
                .then(|| source_audio_streams.first())
                .flatten()
        })
    else {
        return true;
    };

    matches!(
        stream.codec,
        SourceAudioCodec::Aac | SourceAudioCodec::AacLatm | SourceAudioCodec::Ac3
    )
}

fn normalize_hls_direct_mpegts_h264_preset(
    desired: &mut DesiredVideoState,
    acceleration: ersatzrs_domain::HardwareAccelerationKind,
) {
    if desired.video_format != FFmpegProfileVideoFormat::H264 {
        return;
    }

    // Only a software encode gets a substituted preset. Hardware encoders have
    // their own option domains — FFmpeg 9's `h264_nvenc` accepts `p1`-`p7`
    // plus legacy `slow`/`medium`/`fast` and would reject `veryfast` outright —
    // and a profile that carries a preset at all already had it validated
    // against its own acceleration. A profile forced here from video format
    // Copy carries no preset, so the encoder default applies.
    if acceleration != ersatzrs_domain::HardwareAccelerationKind::None {
        return;
    }

    let has_x264_preset = desired.video_preset.as_deref().is_some_and(is_x264_preset);
    if !has_x264_preset {
        desired.video_preset = Some("veryfast".to_owned());
    }
}

fn force_software_h264_encoding(playback_profile: &mut PlaybackProfile) {
    let was_hardware = playback_profile.profile.hardware_acceleration
        != ersatzrs_domain::HardwareAccelerationKind::None;
    playback_profile.profile.hardware_acceleration =
        ersatzrs_domain::HardwareAccelerationKind::None;
    if playback_profile.profile.video_format != FFmpegProfileVideoFormat::H264 {
        return;
    }

    // A hardware profile's preset never carries over to libx264, even when the
    // string happens to be a valid x264 preset name. The ladders collide by
    // name only: QSV spells target usage 4 as `medium` and TU7 as `veryfast`,
    // which are fixed-function quality points, not CPU budgets. Reusing the
    // name would silently hand libx264 a far more expensive preset than the
    // hardware encoder was doing (ADR 0014 demotion must stay realtime).
    let keeps_preset = !was_hardware
        && playback_profile
            .profile
            .video_preset
            .as_deref()
            .is_some_and(is_x264_preset);
    if !keeps_preset {
        playback_profile.profile.video_preset = Some("veryfast".to_owned());
    }
}

/// True when `value` is a preset the software `libx264` / `libx265` encoders
/// understand.
///
/// Name membership alone never justifies reusing a *hardware* profile's preset
/// for a software encode — see [`force_software_h264_encoding`].
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
    audio_selection: impl Into<PlaybackAudioSelection>,
) -> OutputMetadata {
    let audio_selection = audio_selection.into();
    let selected_language = audio_selection.stream_index().and_then(|index| {
        source_audio_streams
            .iter()
            .find(|stream| stream.index == index)
            .and_then(|stream| stream.language.as_deref())
            .and_then(trimmed_string)
    });
    let audio_language = match audio_selection {
        PlaybackAudioSelection::Disabled | PlaybackAudioSelection::AutomaticSilence => None,
        PlaybackAudioSelection::Exact(_) => selected_language,
        PlaybackAudioSelection::Automatic => selected_language
            .or_else(|| preferred_audio_language.and_then(trimmed_string))
            .or_else(|| first_audio_language(source_audio_streams)),
    };

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
    song: Option<Arc<ersatzrs_infra::song_background::NativeSongBackground>>,
    pool: DbPool,
    /// Process-wide runtime state used by future direct and HLS sessions.
    shared_ffmpeg_runtime: Arc<RwLock<FfmpegRuntimeReport>>,
    runtime_capabilities: FfmpegCapabilityReport,
    acceleration: ersatzrs_domain::HardwareAccelerationKind,
    source_stream: Option<SourceVideoStream>,
    /// Graph the spawned command actually selected, so a runtime failure is
    /// attributed to the right operation.
    graph_operation: Option<ersatzrs_ffmpeg::HardwareGraphOperation>,
    /// `ffmpeg` path the denial is keyed to (ADR 0014).
    ffmpeg_path: String,
    /// Rebuild for a hardware-denial retry.
    ///
    /// `None` when the rebuilt command would be refused. Unreachable in
    /// practice — the initial build already accepted this source — but the
    /// retry must fail terminally rather than panic if that ever changes.
    rebuild: Option<StreamRetryBuilder>,
}

struct StreamRetryAttempt {
    command: FfmpegCommand,
    source_stream: Option<SourceVideoStream>,
    prepared: Option<PreparedRemoteStreamInput>,
}

fn retained_retry_input(
    result: Result<
        Option<ersatzrs_infra::published_interval_source::RetainedIntervalPlayback>,
        ersatzrs_infra::addon_media_list_item_runtime::ItemRuntimeError,
    >,
) -> Result<
    Option<ersatzrs_infra::published_interval_source::RetainedIntervalPlayback>,
    StreamSpawnError,
> {
    match result {
        Ok(playback) => Ok(playback),
        Err(
            ersatzrs_infra::addon_media_list_item_runtime::ItemRuntimeError::TerminationUnconfirmed,
        ) => Err(StreamSpawnError::CleanupUnconfirmed(
            "retained retry producer cleanup could not be confirmed".into(),
        )),
        Err(_) => Ok(None),
    }
}

type DeferredStreamRetry = Box<
    dyn FnOnce(
            StreamRetryMode,
            Arc<Notify>,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<
                        Output = Result<Option<StreamRetryAttempt>, StreamSpawnError>,
                    > + Send,
            >,
        > + Send,
>;

struct StreamRetryBuilder(DeferredStreamRetry);

impl StreamRetryBuilder {
    async fn build(
        self,
        mode: StreamRetryMode,
        cancel: &Arc<Notify>,
        item: Option<&LegacyWrapperItemCancellation>,
    ) -> Result<Option<StreamRetryAttempt>, StreamSpawnError> {
        if item.is_some_and(LegacyWrapperItemCancellation::is_cancelled) {
            return Err(StreamSpawnError::Spawn("legacy retry cancelled".into()));
        }
        let operation = (self.0)(mode, cancel.clone());
        tokio::pin!(operation);
        let result = if let Some(item) = item {
            tokio::select! {
                biased;
                () = item.cancelled() => {
                    // The retained retry builder already owns a cooperative
                    // preparation bridge; preserve its managed cleanup.
                    cancel.notify_one();
                    operation.await
                },
                result = &mut operation => result,
            }
        } else {
            operation.await
        }?;
        if item.is_some_and(LegacyWrapperItemCancellation::is_cancelled) {
            if let Some(attempt) = result {
                cancel_stream_retry_attempt(attempt).await?;
            }
            return Err(StreamSpawnError::Spawn("legacy retry cancelled".into()));
        }
        Ok(result)
    }
}

async fn cancel_stream_retry_attempt(
    mut attempt: StreamRetryAttempt,
) -> Result<(), StreamSpawnError> {
    if let Some(prepared) = attempt.prepared.take() {
        prepared
            .cancel()
            .await
            .map_err(|error| StreamSpawnError::prepared(&error))?;
    }
    Ok(())
}

// Used only by Unix shell-backed retry fixtures; native code uses deferred builders.
#[cfg(all(test, unix))]
fn immediate_stream_retry(
    build: Box<dyn FnOnce(StreamRetryMode) -> Option<FfmpegCommand> + Send>,
) -> StreamRetryBuilder {
    StreamRetryBuilder(Box::new(move |mode, _| {
        Box::pin(async move {
            Ok(build(mode).map(|command| StreamRetryAttempt {
                command,
                source_stream: None,
                prepared: None,
            }))
        })
    }))
}

async fn spawn_stream_retry(
    attempt: StreamRetryAttempt,
    producer: Option<&RemoteStreamCommand>,
    channel_number: &str,
    item: Option<&LegacyWrapperItemCancellation>,
) -> Result<RemoteStreamProcess, StreamSpawnError> {
    if item.is_some_and(LegacyWrapperItemCancellation::is_cancelled) {
        cancel_stream_retry_attempt(attempt).await?;
        return Err(StreamSpawnError::Spawn("legacy retry cancelled".into()));
    }
    match attempt.prepared {
        Some(prepared) => spawn_ffmpeg_with_prepared_remote_input(
            &attempt.command,
            None,
            prepared,
            RemoteStreamSpawnContext::mpeg_ts(channel_number),
        )
        .await
        .map_err(|error| StreamSpawnError::prepared(&error)),
        None => spawn_stream_process(&attempt.command, producer, channel_number),
    }
}

// The three variants share a prefix because they are three flavours of the same
// thing: a hardware capability the runtime rejected.
#[allow(clippy::enum_variant_names)]
enum StreamRetryMode {
    HardwareDecodeDenied(FfmpegCapabilityReport),
    HardwareEncodeDenied(FfmpegCapabilityReport),
    /// A hardware *graph* failed at runtime. The rebuilt capabilities carry the
    /// new denial, so the pipeline selects the next proven graph on its own.
    HardwareGraphDenied(FfmpegCapabilityReport),
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
    wrapper_close: Option<LegacyWrapperCloseHandle>,
    waiting_wrapper: Option<Arc<WaitingLegacyWrapperFence>>,
}

impl Drop for StreamBodyDisposeGuard {
    fn drop(&mut self) {
        if let Some(wrapper) = &self.wrapper_close {
            wrapper.close();
        }
        if let Some(wrapper) = &self.waiting_wrapper {
            wrapper.close();
        }
        tracing::debug!("channel stream response body dropped; cancelling ffmpeg supervisor");
        // `notify_one` stores a permit, so the supervisor still observes the
        // disposal even if it is between `notified()` awaits (for example
        // while respawning after a learned hardware-decode denial).
        self.cancel.notify_one();
    }
}

#[derive(Default)]
struct WaitingLegacyWrapperFence {
    closed: std::sync::atomic::AtomicBool,
    owner: std::sync::Mutex<Option<LegacyWrapperCloseHandle>>,
}

impl WaitingLegacyWrapperFence {
    fn attach(&self, owner: LegacyWrapperCloseHandle) {
        let mut stored = self
            .owner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if self.closed.load(Ordering::Acquire) {
            owner.close();
        } else {
            *stored = Some(owner);
        }
    }

    fn close(&self) {
        self.closed.store(true, Ordering::Release);
        let owner = self
            .owner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        if let Some(owner) = owner {
            owner.close();
        }
    }
}

enum LegacyStreamLifetime {
    Wrapper(LegacyWrapperOwner),
    Item(LegacyWrapperItemGuard),
}

impl LegacyStreamLifetime {
    fn item_cancelled(&self) -> bool {
        matches!(self, Self::Item(item) if item.cancellation().is_cancelled())
    }
    fn close_handle(&self) -> LegacyWrapperCloseHandle {
        match self {
            Self::Wrapper(owner) => owner.close_handle(),
            Self::Item(item) => item.close_handle(),
        }
    }

    async fn cancellation(&self) {
        match self {
            Self::Wrapper(owner) => owner.first_item_deadline_expired().await,
            Self::Item(item) => {
                let cancellation = item.cancellation();
                cancellation.cancelled().await;
            }
        }
    }

    fn complete(self, success: bool) {
        if let Self::Item(item) = self {
            if success {
                item.complete();
            }
        }
    }
}

enum DirectStreamOutcome {
    Completed,
    Cancelled,
    Failed(DirectStreamFailure),
    TransportFailed(TeletextPumpError),
    CleanupUnconfirmed,
}

#[derive(Default)]
struct DirectStreamFailure {
    redacted_stderr: String,
    graph_failure: Option<ersatzrs_ffmpeg::GraphFailureKind>,
    decode_denial: Option<ersatzrs_ffmpeg::HardwareDecodeDenial>,
    encoder_open_failure: bool,
    client_disconnect: bool,
    wrote_media: bool,
}

struct StreamFailureContext {
    song: Option<Arc<ersatzrs_infra::song_background::NativeSongBackground>>,
    graph: Option<ersatzrs_ffmpeg::HardwareGraphOperation>,
    acceleration: ersatzrs_domain::HardwareAccelerationKind,
    source: Option<SourceVideoStream>,
}

impl From<String> for DirectStreamFailure {
    fn from(redacted_stderr: String) -> Self {
        Self {
            redacted_stderr,
            ..Self::default()
        }
    }
}

impl std::fmt::Display for DirectStreamFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.redacted_stderr)
    }
}

fn classify_stream_stderr(
    raw: &str,
    remote_stream: Option<&ResolvedRemoteStream>,
    graph: Option<ersatzrs_ffmpeg::HardwareGraphOperation>,
    acceleration: ersatzrs_domain::HardwareAccelerationKind,
    source: Option<&SourceVideoStream>,
) -> DirectStreamFailure {
    DirectStreamFailure {
        redacted_stderr: remote_stream.map_or_else(
            || raw.to_owned(),
            |stream| redact_remote_stream_output(raw, stream),
        ),
        graph_failure: graph
            .and_then(|graph| ersatzrs_ffmpeg::GraphFailureKind::classify_runtime(graph, raw)),
        decode_denial: learned_hardware_decode_denial(false, acceleration, source, raw),
        encoder_open_failure: ffmpeg_stderr_is_hardware_encoder_open_failure(raw),
        client_disconnect: ffmpeg_stderr_is_client_disconnect(raw),
        wrote_media: false,
    }
}

fn spawn_stream_process(
    command: &FfmpegCommand,
    remote_producer: Option<&RemoteStreamCommand>,
    channel_number: &str,
) -> Result<RemoteStreamProcess, StreamSpawnError> {
    spawn_ffmpeg_with_remote_input(
        command,
        None,
        remote_producer,
        RemoteStreamSpawnContext::mpeg_ts(channel_number),
    )
    .map_err(|error| {
        // With no producer, every error precedes consumer creation. A paired
        // producer adapter can fail after creating the consumer, so retain its
        // assets unless that adapter gives an explicit reaping guarantee.
        if remote_producer.is_none() {
            StreamSpawnError::Spawn(error.to_string())
        } else {
            StreamSpawnError::CleanupUnconfirmed(error.to_string())
        }
    })
}

#[allow(clippy::too_many_arguments)]
async fn spawn_stream_body(
    command: &FfmpegCommand,
    cleanup_paths: Vec<PathBuf>,
    dynamic_text_controllers: Vec<DynamicTextController>,
    channel_number: String,
    session_registry: Arc<dyn HlsSessionRegistry>,
    capacity_reservation: Option<ActiveTranscodeReservation>,
    retry: Option<StreamDenialRetry>,
    remote_stream: Option<ResolvedRemoteStream>,
    output: TeletextOutputAdmission,
) -> Result<Body, StreamSpawnError> {
    spawn_stream_body_with_prepared_input(
        command,
        cleanup_paths,
        dynamic_text_controllers,
        channel_number,
        session_registry,
        capacity_reservation,
        retry,
        remote_stream,
        None,
        None,
        None,
        output,
    )
    .await
}

async fn render_stream_song(
    song: Arc<ersatzrs_infra::song_background::NativeSongBackground>,
    reservation: Option<ActiveTranscodeReservation>,
    cancel: Option<&Arc<Notify>>,
) -> Result<(), StreamSpawnError> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let operation = ersatzrs_infra::song_background::render_native(
        song,
        tokio::time::Instant::now() + Duration::from_secs(15),
        receiver,
        reservation,
    );
    tokio::pin!(operation);
    let result = if let Some(cancel) = cancel {
        tokio::select! {
            result = &mut operation => result,
            () = cancel.notified() => { let _ = sender.send(()); operation.await }
        }
    } else {
        let _sender = sender;
        operation.await
    };
    result.map_err(|error| {
        if error.termination_unconfirmed() {
            StreamSpawnError::CleanupUnconfirmed(error.to_string())
        } else {
            StreamSpawnError::Spawn(error.to_string())
        }
    })
}

#[allow(clippy::too_many_arguments)]
async fn spawn_stream_body_with_prepared_input(
    command: &FfmpegCommand,
    cleanup_paths: Vec<PathBuf>,
    dynamic_text_controllers: Vec<DynamicTextController>,
    channel_number: String,
    session_registry: Arc<dyn HlsSessionRegistry>,
    capacity_reservation: Option<ActiveTranscodeReservation>,
    retry: Option<StreamDenialRetry>,
    remote_stream: Option<ResolvedRemoteStream>,
    prepared: Option<PreparedRemoteStreamInput>,
    subtitle_owner: Option<ersatzrs_infra::external_json::subtitles::PreparedSubtitle>,
    terminal: Option<(Arc<std::sync::atomic::AtomicU8>, Arc<Notify>)>,
    output: TeletextOutputAdmission,
) -> Result<Body, StreamSpawnError> {
    spawn_stream_body_with_lifetime(
        command,
        cleanup_paths,
        dynamic_text_controllers,
        channel_number,
        session_registry,
        capacity_reservation,
        retry,
        remote_stream,
        prepared,
        subtitle_owner,
        terminal,
        output,
        None,
    )
    .await
}

#[allow(
    clippy::too_many_arguments,
    reason = "Extends the existing admitted producer context with its exact wrapper or item lifetime"
)]
#[expect(
    clippy::too_many_lines,
    reason = "Keeps admitted producer, song, subtitle and capacity ownership together through startup and supervised cleanup"
)]
async fn spawn_stream_body_with_lifetime(
    command: &FfmpegCommand,
    cleanup_paths: Vec<PathBuf>,
    mut dynamic_text_controllers: Vec<DynamicTextController>,
    channel_number: String,
    session_registry: Arc<dyn HlsSessionRegistry>,
    capacity_reservation: Option<ActiveTranscodeReservation>,
    retry: Option<StreamDenialRetry>,
    remote_stream: Option<ResolvedRemoteStream>,
    mut prepared: Option<PreparedRemoteStreamInput>,
    mut subtitle_owner: Option<ersatzrs_infra::external_json::subtitles::PreparedSubtitle>,
    terminal: Option<(Arc<std::sync::atomic::AtomicU8>, Arc<Notify>)>,
    output: TeletextOutputAdmission,
    lifetime: Option<LegacyStreamLifetime>,
) -> Result<Body, StreamSpawnError> {
    if lifetime
        .as_ref()
        .is_some_and(LegacyStreamLifetime::item_cancelled)
    {
        let confirmed = cancel_retained_startup(
            prepared.take(),
            capacity_reservation.as_ref(),
            Some(&mut subtitle_owner),
            &cleanup_paths,
            Some(&mut dynamic_text_controllers),
        )
        .await;
        return Err(if confirmed {
            StreamSpawnError::Spawn("legacy item startup cancelled".into())
        } else {
            StreamSpawnError::CleanupUnconfirmed("legacy item startup cleanup unconfirmed".into())
        });
    }
    if let Some(song) = retry.as_ref().and_then(|retry| retry.song.clone()) {
        let rendered = {
            let render_cancel = Arc::new(Notify::new());
            let operation =
                render_stream_song(song, capacity_reservation.clone(), Some(&render_cancel));
            tokio::pin!(operation);
            if let Some(LegacyStreamLifetime::Item(item)) = &lifetime {
                let cancellation = item.cancellation();
                tokio::select! {
                    biased;
                    () = cancellation.cancelled() => {
                        render_cancel.notify_one();
                        operation.await
                    },
                    result = &mut operation => result,
                }
            } else {
                operation.await
            }
        };
        if let Err(error) = rendered {
            if !error.cleanup_confirmed() {
                if let Some(reservation) = &capacity_reservation {
                    reservation.retain_until_restart();
                }
                if let Some(owner) = subtitle_owner.take() {
                    owner.preserve_after_unconfirmed_cleanup();
                }
            }
            let confirmed = cancel_retained_startup(
                prepared.take(),
                capacity_reservation.as_ref(),
                Some(&mut subtitle_owner),
                if error.cleanup_confirmed() {
                    &cleanup_paths
                } else {
                    &[]
                },
                Some(&mut dynamic_text_controllers),
            )
            .await;
            return Err(if confirmed {
                error
            } else {
                StreamSpawnError::CleanupUnconfirmed(
                    "legacy item startup cleanup unconfirmed".into(),
                )
            });
        }
    }
    if lifetime
        .as_ref()
        .is_some_and(LegacyStreamLifetime::item_cancelled)
    {
        let confirmed = cancel_retained_startup(
            prepared.take(),
            capacity_reservation.as_ref(),
            Some(&mut subtitle_owner),
            &cleanup_paths,
            Some(&mut dynamic_text_controllers),
        )
        .await;
        return Err(if confirmed {
            StreamSpawnError::Spawn("legacy item startup cancelled".into())
        } else {
            StreamSpawnError::CleanupUnconfirmed("legacy item startup cleanup unconfirmed".into())
        });
    }
    let song = retry.as_ref().and_then(|retry| retry.song.as_ref());
    if let Some(song) = song {
        song.mark_in_use();
    }
    let spawned = match prepared {
        Some(prepared) => spawn_ffmpeg_with_prepared_remote_input(
            command,
            None,
            prepared,
            RemoteStreamSpawnContext::mpeg_ts(&channel_number),
        )
        .await
        .map_err(|error| StreamSpawnError::prepared(&error)),
        None => spawn_stream_process(
            command,
            remote_stream.as_ref().and_then(remote_stream_producer),
            &channel_number,
        ),
    };
    let mut child = match spawned {
        Ok(child) => child,
        Err(error) => {
            if error.cleanup_confirmed() {
                if let Some(song) = song {
                    song.confirm_reaped();
                }
            } else {
                if let Some(reservation) = &capacity_reservation {
                    reservation.retain_until_restart();
                }
                if let Some(owner) = subtitle_owner {
                    owner.preserve_after_unconfirmed_cleanup();
                }
            }
            return Err(error);
        }
    };
    // The immutable snapshot is the command that created this exact child.
    // Plain/fixed callers retain no new argv copy or application context.
    let output = output.with_command(command);
    if child.stdout.is_none() {
        return match reap_stream_process(&mut child).await {
            Ok(()) => {
                if let Some(song) = song {
                    song.confirm_reaped();
                }
                Err(StreamSpawnError::MissingStdout)
            }
            Err(error) => {
                if let Some(reservation) = &capacity_reservation {
                    reservation.retain_until_restart();
                }
                if let Some(owner) = subtitle_owner {
                    owner.preserve_after_unconfirmed_cleanup();
                }
                Err(StreamSpawnError::CleanupUnconfirmed(error.to_string()))
            }
        };
    }
    let mut handle = DirectStreamHandle::new(channel_number.clone());
    if let Some(LegacyStreamLifetime::Wrapper(owner)) = &lifetime {
        handle.cancel = owner.cancel_signal();
    }
    let handle = Arc::new(handle);
    let cancel = handle.cancel.clone();
    let handle_cancel = handle.cancel.clone();
    let handle: Arc<dyn HlsSessionHandle> = handle;
    let (reader, mut writer) = tokio::io::duplex(STREAM_BODY_BUFFER_BYTES);
    let (body, output_completion) = stream_body_for_lifetime(
        reader,
        handle_cancel,
        &output,
        lifetime.is_some(),
        lifetime.as_ref().map(LegacyStreamLifetime::close_handle),
    );

    tokio::spawn(async move {
        let capacity_guard = capacity_reservation;
        session_registry.register_direct(handle.clone()).await;
        let mut retry = retry;
        let song_owner = retry.as_ref().and_then(|retry| retry.song.clone());
        let item_cancellation = lifetime.as_ref().and_then(|lifetime| match lifetime {
            LegacyStreamLifetime::Item(item) => Some(item.cancellation()),
            LegacyStreamLifetime::Wrapper(_) => None,
        });
        let outcome = {
            let operation = supervise_stream_command_with_retry_cancellable(
                child,
                &mut writer,
                &cancel,
                &channel_number,
                &mut retry,
                remote_stream.as_ref(),
                &output,
                item_cancellation.as_ref(),
            );
            tokio::pin!(operation);
            let lifetime_cancel = async {
                match &lifetime {
                    Some(lifetime) => lifetime.cancellation().await,
                    None => std::future::pending::<()>().await,
                }
            };
            tokio::select! {
                biased;
                () = lifetime_cancel => {
                    if let Some(LegacyStreamLifetime::Wrapper(owner)) = &lifetime {
                        owner.close();
                    }
                    cancel.notify_one();
                    operation.await
                },
                outcome = &mut operation => outcome,
            }
        };
        let cleanup_unconfirmed = matches!(outcome, DirectStreamOutcome::CleanupUnconfirmed)
            || song_owner
                .as_ref()
                .is_some_and(|song| song.cleanup_unconfirmed());
        if cleanup_unconfirmed {
            if let Some(reservation) = &capacity_guard {
                reservation.retain_until_restart();
            }
            if let Some(owner) = subtitle_owner {
                owner.preserve_after_unconfirmed_cleanup();
            }
        }
        let outcome = if cleanup_unconfirmed {
            DirectStreamOutcome::CleanupUnconfirmed
        } else {
            outcome
        };
        session_registry.unregister_direct(&handle).await;
        shutdown_dynamic_text_controllers(dynamic_text_controllers).await;
        if !cleanup_unconfirmed {
            for path in cleanup_paths {
                cleanup_stream_artifact(&path).await;
            }
        }
        // The next concat request may arrive immediately after terminal EOF.
        // Release the actual encoder and exclusive item permit before it.
        drop(capacity_guard);
        if let Some(lifetime) = lifetime {
            lifetime.complete(matches!(outcome, DirectStreamOutcome::Completed));
        }
        let close_output = terminal.is_some() || output_completion.is_some();
        if let Some((terminal, completed)) = terminal {
            terminal.store(
                if matches!(outcome, DirectStreamOutcome::Completed) {
                    TERMINAL_SUCCESS
                } else {
                    TERMINAL_FAILURE
                },
                Ordering::Release,
            );
            completed.notify_one();
        }
        if let Some(completion) = output_completion {
            let _ = completion.send(stream_output_result(&outcome));
        }
        if close_output {
            drop(writer);
        }
    });

    Ok(body)
}

#[allow(clippy::too_many_arguments)]
#[expect(
    clippy::too_many_lines,
    reason = "Keeps capacity admission, cancellation, slate retry and uncertainty quarantine in one supervised lifetime"
)]
fn spawn_stream_body_waiting_for_mpegts_capacity(
    command: FfmpegCommand,
    slate_command: FfmpegCommand,
    cleanup_paths: Vec<PathBuf>,
    dynamic_text_controllers: Vec<DynamicTextController>,
    channel_number: String,
    session_registry: Arc<dyn HlsSessionRegistry>,
    pool: DbPool,
    retry: Option<StreamDenialRetry>,
    remote_stream: Option<ResolvedRemoteStream>,
    subtitle_owner: Option<ersatzrs_infra::external_json::subtitles::PreparedSubtitle>,
    output: TeletextOutputAdmission,
) -> Body {
    let handle = Arc::new(DirectStreamHandle::new(channel_number.clone()));
    let cancel = handle.cancel.clone();
    let handle_cancel = handle.cancel.clone();
    let handle: Arc<dyn HlsSessionHandle> = handle;
    let (reader, mut writer) = tokio::io::duplex(STREAM_BODY_BUFFER_BYTES);
    let (body, output_completion) = stream_body_for_output(reader, handle_cancel, &output);

    tokio::spawn(async move {
        session_registry.register_direct(handle.clone()).await;
        let mut retry = retry;
        let staged_song_owner = retry.as_ref().and_then(|retry| retry.song.clone());
        let mut output_result = Err("encoded output ended before producer startup");
        let mut cleanup_unconfirmed = false;
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
                let capacity_guard = reservation;
                if let Some(song) = retry.as_ref().and_then(|retry| retry.song.clone()) {
                    if let Err(error) =
                        render_stream_song(song, Some(capacity_guard.clone()), Some(&cancel)).await
                    {
                        tracing::warn!(%error, channel_number, "failed to prepare song after capacity became available");
                        break;
                    }
                }
                let producer = remote_stream.as_ref().and_then(remote_stream_producer);
                if let Some(song) = retry.as_ref().and_then(|retry| retry.song.as_ref()) {
                    song.mark_in_use();
                }
                match spawn_stream_process(&command, producer, &channel_number) {
                    Ok(child) => {
                        let song_owner = retry.as_ref().and_then(|retry| retry.song.clone());
                        let outcome = supervise_stream_command_with_retry(
                            child,
                            &mut writer,
                            &cancel,
                            &channel_number,
                            &mut retry,
                            remote_stream.as_ref(),
                            &output,
                        )
                        .await;
                        output_result = stream_output_result(&outcome);
                        cleanup_unconfirmed =
                            matches!(outcome, DirectStreamOutcome::CleanupUnconfirmed)
                                || song_owner
                                    .as_ref()
                                    .is_some_and(|song| song.cleanup_unconfirmed());
                        if cleanup_unconfirmed {
                            capacity_guard.retain_until_restart();
                        }
                    }
                    Err(error) => {
                        if error.cleanup_confirmed() {
                            if let Some(song) = retry.as_ref().and_then(|retry| retry.song.as_ref())
                            {
                                song.confirm_reaped();
                            }
                        } else {
                            cleanup_unconfirmed = true;
                            capacity_guard.retain_until_restart();
                        }
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
            let mut slate_child = match spawn_stream_process(&slate_command, None, &channel_number)
            {
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
            match supervise_stream_process(
                &mut slate_child,
                &mut writer,
                &cancel,
                &channel_number,
                None,
            )
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
                DirectStreamOutcome::TransportFailed(error) => {
                    tracing::warn!(%error, channel_number, "MPEG-TS slate transport failed");
                    break;
                }
                DirectStreamOutcome::CleanupUnconfirmed => {
                    cleanup_unconfirmed = true;
                    break;
                }
            }
        }
        if let Some(completion) = output_completion {
            let _ = completion.send(output_result);
            drop(writer);
        }
        session_registry.unregister_direct(&handle).await;
        shutdown_dynamic_text_controllers(dynamic_text_controllers).await;
        let cleanup_unconfirmed = cleanup_unconfirmed
            || staged_song_owner
                .as_ref()
                .is_some_and(|song| song.cleanup_unconfirmed());
        if cleanup_unconfirmed {
            if let Some(owner) = subtitle_owner {
                owner.preserve_after_unconfirmed_cleanup();
            }
        } else {
            for path in cleanup_paths {
                cleanup_stream_artifact(&path).await;
            }
        }
    });

    body
}

// One linear failure-classification ladder: graph denial, then decode denial,
// then encoder rejection, each with its own persist-and-respawn. Splitting it
// would mean threading the FnOnce rebuild and the &mut child through helpers
// for no gain in clarity.
async fn supervise_stream_command_with_retry(
    child: RemoteStreamProcess,
    writer: &mut tokio::io::DuplexStream,
    cancel: &Arc<Notify>,
    channel_number: &str,
    retry: &mut Option<StreamDenialRetry>,
    remote_stream: Option<&ResolvedRemoteStream>,
    output: &TeletextOutputAdmission,
) -> DirectStreamOutcome {
    supervise_stream_command_with_retry_cancellable(
        child,
        writer,
        cancel,
        channel_number,
        retry,
        remote_stream,
        output,
        None,
    )
    .await
}

#[allow(
    clippy::too_many_lines,
    reason = "Keeps the existing failure classification and bounded retry ownership in one linear ladder"
)]
#[allow(
    clippy::too_many_arguments,
    reason = "Adds the exact item cancellation state to the existing linear retry ownership ladder"
)]
async fn supervise_stream_command_with_retry_cancellable(
    mut child: RemoteStreamProcess,
    writer: &mut tokio::io::DuplexStream,
    cancel: &Arc<Notify>,
    channel_number: &str,
    retry: &mut Option<StreamDenialRetry>,
    remote_stream: Option<&ResolvedRemoteStream>,
    output: &TeletextOutputAdmission,
    item: Option<&LegacyWrapperItemCancellation>,
) -> DirectStreamOutcome {
    let mut output = output.clone();
    let song = retry.as_ref().and_then(|retry| retry.song.clone());
    loop {
        let outcome = supervise_stream_process_with_retry_context(
            &mut child,
            writer,
            cancel,
            channel_number,
            remote_stream,
            retry
                .as_ref()
                .map(|retry| StreamFailureContext {
                    song: song.clone(),
                    graph: retry.graph_operation,
                    acceleration: retry.acceleration,
                    source: retry.source_stream.clone(),
                })
                .or_else(|| {
                    song.as_ref().map(|song| StreamFailureContext {
                        song: Some(Arc::clone(song)),
                        graph: None,
                        acceleration: ersatzrs_domain::HardwareAccelerationKind::None,
                        source: Some(song.stream.clone()),
                    })
                }),
            &output,
        )
        .await;
        let DirectStreamOutcome::Failed(mut stderr) = outcome else {
            return outcome;
        };
        if stderr.wrote_media
            && (output.has_authored_output()
                || remote_stream.is_some_and(|stream| {
                    matches!(stream.source, RemoteStreamSource::RetainedIntervals(_))
                }))
        {
            // The reserve or authored response may already contain this attempt's
            // TS prefix. Never concatenate a restarted clock onto those bytes.
            return DirectStreamOutcome::Failed(stderr);
        }
        let Some(mut context) = retry.take() else {
            return DirectStreamOutcome::Failed(stderr);
        };

        // ADR 0014: a graph that only fails under real media cannot be caught by
        // startup probing, so the denial is learned from the failure itself and
        // published even when the session has already spent its one retry.
        // Checked before the decode classifier because a graph failure is the
        // more specific diagnosis.
        if let (Some(operation), Some(kind)) = (context.graph_operation, stderr.graph_failure) {
            let device = context.runtime_capabilities.hardware_graph_device();
            let denial = ersatzrs_ffmpeg::HardwareGraphDenial::new(operation, &device, kind);
            let evidence = denial.evidence();
            if let Err(error) =
                ersatzrs_infra::hardware_graph::persist_hardware_graph_denial_into_runtime(
                    &context.pool,
                    &context.ffmpeg_path,
                    &mut context.runtime_capabilities,
                    &context.shared_ffmpeg_runtime,
                    denial.clone(),
                )
                .await
            {
                tracing::warn!(
                    %error,
                    channel_number = %channel_number,
                    "failed to persist runtime-learned hardware graph denial"
                );
            }

            let Some(build_retry_command) = context.rebuild.take() else {
                tracing::warn!(
                    channel_number = %channel_number,
                    evidence,
                    "learned fallback graph denial after the session retry was already used"
                );
                return DirectStreamOutcome::Failed(stderr);
            };
            tracing::warn!(
                channel_number = %channel_number,
                evidence,
                "retrying channel stream once through the next proven graph after a runtime graph failure"
            );
            let retry_command = match build_retry_command
                .build(
                    StreamRetryMode::HardwareGraphDenied(context.runtime_capabilities.clone()),
                    cancel,
                    item,
                )
                .await
            {
                Ok(Some(attempt)) => attempt,
                Ok(None) => {
                    tracing::warn!(
                        channel_number = %channel_number,
                        "rebuilt command was refused; not retrying after graph denial"
                    );
                    return DirectStreamOutcome::Failed(
                        "rebuilt command was refused".to_owned().into(),
                    );
                }
                Err(error) => {
                    tracing::warn!(%error, channel_number = %channel_number, "retained retry preparation failed");
                    return failed_stream_spawn_outcome(&error);
                }
            };
            let next_graph_operation =
                ersatzrs_ffmpeg::selected_hardware_graph(&retry_command.command);
            let next_ffmpeg_path = retry_command.command.executable.clone();
            if retry_command.source_stream.is_some() {
                context
                    .source_stream
                    .clone_from(&retry_command.source_stream);
            }
            if let Some(song) = &song {
                song.mark_in_use();
            }
            let next_output = output.with_command(&retry_command.command);
            match spawn_stream_retry(
                retry_command,
                remote_stream.and_then(remote_stream_producer),
                channel_number,
                item,
            )
            .await
            {
                Ok(next) => {
                    child = next;
                    output = next_output;
                    context.graph_operation = next_graph_operation;
                    context.ffmpeg_path = next_ffmpeg_path;
                    // Keep an observer for the fallback spawn. It may learn a
                    // second denial, but cannot trigger another retry.
                    context.rebuild = None;
                    *retry = Some(context);
                }
                Err(error) => {
                    if error.cleanup_confirmed() {
                        if let Some(song) = &song {
                            song.confirm_reaped();
                        }
                    }
                    tracing::warn!(%error, channel_number = %channel_number, "failed to respawn ffmpeg channel stream after graph denial");
                    return failed_stream_spawn_outcome(&error);
                }
            }
            continue;
        }

        if let Some(denial) = stderr
            .decode_denial
            .take()
            .filter(|_| context.rebuild.is_some())
        {
            tracing::warn!(
                channel_number = %channel_number,
                acceleration = ?denial.acceleration,
                codec = %denial.codec,
                bit_depth = denial.bit_depth,
                "retrying channel stream once with software input decode after runtime hardware decode rejection"
            );
            if let Err(error) = persist_hardware_decode_denial_into_runtime(
                &context.pool,
                &mut context.runtime_capabilities,
                &context.shared_ffmpeg_runtime,
                denial,
            )
            .await
            {
                tracing::warn!(
                    %error,
                    channel_number = %channel_number,
                    "failed to persist runtime-learned hardware decode denial"
                );
            }
            let Some(rebuild) = context.rebuild.take() else {
                return DirectStreamOutcome::Failed(stderr);
            };
            let retry_command = match rebuild
                .build(
                    StreamRetryMode::HardwareDecodeDenied(context.runtime_capabilities.clone()),
                    cancel,
                    item,
                )
                .await
            {
                Ok(Some(attempt)) => attempt,
                Ok(None) => {
                    tracing::warn!(
                        channel_number = %channel_number,
                        "rebuilt command was refused; not retrying after hardware decode denial"
                    );
                    return DirectStreamOutcome::Failed(
                        "rebuilt command was refused".to_owned().into(),
                    );
                }
                Err(error) => {
                    tracing::warn!(%error, channel_number = %channel_number, "retained retry preparation failed");
                    return failed_stream_spawn_outcome(&error);
                }
            };
            let next_graph_operation =
                ersatzrs_ffmpeg::selected_hardware_graph(&retry_command.command);
            let next_ffmpeg_path = retry_command.command.executable.clone();
            if retry_command.source_stream.is_some() {
                context
                    .source_stream
                    .clone_from(&retry_command.source_stream);
            }
            if let Some(song) = &song {
                song.mark_in_use();
            }
            let next_output = output.with_command(&retry_command.command);
            match spawn_stream_retry(
                retry_command,
                remote_stream.and_then(remote_stream_producer),
                channel_number,
                item,
            )
            .await
            {
                Ok(next) => {
                    child = next;
                    output = next_output;
                    context.graph_operation = next_graph_operation;
                    context.ffmpeg_path = next_ffmpeg_path;
                    // The retry is spent, but its hardware graph can still
                    // produce new evidence that future sessions must learn.
                    context.rebuild = None;
                    *retry = Some(context);
                }
                Err(error) => {
                    if error.cleanup_confirmed() {
                        if let Some(song) = &song {
                            song.confirm_reaped();
                        }
                    }
                    tracing::warn!(%error, channel_number = %channel_number, "failed to respawn ffmpeg channel stream after hardware decode denial");
                    return failed_stream_spawn_outcome(&error);
                }
            }
            continue;
        }

        if context.acceleration == ersatzrs_domain::HardwareAccelerationKind::None
            || !stderr.encoder_open_failure
        {
            return DirectStreamOutcome::Failed(stderr);
        }

        let Some(build_retry_command) = context.rebuild.take() else {
            tracing::warn!(
                channel_number = %channel_number,
                acceleration = ?context.acceleration,
                "hardware encoder rejection observed after the session retry was already used"
            );
            return DirectStreamOutcome::Failed(stderr);
        };
        tracing::warn!(
            channel_number = %channel_number,
            acceleration = ?context.acceleration,
            "retrying channel stream once with software video encode after runtime hardware encoder rejection"
        );
        let retry_command = match build_retry_command
            .build(
                StreamRetryMode::HardwareEncodeDenied(context.runtime_capabilities),
                cancel,
                item,
            )
            .await
        {
            Ok(Some(attempt)) => attempt,
            Ok(None) => {
                tracing::warn!(
                    channel_number = %channel_number,
                    "rebuilt command was refused; not retrying after hardware encoder rejection"
                );
                return DirectStreamOutcome::Failed(
                    "rebuilt command was refused".to_owned().into(),
                );
            }
            Err(error) => {
                tracing::warn!(%error, channel_number = %channel_number, "retained retry preparation failed");
                return failed_stream_spawn_outcome(&error);
            }
        };
        if let Some(song) = &song {
            song.mark_in_use();
        }
        let next_output = output.with_command(&retry_command.command);
        match spawn_stream_retry(
            retry_command,
            remote_stream.and_then(remote_stream_producer),
            channel_number,
            item,
        )
        .await
        {
            Ok(next) => {
                child = next;
                output = next_output;
            }
            Err(error) => {
                if error.cleanup_confirmed() {
                    if let Some(song) = &song {
                        song.confirm_reaped();
                    }
                }
                tracing::warn!(%error, channel_number = %channel_number, "failed to respawn ffmpeg channel stream after hardware encoder rejection");
                return failed_stream_spawn_outcome(&error);
            }
        }
    }
}

fn failed_stream_spawn_outcome(error: &StreamSpawnError) -> DirectStreamOutcome {
    if error.cleanup_confirmed() {
        DirectStreamOutcome::Failed(error.to_string().into())
    } else {
        DirectStreamOutcome::CleanupUnconfirmed
    }
}

/// Builds the response [`Body`], tying the `FFmpeg` process lifetime to the
/// response: dropping the body fires [`StreamBodyDisposeGuard`], which cancels
/// the supervisor and kills the process (`RegisterForDispose` parity).
fn guarded_stream_body(reader: tokio::io::DuplexStream, cancel: Arc<Notify>) -> Body {
    use futures_util::StreamExt;

    let guard = StreamBodyDisposeGuard {
        cancel,
        wrapper_close: None,
        waiting_wrapper: None,
    };
    Body::from_stream(ReaderStream::new(reader).map(move |chunk| {
        let _held_until_body_drop = &guard;
        chunk
    }))
}

type StreamOutputCompletion = tokio::sync::oneshot::Sender<Result<(), &'static str>>;

fn stream_output_result(outcome: &DirectStreamOutcome) -> Result<(), &'static str> {
    match outcome {
        DirectStreamOutcome::Completed => Ok(()),
        DirectStreamOutcome::Cancelled => Err("encoded output was cancelled"),
        DirectStreamOutcome::Failed(_) => Err("encoded output producer failed"),
        DirectStreamOutcome::TransportFailed(_) => Err("teletext transport failed"),
        DirectStreamOutcome::CleanupUnconfirmed => Err("encoded output cleanup is unconfirmed"),
    }
}

fn stream_body_for_output(
    reader: tokio::io::DuplexStream,
    cancel: Arc<Notify>,
    output: &TeletextOutputAdmission,
) -> (Body, Option<StreamOutputCompletion>) {
    stream_body_for_lifetime(reader, cancel, output, false, None)
}

fn stream_body_for_lifetime(
    reader: tokio::io::DuplexStream,
    cancel: Arc<Notify>,
    output: &TeletextOutputAdmission,
    force_completion: bool,
    wrapper_close: Option<LegacyWrapperCloseHandle>,
) -> (Body, Option<StreamOutputCompletion>) {
    stream_body_for_wrapper_wait(
        reader,
        cancel,
        output,
        force_completion,
        wrapper_close,
        None,
    )
}

fn stream_body_for_wrapper_wait(
    reader: tokio::io::DuplexStream,
    cancel: Arc<Notify>,
    output: &TeletextOutputAdmission,
    force_completion: bool,
    wrapper_close: Option<LegacyWrapperCloseHandle>,
    waiting_wrapper: Option<Arc<WaitingLegacyWrapperFence>>,
) -> (Body, Option<StreamOutputCompletion>) {
    use futures_util::StreamExt as _;

    if !output.has_authored_output() && !force_completion {
        return (guarded_stream_body(reader, cancel), None);
    }
    let (sender, completion) = tokio::sync::oneshot::channel();
    let terminal = futures_util::stream::once(async move {
        match completion.await {
            Ok(Ok(())) => None,
            Ok(Err(message)) => Some(Err(std::io::Error::other(message))),
            Err(_) => Some(Err(std::io::Error::other(
                "encoded output supervisor ended",
            ))),
        }
    })
    .filter_map(std::future::ready);
    let guard = StreamBodyDisposeGuard {
        cancel,
        wrapper_close,
        waiting_wrapper,
    };
    let body = Body::from_stream(ReaderStream::new(reader).chain(terminal).map(move |chunk| {
        let _held_until_body_drop = &guard;
        chunk
    }));
    (body, Some(sender))
}

async fn supervise_stream_process(
    child: &mut RemoteStreamProcess,
    writer: &mut tokio::io::DuplexStream,
    cancel: &Arc<Notify>,
    channel_number: &str,
    remote_stream: Option<&ResolvedRemoteStream>,
) -> DirectStreamOutcome {
    supervise_stream_process_with_retry_context(
        child,
        writer,
        cancel,
        channel_number,
        remote_stream,
        None,
        &TeletextOutputAdmission::plain(TeletextOutputRole::CapacitySlate),
    )
    .await
}

async fn reap_stream_process(child: &mut RemoteStreamProcess) -> std::io::Result<()> {
    tokio::time::timeout(Duration::from_secs(5), child.kill())
        .await
        .unwrap_or_else(|_| Err(std::io::ErrorKind::TimedOut.into()))
}

#[expect(
    clippy::too_many_lines,
    reason = "Keeps stdout pumping, cancellation, process reaping and failure classification in their required teardown order"
)]
async fn supervise_stream_process_with_retry_context(
    child: &mut RemoteStreamProcess,
    writer: &mut tokio::io::DuplexStream,
    cancel: &Arc<Notify>,
    channel_number: &str,
    remote_stream: Option<&ResolvedRemoteStream>,
    retry: Option<StreamFailureContext>,
    output: &TeletextOutputAdmission,
) -> DirectStreamOutcome {
    let mut stdout = child.stdout.take();
    let stderr_remote_stream = remote_stream.cloned();
    let graph = retry.as_ref().and_then(|retry| retry.graph);
    let acceleration = retry
        .as_ref()
        .map_or(ersatzrs_domain::HardwareAccelerationKind::None, |retry| {
            retry.acceleration
        });
    let song = retry.as_ref().and_then(|retry| retry.song.clone());
    if let Some(song) = &song {
        song.mark_in_use();
    }
    let source = retry.and_then(|retry| retry.source);
    let stderr_task = child.stderr.take().map(|stderr| {
        tokio::spawn(async move {
            let stderr = read_ffmpeg_stderr_tail(stderr).await;
            classify_stream_stderr(
                &stderr,
                stderr_remote_stream.as_ref(),
                graph,
                acceleration,
                source.as_ref(),
            )
        })
    });

    // Pump stdout into the response body until EOF (process exit), a body
    // write failure (client gone), or session cancellation.
    let pump = async {
        match stdout.as_mut() {
            Some(stdout) => match output.binding() {
                Some(binding) => {
                    // Every child gets fresh PSI/PCR/continuity and timing state.
                    super::teletext_optional_output::forward_bound_teletext_output(
                        stdout, writer, binding,
                    )
                    .await
                }
                None if output.live().is_some() => {
                    let live = output.live().ok_or(TeletextPumpError::Configuration)?;
                    let command = output.command().ok_or(TeletextPumpError::Configuration)?;
                    let result = super::teletext_live_output::forward_live_teletext_output(
                        stdout,
                        writer,
                        command,
                        &live.intent,
                        live.deadline,
                    )
                    .await?;
                    if let Some(rejection) = result.rejection {
                        tracing::debug!(%rejection, "live teletext rejected before commitment; preserving raw output");
                    }
                    Ok(result.bytes)
                }
                None if output.accepted().is_some() => {
                    let accepted = output.accepted().ok_or(TeletextPumpError::Configuration)?;
                    let command = output.command().ok_or(TeletextPumpError::Configuration)?;
                    let result = forward_accepted_teletext_output(
                        stdout,
                        writer,
                        command,
                        &accepted.intent,
                        &accepted.application,
                        accepted.deadline,
                    )
                    .await?;
                    if let Some(rejection) = result.rejection {
                        tracing::debug!(%rejection, "accepted teletext rejected before commitment; preserving raw output");
                    }
                    Ok(result.bytes)
                }
                None => tokio::io::copy(stdout, writer)
                    .await
                    .map_err(|error| TeletextPumpError::Write(error.kind())),
            },
            None if output.has_authored_output() => Err(TeletextPumpError::EmptyOutput),
            None => Ok(0),
        }
    };
    let (wrote_media, transport_error) = tokio::select! {
        biased;
        () = cancel.notified() => {
            tracing::debug!(channel_number = %channel_number, "channel stream cancelled; killing ffmpeg");
            let reaped = match reap_stream_process(child).await {
                Ok(()) => { if let Some(song) = &song { song.confirm_reaped(); } true },
                Err(error) => { tracing::warn!(%error, channel_number = %channel_number, "failed to kill ffmpeg channel stream"); false },
            };
            let stderr = await_stderr_tail(stderr_task).await;
            if !stderr.redacted_stderr.is_empty() {
                tracing::debug!(stderr = %stderr, channel_number = %channel_number, "ffmpeg channel stream stderr after cancellation");
            }
            return if reaped { DirectStreamOutcome::Cancelled } else { DirectStreamOutcome::CleanupUnconfirmed };
        }
        result = pump => {
            let empty_output = matches!(&result, Err(TeletextPumpError::EmptyOutput));
            if result.is_err() && !empty_output {
                // The body side is gone; stop the process instead of letting
                // it block on a pipe nobody drains.
                tracing::debug!(channel_number = %channel_number, "channel stream body writer failed; killing ffmpeg");
                let _ = child.start_kill();
            }
            let wrote_media = !empty_output && result.as_ref().map_or(true, |bytes| *bytes > 0);
            let transport_error = if output.has_authored_output() { result.err() } else { None };
            (wrote_media, transport_error)
        }
    };

    // Close our pipe after forwarding/cancellation, including shell-producer
    // failures, so an inherited stdout cannot retain an abandoned output epoch.
    drop(stdout);

    let mut cleanup_confirmed = false;
    let waited = tokio::select! {
        biased;
        () = cancel.notified() => {
            match reap_stream_process(child).await {
                Ok(()) => { if let Some(song) = &song { song.confirm_reaped(); } cleanup_confirmed = true; },
                Err(error) => tracing::warn!(%error, channel_number, "failed to reap cancelled channel stream after stdout closed"),
            }
            None
        },
        result = async {
            if output.has_authored_output() {
                tokio::time::timeout(Duration::from_secs(5), child.wait())
                    .await
                    .unwrap_or_else(|_| Err(std::io::ErrorKind::TimedOut.into()))
            } else {
                child.wait().await
            }
        } => Some(result),
    };
    let Some(waited) = waited else {
        let _ = await_stderr_tail(stderr_task).await;
        return if cleanup_confirmed {
            DirectStreamOutcome::Cancelled
        } else {
            DirectStreamOutcome::CleanupUnconfirmed
        };
    };
    if waited.is_ok() {
        if let Some(song) = &song {
            song.confirm_reaped();
        }
    }
    match waited {
        Ok(status) if status.success() => {
            let _ = await_stderr_tail(stderr_task).await;
            transport_error.map_or(
                DirectStreamOutcome::Completed,
                DirectStreamOutcome::TransportFailed,
            )
        }
        Ok(status) => {
            let mut stderr = await_stderr_tail(stderr_task).await;
            stderr.wrote_media = wrote_media;
            if let Some(error) = transport_error {
                // No transport existed when FFmpeg rejected a hardware graph
                // before its first byte. Preserve that proven producer failure;
                // all actual transport faults remain terminal, regardless of stderr.
                if matches!(error, TeletextPumpError::EmptyOutput)
                    && (stderr.graph_failure.is_some()
                        || stderr.decode_denial.is_some()
                        || stderr.encoder_open_failure)
                {
                    return DirectStreamOutcome::Failed(stderr);
                }
                tracing::warn!(%error, channel_number, "teletext transport ended unsuccessfully");
                return DirectStreamOutcome::TransportFailed(error);
            }
            if legacy_disconnect_is_success(
                stderr.client_disconnect,
                output.has_authored_output()
                    || remote_stream.is_some_and(|stream| {
                        matches!(stream.source, RemoteStreamSource::RetainedIntervals(_))
                    }),
            ) {
                tracing::debug!(%status, stderr = %stderr, "ffmpeg channel stream ended after client disconnect");
                DirectStreamOutcome::Completed
            } else {
                tracing::warn!(%status, stderr = %stderr, "ffmpeg channel stream exited unsuccessfully");
                DirectStreamOutcome::Failed(stderr)
            }
        }
        Err(error) => {
            let reaped = reap_stream_process(child).await.is_ok();
            if reaped {
                if let Some(song) = &song {
                    song.confirm_reaped();
                }
            }
            let mut stderr = await_stderr_tail(stderr_task).await;
            stderr.wrote_media = wrote_media;
            tracing::warn!(%error, stderr = %stderr, "failed to wait for ffmpeg channel stream");
            if !reaped {
                return DirectStreamOutcome::CleanupUnconfirmed;
            }
            transport_error.map_or_else(
                || DirectStreamOutcome::Failed(stderr),
                DirectStreamOutcome::TransportFailed,
            )
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

fn legacy_disconnect_is_success(client_disconnect: bool, retained: bool) -> bool {
    client_disconnect && !retained
}

async fn cleanup_stream_artifacts(paths: &[PathBuf]) {
    for path in paths {
        cleanup_stream_artifact(path).await;
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

async fn await_stderr_tail<T: Default + From<String>>(
    stderr_task: Option<tokio::task::JoinHandle<T>>,
) -> T {
    let Some(mut task) = stderr_task else {
        return T::default();
    };
    match tokio::time::timeout(STDERR_TAIL_TIMEOUT, &mut task).await {
        Ok(result) => result
            .unwrap_or_else(|error| format!("failed to join ffmpeg stderr reader: {error}").into()),
        Err(_elapsed) => {
            // A grandchild still holds the stderr write end open; abandon the
            // reader so the pipe read ends drop and the orphan gets EPIPE.
            task.abort();
            T::default()
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

    #[test]
    fn legacy_waiting_body_drop_fences_owner_installed_after_disposal() {
        use ersatzrs_infra::transcode_capacity::{
            active_transcode_snapshot, active_transcode_test_guard_for_tests,
            reset_active_transcode_state_for_tests,
        };
        let _lock = active_transcode_test_guard_for_tests();
        reset_active_transcode_state_for_tests();
        let registry =
            Arc::new(super::super::legacy_wrapper_admission::LegacyWrapperRegistry::new(1));
        let fence = Arc::new(WaitingLegacyWrapperFence::default());
        let guard = StreamBodyDisposeGuard {
            cancel: Arc::new(Notify::new()),
            wrapper_close: None,
            waiting_wrapper: Some(fence.clone()),
        };
        drop(guard);
        let (wrapper, encoder) = try_reserve_mpeg_ts_wrapper_capacity(2, "late").unwrap();
        let owner = registry
            .register(
                "late",
                &wrapper,
                encoder,
                tokio::time::Instant::now() + Duration::from_secs(20),
            )
            .unwrap();
        fence.attach(owner.close_handle());
        assert!(registry.lookup(owner.id(), "late").is_err());
        assert_eq!(active_transcode_snapshot(2).active, 1);
        drop(owner);
        drop(wrapper);
        assert_eq!(active_transcode_snapshot(2).active, 0);
    }

    #[cfg(unix)] // POSIX process fixture; native Windows runtime proof is separate.
    #[tokio::test]
    async fn legacy_handler_drop_cancels_owned_startup_and_retains_lease_until_cleanup() {
        use ersatzrs_infra::transcode_capacity::{
            active_transcode_snapshot, reset_active_transcode_state_for_tests,
        };
        let _lock = active_transcode_stream_test_guard().await;
        reset_active_transcode_state_for_tests();
        let registry =
            Arc::new(super::super::legacy_wrapper_admission::LegacyWrapperRegistry::new(1));
        let (wrapper, encoder) = try_reserve_mpeg_ts_wrapper_capacity(2, "startup-drop").unwrap();
        let owner = registry
            .register(
                "startup-drop",
                &wrapper,
                encoder,
                tokio::time::Instant::now() + Duration::from_secs(20),
            )
            .unwrap();
        let (item, encoder) = registry
            .lookup(owner.id(), "startup-drop")
            .unwrap()
            .begin_item()
            .unwrap();
        let cancellation = item.cancellation();
        let close = item.close_handle();
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (cleaning_tx, cleaning_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = tokio::sync::oneshot::channel();
        let (completed_tx, completed_rx) = tokio::sync::oneshot::channel();
        let handler = tokio::spawn(owned_legacy_item_startup(
            cancellation,
            close,
            move |cancel| async move {
                let mut child = spawn_stream_process(
                    &shell_command("exec sleep 30", "exit /b 1"),
                    None,
                    "startup-drop",
                )
                .unwrap();
                let _ = started_tx.send(());
                let _ = cancel.await;
                reap_stream_process(&mut child).await.unwrap();
                let _ = cleaning_tx.send(());
                let _ = release_rx.await;
                drop(encoder);
                drop(item);
                let _ = completed_tx.send(());
                StatusCode::SERVICE_UNAVAILABLE.into_response()
            },
        ));
        tokio::time::timeout(Duration::from_secs(5), started_rx)
            .await
            .unwrap()
            .unwrap();
        handler.abort();
        let _ = handler.await;
        tokio::time::timeout(Duration::from_secs(10), cleaning_rx)
            .await
            .unwrap()
            .unwrap();
        assert!(registry.lookup(owner.id(), "startup-drop").is_err());
        assert_eq!(
            active_transcode_snapshot(2).active,
            2,
            "HTTP abandonment must not release managed startup capacity before cleanup"
        );
        let _ = release_tx.send(());
        tokio::time::timeout(Duration::from_secs(5), completed_rx)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(active_transcode_snapshot(2).active, 1);
        drop(owner);
        drop(wrapper);
        assert_eq!(active_transcode_snapshot(2).active, 0);
    }

    #[tokio::test]
    async fn legacy_closed_item_never_runs_retry_builder_or_spawns_retry_child() {
        use ersatzrs_infra::transcode_capacity::reset_active_transcode_state_for_tests;
        let _lock = active_transcode_stream_test_guard().await;
        reset_active_transcode_state_for_tests();
        let registry =
            Arc::new(super::super::legacy_wrapper_admission::LegacyWrapperRegistry::new(1));
        let (wrapper, encoder) = try_reserve_mpeg_ts_wrapper_capacity(2, "retry-fence").unwrap();
        let owner = registry
            .register(
                "retry-fence",
                &wrapper,
                encoder,
                tokio::time::Instant::now() + Duration::from_secs(20),
            )
            .unwrap();
        let (item, encoder) = registry
            .lookup(owner.id(), "retry-fence")
            .unwrap()
            .begin_item()
            .unwrap();
        let cancellation = item.cancellation();
        owner.close();
        let called = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let observed = called.clone();
        let builder = StreamRetryBuilder(Box::new(move |_, _| {
            observed.store(true, Ordering::Release);
            Box::pin(async { Ok(None) })
        }));
        let result = builder
            .build(
                StreamRetryMode::HardwareEncodeDenied(FfmpegCapabilityReport::default()),
                &Arc::new(Notify::new()),
                Some(&cancellation),
            )
            .await;
        assert!(result.is_err());
        assert!(!called.load(Ordering::Acquire));
        let result = spawn_stream_retry(
            StreamRetryAttempt {
                command: FfmpegCommand {
                    mpegts_output: None,
                    executable: "must-not-spawn".into(),
                    arguments: Vec::new(),
                    environment: Vec::new(),
                },
                source_stream: None,
                prepared: None,
            },
            None,
            "retry-fence",
            Some(&cancellation),
        )
        .await;
        assert!(
            matches!(result, Err(StreamSpawnError::Spawn(message)) if message == "legacy retry cancelled")
        );
        drop(encoder);
        drop(item);
        drop(owner);
        drop(wrapper);
    }

    #[tokio::test]
    async fn legacy_item_failure_reports_body_error_and_fences_parent() {
        use ersatzrs_infra::transcode_capacity::{
            active_transcode_snapshot, reset_active_transcode_state_for_tests,
        };
        let _lock = active_transcode_stream_test_guard().await;
        reset_active_transcode_state_for_tests();
        let registry =
            Arc::new(super::super::legacy_wrapper_admission::LegacyWrapperRegistry::new(1));
        let (wrapper, encoder) = try_reserve_mpeg_ts_wrapper_capacity(2, "item-failure").unwrap();
        let owner = registry
            .register(
                "item-failure",
                &wrapper,
                encoder,
                tokio::time::Instant::now() + Duration::from_secs(20),
            )
            .unwrap();
        let (item, encoder) = registry
            .lookup(owner.id(), "item-failure")
            .unwrap()
            .begin_item()
            .unwrap();
        let body = spawn_stream_body_with_lifetime(
            &shell_command("printf item-prefix; exit 7", "echo item-prefix&exit /b 7"),
            Vec::new(),
            Vec::new(),
            "item-failure".into(),
            Arc::new(ersatzrs_infra::hls::InMemoryHlsSessionRegistry::new()),
            encoder,
            None,
            None,
            None,
            None,
            None,
            TeletextOutputAdmission::plain(TeletextOutputRole::InternalItem),
            Some(LegacyStreamLifetime::Item(item)),
        )
        .await
        .unwrap();
        let result =
            tokio::time::timeout(Duration::from_secs(10), axum::body::to_bytes(body, 1024))
                .await
                .unwrap();
        assert!(
            result.is_err(),
            "a failed item must not appear as clean concat EOF"
        );
        assert!(registry.lookup(owner.id(), "item-failure").is_err());
        assert_eq!(active_transcode_snapshot(2).active, 1);
        drop(owner);
        drop(wrapper);
        assert_eq!(active_transcode_snapshot(2).active, 0);
    }

    #[tokio::test]
    async fn legacy_item_success_releases_encoder_and_permit_before_eof() {
        use ersatzrs_infra::transcode_capacity::{
            active_transcode_snapshot, reset_active_transcode_state_for_tests,
        };
        let _lock = active_transcode_stream_test_guard().await;
        reset_active_transcode_state_for_tests();
        let registry =
            Arc::new(super::super::legacy_wrapper_admission::LegacyWrapperRegistry::new(1));
        let (wrapper, encoder) = try_reserve_mpeg_ts_wrapper_capacity(2, "item-success").unwrap();
        let owner = registry
            .register(
                "item-success",
                &wrapper,
                encoder,
                tokio::time::Instant::now() + Duration::from_secs(20),
            )
            .unwrap();
        let (item, encoder) = registry
            .lookup(owner.id(), "item-success")
            .unwrap()
            .begin_item()
            .unwrap();
        let body = spawn_stream_body_with_lifetime(
            &shell_command("printf item-prefix", "echo item-prefix"),
            Vec::new(),
            Vec::new(),
            "item-success".into(),
            Arc::new(ersatzrs_infra::hls::InMemoryHlsSessionRegistry::new()),
            encoder,
            None,
            None,
            None,
            None,
            None,
            TeletextOutputAdmission::plain(TeletextOutputRole::InternalItem),
            Some(LegacyStreamLifetime::Item(item)),
        )
        .await
        .unwrap();
        let bytes = tokio::time::timeout(Duration::from_secs(10), axum::body::to_bytes(body, 1024))
            .await
            .unwrap()
            .unwrap();
        assert!(bytes.starts_with(b"item-prefix"));
        assert_eq!(active_transcode_snapshot(2).active, 1);
        let (next, first_encoder) = registry
            .lookup(owner.id(), "item-success")
            .unwrap()
            .begin_item()
            .unwrap();
        assert!(
            first_encoder.is_none(),
            "first encoder handoff must be consumed once"
        );
        let next_encoder =
            try_reserve_active_transcode(2, ActiveTranscodeSlot::new_mpeg_ts("item-success"));
        assert!(next_encoder.is_some());
        drop(next_encoder);
        next.complete();
        drop(owner);
        drop(wrapper);
        assert_eq!(active_transcode_snapshot(2).active, 0);
    }

    #[tokio::test]
    async fn retained_startup_cancellation_awaits_stage_and_preserves_cleanup_result() {
        let (outer, mut cancellation) = tokio::sync::oneshot::channel();
        let (sender, receiver) = tokio::sync::oneshot::channel();
        drop(outer);
        let (cancelled, result) =
            await_retained_startup_stage(&mut cancellation, sender, async move {
                assert!(receiver.await.is_err());
                Ok::<_, ()>("completed artifact still needs shutdown")
            })
            .await;
        assert!(cancelled);
        assert_eq!(result, Ok("completed artifact still needs shutdown"));
    }

    #[test]
    fn retained_nonzero_broken_pipe_never_counts_as_successful_eof() {
        assert!(!legacy_disconnect_is_success(true, true));
        assert!(legacy_disconnect_is_success(true, false));
        assert!(!legacy_disconnect_is_success(false, false));
    }

    // POSIX shell closes stdout then replaces itself, leaving no grandchild.
    #[cfg(unix)]
    #[tokio::test]
    async fn stream_cancellation_reaps_child_after_stdout_has_already_closed() {
        let command = FfmpegCommand::new_with_executable(
            "sh",
            Vec::new(),
            vec!["-c".into(), "exec 1>&-; exec sleep 30".into()],
        );
        let mut child = spawn_stream_process(&command, None, "synthetic").unwrap();
        let mut stdout = child.stdout.take().unwrap();
        let mut bytes = Vec::new();
        tokio::time::timeout(Duration::from_secs(2), stdout.read_to_end(&mut bytes))
            .await
            .unwrap()
            .unwrap();
        assert!(bytes.is_empty());
        child.stdout = Some(stdout);
        let (_reader, mut writer) = tokio::io::duplex(4096);
        let cancel = Arc::new(Notify::new());
        let operation =
            supervise_stream_process(&mut child, &mut writer, &cancel, "synthetic", None);
        tokio::pin!(operation);
        assert!(matches!(
            futures_util::poll!(&mut operation),
            std::task::Poll::Pending
        ));
        cancel.notify_one();
        assert!(matches!(
            tokio::time::timeout(Duration::from_secs(2), operation)
                .await
                .unwrap(),
            DirectStreamOutcome::Cancelled
        ));
    }

    #[test]
    fn stream_failure_classifies_before_redacting_private_source() {
        let stream = ResolvedRemoteStream {
            source: RemoteStreamSource::Prepared(PathBuf::from("synthetic-private-input")),
            liveness: ersatzrs_domain::RemoteStreamLiveness::Finite,
            is_live: false,
            configured_duration: None,
            fallback_query: None,
            uses_addon: false,
        };
        let classified = classify_stream_stderr(
            "synthetic-private-input Error while opening encoder",
            Some(&stream),
            None,
            ersatzrs_domain::HardwareAccelerationKind::Nvenc,
            None,
        );
        assert!(classified.encoder_open_failure);
        assert!(!classified.to_string().contains("synthetic-private-input"));
    }

    #[test]
    fn retained_automatic_audio_uses_actual_default_without_overriding_disabled() {
        let first = SourceAudioStream::new(SourceAudioCodec::Aac).with_index(7);
        let mut default = SourceAudioStream::new(SourceAudioCodec::Aac).with_index(3);
        default.default = true;
        let streams = [first, default];
        assert_eq!(
            retained_audio_selection(PlaybackAudioSelection::Automatic, &streams),
            PlaybackAudioSelection::Exact(3)
        );
        assert_eq!(
            retained_audio_selection(PlaybackAudioSelection::Disabled, &streams),
            PlaybackAudioSelection::Disabled
        );
        assert_eq!(
            retained_audio_selection(PlaybackAudioSelection::Automatic, &[])
                .with_complete_source_audio(&[]),
            PlaybackAudioSelection::AutomaticSilence
        );
    }

    #[test]
    fn native_null_audio_direct_commands_preserve_unknown_disabled_and_selected_audio() {
        let streams = vec![SourceAudioStream::new(SourceAudioCodec::Aac).with_index(3)];
        let mut snapshots = Vec::new();
        for (label, selection, audio) in [
            (
                "confirmed absence",
                PlaybackAudioSelection::AutomaticSilence,
                Vec::new(),
            ),
            (
                "unknown metadata",
                PlaybackAudioSelection::Automatic,
                Vec::new(),
            ),
            (
                "explicit disabled",
                PlaybackAudioSelection::Disabled,
                Vec::new(),
            ),
            ("selected audio", PlaybackAudioSelection::Exact(3), streams),
        ] {
            let command = native_null_audio_command(
                InputFile::video("/media/video-only.mkv"),
                &playback_profile(),
                selection,
                audio,
                false,
            );
            let arguments = command.arguments.join(" ");
            assert_eq!(
                arguments.contains("anullsrc="),
                selection == PlaybackAudioSelection::AutomaticSilence
            );
            if selection == PlaybackAudioSelection::Disabled {
                assert!(!arguments.contains("-codec:a"), "{arguments}");
            }
            if selection == PlaybackAudioSelection::Exact(3) {
                assert!(arguments.contains("[0:3]"), "{arguments}");
            }
            snapshots.push(format!("{label}:\n{}", command.arguments.join("\n")));
        }
        insta::assert_snapshot!("native_null_audio_direct_selection", snapshots.join("\n\n"));
    }

    #[test]
    fn native_null_audio_direct_commands_preserve_copy_and_retained_seek_contracts() {
        let mut profile = playback_profile();
        profile.profile.audio_format = FFmpegProfileAudioFormat::Copy;
        let mut snapshots = Vec::new();
        for (label, input, retained) in [
            (
                "ordinary seek",
                InputFile::video("/media/video-only.mkv"),
                false,
            ),
            (
                "retained zero clock",
                InputFile::video("pipe:0")
                    .with_input_options(["-f", "nut"])
                    .with_stream_index(7),
                true,
            ),
        ] {
            let command = native_null_audio_command(
                input,
                &profile,
                PlaybackAudioSelection::Automatic.with_complete_source_audio(&[]),
                Vec::new(),
                retained,
            );
            let arguments = command.arguments.join(" ");
            assert!(arguments.contains("-codec:a aac"), "{arguments}");
            assert!(arguments.contains("-t 00:00:02.0000000"), "{arguments}");
            assert_eq!(
                arguments.contains("-ss 00:00:01.0000000"),
                !retained,
                "{arguments}"
            );
            assert!(arguments.contains("[1:0]"), "{arguments}");
            if retained {
                assert!(arguments.contains("[0:7]"), "{arguments}");
            }
            snapshots.push(format!("{label}:\n{}", command.arguments.join("\n")));
        }
        insta::assert_snapshot!(
            "native_null_audio_direct_copy_retained",
            snapshots.join("\n\n")
        );
    }

    #[test]
    fn native_null_audio_hls_direct_preserves_video_copy_and_restores_audio_profile() {
        let mut profile = playback_profile();
        profile.profile.audio_format = FFmpegProfileAudioFormat::Copy;
        let command = native_null_audio_hls_direct_command("/media/video-only.mkv", &profile);
        let arguments = command.arguments.join(" ");
        assert!(arguments.contains("-codec:v copy"), "{arguments}");
        assert!(arguments.contains("-codec:a aac"), "{arguments}");
        assert!(arguments.contains("-ar 48k"), "{arguments}");
        assert!(arguments.contains("-t 00:00:02.0000000"), "{arguments}");
        assert!(!arguments.contains("-codec:a copy"), "{arguments}");
        insta::assert_snapshot!(
            "native_null_audio_hls_direct_copy",
            command.arguments.join("\n")
        );
    }

    #[test]
    fn native_null_audio_hls_direct_fresh_retry_retains_the_programme_duration() {
        let audio = [SourceAudioStream::new(SourceAudioCodec::Aac).with_index(3)];
        let initial = retained_audio_selection(PlaybackAudioSelection::Automatic, &audio)
            .with_complete_source_audio(&audio);
        assert_eq!(initial, PlaybackAudioSelection::Exact(3));
        let remaining = Some(Duration::from_secs(5));
        let duration = native_generated_audio_duration(initial, None, remaining);
        assert_eq!(
            duration, None,
            "ordinary copy retains its existing EOF contract"
        );
        let fresh = retained_audio_selection(PlaybackAudioSelection::Automatic, &[])
            .with_complete_source_audio(&[]);
        let retry_duration = native_generated_audio_duration(fresh, duration, remaining);
        assert_eq!(retry_duration, remaining);
        assert_eq!(
            native_generated_audio_duration(PlaybackAudioSelection::Disabled, None, remaining),
            None
        );
        assert_eq!(
            native_generated_audio_duration(fresh, Some(Duration::from_secs(2)), remaining),
            Some(Duration::from_secs(2))
        );
        let mut source = source_320p().with_index(7);
        source.codec = SourceVideoCodec::RawVideo;
        let command = build_stream_command_with_retained_inputs(
            InputFile::video("pipe:0")
                .with_input_options(["-f", "nut"])
                .with_stream_index(7),
            OutputFormat::MpegTs,
            Duration::from_secs(3),
            retry_duration,
            &playback_profile(),
            None,
            StreamingMode::HttpLiveStreamingDirect,
            OutputMetadata::default(),
            FfmpegCapabilityReport::default(),
            Some(source),
            Vec::new(),
            fresh,
            None,
            Vec::new(),
            Vec::new(),
            None,
            true,
            None,
        )
        .expect("fresh retained video-only retry");
        let arguments = command.arguments.join(" ");
        assert!(arguments.contains("anullsrc="), "{arguments}");
        assert!(arguments.contains("-t 00:00:05.0000000"), "{arguments}");
        assert!(!arguments.contains("-ss"), "{arguments}");
        assert!(arguments.contains("[0:7]"), "{arguments}");
        assert!(arguments.contains("[1:0]"), "{arguments}");
    }

    fn native_null_audio_hls_direct_command(
        input: &str,
        profile: &PlaybackProfile,
    ) -> FfmpegCommand {
        build_stream_command_with_opacity_clock(
            input,
            OutputFormat::MpegTs,
            Duration::from_secs(1),
            Some(Duration::from_secs(2)),
            profile,
            None,
            StreamingMode::HttpLiveStreamingDirect,
            OutputMetadata::default(),
            FfmpegCapabilityReport::default(),
            Some(source_320p()),
            Vec::new(),
            PlaybackAudioSelection::Automatic.with_complete_source_audio(&[]),
            None,
            Vec::new(),
            Vec::new(),
            None,
        )
        .expect("confirmed video-only HLS Direct source")
    }

    fn native_null_audio_command(
        input: InputFile,
        profile: &PlaybackProfile,
        selection: PlaybackAudioSelection,
        audio: Vec<SourceAudioStream>,
        retained: bool,
    ) -> FfmpegCommand {
        let mut stream = source_320p().with_index(if retained { 7 } else { 0 });
        if retained {
            stream.codec = SourceVideoCodec::RawVideo;
        }
        build_stream_command_with_retained_inputs(
            input,
            OutputFormat::MpegTs,
            Duration::from_secs(1),
            Some(Duration::from_secs(2)),
            profile,
            None,
            StreamingMode::TransportStream,
            OutputMetadata::default(),
            FfmpegCapabilityReport::default(),
            Some(stream),
            audio,
            selection,
            None,
            Vec::new(),
            Vec::new(),
            None,
            retained,
            None,
        )
        .expect("native known source command")
    }

    #[test]
    fn retained_mpegts_command_keeps_actual_indexes_and_edited_shared_clock() {
        let video = InputFile::video("pipe:0")
            .with_input_options(["-f", "nut"])
            .with_stream_index(7);
        let mut source = source_320p().with_index(7);
        source.codec = SourceVideoCodec::RawVideo;
        let command = build_stream_command_with_retained_inputs(
            video,
            OutputFormat::MpegTs,
            Duration::from_secs(6),
            Some(Duration::from_secs(20)),
            &playback_profile(),
            None,
            StreamingMode::TransportStream,
            OutputMetadata::default(),
            FfmpegCapabilityReport::default(),
            Some(source),
            vec![SourceAudioStream::new(SourceAudioCodec::Unknown).with_index(3)],
            PlaybackAudioSelection::Exact(3),
            None,
            Vec::new(),
            Vec::new(),
            None,
            true,
            None,
        )
        .unwrap();
        let argv = command.arguments.join(" ");
        assert!(!argv.contains("concatdec_select"));
        assert!(argv.contains("0:7"));
        assert!(argv.contains("0:3"));
        assert!(command.arguments.iter().any(|arg| arg == "-copyts"));
        assert!(!command.arguments.iter().any(|arg| arg == "-ss"));
        let input_contract = command
            .arguments
            .windows(2)
            .filter(|pair| matches!(pair[0].as_str(), "-f" | "-i"))
            .map(|pair| pair.join(" "))
            .collect::<Vec<_>>()
            .join("\n");
        insta::assert_snapshot!(input_contract, @"
        -f nut
        -i pipe:0
        -f mpegts
        ");
    }
    use ersatzrs_domain::{
        FFmpegProfile, FFmpegProfileAudioFormat, FFmpegProfileVideoFormat, Resolution,
        StreamingMode, WatermarkLocation, WatermarkSize,
    };
    use ersatzrs_ffmpeg::{FfmpegCapabilityReport, GraphicsPipelineOverlay, WatermarkOverlay};
    #[cfg(unix)]
    use ersatzrs_ffmpeg::{
        FfmpegRuntimeReport, GraphFailureKind, HardwareGraphDenial, HardwareGraphOperation,
    };
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
        sqlx::query(
            r#"UPDATE "PlayoutItem"
               SET "SubtitleMode" = 0,
                   "MediaVersionId" = 21,
                   "VideoStreamIndex" = 0,
                   "AudioStreamIndex" = NULL,
                   "SubtitleStreamIndex" = NULL
               WHERE "Id" = 12"#,
        )
        .execute(&pool)
        .await
        .expect("pin silent subtitle-off playback item");
        let active = load_active_media_path(&pool, 7, now)
            .await
            .expect("load pinned active playback item")
            .expect("active playback item");
        assert!(active.has_pinned_stream_selection);
        assert_eq!(active.audio_stream_index, None);
        assert_eq!(active.subtitle_stream_index, None);

        sqlx::query(
            r#"INSERT INTO "RemoteStream" (
                   "Id", "Url", "Script", "Duration", "FallbackQuery", "Liveness", "IsLive"
               ) VALUES (
                   30, 'https://user:secret@example.test/live.m3u8?token=hidden',
                   NULL, NULL, 'tag:backup', 2, 1
               )"#,
        )
        .execute(&pool)
        .await
        .expect("remote stream");
        sqlx::query(
            r#"INSERT INTO "MediaVersion" ("Id", "Duration", "RemoteStreamId")
               VALUES (31, '00:00:00', 30)"#,
        )
        .execute(&pool)
        .await
        .expect("remote media version");
        sqlx::query(
            r#"INSERT INTO "MediaFile" ("MediaVersionId", "Path")
               VALUES (31, '/media/streams/live.yml')"#,
        )
        .execute(&pool)
        .await
        .expect("remote definition file");
        sqlx::query(
            r#"UPDATE "PlayoutItem"
               SET "MediaItemId" = 30, "MediaVersionId" = 31
               WHERE "Id" = 12"#,
        )
        .execute(&pool)
        .await
        .expect("select remote stream");

        let active = load_active_media_path(&pool, 7, now)
            .await
            .expect("load active remote stream")
            .expect("active remote stream");
        let remote = active.remote_stream.expect("resolved remote stream");
        assert!(remote.is_live);
        assert!(matches!(
            remote.source,
            ersatzrs_infra::remote_stream::RemoteStreamSource::Url(ref url)
                if url.contains("live.m3u8")
        ));
        assert_eq!(active.media_path, "/media/streams/live.yml");
    }

    #[tokio::test]
    async fn active_playback_item_loader_resolves_and_projects_delayed_mirror() {
        let pool = active_playback_item_test_pool().await;
        sqlx::query(
            r#"INSERT INTO "Channel" (
                   "Id", "PlayoutSource", "MirrorSourceChannelId", "PlayoutOffset"
               ) VALUES (8, 1, 7, '01:00:00')"#,
        )
        .execute(&pool)
        .await
        .expect("seed delayed mirror");
        sqlx::query(r#"UPDATE "PlayoutItem" SET "SubtitleMode" = 0 WHERE "Id" = 12"#)
            .execute(&pool)
            .await
            .expect("make playback item valid");
        let mirror_now = DateTime::parse_from_rfc3339("2026-06-08T13:00:00Z")
            .expect("timestamp")
            .with_timezone(&Utc);

        let active = load_active_media_path(&pool, 8, mirror_now)
            .await
            .expect("load mirror item")
            .expect("active mirror item");

        assert_eq!(active.playout_item_id, 12);
        assert_eq!(active.playout_start, "2026-06-08 12:59:00");
        assert_eq!(active.playout_finish, "2026-06-08 13:01:00");
        assert_eq!(
            active.content_seek_position(mirror_now),
            Duration::from_mins(1)
        );
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

    #[allow(clippy::too_many_lines)] // Keep the minimal playback schema and its seed together.
    async fn active_playback_item_test_pool() -> DbPool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("open active playback item test database");
        for statement in [
            r#"CREATE TABLE "Channel" (
                "Id" INTEGER PRIMARY KEY,
                "PlayoutSource" INTEGER NOT NULL,
                "MirrorSourceChannelId" INTEGER NULL,
                "PlayoutOffset" TEXT NULL
            )"#,
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
                "SubtitleMode" INTEGER NULL,
                "MediaVersionId" INTEGER NULL,
                "VideoStreamIndex" INTEGER NULL,
                "AudioStreamIndex" INTEGER NULL,
                "SubtitleStreamIndex" INTEGER NULL
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
            r#"CREATE TABLE "RemoteStream" (
                "Id" INTEGER PRIMARY KEY,
                "ProviderId" TEXT NULL,
                "Url" TEXT NULL,
                "AddonId" TEXT NULL,
                "Script" TEXT NULL,
                "Duration" TEXT NULL,
                "FallbackQuery" TEXT NULL,
                "Liveness" INTEGER NOT NULL,
                "IsLive" INTEGER NOT NULL
            )"#,
            r#"CREATE TABLE "MediaItem" (
                "Id" INTEGER PRIMARY KEY,
                "LibraryPathId" INTEGER NULL
            )"#,
            r#"CREATE TABLE "LibraryPath" (
                "Id" INTEGER PRIMARY KEY,
                "LibraryId" INTEGER NOT NULL
            )"#,
            r#"CREATE TABLE "AddonMediaListLibrary" (
                "LibraryId" INTEGER PRIMARY KEY,
                "AddonMediaListId" INTEGER NOT NULL
            )"#,
            r#"CREATE TABLE "AddonMediaList" (
                "Id" INTEGER PRIMARY KEY,
                "PublishedRevision" INTEGER NOT NULL,
                "AddonId" TEXT NOT NULL
            )"#,
        ] {
            sqlx::query(statement)
                .execute(&pool)
                .await
                .expect("create active playback item test table");
        }
        for statement in [
            r#"INSERT INTO "Channel" (
                "Id", "PlayoutSource", "MirrorSourceChannelId", "PlayoutOffset"
            ) VALUES (7, 0, NULL, NULL)"#,
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
            PlaybackAudioSelection::Automatic,
            None,
            Vec::new(),
            Vec::new(),
        )
        .expect("the fixture source is playable");
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
        )
        .expect("the fixture source is playable");
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
        let first = build(report.clone())
            .expect("the fixture source is playable")
            .arguments
            .join(" ");
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
        let second = build(denied)
            .expect("the fixture source is playable")
            .arguments
            .join(" ");
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
        profile.profile.video_preset = Some("p7".to_owned());
        profile.profile.video_tune = Some("ll".to_owned());
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
        .expect("the fixture source is playable")
        .arguments
        .join(" ");
        assert!(first.contains("-codec:v h264_nvenc"), "{first}");
        assert!(first.contains("-preset:v p7 -tune:v ll"), "{first}");

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
        .expect("the fixture source is playable")
        .arguments
        .join(" ");
        assert!(second.contains("-codec:v libx264"), "{second}");
        assert!(second.contains("-preset:v veryfast"), "{second}");
        assert!(!second.contains("h264_nvenc"), "{second}");
        assert!(!second.contains("-tune:v"), "{second}");
        assert!(!second.contains("-hwaccel cuda"), "{second}");
    }

    #[test]
    fn hardware_encode_retry_does_not_carry_a_qsv_preset_into_libx264() {
        // ADR 0017 v5 seeds QSV at `medium`, which is target usage 4 on a
        // fixed-function encoder. libx264's `medium` is a far more expensive
        // CPU preset that shares nothing but the name, so an ADR 0014 demotion
        // must fall back to `veryfast` rather than reuse it.
        let mut profile = playback_profile();
        profile.profile.hardware_acceleration = ersatzrs_domain::HardwareAccelerationKind::Qsv;
        profile.profile.video_preset = Some("medium".to_owned());

        force_software_h264_encoding(&mut profile);

        assert_eq!(profile.profile.video_preset.as_deref(), Some("veryfast"));
        assert_eq!(
            profile.profile.hardware_acceleration,
            ersatzrs_domain::HardwareAccelerationKind::None
        );
    }

    #[test]
    fn hls_direct_mpegts_transcode_does_not_substitute_an_x264_preset_for_nvenc() {
        // FFmpeg 9's h264_nvenc preset domain is p1-p7 plus legacy
        // slow/medium/fast — it has no `veryfast`, so forcing an H.264
        // transcode for HLS-direct MPEG-TS must not inject one. A profile
        // forced here from video format Copy carries no preset at all and
        // inherits the encoder default.
        let mut desired = DesiredVideoState {
            video_preset: None,
            ..DesiredVideoState::h264_1080p()
        };

        normalize_hls_direct_mpegts_h264_preset(
            &mut desired,
            ersatzrs_domain::HardwareAccelerationKind::Nvenc,
        );

        assert_eq!(desired.video_preset, None);
    }

    #[test]
    fn hls_direct_mpegts_transcode_still_defaults_software_encodes_to_veryfast() {
        let mut desired = DesiredVideoState {
            video_preset: None,
            ..DesiredVideoState::h264_1080p()
        };

        normalize_hls_direct_mpegts_h264_preset(
            &mut desired,
            ersatzrs_domain::HardwareAccelerationKind::None,
        );

        assert_eq!(desired.video_preset.as_deref(), Some("veryfast"));
    }

    #[test]
    fn software_profiles_keep_their_operator_chosen_x264_preset() {
        // A profile that was already software carries a real libx264 preset,
        // so a forced-software rebuild must not overwrite the operator's
        // choice with `veryfast`.
        let mut profile = playback_profile();
        profile.profile.hardware_acceleration = ersatzrs_domain::HardwareAccelerationKind::None;
        profile.profile.video_preset = Some("slow".to_owned());

        force_software_h264_encoding(&mut profile);

        assert_eq!(profile.profile.video_preset.as_deref(), Some("slow"));
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
            PlaybackAudioSelection::Automatic,
            None,
            Vec::new(),
            Vec::new(),
        );
        let arguments = command
            .expect("the fixture source is playable")
            .arguments
            .join(" ");
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
        let arguments = command
            .expect("the fixture source is playable")
            .arguments
            .join(" ");

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
        let arguments = command
            .expect("the fixture source is playable")
            .arguments
            .join(" ");

        assert!(arguments.contains("-codec:v copy"), "{arguments}");
        assert!(arguments.contains("-codec:a copy"), "{arguments}");
        assert!(arguments.contains("-f mpegts"), "{arguments}");
    }

    #[test]
    fn pinned_silent_stream_command_has_no_audio_mapping_or_encoder() {
        let command = build_stream_command(
            "/media/movie.mkv",
            OutputFormat::MpegTs,
            Duration::ZERO,
            &playback_profile(),
            None,
            StreamingMode::TransportStream,
            OutputMetadata::default(),
            FfmpegCapabilityReport::default(),
            None,
            vec![SourceAudioStream::new(SourceAudioCodec::Aac).with_index(1)],
            PlaybackAudioSelection::Disabled,
            None,
            Vec::new(),
            Vec::new(),
        );
        let arguments = command
            .expect("the fixture source is playable")
            .arguments
            .join(" ");

        assert!(!arguments.contains("-map 0:1"), "{arguments}");
        assert!(!arguments.contains("-map 0:a"), "{arguments}");
        assert!(!arguments.contains("-codec:a"), "{arguments}");
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
        let arguments = command
            .expect("the fixture source is playable")
            .arguments
            .join(" ");
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
        let arguments = command
            .expect("the fixture source is playable")
            .arguments
            .join(" ");

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
        let arguments = command
            .expect("the fixture source is playable")
            .arguments
            .join(" ");

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
        let arguments = command
            .expect("the fixture source is playable")
            .arguments
            .join(" ");

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
            external: None,
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
            has_pinned_stream_selection: false,
            video_stream_index: None,
            audio_stream_index: None,
            subtitle_stream_index: None,
            remote_stream: None,
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
            PlaybackAudioSelection::Automatic,
            None,
            Vec::new(),
            Vec::new(),
            None,
        )
        .expect("the fixture source is playable");

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
            TransportStreamMode::HybridWrapper
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
        assert_eq!(
            resolve_transport_stream_mode(Some("hybrid"), StreamingMode::TransportStream),
            TransportStreamMode::HybridWrapper
        );
    }

    #[test]
    fn next_transport_dispatch_preserves_engine_and_allows_hybrid_wrapper() {
        use ersatzrs_domain::StreamingEngine;
        let mut channel = Channel {
            streaming_engine: StreamingEngine::Next,
            ..Default::default()
        };
        for setting in [
            StreamingMode::TransportStream,
            StreamingMode::TransportStreamHybrid,
            StreamingMode::HttpLiveStreamingDirect,
            StreamingMode::HttpLiveStreamingSegmenter,
        ] {
            channel.streaming_mode = setting;
            for requested in [None, Some(""), Some("mixed"), Some(" mixed ")] {
                let resolved = resolve_transport_stream_mode(requested, setting);
                let expected = if resolved == TransportStreamMode::Legacy {
                    TransportStreamMode::HybridWrapper
                } else {
                    resolved
                };
                assert_eq!(channel_transport_stream_mode(requested, &channel), expected);
            }
            for requested in [
                "ts-legacy",
                " ts-legacy ",
                "mpegts",
                "mpeg-ts",
                "ts",
                "hybrid",
                "unknown",
            ] {
                assert_eq!(
                    channel_transport_stream_mode(Some(requested), &channel),
                    TransportStreamMode::HybridWrapper,
                    "Next setting {setting:?}, cached request {requested}"
                );
            }
            channel.streaming_engine = StreamingEngine::Legacy;
            for requested in [
                None,
                Some(""),
                Some("mixed"),
                Some("ts-legacy"),
                Some("mpegts"),
                Some("mpeg-ts"),
                Some("ts"),
            ] {
                assert_eq!(
                    channel_transport_stream_mode(requested, &channel),
                    resolve_transport_stream_mode(requested, setting),
                    "native setting {setting:?}, request {requested:?}"
                );
            }
            channel.streaming_engine = StreamingEngine::Next;
        }
    }

    #[test]
    fn segmenter_wrapper_url_preserves_access_token() {
        let token = AccessToken::new(Some("abc123".to_owned()));
        assert_eq!(
            build_segmenter_wrapper_hls_url(
                "127.0.0.1:8409".parse().unwrap(),
                "7",
                ersatzrs_domain::StreamingEngine::Legacy,
                &token
            ),
            "http://127.0.0.1:8409/iptv/channel/7.m3u8?mode=segmenter&access_token=abc123"
        );
    }

    #[test]
    fn next_segmenter_wrapper_uses_authenticated_media_playlist_without_webvtt() {
        let token = AccessToken::new(Some("synthetic-token".to_owned()));
        let url = build_segmenter_wrapper_hls_url(
            "127.0.0.1:8409".parse().unwrap(),
            "7",
            ersatzrs_domain::StreamingEngine::Next,
            &token,
        );
        assert_eq!(
            url,
            "http://127.0.0.1:8409/iptv/session/7/hls.m3u8?access_token=synthetic-token"
        );
        assert_eq!(
            build_segmenter_wrapper_hls_url(
                "127.0.0.1:8409".parse().unwrap(),
                "7",
                ersatzrs_domain::StreamingEngine::Next,
                &AccessToken::default(),
            ),
            "http://127.0.0.1:8409/iptv/session/7/hls.m3u8"
        );
        // Preserve every in-band stream; only the HLS-only WebVTT rendition is absent.
        let command =
            build_mpegts_segmenter_wrapper_command(url, &OutputMetadata::default(), false, None);
        insta::assert_snapshot!(
            "next_hybrid_media_playlist_command",
            command.arguments.join("\n")
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

    #[tokio::test]
    async fn mpegts_script_missing_platform_name_or_file_returns_fallback_error() {
        let root = tempfile::tempdir().unwrap();
        let data = MpegTsScriptRenderData {
            hls_url: "http://127.0.0.1/test.m3u8".into(),
            channel_name: "Synthetic channel".into(),
            ffmpeg_path: "ffmpeg".into(),
            muxrate_bps: 12_000_000,
            pcr_period_ms: MPEGTS_PCR_PERIOD_MS,
        };
        for name in [None, Some(" ".into()), Some("missing-script".into())] {
            let script = MpegTsScript {
                id: "custom".into(),
                folder: root.path().to_owned(),
                definition: MpegTsScriptDefinition {
                    name: "Custom".into(),
                    linux_script: name.clone(),
                    windows_script: name,
                },
            };
            assert_eq!(
                render_mpegts_script_command(&script, &data)
                    .await
                    .unwrap_err()
                    .kind(),
                std::io::ErrorKind::NotFound,
            );
        }
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
        assert!(!should_warn_missing_mpegts_script("Default"));
        assert!(!should_warn_missing_mpegts_script("DEFAULT"));
        assert!(should_warn_missing_mpegts_script("custom"));
    }

    #[test]
    fn mpegts_script_ids_compare_without_culture_or_full_folding() {
        assert!(mpegts_script_id_eq("Ähnliche", "ähnliche"));
        assert!(!mpegts_script_id_eq("resume", "résumé"));
        assert!(!mpegts_script_id_eq("Straße", "STRASSE"));
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

        let script = find_mpegts_script_in(root.path(), "CUSTOM")
            .await
            .expect("script discovered");

        assert_eq!(script.id, "custom");
        assert_eq!(script.definition.name, "Custom");
        assert_eq!(script.definition.linux_script.as_deref(), Some("run.sh"));
    }

    #[tokio::test]
    async fn mpegts_script_discovery_matches_unicode_case_without_accent_folding() {
        let root = tempfile::tempdir().expect("temp script root");
        let folder = root.path().join("Ähnliche");
        std::fs::create_dir_all(&folder).expect("create script folder");
        std::fs::write(
            folder.join("mpegts.yml"),
            "name: Similar\nlinux_script: run.sh\n",
        )
        .expect("write script yaml");

        let script = find_mpegts_script_in(root.path(), "ähnliche")
            .await
            .expect("Unicode case variant discovered");
        assert_eq!(script.id, "Ähnliche");
        assert!(find_mpegts_script_in(root.path(), "ahnliche")
            .await
            .is_none());
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

    #[tokio::test]
    async fn mpegts_script_command_preserves_unicode_in_environment() {
        let root = tempfile::tempdir().unwrap();
        let script = MpegTsScript {
            id: "unicode".into(),
            folder: root.path().to_owned(),
            definition: MpegTsScriptDefinition {
                name: "Unicode".into(),
                linux_script: Some("run.sh".into()),
                windows_script: Some("run.bat".into()),
            },
        };
        let data = MpegTsScriptRenderData {
            hls_url:
                "http://localhost:8409/iptv/channel/7.m3u8?mode=segmenter&access_token=synthetic"
                    .into(),
            channel_name: "Télévision \"日本語\"".into(),
            ffmpeg_path: "outils été/映像/ffmpeg".into(),
            muxrate_bps: 9_830_400,
            pcr_period_ms: 20,
        };
        for (name, template) in [
            (
                "run.sh",
                include_str!("../../../../../assets/Scripts/MpegTs/run.sh"),
            ),
            (
                "run.bat",
                include_str!("../../../../../assets/Scripts/MpegTs/run.bat"),
            ),
        ] {
            tokio::fs::write(root.path().join(name), template)
                .await
                .unwrap();
            let rendered = render_mpegts_script_template(template, &data);
            assert!(rendered.trim_start_matches('\u{feff}').is_ascii());
            assert!(rendered.contains("ETV_CHANNEL_NAME"));
            assert!(rendered.contains("ETV_HLS_URL"));
            assert!(rendered.contains("ETV_FFMPEG_PATH"));
            assert!(rendered.contains("-muxrate 9830400 -pcr_period 20"));
            insta::assert_snapshot!(
                format!("mpegts_builtin_{}", name.replace('.', "_")),
                rendered
            );
        }
        let (command, cleanup) = render_mpegts_script_command(&script, &data).await.unwrap();
        for (name, value) in [
            ("ETV_CHANNEL_NAME", "Télévision 日本語"),
            ("ETV_HLS_URL", data.hls_url.as_str()),
            ("ETV_FFMPEG_PATH", data.ffmpeg_path.as_str()),
        ] {
            assert!(command
                .environment
                .iter()
                .any(|variable| variable.name == name && variable.value == value));
        }
        cleanup_stream_artifacts(&cleanup).await;
    }

    #[tokio::test]
    async fn early_stream_refusal_cleanup_removes_rendered_graphics_directories() {
        let root = tempfile::tempdir().expect("temp root");
        let graphics_dir = root.path().join("rendered-graphics");
        std::fs::create_dir_all(&graphics_dir).expect("create graphics directory");
        std::fs::write(graphics_dir.join("overlay.vgs"), "rect 0 0 1 1 fill\n")
            .expect("write rendered graphics");

        cleanup_stream_artifacts(std::slice::from_ref(&graphics_dir)).await;

        assert!(!graphics_dir.exists());
    }

    #[test]
    fn legacy_concat_url_matches_internal_endpoint_shape() {
        assert_eq!(
            build_legacy_concat_url("127.0.0.1:8409".parse().unwrap(), "7"),
            "http://127.0.0.1:8409/ffmpeg/concat/7?mode=ts-legacy"
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
            mpegts_output: None,
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
            Vec::new(),
            "98".to_owned(),
            Arc::clone(&registry),
            None,
            None,
            None,
            TeletextOutputAdmission::plain(TeletextOutputRole::FinalDirectMpegTs),
        )
        .await
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
        assert_eq!(
            ersatzrs_infra::transcode_capacity::active_transcode_snapshot(1).active,
            1
        );

        let body = spawn_stream_body(
            &shell_command("printf done", "echo done"),
            Vec::new(),
            Vec::new(),
            "91".to_owned(),
            registry,
            Some(reservation),
            None,
            None,
            TeletextOutputAdmission::plain(TeletextOutputRole::FinalDirectMpegTs),
        )
        .await
        .expect("spawn stream body");

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
            Vec::new(),
            "92".to_owned(),
            registry,
            Some(reservation),
            None,
            None,
            TeletextOutputAdmission::plain(TeletextOutputRole::FinalDirectMpegTs),
        )
        .await
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
            Vec::new(),
            "93".to_owned(),
            registry,
            pool,
            None,
            None,
            None,
            TeletextOutputAdmission::plain(TeletextOutputRole::FinalDirectMpegTs),
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
            Vec::new(),
            "94".to_owned(),
            registry,
            pool,
            None,
            None,
            None,
            TeletextOutputAdmission::plain(TeletextOutputRole::FinalDirectMpegTs),
        );

        let bytes = axum::body::to_bytes(body, 64 * 1024)
            .await
            .expect("read MPEG-TS wait body");
        release.await.expect("release occupied HLS capacity");
        let output = String::from_utf8_lossy(&bytes);

        assert_eq!(output.trim_end_matches(['\r', '\n']), "content");
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
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("source.srt");
        std::fs::write(&source, "1\n00:00:00,000 --> 00:00:30,000\nFixture\n").unwrap();
        let subtitle =
            ersatzrs_infra::playback::transient::TransientPlaybackTracks::from_streams(&[
                ersatzrs_domain::MediaStream {
                    kind: ersatzrs_domain::MediaStreamKind::Subtitle,
                    codec: Some("subrip".into()),
                    ..Default::default()
                },
            ])
            .subtitles
            .remove(0);
        let owner = ersatzrs_infra::external_json::subtitles::PreparedSubtitle::extract(
            ersatzrs_infra::next_engine::protocol::Source::Local {
                path: source.to_str().unwrap().into(),
                probe_hint: None,
                in_point_ms: None,
                out_point_ms: None,
            },
            subtitle,
            ersatzrs_ffmpeg::ffmpeg_path().into(),
            tokio::time::Instant::now() + Duration::from_secs(10),
        )
        .await
        .unwrap();
        let artifact_path = PathBuf::from(owner.subtitle().path.as_ref().unwrap());
        let command = FfmpegCommand {
            mpegts_output: None,
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

        let body = spawn_stream_body_with_prepared_input(
            &command,
            Vec::new(),
            Vec::new(),
            "97".to_owned(),
            Arc::clone(&registry),
            None,
            None,
            None,
            None,
            Some(owner),
            None,
            TeletextOutputAdmission::plain(TeletextOutputRole::FinalDirectMpegTs),
        )
        .await
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

        assert!(
            registry.try_get("97").await.is_none(),
            "a direct stream must not occupy the HLS worker slot"
        );
        assert!(
            artifact_path.exists(),
            "stream supervisor must retain subtitle ownership"
        );
        drop(body);

        // The supervisor kills ffmpeg on disposal and removes the session.
        let removed_deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        while registry.is_active("97").await || artifact_path.exists() {
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
                mpegts_output: None,
                executable: "cmd.exe".to_owned(),
                arguments: vec!["/C".to_owned(), windows_script.to_owned()],
                environment: Vec::new(),
            }
        } else {
            FfmpegCommand {
                mpegts_output: None,
                executable: "/bin/sh".to_owned(),
                arguments: vec!["-c".to_owned(), unix_script.to_owned()],
                environment: Vec::new(),
            }
        }
    }

    async fn capacity_test_pool(limit: i32) -> DbPool {
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
        )
        .expect("the fixture source is playable");
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

        let mut command = build_stream_command(
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
        )
        .expect("the fixture source is playable");
        // A pipe read can contain only transport headers. Encode one complete
        // video frame and drain that bounded output before checking its pixels.
        let output_index = command.arguments.len() - 1;
        assert_eq!(command.arguments[output_index], "pipe:1");
        command.arguments.splice(
            output_index..output_index,
            ["-frames:v".to_owned(), "1".to_owned()],
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
        let stdout = child.stdout.take().expect("stdout pipe");
        let mut buffer = Vec::new();
        let (read, status) = timeout(Duration::from_secs(20), async {
            let read = stdout
                .take(2 * 1024 * 1024 + 1)
                .read_to_end(&mut buffer)
                .await?;
            let status = child.wait().await?;
            Ok::<_, std::io::Error>((read, status))
        })
        .await
        .expect("timed out waiting for a complete watermarked frame")
        .expect("capture ffmpeg output");
        assert!(
            read > 0 && read <= 2 * 1024 * 1024,
            "invalid capture size: {read}"
        );
        assert!(status.success(), "watermark encoder failed: {status}");

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
            PlaybackAudioSelection::Automatic,
            None,
            vec![(
                InputFile::watermark_looped(logo.to_string_lossy().into_owned()),
                opacity_clock_watermark(expression),
            )],
            Vec::new(),
            Some(clock),
        )
        .expect("the fixture source is playable");
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
    #[ignore = "requires the exact published FFmpeg runtime; genuine video-only direct audio/seek/Stop proof"]
    async fn native_null_audio_direct_body_decodes_silence_and_reaps_on_stop() {
        use futures_util::FutureExt as _;
        let _guard = active_transcode_stream_test_guard().await;
        let ffmpeg = ersatzrs_ffmpeg::ffmpeg_path();
        let ffprobe = ersatzrs_ffmpeg::ffprobe_path();
        let root = tempfile::tempdir().expect("private null-audio fixture");
        let proof = std::panic::AssertUnwindSafe(native_null_audio_runtime_proof(
            &ffmpeg,
            &ffprobe,
            root.path(),
        ))
        .catch_unwind()
        .await;
        if let Err(panic) = proof {
            eprintln!(
                "retained synthetic native null-audio fixture: {}",
                root.keep().display()
            );
            std::panic::resume_unwind(panic);
        }
        let path = root.path().to_path_buf();
        root.close()
            .expect("remove owned fixture after confirmed producers ended");
        assert!(!path.exists());
    }

    async fn native_null_audio_runtime_proof(ffmpeg: &str, ffprobe: &str, root: &Path) {
        let input = root.join("video-only.mkv");
        let generated = bounded_null_audio_tool(
            Command::new(ffmpeg).args([
                "-v", "error", "-nostdin", "-f", "lavfi", "-i",
                "color=blue:s=320x180:r=25:d=1[a];color=red:s=320x180:r=25:d=19[b];[a][b]concat=n=2:v=1:a=0",
                "-an", "-c:v", "libx264", "-threads", "1", "-pix_fmt", "yuv420p",
                "-g", "25", "-keyint_min", "25", "-sc_threshold", "0", "-bf", "0",
            ]).arg(&input),
        ).await;
        assert!(generated.stdout.is_empty());
        let probe = bounded_null_audio_tool(
            Command::new(ffprobe)
                .args(["-v", "error", "-show_streams", "-of", "json"])
                .arg(&input),
        )
        .await;
        tokio::fs::write(root.join("input-probe.json"), &probe.stdout)
            .await
            .unwrap();
        let probe: serde_json::Value = serde_json::from_slice(&probe.stdout).unwrap();
        let streams = probe["streams"].as_array().unwrap();
        assert_eq!(streams.len(), 1, "genuine source must have no audio");
        assert_eq!(streams[0]["codec_type"], "video");
        let mut profile = playback_profile();
        profile.resolution.width = 320;
        profile.resolution.height = 180;
        profile.profile.thread_count = 1;
        let command = native_null_audio_command(
            InputFile::video(input.to_string_lossy().into_owned()),
            &profile,
            PlaybackAudioSelection::Automatic.with_complete_source_audio(&[]),
            Vec::new(),
            false,
        );
        let registry: Arc<dyn HlsSessionRegistry> =
            Arc::new(ersatzrs_infra::hls::InMemoryHlsSessionRegistry::new());
        let bytes = capture_native_null_audio_body(&command, &registry, root, "214", false).await;
        let output = root.join("generated-silence.ts");
        tokio::fs::write(&output, bytes).await.unwrap();
        prove_native_null_audio_output(ffmpeg, ffprobe, &output).await;

        profile.profile.audio_format = FFmpegProfileAudioFormat::Copy;
        let direct = native_null_audio_hls_direct_command(input.to_str().unwrap(), &profile);
        let bytes = capture_native_null_audio_body(&direct, &registry, root, "216", false).await;
        let direct_output = root.join("hls-direct-silence.ts");
        tokio::fs::write(&direct_output, bytes).await.unwrap();
        prove_native_null_audio_output(ffmpeg, ffprobe, &direct_output).await;

        // A second real producer exercises explicit channel Stop before EOF.
        let mut running = command;
        let duration = running
            .arguments
            .iter()
            .position(|argument| argument == "-t")
            .unwrap();
        running.arguments[duration + 1] = "00:00:15.0000000".to_owned();
        let _ = capture_native_null_audio_body(&running, &registry, root, "215", true).await;
        tokio::fs::write(root.join("cleanup.json"), b"{\"finite_transcode_cleaned\":true,\"finite_hls_direct_cleaned\":true,\"stop_confirmed\":true,\"capacity_restored\":true,\"owned_output_removed\":true}").await.unwrap();
        export_native_null_audio_evidence(root).await;
    }

    async fn bounded_null_audio_tool(command: &mut Command) -> std::process::Output {
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        apply_ffmpeg_library_path(command);
        let mut child = command.spawn().expect("spawn synthetic media tool");
        let mut stdout = child.stdout.take().unwrap().take(8 * 1024 * 1024 + 1);
        let mut stderr = child.stderr.take().unwrap().take(128 * 1024 + 1);
        let stdout = tokio::spawn(async move {
            let mut bytes = Vec::new();
            stdout.read_to_end(&mut bytes).await.unwrap();
            bytes
        });
        let stderr = tokio::spawn(async move {
            let mut bytes = Vec::new();
            stderr.read_to_end(&mut bytes).await.unwrap();
            bytes
        });
        let waited = tokio::time::timeout(Duration::from_secs(30), child.wait()).await;
        if !matches!(&waited, Ok(Ok(_))) {
            tokio::time::timeout(Duration::from_secs(5), child.kill())
                .await
                .expect("timed-out fixture tool must terminate within its cleanup bound")
                .expect("timed-out fixture tool must be reaped before removing assets");
        }
        let stdout = stdout.await.unwrap();
        let stderr = stderr.await.unwrap();
        let output = std::process::Output {
            status: waited
                .expect("bounded synthetic media operation")
                .expect("wait for synthetic media tool"),
            stdout,
            stderr,
        };
        assert!(
            output.status.success(),
            "synthetic tool failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stdout.len() <= 8 * 1024 * 1024 && output.stderr.len() <= 128 * 1024);
        output
    }

    async fn capture_native_null_audio_body(
        command: &FfmpegCommand,
        registry: &Arc<dyn HlsSessionRegistry>,
        root: &Path,
        channel: &str,
        stop: bool,
    ) -> Vec<u8> {
        use futures_util::{FutureExt as _, StreamExt as _};
        let before = ersatzrs_infra::transcode_capacity::active_transcode_snapshot(4).active;
        let assets = root.join(format!("owned-{channel}"));
        tokio::fs::create_dir(&assets).await.unwrap();
        let reservation =
            try_reserve_active_transcode(4, ActiveTranscodeSlot::new_mpeg_ts(channel))
                .expect("owned direct admission");
        let body = spawn_stream_body(
            command,
            vec![assets.clone()],
            Vec::new(),
            channel.to_owned(),
            Arc::clone(registry),
            Some(reservation),
            None,
            None,
            TeletextOutputAdmission::plain(TeletextOutputRole::FinalDirectMpegTs),
        )
        .await
        .expect("spawn genuine video-only native body");
        let mut stream = body.into_data_stream();
        let capture =
            std::panic::AssertUnwindSafe(tokio::time::timeout(Duration::from_secs(30), async {
                let mut bytes = Vec::new();
                if stop {
                    let first = stream.next().await.expect("active stream bytes").unwrap();
                    bytes.extend_from_slice(&first);
                    assert!(
                        registry.stop_channel(channel).await,
                        "Stop must find the active producer"
                    );
                }
                while let Some(chunk) = stream.next().await {
                    bytes.extend_from_slice(&chunk.unwrap());
                    assert!(bytes.len() <= 8 * 1024 * 1024, "bounded native capture");
                }
                bytes
            }))
            .catch_unwind()
            .await;
        // Preserve the failed capture until the real supervisor confirms
        // producer cleanup. Dropping the body alone only signals cancellation.
        let _ = registry.stop_channel(channel).await;
        drop(stream);
        wait_for_capacity_active(4, before).await;
        assert!(!registry
            .active_channels()
            .await
            .contains(&channel.to_owned()));
        assert!(
            !assets.exists(),
            "owned assets removed only after confirmed encoder reaping"
        );
        let bytes = match capture {
            Ok(result) => result.expect("owned producer must end after finite output or Stop"),
            Err(panic) => std::panic::resume_unwind(panic),
        };
        assert!(!bytes.is_empty(), "real native output required");
        bytes
    }

    async fn prove_native_null_audio_output(ffmpeg: &str, ffprobe: &str, output: &Path) {
        let probe = bounded_null_audio_tool(
            Command::new(ffprobe)
                .args(["-v", "error", "-show_streams", "-of", "json"])
                .arg(output),
        )
        .await;
        tokio::fs::write(output.with_extension("probe.json"), &probe.stdout)
            .await
            .unwrap();
        let probe: serde_json::Value = serde_json::from_slice(&probe.stdout).unwrap();
        let audio = probe["streams"]
            .as_array()
            .unwrap()
            .iter()
            .find(|stream| stream["codec_type"] == "audio")
            .expect("generated audio stream");
        assert_eq!(audio["codec_name"], "aac");
        assert_eq!(audio["sample_rate"], "48000");
        assert_eq!(audio["channels"], 2);
        let frame = bounded_null_audio_tool(
            Command::new(ffmpeg)
                .args(["-v", "error", "-nostdin", "-i"])
                .arg(output)
                .args([
                    "-map",
                    "0:v:0",
                    "-vf",
                    "scale=1:1:flags=area",
                    "-frames:v",
                    "1",
                    "-pix_fmt",
                    "rgb24",
                    "-f",
                    "rawvideo",
                    "pipe:1",
                ]),
        )
        .await;
        assert_eq!(frame.stdout.len(), 3);
        assert!(
            frame.stdout[0] > 180 && frame.stdout[1] < 70 && frame.stdout[2] < 70,
            "video seek must reach red after the first blue second: {:?}",
            frame.stdout
        );
        tokio::fs::write(output.with_extension("rgb"), &frame.stdout)
            .await
            .unwrap();
        let silence = bounded_null_audio_tool(
            Command::new(ffmpeg)
                .args(["-v", "error", "-nostdin", "-i"])
                .arg(output)
                .args(["-map", "0:a:0", "-t", "1", "-f", "s16le", "pipe:1"]),
        )
        .await;
        assert!(
            silence.stdout.len() >= 48_000 * 2 * 2,
            "one second of stereo samples"
        );
        assert!(
            silence.stdout.iter().all(|sample| *sample == 0),
            "generated audio must decode as silence"
        );
        tokio::fs::write(output.with_extension("pcm"), &silence.stdout)
            .await
            .unwrap();
    }

    #[expect(
        clippy::disallowed_methods,
        reason = "Explicit opt-in synthetic runtime evidence export, never production configuration"
    )]
    async fn export_native_null_audio_evidence(root: &Path) {
        let Some(destination) = std::env::var_os("ERSATZRS_ROW194_PROOF_DIR") else {
            return;
        };
        let destination = PathBuf::from(destination).join("native-direct-video-only");
        tokio::fs::create_dir_all(&destination).await.unwrap();
        for stem in ["generated-silence", "hls-direct-silence"] {
            for extension in ["ts", "probe.json", "rgb", "pcm"] {
                let name = format!("{stem}.{extension}");
                tokio::fs::copy(root.join(&name), destination.join(&name))
                    .await
                    .unwrap();
            }
        }
        for name in ["input-probe.json", "cleanup.json"] {
            tokio::fs::copy(root.join(name), destination.join(name))
                .await
                .unwrap();
        }
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
        )
        .expect("the fixture source is playable");
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
            "-fps_mode:v",
            "passthrough",
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

    fn dolby_vision_source(profile: u8, compatibility: u8) -> SourceVideoStream {
        let mut source = SourceVideoStream::new(ersatzrs_ffmpeg::SourceVideoCodec::Hevc);
        source.dolby_vision = Some(ersatzrs_ffmpeg::DolbyVisionConfiguration {
            profile,
            bl_signal_compatibility_id: compatibility,
            rpu_present: true,
            el_present: profile == 7,
            bl_present: true,
        });
        source
    }

    /// ADR 0064 at the IPTV command boundary.
    ///
    /// Asserted here rather than at the supervisor because this is where the
    /// refusal is enforced: a refused source produces no command, so there is
    /// nothing for `spawn_stream_process` to spawn. Testing "no child started"
    /// downstream would be testing that a `None` cannot be passed to a function
    /// that requires a command.
    #[test]
    fn the_iptv_builder_refuses_unplayable_dolby_vision_before_any_spawn() {
        for (profile, compatibility) in [(5_u8, 0_u8), (7, 0), (9, 1)] {
            let error = build_stream_command(
                "/media/dv.mkv",
                OutputFormat::MpegTs,
                Duration::ZERO,
                &playback_profile(),
                None,
                StreamingMode::TransportStream,
                OutputMetadata::default(),
                FfmpegCapabilityReport::default(),
                Some(dolby_vision_source(profile, compatibility)),
                Vec::new(),
                None,
                None,
                Vec::new(),
                Vec::new(),
            )
            .expect_err("an unplayable Dolby Vision profile must be refused");

            assert!(
                !error.reason.is_empty(),
                "profile {profile} must carry an operator-facing reason"
            );
        }
    }

    #[test]
    fn the_iptv_builder_allows_profile_8_1_transcode_but_rejects_unproven_stream_copy() {
        // Profile 8.1: the base layer is a conforming HDR10 signal.
        assert!(build_stream_command(
            "/media/dv81.mkv",
            OutputFormat::MpegTs,
            Duration::ZERO,
            &playback_profile(),
            None,
            StreamingMode::TransportStream,
            OutputMetadata::default(),
            FfmpegCapabilityReport::default(),
            Some(dolby_vision_source(8, 1)),
            Vec::new(),
            None,
            None,
            Vec::new(),
            Vec::new(),
        )
        .is_ok());

        // ADR 0064 has no proven source/container/client passthrough matrix,
        // so even a low-level copy candidate is refused before spawn.
        assert!(build_stream_command(
            "/media/dv5.mkv",
            OutputFormat::HlsMp4,
            Duration::ZERO,
            &playback_profile(),
            None,
            StreamingMode::HttpLiveStreamingDirect,
            OutputMetadata::default(),
            FfmpegCapabilityReport::default(),
            Some(dolby_vision_source(5, 0)),
            Vec::new(),
            None,
            None,
            Vec::new(),
            Vec::new(),
        )
        .is_err());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn profile_5_graph_failure_is_learned_without_a_wrong_colour_respawn() {
        use std::os::unix::fs::PermissionsExt as _;
        use std::sync::atomic::{AtomicBool, Ordering};

        let dir = tempfile::tempdir().expect("temp dir");
        let ffmpeg = dir.path().join("profile5-failing-ffmpeg");
        std::fs::write(
            &ffmpeg,
            "#!/bin/sh\necho '[libplacebo] Failed to initialize renderer' >&2\nexit 1\n",
        )
        .expect("write profile 5 tool");
        let mut permissions = std::fs::metadata(&ffmpeg).expect("metadata").permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&ffmpeg, permissions).expect("make executable");

        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("open profile 5 retry database");
        sqlx::query(
            r#"CREATE TABLE "ConfigElement" (
                "Key" TEXT PRIMARY KEY,
                "Value" TEXT NULL
            )"#,
        )
        .execute(&pool)
        .await
        .expect("create config table");

        let gpu = "NVIDIA GeForce RTX 3080 Ti";
        let mut capabilities = FfmpegCapabilityReport {
            nvidia_gpu_list: Some(format!("[ GPU #0 - < {gpu} > has Compute SM 8.6 ]")),
            ..FfmpegCapabilityReport::default()
        };
        capabilities.prove_hardware_graph(HardwareGraphOperation::NvencDolbyVisionProfile5Sdr, gpu);
        let mut runtime = FfmpegRuntimeReport::for_tests();
        runtime.capabilities = capabilities.clone();
        let shared_runtime = Arc::new(RwLock::new(runtime));
        let rebuild_called = Arc::new(AtomicBool::new(false));
        let rebuild_observer = Arc::clone(&rebuild_called);

        let command =
            FfmpegCommand::new_with_executable(ffmpeg.to_string_lossy(), Vec::new(), Vec::new());
        let mut retry = Some(StreamDenialRetry {
            song: None,
            pool: pool.clone(),
            shared_ffmpeg_runtime: Arc::clone(&shared_runtime),
            runtime_capabilities: capabilities,
            acceleration: ersatzrs_domain::HardwareAccelerationKind::Nvenc,
            source_stream: None,
            graph_operation: Some(HardwareGraphOperation::NvencDolbyVisionProfile5Sdr),
            ffmpeg_path: ffmpeg.to_string_lossy().into_owned(),
            rebuild: Some(immediate_stream_retry(Box::new(move |retry_mode| {
                rebuild_observer.store(true, Ordering::SeqCst);
                let StreamRetryMode::HardwareGraphDenied(runtime_capabilities) = retry_mode else {
                    panic!("profile 5 graph failure must use the graph retry mode");
                };
                assert!(runtime_capabilities.hardware_graph_denied(
                    HardwareGraphOperation::NvencDolbyVisionProfile5Sdr,
                    gpu
                ));
                None
            }))),
        });
        let child = spawn_stream_process(&command, None, "test").expect("spawn profile 5 tool");
        let (_reader, mut writer) = tokio::io::duplex(1024);

        let outcome = supervise_stream_command_with_retry(
            child,
            &mut writer,
            &Arc::new(Notify::new()),
            "1",
            &mut retry,
            None,
            &TeletextOutputAdmission::plain(TeletextOutputRole::FinalDirectMpegTs),
        )
        .await;

        assert!(matches!(outcome, DirectStreamOutcome::Failed(_)));
        assert!(rebuild_called.load(Ordering::SeqCst));
        assert!(retry.is_none(), "there must be no fallback spawn observer");
        let persisted = ersatzrs_infra::hardware_graph::load_hardware_graph_denials(&pool)
            .await
            .expect("load profile 5 denial");
        assert_eq!(
            persisted,
            vec![HardwareGraphDenial::new(
                HardwareGraphOperation::NvencDolbyVisionProfile5Sdr,
                gpu,
                GraphFailureKind::Libplacebo,
            )]
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn graph_fallback_failure_is_learned_without_a_second_retry() {
        use std::os::unix::fs::PermissionsExt as _;

        let dir = tempfile::tempdir().expect("temp dir");
        let primary = dir.path().join("primary-ffmpeg");
        let fallback = dir.path().join("fallback-ffmpeg");
        std::fs::write(
            &primary,
            "#!/bin/sh\necho 'Could not derive CUDA device' >&2\nexit 1\n",
        )
        .expect("write primary tool");
        std::fs::write(
            &fallback,
            "#!/bin/sh\necho '[Parsed_scale_cuda_4] Failed to configure output pad' >&2\nexit 1\n",
        )
        .expect("write fallback tool");
        for path in [&primary, &fallback] {
            let mut permissions = std::fs::metadata(path).expect("metadata").permissions();
            permissions.set_mode(0o755);
            std::fs::set_permissions(path, permissions).expect("make executable");
        }

        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("open graph retry database");
        sqlx::query(
            r#"CREATE TABLE "ConfigElement" (
                "Key" TEXT PRIMARY KEY,
                "Value" TEXT NULL
            )"#,
        )
        .execute(&pool)
        .await
        .expect("create config table");

        let gpu = "NVIDIA GeForce RTX 3080 Ti";
        let mut capabilities = FfmpegCapabilityReport {
            nvidia_gpu_list: Some(format!("[ GPU #0 - < {gpu} > has Compute SM 8.6 ]")),
            ..FfmpegCapabilityReport::default()
        };
        capabilities.prove_hardware_graph(HardwareGraphOperation::NvencHdrVulkanLibplacebo, gpu);
        capabilities.prove_hardware_graph(HardwareGraphOperation::NvencCudaScale, gpu);
        let mut runtime = FfmpegRuntimeReport::for_tests();
        runtime.capabilities = capabilities.clone();
        let shared_runtime = Arc::new(RwLock::new(runtime));

        let initial_command =
            FfmpegCommand::new_with_executable(primary.to_string_lossy(), Vec::new(), Vec::new());
        let fallback_path = fallback.to_string_lossy().into_owned();
        let mut retry = Some(StreamDenialRetry {
            song: None,
            pool: pool.clone(),
            shared_ffmpeg_runtime: Arc::clone(&shared_runtime),
            runtime_capabilities: capabilities,
            acceleration: ersatzrs_domain::HardwareAccelerationKind::Nvenc,
            source_stream: None,
            graph_operation: Some(HardwareGraphOperation::NvencHdrVulkanLibplacebo),
            ffmpeg_path: primary.to_string_lossy().into_owned(),
            rebuild: Some(immediate_stream_retry(Box::new(move |_| {
                Some(FfmpegCommand::new_with_executable(
                    fallback_path,
                    Vec::new(),
                    vec!["-vf".to_owned(), "scale_cuda=256:144".to_owned()],
                ))
            }))),
        });
        let child =
            spawn_stream_process(&initial_command, None, "test").expect("spawn primary tool");
        let (_reader, mut writer) = tokio::io::duplex(1024);

        let outcome = supervise_stream_command_with_retry(
            child,
            &mut writer,
            &Arc::new(Notify::new()),
            "1",
            &mut retry,
            None,
            &TeletextOutputAdmission::plain(TeletextOutputRole::FinalDirectMpegTs),
        )
        .await;

        assert!(matches!(outcome, DirectStreamOutcome::Failed(_)));
        let persisted = ersatzrs_infra::hardware_graph::load_hardware_graph_denials(&pool)
            .await
            .expect("load persisted denials");
        assert_eq!(persisted.len(), 2, "both failed graphs must be learned");
        assert!(persisted.iter().any(|denial| {
            denial
                == &HardwareGraphDenial::new(
                    HardwareGraphOperation::NvencHdrVulkanLibplacebo,
                    gpu,
                    GraphFailureKind::CudaInterop,
                )
        }));
        assert!(persisted.iter().any(|denial| {
            denial
                == &HardwareGraphDenial::new(
                    HardwareGraphOperation::NvencCudaScale,
                    gpu,
                    GraphFailureKind::HardwareMap,
                )
        }));
        let shared = shared_runtime.read().await;
        assert!(shared
            .capabilities
            .hardware_graph_denied(HardwareGraphOperation::NvencHdrVulkanLibplacebo, gpu));
        assert!(shared
            .capabilities
            .hardware_graph_denied(HardwareGraphOperation::NvencCudaScale, gpu));
    }

    #[cfg(unix)]
    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn graph_failure_after_decode_retry_is_learned_without_a_second_retry() {
        use std::os::unix::fs::PermissionsExt as _;

        let dir = tempfile::tempdir().expect("temp dir");
        let primary = dir.path().join("decode-failing-ffmpeg");
        let fallback = dir.path().join("graph-failing-ffmpeg");
        std::fs::write(
            &primary,
            "#!/bin/sh\necho \"Your platform doesn't support hardware accelerated AV1 decoding.\" >&2\nexit 1\n",
        )
        .expect("write primary tool");
        std::fs::write(
            &fallback,
            "#!/bin/sh\necho '[Parsed_scale_cuda_4] Failed to configure output pad' >&2\nexit 1\n",
        )
        .expect("write fallback tool");
        for path in [&primary, &fallback] {
            let mut permissions = std::fs::metadata(path).expect("metadata").permissions();
            permissions.set_mode(0o755);
            std::fs::set_permissions(path, permissions).expect("make executable");
        }

        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("open denial retry database");
        sqlx::query(
            r#"CREATE TABLE "ConfigElement" (
                "Key" TEXT PRIMARY KEY,
                "Value" TEXT NULL
            )"#,
        )
        .execute(&pool)
        .await
        .expect("create config table");

        let gpu = "NVIDIA GeForce RTX 3080 Ti";
        let mut capabilities = FfmpegCapabilityReport {
            nvidia_gpu_list: Some(format!("[ GPU #0 - < {gpu} > has Compute SM 8.6 ]")),
            ..FfmpegCapabilityReport::default()
        };
        capabilities.prove_hardware_graph(HardwareGraphOperation::NvencCudaScale, gpu);
        let mut runtime = FfmpegRuntimeReport::for_tests();
        runtime.capabilities = capabilities.clone();
        let shared_runtime = Arc::new(RwLock::new(runtime));

        let initial_command =
            FfmpegCommand::new_with_executable(primary.to_string_lossy(), Vec::new(), Vec::new());
        let fallback_path = fallback.to_string_lossy().into_owned();
        let mut retry = Some(StreamDenialRetry {
            song: None,
            pool: pool.clone(),
            shared_ffmpeg_runtime: Arc::clone(&shared_runtime),
            runtime_capabilities: capabilities,
            acceleration: ersatzrs_domain::HardwareAccelerationKind::Nvenc,
            source_stream: Some(SourceVideoStream::new(SourceVideoCodec::Av1)),
            graph_operation: Some(HardwareGraphOperation::NvencCudaScale),
            ffmpeg_path: primary.to_string_lossy().into_owned(),
            rebuild: Some(immediate_stream_retry(Box::new(move |_| {
                Some(FfmpegCommand::new_with_executable(
                    fallback_path,
                    Vec::new(),
                    vec!["-vf".to_owned(), "scale_cuda=256:144".to_owned()],
                ))
            }))),
        });
        let child =
            spawn_stream_process(&initial_command, None, "test").expect("spawn primary tool");
        let (_reader, mut writer) = tokio::io::duplex(1024);

        let outcome = supervise_stream_command_with_retry(
            child,
            &mut writer,
            &Arc::new(Notify::new()),
            "1",
            &mut retry,
            None,
            &TeletextOutputAdmission::plain(TeletextOutputRole::FinalDirectMpegTs),
        )
        .await;

        assert!(matches!(outcome, DirectStreamOutcome::Failed(_)));
        let graph_denials = ersatzrs_infra::hardware_graph::load_hardware_graph_denials(&pool)
            .await
            .expect("load graph denials");
        assert_eq!(
            graph_denials,
            vec![HardwareGraphDenial::new(
                HardwareGraphOperation::NvencCudaScale,
                gpu,
                GraphFailureKind::HardwareMap,
            )]
        );
        let decode_denials =
            ersatzrs_infra::ffmpeg_capabilities::load_hardware_decode_denials(&pool)
                .await
                .expect("load decode denials");
        assert_eq!(decode_denials.len(), 1);
        assert!(decode_denials[0].matches(
            ersatzrs_domain::HardwareAccelerationKind::Nvenc,
            "av1",
            8
        ));
        let shared = shared_runtime.read().await;
        assert!(shared
            .capabilities
            .hardware_graph_denied(HardwareGraphOperation::NvencCudaScale, gpu));
        assert!(shared.capabilities.hardware_decode_denied(
            ersatzrs_domain::HardwareAccelerationKind::Nvenc,
            "av1",
            8
        ));
    }
}
