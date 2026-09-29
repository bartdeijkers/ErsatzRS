#[path = "stream_external.rs"]
mod external;

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
use ersatzrs_infra::playback::{
    enrich_source_video_stream_for_playback, load_channel_playback_profile,
    load_channel_target_frame_rate, load_source_audio_streams, load_source_video_stream_by_index,
    render_playback_graphics_overlays, select_channel_playback_subtitle,
    select_custom_playback_streams, select_pinned_playback_subtitle,
    select_playback_audio_stream_index, select_playback_graphics, select_playback_watermarks,
    subtitle_fonts_dir, subtitle_input_for_stream, watermark_inputs_for_stream,
    CustomStreamSelection, PlaybackAudioSelection, PlaybackGraphicsArtifacts,
    PlaybackGraphicsRenderContext, PlaybackProfile, PlaybackSubtitle,
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
    ActiveTranscodeReservation, ActiveTranscodeSlot, DEFAULT_ACTIVE_TRANSCODE_LIMIT,
};
use serde::Deserialize;
use sqlx::Row;
use tokio::io::AsyncReadExt;
#[cfg(test)]
use tokio::process::Command;
use tokio::sync::{Notify, RwLock};
use tokio_util::io::ReaderStream;

use super::retained_stream_buffer::{
    forward_retained_ts, ReservePolicy, TERMINAL_FAILURE, TERMINAL_SUCCESS,
};
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
    _headers: &HeaderMap,
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

    let Some(mode) = channel_transport_stream_mode(query.mode.as_deref(), &channel) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    match mode {
        TransportStreamMode::RedirectToHls => {
            let access_token = AccessToken::new(query.access_token.clone());
            let target = format!(
                "/iptv/channel/{channel_number}.m3u8{}",
                access_token.query()
            );
            axum::response::Redirect::temporary(&target).into_response()
        }
        TransportStreamMode::Legacy => serve_legacy_concat_stream(state, &channel).await,
        TransportStreamMode::HybridWrapper => {
            serve_segmenter_wrapper_stream(state, &channel, query).await
        }
    }
}

#[allow(clippy::too_many_lines)]
pub async fn serve_channel_stream(
    state: &AppState,
    channel_number: &str,
    output_format: OutputFormat,
) -> Response {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
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

    let effective_streaming_mode = channel.streaming_mode;
    serve_channel_stream_with_channel(
        state,
        channel,
        output_format,
        effective_streaming_mode,
        deadline,
    )
    .await
}

pub async fn serve_concat_playlist(
    state: &AppState,
    channel_number: &str,
    _headers: &HeaderMap,
    query: &HlsQuery,
) -> Response {
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
    let playlist = ConcatPlaylist::new("http", internal_addr.to_string(), channel.number, mode);

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

    serve_channel_stream_with_channel(
        state,
        channel,
        OutputFormat::MpegTs,
        StreamingMode::TransportStream,
        deadline,
    )
    .await
}

#[allow(clippy::too_many_lines)]
async fn serve_channel_stream_with_channel(
    state: &AppState,
    channel: Channel,
    output_format: OutputFormat,
    effective_streaming_mode: StreamingMode,
    deadline: tokio::time::Instant,
) -> Response {
    if channel.streaming_engine == ersatzrs_domain::StreamingEngine::Next {
        return StatusCode::NOT_FOUND.into_response();
    }

    serve_channel_stream_admitted(
        state,
        channel,
        output_format,
        effective_streaming_mode,
        deadline,
        None,
    )
    .await
}

struct IntervalStreamAdmission {
    reservation: ActiveTranscodeReservation,
    deadline: tokio::time::Instant,
    cancellation: tokio::sync::oneshot::Receiver<()>,
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
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Response> + Send + '_>> {
    Box::pin(serve_channel_stream_admitted_inner(
        state,
        channel,
        output_format,
        mode,
        deadline,
        admission,
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
    let body = guarded_stream_body(reader, cancel.clone());
    tokio::spawn(async move {
        use futures_util::StreamExt as _;

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
                }),
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
                        if let Ok(policy) = ReservePolicy::new(output.muxrate_bps, output.remaining)
                        {
                            let stream = response
                                .into_body()
                                .into_data_stream()
                                .map(|chunk| chunk.map_err(std::io::Error::other));
                            let mut source = tokio_util::io::StreamReader::new(stream);
                            if let Err(error) = forward_retained_ts(
                                &mut source,
                                &mut writer,
                                policy,
                                &output.terminal,
                                deadline,
                                &cancel,
                            )
                            .await
                            {
                                tracing::warn!(channel_id = channel.id, %error, "retained MPEG-TS reserve ended");
                            }
                            drop(writer); // Public output ends before asynchronous child cleanup.
                            drop(source); // Body disposal signals the owned encoder supervisor.
                            if output.terminal.load(Ordering::Acquire) == 0 {
                                output.completed.notified().await;
                            }
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
        cleanup_stream_artifact(&slate_dir).await;
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

#[allow(clippy::too_many_lines)]
async fn serve_channel_stream_admitted_inner(
    state: &AppState,
    channel: Channel,
    output_format: OutputFormat,
    effective_streaming_mode: StreamingMode,
    startup_deadline: tokio::time::Instant,
    interval_admission: Option<IntervalStreamAdmission>,
) -> Response {
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

    let interval_source = active
        .remote_stream
        .as_ref()
        .and_then(|stream| match &stream.source {
            RemoteStreamSource::RetainedIntervals(source) => Some(source.clone()),
            _ => None,
        });
    if interval_source.is_some() && interval_admission.is_none() {
        return retained_interval_stream_response(
            state.clone(),
            channel,
            output_format,
            effective_streaming_mode,
            startup_deadline,
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
    let previous_audio_streams = source_audio_streams.clone();
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
        if cancelled {
            if let Ok(Some(playback)) = result {
                let _ = playback.prepared.cancel().await;
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
                if let Some(playback) = retained_playback.take() {
                    let _ = playback.prepared.cancel().await;
                }
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
            if let Some(playback) = retained_playback.take() {
                let _ = playback.prepared.cancel().await;
            }
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
                if let Some(playback) = retained_playback.take() {
                    let _ = playback.prepared.cancel().await;
                }
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
                if let Some(playback) = retained_playback.take() {
                    let _ = playback.prepared.cancel().await;
                }
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
                    if let Some(playback) = retained_playback.take() {
                        let _ = playback.prepared.cancel().await;
                    }
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
                if let Some(playback) = retained_playback.take() {
                    let _ = playback.prepared.cancel().await;
                }
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        }
    };
    let mut external_subtitle_owner = None;
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
                if let Some(playback) = retained_playback.take() {
                    let _ = playback.prepared.cancel().await;
                }
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
        if let Some(playback) = retained_playback.take() {
            let _ = playback.prepared.cancel().await;
        }
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
        if let Some(playback) = retained_playback.take() {
            let _ = playback.prepared.cancel().await;
        }
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    let subtitle_input = if interval_source.is_some() {
        retained_subtitle_input
    } else {
        rebase_burned_text_subtitle_input(subtitle_input, start_seconds).await
    };
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
                    if let Some(playback) = retained_playback.take() {
                        let _ = playback.prepared.cancel().await;
                    }
                    return StatusCode::SERVICE_UNAVAILABLE.into_response();
                }
                match result {
                    Ok(inputs) => inputs,
                    Err(error) => {
                        tracing::warn!(channel_id = channel.id, %error,
                            "retained interval watermark preparation failed");
                        if let Some(playback) = retained_playback.take() {
                            let _ = playback.prepared.cancel().await;
                        }
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
                stream_seek_seconds: start_position.as_secs_f64(),
                duration_seconds: active
                    .program_duration()
                    .map_or(0.0, |duration| duration.as_secs_f64()),
                frame_size,
                square_pixel_frame_size,
                frame_rate: target_frame_rate
                    .or_else(|| source_stream.as_ref().and_then(|stream| stream.frame_rate)),
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
                    cleanup_stream_artifact(&graphics_cleanup_dir).await;
                    if let Some(playback) = retained_playback.take() {
                        let _ = playback.prepared.cancel().await;
                    }
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
            shutdown_dynamic_text_controllers(dynamic_text_controllers).await;
            cleanup_stream_artifacts(&graphics_cleanup_dirs).await;
            if let Some(playback) = retained_playback.take() {
                let _ = playback.prepared.cancel().await;
            }
            return (StatusCode::SERVICE_UNAVAILABLE, error.to_string()).into_response();
        }
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
            shutdown_dynamic_text_controllers(dynamic_text_controllers).await;
            cleanup_stream_artifacts(&graphics_cleanup_dirs).await;
            if let Some(playback) = retained_playback.take() {
                let _ = playback.prepared.cancel().await;
            }
            return (StatusCode::UNSUPPORTED_MEDIA_TYPE, error.to_string()).into_response();
        }
    };
    let slate_metadata = metadata.clone();
    let slate_runtime_capabilities = runtime_capabilities.clone();
    let retry_playback_profile = playback_profile.clone();
    let retry_preferred_audio_title = preferred_audio_title.map(str::to_owned);
    let retry_preferred_audio_language = preferred_audio_language.clone();
    let retry = StreamDenialRetry {
        pool: state.pool.clone(),
        shared_ffmpeg_runtime: Arc::clone(&state.ffmpeg_runtime),
        runtime_capabilities,
        acceleration: effective_acceleration,
        source_stream: source_stream.clone(),
        graph_operation: ersatzrs_ffmpeg::selected_hardware_graph(&command),
        ffmpeg_path: command.executable.clone(),
        rebuild: Some(StreamRetryBuilder({
            let media_input = media_input.clone();
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
                        let programme_ms = u64::try_from(start_position.as_millis()).ok()?;
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
                                if let Ok(Some(playback)) = operation.await {
                                    let _ = playback.prepared.cancel().await;
                                }
                                return None;
                            },
                            result = &mut operation => result,
                        };
                        Some(result.ok()??)
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
                                let _ = fresh.prepared.cancel().await;
                            }
                            return None;
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
                    );
                    let Ok(command) = command else {
                        if let Some(fresh) = fresh.take() {
                            let _ = fresh.prepared.cancel().await;
                        }
                        return None;
                    };
                    if fresh.is_some() && tokio::time::Instant::now() >= startup_deadline {
                        if let Some(fresh) = fresh {
                            let _ = fresh.prepared.cancel().await;
                        }
                        return None;
                    }
                    Some(StreamRetryAttempt {
                        command,
                        source_stream,
                        prepared: fresh.map(|fresh| fresh.prepared),
                    })
                })
            })
        })),
    };
    let capacity_reservation = if interval_reservation.is_some() {
        interval_reservation
    } else if effective_streaming_mode == ersatzrs_domain::StreamingMode::TransportStream {
        match reserve_mpeg_ts_capacity(state, &channel.number).await {
            Ok(Some(reservation)) => Some(reservation),
            Ok(None) => {
                if let Some(playback) = retained_playback.take() {
                    let _ = playback.prepared.cancel().await;
                    shutdown_dynamic_text_controllers(dynamic_text_controllers).await;
                    cleanup_stream_artifacts(&graphics_cleanup_dirs).await;
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
                if let Some(playback) = retained_playback.take() {
                    let _ = playback.prepared.cancel().await;
                }
                shutdown_dynamic_text_controllers(dynamic_text_controllers).await;
                cleanup_stream_artifacts(&graphics_cleanup_dirs).await;
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        }
    } else {
        None
    };

    let cleanup_on_spawn_failure = graphics_cleanup_dirs.clone();
    if retained_playback.is_some() && tokio::time::Instant::now() >= startup_deadline {
        if let Some(playback) = retained_playback.take() {
            let _ = playback.prepared.cancel().await;
        }
        shutdown_dynamic_text_controllers(dynamic_text_controllers).await;
        cleanup_stream_artifacts(&graphics_cleanup_dirs).await;
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
        ) {
            Ok(output) => Some(output),
            Err(reason) => {
                tracing::warn!(
                    channel_id = channel.id,
                    reason,
                    "retained MPEG-TS reserve refused"
                );
                if let Some(playback) = retained_playback.take() {
                    let _ = playback.prepared.cancel().await;
                }
                shutdown_dynamic_text_controllers(dynamic_text_controllers).await;
                cleanup_stream_artifacts(&graphics_cleanup_dirs).await;
                return (StatusCode::UNSUPPORTED_MEDIA_TYPE, reason).into_response();
            }
        }
    } else {
        None
    };
    match spawn_stream_body_with_prepared_input(
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
            cleanup_stream_artifacts(&cleanup_on_spawn_failure).await;
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

// A legacy concat request must never silently switch a Next channel's engine.
fn channel_transport_stream_mode(
    requested_mode: Option<&str>,
    channel: &Channel,
) -> Option<TransportStreamMode> {
    let mode = resolve_transport_stream_mode(requested_mode, channel.streaming_mode);
    if channel.streaming_engine == ersatzrs_domain::StreamingEngine::Next
        && mode == TransportStreamMode::Legacy
    {
        None
    } else {
        Some(mode)
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
) -> Response {
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

async fn serve_legacy_concat_stream(state: &AppState, channel: &Channel) -> Response {
    let Some(internal_addr) = state.internal_http_addr.get() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let concat_url = build_legacy_concat_url(*internal_addr, &channel.number);
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
                Vec::new(),
                &playback_profile,
                target_frame_rate,
                metadata,
                runtime_capabilities,
                None,
                None,
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
        Vec::new(),
        channel.number.clone(),
        state.hls_sessions.clone(),
        capacity_reservation,
        None,
        None,
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

fn build_legacy_concat_url(address: std::net::SocketAddr, channel_number: &str) -> String {
    format!("http://{address}/ffmpeg/concat/{channel_number}?mode=ts-legacy")
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
    let mut entries = tokio::fs::read_dir(&root).await.ok()?;
    while let Ok(Some(entry)) = entries.next_entry().await {
        let Ok(file_type) = entry.file_type().await else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }
        let id = entry.file_name().to_string_lossy().into_owned();
        if !mpegts_script_id_eq(&id, script_id) {
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
    #[error("ffmpeg did not expose stdout")]
    MissingStdout,
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
        PlaybackAudioSelection::Automatic => {}
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
    ersatzrs_ffmpeg::build_pipeline_command(acceleration, request)
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
            .map_or(PlaybackAudioSelection::Disabled, |audio| {
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
        PlaybackAudioSelection::Disabled => None,
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

type DeferredStreamRetry = Box<
    dyn FnOnce(
            StreamRetryMode,
            Arc<Notify>,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Option<StreamRetryAttempt>> + Send>,
        > + Send,
>;

struct StreamRetryBuilder(DeferredStreamRetry);

impl StreamRetryBuilder {
    async fn build(
        self,
        mode: StreamRetryMode,
        cancel: &Arc<Notify>,
    ) -> Option<StreamRetryAttempt> {
        (self.0)(mode, cancel.clone()).await
    }
}

#[cfg(test)]
fn immediate_stream_retry(
    build: Box<dyn FnOnce(StreamRetryMode) -> Option<FfmpegCommand> + Send>,
) -> StreamRetryBuilder {
    StreamRetryBuilder(Box::new(move |mode, _| {
        Box::pin(async move {
            build(mode).map(|command| StreamRetryAttempt {
                command,
                source_stream: None,
                prepared: None,
            })
        })
    }))
}

async fn spawn_stream_retry(
    attempt: StreamRetryAttempt,
    producer: Option<&RemoteStreamCommand>,
    channel_number: &str,
) -> Result<RemoteStreamProcess, StreamSpawnError> {
    match attempt.prepared {
        Some(prepared) => spawn_ffmpeg_with_prepared_remote_input(
            &attempt.command,
            None,
            prepared,
            RemoteStreamSpawnContext::mpeg_ts(channel_number),
        )
        .await
        .map_err(|error| StreamSpawnError::Spawn(error.to_string())),
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
    Failed(DirectStreamFailure),
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
    .map_err(|error| StreamSpawnError::Spawn(error.to_string()))
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
    )
    .await
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
) -> Result<Body, StreamSpawnError> {
    let mut child = match prepared {
        Some(prepared) => spawn_ffmpeg_with_prepared_remote_input(
            command,
            None,
            prepared,
            RemoteStreamSpawnContext::mpeg_ts(&channel_number),
        )
        .await
        .map_err(|error| StreamSpawnError::Spawn(error.to_string()))?,
        None => spawn_stream_process(
            command,
            remote_stream.as_ref().and_then(remote_stream_producer),
            &channel_number,
        )?,
    };
    if child.stdout.is_none() {
        let _ = child.kill().await;
        return Err(StreamSpawnError::MissingStdout);
    }
    let handle = Arc::new(DirectStreamHandle::new(channel_number.clone()));
    let cancel = handle.cancel.clone();
    let handle_cancel = handle.cancel.clone();
    let handle: Arc<dyn HlsSessionHandle> = handle;
    let (reader, mut writer) = tokio::io::duplex(STREAM_BODY_BUFFER_BYTES);

    tokio::spawn(async move {
        let _subtitle_owner = subtitle_owner;
        let _capacity_reservation = capacity_reservation;
        session_registry.register_direct(handle.clone()).await;
        let mut retry = retry;
        let outcome = supervise_stream_command_with_retry(
            child,
            &mut writer,
            &cancel,
            &channel_number,
            &mut retry,
            remote_stream.as_ref(),
        )
        .await;
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
            drop(writer);
        }
        session_registry.unregister_direct(&handle).await;
        shutdown_dynamic_text_controllers(dynamic_text_controllers).await;
        for path in cleanup_paths {
            cleanup_stream_artifact(&path).await;
        }
    });

    Ok(guarded_stream_body(reader, handle_cancel))
}

#[allow(clippy::too_many_arguments)]
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
) -> Body {
    let handle = Arc::new(DirectStreamHandle::new(channel_number.clone()));
    let cancel = handle.cancel.clone();
    let handle_cancel = handle.cancel.clone();
    let handle: Arc<dyn HlsSessionHandle> = handle;
    let (reader, mut writer) = tokio::io::duplex(STREAM_BODY_BUFFER_BYTES);

    tokio::spawn(async move {
        let _subtitle_owner = subtitle_owner;
        session_registry.register_direct(handle.clone()).await;
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
                let producer = remote_stream.as_ref().and_then(remote_stream_producer);
                match spawn_stream_process(&command, producer, &channel_number) {
                    Ok(child) => {
                        let _ = supervise_stream_command_with_retry(
                            child,
                            &mut writer,
                            &cancel,
                            &channel_number,
                            &mut retry,
                            remote_stream.as_ref(),
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
            }
        }
        session_registry.unregister_direct(&handle).await;
        shutdown_dynamic_text_controllers(dynamic_text_controllers).await;
        for path in cleanup_paths {
            cleanup_stream_artifact(&path).await;
        }
    });

    guarded_stream_body(reader, handle_cancel)
}

// One linear failure-classification ladder: graph denial, then decode denial,
// then encoder rejection, each with its own persist-and-respawn. Splitting it
// would mean threading the FnOnce rebuild and the &mut child through helpers
// for no gain in clarity.
#[allow(clippy::too_many_lines)]
async fn supervise_stream_command_with_retry(
    mut child: RemoteStreamProcess,
    writer: &mut tokio::io::DuplexStream,
    cancel: &Arc<Notify>,
    channel_number: &str,
    retry: &mut Option<StreamDenialRetry>,
    remote_stream: Option<&ResolvedRemoteStream>,
) -> DirectStreamOutcome {
    loop {
        let outcome = supervise_stream_process_with_retry_context(
            &mut child,
            writer,
            cancel,
            channel_number,
            remote_stream,
            retry.as_ref().map(|retry| StreamFailureContext {
                graph: retry.graph_operation,
                acceleration: retry.acceleration,
                source: retry.source_stream.clone(),
            }),
        )
        .await;
        let DirectStreamOutcome::Failed(mut stderr) = outcome else {
            return outcome;
        };
        if stderr.wrote_media
            && remote_stream.is_some_and(|stream| {
                matches!(stream.source, RemoteStreamSource::RetainedIntervals(_))
            })
        {
            // The outer reserve may already contain this attempt's TS prefix.
            // Never concatenate a restarted programme clock onto those bytes.
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
            let Some(retry_command) = build_retry_command
                .build(
                    StreamRetryMode::HardwareGraphDenied(context.runtime_capabilities.clone()),
                    cancel,
                )
                .await
            else {
                tracing::warn!(
                    channel_number = %channel_number,
                    "rebuilt command was refused; not retrying after graph denial"
                );
                return DirectStreamOutcome::Failed(
                    "rebuilt command was refused".to_owned().into(),
                );
            };
            let next_graph_operation =
                ersatzrs_ffmpeg::selected_hardware_graph(&retry_command.command);
            let next_ffmpeg_path = retry_command.command.executable.clone();
            if retry_command.source_stream.is_some() {
                context
                    .source_stream
                    .clone_from(&retry_command.source_stream);
            }
            match spawn_stream_retry(
                retry_command,
                remote_stream.and_then(remote_stream_producer),
                channel_number,
            )
            .await
            {
                Ok(next) => {
                    child = next;
                    context.graph_operation = next_graph_operation;
                    context.ffmpeg_path = next_ffmpeg_path;
                    // Keep an observer for the fallback spawn. It may learn a
                    // second denial, but cannot trigger another retry.
                    context.rebuild = None;
                    *retry = Some(context);
                }
                Err(error) => {
                    tracing::warn!(%error, channel_number = %channel_number, "failed to respawn ffmpeg channel stream after graph denial");
                    return DirectStreamOutcome::Failed(error.to_string().into());
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
            let Some(retry_command) = rebuild
                .build(
                    StreamRetryMode::HardwareDecodeDenied(context.runtime_capabilities.clone()),
                    cancel,
                )
                .await
            else {
                tracing::warn!(
                    channel_number = %channel_number,
                    "rebuilt command was refused; not retrying after hardware decode denial"
                );
                return DirectStreamOutcome::Failed(
                    "rebuilt command was refused".to_owned().into(),
                );
            };
            let next_graph_operation =
                ersatzrs_ffmpeg::selected_hardware_graph(&retry_command.command);
            let next_ffmpeg_path = retry_command.command.executable.clone();
            if retry_command.source_stream.is_some() {
                context
                    .source_stream
                    .clone_from(&retry_command.source_stream);
            }
            match spawn_stream_retry(
                retry_command,
                remote_stream.and_then(remote_stream_producer),
                channel_number,
            )
            .await
            {
                Ok(next) => {
                    child = next;
                    context.graph_operation = next_graph_operation;
                    context.ffmpeg_path = next_ffmpeg_path;
                    // The retry is spent, but its hardware graph can still
                    // produce new evidence that future sessions must learn.
                    context.rebuild = None;
                    *retry = Some(context);
                }
                Err(error) => {
                    tracing::warn!(%error, channel_number = %channel_number, "failed to respawn ffmpeg channel stream after hardware decode denial");
                    return DirectStreamOutcome::Failed(error.to_string().into());
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
        let Some(retry_command) = build_retry_command
            .build(
                StreamRetryMode::HardwareEncodeDenied(context.runtime_capabilities),
                cancel,
            )
            .await
        else {
            tracing::warn!(
                channel_number = %channel_number,
                "rebuilt command was refused; not retrying after hardware encoder rejection"
            );
            return DirectStreamOutcome::Failed("rebuilt command was refused".to_owned().into());
        };
        match spawn_stream_retry(
            retry_command,
            remote_stream.and_then(remote_stream_producer),
            channel_number,
        )
        .await
        {
            Ok(next) => child = next,
            Err(error) => {
                tracing::warn!(%error, channel_number = %channel_number, "failed to respawn ffmpeg channel stream after hardware encoder rejection");
                return DirectStreamOutcome::Failed(error.to_string().into());
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
    )
    .await
}

async fn supervise_stream_process_with_retry_context(
    child: &mut RemoteStreamProcess,
    writer: &mut tokio::io::DuplexStream,
    cancel: &Arc<Notify>,
    channel_number: &str,
    remote_stream: Option<&ResolvedRemoteStream>,
    retry: Option<StreamFailureContext>,
) -> DirectStreamOutcome {
    let mut stdout = child.stdout.take();
    let stderr_remote_stream = remote_stream.cloned();
    let graph = retry.as_ref().and_then(|retry| retry.graph);
    let acceleration = retry
        .as_ref()
        .map_or(ersatzrs_domain::HardwareAccelerationKind::None, |retry| {
            retry.acceleration
        });
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
            Some(stdout) => tokio::io::copy(stdout, writer).await,
            None => Ok(0),
        }
    };
    let wrote_media = tokio::select! {
        result = pump => {
            if result.is_err() {
                // The body side is gone; stop the process instead of letting
                // it block on a pipe nobody drains.
                tracing::debug!(channel_number = %channel_number, "channel stream body writer failed; killing ffmpeg");
                let _ = child.start_kill();
            }
            result.map_or(true, |bytes| bytes > 0)
        }
        () = cancel.notified() => {
            tracing::debug!(channel_number = %channel_number, "channel stream cancelled; killing ffmpeg");
            if let Err(error) = child.kill().await {
                tracing::warn!(%error, channel_number = %channel_number, "failed to kill ffmpeg channel stream");
            }
            let stderr = await_stderr_tail(stderr_task).await;
            if !stderr.redacted_stderr.is_empty() {
                tracing::debug!(stderr = %stderr, channel_number = %channel_number, "ffmpeg channel stream stderr after cancellation");
            }
            return DirectStreamOutcome::Cancelled;
        }
    };

    let waited = tokio::select! {
        biased;
        () = cancel.notified() => {
            if let Err(error) = child.kill().await {
                tracing::warn!(%error, channel_number, "failed to reap cancelled channel stream after stdout closed");
            }
            None
        },
        result = child.wait() => Some(result),
    };
    let Some(waited) = waited else {
        let _ = await_stderr_tail(stderr_task).await;
        return DirectStreamOutcome::Cancelled;
    };
    match waited {
        Ok(status) if status.success() => {
            let _ = await_stderr_tail(stderr_task).await;
            DirectStreamOutcome::Completed
        }
        Ok(status) => {
            let mut stderr = await_stderr_tail(stderr_task).await;
            stderr.wrote_media = wrote_media;
            if legacy_disconnect_is_success(
                stderr.client_disconnect,
                remote_stream.is_some_and(|stream| {
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
            let mut stderr = await_stderr_tail(stderr_task).await;
            stderr.wrote_media = wrote_media;
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
        let mut channel = Channel {
            streaming_engine: ersatzrs_domain::StreamingEngine::Next,
            streaming_mode: StreamingMode::TransportStreamHybrid,
            ..Default::default()
        };
        for requested in [None, Some("mixed"), Some("ts"), Some("hybrid")] {
            assert_eq!(
                channel_transport_stream_mode(requested, &channel),
                Some(TransportStreamMode::HybridWrapper)
            );
        }
        for requested in ["ts-legacy", "mpegts", "mpeg-ts"] {
            assert_eq!(
                channel_transport_stream_mode(Some(requested), &channel),
                None
            );
        }
        channel.streaming_mode = StreamingMode::HttpLiveStreamingSegmenter;
        assert_eq!(
            channel_transport_stream_mode(None, &channel),
            Some(TransportStreamMode::RedirectToHls)
        );
        assert_eq!(
            channel_transport_stream_mode(Some("ts"), &channel),
            Some(TransportStreamMode::HybridWrapper)
        );
        channel.streaming_mode = StreamingMode::TransportStream;
        assert_eq!(channel_transport_stream_mode(None, &channel), None);
        channel.streaming_engine = ersatzrs_domain::StreamingEngine::Legacy;
        assert_eq!(
            channel_transport_stream_mode(None, &channel),
            Some(TransportStreamMode::Legacy)
        );
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
