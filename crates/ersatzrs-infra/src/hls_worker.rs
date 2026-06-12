use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{
    atomic::{AtomicI32, AtomicI64, Ordering},
    Arc,
};
use std::time::Duration;

use crate::db::DbPool;
use crate::repositories::common::{decode_duration_text, encode_datetime};
use async_trait::async_trait;
use chrono::{DateTime, FixedOffset, NaiveDateTime, Timelike, Utc};
use ersatzrs_core::hls::{HlsSessionHandle, HlsSessionRegistry};
use ersatzrs_core::path_replacement::{replace_path, PathPlatform, PathReplacement};
use ersatzrs_domain::{
    Channel, ChannelMusicVideoCreditsMode, ChannelSubtitleMode, FFmpegProfileAudioFormat,
    FFmpegProfileVideoFormat, StreamingMode, VideoScanKind,
};
use ersatzrs_ffmpeg::{
    build_pipeline_for_acceleration, ffprobe_path, DesiredVideoState, FfmpegCapabilityReport,
    FfmpegCommand, FrameRate, FrameSize, GraphicsPipelineOverlay, HlsOutput, InputFile, InputKind,
    OutputFormat, OutputMetadata, PipelineRequest, RealtimeMode, SourceAudioCodec,
    SourceAudioStream, SourceVideoCodec, SourceVideoStream, SubtitleMethod, WatermarkOpacityClock,
};

use crate::ffmpeg_capabilities::learned_hardware_decode_denial;
use crate::playback::{
    enrich_source_video_stream_for_playback, load_channel_playback_profile,
    load_channel_target_frame_rate, load_source_audio_streams, load_source_video_stream,
    render_playback_graphics_overlays, select_channel_playback_subtitle, select_playback_graphics,
    select_playback_watermarks, subtitle_fonts_dir, subtitle_input_for_stream,
    watermark_inputs_for_stream, PlaybackGraphicsRenderContext, PlaybackProfile, PlaybackSubtitle,
};
use crate::subtitles::rebase_burned_text_subtitle_input;
use sha2::{Digest, Sha256};
use sqlx::Row;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::sync::{Mutex, Notify};
use tokio::time::MissedTickBehavior;

const PLAYLIST_MAX_SEGMENTS: usize = 10;
const PLAYLIST_DELETE_INTERVAL_MS: i64 = 30_000;
const PLAYLIST_FILTER_BEFORE_SECONDS: i64 = 30;
const SEGMENT_CLEANUP_INTERVAL: Duration = Duration::from_secs(5);
const FFMPEG_STDERR_TAIL_BYTES: usize = 16 * 1024;
const HLS_WORK_AHEAD_BUFFER_SECONDS: i64 = 60;
const HLS_REALTIME_BUFFER_SECONDS: i64 = 30;
const HLS_WORK_AHEAD_TRANSCODE_SECONDS: u64 = 44;
const DEFAULT_WORK_AHEAD_SEGMENTER_LIMIT: i32 = 1;

static HLS_WORK_AHEAD_COUNT: AtomicI32 = AtomicI32::new(0);

const ERROR_BACKGROUND_PNG: &[u8] = include_bytes!("../../../assets/_background.png");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HlsSessionState {
    SeekAndWorkAhead,
    ZeroAndWorkAhead,
    SeekAndRealtime,
    ZeroAndRealtime,
    SlugAndWorkAhead,
    SlugAndRealtime,
    #[allow(dead_code)]
    PlayoutUpdated,
}

impl HlsSessionState {
    const fn as_i32(self) -> i32 {
        match self {
            Self::SeekAndWorkAhead => 0,
            Self::ZeroAndWorkAhead => 1,
            Self::SeekAndRealtime => 2,
            Self::ZeroAndRealtime => 3,
            Self::SlugAndWorkAhead => 4,
            Self::SlugAndRealtime => 5,
            Self::PlayoutUpdated => 6,
        }
    }

    const fn from_i32(value: i32) -> Self {
        match value {
            0 => Self::SeekAndWorkAhead,
            1 => Self::ZeroAndWorkAhead,
            3 => Self::ZeroAndRealtime,
            4 => Self::SlugAndWorkAhead,
            5 => Self::SlugAndRealtime,
            6 => Self::PlayoutUpdated,
            // Discriminant 2 is `SeekAndRealtime`; unknown values use the same
            // conservative state instead of panicking in a live stream worker.
            _ => Self::SeekAndRealtime,
        }
    }

    fn start_at_zero(self) -> bool {
        matches!(
            self,
            Self::ZeroAndWorkAhead
                | Self::ZeroAndRealtime
                | Self::SlugAndWorkAhead
                | Self::SlugAndRealtime
        )
    }

    fn is_slug(self) -> bool {
        matches!(self, Self::SlugAndWorkAhead | Self::SlugAndRealtime)
    }

    fn accelerate(self) -> Self {
        match self {
            Self::SeekAndRealtime => Self::SeekAndWorkAhead,
            Self::ZeroAndRealtime => Self::ZeroAndWorkAhead,
            _ => self,
        }
    }

    fn throttle(self) -> Self {
        match self {
            Self::SeekAndWorkAhead => Self::SeekAndRealtime,
            Self::ZeroAndWorkAhead => Self::ZeroAndRealtime,
            Self::SlugAndWorkAhead => Self::SlugAndRealtime,
            _ => self,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct HlsTranscodeDecision {
    state: HlsSessionState,
    realtime: bool,
    working_ahead: bool,
    request_time: DateTime<Utc>,
    start_at_zero: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum HlsProcessOutcome {
    Success,
    Failed(String),
    Cancelled,
}

/// Concrete HLS segmenter worker backed by `FFmpeg` CLI output in a temp folder.
pub struct HlsSessionWorker {
    channel_number: String,
    channel_id: i64,
    working_dir: PathBuf,
    output_format: OutputFormat,
    path_replacements: Vec<PathReplacement>,
    runtime_capabilities: FfmpegCapabilityReport,
    last_access_ms: Arc<AtomicI64>,
    last_delete_ms: Arc<AtomicI64>,
    state: AtomicI32,
    init_segment_cache: Mutex<HlsInitSegmentCache>,
    cancel: Arc<Notify>,
}

impl HlsSessionWorker {
    #[must_use]
    pub fn new(
        channel_number: String,
        channel_id: i64,
        working_dir: PathBuf,
        output_format: OutputFormat,
        path_replacements: Vec<PathReplacement>,
        runtime_capabilities: FfmpegCapabilityReport,
    ) -> Arc<Self> {
        Arc::new(Self {
            channel_number,
            channel_id,
            working_dir,
            output_format,
            path_replacements,
            runtime_capabilities,
            last_access_ms: Arc::new(AtomicI64::new(0)),
            last_delete_ms: Arc::new(AtomicI64::new(0)),
            state: AtomicI32::new(HlsSessionState::SeekAndRealtime.as_i32()),
            init_segment_cache: Mutex::new(HlsInitSegmentCache::default()),
            cancel: Arc::new(Notify::new()),
        })
    }

    fn session_state(&self) -> HlsSessionState {
        HlsSessionState::from_i32(self.state.load(Ordering::Relaxed))
    }

    fn set_session_state(&self, state: HlsSessionState) {
        self.state.store(state.as_i32(), Ordering::Relaxed);
    }

    pub fn start(
        self: Arc<Self>,
        pool: DbPool,
        registry: Arc<dyn HlsSessionRegistry>,
        idle_timeout: Option<Duration>,
    ) {
        tokio::spawn(async move {
            self.run(pool, registry, idle_timeout).await;
        });
    }

    #[allow(clippy::too_many_lines)]
    async fn run(
        self: Arc<Self>,
        pool: DbPool,
        registry: Arc<dyn HlsSessionRegistry>,
        idle_timeout: Option<Duration>,
    ) {
        if let Err(error) = prepare_working_dir(&self.working_dir).await {
            tracing::warn!(%error, working_dir = %self.working_dir.display(), "failed to prepare hls working directory");
            registry.remove(&self.channel_number).await;
            return;
        }
        self.touch();

        let mut is_first = true;
        let mut transcoded_until = Utc::now();
        let mut runtime_capabilities = self.runtime_capabilities.clone();
        let mut learned_decode_retry_used = false;
        let slug_duration = load_channel_slug_seconds(&pool, self.channel_id)
            .await
            .filter(|seconds| *seconds > 0.0)
            .map(Duration::from_secs_f64);
        let has_slug = slug_duration.is_some();
        if let Err(error) =
            time_shift_on_demand_playout(&pool, self.channel_id, transcoded_until).await
        {
            tracing::warn!(
                %error,
                channel_number = %self.channel_number,
                "failed to time shift on-demand playout before starting hls segmenter"
            );
        }
        let initial_work_ahead =
            HLS_WORK_AHEAD_COUNT.load(Ordering::Relaxed) < load_hls_work_ahead_limit(&pool).await;
        self.set_session_state(if initial_work_ahead {
            HlsSessionState::SeekAndWorkAhead
        } else {
            HlsSessionState::SeekAndRealtime
        });
        let mut cleanup_interval = tokio::time::interval(SEGMENT_CLEANUP_INTERVAL);
        cleanup_interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            if let Some(timeout) = idle_timeout {
                let timeout_ms = i64::try_from(timeout.as_millis()).unwrap_or(i64::MAX);
                let idle_for_ms =
                    Utc::now().timestamp_millis() - self.last_access_ms.load(Ordering::Relaxed);
                if idle_for_ms > timeout_ms {
                    break;
                }
            }

            let now = Utc::now();
            if !should_transcode_for_buffer(transcoded_until, now) {
                let _ = self.trim_playlist_inner().await;
                tokio::time::sleep(SEGMENT_CLEANUP_INTERVAL).await;
                continue;
            }

            let realtime_by_buffer = should_use_realtime(transcoded_until, now);
            let work_ahead_reservation = if realtime_by_buffer {
                None
            } else {
                let work_ahead_limit = load_hls_work_ahead_limit(&pool).await;
                try_reserve_work_ahead(work_ahead_limit)
            };
            let realtime = realtime_by_buffer || work_ahead_reservation.is_none();
            let decision =
                hls_transcode_decision(self.session_state(), transcoded_until, now, realtime);
            self.set_session_state(decision.state);

            if decision.state.is_slug() {
                let Some(duration) = slug_duration else {
                    self.set_session_state(next_hls_session_state(decision.state, true, false));
                    continue;
                };
                let playback_profile =
                    match load_channel_playback_profile(&pool, self.channel_id).await {
                        Ok(Some(profile)) => profile,
                        Ok(None) => {
                            tracing::warn!(
                                channel_number = %self.channel_number,
                                "channel has no playback ffmpeg profile; delaying hls slug"
                            );
                            tokio::time::sleep(Duration::from_secs(1)).await;
                            continue;
                        }
                        Err(error) => {
                            tracing::warn!(
                                %error,
                                channel_number = %self.channel_number,
                                "failed to load playback ffmpeg profile; delaying hls slug"
                            );
                            tokio::time::sleep(Duration::from_secs(1)).await;
                            continue;
                        }
                    };
                let channel_name = load_channel_name(&pool, self.channel_id)
                    .await
                    .unwrap_or_else(|| self.channel_number.clone());
                let target_frame_rate = if playback_profile.profile.normalize_framerate {
                    match load_channel_target_frame_rate(&pool, self.channel_id).await {
                        Ok(frame_rate) => frame_rate,
                        Err(error) => {
                            tracing::warn!(
                                %error,
                                channel_number = %self.channel_number,
                                "failed to load channel target frame rate for hls slug"
                            );
                            None
                        }
                    }
                } else {
                    None
                };
                let output_ts_offset = self.probe_output_ts_offset(is_first).await;
                let command = build_hls_slug_command(
                    self.output_format,
                    is_first,
                    decision.realtime,
                    duration,
                    output_ts_offset,
                    &playback_profile,
                    target_frame_rate,
                    OutputMetadata {
                        service_provider: Some("ErsatzTV".to_owned()),
                        service_name: Some(channel_name),
                        ..OutputMetadata::default()
                    },
                    runtime_capabilities.clone(),
                );
                let outcome = self.run_hls_process(command, &mut cleanup_interval).await;
                if let Err(error) =
                    update_on_demand_checkpoint(&pool, self.channel_id, Utc::now()).await
                {
                    tracing::debug!(
                        %error,
                        channel_number = %self.channel_number,
                        "failed to update on-demand checkpoint after hls slug transcode"
                    );
                }
                if outcome == HlsProcessOutcome::Success {
                    transcoded_until = add_std_duration(decision.request_time, duration);
                    self.set_session_state(next_hls_session_state(
                        self.session_state(),
                        true,
                        has_slug,
                    ));
                    is_first = false;
                    continue;
                }
                break;
            }

            let Some(active) = load_active_hls_item(
                &pool,
                self.channel_id,
                &self.path_replacements,
                decision.request_time,
            )
            .await
            else {
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            };
            let elapsed_seconds = if decision.start_at_zero {
                0
            } else {
                playout_start_seconds(decision.request_time, active.playout_start)
            };
            let start_seconds = active.content_seek_seconds(elapsed_seconds);
            let duration = hls_playout_item_duration(
                &active,
                decision.request_time,
                decision.start_at_zero,
                decision.realtime,
            );
            if duration.is_none_or(|duration| duration.is_zero()) {
                tokio::time::sleep(Duration::from_millis(250)).await;
                continue;
            }

            let source_stream = load_source_video_stream(&pool, active.media_version_id).await;
            let source_audio_streams =
                load_source_audio_streams(&pool, active.media_version_id).await;
            let playback_profile = match load_channel_playback_profile(&pool, self.channel_id).await
            {
                Ok(Some(profile)) => profile,
                Ok(None) => {
                    tracing::warn!(
                        channel_number = %self.channel_number,
                        "channel has no playback ffmpeg profile; delaying hls worker"
                    );
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    continue;
                }
                Err(error) => {
                    tracing::warn!(
                        %error,
                        channel_number = %self.channel_number,
                        "failed to load playback ffmpeg profile; delaying hls worker"
                    );
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    continue;
                }
            };
            let source_stream = enrich_source_video_stream_for_playback(
                source_stream,
                playback_profile.profile.hardware_acceleration,
                &active.media_path,
            )
            .await;
            let target_frame_rate = if playback_profile.profile.normalize_framerate {
                match load_channel_target_frame_rate(&pool, self.channel_id).await {
                    Ok(frame_rate) => frame_rate,
                    Err(error) => {
                        tracing::warn!(
                            %error,
                            channel_number = %self.channel_number,
                            "failed to load channel target frame rate"
                        );
                        None
                    }
                }
            } else {
                None
            };
            let subtitle_mode = active.subtitle_mode.unwrap_or(active.channel_subtitle_mode);
            let preferred_subtitle_language = active
                .preferred_subtitle_language_code
                .as_deref()
                .or(active.channel_preferred_subtitle_language_code.as_deref());
            let channel_for_subtitles =
                active.channel_for_runtime_subtitles(self.channel_id, self.channel_number.clone());
            let subtitle = match select_channel_playback_subtitle(
                &pool,
                active.media_item_id,
                preferred_subtitle_language,
                subtitle_mode,
                StreamingMode::HttpLiveStreamingSegmenter,
                &channel_for_subtitles,
                &playback_profile.resolution,
                start_seconds,
            )
            .await
            {
                Ok(subtitle) => subtitle,
                Err(error) => {
                    tracing::warn!(%error, media_item_id = active.media_item_id, "failed to select HLS subtitle");
                    None
                }
            };
            let subtitle_input = subtitle.as_ref().and_then(|subtitle| {
                subtitle_input_for_stream(
                    subtitle,
                    &active.media_path,
                    self.output_format,
                    StreamingMode::HttpLiveStreamingSegmenter,
                )
            });
            let metadata = playback_output_metadata(
                Some("ErsatzTV"),
                Some(&active.channel_name),
                active
                    .preferred_audio_language_code
                    .as_deref()
                    .or(active.channel_preferred_audio_language_code.as_deref()),
                &source_audio_streams,
                subtitle.as_ref(),
                None,
            );
            let subtitle_input =
                rebase_burned_text_subtitle_input(subtitle_input, start_seconds).await;
            let watermark_channel = Channel {
                id: self.channel_id,
                watermark_id: active.channel_watermark_id,
                streaming_mode: StreamingMode::HttpLiveStreamingSegmenter,
                ..Channel::default()
            };
            let watermark_inputs = match select_playback_watermarks(
                &pool,
                &watermark_channel,
                active.playout_item_id,
                active.disable_watermarks,
                now,
            )
            .await
            {
                Ok(watermarks) => {
                    watermark_inputs_for_stream(watermarks, active.program_duration).await
                }
                Err(error) => {
                    tracing::warn!(%error, channel_number = %self.channel_number, "failed to select HLS watermarks");
                    Vec::new()
                }
            };
            let mut graphics_cleanup_dir = None;
            let graphics_overlays = match select_playback_graphics(
                &pool,
                &watermark_channel,
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
                    let output_dir = self
                        .working_dir
                        .join("graphics")
                        .join(format!("item-{}", active.playout_item_id));
                    graphics_cleanup_dir = Some(output_dir.clone());
                    match render_playback_graphics_overlays(
                        &pool,
                        &graphics,
                        &PlaybackGraphicsRenderContext {
                            channel_id: self.channel_id,
                            channel_number: self.channel_number.clone(),
                            media_item_id: active.media_item_id,
                            media_path: active.media_path.clone(),
                            channel_start_time: active.playout_start,
                            content_start_time: active.playout_start,
                            stream_seek_seconds: seconds_as_f64(start_seconds),
                            duration_seconds: active
                                .program_duration
                                .map_or(0.0, |duration| duration.as_secs_f64()),
                            frame_size,
                            square_pixel_frame_size,
                            frame_rate: target_frame_rate.or_else(|| {
                                source_stream.as_ref().and_then(|stream| stream.frame_rate)
                            }),
                        },
                        output_dir,
                    )
                    .await
                    {
                        Ok(overlays) => overlays,
                        Err(error) => {
                            if let Some(path) = &graphics_cleanup_dir {
                                let _ = tokio::fs::remove_dir_all(path).await;
                            }
                            graphics_cleanup_dir = None;
                            tracing::warn!(%error, channel_number = %self.channel_number, "failed to render HLS graphics overlays");
                            Vec::new()
                        }
                    }
                }
                Err(error) => {
                    tracing::warn!(%error, channel_number = %self.channel_number, "failed to select HLS graphics");
                    Vec::new()
                }
            };
            let output_ts_offset = self.probe_output_ts_offset(is_first).await;
            let source_stream_for_denial = source_stream.clone();
            let opacity_clock = watermark_opacity_clock_at_spawn(
                decision.request_time,
                duration_from_seconds(start_seconds),
                active.content_duration,
                active.playout_start,
            );
            let command = build_hls_command_with_opacity_clock(
                &active.media_path,
                &self.working_dir,
                self.output_format,
                is_first,
                false,
                decision.realtime,
                start_seconds,
                duration,
                output_ts_offset,
                &playback_profile,
                target_frame_rate,
                metadata,
                runtime_capabilities.clone(),
                source_stream,
                source_audio_streams,
                None,
                subtitle_input,
                watermark_inputs,
                graphics_overlays,
                Some(opacity_clock),
            );
            log_hls_av1_spawn_diagnostics(
                &self.channel_number,
                &command,
                &runtime_capabilities,
                source_stream_for_denial.as_ref(),
            );
            let outcome = self.run_hls_process(command, &mut cleanup_interval).await;
            if let Some(path) = graphics_cleanup_dir {
                let _ = cleanup_working_dir(&path).await;
            }
            if let Err(error) =
                update_on_demand_checkpoint(&pool, self.channel_id, Utc::now()).await
            {
                tracing::debug!(
                    %error,
                    channel_number = %self.channel_number,
                    "failed to update on-demand checkpoint after hls transcode"
                );
            }
            match outcome {
                HlsProcessOutcome::Success => {
                    if let Some(duration) = duration {
                        transcoded_until = add_std_duration(decision.request_time, duration);
                        self.set_session_state(next_hls_session_state(
                            self.session_state(),
                            item_complete(&active, transcoded_until),
                            has_slug,
                        ));
                    }
                }
                HlsProcessOutcome::Failed(error_message) => {
                    if let Some(denial) = learned_hardware_decode_denial(
                        learned_decode_retry_used,
                        playback_profile.profile.hardware_acceleration,
                        source_stream_for_denial.as_ref(),
                        &error_message,
                    ) {
                        learned_decode_retry_used = true;
                        tracing::warn!(
                            channel_number = %self.channel_number,
                            acceleration = ?denial.acceleration,
                            codec = %denial.codec,
                            bit_depth = denial.bit_depth,
                            "retrying hls segmenter once with software input decode after runtime hardware decode rejection"
                        );
                        if let Err(error) =
                            crate::ffmpeg_capabilities::persist_hardware_decode_denial(
                                &pool,
                                &mut runtime_capabilities,
                                denial,
                            )
                            .await
                        {
                            tracing::warn!(
                                %error,
                                channel_number = %self.channel_number,
                                "failed to persist runtime-learned hardware decode denial"
                            );
                        }
                        continue;
                    }
                    let Some(duration) = duration else {
                        break;
                    };
                    let Some(error_command) = build_hls_error_command(HlsErrorProcessRequest {
                        working_dir: &self.working_dir,
                        output_format: self.output_format,
                        is_first_transcode: is_first,
                        hls_realtime: decision.realtime,
                        duration,
                        output_ts_offset,
                        playback_profile: &playback_profile,
                        target_frame_rate,
                        metadata: OutputMetadata {
                            service_provider: Some("ErsatzTV".to_owned()),
                            service_name: Some(active.channel_name.clone()),
                            ..OutputMetadata::default()
                        },
                        runtime_capabilities: runtime_capabilities.clone(),
                        error_message: &error_message,
                    })
                    .await
                    else {
                        break;
                    };
                    if self
                        .run_hls_process(error_command, &mut cleanup_interval)
                        .await
                        == HlsProcessOutcome::Success
                    {
                        transcoded_until = add_std_duration(decision.request_time, duration);
                        self.set_session_state(next_hls_session_state(
                            self.session_state(),
                            false,
                            has_slug,
                        ));
                    } else {
                        break;
                    }
                }
                HlsProcessOutcome::Cancelled => break,
            }

            is_first = false;
        }

        let _ = cleanup_working_dir(&self.working_dir).await;
        registry.remove(&self.channel_number).await;
    }

    async fn run_hls_process(
        &self,
        command: FfmpegCommand,
        cleanup_interval: &mut tokio::time::Interval,
    ) -> HlsProcessOutcome {
        let mut process = Command::new(&command.executable);
        process
            .args(&command.arguments)
            .current_dir(&self.working_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        for variable in command.environment {
            process.env(variable.name, variable.value);
        }

        let Ok(mut child) = process.spawn() else {
            tracing::warn!(channel_number = %self.channel_number, "failed to spawn ffmpeg hls segmenter");
            return HlsProcessOutcome::Failed("failed to spawn ffmpeg hls segmenter".to_owned());
        };
        let mut stderr_task = child
            .stderr
            .take()
            .map(|stderr| tokio::spawn(read_ffmpeg_stderr_tail(stderr)));

        loop {
            tokio::select! {
                result = child.wait() => {
                    match result {
                        Ok(status) if status.success() => return HlsProcessOutcome::Success,
                        Ok(status) => {
                            let stderr = await_stderr_tail(stderr_task.take()).await;
                            let error_message = if stderr.trim().is_empty() {
                                format!("Unknown FFMPEG error; exit code {status}")
                            } else {
                                stderr.clone()
                            };
                            tracing::warn!(
                                channel_number = %self.channel_number,
                                %status,
                                stderr = %stderr,
                                "ffmpeg hls segmenter exited unsuccessfully"
                            );
                            return HlsProcessOutcome::Failed(error_message);
                        }
                        Err(error) => {
                            let stderr = await_stderr_tail(stderr_task.take()).await;
                            let error_message = if stderr.trim().is_empty() {
                                format!("failed to wait for ffmpeg hls segmenter: {error}")
                            } else {
                                stderr.clone()
                            };
                            tracing::warn!(
                                channel_number = %self.channel_number,
                                %error,
                                stderr = %stderr,
                                "failed to wait for ffmpeg hls segmenter"
                            );
                            return HlsProcessOutcome::Failed(error_message);
                        }
                    }
                }
                () = self.cancel.notified() => {
                    let _ = child.start_kill();
                    return HlsProcessOutcome::Cancelled;
                }
                _ = cleanup_interval.tick() => {
                    let _ = self.trim_playlist_inner().await;
                }
            }
        }
    }

    async fn probe_output_ts_offset(&self, is_first: bool) -> Option<Duration> {
        if is_first {
            return None;
        }

        if self.output_format == OutputFormat::HlsMp4 {
            let mut init_cache = self.init_segment_cache.lock().await;
            let _ = refresh_init_segments(&self.working_dir, &mut init_cache).await;
            match probe_last_pts_offset(&self.working_dir, Some(&init_cache)).await {
                Ok(offset) => offset,
                Err(error) => {
                    tracing::warn!(
                        %error,
                        channel_number = %self.channel_number,
                        "failed to probe last fMP4 HLS segment PTS offset"
                    );
                    None
                }
            }
        } else {
            match probe_last_pts_offset(&self.working_dir, None).await {
                Ok(offset) => offset,
                Err(error) => {
                    tracing::warn!(
                        %error,
                        channel_number = %self.channel_number,
                        "failed to probe last HLS segment PTS offset"
                    );
                    None
                }
            }
        }
    }

    async fn trim_playlist_inner(&self) -> Option<TrimmedPlaylist> {
        let playlist_path = self.working_dir.join("live.m3u8");
        let playlist = tokio::fs::read_to_string(playlist_path).await.ok()?;
        let filter_before = Utc::now() - chrono::Duration::seconds(PLAYLIST_FILTER_BEFORE_SECONDS);
        let trimmed = if self.output_format == OutputFormat::HlsMp4 {
            let mut init_cache = self.init_segment_cache.lock().await;
            let _ = refresh_init_segments(&self.working_dir, &mut init_cache).await;
            trim_playlist_contents(
                self.output_format,
                &playlist,
                filter_before,
                Some(PLAYLIST_MAX_SEGMENTS),
                Some(&init_cache),
            )?
        } else {
            trim_playlist_contents(
                self.output_format,
                &playlist,
                filter_before,
                Some(PLAYLIST_MAX_SEGMENTS),
                None,
            )?
        };

        let now_ms = Utc::now().timestamp_millis();
        if now_ms - self.last_delete_ms.load(Ordering::Relaxed) >= PLAYLIST_DELETE_INTERVAL_MS {
            let mut init_cache = self.init_segment_cache.lock().await;
            let _ = refresh_init_segments(&self.working_dir, &mut init_cache).await;
            let _ = delete_old_segments(&self.working_dir, &trimmed, &mut init_cache).await;
            self.last_delete_ms.store(now_ms, Ordering::Relaxed);
        }

        Some(trimmed)
    }
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

fn log_hls_av1_spawn_diagnostics(
    channel_number: &str,
    command: &FfmpegCommand,
    runtime_capabilities: &FfmpegCapabilityReport,
    source: Option<&SourceVideoStream>,
) {
    if !source.is_some_and(|source| source.codec == SourceVideoCodec::Av1) {
        return;
    }

    let av1_decoders: Vec<String> = runtime_capabilities
        .nvidia_decoders
        .iter()
        .filter(|decoder| decoder.codec.eq_ignore_ascii_case("av1"))
        .map(|decoder| format!("{} {}-bit", decoder.codec, decoder.bit_depth))
        .collect();
    let av1_denials: Vec<String> = runtime_capabilities
        .hardware_decode_denials
        .iter()
        .filter(|denial| denial.codec.eq_ignore_ascii_case("av1"))
        .map(|denial| {
            format!(
                "{:?} {} {}-bit",
                denial.acceleration, denial.codec, denial.bit_depth
            )
        })
        .collect();

    tracing::debug!(
        channel_number = %channel_number,
        executable = %command.executable,
        arguments = ?command.arguments,
        nvidia_av1_decoders = ?av1_decoders,
        hardware_decode_denials = ?av1_denials,
        "spawning hls segmenter for av1 source with runtime capability rows"
    );
}

#[async_trait]
impl HlsSessionHandle for HlsSessionWorker {
    fn channel_number(&self) -> &str {
        &self.channel_number
    }

    fn touch(&self) {
        self.last_access_ms
            .store(Utc::now().timestamp_millis(), Ordering::Relaxed);
    }

    async fn trim_playlist(&self) -> Option<String> {
        self.trim_playlist_inner()
            .await
            .map(|trimmed| trimmed.playlist)
    }

    async fn cancel(&self) {
        // Wake the worker's `run` select loop, which kills the FFmpeg child,
        // cleans up the working directory, and removes itself from the
        // registry. `notify_one` stores a permit if the worker is briefly
        // between iterations, so the cancellation is never lost.
        self.cancel.notify_one();
    }

    fn playout_updated(&self) {
        self.set_session_state(HlsSessionState::PlayoutUpdated);
    }
}

#[allow(clippy::too_many_arguments)]
#[must_use]
pub fn build_hls_command(
    media_path: &str,
    _working_dir: &Path,
    output_format: OutputFormat,
    is_first_transcode: bool,
    troubleshooting: bool,
    hls_realtime: bool,
    start_seconds: i64,
    duration: Option<Duration>,
    output_ts_offset: Option<Duration>,
    playback_profile: &PlaybackProfile,
    target_frame_rate: Option<FrameRate>,
    metadata: OutputMetadata,
    runtime_capabilities: FfmpegCapabilityReport,
    source_stream: Option<SourceVideoStream>,
    source_audio_streams: Vec<ersatzrs_ffmpeg::SourceAudioStream>,
    selected_audio_stream_index: Option<usize>,
    subtitle_input: Option<InputFile>,
    watermark_inputs: Vec<(InputFile, ersatzrs_ffmpeg::WatermarkOverlay)>,
    graphics_overlays: Vec<GraphicsPipelineOverlay>,
) -> FfmpegCommand {
    build_hls_command_with_opacity_clock(
        media_path,
        _working_dir,
        output_format,
        is_first_transcode,
        troubleshooting,
        hls_realtime,
        start_seconds,
        duration,
        output_ts_offset,
        playback_profile,
        target_frame_rate,
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
#[must_use]
pub fn build_hls_command_with_opacity_clock(
    media_path: &str,
    _working_dir: &Path,
    output_format: OutputFormat,
    is_first_transcode: bool,
    troubleshooting: bool,
    hls_realtime: bool,
    start_seconds: i64,
    duration: Option<Duration>,
    output_ts_offset: Option<Duration>,
    playback_profile: &PlaybackProfile,
    target_frame_rate: Option<FrameRate>,
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
    let input = InputFile::video(media_path.to_string());
    let mut desired = DesiredVideoState::from_profile(
        &playback_profile.profile,
        &playback_profile.resolution,
        source_stream.as_ref(),
        target_frame_rate,
        StreamingMode::HttpLiveStreamingSegmenter,
    );
    if hls_realtime {
        desired.realtime_mode = RealtimeMode::On;
    }
    let mut request =
        PipelineRequest::new(input, desired).with_runtime_capabilities(runtime_capabilities);
    if let Some(clock) = opacity_clock {
        request = request.with_watermark_opacity_clock(clock);
    }
    if let Some(stream_index) = selected_audio_stream_index {
        if let Some(audio_input) = request
            .inputs
            .iter_mut()
            .find(|input| input.kind == InputKind::Audio)
        {
            *audio_input = audio_input.clone().with_stream_index(stream_index);
        }
    }
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
    request.state.duration = duration;
    request.state.output_ts_offset = output_ts_offset;
    request.state.metadata = metadata;
    request.state.report_progress = false;
    if let Ok(thread_count) = u32::try_from(playback_profile.profile.thread_count) {
        if thread_count > 0 {
            request.state.thread_count = Some(thread_count);
        }
    }
    let hls_generated_at = Utc::now().timestamp();
    request.state.hls = Some(HlsOutput {
        segment_template: match output_format {
            OutputFormat::HlsMp4 => format!("live_{hls_generated_at}_%06d.m4s"),
            _ => "live%06d.ts".to_string(),
        },
        init_template: match output_format {
            OutputFormat::HlsMp4 => Some(format!("{hls_generated_at}_init.mp4")),
            _ => None,
        },
        playlist_path: "live.m3u8".to_string(),
        segment_options: hls_segment_options(
            output_format,
            output_ts_offset,
            playback_profile.profile.audio_format,
        ),
        is_first_transcode,
        troubleshooting,
    });

    build_pipeline_for_acceleration(
        playback_profile.profile.hardware_acceleration,
        request.clone(),
    )
    .command(&request.inputs)
}

#[allow(clippy::too_many_arguments)]
#[must_use]
pub fn build_hls_slug_command(
    output_format: OutputFormat,
    is_first_transcode: bool,
    hls_realtime: bool,
    duration: Duration,
    output_ts_offset: Option<Duration>,
    playback_profile: &PlaybackProfile,
    target_frame_rate: Option<FrameRate>,
    metadata: OutputMetadata,
    runtime_capabilities: FfmpegCapabilityReport,
) -> FfmpegCommand {
    let frame_size = playback_frame_size(playback_profile);
    let frame_rate = target_frame_rate.unwrap_or(FrameRate::DEFAULT);
    let video_input = InputFile::lavfi_video(format!(
        "color=c=black:s={}x{}:r={}:d={}",
        frame_size.width,
        frame_size.height,
        frame_rate.r_frame_rate(),
        duration.as_secs_f64()
    ));
    let audio_sample_rate = generated_audio_sample_rate(playback_profile);
    let audio_input = InputFile::lavfi_audio(format!(
        "anullsrc=channel_layout=stereo:sample_rate={audio_sample_rate}"
    ));
    let mut desired = DesiredVideoState::from_profile(
        &playback_profile.profile,
        &playback_profile.resolution,
        None,
        target_frame_rate,
        StreamingMode::HttpLiveStreamingSegmenter,
    );
    if matches!(
        desired.video_format,
        FFmpegProfileVideoFormat::Copy | FFmpegProfileVideoFormat::None
    ) {
        desired.video_format = FFmpegProfileVideoFormat::H264;
    }
    if desired.audio_format == FFmpegProfileAudioFormat::Copy {
        desired.audio_format = FFmpegProfileAudioFormat::Aac;
    }
    if hls_realtime {
        desired.realtime_mode = RealtimeMode::On;
    }
    desired.frame_rate = Some(frame_rate);
    desired.scaled_size = frame_size;
    desired.frame_size = frame_size;
    desired.cropped_size = None;

    let mut request = PipelineRequest {
        inputs: vec![video_input, audio_input],
        state: ersatzrs_ffmpeg::FfmpegState::default(),
        desired,
        source_video_stream: Some(generated_slug_video_stream(frame_size, frame_rate)),
        source_audio_streams: vec![generated_slug_audio_stream(audio_sample_rate)],
        runtime_capabilities: Some(runtime_capabilities),
        watermark: None,
        watermarks: Vec::new(),
        watermark_opacity_clock: WatermarkOpacityClock::default(),
        drawvg_filters: Vec::new(),
        graphics_overlays: Vec::new(),
        subtitle_fonts_dir: None,
    };
    request.state.output_format = output_format;
    request.state.duration = Some(duration);
    request.state.output_ts_offset = output_ts_offset;
    request.state.metadata = metadata;
    request.state.report_progress = false;
    if let Ok(thread_count) = u32::try_from(playback_profile.profile.thread_count) {
        if thread_count > 0 {
            request.state.thread_count = Some(thread_count);
        }
    }
    let hls_generated_at = Utc::now().timestamp();
    request.state.hls = Some(HlsOutput {
        segment_template: match output_format {
            OutputFormat::HlsMp4 => format!("live_{hls_generated_at}_%06d.m4s"),
            _ => "live%06d.ts".to_string(),
        },
        init_template: match output_format {
            OutputFormat::HlsMp4 => Some(format!("{hls_generated_at}_init.mp4")),
            _ => None,
        },
        playlist_path: "live.m3u8".to_string(),
        segment_options: hls_segment_options(
            output_format,
            output_ts_offset,
            request.desired.audio_format,
        ),
        is_first_transcode,
        troubleshooting: false,
    });

    build_pipeline_for_acceleration(
        playback_profile.profile.hardware_acceleration,
        request.clone(),
    )
    .command(&request.inputs)
}

struct HlsErrorProcessRequest<'a> {
    working_dir: &'a Path,
    output_format: OutputFormat,
    is_first_transcode: bool,
    hls_realtime: bool,
    duration: Duration,
    output_ts_offset: Option<Duration>,
    playback_profile: &'a PlaybackProfile,
    target_frame_rate: Option<FrameRate>,
    metadata: OutputMetadata,
    runtime_capabilities: FfmpegCapabilityReport,
    error_message: &'a str,
}

async fn build_hls_error_command(request: HlsErrorProcessRequest<'_>) -> Option<FfmpegCommand> {
    let assets_dir = request.working_dir.join("error");
    if let Err(error) = tokio::fs::create_dir_all(&assets_dir).await {
        tracing::warn!(%error, path = %assets_dir.display(), "failed to create hls error asset directory");
        return None;
    }

    let background_path = assets_dir.join("background.png");
    if let Err(error) = tokio::fs::write(&background_path, ERROR_BACKGROUND_PNG).await {
        tracing::warn!(%error, path = %background_path.display(), "failed to write hls error background");
        return None;
    }

    let subtitle_path = assets_dir.join("error.ass");
    let subtitle = hls_error_ass(
        request.error_message,
        playback_frame_size(request.playback_profile),
    );
    if let Err(error) = tokio::fs::write(&subtitle_path, subtitle).await {
        tracing::warn!(%error, path = %subtitle_path.display(), "failed to write hls error subtitle");
        return None;
    }

    Some(build_hls_error_command_from_assets(
        HlsErrorCommandRequest {
            background_path: &background_path,
            subtitle_path: &subtitle_path,
            output_format: request.output_format,
            is_first_transcode: request.is_first_transcode,
            hls_realtime: request.hls_realtime,
            duration: request.duration,
            output_ts_offset: request.output_ts_offset,
            playback_profile: request.playback_profile,
            target_frame_rate: request.target_frame_rate,
            metadata: request.metadata,
            runtime_capabilities: request.runtime_capabilities,
        },
    ))
}

struct HlsErrorCommandRequest<'a> {
    background_path: &'a Path,
    subtitle_path: &'a Path,
    output_format: OutputFormat,
    is_first_transcode: bool,
    hls_realtime: bool,
    duration: Duration,
    output_ts_offset: Option<Duration>,
    playback_profile: &'a PlaybackProfile,
    target_frame_rate: Option<FrameRate>,
    metadata: OutputMetadata,
    runtime_capabilities: FfmpegCapabilityReport,
}

#[must_use]
fn build_hls_error_command_from_assets(request: HlsErrorCommandRequest<'_>) -> FfmpegCommand {
    let frame_size = playback_frame_size(request.playback_profile);
    let frame_rate = request.target_frame_rate.unwrap_or(FrameRate::DEFAULT);
    let video_input = InputFile::video(request.background_path.to_string_lossy().into_owned())
        .with_input_options(["-loop", "1"]);
    let audio_sample_rate = generated_audio_sample_rate(request.playback_profile);
    let audio_input = InputFile::lavfi_audio(format!(
        "anullsrc=channel_layout=stereo:sample_rate={audio_sample_rate}"
    ));
    let subtitle_input = InputFile::text_subtitle(
        request.subtitle_path.to_string_lossy().into_owned(),
        SubtitleMethod::Burn,
    );
    let mut desired = DesiredVideoState::from_profile(
        &request.playback_profile.profile,
        &request.playback_profile.resolution,
        Some(&generated_error_video_stream(frame_size, frame_rate)),
        request.target_frame_rate,
        StreamingMode::HttpLiveStreamingSegmenter,
    );
    if matches!(
        desired.video_format,
        FFmpegProfileVideoFormat::Copy | FFmpegProfileVideoFormat::None
    ) {
        desired.video_format = FFmpegProfileVideoFormat::H264;
    }
    if desired.audio_format == FFmpegProfileAudioFormat::Copy {
        desired.audio_format = FFmpegProfileAudioFormat::Aac;
    }
    if request.hls_realtime {
        desired.realtime_mode = RealtimeMode::On;
    }
    desired.frame_rate = Some(frame_rate);
    desired.scaled_size = frame_size;
    desired.frame_size = frame_size;
    desired.cropped_size = None;

    let mut pipeline_request = PipelineRequest {
        inputs: vec![video_input, audio_input, subtitle_input],
        state: ersatzrs_ffmpeg::FfmpegState::default(),
        desired,
        source_video_stream: Some(generated_error_video_stream(frame_size, frame_rate)),
        source_audio_streams: vec![generated_slug_audio_stream(audio_sample_rate)],
        runtime_capabilities: Some(request.runtime_capabilities),
        watermark: None,
        watermarks: Vec::new(),
        watermark_opacity_clock: WatermarkOpacityClock::default(),
        drawvg_filters: Vec::new(),
        graphics_overlays: Vec::new(),
        subtitle_fonts_dir: Some(subtitle_fonts_dir()),
    };
    pipeline_request.state.output_format = request.output_format;
    pipeline_request.state.duration = Some(request.duration);
    pipeline_request.state.output_ts_offset = request.output_ts_offset;
    pipeline_request.state.metadata = request.metadata;
    pipeline_request.state.report_progress = false;
    if let Ok(thread_count) = u32::try_from(request.playback_profile.profile.thread_count) {
        if thread_count > 0 {
            pipeline_request.state.thread_count = Some(thread_count);
        }
    }
    let hls_generated_at = Utc::now().timestamp();
    pipeline_request.state.hls = Some(HlsOutput {
        segment_template: match request.output_format {
            OutputFormat::HlsMp4 => format!("live_{hls_generated_at}_%06d.m4s"),
            _ => "live%06d.ts".to_string(),
        },
        init_template: match request.output_format {
            OutputFormat::HlsMp4 => Some(format!("{hls_generated_at}_init.mp4")),
            _ => None,
        },
        playlist_path: "live.m3u8".to_string(),
        segment_options: hls_segment_options(
            request.output_format,
            request.output_ts_offset,
            pipeline_request.desired.audio_format,
        ),
        is_first_transcode: request.is_first_transcode,
        troubleshooting: false,
    });

    build_pipeline_for_acceleration(
        request.playback_profile.profile.hardware_acceleration,
        pipeline_request.clone(),
    )
    .command(&pipeline_request.inputs)
}

fn generated_audio_sample_rate(playback_profile: &PlaybackProfile) -> u32 {
    playback_profile
        .profile
        .audio_sample_rate
        .try_into()
        .ok()
        .filter(|sample_rate: &u32| *sample_rate > 0)
        .map_or(48_000, |sample_rate| sample_rate.saturating_mul(1_000))
}

fn generated_slug_video_stream(frame_size: FrameSize, frame_rate: FrameRate) -> SourceVideoStream {
    let mut stream = SourceVideoStream::new(SourceVideoCodec::Unknown);
    stream.frame_size = Some(frame_size);
    stream.frame_rate = Some(frame_rate);
    stream.sample_aspect_ratio = Some("1:1".to_owned());
    stream.scan_kind = VideoScanKind::Progressive;
    stream
}

fn generated_error_video_stream(frame_size: FrameSize, frame_rate: FrameRate) -> SourceVideoStream {
    let mut stream = generated_slug_video_stream(frame_size, frame_rate);
    stream.is_still_image = true;
    stream
}

fn generated_slug_audio_stream(sample_rate: u32) -> SourceAudioStream {
    let mut stream = SourceAudioStream::new(SourceAudioCodec::Aac).with_index(0);
    stream.channels = Some(2);
    stream.sample_rate = Some(sample_rate);
    stream
}

fn hls_error_ass(error_message: &str, frame_size: FrameSize) -> String {
    let font_size = (frame_size.height / 20).max(1);
    let margin = frame_size
        .height
        .saturating_add(10)
        .saturating_div(20)
        .max(1);
    let escaped = escape_ass_dialogue_text(error_message);

    format!(
        "[Script Info]\n\
ScriptType: v4.00+\n\
PlayResX: {}\n\
PlayResY: {}\n\
\n\
[V4+ Styles]\n\
Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\n\
Style: Default,Roboto,{font_size},&H00FFFFFF,&H00FFFFFF,&H00000000,&H80000000,0,0,0,0,100,100,0,0,1,2,0,2,{margin},{margin},{margin},1\n\
\n\
[Events]\n\
Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n\
Dialogue: 0,0:00:00.00,9:59:59.00,Default,,0,0,0,,{escaped}\n",
        frame_size.width, frame_size.height
    )
}

fn escape_ass_dialogue_text(value: &str) -> String {
    value
        .replace('\r', "")
        .replace('\\', r"\\")
        .replace('\n', r"\N")
        .replace('{', "(")
        .replace('}', ")")
}

fn hls_segment_options(
    output_format: OutputFormat,
    output_ts_offset: Option<Duration>,
    audio_format: FFmpegProfileAudioFormat,
) -> Option<String> {
    if output_format != OutputFormat::Hls {
        return None;
    }

    let mut mpegts_flags = Vec::new();
    if output_ts_offset.is_none_or(|offset| offset.is_zero()) {
        mpegts_flags.push("+initial_discontinuity");
    }
    if audio_format == FFmpegProfileAudioFormat::AacLatm {
        mpegts_flags.push("+latm");
    }

    (!mpegts_flags.is_empty()).then(|| format!("mpegts_flags={}", mpegts_flags.join("")))
}

#[allow(clippy::match_same_arms)]
fn next_hls_session_state(
    state: HlsSessionState,
    item_complete: bool,
    has_slug: bool,
) -> HlsSessionState {
    match state {
        HlsSessionState::PlayoutUpdated => HlsSessionState::SeekAndWorkAhead,
        HlsSessionState::SeekAndWorkAhead if !item_complete => HlsSessionState::SeekAndRealtime,
        HlsSessionState::SlugAndWorkAhead => HlsSessionState::ZeroAndWorkAhead,
        HlsSessionState::SlugAndRealtime => HlsSessionState::ZeroAndRealtime,
        HlsSessionState::ZeroAndWorkAhead | HlsSessionState::SeekAndWorkAhead
            if item_complete && has_slug =>
        {
            HlsSessionState::SlugAndWorkAhead
        }
        HlsSessionState::ZeroAndRealtime | HlsSessionState::SeekAndRealtime
            if item_complete && has_slug =>
        {
            HlsSessionState::SlugAndRealtime
        }
        HlsSessionState::SeekAndWorkAhead => HlsSessionState::ZeroAndWorkAhead,
        HlsSessionState::ZeroAndWorkAhead if !item_complete => HlsSessionState::SeekAndRealtime,
        HlsSessionState::ZeroAndWorkAhead => HlsSessionState::ZeroAndWorkAhead,
        HlsSessionState::SeekAndRealtime => HlsSessionState::ZeroAndRealtime,
        HlsSessionState::ZeroAndRealtime => HlsSessionState::ZeroAndRealtime,
    }
}

fn should_transcode_for_buffer(transcoded_until: DateTime<Utc>, now: DateTime<Utc>) -> bool {
    transcoded_buffer(transcoded_until, now).num_seconds() <= HLS_WORK_AHEAD_BUFFER_SECONDS
}

fn should_use_realtime(transcoded_until: DateTime<Utc>, now: DateTime<Utc>) -> bool {
    transcoded_buffer(transcoded_until, now).num_seconds() >= HLS_REALTIME_BUFFER_SECONDS
}

fn transcoded_buffer(transcoded_until: DateTime<Utc>, now: DateTime<Utc>) -> chrono::Duration {
    (transcoded_until - now).max(chrono::Duration::zero())
}

fn hls_transcode_decision(
    state: HlsSessionState,
    transcoded_until: DateTime<Utc>,
    wall_clock_now: DateTime<Utc>,
    realtime: bool,
) -> HlsTranscodeDecision {
    let uses_wall_clock_request_time = matches!(
        state,
        HlsSessionState::SeekAndWorkAhead | HlsSessionState::PlayoutUpdated
    );
    let state = if realtime {
        state.throttle()
    } else {
        state.accelerate()
    };
    let request_time = if uses_wall_clock_request_time {
        wall_clock_now
    } else {
        transcoded_until
    };

    HlsTranscodeDecision {
        state,
        realtime,
        working_ahead: !realtime,
        request_time,
        start_at_zero: state.start_at_zero(),
    }
}

fn remaining_item_duration(
    active: &ActiveHlsItem,
    request_time: DateTime<Utc>,
    start_at_zero: bool,
) -> Option<Duration> {
    let finish = active.playout_finish?;
    let effective_start = if start_at_zero {
        active.playout_start
    } else {
        request_time.max(active.playout_start)
    };
    (finish - effective_start).to_std().ok()
}

fn hls_playout_item_duration(
    active: &ActiveHlsItem,
    request_time: DateTime<Utc>,
    start_at_zero: bool,
    hls_realtime: bool,
) -> Option<Duration> {
    let duration = remaining_item_duration(active, request_time, start_at_zero)?;
    if hls_realtime || duration <= Duration::from_secs(HLS_WORK_AHEAD_TRANSCODE_SECONDS) {
        Some(duration)
    } else {
        Some(Duration::from_secs(HLS_WORK_AHEAD_TRANSCODE_SECONDS))
    }
}

fn add_std_duration(timestamp: DateTime<Utc>, duration: Duration) -> DateTime<Utc> {
    timestamp + chrono::Duration::from_std(duration).unwrap_or_else(|_| chrono::Duration::zero())
}

fn item_complete(active: &ActiveHlsItem, transcoded_until: DateTime<Utc>) -> bool {
    active
        .playout_finish
        .is_some_and(|finish| transcoded_until >= finish)
}

struct WorkAheadReservation;

impl Drop for WorkAheadReservation {
    fn drop(&mut self) {
        release_work_ahead();
    }
}

fn try_reserve_work_ahead(limit: i32) -> Option<WorkAheadReservation> {
    if limit <= 0 {
        return None;
    }

    let mut current = HLS_WORK_AHEAD_COUNT.load(Ordering::Relaxed);
    loop {
        if current >= limit {
            return None;
        }
        match HLS_WORK_AHEAD_COUNT.compare_exchange_weak(
            current,
            current + 1,
            Ordering::AcqRel,
            Ordering::Relaxed,
        ) {
            Ok(_) => return Some(WorkAheadReservation),
            Err(next) => current = next,
        }
    }
}

fn release_work_ahead() {
    HLS_WORK_AHEAD_COUNT
        .fetch_update(Ordering::AcqRel, Ordering::Relaxed, |current| {
            (current > 0).then_some(current - 1)
        })
        .ok();
}

async fn load_hls_work_ahead_limit(pool: &DbPool) -> i32 {
    sqlx::query_scalar::<_, String>(r#"SELECT "Value" FROM "ConfigElement" WHERE "Key" = ?"#)
        .bind(ersatzrs_domain::CONFIG_KEY_FFMPEG_WORK_AHEAD_SEGMENTERS)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .and_then(|value| value.trim().parse::<i32>().ok())
        .unwrap_or(DEFAULT_WORK_AHEAD_SEGMENTER_LIMIT)
        .max(0)
}

async fn load_channel_slug_seconds(pool: &DbPool, channel_id: i64) -> Option<f64> {
    sqlx::query_scalar::<_, Option<f64>>(r#"SELECT "SlugSeconds" FROM "Channel" WHERE "Id" = ?"#)
        .bind(channel_id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .flatten()
}

async fn load_channel_name(pool: &DbPool, channel_id: i64) -> Option<String> {
    sqlx::query_scalar::<_, String>(r#"SELECT "Name" FROM "Channel" WHERE "Id" = ?"#)
        .bind(channel_id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
}

async fn time_shift_on_demand_playout(
    pool: &DbPool,
    channel_id: i64,
    now: DateTime<Utc>,
) -> Result<(), sqlx::Error> {
    let Some(playout) = sqlx::query(
        r#"SELECT p."Id" AS "PlayoutId",
                  p."OnDemandCheckpoint" AS "OnDemandCheckpoint"
           FROM "Playout" p
           JOIN "Channel" c ON c."Id" = p."ChannelId"
           WHERE p."ChannelId" = ?
             AND c."PlayoutMode" = 1
           LIMIT 1"#,
    )
    .bind(channel_id)
    .fetch_optional(pool)
    .await?
    else {
        return Ok(());
    };

    let playout_id = playout.get::<i64, _>("PlayoutId");
    let item_window = sqlx::query(
        r#"SELECT COUNT(*) AS "ItemCount",
                  MIN("Start") AS "FirstStart"
           FROM "PlayoutItem"
           WHERE "PlayoutId" = ?"#,
    )
    .bind(playout_id)
    .fetch_one(pool)
    .await?;
    if item_window.get::<i64, _>("ItemCount") == 0 {
        return Ok(());
    }

    let checkpoint = playout
        .try_get::<Option<String>, _>("OnDemandCheckpoint")?
        .as_deref()
        .map(|value| parse_required_timestamp("playout OnDemandCheckpoint", value))
        .transpose()?
        .or_else(|| {
            item_window
                .try_get::<Option<String>, _>("FirstStart")
                .ok()
                .flatten()
                .and_then(|value| parse_sqlite_utc_timestamp(&value))
        })
        .unwrap_or(now);
    let offset = now - checkpoint;

    let mut tx = pool.begin().await?;
    shift_playout_items(&mut tx, playout_id, offset).await?;
    shift_playout_history(&mut tx, playout_id, offset).await?;
    shift_playout_program_schedule_anchors(&mut tx, playout_id, offset).await?;
    shift_playout_anchor(&mut tx, playout_id, offset).await?;
    sqlx::query(r#"UPDATE "Playout" SET "OnDemandCheckpoint" = ? WHERE "Id" = ?"#)
        .bind(encode_datetime(now))
        .bind(playout_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

async fn update_on_demand_checkpoint(
    pool: &DbPool,
    channel_id: i64,
    now: DateTime<Utc>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"UPDATE "Playout"
           SET "OnDemandCheckpoint" = ?
           WHERE "ChannelId" = ?
             AND EXISTS (
                 SELECT 1
                 FROM "Channel" c
                 WHERE c."Id" = "Playout"."ChannelId"
                   AND c."PlayoutMode" = 1
             )"#,
    )
    .bind(encode_datetime(now))
    .bind(channel_id)
    .execute(pool)
    .await?;
    Ok(())
}

async fn shift_playout_items(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    playout_id: i64,
    offset: chrono::Duration,
) -> Result<(), sqlx::Error> {
    let rows = sqlx::query(
        r#"SELECT "Id", "Start", "Finish", "GuideStart", "GuideFinish"
           FROM "PlayoutItem"
           WHERE "PlayoutId" = ?"#,
    )
    .bind(playout_id)
    .fetch_all(&mut **tx)
    .await?;

    for row in rows {
        sqlx::query(
            r#"UPDATE "PlayoutItem"
               SET "Start" = ?, "Finish" = ?, "GuideStart" = ?, "GuideFinish" = ?
               WHERE "Id" = ?"#,
        )
        .bind(shift_required_datetime(
            "playout item Start",
            &row.try_get::<String, _>("Start")?,
            offset,
        )?)
        .bind(shift_required_datetime(
            "playout item Finish",
            &row.try_get::<String, _>("Finish")?,
            offset,
        )?)
        .bind(shift_optional_datetime(
            "playout item GuideStart",
            row.try_get("GuideStart")?,
            offset,
        )?)
        .bind(shift_optional_datetime(
            "playout item GuideFinish",
            row.try_get("GuideFinish")?,
            offset,
        )?)
        .bind(row.get::<i64, _>("Id"))
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

async fn shift_playout_history(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    playout_id: i64,
    offset: chrono::Duration,
) -> Result<(), sqlx::Error> {
    let rows = sqlx::query(
        r#"SELECT "Id", "When", "Finish"
           FROM "PlayoutHistory"
           WHERE "PlayoutId" = ?"#,
    )
    .bind(playout_id)
    .fetch_all(&mut **tx)
    .await?;

    for row in rows {
        sqlx::query(
            r#"UPDATE "PlayoutHistory"
               SET "When" = ?, "Finish" = ?
               WHERE "Id" = ?"#,
        )
        .bind(shift_required_datetime(
            "playout history When",
            &row.try_get::<String, _>("When")?,
            offset,
        )?)
        .bind(shift_required_datetime(
            "playout history Finish",
            &row.try_get::<String, _>("Finish")?,
            offset,
        )?)
        .bind(row.get::<i64, _>("Id"))
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

async fn shift_playout_program_schedule_anchors(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    playout_id: i64,
    offset: chrono::Duration,
) -> Result<(), sqlx::Error> {
    let rows = sqlx::query(
        r#"SELECT "Id", "AnchorDate"
           FROM "PlayoutProgramScheduleAnchor"
           WHERE "PlayoutId" = ?"#,
    )
    .bind(playout_id)
    .fetch_all(&mut **tx)
    .await?;

    for row in rows {
        sqlx::query(
            r#"UPDATE "PlayoutProgramScheduleAnchor"
               SET "AnchorDate" = ?
               WHERE "Id" = ?"#,
        )
        .bind(shift_optional_datetime(
            "playout program schedule anchor AnchorDate",
            row.try_get("AnchorDate")?,
            offset,
        )?)
        .bind(row.get::<i64, _>("Id"))
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

async fn shift_playout_anchor(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    playout_id: i64,
    offset: chrono::Duration,
) -> Result<(), sqlx::Error> {
    let Some(row) = sqlx::query(
        r#"SELECT "NextStart", "DurationFinish"
           FROM "PlayoutAnchor"
           WHERE "PlayoutId" = ?"#,
    )
    .bind(playout_id)
    .fetch_optional(&mut **tx)
    .await?
    else {
        return Ok(());
    };

    sqlx::query(
        r#"UPDATE "PlayoutAnchor"
           SET "NextStart" = ?, "DurationFinish" = ?
           WHERE "PlayoutId" = ?"#,
    )
    .bind(shift_required_datetime(
        "playout anchor NextStart",
        &row.try_get::<String, _>("NextStart")?,
        offset,
    )?)
    .bind(shift_optional_datetime(
        "playout anchor DurationFinish",
        row.try_get("DurationFinish")?,
        offset,
    )?)
    .bind(playout_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

fn shift_required_datetime(
    kind: &'static str,
    value: &str,
    offset: chrono::Duration,
) -> Result<String, sqlx::Error> {
    Ok(encode_datetime(
        parse_required_timestamp(kind, value)? + offset,
    ))
}

fn shift_optional_datetime(
    kind: &'static str,
    value: Option<String>,
    offset: chrono::Duration,
) -> Result<Option<String>, sqlx::Error> {
    value
        .filter(|value| !value.trim().is_empty())
        .map(|value| shift_required_datetime(kind, &value, offset))
        .transpose()
}

fn parse_required_timestamp(kind: &'static str, value: &str) -> Result<DateTime<Utc>, sqlx::Error> {
    parse_sqlite_utc_timestamp(value)
        .ok_or_else(|| sqlx::Error::Decode(format!("invalid {kind}: {value}").into()))
}

async fn probe_last_pts_offset(
    working_dir: &Path,
    init_cache: Option<&HlsInitSegmentCache>,
) -> io::Result<Option<Duration>> {
    let Some(segment) = last_hls_segment(working_dir).await? else {
        return Ok(None);
    };
    let mut temporary_probe_segment = None;
    let segment_path =
        if segment.extension().and_then(|extension| extension.to_str()) == Some("m4s") {
            let Some(cache) = init_cache else {
                return Ok(None);
            };
            let Some(generated_at) = generated_at_from_fmp4_segment(&segment) else {
                return Ok(None);
            };
            let Some(init_file) = cache.earliest_segment_by_generated_at(generated_at) else {
                return Ok(None);
            };
            temporary_probe_segment =
                Some(concatenate_fmp4_init_and_segment(working_dir, &init_file, &segment).await?);
            temporary_probe_segment
                .as_deref()
                .unwrap_or(segment.as_path())
        } else {
            segment.as_path()
        };

    let output_result = Command::new(ffprobe_path())
        .args([
            "-v",
            "0",
            "-show_entries",
            "packet=pts_time,duration_time",
            "-of",
            "compact=p=0:nk=1",
        ])
        .arg(segment_path)
        .output()
        .await;
    if let Some(path) = temporary_probe_segment {
        let _ = tokio::fs::remove_file(path).await;
    }
    let output = output_result?;

    if !output.status.success() {
        return Ok(None);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let max_pts = stdout
        .lines()
        .filter_map(parse_pts_line)
        .max_by(f64::total_cmp);
    Ok(max_pts.and_then(|seconds| {
        seconds
            .is_finite()
            .then(|| Duration::from_secs_f64(seconds.max(0.0)))
    }))
}

async fn last_hls_segment(working_dir: &Path) -> io::Result<Option<PathBuf>> {
    let mut entries = tokio::fs::read_dir(working_dir).await?;
    let mut segments = Vec::new();
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if matches!(
            path.extension().and_then(|extension| extension.to_str()),
            Some("ts" | "m4s")
        ) {
            segments.push(path);
        }
    }
    segments.sort_by(|left, right| left.file_name().cmp(&right.file_name()));
    Ok(segments.pop())
}

async fn concatenate_fmp4_init_and_segment(
    working_dir: &Path,
    init_file: &str,
    segment: &Path,
) -> io::Result<PathBuf> {
    let combined = working_dir.join(format!(
        ".last-{}.mp4",
        Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    let mut data = tokio::fs::read(working_dir.join(init_file)).await?;
    data.extend(tokio::fs::read(segment).await?);
    tokio::fs::write(&combined, data).await?;
    Ok(combined)
}

#[derive(Default, Debug)]
struct HlsInitSegmentCache {
    known_segments: HashMap<String, String>,
    earliest_segments_by_hash: HashMap<String, String>,
}

impl HlsInitSegmentCache {
    async fn add_segment(&mut self, segment: &Path) -> io::Result<()> {
        let Some(file_name) = segment
            .file_name()
            .and_then(|file_name| file_name.to_str())
            .map(str::to_owned)
        else {
            return Ok(());
        };
        if self.known_segments.contains_key(&file_name) {
            return Ok(());
        }

        let bytes = tokio::fs::read(segment).await?;
        let hash = hex_hash(&bytes);
        self.known_segments.insert(file_name.clone(), hash.clone());
        self.earliest_segments_by_hash
            .entry(hash)
            .or_insert(file_name);
        Ok(())
    }

    fn earliest_segment_by_generated_at(&self, generated_at: i64) -> Option<String> {
        let file_name = format!("{generated_at}_init.mp4");
        let hash = self.known_segments.get(&file_name)?;
        self.earliest_segments_by_hash.get(hash).cloned()
    }

    fn is_earliest_by_hash(&self, file_name: &str) -> bool {
        self.known_segments
            .get(file_name)
            .and_then(|hash| self.earliest_segments_by_hash.get(hash))
            .is_some_and(|earliest| earliest == file_name)
    }

    fn delete_segment(&mut self, file_name: &str) {
        self.known_segments.remove(file_name);
    }
}

fn hex_hash(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().fold(String::new(), |mut output, byte| {
        let _ = write!(output, "{byte:02x}");
        output
    })
}

async fn refresh_init_segments(
    working_dir: &Path,
    init_cache: &mut HlsInitSegmentCache,
) -> io::Result<()> {
    let mut generated_ats = HashSet::new();
    let mut entries = tokio::fs::read_dir(working_dir).await?;
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if path.extension().and_then(|extension| extension.to_str()) == Some("m4s") {
            if let Some(generated_at) = generated_at_from_fmp4_segment(&path) {
                generated_ats.insert(generated_at);
            }
        }
    }

    let mut entries = tokio::fs::read_dir(working_dir).await?;
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        let Some(file_name) = path.file_name().and_then(|file_name| file_name.to_str()) else {
            continue;
        };
        if !file_name.ends_with("_init.mp4") {
            continue;
        }
        if generated_at_from_init_file(file_name)
            .is_some_and(|generated_at| generated_ats.contains(&generated_at))
        {
            init_cache.add_segment(&path).await?;
        }
    }

    Ok(())
}

fn generated_at_from_init_file(file_name: &str) -> Option<i64> {
    file_name.strip_suffix("_init.mp4")?.parse().ok()
}

fn generated_at_from_fmp4_segment(path: &Path) -> Option<i64> {
    let stem = path.file_stem()?.to_str()?;
    stem.strip_prefix("live_")?.split('_').next()?.parse().ok()
}

fn parse_pts_line(line: &str) -> Option<f64> {
    let mut parts = line.trim().split('|');
    let pts_time = parts.next()?.parse::<f64>().ok()?;
    let duration_time = parts
        .next()
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or_default();
    Some(pts_time + duration_time)
}

fn playback_output_metadata(
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

#[allow(clippy::too_many_lines)]
async fn load_active_hls_item(
    pool: &DbPool,
    channel_id: i64,
    path_replacements: &[PathReplacement],
    now: DateTime<Utc>,
) -> Option<ActiveHlsItem> {
    let now_text = now.format("%Y-%m-%d %H:%M:%S").to_string();
    let row = sqlx::query(
        r#"SELECT mf."Path" AS "MediaPath",
                  pi."Id" AS "PlayoutItemId",
                  pi."MediaItemId" AS "MediaItemId",
                  mv."Id" AS "MediaVersionId",
                  mv."Duration" AS "ContentDuration",
                  pi."InPoint" AS "InPoint",
                  pi."Start" AS "PlayoutStart",
                  pi."Finish" AS "PlayoutFinish",
                  pi."DisableWatermarks" AS "DisableWatermarks",
                  pi."PreferredAudioLanguageCode" AS "PreferredAudioLanguageCode",
                  pi."PreferredSubtitleLanguageCode" AS "PreferredSubtitleLanguageCode",
                  pi."SubtitleMode" AS "SubtitleMode",
                  c."Name" AS "ChannelName",
                  c."WatermarkId" AS "ChannelWatermarkId",
                  c."PreferredAudioLanguageCode" AS "ChannelPreferredAudioLanguageCode",
                  c."PreferredSubtitleLanguageCode" AS "ChannelPreferredSubtitleLanguageCode",
                  c."SubtitleMode" AS "ChannelSubtitleMode",
                  c."MusicVideoCreditsMode" AS "ChannelMusicVideoCreditsMode",
                  c."MusicVideoCreditsTemplate" AS "ChannelMusicVideoCreditsTemplate"
           FROM "PlayoutItem" pi
           JOIN "Playout" p ON p."Id" = pi."PlayoutId"
           JOIN "Channel" c ON c."Id" = p."ChannelId"
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
    .await
    .ok()??;

    let media_path = row.get::<String, _>("MediaPath");
    let resolved = if path_replacements.is_empty() {
        media_path
    } else {
        replace_path(path_replacements, &media_path, PathPlatform::Unix)
    };
    let playout_start = parse_sqlite_utc_timestamp(&row.get::<String, _>("PlayoutStart"))?;
    let program_duration = row
        .try_get::<String, _>("PlayoutFinish")
        .ok()
        .and_then(|finish| parse_sqlite_utc_timestamp(&finish))
        .and_then(|finish| (finish - playout_start).to_std().ok());
    let playout_finish = row
        .try_get::<String, _>("PlayoutFinish")
        .ok()
        .and_then(|finish| parse_sqlite_utc_timestamp(&finish));
    Some(ActiveHlsItem {
        media_path: resolved,
        playout_item_id: row.get::<i64, _>("PlayoutItemId"),
        media_item_id: row.get::<i64, _>("MediaItemId"),
        media_version_id: row.get::<i64, _>("MediaVersionId"),
        content_duration: row
            .try_get::<Option<f64>, _>("ContentDuration")
            .ok()
            .flatten()
            .and_then(duration_from_f64),
        in_point: row
            .try_get::<String, _>("InPoint")
            .ok()
            .map_or(Duration::ZERO, |value| {
                decode_duration_text(&value).to_std().unwrap_or_default()
            }),
        program_duration,
        playout_start,
        playout_finish,
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
        channel_name: row
            .try_get::<String, _>("ChannelName")
            .unwrap_or_else(|_| String::new()),
        channel_preferred_audio_language_code: row
            .try_get::<Option<String>, _>("ChannelPreferredAudioLanguageCode")
            .ok()
            .flatten(),
        channel_preferred_subtitle_language_code: row
            .try_get::<Option<String>, _>("ChannelPreferredSubtitleLanguageCode")
            .ok()
            .flatten(),
        channel_subtitle_mode: row
            .try_get::<i64, _>("ChannelSubtitleMode")
            .ok()
            .and_then(decode_subtitle_mode)
            .unwrap_or(ChannelSubtitleMode::None),
        channel_watermark_id: row
            .try_get::<Option<i64>, _>("ChannelWatermarkId")
            .ok()
            .flatten(),
        channel_music_video_credits_mode: row
            .try_get::<i64, _>("ChannelMusicVideoCreditsMode")
            .ok()
            .and_then(decode_music_video_credits_mode)
            .unwrap_or(ChannelMusicVideoCreditsMode::None),
        channel_music_video_credits_template: row
            .try_get::<Option<String>, _>("ChannelMusicVideoCreditsTemplate")
            .ok()
            .flatten(),
    })
}

#[derive(Debug, Clone)]
struct ActiveHlsItem {
    media_path: String,
    playout_item_id: i64,
    media_item_id: i64,
    media_version_id: i64,
    content_duration: Option<Duration>,
    in_point: Duration,
    program_duration: Option<Duration>,
    playout_start: DateTime<Utc>,
    playout_finish: Option<DateTime<Utc>>,
    disable_watermarks: bool,
    preferred_audio_language_code: Option<String>,
    preferred_subtitle_language_code: Option<String>,
    subtitle_mode: Option<ChannelSubtitleMode>,
    channel_name: String,
    channel_preferred_audio_language_code: Option<String>,
    channel_preferred_subtitle_language_code: Option<String>,
    channel_subtitle_mode: ChannelSubtitleMode,
    channel_watermark_id: Option<i64>,
    channel_music_video_credits_mode: ChannelMusicVideoCreditsMode,
    channel_music_video_credits_template: Option<String>,
}

impl ActiveHlsItem {
    fn channel_for_runtime_subtitles(&self, id: i64, number: String) -> Channel {
        Channel {
            id,
            number,
            name: self.channel_name.clone(),
            preferred_subtitle_language_code: self.channel_preferred_subtitle_language_code.clone(),
            subtitle_mode: self.channel_subtitle_mode,
            music_video_credits_mode: self.channel_music_video_credits_mode,
            music_video_credits_template: self.channel_music_video_credits_template.clone(),
            ..Channel::default()
        }
    }

    fn content_seek_seconds(&self, elapsed_seconds: i64) -> i64 {
        let elapsed = u64::try_from(elapsed_seconds).unwrap_or(0);
        self.in_point
            .as_secs()
            .saturating_add(elapsed)
            .min(i64::MAX as u64) as i64
    }
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

const fn decode_music_video_credits_mode(value: i64) -> Option<ChannelMusicVideoCreditsMode> {
    match value {
        0 => Some(ChannelMusicVideoCreditsMode::None),
        1 => Some(ChannelMusicVideoCreditsMode::GenerateSubtitles),
        _ => None,
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

fn playout_start_seconds(now: DateTime<Utc>, playout_start: DateTime<Utc>) -> i64 {
    (now - playout_start).num_seconds().max(0)
}

fn seconds_as_f64(seconds: i64) -> f64 {
    u64::try_from(seconds)
        .map(Duration::from_secs)
        .map_or(0.0, |duration| duration.as_secs_f64())
}

fn duration_from_seconds(seconds: i64) -> Duration {
    u64::try_from(seconds).map_or(Duration::ZERO, Duration::from_secs)
}

fn duration_from_f64(seconds: f64) -> Option<Duration> {
    seconds
        .is_finite()
        .then_some(seconds)
        .filter(|seconds| *seconds >= 0.0)
        .map(Duration::from_secs_f64)
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

fn playback_frame_size(playback_profile: &PlaybackProfile) -> FrameSize {
    FrameSize::new(
        u32::try_from(playback_profile.resolution.width)
            .ok()
            .filter(|width| *width > 0)
            .unwrap_or(1920),
        u32::try_from(playback_profile.resolution.height)
            .ok()
            .filter(|height| *height > 0)
            .unwrap_or(1080),
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PlaylistItem {
    Discontinuity,
    Segment(PlaylistSegment),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PlaylistSegment {
    extinf_line: String,
    program_date_time_line: Option<String>,
    program_date_time: Option<DateTime<FixedOffset>>,
    uri: String,
    map_uri: Option<String>,
    generated_at: Option<i64>,
    sequence: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedPlaylist {
    version: String,
    target_duration: u32,
    items: Vec<PlaylistItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TrimmedPlaylist {
    playlist: String,
    referenced_files: HashSet<String>,
    sequence: u64,
    generated_at: Option<i64>,
}

fn trim_playlist_contents(
    output_format: OutputFormat,
    playlist: &str,
    filter_before: DateTime<Utc>,
    maybe_max_segments: Option<usize>,
    init_cache: Option<&HlsInitSegmentCache>,
) -> Option<TrimmedPlaylist> {
    let parsed = parse_playlist(playlist)?;
    let all_segments: Vec<PlaylistSegment> = parsed
        .items
        .iter()
        .filter_map(|item| match item {
            PlaylistItem::Segment(segment) => Some(segment.clone()),
            PlaylistItem::Discontinuity => None,
        })
        .collect();
    if all_segments.is_empty() {
        return None;
    }

    let retained_segments =
        select_retained_segments(&all_segments, filter_before, maybe_max_segments);
    render_trimmed_playlist(output_format, &parsed, &retained_segments, init_cache)
}

fn select_retained_segments(
    all_segments: &[PlaylistSegment],
    filter_before: DateTime<Utc>,
    maybe_max_segments: Option<usize>,
) -> Vec<PlaylistSegment> {
    if let Some(max_segments) = maybe_max_segments {
        if all_segments.len() > max_segments {
            let after_filter: Vec<PlaylistSegment> = all_segments
                .iter()
                .filter(|segment| segment_is_after_filter(segment, filter_before))
                .cloned()
                .collect();

            if after_filter.len() >= max_segments {
                return after_filter.into_iter().take(max_segments).collect();
            }

            let keep_from = all_segments.len().saturating_sub(max_segments);
            return all_segments[keep_from..].to_vec();
        }

        return all_segments.to_vec();
    }

    all_segments
        .iter()
        .filter(|segment| segment_is_after_filter(segment, filter_before))
        .cloned()
        .collect()
}

fn segment_is_after_filter(segment: &PlaylistSegment, filter_before: DateTime<Utc>) -> bool {
    segment
        .program_date_time
        .is_some_and(|pdt| pdt.with_timezone(&Utc) >= filter_before)
}

fn render_trimmed_playlist(
    output_format: OutputFormat,
    parsed: &ParsedPlaylist,
    retained_segments: &[PlaylistSegment],
    init_cache: Option<&HlsInitSegmentCache>,
) -> Option<TrimmedPlaylist> {
    let first_segment = retained_segments.first()?;
    let last_segment = retained_segments.last()?;
    let retained_uris: HashSet<&str> = retained_segments
        .iter()
        .map(|segment| segment.uri.as_str())
        .collect();
    let first_index = parsed.items.iter().position(
        |item| matches!(item, PlaylistItem::Segment(segment) if segment.uri == first_segment.uri),
    )?;
    let last_index = parsed.items.iter().rposition(
        |item| matches!(item, PlaylistItem::Segment(segment) if segment.uri == last_segment.uri),
    )?;
    let leading_discontinuities = parsed.items[..first_index]
        .iter()
        .filter(|item| matches!(item, PlaylistItem::Discontinuity))
        .count();
    let generated_at = retained_segments
        .iter()
        .filter_map(|segment| segment.generated_at)
        .min();

    let mut out = String::new();
    out.push_str("#EXTM3U\n");
    let _ = writeln!(out, "#EXT-X-VERSION:{}", parsed.version);
    let _ = writeln!(out, "#EXT-X-TARGETDURATION:{}", parsed.target_duration);
    let _ = writeln!(out, "#EXT-X-MEDIA-SEQUENCE:{}", first_segment.sequence);
    if leading_discontinuities > 1 {
        let _ = writeln!(
            out,
            "#EXT-X-DISCONTINUITY-SEQUENCE:{leading_discontinuities}"
        );
    }
    out.push_str("#EXT-X-INDEPENDENT-SEGMENTS\n");
    if leading_discontinuities == 1 {
        out.push_str("#EXT-X-DISCONTINUITY\n");
    }

    let mut referenced_files = HashSet::new();
    if output_format == OutputFormat::HlsMp4 {
        if let Some(map_uri) = segment_map_uri(first_segment, init_cache) {
            let _ = writeln!(out, "#EXT-X-MAP:URI=\"{map_uri}\"");
            referenced_files.insert(map_uri.clone());
        }
    }

    for (index, item) in parsed.items[first_index..=last_index].iter().enumerate() {
        match item {
            PlaylistItem::Discontinuity => {
                let next_retained = parsed.items[first_index + index + 1..=last_index]
                    .iter()
                    .find_map(|next_item| match next_item {
                        PlaylistItem::Segment(segment)
                            if retained_uris.contains(segment.uri.as_str()) =>
                        {
                            Some(segment)
                        }
                        PlaylistItem::Segment(_) | PlaylistItem::Discontinuity => None,
                    });
                if let Some(next_segment) = next_retained {
                    if next_segment.uri != first_segment.uri {
                        out.push_str("#EXT-X-DISCONTINUITY\n");
                        if output_format == OutputFormat::HlsMp4 {
                            if let Some(map_uri) = segment_map_uri(next_segment, init_cache) {
                                let _ = writeln!(out, "#EXT-X-MAP:URI=\"{map_uri}\"");
                                referenced_files.insert(map_uri.clone());
                            }
                        }
                    }
                }
            }
            PlaylistItem::Segment(segment) if retained_uris.contains(segment.uri.as_str()) => {
                out.push_str(&segment.extinf_line);
                out.push('\n');
                if let Some(program_date_time_line) = &segment.program_date_time_line {
                    out.push_str(program_date_time_line);
                    out.push('\n');
                }
                out.push_str(&segment.uri);
                out.push('\n');
                referenced_files.insert(segment.uri.clone());
                if let Some(map_uri) = segment_map_uri(segment, init_cache) {
                    referenced_files.insert(map_uri.clone());
                }
            }
            PlaylistItem::Segment(_) => {}
        }
    }

    Some(TrimmedPlaylist {
        playlist: out,
        referenced_files,
        sequence: first_segment.sequence,
        generated_at,
    })
}

fn segment_map_uri(
    segment: &PlaylistSegment,
    init_cache: Option<&HlsInitSegmentCache>,
) -> Option<String> {
    segment
        .generated_at
        .and_then(|generated_at| init_cache?.earliest_segment_by_generated_at(generated_at))
        .or_else(|| segment.map_uri.clone())
}

fn parse_playlist(playlist: &str) -> Option<ParsedPlaylist> {
    let lines: Vec<&str> = playlist.lines().collect();
    let mut version = "7".to_string();
    let mut target_duration = None;
    let mut current_map_uri = None::<String>;
    let mut items = Vec::new();
    let mut index = 0;

    while index < lines.len() {
        let line = lines[index].trim();
        if line.is_empty() {
            index += 1;
            continue;
        }

        if let Some(value) = line.strip_prefix("#EXT-X-VERSION:") {
            version = value.to_string();
            index += 1;
            continue;
        }
        if let Some(value) = line.strip_prefix("#EXT-X-TARGETDURATION:") {
            target_duration = value.parse::<u32>().ok();
            index += 1;
            continue;
        }
        if line.eq_ignore_ascii_case("#EXT-X-DISCONTINUITY") {
            items.push(PlaylistItem::Discontinuity);
            index += 1;
            continue;
        }
        if let Some(value) = line.strip_prefix("#EXT-X-MAP:URI=") {
            current_map_uri = Some(value.trim_matches('"').to_string());
            index += 1;
            continue;
        }
        if !line.starts_with("#EXTINF:") {
            index += 1;
            continue;
        }

        let extinf_line = line.to_string();
        index += 1;

        let mut program_date_time_line = None;
        let mut program_date_time = None;
        while index < lines.len() {
            let next_line = lines[index].trim();
            if next_line.is_empty() {
                index += 1;
                continue;
            }

            if let Some(value) = next_line.strip_prefix("#EXT-X-PROGRAM-DATE-TIME:") {
                program_date_time_line = Some(next_line.to_string());
                program_date_time = DateTime::parse_from_rfc3339(value).ok();
                index += 1;
                continue;
            }

            if let Some(value) = next_line.strip_prefix("#EXT-X-MAP:URI=") {
                current_map_uri = Some(value.trim_matches('"').to_string());
                index += 1;
                continue;
            }

            if next_line.starts_with('#') {
                index += 1;
                continue;
            }

            let uri = next_line.to_string();
            let sequence = parse_segment_sequence(&uri)?;
            let generated_at = generated_at_from_fmp4_segment(Path::new(&uri));
            items.push(PlaylistItem::Segment(PlaylistSegment {
                extinf_line,
                program_date_time_line,
                program_date_time,
                uri,
                map_uri: current_map_uri.clone(),
                generated_at,
                sequence,
            }));
            index += 1;
            break;
        }
    }

    Some(ParsedPlaylist {
        version,
        target_duration: target_duration?,
        items,
    })
}

fn parse_segment_sequence(uri: &str) -> Option<u64> {
    let stem = Path::new(uri).file_stem()?.to_str()?;
    let digits: String = stem
        .chars()
        .rev()
        .take_while(char::is_ascii_digit)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    if digits.is_empty() {
        None
    } else {
        digits.parse::<u64>().ok()
    }
}

async fn delete_old_segments(
    working_dir: &Path,
    trim_result: &TrimmedPlaylist,
    init_cache: &mut HlsInitSegmentCache,
) -> std::io::Result<()> {
    let mut fmp4_generations_on_disk = HashSet::new();
    let mut files_to_delete = Vec::new();
    let mut entries = tokio::fs::read_dir(working_dir).await?;
    while let Some(entry) = entries.next_entry().await? {
        if !entry.file_type().await?.is_file() {
            continue;
        }
        let path = entry.path();
        if path.extension().and_then(|extension| extension.to_str()) == Some("m4s") {
            if let Some(generated_at) = generated_at_from_fmp4_segment(&path) {
                fmp4_generations_on_disk.insert(generated_at);
            }
        }

        let file_name = entry.file_name().to_string_lossy().into_owned();
        let extension = Path::new(&file_name)
            .extension()
            .and_then(|ext| ext.to_str())
            .map(str::to_ascii_lowercase);
        if matches!(extension.as_deref(), Some("ts" | "m4s"))
            && parse_segment_sequence(&file_name)
                .is_some_and(|sequence| sequence < trim_result.sequence)
        {
            files_to_delete.push(path);
        }
    }

    let mut entries = tokio::fs::read_dir(working_dir).await?;
    while let Some(entry) = entries.next_entry().await? {
        if !entry.file_type().await?.is_file() {
            continue;
        }

        let file_name = entry.file_name().to_string_lossy().into_owned();
        let extension = Path::new(&file_name)
            .extension()
            .and_then(|ext| ext.to_str())
            .map(str::to_ascii_lowercase);
        let is_segment_file = matches!(extension.as_deref(), Some("ts" | "mp4" | "m4s"));
        if !is_segment_file || trim_result.referenced_files.contains(&file_name) {
            continue;
        }

        if file_name.ends_with("_init.mp4") {
            if generated_at_from_init_file(&file_name)
                .is_some_and(|generated_at| fmp4_generations_on_disk.contains(&generated_at))
            {
                continue;
            }
            if init_is_at_or_after_trim_generation(&file_name, trim_result.generated_at) {
                continue;
            }
            if init_cache.is_earliest_by_hash(&file_name) {
                continue;
            }
            init_cache.delete_segment(&file_name);
            files_to_delete.push(entry.path());
        } else if extension.as_deref() == Some("mp4") {
            files_to_delete.push(entry.path());
        }
    }

    for path in files_to_delete {
        let _ = tokio::fs::remove_file(path).await;
    }

    Ok(())
}

fn init_is_at_or_after_trim_generation(file_name: &str, trim_generated_at: Option<i64>) -> bool {
    trim_generated_at.is_some_and(|generated_at| {
        generated_at_from_init_file(file_name)
            .is_some_and(|init_generated_at| init_generated_at >= generated_at)
    })
}

async fn prepare_working_dir(working_dir: &Path) -> io::Result<()> {
    match tokio::fs::symlink_metadata(working_dir).await {
        Ok(metadata) if metadata.is_dir() => {
            tokio::fs::remove_dir_all(working_dir).await?;
        }
        Ok(_) => {
            tokio::fs::remove_file(working_dir).await?;
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }

    tokio::fs::create_dir_all(working_dir).await
}

async fn cleanup_working_dir(working_dir: &Path) -> io::Result<()> {
    match tokio::fs::remove_dir_all(working_dir).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ersatzrs_domain::{
        FFmpegProfile, FFmpegProfileAudioFormat, FFmpegProfileVideoFormat,
        HardwareAccelerationKind, Resolution,
    };
    use ersatzrs_ffmpeg::PixelFormat;
    use std::process::Stdio;
    use tokio::time::timeout;
    use uuid::Uuid;

    #[tokio::test]
    async fn cancel_signals_worker_run_loop() {
        let worker = HlsSessionWorker::new(
            "42".to_string(),
            1,
            std::env::temp_dir().join("ersatzrs-hls-test-cancel"),
            OutputFormat::Hls,
            Vec::new(),
            FfmpegCapabilityReport::default(),
        );

        // `cancel` is requested while nothing is awaiting; `notify_one` must
        // store a permit so the worker's select loop observes it on its next
        // `cancel.notified()` poll.
        worker.cancel().await;
        timeout(Duration::from_millis(250), worker.cancel.notified())
            .await
            .expect("worker run loop observes the cancellation signal");
    }

    #[test]
    fn playout_updated_sets_worker_state_for_next_iteration() {
        let worker = HlsSessionWorker::new(
            "42".to_string(),
            1,
            std::env::temp_dir().join("ersatzrs-hls-test-playout-updated"),
            OutputFormat::Hls,
            Vec::new(),
            FfmpegCapabilityReport::default(),
        );

        worker.set_session_state(HlsSessionState::ZeroAndRealtime);
        worker.playout_updated();

        assert_eq!(worker.session_state(), HlsSessionState::PlayoutUpdated);
    }

    #[test]
    fn hls_session_state_transitions_match_ersatztv_table_without_slug() {
        assert_eq!(
            next_hls_session_state(HlsSessionState::PlayoutUpdated, false, false),
            HlsSessionState::SeekAndWorkAhead
        );
        assert_eq!(
            next_hls_session_state(HlsSessionState::SeekAndWorkAhead, false, false),
            HlsSessionState::SeekAndRealtime
        );
        assert_eq!(
            next_hls_session_state(HlsSessionState::SeekAndWorkAhead, true, false),
            HlsSessionState::ZeroAndWorkAhead
        );
        assert_eq!(
            next_hls_session_state(HlsSessionState::ZeroAndWorkAhead, false, false),
            HlsSessionState::SeekAndRealtime
        );
        assert_eq!(
            next_hls_session_state(HlsSessionState::ZeroAndWorkAhead, true, false),
            HlsSessionState::ZeroAndWorkAhead
        );
        assert_eq!(
            next_hls_session_state(HlsSessionState::SeekAndRealtime, true, false),
            HlsSessionState::ZeroAndRealtime
        );
        assert_eq!(
            next_hls_session_state(HlsSessionState::ZeroAndRealtime, true, false),
            HlsSessionState::ZeroAndRealtime
        );
    }

    #[test]
    fn hls_session_state_transitions_insert_and_leave_slug_when_configured() {
        assert_eq!(
            next_hls_session_state(HlsSessionState::SeekAndWorkAhead, true, true),
            HlsSessionState::SlugAndWorkAhead
        );
        assert_eq!(
            next_hls_session_state(HlsSessionState::ZeroAndRealtime, true, true),
            HlsSessionState::SlugAndRealtime
        );
        assert_eq!(
            next_hls_session_state(HlsSessionState::SlugAndWorkAhead, true, true),
            HlsSessionState::ZeroAndWorkAhead
        );
        assert_eq!(
            next_hls_session_state(HlsSessionState::SlugAndRealtime, true, true),
            HlsSessionState::ZeroAndRealtime
        );
    }

    #[test]
    fn hls_transcode_decision_uses_buffer_state_and_request_time() {
        let now = DateTime::parse_from_rfc3339("2026-06-08T12:00:00Z")
            .expect("timestamp")
            .with_timezone(&Utc);
        let transcoded_until = now + chrono::Duration::seconds(20);
        let decision = hls_transcode_decision(
            HlsSessionState::SeekAndWorkAhead,
            transcoded_until,
            now,
            false,
        );

        assert_eq!(decision.state, HlsSessionState::SeekAndWorkAhead);
        assert!(!decision.realtime);
        assert!(decision.working_ahead);
        assert_eq!(decision.request_time, now);
        assert!(!decision.start_at_zero);

        let throttled = hls_transcode_decision(
            HlsSessionState::ZeroAndWorkAhead,
            transcoded_until,
            now,
            true,
        );
        assert_eq!(throttled.state, HlsSessionState::ZeroAndRealtime);
        assert!(throttled.realtime);
        assert!(!throttled.working_ahead);
        assert_eq!(throttled.request_time, transcoded_until);
        assert!(throttled.start_at_zero);
    }

    #[test]
    fn hls_transcode_decision_treats_playout_updated_as_live_seek() {
        let now = DateTime::parse_from_rfc3339("2026-06-08T12:00:00Z")
            .expect("timestamp")
            .with_timezone(&Utc);
        let stale_buffered_until = now + chrono::Duration::minutes(10);

        let working_ahead = hls_transcode_decision(
            HlsSessionState::PlayoutUpdated,
            stale_buffered_until,
            now,
            false,
        );
        assert_eq!(working_ahead.state, HlsSessionState::PlayoutUpdated);
        assert!(!working_ahead.realtime);
        assert!(working_ahead.working_ahead);
        assert_eq!(working_ahead.request_time, now);
        assert!(!working_ahead.start_at_zero);

        let realtime = hls_transcode_decision(
            HlsSessionState::PlayoutUpdated,
            stale_buffered_until,
            now,
            true,
        );
        assert_eq!(realtime.state, HlsSessionState::PlayoutUpdated);
        assert!(realtime.realtime);
        assert!(!realtime.working_ahead);
        assert_eq!(realtime.request_time, now);
        assert!(!realtime.start_at_zero);
    }

    #[test]
    fn hls_buffer_thresholds_match_ersatztv_worker_loop() {
        let now = DateTime::parse_from_rfc3339("2026-06-08T12:00:00Z")
            .expect("timestamp")
            .with_timezone(&Utc);

        assert!(should_transcode_for_buffer(
            now + chrono::Duration::seconds(60),
            now
        ));
        assert!(!should_transcode_for_buffer(
            now + chrono::Duration::seconds(61),
            now
        ));
        assert!(!should_use_realtime(
            now + chrono::Duration::seconds(29),
            now
        ));
        assert!(should_use_realtime(
            now + chrono::Duration::seconds(30),
            now
        ));
    }

    #[test]
    fn remaining_item_duration_uses_start_at_zero_or_request_time() {
        let start = DateTime::parse_from_rfc3339("2026-06-08T12:00:00Z")
            .expect("timestamp")
            .with_timezone(&Utc);
        let active = active_hls_item_for_window(start, start + chrono::Duration::seconds(48));

        assert_eq!(
            remaining_item_duration(&active, start + chrono::Duration::seconds(20), false),
            Some(Duration::from_secs(28))
        );
        assert_eq!(
            remaining_item_duration(&active, start + chrono::Duration::seconds(20), true),
            Some(Duration::from_secs(48))
        );
    }

    #[test]
    fn hls_playout_item_duration_caps_work_ahead_to_ersatztv_limit() {
        let start = DateTime::parse_from_rfc3339("2026-06-08T12:00:00Z")
            .expect("timestamp")
            .with_timezone(&Utc);
        let active = active_hls_item_for_window(start, start + chrono::Duration::minutes(45));

        assert_eq!(
            hls_playout_item_duration(&active, start, false, false),
            Some(Duration::from_secs(HLS_WORK_AHEAD_TRANSCODE_SECONDS))
        );
        assert_eq!(
            hls_playout_item_duration(&active, start, false, true),
            Some(Duration::from_mins(45))
        );
    }

    #[test]
    fn work_ahead_reservation_respects_configured_limit() {
        HLS_WORK_AHEAD_COUNT.store(0, Ordering::Relaxed);
        let first = try_reserve_work_ahead(1);
        assert!(first.is_some());
        assert!(try_reserve_work_ahead(1).is_none());
        drop(first);
        assert_eq!(HLS_WORK_AHEAD_COUNT.load(Ordering::Relaxed), 0);
        assert!(try_reserve_work_ahead(0).is_none());
    }

    #[cfg(not(feature = "mysql"))]
    #[tokio::test]
    async fn on_demand_startup_timeshifts_playout_and_updates_checkpoint() {
        let pool = on_demand_test_pool().await;

        let now = DateTime::parse_from_rfc3339("2026-06-08T12:00:00Z")
            .expect("timestamp")
            .with_timezone(&Utc);
        assert_eq!(load_channel_slug_seconds(&pool, 7).await, Some(2.5));
        time_shift_on_demand_playout(&pool, 7, now)
            .await
            .expect("time shift on-demand playout");

        assert_on_demand_playout_shifted(&pool).await;

        let later = DateTime::parse_from_rfc3339("2026-06-08T12:05:00Z")
            .expect("timestamp")
            .with_timezone(&Utc);
        update_on_demand_checkpoint(&pool, 7, later)
            .await
            .expect("update on-demand checkpoint");
        let checkpoint =
            sqlx::query(r#"SELECT "OnDemandCheckpoint" FROM "Playout" WHERE "Id" = 11"#)
                .fetch_one(&pool)
                .await
                .expect("load updated checkpoint");
        assert_eq!(
            checkpoint.get::<String, _>("OnDemandCheckpoint"),
            "2026-06-08 12:05:00"
        );
    }

    #[cfg(not(feature = "mysql"))]
    async fn on_demand_test_pool() -> sqlx::SqlitePool {
        use sqlx::sqlite::SqlitePoolOptions;

        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("open in-memory sqlite");
        create_on_demand_test_schema(&pool).await;
        seed_on_demand_test_playout(&pool).await;
        pool
    }

    #[cfg(not(feature = "mysql"))]
    async fn create_on_demand_test_schema(pool: &sqlx::SqlitePool) {
        for statement in [
            r#"CREATE TABLE "Channel" (
                "Id" INTEGER PRIMARY KEY, "Number" TEXT NOT NULL, "Name" TEXT NOT NULL,
                "PlayoutMode" INTEGER NOT NULL, "SlugSeconds" REAL NULL)"#,
            r#"CREATE TABLE "Playout" (
                "Id" INTEGER PRIMARY KEY, "ChannelId" INTEGER NOT NULL,
                "OnDemandCheckpoint" TEXT NULL)"#,
            r#"CREATE TABLE "PlayoutItem" (
                "Id" INTEGER PRIMARY KEY, "PlayoutId" INTEGER NOT NULL,
                "Start" TEXT NOT NULL, "Finish" TEXT NOT NULL,
                "GuideStart" TEXT NULL, "GuideFinish" TEXT NULL)"#,
            r#"CREATE TABLE "PlayoutHistory" (
                "Id" INTEGER PRIMARY KEY, "PlayoutId" INTEGER NOT NULL,
                "When" TEXT NOT NULL, "Finish" TEXT NOT NULL)"#,
            r#"CREATE TABLE "PlayoutProgramScheduleAnchor" (
                "Id" INTEGER PRIMARY KEY, "PlayoutId" INTEGER NOT NULL,
                "AnchorDate" TEXT NULL)"#,
            r#"CREATE TABLE "PlayoutAnchor" (
                "PlayoutId" INTEGER PRIMARY KEY, "NextStart" TEXT NOT NULL,
                "DurationFinish" TEXT NULL)"#,
        ] {
            sqlx::query(statement)
                .execute(pool)
                .await
                .expect("create on-demand test table");
        }
    }

    #[cfg(not(feature = "mysql"))]
    async fn seed_on_demand_test_playout(pool: &sqlx::SqlitePool) {
        for statement in [
            r#"INSERT INTO "Channel" ("Id", "Number", "Name", "PlayoutMode", "SlugSeconds")
               VALUES (7, '7', 'On Demand', 1, 2.5)"#,
            r#"INSERT INTO "Playout" ("Id", "ChannelId", "OnDemandCheckpoint")
               VALUES (11, 7, NULL)"#,
            r#"INSERT INTO "PlayoutItem"
               ("Id", "PlayoutId", "Start", "Finish", "GuideStart", "GuideFinish")
               VALUES
               (21, 11, '2026-06-08 10:00:00', '2026-06-08 10:00:24',
                '2026-06-08 10:00:05', '2026-06-08 10:00:20')"#,
            r#"INSERT INTO "PlayoutHistory" ("Id", "PlayoutId", "When", "Finish")
               VALUES (31, 11, '2026-06-08 09:30:00', '2026-06-08 09:45:00')"#,
            r#"INSERT INTO "PlayoutProgramScheduleAnchor" ("Id", "PlayoutId", "AnchorDate")
               VALUES (41, 11, '2026-06-08 09:00:00')"#,
            r#"INSERT INTO "PlayoutAnchor" ("PlayoutId", "NextStart", "DurationFinish")
               VALUES (11, '2026-06-08 11:00:00', '2026-06-08 11:30:00')"#,
        ] {
            sqlx::query(statement)
                .execute(pool)
                .await
                .expect("seed on-demand test data");
        }
    }

    #[cfg(not(feature = "mysql"))]
    async fn assert_on_demand_playout_shifted(pool: &sqlx::SqlitePool) {
        let item = sqlx::query(
            r#"SELECT "Start", "Finish", "GuideStart", "GuideFinish"
               FROM "PlayoutItem"
               WHERE "Id" = 21"#,
        )
        .fetch_one(pool)
        .await
        .expect("load shifted item");
        assert_eq!(item.get::<String, _>("Start"), "2026-06-08 12:00:00");
        assert_eq!(item.get::<String, _>("Finish"), "2026-06-08 12:00:24");
        assert_eq!(item.get::<String, _>("GuideStart"), "2026-06-08 12:00:05");
        assert_eq!(item.get::<String, _>("GuideFinish"), "2026-06-08 12:00:20");

        let history = sqlx::query(
            r#"SELECT "When", "Finish"
               FROM "PlayoutHistory"
               WHERE "Id" = 31"#,
        )
        .fetch_one(pool)
        .await
        .expect("load shifted history");
        assert_eq!(history.get::<String, _>("When"), "2026-06-08 11:30:00");
        assert_eq!(history.get::<String, _>("Finish"), "2026-06-08 11:45:00");

        let program_anchor = sqlx::query(
            r#"SELECT "AnchorDate" FROM "PlayoutProgramScheduleAnchor" WHERE "Id" = 41"#,
        )
        .fetch_one(pool)
        .await
        .expect("load shifted program anchor");
        assert_eq!(
            program_anchor.get::<String, _>("AnchorDate"),
            "2026-06-08 11:00:00"
        );

        let anchor = sqlx::query(
            r#"SELECT "NextStart", "DurationFinish"
               FROM "PlayoutAnchor"
               WHERE "PlayoutId" = 11"#,
        )
        .fetch_one(pool)
        .await
        .expect("load shifted anchor");
        assert_eq!(anchor.get::<String, _>("NextStart"), "2026-06-08 13:00:00");
        assert_eq!(
            anchor.get::<String, _>("DurationFinish"),
            "2026-06-08 13:30:00"
        );

        let checkpoint =
            sqlx::query(r#"SELECT "OnDemandCheckpoint" FROM "Playout" WHERE "Id" = 11"#)
                .fetch_one(pool)
                .await
                .expect("load checkpoint");
        assert_eq!(
            checkpoint.get::<String, _>("OnDemandCheckpoint"),
            "2026-06-08 12:00:00"
        );
    }

    #[test]
    fn build_hls_command_produces_hls_flags() {
        let command = build_hls_command(
            "/media/test.mkv",
            Path::new("/tmp/hls/1"),
            OutputFormat::Hls,
            true,
            false,
            false,
            0,
            None,
            None,
            &playback_profile(),
            None,
            OutputMetadata::default(),
            FfmpegCapabilityReport::default(),
            None,
            Vec::new(),
            None,
            None,
            Vec::new(),
            Vec::new(),
        );
        insta::assert_snapshot!(command.arguments.join(" "));
    }

    #[test]
    fn hls_segment_options_match_ersatztv_mpegts_rules() {
        assert_eq!(
            hls_segment_options(OutputFormat::Hls, None, FFmpegProfileAudioFormat::Aac),
            Some("mpegts_flags=+initial_discontinuity".to_owned())
        );
        assert_eq!(
            hls_segment_options(
                OutputFormat::Hls,
                Some(Duration::ZERO),
                FFmpegProfileAudioFormat::AacLatm,
            ),
            Some("mpegts_flags=+initial_discontinuity+latm".to_owned())
        );
        assert_eq!(
            hls_segment_options(
                OutputFormat::Hls,
                Some(Duration::from_secs(12)),
                FFmpegProfileAudioFormat::AacLatm,
            ),
            Some("mpegts_flags=+latm".to_owned())
        );
        assert_eq!(
            hls_segment_options(
                OutputFormat::HlsMp4,
                None,
                FFmpegProfileAudioFormat::AacLatm
            ),
            None
        );
    }

    #[test]
    fn build_hls_command_emits_initial_discontinuity_segment_options() {
        let command = build_hls_command(
            "/media/test.mkv",
            Path::new("/tmp/hls/1"),
            OutputFormat::Hls,
            true,
            false,
            false,
            0,
            None,
            None,
            &playback_profile(),
            None,
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

        assert!(arguments.contains("-hls_segment_options mpegts_flags=+initial_discontinuity"));
    }

    #[test]
    fn build_hls_command_emits_output_ts_offset() {
        let command = build_hls_command(
            "/media/test.mkv",
            Path::new("/tmp/hls/1"),
            OutputFormat::Hls,
            false,
            false,
            false,
            0,
            None,
            Some(Duration::from_millis(12_500)),
            &playback_profile(),
            None,
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
        assert!(arguments.contains("-output_ts_offset 12.5s"));
        assert!(
            !arguments.contains("mpegts_flags=+initial_discontinuity"),
            "{arguments}"
        );
    }

    #[test]
    fn build_hls_command_paces_realtime_segmenter_input() {
        let command = build_hls_command(
            "/media/test.mkv",
            Path::new("/tmp/hls/1"),
            OutputFormat::Hls,
            true,
            false,
            true,
            0,
            Some(Duration::from_secs(30)),
            None,
            &playback_profile(),
            None,
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

        assert!(arguments.contains("-readrate 1.05 -i /media/test.mkv"));
    }

    #[test]
    fn build_hls_command_emits_aac_latm_segment_options() {
        let mut playback_profile = playback_profile();
        playback_profile.profile.audio_format = FFmpegProfileAudioFormat::AacLatm;
        let command = build_hls_command(
            "/media/test.mkv",
            Path::new("/tmp/hls/1"),
            OutputFormat::Hls,
            true,
            false,
            false,
            0,
            None,
            None,
            &playback_profile,
            None,
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

        assert!(arguments.contains("-hls_segment_options mpegts_flags=+initial_discontinuity+latm"));
    }

    #[test]
    fn build_hls_slug_command_uses_generated_black_video_and_null_audio() {
        let command = build_hls_slug_command(
            OutputFormat::Hls,
            true,
            false,
            Duration::from_secs(3),
            None,
            &playback_profile(),
            Some(FrameRate::new(24, 1)),
            OutputMetadata {
                service_provider: Some("ErsatzTV".to_owned()),
                service_name: Some("Channel One".to_owned()),
                ..OutputMetadata::default()
            },
            FfmpegCapabilityReport::default(),
        );
        let arguments = command.arguments.join(" ");

        assert!(arguments.contains("-f lavfi -i color=c=black:s=1920x1080:r=24/1:d=3"));
        assert!(arguments.contains("-f lavfi -i anullsrc=channel_layout=stereo:sample_rate=48000"));
        assert!(arguments.contains("-t 00:00:03.0000000"));
        assert!(arguments.contains("-metadata service_provider=\"ErsatzTV\""));
        assert!(arguments.contains("-metadata service_name=\"Channel One\""));
        assert!(arguments.contains("-hls_segment_options mpegts_flags=+initial_discontinuity"));
    }

    #[test]
    fn build_hls_slug_command_uses_pts_offset_for_subsequent_segments() {
        let command = build_hls_slug_command(
            OutputFormat::Hls,
            false,
            false,
            Duration::from_secs(2),
            Some(Duration::from_millis(7_500)),
            &playback_profile(),
            None,
            OutputMetadata::default(),
            FfmpegCapabilityReport::default(),
        );
        let arguments = command.arguments.join(" ");

        assert!(arguments.contains("-output_ts_offset 7.5s"));
        assert!(arguments.contains("-hls_flags program_date_time+omit_endlist+append_list+discont_start+independent_segments"));
        assert!(!arguments.contains("mpegts_flags=+initial_discontinuity"));
    }

    #[test]
    fn build_hls_error_command_uses_background_subtitle_and_null_audio() {
        let command = build_hls_error_command_from_assets(HlsErrorCommandRequest {
            background_path: Path::new("/tmp/hls/error/background.png"),
            subtitle_path: Path::new("/tmp/hls/error/error.ass"),
            output_format: OutputFormat::Hls,
            is_first_transcode: true,
            hls_realtime: false,
            duration: Duration::from_secs(4),
            output_ts_offset: None,
            playback_profile: &playback_profile(),
            target_frame_rate: Some(FrameRate::new(30000, 1001)),
            metadata: OutputMetadata {
                service_provider: Some("ErsatzTV".to_owned()),
                service_name: Some("Channel One".to_owned()),
                ..OutputMetadata::default()
            },
            runtime_capabilities: FfmpegCapabilityReport::default(),
        });
        let arguments = command.arguments.join(" ");

        assert!(arguments.contains("-loop 1 -i /tmp/hls/error/background.png"));
        assert!(arguments.contains("-f lavfi -i anullsrc=channel_layout=stereo:sample_rate=48000"));
        assert!(arguments.contains("subtitles=/tmp/hls/error/error.ass"));
        assert!(arguments.contains("-t 00:00:04.0000000"));
        assert!(arguments.contains("-metadata service_provider=\"ErsatzTV\""));
        assert!(arguments.contains("-metadata service_name=\"Channel One\""));
        assert!(arguments.contains("-hls_segment_options mpegts_flags=+initial_discontinuity"));
    }

    #[test]
    fn build_hls_error_command_keeps_still_image_in_software_for_nvenc() {
        let mut nvenc_profile = playback_profile();
        nvenc_profile.profile.hardware_acceleration =
            ersatzrs_domain::HardwareAccelerationKind::Nvenc;
        let command = build_hls_error_command_from_assets(HlsErrorCommandRequest {
            background_path: Path::new("/tmp/hls/error/background.png"),
            subtitle_path: Path::new("/tmp/hls/error/error.ass"),
            output_format: OutputFormat::Hls,
            is_first_transcode: true,
            hls_realtime: false,
            duration: Duration::from_secs(4),
            output_ts_offset: None,
            playback_profile: &nvenc_profile,
            target_frame_rate: Some(FrameRate::new(30000, 1001)),
            metadata: OutputMetadata::default(),
            runtime_capabilities: FfmpegCapabilityReport::default(),
        });
        let arguments = command.arguments.join(" ");

        // The PNG background feeds software-only filters (`loop`, subtitle
        // burn): no upfront hwupload/scale_cuda, and the RGB still image must
        // be converted for the encoder via the pixel-format output option.
        assert!(
            !arguments.contains("hwupload"),
            "still-image error pipeline must stay in software, got: {arguments}"
        );
        assert!(
            !arguments.contains("scale_cuda"),
            "software frames cannot feed scale_cuda, got: {arguments}"
        );
        assert!(
            arguments.contains("-pix_fmt yuv420p"),
            "still image needs a pixel format conversion before encode, got: {arguments}"
        );
        assert!(arguments.contains("loop=-1:1"));
        assert!(arguments.contains("subtitles=/tmp/hls/error/error.ass"));
    }

    #[tokio::test]
    async fn build_hls_error_command_writes_generated_assets() {
        let working_dir = tempfile::tempdir().expect("create hls temp dir");
        let command = build_hls_error_command(HlsErrorProcessRequest {
            working_dir: working_dir.path(),
            output_format: OutputFormat::Hls,
            is_first_transcode: false,
            hls_realtime: false,
            duration: Duration::from_secs(2),
            output_ts_offset: Some(Duration::from_secs(8)),
            playback_profile: &playback_profile(),
            target_frame_rate: None,
            metadata: OutputMetadata::default(),
            runtime_capabilities: FfmpegCapabilityReport::default(),
            error_message: "first line\nsecond {line}",
        })
        .await
        .expect("hls error command");

        let subtitle = tokio::fs::read_to_string(working_dir.path().join("error/error.ass"))
            .await
            .expect("read hls error subtitle");
        assert!(working_dir.path().join("error/background.png").exists());
        assert!(subtitle.contains("PlayResX: 1920"));
        assert!(subtitle.contains("PlayResY: 1080"));
        assert!(subtitle.contains(r"first line\Nsecond (line)"));

        let arguments = command.arguments.join(" ");
        assert!(arguments.contains("-output_ts_offset 8s"));
        assert!(!arguments.contains("mpegts_flags=+initial_discontinuity"));
    }

    #[test]
    fn hls_error_ass_matches_ersatztv_overlay_shape() {
        let ass = hls_error_ass(
            "ffmpeg failed\r\nUse {fallback}\\path",
            FrameSize::new(1280, 720),
        );

        assert!(ass.contains("PlayResX: 1280"));
        assert!(ass.contains("PlayResY: 720"));
        assert!(ass.contains("Style: Default,Roboto,36"));
        assert!(ass.contains(",36,36,36,1"));
        assert!(ass.contains(r"ffmpeg failed\NUse (fallback)\\path"));
    }

    #[test]
    fn parse_pts_line_adds_packet_duration() {
        assert_eq!(parse_pts_line("12.5|0.25"), Some(12.75));
        assert_eq!(parse_pts_line("12.5|N/A"), Some(12.5));
        assert_eq!(parse_pts_line("not-a-number|0.25"), None);
    }

    #[test]
    fn build_hls_command_maps_selected_audio_stream() {
        let command = build_hls_command(
            "/media/test.mkv",
            Path::new("/tmp/hls/1"),
            OutputFormat::Hls,
            true,
            true,
            false,
            0,
            None,
            None,
            &playback_profile(),
            None,
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
    fn build_hls_command_emits_playback_output_metadata() {
        let mut audio_stream =
            ersatzrs_ffmpeg::SourceAudioStream::new(ersatzrs_ffmpeg::SourceAudioCodec::Aac)
                .with_index(3);
        audio_stream.language = Some("jpn".to_owned());
        let subtitle = Some(PlaybackSubtitle {
            id: 1,
            subtitle_kind: 2,
            title: Some("Dutch SDH".to_owned()),
            stream_index: 4,
            codec: Some("subrip".to_owned()),
            default: false,
            forced: false,
            sdh: true,
            language: Some("nld".to_owned()),
            is_extracted: false,
            path: None,
        });
        let metadata = playback_output_metadata(
            Some("ErsatzTV"),
            Some("Cinema One"),
            Some("eng"),
            &[audio_stream.clone()],
            subtitle.as_ref(),
            Some(3),
        );
        let command = build_hls_command(
            "/media/test.mkv",
            Path::new("/tmp/hls/1"),
            OutputFormat::Hls,
            true,
            false,
            false,
            0,
            None,
            None,
            &playback_profile(),
            None,
            metadata,
            FfmpegCapabilityReport::default(),
            None,
            vec![audio_stream],
            Some(3),
            None,
            Vec::new(),
            Vec::new(),
        );
        let arguments = command.arguments.join(" ");

        assert!(arguments.contains("-metadata service_provider=\"ErsatzTV\""));
        assert!(arguments.contains("-metadata service_name=\"Cinema One\""));
        assert!(arguments.contains("-metadata:s:a:0 language=jpn"));
        assert!(arguments.contains("-metadata:s:s:0 language=nld"));
        assert!(arguments.contains("-metadata:s:s:0 title=Dutch SDH"));
    }

    #[test]
    fn build_hls_command_burns_generated_music_video_credits_subtitle() {
        let subtitle = PlaybackSubtitle {
            id: 0,
            subtitle_kind: crate::metadata::subtitle_kind::GENERATED,
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
            OutputFormat::Hls,
            StreamingMode::HttpLiveStreamingSegmenter,
        );
        let command = build_hls_command(
            "/media/music-video.mkv",
            Path::new("/tmp/hls/1"),
            OutputFormat::Hls,
            true,
            false,
            false,
            0,
            None,
            None,
            &playback_profile(),
            None,
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
    fn build_hls_command_seeking_dts_uses_core_only_decoder() {
        let command = build_hls_command(
            "/media/test.mkv",
            Path::new("/tmp/hls/1"),
            OutputFormat::Hls,
            true,
            true,
            false,
            120,
            None,
            None,
            &playback_profile(),
            None,
            OutputMetadata::default(),
            FfmpegCapabilityReport::default(),
            None,
            vec![
                ersatzrs_ffmpeg::SourceAudioStream::new(ersatzrs_ffmpeg::SourceAudioCodec::Dts)
                    .with_index(1),
            ],
            None,
            None,
            Vec::new(),
            Vec::new(),
        );
        let arguments = command.arguments.join(" ");

        assert!(
            arguments.contains("-ss 120 -c:a dts -core_only true -i /media/test.mkv"),
            "{arguments}"
        );
    }

    #[test]
    fn build_hls_command_produces_fmp4_flags() {
        let command = build_hls_command(
            "/media/test.mkv",
            Path::new("/tmp/hls/1"),
            OutputFormat::HlsMp4,
            true,
            false,
            false,
            0,
            None,
            None,
            &playback_profile(),
            None,
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
        assert!(arguments.contains("-hls_segment_type fmp4"));
        assert!(arguments.contains("_init.mp4"));
        assert!(arguments.contains("-hls_segment_filename live_"));
        assert!(arguments.contains("_%06d.m4s"));
        assert!(!arguments.contains("-hls_segment_options"));
    }

    #[test]
    fn build_hls_command_seeks_to_playout_offset() {
        let command = build_hls_command(
            "/media/test.mkv",
            Path::new("/tmp/hls/1"),
            OutputFormat::Hls,
            true,
            false,
            false,
            900,
            None,
            None,
            &playback_profile(),
            None,
            OutputMetadata::default(),
            FfmpegCapabilityReport::default(),
            None,
            Vec::new(),
            None,
            None,
            Vec::new(),
            Vec::new(),
        );

        assert!(command
            .arguments
            .join(" ")
            .contains("-ss 900 -i /media/test.mkv"));
    }

    #[test]
    fn build_hls_command_attaches_graphics_overlays() {
        let command = build_hls_command(
            "/media/test.mkv",
            Path::new("/tmp/hls/1"),
            OutputFormat::Hls,
            true,
            false,
            false,
            0,
            None,
            None,
            &playback_profile(),
            None,
            OutputMetadata::default(),
            FfmpegCapabilityReport::default(),
            None,
            Vec::new(),
            None,
            None,
            Vec::new(),
            vec![GraphicsPipelineOverlay::drawvg(
                12,
                "drawvg=file=/tmp/hls/1/graphics/item-1/title.vgs",
            )],
        );
        let arguments = command.arguments.join(" ");

        assert!(arguments.contains("-filter_complex"));
        assert!(
            arguments.contains("drawvg=file=/tmp/hls/1/graphics/item-1/title.vgs"),
            "{arguments}"
        );
    }

    #[test]
    fn playout_start_seconds_matches_mid_item_offset() {
        let now = DateTime::parse_from_rfc3339("2026-05-31T21:45:00.000+00:00")
            .expect("valid timestamp")
            .with_timezone(&Utc);
        let start = DateTime::parse_from_rfc3339("2026-05-31T21:30:00.000+00:00")
            .expect("valid timestamp")
            .with_timezone(&Utc);

        assert_eq!(playout_start_seconds(now, start), 900);
        assert_eq!(playout_start_seconds(start, now), 0);
    }

    #[test]
    fn av1_nvenc_decode_rejection_creates_retry_denial() {
        let mut source = SourceVideoStream::new(SourceVideoCodec::Av1);
        source.pixel_format = Some(PixelFormat::Yuv420P10Le);
        let stderr = "Your platform doesn't support hardware accelerated AV1 decoding\n\
                      Error submitting packet to decoder: Function not implemented";

        let denial = learned_hardware_decode_denial(
            false,
            HardwareAccelerationKind::Nvenc,
            Some(&source),
            stderr,
        )
        .expect("denial");

        assert_eq!(denial.acceleration, HardwareAccelerationKind::Nvenc);
        assert_eq!(denial.codec, "av1");
        assert_eq!(denial.bit_depth, 10);
    }

    #[test]
    fn hardware_decode_rejection_does_not_retry_twice() {
        let source = SourceVideoStream::new(SourceVideoCodec::Av1);
        let stderr = "Your platform doesn't support hardware accelerated AV1 decoding";

        assert!(learned_hardware_decode_denial(
            true,
            HardwareAccelerationKind::Nvenc,
            Some(&source),
            stderr,
        )
        .is_none());
    }

    #[test]
    fn trim_playlist_contents_keeps_recent_segments() {
        let playlist = concat!(
            "#EXTM3U\n",
            "#EXT-X-VERSION:7\n",
            "#EXT-X-TARGETDURATION:4\n",
            "#EXT-X-MEDIA-SEQUENCE:1\n",
            "#EXT-X-INDEPENDENT-SEGMENTS\n",
            "#EXTINF:4.000000,\n",
            "#EXT-X-PROGRAM-DATE-TIME:2026-04-20T20:00:00.000+00:00\n",
            "seg00001.ts\n",
            "#EXTINF:4.000000,\n",
            "#EXT-X-PROGRAM-DATE-TIME:2026-04-20T20:00:40.000+00:00\n",
            "seg00002.ts\n",
            "#EXT-X-DISCONTINUITY\n",
            "#EXTINF:4.000000,\n",
            "#EXT-X-PROGRAM-DATE-TIME:2026-04-20T20:01:20.000+00:00\n",
            "seg00003.ts\n"
        );

        let trimmed = trim_playlist_contents(
            OutputFormat::Hls,
            playlist,
            DateTime::parse_from_rfc3339("2026-04-20T20:00:30.000+00:00")
                .expect("valid timestamp")
                .with_timezone(&Utc),
            None,
            None,
        )
        .expect("trimmed playlist");

        assert!(!trimmed.playlist.contains("seg00001.ts"));
        assert!(trimmed.playlist.contains("#EXT-X-DISCONTINUITY"));
        assert!(trimmed.playlist.contains("seg00002.ts"));
        assert!(trimmed.playlist.contains("seg00003.ts"));
        assert_eq!(trimmed.referenced_files.len(), 2);
    }

    #[test]
    fn trim_playlist_contents_caps_playlist_window() {
        let mut playlist = concat!(
            "#EXTM3U\n",
            "#EXT-X-VERSION:7\n",
            "#EXT-X-TARGETDURATION:4\n",
            "#EXT-X-MEDIA-SEQUENCE:1\n",
            "#EXT-X-INDEPENDENT-SEGMENTS\n"
        )
        .to_string();
        for sequence in 1..=12 {
            let _ = writeln!(playlist, "#EXTINF:4.000000,");
            let _ = writeln!(
                playlist,
                "#EXT-X-PROGRAM-DATE-TIME:2026-04-20T20:{sequence:02}:00.000+00:00"
            );
            let _ = writeln!(playlist, "seg{sequence:05}.ts");
        }

        let trimmed = trim_playlist_contents(
            OutputFormat::Hls,
            &playlist,
            DateTime::parse_from_rfc3339("2026-04-20T20:13:00.000+00:00")
                .expect("valid timestamp")
                .with_timezone(&Utc),
            Some(10),
            None,
        )
        .expect("trimmed playlist");

        assert!(!trimmed.playlist.contains("seg00001.ts"));
        assert!(!trimmed.playlist.contains("seg00002.ts"));
        assert!(trimmed.playlist.contains("seg00003.ts"));
        assert!(trimmed.playlist.contains("seg00012.ts"));
        assert_eq!(trimmed.referenced_files.len(), 10);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ignore = "requires ffmpeg, ffprobe, and the Big Buck Bunny fixture under testdata/"]
    async fn hls_segmenter_output_is_ffprobe_readable() {
        let Some(fixture) = testdata_fixture() else {
            eprintln!("skipping: no Big Buck Bunny fixture found in testdata/");
            return;
        };
        if Command::new(ersatzrs_ffmpeg::ffmpeg_path())
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
        if Command::new(ersatzrs_ffmpeg::ffprobe_path())
            .arg("-version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await
            .is_err()
        {
            eprintln!("skipping: ffprobe not on PATH");
            return;
        }

        let working_dir = tempfile::tempdir().expect("create hls temp dir");
        let command = build_hls_command(
            fixture.to_string_lossy().as_ref(),
            working_dir.path(),
            OutputFormat::Hls,
            true,
            false,
            false,
            0,
            None,
            None,
            &playback_profile(),
            None,
            OutputMetadata::default(),
            FfmpegCapabilityReport::default(),
            Some(source_1080p()),
            Vec::new(),
            None,
            None,
            Vec::new(),
            Vec::new(),
        );

        let mut process = Command::new(&command.executable);
        process
            .args(&command.arguments)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        let mut child = process.spawn().expect("spawn ffmpeg hls segmenter");
        let segment = working_dir.path().join("seg00000.ts");

        timeout(Duration::from_secs(30), async {
            loop {
                if let Ok(metadata) = tokio::fs::metadata(&segment).await {
                    if metadata.len() > 0 {
                        break;
                    }
                }
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
        })
        .await
        .expect("timed out waiting for first HLS segment");

        child.kill().await.ok();
        let _ = child.wait().await;

        let output = Command::new(ersatzrs_ffmpeg::ffprobe_path())
            .args([
                "-v",
                "error",
                "-show_entries",
                "stream=codec_type",
                "-of",
                "csv=p=0",
            ])
            .arg(&segment)
            .output()
            .await
            .expect("run ffprobe");
        assert!(
            output.status.success(),
            "ffprobe failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let streams = String::from_utf8_lossy(&output.stdout);
        assert!(
            streams.lines().any(|line| line == "video"),
            "expected a readable video stream, got: {streams}"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ignore = "requires ffmpeg, ffprobe, and the Big Buck Bunny fixture under testdata/"]
    async fn hls_fmp4_segmenter_output_has_map_and_readable_segment() {
        let Some(fixture) = testdata_fixture() else {
            eprintln!("skipping: no Big Buck Bunny fixture found in testdata/");
            return;
        };
        if !ffmpeg_is_available().await || !ffprobe_is_available().await {
            eprintln!("skipping: ffmpeg or ffprobe not usable");
            return;
        }

        let working_dir = tempfile::tempdir().expect("create hls temp dir");
        let command = build_hls_command(
            fixture.to_string_lossy().as_ref(),
            working_dir.path(),
            OutputFormat::HlsMp4,
            true,
            false,
            false,
            0,
            Some(Duration::from_secs(5)),
            None,
            &playback_profile(),
            None,
            OutputMetadata::default(),
            FfmpegCapabilityReport::default(),
            Some(source_1080p()),
            Vec::new(),
            None,
            None,
            Vec::new(),
            Vec::new(),
        );

        let mut process = Command::new(&command.executable);
        process
            .args(&command.arguments)
            .current_dir(working_dir.path())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        apply_ffmpeg_library_path(&mut process);
        let output = timeout(Duration::from_secs(45), process.output())
            .await
            .expect("timed out running ffmpeg fmp4 hls segmenter")
            .expect("run ffmpeg fmp4 hls segmenter");
        assert!(
            output.status.success(),
            "ffmpeg fmp4 hls failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let playlist = tokio::fs::read_to_string(working_dir.path().join("live.m3u8"))
            .await
            .expect("read fmp4 playlist");
        let init = find_generated_file(working_dir.path(), "_init.mp4")
            .await
            .expect("find generated init");
        let segment = find_generated_file(working_dir.path(), ".m4s")
            .await
            .expect("find generated segment");
        let init_name = init.file_name().and_then(|name| name.to_str()).unwrap();

        assert!(playlist.contains(&format!("#EXT-X-MAP:URI=\"{init_name}\"")));
        assert!(segment
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| {
                name.starts_with("live_")
                    && Path::new(name)
                        .extension()
                        .is_some_and(|extension| extension.eq_ignore_ascii_case("m4s"))
            }));

        let combined = concatenate_fmp4_init_and_segment(working_dir.path(), init_name, &segment)
            .await
            .expect("combine init and segment");
        let mut probe = Command::new(ersatzrs_ffmpeg::ffprobe_path());
        probe
            .args([
                "-v",
                "error",
                "-show_entries",
                "stream=codec_type",
                "-of",
                "csv=p=0",
            ])
            .arg(&combined);
        apply_ffmpeg_library_path(&mut probe);
        let output = probe.output().await.expect("run ffprobe");
        let _ = tokio::fs::remove_file(combined).await;
        assert!(
            output.status.success(),
            "ffprobe failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let streams = String::from_utf8_lossy(&output.stdout);
        assert!(
            streams.lines().any(|line| line == "video"),
            "expected a readable video stream, got: {streams}"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ignore = "requires ffmpeg with drawvg on PATH"]
    async fn hls_segmenter_composites_drawvg_graphics_from_generated_input() {
        let ffmpeg = ersatzrs_ffmpeg::ffmpeg_path();
        if !ffmpeg_has_drawvg(&ffmpeg).await {
            eprintln!("skipping: ffmpeg drawvg filter not available");
            return;
        }

        let temp = tempfile::tempdir().expect("tempdir");
        let input = temp.path().join("black.mp4");
        let script = temp.path().join("green.vgs");
        std::fs::write(&script, green_drawvg_script()).expect("write graphics script");
        generate_hls_graphics_input(&ffmpeg, &input).await;

        let working_dir = temp.path().join("hls");
        tokio::fs::create_dir_all(&working_dir)
            .await
            .expect("create hls dir");
        run_hls_graphics_segmenter(&input, &script, &working_dir).await;

        let segment = working_dir.join("seg00000.ts");
        assert!(
            segment.is_file(),
            "expected first HLS segment at {}",
            segment.display()
        );
        assert_hls_segment_has_green_graphics(&ffmpeg, &segment).await;
    }

    async fn ffmpeg_has_drawvg(ffmpeg: &str) -> bool {
        Command::new(ffmpeg)
            .args(["-hide_banner", "-filters"])
            .output()
            .await
            .is_ok_and(|output| String::from_utf8_lossy(&output.stdout).contains("drawvg"))
    }

    async fn ffmpeg_is_available() -> bool {
        let mut command = Command::new(ersatzrs_ffmpeg::ffmpeg_path());
        command
            .arg("-version")
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        apply_ffmpeg_library_path(&mut command);
        command.status().await.is_ok_and(|status| status.success())
    }

    async fn ffprobe_is_available() -> bool {
        let mut command = Command::new(ersatzrs_ffmpeg::ffprobe_path());
        command
            .arg("-version")
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        apply_ffmpeg_library_path(&mut command);
        command.status().await.is_ok_and(|status| status.success())
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

    async fn find_generated_file(working_dir: &Path, suffix: &str) -> Option<PathBuf> {
        let mut entries = tokio::fs::read_dir(working_dir).await.ok()?;
        while let Some(entry) = entries.next_entry().await.ok()? {
            let path = entry.path();
            if path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(suffix))
            {
                return Some(path);
            }
        }
        None
    }

    async fn generate_hls_graphics_input(ffmpeg: &str, input: &Path) {
        let generated = Command::new(ffmpeg)
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

    async fn run_hls_graphics_segmenter(input: &Path, script: &Path, working_dir: &Path) {
        let command = build_hls_command(
            input.to_string_lossy().as_ref(),
            working_dir,
            OutputFormat::Hls,
            true,
            false,
            false,
            0,
            None,
            None,
            &playback_profile(),
            None,
            OutputMetadata::default(),
            FfmpegCapabilityReport::default(),
            Some(source_320p()),
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
            .current_dir(working_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        for variable in command.environment {
            process.env(variable.name, variable.value);
        }
        let output = timeout(Duration::from_secs(30), process.output())
            .await
            .expect("timed out running hls graphics segmenter")
            .expect("run hls graphics segmenter");
        assert!(
            output.status.success(),
            "ffmpeg hls failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    async fn assert_hls_segment_has_green_graphics(ffmpeg: &str, segment: &Path) {
        let decoded = Command::new(ffmpeg)
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-i",
                segment.to_string_lossy().as_ref(),
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
            .expect("decode hls graphics frame");
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

    fn active_hls_item_for_window(
        playout_start: DateTime<Utc>,
        playout_finish: DateTime<Utc>,
    ) -> ActiveHlsItem {
        ActiveHlsItem {
            media_path: "/media/test.mkv".to_owned(),
            playout_item_id: 1,
            media_item_id: 1,
            media_version_id: 1,
            content_duration: (playout_finish - playout_start).to_std().ok(),
            in_point: Duration::ZERO,
            program_duration: (playout_finish - playout_start).to_std().ok(),
            playout_start,
            playout_finish: Some(playout_finish),
            disable_watermarks: false,
            preferred_audio_language_code: None,
            preferred_subtitle_language_code: None,
            subtitle_mode: None,
            channel_name: "Channel One".to_owned(),
            channel_preferred_audio_language_code: None,
            channel_preferred_subtitle_language_code: None,
            channel_subtitle_mode: ChannelSubtitleMode::None,
            channel_watermark_id: None,
            channel_music_video_credits_mode: ChannelMusicVideoCreditsMode::None,
            channel_music_video_credits_template: None,
        }
    }

    fn source_1080p() -> SourceVideoStream {
        let mut source = SourceVideoStream::new(ersatzrs_ffmpeg::SourceVideoCodec::H264);
        source.frame_size = Some(ersatzrs_ffmpeg::FrameSize::new(1920, 1080));
        source.sample_aspect_ratio = Some("1:1".to_owned());
        source.display_aspect_ratio = Some("1920:1080".to_owned());
        source
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

    fn testdata_fixture() -> Option<PathBuf> {
        let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .unwrap_or_else(|| Path::new("."));
        [
            "testdata/bbb_sunflower_1080p_30fps_normal.mp4",
            "testdata/bbb_sunflower_1080p_60fps_normal.mp4",
            "testdata/bbb_sunflower_2160p_30fps_normal.mp4",
            "testdata/bbb_sunflower_2160p_60fps_normal.mp4",
        ]
        .into_iter()
        .map(|path| workspace_root.join(path))
        .find(|path| path.is_file())
    }

    #[test]
    fn trim_playlist_contents_preserves_map_for_fmp4() {
        let playlist = concat!(
            "#EXTM3U\n",
            "#EXT-X-VERSION:7\n",
            "#EXT-X-TARGETDURATION:4\n",
            "#EXT-X-MEDIA-SEQUENCE:1\n",
            "#EXT-X-INDEPENDENT-SEGMENTS\n",
            "#EXT-X-MAP:URI=\"init.mp4\"\n",
            "#EXTINF:4.000000,\n",
            "#EXT-X-PROGRAM-DATE-TIME:2026-04-20T20:01:20.000+00:00\n",
            "seg00003.m4s\n"
        );

        let trimmed = trim_playlist_contents(
            OutputFormat::HlsMp4,
            playlist,
            DateTime::parse_from_rfc3339("2026-04-20T20:00:30.000+00:00")
                .expect("valid timestamp")
                .with_timezone(&Utc),
            Some(10),
            None,
        )
        .expect("trimmed playlist");

        assert!(trimmed.playlist.contains("#EXT-X-MAP:URI=\"init.mp4\""));
        assert!(trimmed.referenced_files.contains("init.mp4"));
        assert!(trimmed.referenced_files.contains("seg00003.m4s"));
    }

    #[tokio::test]
    async fn trim_playlist_contents_rewrites_fmp4_map_to_earliest_cached_init() {
        let working_dir =
            std::env::temp_dir().join(format!("ersatzrs-hls-test-{}", Uuid::new_v4()));
        tokio::fs::create_dir_all(&working_dir)
            .await
            .expect("create temp dir");
        tokio::fs::write(working_dir.join("1760874038_init.mp4"), b"same-init")
            .await
            .expect("write earliest init");
        tokio::fs::write(working_dir.join("1760874042_init.mp4"), b"same-init")
            .await
            .expect("write duplicate init");
        tokio::fs::write(working_dir.join("live_1760874038_000001.m4s"), b"segment")
            .await
            .expect("write first segment");
        tokio::fs::write(working_dir.join("live_1760874042_000001.m4s"), b"segment")
            .await
            .expect("write second segment");

        let mut init_cache = HlsInitSegmentCache::default();
        refresh_init_segments(&working_dir, &mut init_cache)
            .await
            .expect("refresh init cache");

        let playlist = concat!(
            "#EXTM3U\n",
            "#EXT-X-VERSION:7\n",
            "#EXT-X-TARGETDURATION:4\n",
            "#EXT-X-MEDIA-SEQUENCE:1\n",
            "#EXT-X-INDEPENDENT-SEGMENTS\n",
            "#EXT-X-MAP:URI=\"1760874038_init.mp4\"\n",
            "#EXTINF:4.000000,\n",
            "#EXT-X-PROGRAM-DATE-TIME:2026-04-20T20:01:20.000+00:00\n",
            "live_1760874038_000001.m4s\n",
            "#EXT-X-DISCONTINUITY\n",
            "#EXT-X-MAP:URI=\"1760874042_init.mp4\"\n",
            "#EXTINF:4.000000,\n",
            "#EXT-X-PROGRAM-DATE-TIME:2026-04-20T20:01:24.000+00:00\n",
            "live_1760874042_000001.m4s\n"
        );

        let trimmed = trim_playlist_contents(
            OutputFormat::HlsMp4,
            playlist,
            DateTime::parse_from_rfc3339("2026-04-20T20:01:00.000+00:00")
                .expect("valid timestamp")
                .with_timezone(&Utc),
            Some(10),
            Some(&init_cache),
        )
        .expect("trimmed playlist");

        assert_eq!(
            trimmed
                .playlist
                .matches("#EXT-X-MAP:URI=\"1760874038_init.mp4\"")
                .count(),
            2
        );
        assert!(!trimmed.playlist.contains("1760874042_init.mp4"));
        assert!(trimmed.referenced_files.contains("1760874038_init.mp4"));
        assert!(!trimmed.referenced_files.contains("1760874042_init.mp4"));

        let _ = tokio::fs::remove_dir_all(&working_dir).await;
    }

    #[tokio::test]
    async fn trim_playlist_contents_uses_retained_fmp4_generation_for_cleanup() {
        let working_dir =
            std::env::temp_dir().join(format!("ersatzrs-hls-test-{}", Uuid::new_v4()));
        tokio::fs::create_dir_all(&working_dir)
            .await
            .expect("create temp dir");
        for generated_at in [1_760_873_990_i64, 1_760_874_038, 1_760_874_042] {
            tokio::fs::write(
                working_dir.join(format!("{generated_at}_init.mp4")),
                b"same-init",
            )
            .await
            .expect("write init");
            tokio::fs::write(
                working_dir.join(format!("live_{generated_at}_000001.m4s")),
                b"segment",
            )
            .await
            .expect("write segment");
        }

        let mut init_cache = HlsInitSegmentCache::default();
        refresh_init_segments(&working_dir, &mut init_cache)
            .await
            .expect("refresh init cache");

        let playlist = concat!(
            "#EXTM3U\n",
            "#EXT-X-VERSION:7\n",
            "#EXT-X-TARGETDURATION:4\n",
            "#EXT-X-MEDIA-SEQUENCE:1\n",
            "#EXT-X-INDEPENDENT-SEGMENTS\n",
            "#EXT-X-MAP:URI=\"1760873990_init.mp4\"\n",
            "#EXTINF:4.000000,\n",
            "#EXT-X-PROGRAM-DATE-TIME:2026-04-20T20:00:40.000+00:00\n",
            "live_1760873990_000001.m4s\n",
            "#EXT-X-DISCONTINUITY\n",
            "#EXT-X-MAP:URI=\"1760874038_init.mp4\"\n",
            "#EXTINF:4.000000,\n",
            "#EXT-X-PROGRAM-DATE-TIME:2026-04-20T20:01:20.000+00:00\n",
            "live_1760874038_000001.m4s\n",
            "#EXT-X-DISCONTINUITY\n",
            "#EXT-X-MAP:URI=\"1760874042_init.mp4\"\n",
            "#EXTINF:4.000000,\n",
            "#EXT-X-PROGRAM-DATE-TIME:2026-04-20T20:01:24.000+00:00\n",
            "live_1760874042_000001.m4s\n"
        );

        let trimmed = trim_playlist_contents(
            OutputFormat::HlsMp4,
            playlist,
            DateTime::parse_from_rfc3339("2026-04-20T20:01:00.000+00:00")
                .expect("valid timestamp")
                .with_timezone(&Utc),
            Some(2),
            Some(&init_cache),
        )
        .expect("trimmed playlist");

        assert_eq!(trimmed.generated_at, Some(1_760_874_038));
        assert!(!trimmed
            .referenced_files
            .contains("live_1760873990_000001.m4s"));
        assert!(init_is_at_or_after_trim_generation(
            "1760874038_init.mp4",
            trimmed.generated_at
        ));
        assert!(init_is_at_or_after_trim_generation(
            "1760874042_init.mp4",
            trimmed.generated_at
        ));

        let _ = tokio::fs::remove_dir_all(&working_dir).await;
    }

    #[tokio::test]
    async fn prepare_working_dir_removes_stale_contents() {
        let working_dir =
            std::env::temp_dir().join(format!("ersatzrs-hls-test-{}", Uuid::new_v4()));
        tokio::fs::create_dir_all(&working_dir)
            .await
            .expect("create temp dir");
        tokio::fs::write(working_dir.join("seg00001.ts"), b"stale")
            .await
            .expect("write stale segment");

        prepare_working_dir(&working_dir)
            .await
            .expect("prepare working dir");

        assert!(working_dir.exists());
        assert!(!working_dir.join("seg00001.ts").exists());

        let _ = tokio::fs::remove_dir_all(&working_dir).await;
    }

    #[tokio::test]
    async fn cleanup_working_dir_removes_directory() {
        let working_dir =
            std::env::temp_dir().join(format!("ersatzrs-hls-test-{}", Uuid::new_v4()));
        tokio::fs::create_dir_all(&working_dir)
            .await
            .expect("create temp dir");
        tokio::fs::write(working_dir.join("seg00001.ts"), b"segment")
            .await
            .expect("write segment");

        cleanup_working_dir(&working_dir)
            .await
            .expect("cleanup working dir");

        assert!(!working_dir.exists());
    }

    #[tokio::test]
    async fn delete_old_segments_removes_unreferenced_files() {
        let working_dir =
            std::env::temp_dir().join(format!("ersatzrs-hls-test-{}", Uuid::new_v4()));
        tokio::fs::create_dir_all(&working_dir)
            .await
            .expect("create temp dir");
        tokio::fs::write(working_dir.join("seg00001.ts"), b"old")
            .await
            .expect("write old segment");
        tokio::fs::write(working_dir.join("seg00002.ts"), b"keep")
            .await
            .expect("write retained segment");
        tokio::fs::write(working_dir.join("live.m3u8"), b"playlist")
            .await
            .expect("write playlist");

        let trim_result = test_trim_result(2, None, ["seg00002.ts"]);

        let mut init_cache = HlsInitSegmentCache::default();
        delete_old_segments(&working_dir, &trim_result, &mut init_cache)
            .await
            .expect("delete old segments");

        assert!(!working_dir.join("seg00001.ts").exists());
        assert!(working_dir.join("seg00002.ts").exists());
        assert!(working_dir.join("live.m3u8").exists());

        let _ = tokio::fs::remove_dir_all(&working_dir).await;
    }

    #[tokio::test]
    async fn delete_old_segments_removes_stale_fmp4_init() {
        let working_dir =
            std::env::temp_dir().join(format!("ersatzrs-hls-test-{}", Uuid::new_v4()));
        tokio::fs::create_dir_all(&working_dir)
            .await
            .expect("create temp dir");
        tokio::fs::write(working_dir.join("init-old.mp4"), b"old")
            .await
            .expect("write old init");
        tokio::fs::write(working_dir.join("init.mp4"), b"keep")
            .await
            .expect("write retained init");
        tokio::fs::write(working_dir.join("seg00001.m4s"), b"old")
            .await
            .expect("write old segment");
        tokio::fs::write(working_dir.join("seg00002.m4s"), b"keep")
            .await
            .expect("write retained segment");

        let trim_result = test_trim_result(2, None, ["init.mp4", "seg00002.m4s"]);

        let mut init_cache = HlsInitSegmentCache::default();
        delete_old_segments(&working_dir, &trim_result, &mut init_cache)
            .await
            .expect("delete old segments");

        assert!(!working_dir.join("init-old.mp4").exists());
        assert!(working_dir.join("init.mp4").exists());
        assert!(!working_dir.join("seg00001.m4s").exists());
        assert!(working_dir.join("seg00002.m4s").exists());

        let _ = tokio::fs::remove_dir_all(&working_dir).await;
    }

    #[tokio::test]
    async fn delete_old_segments_keeps_earliest_cached_init_and_removes_stale_duplicate() {
        let working_dir =
            std::env::temp_dir().join(format!("ersatzrs-hls-test-{}", Uuid::new_v4()));
        tokio::fs::create_dir_all(&working_dir)
            .await
            .expect("create temp dir");
        tokio::fs::write(working_dir.join("1760873990_init.mp4"), b"same-init")
            .await
            .expect("write earliest init");
        tokio::fs::write(working_dir.join("1760874000_init.mp4"), b"same-init")
            .await
            .expect("write stale duplicate init");
        tokio::fs::write(working_dir.join("1760874042_init.mp4"), b"same-init")
            .await
            .expect("write future duplicate init");
        tokio::fs::write(working_dir.join("live_1760874038_000001.m4s"), b"segment")
            .await
            .expect("write retained segment");

        let mut init_cache = HlsInitSegmentCache::default();
        init_cache
            .add_segment(&working_dir.join("1760873990_init.mp4"))
            .await
            .expect("cache earliest init");
        init_cache
            .add_segment(&working_dir.join("1760874000_init.mp4"))
            .await
            .expect("cache stale duplicate init");
        init_cache
            .add_segment(&working_dir.join("1760874042_init.mp4"))
            .await
            .expect("cache future duplicate init");

        let trim_result = test_trim_result(
            1,
            Some(1_760_874_038),
            ["1760873990_init.mp4", "live_1760874038_000001.m4s"],
        );

        delete_old_segments(&working_dir, &trim_result, &mut init_cache)
            .await
            .expect("delete old segments");

        assert!(working_dir.join("1760873990_init.mp4").exists());
        assert!(!working_dir.join("1760874000_init.mp4").exists());
        assert!(working_dir.join("1760874042_init.mp4").exists());
        assert!(working_dir.join("live_1760874038_000001.m4s").exists());

        let _ = tokio::fs::remove_dir_all(&working_dir).await;
    }

    #[tokio::test]
    async fn delete_old_segments_keeps_work_ahead_segments_after_playlist_window() {
        let working_dir =
            std::env::temp_dir().join(format!("ersatzrs-hls-test-{}", Uuid::new_v4()));
        tokio::fs::create_dir_all(&working_dir)
            .await
            .expect("create temp dir");
        tokio::fs::write(working_dir.join("live000001.ts"), b"old")
            .await
            .expect("write old segment");
        tokio::fs::write(working_dir.join("live000002.ts"), b"keep")
            .await
            .expect("write retained segment");
        tokio::fs::write(working_dir.join("live000003.ts"), b"future")
            .await
            .expect("write future segment");

        let trim_result = test_trim_result(2, None, ["live000002.ts"]);
        let mut init_cache = HlsInitSegmentCache::default();
        delete_old_segments(&working_dir, &trim_result, &mut init_cache)
            .await
            .expect("delete old segments");

        assert!(!working_dir.join("live000001.ts").exists());
        assert!(working_dir.join("live000002.ts").exists());
        assert!(working_dir.join("live000003.ts").exists());

        let _ = tokio::fs::remove_dir_all(&working_dir).await;
    }

    fn test_trim_result<const N: usize>(
        sequence: u64,
        generated_at: Option<i64>,
        referenced_files: [&str; N],
    ) -> TrimmedPlaylist {
        TrimmedPlaylist {
            playlist: String::new(),
            referenced_files: referenced_files.into_iter().map(str::to_owned).collect(),
            sequence,
            generated_at,
        }
    }
}
