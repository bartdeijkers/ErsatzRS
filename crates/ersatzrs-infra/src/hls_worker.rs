mod output_status;
mod producer_playlist;

#[path = "hls_worker_external.rs"]
mod external;
#[path = "hls_worker_source_caption.rs"]
mod source_caption;

use crate::repositories::SqlxResultExt as _;
use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, AtomicI32, AtomicI64, AtomicU64, Ordering},
    Arc,
};
use std::time::Duration;

use crate::addons::AddonRuntimeContext;
use crate::channel_playout::load_channel_playout_context;
use crate::db::DbPool;
use crate::repositories::common::{
    encode_datetime, map_data_access_error, try_decode_duration_text,
};
use async_trait::async_trait;
use chrono::{DateTime, NaiveDateTime, Timelike, Utc};
use ersatzrs_core::hls::{
    HlsAdmissionStatus, HlsSessionHandle, HlsSessionRegistry, HlsStartupReadiness,
};
use ersatzrs_core::path_replacement::{replace_path, PathPlatform, PathReplacement};
use ersatzrs_core::repositories::{RepositoryError, Result as RepositoryResult};
use ersatzrs_domain::{
    Channel, ChannelMusicVideoCreditsMode, ChannelStreamSelectorMode, ChannelSubtitleMode,
    FFmpegProfileAudioFormat, FFmpegProfileBitDepth, FFmpegProfileVideoFormat,
    HardwareAccelerationKind, StreamingMode, VideoScanKind,
};
use ersatzrs_ffmpeg::{
    build_pipeline_for_acceleration, ffmpeg_path, ffprobe_path, DesiredVideoState,
    FfmpegCapabilityReport, FfmpegCommand, FfmpegRuntimeReport, FrameRate, FrameSize,
    GraphicsPipelineOverlay, HlsOutput, InputFile, InputKind, OutputFormat, OutputMetadata,
    PipelineRequest, RealtimeMode, SourceAudioCodec, SourceAudioStream, SourceVideoCodec,
    SourceVideoStream, SubtitleMethod, WatermarkOpacityClock,
};

use crate::dynamic_text::shutdown_dynamic_text_controllers;
use crate::ffmpeg_capabilities::learned_hardware_decode_denial;
use crate::ffmpeg_config::playback_language_preferences;
use crate::hls_delivery::{
    clear_hls_delivery_fallback, load_hls_delivery_fallback, persist_hls_delivery_fallback,
    persist_hls_delivery_observation, HlsDeliveryObservation, HlsDeliverySoftwareScope,
    HlsDeliveryState,
};
use crate::playback::{
    apply_automatic_null_audio, enrich_source_video_stream_for_playback,
    load_channel_playback_profile, load_channel_target_frame_rate, load_source_audio_streams,
    load_source_video_stream_by_index, render_playback_graphics_overlays,
    select_channel_playback_subtitle, select_custom_playback_streams,
    select_pinned_playback_subtitle, select_playback_audio_stream_index, select_playback_graphics,
    select_playback_watermarks, subtitle_fonts_dir, subtitle_input_for_stream,
    watermark_inputs_for_stream, CustomStreamSelection, PlaybackAudioSelection,
    PlaybackGraphicsArtifacts, PlaybackGraphicsRenderContext, PlaybackProfile, PlaybackSubtitle,
};
use crate::remote_stream::{
    decode_persisted_liveness, redact_remote_stream_output, redacted_source_label,
    remote_stream_input_file, remote_stream_playback_start, remote_stream_producer,
    remote_stream_video_height_ceiling, resolve_remote_stream_with_addons,
    spawn_ffmpeg_with_remote_input, with_remote_stream_video_height_ceiling, RemoteStreamCommand,
    RemoteStreamDefinition, RemoteStreamProcess, RemoteStreamSpawnContext, ResolvedRemoteStream,
    REMOTE_STREAM_DEFAULT_MAX_VIDEO_HEIGHT, REMOTE_STREAM_FALLBACK_MAX_VIDEO_HEIGHT,
};
use crate::subtitles::rebase_burned_text_subtitle_input;
use crate::transcode_capacity::{
    load_active_transcode_limit, promote_hls_transcode_to_realtime,
    request_active_transcode_reclaim, try_reserve_active_transcode,
    try_reserve_wrapper_and_producer_capacity, ActiveTranscodeKind, ActiveTranscodeReclaimHandle,
    ActiveTranscodeReservation, ActiveTranscodeSlot,
};
use sha2::{Digest, Sha256};
use sqlx::Row;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::sync::{Mutex, Notify, RwLock};
use tokio::time::MissedTickBehavior;

const PLAYLIST_MAX_SEGMENTS: usize = 10;
const PLAYLIST_DELETE_INTERVAL_MS: i64 = 30_000;
const PLAYLIST_FILTER_BEFORE_SECONDS: i64 = 30;
const SEGMENT_CLEANUP_INTERVAL: Duration = Duration::from_secs(5);
const FFMPEG_STDERR_TAIL_BYTES: usize = 16 * 1024;
use ersatzrs_core::hls::HLS_WORK_AHEAD_BUFFER_SECONDS;
const HLS_REALTIME_BUFFER_SECONDS: i64 = 30;
const HLS_WORK_AHEAD_TRANSCODE_SECONDS: u64 = 44;
const HLS_REMOTE_INITIAL_BURST_SECONDS: u64 = HLS_WORK_AHEAD_TRANSCODE_SECONDS;
const HLS_REMOTE_PROMOTION_BUFFER_SECONDS: u64 = 30;
const HLS_ACTIVE_WATCH_WINDOW: Duration = Duration::from_secs(30);
const HLS_STALE_WATCHED_PREEMPTION_THRESHOLD: Duration = Duration::from_secs(25);
const HLS_ADMISSION_RETRY_INTERVAL: Duration = Duration::from_millis(250);
const HLS_RECLAIMED_REACQUIRE_DELAY: Duration = Duration::from_millis(500);
const HLS_AT_CAPACITY_SLATE_DURATION: Duration = Duration::from_secs(4);
const HLS_DELIVERY_WINDOW: Duration = Duration::from_mins(1);
const HLS_DELIVERY_FAST_WINDOW: Duration = Duration::from_secs(20);
const HLS_DELIVERY_STARTUP_TIMEOUT: Duration = Duration::from_secs(20);
const HLS_DELIVERY_NO_PROGRESS_TIMEOUT: Duration = Duration::from_secs(12);
const HLS_DELIVERY_FAST_MIN_RATIO: f64 = 0.90;
const HLS_DELIVERY_MIN_RATIO: f64 = 0.98;
pub(crate) const DEFAULT_WORK_AHEAD_SEGMENTER_LIMIT: i32 = 1;
const TS_PACKET_SIZE: usize = 188;
const TS_SYNC_BYTE: u8 = 0x47;
const TS_NULL_PID: u16 = 0x1fff;
const PCR_TICKS_PER_SECOND: i128 = 27_000_000;
const PCR_MODULUS_TICKS: i128 = (1_i128 << 33) * 300;
const PCR_LEAP_TOLERANCE_TICKS: i128 = 27_000_000;

static HLS_WORK_AHEAD_COUNT: AtomicI32 = AtomicI32::new(0);

const ERROR_BACKGROUND_PNG: &[u8] = include_bytes!("../../../assets/_background.png");
// Source dimensions belong to the embedded PNG, not the selected output profile.
const ERROR_BACKGROUND_FRAME_SIZE: FrameSize = FrameSize::new(1920, 1080);
pub const ALL_STREAMS_IN_USE_SLATE_MESSAGE: &str =
    "All streams are in use. Playback will start when a stream is available.";

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
    SourceCaptionUnavailable,
    Reclaimed,
    DeliverySoftwareFallback {
        detected_at: DateTime<Utc>,
        max_video_height: u16,
        software_video_scope: HlsDeliverySoftwareScope,
    },
    DeliveryFallback {
        detected_at: DateTime<Utc>,
        software_video_scope: Option<HlsDeliverySoftwareScope>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct HlsDeliverySegment {
    sequence: u64,
    duration: Duration,
}

#[derive(Debug)]
struct HlsDeliveryTracker {
    last_sequence: Option<u64>,
    window_started_at: Duration,
    media_in_window: Duration,
    max_segment_duration: Duration,
    last_progress_at: Option<Duration>,
    under_run_active: bool,
    last_under_run_at: Option<DateTime<Utc>>,
}

impl HlsDeliveryTracker {
    fn new(last_sequence: Option<u64>, active_under_run_at: Option<DateTime<Utc>>) -> Self {
        Self {
            last_sequence,
            window_started_at: Duration::ZERO,
            media_in_window: Duration::ZERO,
            max_segment_duration: Duration::ZERO,
            last_progress_at: None,
            under_run_active: active_under_run_at.is_some(),
            last_under_run_at: active_under_run_at,
        }
    }

    fn observe(
        &mut self,
        elapsed: Duration,
        segments: &[HlsDeliverySegment],
        observed_at: DateTime<Utc>,
    ) -> Option<HlsDeliveryObservation> {
        let mut progressed = false;
        for segment in segments {
            if self
                .last_sequence
                .is_none_or(|sequence| segment.sequence > sequence)
            {
                self.media_in_window = self.media_in_window.saturating_add(segment.duration);
                self.max_segment_duration = self.max_segment_duration.max(segment.duration);
                self.last_sequence = Some(segment.sequence);
                progressed = true;
            }
        }
        if progressed {
            self.last_progress_at = Some(elapsed);
        }

        let wall = elapsed.saturating_sub(self.window_started_at);
        let no_progress = self
            .last_progress_at
            .map_or(elapsed, |progress_at| elapsed.saturating_sub(progress_at));
        let startup_stalled =
            self.last_progress_at.is_none() && elapsed >= HLS_DELIVERY_STARTUP_TIMEOUT;
        let delivery_stalled =
            self.last_progress_at.is_some() && no_progress >= HLS_DELIVERY_NO_PROGRESS_TIMEOUT;
        let window_complete = wall >= HLS_DELIVERY_WINDOW;
        let ratio = if wall.is_zero() {
            1.0
        } else {
            self.media_in_window.as_secs_f64() / wall.as_secs_f64()
        };
        let media_seconds = self.media_in_window.as_secs_f64();
        // One segment of allowance prevents a healthy segmented stream from
        // failing solely because the observation tick landed just before the
        // next playlist update.
        let fast_window_under_run = wall >= HLS_DELIVERY_FAST_WINDOW
            && (self.media_in_window + self.max_segment_duration).as_secs_f64()
                < wall.as_secs_f64() * HLS_DELIVERY_FAST_MIN_RATIO;
        let window_under_run = window_complete
            && (self.media_in_window + self.max_segment_duration).as_secs_f64()
                < wall.as_secs_f64() * HLS_DELIVERY_MIN_RATIO;
        let under_run =
            startup_stalled || delivery_stalled || fast_window_under_run || window_under_run;

        let update = if under_run && !self.under_run_active {
            self.under_run_active = true;
            self.last_under_run_at = Some(observed_at);
            Some((HlsDeliveryState::UnderRun, true))
        } else if window_complete && under_run {
            self.last_under_run_at = Some(observed_at);
            Some((HlsDeliveryState::UnderRun, false))
        } else if window_complete && !under_run && self.under_run_active {
            self.under_run_active = false;
            Some((HlsDeliveryState::Recovered, true))
        } else {
            None
        };

        if window_complete {
            self.window_started_at = elapsed;
            self.media_in_window = Duration::ZERO;
            self.max_segment_duration = Duration::ZERO;
        }

        update.map(|(state, transitioned)| HlsDeliveryObservation {
            transitioned,
            observed_at,
            state,
            last_under_run_at: self.last_under_run_at.unwrap_or(observed_at),
            wall_seconds: wall.as_secs_f64(),
            media_seconds,
            delivery_ratio_percent: ratio * 100.0,
            no_progress_seconds: no_progress.as_secs_f64(),
        })
    }
}

struct HlsProcessCapacity<'a> {
    completed_media_duration: Option<&'a std::sync::Mutex<Option<Duration>>>,
    producer_playlist: Option<&'a producer_playlist::MpegTsProducerPlaylist>,
    song: Option<&'a crate::song_background::NativeSongBackground>,
    active_transcode_slot_id: Option<u64>,
    work_ahead_reservation: Option<&'a mut Option<WorkAheadReservation>>,
    delivery_pool: Option<&'a DbPool>,
    delivery_recovery: Option<HlsDeliveryRecovery>,
    source_caption: Option<(
        Arc<crate::playback::source_teletext::runtime::PlaybackSource>,
        crate::transcode_capacity::ActiveTranscodeReservation,
        Option<ersatzrs_ffmpeg::SourceTeletextInputPlan>,
        crate::playback::source_teletext::runtime::SourceTeletextConsumerRequest,
    )>,
}

#[derive(Default)]
struct RetainedHlsFailure {
    graph: Option<ersatzrs_ffmpeg::GraphFailureKind>,
    decode: Option<ersatzrs_ffmpeg::HardwareDecodeDenial>,
    media_published: bool,
}

struct RetainedHlsProcess {
    prepared: crate::remote_stream::PreparedRemoteStreamInput,
    failure: Arc<std::sync::Mutex<RetainedHlsFailure>>,
    graph: Option<ersatzrs_ffmpeg::HardwareGraphOperation>,
    source: Option<SourceVideoStream>,
    acceleration: HardwareAccelerationKind,
    deadline: tokio::time::Instant,
    remaining: Duration,
}

#[derive(Debug, Clone, Copy)]
struct HlsDeliveryRecovery {
    media_item_id: i64,
    max_video_height: u16,
    supports_quality_fallback: bool,
    software_video_scope: Option<HlsDeliverySoftwareScope>,
    available_software_video_scope: Option<HlsDeliverySoftwareScope>,
    fallback_detected_at: Option<DateTime<Utc>>,
    fallback_verified: bool,
}

#[derive(Debug, Clone, Copy)]
struct HlsDeliveryAdaptation {
    detected_at: DateTime<Utc>,
    max_video_height: u16,
    software_video_scope: Option<HlsDeliverySoftwareScope>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HlsDeliveryRecoveryAction {
    None,
    RestartWithSoftware(HlsDeliverySoftwareScope),
    RestartAtFallback,
    VerifyFallback,
    ClearFallback,
}

fn hls_delivery_recovery_action(
    recovery: HlsDeliveryRecovery,
    observation: &HlsDeliveryObservation,
    fallback_verified: bool,
) -> HlsDeliveryRecoveryAction {
    if observation.state == HlsDeliveryState::UnderRun
        && observation.transitioned
        && recovery.software_video_scope.is_none()
    {
        if let Some(scope) = recovery.available_software_video_scope {
            return HlsDeliveryRecoveryAction::RestartWithSoftware(scope);
        }
    }
    if observation.state == HlsDeliveryState::UnderRun
        && (observation.transitioned || recovery.software_video_scope.is_some())
        && recovery.supports_quality_fallback
        && recovery.max_video_height > REMOTE_STREAM_FALLBACK_MAX_VIDEO_HEIGHT
    {
        return HlsDeliveryRecoveryAction::RestartAtFallback;
    }
    if observation.state == HlsDeliveryState::UnderRun
        && (recovery.max_video_height == REMOTE_STREAM_FALLBACK_MAX_VIDEO_HEIGHT
            || recovery.software_video_scope.is_some())
    {
        return HlsDeliveryRecoveryAction::ClearFallback;
    }
    if observation.state == HlsDeliveryState::Recovered
        && observation.transitioned
        && !fallback_verified
        && (recovery.max_video_height == REMOTE_STREAM_FALLBACK_MAX_VIDEO_HEIGHT
            || recovery.software_video_scope.is_some())
    {
        return HlsDeliveryRecoveryAction::VerifyFallback;
    }
    HlsDeliveryRecoveryAction::None
}

fn pending_hls_delivery_fallback_detected_at(
    recovery: Option<HlsDeliveryRecovery>,
) -> Option<DateTime<Utc>> {
    recovery
        .filter(|recovery| !recovery.fallback_verified)
        .and_then(|recovery| recovery.fallback_detected_at)
}

/// Concrete HLS segmenter worker backed by `FFmpeg` CLI output in a temp folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HlsPlaybackProfileOverride {
    /// Force browser-decodable H.264 8-bit video and AAC audio while retaining
    /// resolution, frame rate, stream selection, overlays, and processing.
    BrowserCompatible,
}

/// Per-session identity and output options for an HLS worker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HlsSessionOptions {
    pub registry_key: String,
    /// Identity used by the process-wide transcode-capacity registry.
    ///
    /// This may differ from the channel number for an isolated derivative
    /// session such as an administrator browser preview.
    pub capacity_key: String,
    pub working_dir: PathBuf,
    pub playback_profile_override: Option<HlsPlaybackProfileOverride>,
}

impl HlsSessionOptions {
    #[must_use]
    pub fn standard(channel_number: &str, working_dir: PathBuf) -> Self {
        Self {
            registry_key: channel_number.to_owned(),
            capacity_key: channel_number.to_owned(),
            working_dir,
            playback_profile_override: None,
        }
    }
}

pub struct HlsSessionWorker {
    channel_number: String,
    channel_id: i64,
    registry_key: String,
    capacity_key: String,
    working_dir: PathBuf,
    playback_profile_override: Option<HlsPlaybackProfileOverride>,
    output_format: OutputFormat,
    path_replacements: Vec<PathReplacement>,
    ffmpeg_runtime: Arc<RwLock<FfmpegRuntimeReport>>,
    media_relay: std::sync::OnceLock<Arc<crate::next_engine::media::MediaRelay>>,
    source_teletext_locker: std::sync::OnceLock<ersatzrs_core::locking::EntityLocker>,
    output_status: std::sync::Mutex<output_status::SegmentOutputs>,
    output_error: std::sync::Mutex<Option<String>>,
    startup: std::sync::Mutex<HlsStartupReadiness>,
    startup_minimum_segments: std::sync::atomic::AtomicUsize,
    startup_changed: Notify,
    last_access_ms: Arc<AtomicI64>,
    started_at_ms: AtomicI64,
    transcoded_until_ms: AtomicI64,
    last_delete_ms: Arc<AtomicI64>,
    working_dir_ready: AtomicBool,
    state: AtomicI32,
    admission_status: AtomicI32,
    reclaim_requested_slot_id: Arc<AtomicU64>,
    reclaim_reacquire_deferred_until_ms: Arc<AtomicI64>,
    playlist_start: Mutex<DateTime<Utc>>,
    ts_continuity: Mutex<TsContinuityState>,
    init_segment_cache: Mutex<HlsInitSegmentCache>,
    cancel: Arc<Notify>,
    reclaim_transcode: Arc<Notify>,
}

impl HlsSessionWorker {
    #[must_use]
    pub fn new(
        channel_number: String,
        channel_id: i64,
        working_dir: PathBuf,
        output_format: OutputFormat,
        path_replacements: Vec<PathReplacement>,
        ffmpeg_runtime: Arc<RwLock<FfmpegRuntimeReport>>,
    ) -> Arc<Self> {
        let options = HlsSessionOptions::standard(&channel_number, working_dir);
        Self::new_with_options(
            channel_number,
            channel_id,
            options,
            output_format,
            path_replacements,
            ffmpeg_runtime,
        )
    }

    #[must_use]
    pub fn new_with_options(
        channel_number: String,
        channel_id: i64,
        options: HlsSessionOptions,
        output_format: OutputFormat,
        path_replacements: Vec<PathReplacement>,
        ffmpeg_runtime: Arc<RwLock<FfmpegRuntimeReport>>,
    ) -> Arc<Self> {
        Arc::new(Self {
            channel_number,
            channel_id,
            registry_key: options.registry_key,
            capacity_key: options.capacity_key,
            working_dir: options.working_dir,
            playback_profile_override: options.playback_profile_override,
            output_format,
            path_replacements,
            ffmpeg_runtime,
            media_relay: std::sync::OnceLock::new(),
            source_teletext_locker: std::sync::OnceLock::new(),
            output_status: std::sync::Mutex::default(),
            output_error: std::sync::Mutex::default(),
            startup: std::sync::Mutex::new(HlsStartupReadiness::Legacy),
            startup_minimum_segments: std::sync::atomic::AtomicUsize::new(3),
            startup_changed: Notify::new(),
            last_access_ms: Arc::new(AtomicI64::new(0)),
            started_at_ms: AtomicI64::new(Utc::now().timestamp_millis()),
            transcoded_until_ms: AtomicI64::new(i64::MIN),
            last_delete_ms: Arc::new(AtomicI64::new(0)),
            working_dir_ready: AtomicBool::new(false),
            state: AtomicI32::new(HlsSessionState::SeekAndRealtime.as_i32()),
            admission_status: AtomicI32::new(HlsAdmissionStatus::Waiting.as_i32()),
            reclaim_requested_slot_id: Arc::new(AtomicU64::new(0)),
            reclaim_reacquire_deferred_until_ms: Arc::new(AtomicI64::new(0)),
            playlist_start: Mutex::new(Utc::now()),
            ts_continuity: Mutex::new(TsContinuityState::default()),
            init_segment_cache: Mutex::new(HlsInitSegmentCache::default()),
            cancel: Arc::new(Notify::new()),
            reclaim_transcode: Arc::new(Notify::new()),
        })
    }

    fn report_transcoded_until(&self, until: DateTime<Utc>) {
        self.transcoded_until_ms
            .store(until.timestamp_millis(), Ordering::Relaxed);
    }

    fn session_state(&self) -> HlsSessionState {
        HlsSessionState::from_i32(self.state.load(Ordering::Relaxed))
    }

    /// Set the first request's deadline before publishing a new worker handle.
    pub fn initialize_startup(&self, deadline: std::time::Instant, minimum_segments: usize) {
        self.startup_minimum_segments
            .store(minimum_segments.max(3), Ordering::Relaxed);
        self.set_startup_readiness(HlsStartupReadiness::Pending { deadline });
    }

    fn set_startup_readiness(&self, next: HlsStartupReadiness) {
        if let Ok(mut state) = self.startup.lock() {
            if matches!(*state, HlsStartupReadiness::Failed) {
                return;
            }
            *state = match (*state, next) {
                (HlsStartupReadiness::Pending { deadline }, HlsStartupReadiness::Ready)
                    if std::time::Instant::now() >= deadline =>
                {
                    HlsStartupReadiness::Failed
                }
                (_, next) => next,
            };
        }
        self.startup_changed.notify_one();
    }

    async fn wait_startup_deadline(&self) {
        match self.startup_readiness() {
            HlsStartupReadiness::Pending { deadline } => {
                tokio::time::sleep_until(deadline.into()).await;
            }
            HlsStartupReadiness::Failed => (),
            HlsStartupReadiness::Ready | HlsStartupReadiness::Legacy => {
                std::future::pending::<()>().await;
            }
        }
    }

    async fn startup_read<T>(
        &self,
        read: impl std::future::Future<Output = RepositoryResult<T>>,
    ) -> RepositoryResult<T> {
        tokio::select! {
            biased;
            () = self.cancel.notified() => {
                // Reads can repeatedly fail before a transcode process exists.
                // Consume cancellation here and stop the retry loop as well.
                self.set_startup_readiness(HlsStartupReadiness::Failed);
                Err(RepositoryError::Conflict("HLS startup cancelled".into()))
            },
            () = self.wait_startup_deadline() => Err(RepositoryError::Conflict("HLS startup deadline expired".into())),
            result = read => result,
        }
    }

    fn set_session_state(&self, state: HlsSessionState) {
        self.state.store(state.as_i32(), Ordering::Relaxed);
    }

    fn set_admission_status(&self, status: HlsAdmissionStatus) {
        self.admission_status
            .store(status.as_i32(), Ordering::Relaxed);
    }

    async fn load_playback_profile(
        &self,
        pool: &DbPool,
    ) -> RepositoryResult<Option<PlaybackProfile>> {
        Ok(load_channel_playback_profile(pool, self.channel_id)
            .await?
            .map(|profile| match self.playback_profile_override {
                Some(override_kind) => apply_playback_profile_override(profile, override_kind),
                None => profile,
            }))
    }

    async fn runtime_capabilities(&self) -> FfmpegCapabilityReport {
        self.ffmpeg_runtime.read().await.capabilities.clone()
    }

    pub fn start(
        self: Arc<Self>,
        pool: DbPool,
        registry: Arc<dyn HlsSessionRegistry>,
        idle_timeout: Option<Duration>,
    ) {
        self.start_with_reservation(pool, registry, idle_timeout, None);
    }

    /// Reserve a Hybrid wrapper and this worker's first realtime producer
    /// without retaining a partial admission. Only start the producer after
    /// this worker wins insertion into its shared session registry.
    #[must_use]
    pub fn try_reserve_wrapper_capacity(
        &self,
        limit: i32,
        idle_timeout: Option<Duration>,
    ) -> Option<(ActiveTranscodeReservation, ActiveTranscodeReservation)> {
        try_reserve_wrapper_and_producer_capacity(
            limit,
            &self.channel_number,
            self.active_transcode_slot(ActiveTranscodeKind::HlsRealtime, idle_timeout),
        )
    }

    fn active_transcode_slot(
        &self,
        kind: ActiveTranscodeKind,
        idle_timeout: Option<Duration>,
    ) -> ActiveTranscodeSlot {
        ActiveTranscodeSlot::new_hls_with_capacity_key(
            &self.channel_number,
            &self.capacity_key,
            kind,
            Arc::clone(&self.last_access_ms),
            ActiveTranscodeReclaimHandle::new(
                Arc::clone(&self.reclaim_transcode),
                Arc::clone(&self.reclaim_requested_slot_id),
                Arc::clone(&self.reclaim_reacquire_deferred_until_ms),
                idle_timeout.is_some(),
            ),
        )
    }

    /// Move an atomically admitted first producer into the worker task. The
    /// task owns its lease through startup, cancellation and process cleanup.
    pub fn start_with_initial_reservation(
        self: Arc<Self>,
        pool: DbPool,
        registry: Arc<dyn HlsSessionRegistry>,
        idle_timeout: Option<Duration>,
        initial_reservation: ActiveTranscodeReservation,
    ) {
        self.start_with_reservation(pool, registry, idle_timeout, Some(initial_reservation));
    }

    fn start_with_reservation(
        self: Arc<Self>,
        pool: DbPool,
        registry: Arc<dyn HlsSessionRegistry>,
        idle_timeout: Option<Duration>,
        initial_reservation: Option<ActiveTranscodeReservation>,
    ) {
        tokio::spawn(async move {
            let operation = self
                .clone()
                .run(pool, registry, idle_timeout, initial_reservation);
            tokio::pin!(operation);
            loop {
                tokio::select! {
                    () = &mut operation => {
                        if matches!(self.startup_readiness(), HlsStartupReadiness::Pending { .. } | HlsStartupReadiness::Failed) { self.set_startup_readiness(HlsStartupReadiness::Failed); }
                        break;
                    },
                    () = self.startup_changed.notified() => {},
                    () = self.wait_startup_deadline() => {
                        if matches!(self.startup_readiness(), HlsStartupReadiness::Ready | HlsStartupReadiness::Legacy) { continue; }
                        self.set_startup_readiness(HlsStartupReadiness::Failed);
                        self.cancel.notify_one();
                        operation.await;
                        break;
                    }
                }
            }
        });
    }

    #[allow(clippy::too_many_lines)]
    async fn run(
        self: Arc<Self>,
        pool: DbPool,
        registry: Arc<dyn HlsSessionRegistry>,
        idle_timeout: Option<Duration>,
        mut initial_reservation: Option<ActiveTranscodeReservation>,
    ) {
        if let Err(error) = self
            .startup_read(async {
                prepare_working_dir(&self.working_dir).await.map_err(|_| {
                    RepositoryError::unavailable("HLS startup", "working directory unavailable")
                })
            })
            .await
        {
            tracing::warn!(%error, working_dir = %self.working_dir.display(), "failed to prepare hls working directory");
            registry
                .remove_if_current(&(self.clone() as Arc<dyn HlsSessionHandle>))
                .await;
            return;
        }
        self.working_dir_ready.store(true, Ordering::Release);
        self.touch();

        let mut is_first = true;
        let started_with_initial_reservation = initial_reservation.is_some();
        let mut last_playout_item_id = None::<(i64, DateTime<Utc>)>;
        let channel_start_time = Utc::now();
        self.started_at_ms
            .store(channel_start_time.timestamp_millis(), Ordering::Relaxed);
        let mut transcoded_until = channel_start_time;
        self.report_transcoded_until(transcoded_until);
        let mut next_mpegts_output_ts_offset = None::<Duration>;
        *self.playlist_start.lock().await = transcoded_until;
        let mut learned_decode_retry_used = false;
        // ADR 0014 allows exactly one graph retry per session, mirroring the
        // decode-denial bound above.
        let mut learned_graph_retry_used = false;
        let mut source_caption_fallback = None::<source_caption::CaptionOccurrence>;
        let mut interval_startup: Option<(i64, tokio::time::Instant)> = None;
        let mut startup_classified = false;
        let mut startup_attempt_started_at = match self.startup_readiness() {
            HlsStartupReadiness::Pending { deadline } => {
                Some(tokio::time::Instant::from_std(deadline) - Duration::from_secs(20))
            }
            _ => None,
        };
        let mut delivery_adaptations = HashMap::<i64, HlsDeliveryAdaptation>::new();
        let slug_duration = match self
            .startup_read(load_channel_slug_seconds(&pool, self.channel_id))
            .await
        {
            Ok(seconds) => seconds
                .filter(|seconds| *seconds > 0.0)
                .map(Duration::from_secs_f64),
            Err(error) => {
                tracing::error!(
                    %error,
                    channel_id = self.channel_id,
                    channel_number = %self.channel_number,
                    "failed to load channel slug configuration; stopping hls worker"
                );
                let _ = cleanup_working_dir(&self.working_dir).await;
                registry
                    .remove_if_current(&(self.clone() as Arc<dyn HlsSessionHandle>))
                    .await;
                return;
            }
        };
        let has_slug = slug_duration.is_some();
        if let Err(error) = self
            .startup_read(async {
                time_shift_on_demand_playout(&pool, self.channel_id, transcoded_until)
                    .await
                    .map_err(|_| {
                        RepositoryError::unavailable(
                            "HLS on-demand startup",
                            "database update failed",
                        )
                    })
            })
            .await
        {
            tracing::warn!(
                %error,
                channel_number = %self.channel_number,
                "failed to time shift on-demand playout before starting hls segmenter"
            );
        }
        let initial_work_ahead_limit =
            match self.startup_read(load_hls_work_ahead_limit(&pool)).await {
                Ok(limit) => limit.unwrap_or(DEFAULT_WORK_AHEAD_SEGMENTER_LIMIT),
                Err(error) => {
                    tracing::error!(
                        %error,
                        channel_id = self.channel_id,
                        channel_number = %self.channel_number,
                        "failed to load HLS work-ahead limit; stopping hls worker"
                    );
                    let _ = cleanup_working_dir(&self.working_dir).await;
                    registry
                        .remove_if_current(&(self.clone() as Arc<dyn HlsSessionHandle>))
                        .await;
                    return;
                }
            };
        let initial_work_ahead =
            HLS_WORK_AHEAD_COUNT.load(Ordering::Relaxed) < initial_work_ahead_limit;
        self.set_session_state(if initial_work_ahead {
            HlsSessionState::SeekAndWorkAhead
        } else {
            HlsSessionState::SeekAndRealtime
        });
        let mut cleanup_interval = tokio::time::interval(SEGMENT_CLEANUP_INTERVAL);
        cleanup_interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
        self.set_admission_status(HlsAdmissionStatus::Waiting);
        loop {
            if let Some(timeout) = idle_timeout {
                if self.idle_for_duration() >= timeout {
                    break;
                }
            }

            let now = Utc::now();
            if !should_transcode_for_buffer(transcoded_until, now) {
                self.set_admission_status(HlsAdmissionStatus::Waiting);
                let _ = self.trim_playlist_inner().await;
                tokio::time::sleep(SEGMENT_CLEANUP_INTERVAL).await;
                continue;
            }

            let realtime_by_buffer = should_use_realtime(transcoded_until, now);
            let transcode_state = self.session_state();
            let transcode_cursor =
                hls_transcode_request_time(transcode_state, transcoded_until, now);
            if matches!(self.startup_readiness(), HlsStartupReadiness::Failed) {
                break;
            }
            if !startup_classified
                && matches!(
                    self.startup_readiness(),
                    HlsStartupReadiness::Pending { .. }
                )
            {
                match self
                    .startup_read(external::retained(
                        &pool,
                        self.channel_id,
                        &self.path_replacements,
                        transcode_cursor,
                    ))
                    .await
                {
                    Ok(Some(retained)) => {
                        startup_classified = true;
                        if !retained {
                            self.set_startup_readiness(HlsStartupReadiness::Legacy);
                        }
                    }
                    Ok(None) => {}
                    Err(_) => break,
                }
            }
            let live_remote_stream = match self
                .startup_read(current_remote_stream_is_live(
                    &pool,
                    self.channel_id,
                    transcode_cursor,
                ))
                .await
            {
                Ok(is_live) => is_live,
                Err(error) => {
                    tracing::error!(
                        %error,
                        channel_id = self.channel_id,
                        channel_number = %self.channel_number,
                        "failed to determine live Remote Stream state; stopping hls worker"
                    );
                    break;
                }
            };
            // A live source is never opened at a future work-ahead cursor. Keep
            // the session responsive to cancellation while waiting for its
            // scheduled wall-clock boundary, then admit it in realtime.
            if let Some(delay) =
                live_source_admission_delay(live_remote_stream, transcode_cursor, now)
            {
                self.set_admission_status(HlsAdmissionStatus::Waiting);
                self.set_session_state(self.session_state().throttle());
                let _ = self.trim_playlist_inner().await;
                tokio::time::sleep(delay.min(Duration::from_secs(1))).await;
                continue;
            }
            let initial_bootstrap = started_with_initial_reservation && is_first;
            let force_realtime = realtime_by_buffer || live_remote_stream || initial_bootstrap;
            let recently_accessed = self.was_recently_accessed();
            if idle_timeout.is_some() && !recently_accessed {
                self.set_admission_status(HlsAdmissionStatus::Waiting);
                self.set_session_state(self.session_state().throttle());
                let _ = self.trim_playlist_inner().await;
                tokio::time::sleep(SEGMENT_CLEANUP_INTERVAL).await;
                continue;
            }

            if let Some(delay) = self.active_transcode_reacquire_delay() {
                self.set_admission_status(HlsAdmissionStatus::Waiting);
                self.set_session_state(self.session_state().throttle());
                let _ = self.trim_playlist_inner().await;
                tokio::time::sleep(delay).await;
                continue;
            }

            let mut work_ahead_reservation = if force_realtime {
                None
            } else {
                let work_ahead_limit =
                    match self.startup_read(load_hls_work_ahead_limit(&pool)).await {
                        Ok(limit) => limit.unwrap_or(DEFAULT_WORK_AHEAD_SEGMENTER_LIMIT),
                        Err(error) => {
                            tracing::error!(
                                %error,
                                channel_id = self.channel_id,
                                channel_number = %self.channel_number,
                                "failed to reload HLS work-ahead limit; stopping hls worker"
                            );
                            break;
                        }
                    };
                try_reserve_work_ahead(work_ahead_limit)
            };
            let realtime = force_realtime || work_ahead_reservation.is_none();
            let active_transcode_kind = if realtime {
                ActiveTranscodeKind::HlsRealtime
            } else {
                ActiveTranscodeKind::HlsWorkAhead
            };
            let startup_started_at =
                *startup_attempt_started_at.get_or_insert_with(tokio::time::Instant::now);
            let active_limit = match self.startup_read(load_active_transcode_limit(&pool)).await {
                Ok(limit) => {
                    limit.unwrap_or(crate::transcode_capacity::DEFAULT_ACTIVE_TRANSCODE_LIMIT)
                }
                Err(error) => {
                    tracing::error!(
                        %error,
                        channel_id = self.channel_id,
                        channel_number = %self.channel_number,
                        "failed to load active transcode limit; stopping hls worker"
                    );
                    break;
                }
            };
            let active_transcode_reservation = initial_reservation.take().or_else(|| {
                try_reserve_active_transcode(
                    active_limit,
                    self.active_transcode_slot(active_transcode_kind, idle_timeout),
                )
            });
            let Some(active_transcode_reservation) = active_transcode_reservation else {
                // Frozen interval startup includes admission wait, but never
                // starts a resolver or capacity-slate encoder without capacity.
                let retained_wait = self
                    .startup_read(external::retained(
                        &pool,
                        self.channel_id,
                        &self.path_replacements,
                        transcode_cursor,
                    ))
                    .await
                    .ok()
                    .flatten()
                    .unwrap_or(false);
                if let Some(reclaim) = request_active_transcode_reclaim(
                    &self.channel_number,
                    HLS_ACTIVE_WATCH_WINDOW,
                    HLS_STALE_WATCHED_PREEMPTION_THRESHOLD,
                    HLS_RECLAIMED_REACQUIRE_DELAY,
                ) {
                    tracing::info!(
                        channel_number = %self.channel_number,
                        target_channel = %reclaim.channel_number,
                        target_slot_id = reclaim.slot_id,
                        reason = ?reclaim.reason,
                        "requesting HLS transcode slot reclaim"
                    );
                    reclaim.notify.notify_one();
                }
                if retained_wait {
                    self.set_admission_status(HlsAdmissionStatus::AtCapacity);
                    let deadline = startup_started_at + Duration::from_secs(20);
                    if tokio::time::Instant::now() >= deadline {
                        break;
                    }
                    drop(work_ahead_reservation);
                    tokio::select! {
                        biased;
                        () = self.cancel.notified() => break,
                        () = tokio::time::sleep_until(deadline) => break,
                        () = tokio::time::sleep(HLS_ADMISSION_RETRY_INTERVAL) => continue,
                    }
                }
                self.set_admission_status(HlsAdmissionStatus::AtCapacity);
                self.set_session_state(self.session_state().throttle());
                let _ = self.trim_playlist_inner().await;
                drop(work_ahead_reservation);
                match self
                    .run_all_streams_in_use_slate(
                        &pool,
                        &mut cleanup_interval,
                        idle_timeout,
                        is_first,
                        next_mpegts_output_ts_offset,
                    )
                    .await
                {
                    HlsProcessOutcome::Success => {
                        is_first = false;
                        next_mpegts_output_ts_offset = advance_mpegts_output_ts_offset(
                            self.output_format,
                            next_mpegts_output_ts_offset,
                            HLS_AT_CAPACITY_SLATE_DURATION,
                        );
                        last_playout_item_id = None;
                    }
                    HlsProcessOutcome::Cancelled => break,
                    HlsProcessOutcome::Reclaimed
                    | HlsProcessOutcome::SourceCaptionUnavailable
                    | HlsProcessOutcome::DeliverySoftwareFallback { .. }
                    | HlsProcessOutcome::DeliveryFallback { .. } => {}
                    HlsProcessOutcome::Failed(error) => {
                        tracing::warn!(
                            channel_number = %self.channel_number,
                            %error,
                            "failed to render all-streams-in-use HLS slate"
                        );
                        tokio::time::sleep(HLS_ADMISSION_RETRY_INTERVAL).await;
                    }
                }
                continue;
            };
            self.set_admission_status(HlsAdmissionStatus::Transcoding);
            let _admission_status_guard = AdmissionStatusGuard::new(&self);
            let active_transcode_slot_id = active_transcode_reservation.slot_id();
            let watch_idle_timeout = transcode_watch_idle_timeout(idle_timeout);
            let decision = hls_transcode_decision(transcode_state, transcoded_until, now, realtime);
            self.set_session_state(decision.state);

            if decision.state.is_slug() {
                let Some(duration) = slug_duration else {
                    self.set_session_state(next_hls_session_state(decision.state, true, false));
                    continue;
                };
                let playback_profile = match self.load_playback_profile(&pool).await {
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
                let channel_name = match load_channel_name(&pool, self.channel_id).await {
                    Ok(name) => name.unwrap_or_else(|| self.channel_number.clone()),
                    Err(error) => {
                        tracing::error!(
                            %error,
                            channel_id = self.channel_id,
                            channel_number = %self.channel_number,
                            "failed to load channel name for hls slug; stopping hls worker"
                        );
                        break;
                    }
                };
                let Ok(target_frame_rate) = self.load_target_frame_rate(&pool, "hls slug").await
                else {
                    break;
                };
                let output_ts_offset = self
                    .next_output_ts_offset(is_first, next_mpegts_output_ts_offset)
                    .await;
                let discontinuity =
                    hls_output_discontinuity(self.output_format, is_first, output_ts_offset, true);
                let runtime_capabilities = self.runtime_capabilities().await;
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
                    runtime_capabilities,
                );
                let _ = self.trim_playlist_for_append(discontinuity).await;
                let outcome = self
                    .run_hls_process(
                        command,
                        &mut cleanup_interval,
                        idle_timeout,
                        watch_idle_timeout,
                        Some(active_transcode_slot_id),
                    )
                    .await;
                if let Err(error) =
                    update_on_demand_checkpoint(&pool, self.channel_id, Utc::now()).await
                {
                    tracing::debug!(
                        %error,
                        channel_number = %self.channel_number,
                        "failed to update on-demand checkpoint after hls slug transcode"
                    );
                }
                match outcome {
                    HlsProcessOutcome::Success => {
                        self.set_admission_status(HlsAdmissionStatus::Waiting);
                        transcoded_until = add_std_duration(decision.request_time, duration);
                        self.report_transcoded_until(transcoded_until);
                        self.set_session_state(next_hls_session_state(
                            self.session_state(),
                            true,
                            has_slug,
                        ));
                        interval_startup = None;
                        startup_attempt_started_at = None;
                        is_first = false;
                        next_mpegts_output_ts_offset = advance_mpegts_output_ts_offset(
                            self.output_format,
                            next_mpegts_output_ts_offset,
                            duration,
                        );
                        last_playout_item_id = None;
                        continue;
                    }
                    HlsProcessOutcome::Reclaimed => {
                        self.set_admission_status(HlsAdmissionStatus::Waiting);
                        self.set_session_state(self.session_state().throttle());
                        continue;
                    }
                    HlsProcessOutcome::Failed(_)
                    | HlsProcessOutcome::SourceCaptionUnavailable
                    | HlsProcessOutcome::Cancelled
                    | HlsProcessOutcome::DeliverySoftwareFallback { .. }
                    | HlsProcessOutcome::DeliveryFallback { .. } => {}
                }
                break;
            }

            let active = match self
                .startup_read(Box::pin(external::load(
                    &self,
                    &pool,
                    self.channel_id,
                    &self.path_replacements,
                    decision.request_time,
                )))
                .await
            {
                Ok(Some(active)) => active,
                Ok(None) => {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    continue;
                }
                Err(error) => {
                    tracing::error!(
                        %error,
                        channel_id = self.channel_id,
                        channel_number = %self.channel_number,
                        "failed to load active HLS item; delaying hls worker"
                    );
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    continue;
                }
            };
            let elapsed = if decision.start_at_zero {
                Duration::ZERO
            } else {
                playout_start_duration(decision.request_time, active.playout_start)
            };
            let is_live_remote = active.remote_stream.as_ref().is_some_and(|stream| {
                stream.liveness == ersatzrs_domain::RemoteStreamLiveness::Live
            });
            let is_continuous_remote = active.remote_stream.as_ref().is_some_and(|stream| {
                stream.liveness == ersatzrs_domain::RemoteStreamLiveness::Finite
            });
            let hls_realtime = decision.realtime || is_live_remote || is_continuous_remote;
            let start_position = remote_stream_playback_start(
                active.remote_stream.as_ref(),
                active.content_seek_position(elapsed),
            );
            let start_seconds = duration_as_i64_seconds(start_position);
            let duration = hls_playout_item_duration(
                &active,
                decision.request_time,
                decision.start_at_zero,
                hls_realtime,
            );
            if duration.is_none_or(|duration| duration.is_zero()) {
                tokio::time::sleep(Duration::from_millis(250)).await;
                continue;
            }

            let source_stream = match self
                .startup_read(async {
                    if let Some(media) = &active.external {
                        crate::playback::source_video_stream_from_probe(&media.probe)
                    } else {
                        load_source_video_stream_by_index(
                            &pool,
                            active.media_version_id,
                            active.video_stream_index,
                        )
                        .await
                    }
                })
                .await
            {
                Ok(stream) => stream,
                Err(error) => {
                    tracing::error!(
                        %error,
                        channel_id = self.channel_id,
                        channel_number = %self.channel_number,
                        media_version_id = active.media_version_id,
                        "failed to load source video metadata; delaying hls worker"
                    );
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    continue;
                }
            };
            let source_audio_streams = match self
                .startup_read(async {
                    if let Some(media) = &active.external {
                        crate::playback::source_audio_streams_from_probe(&media.probe)
                    } else {
                        load_source_audio_streams(&pool, active.media_version_id).await
                    }
                })
                .await
            {
                Ok(streams) => streams,
                Err(error) => {
                    tracing::error!(
                        %error,
                        channel_id = self.channel_id,
                        channel_number = %self.channel_number,
                        media_version_id = active.media_version_id,
                        "failed to load source audio metadata; delaying hls worker"
                    );
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    continue;
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
                        channel_id = self.channel_id,
                        media_version_id = active.media_version_id,
                        stream_index,
                        "pinned HLS audio stream is unavailable; delaying worker"
                    );
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    continue;
                }
            }
            let mut playback_profile =
                match self.startup_read(self.load_playback_profile(&pool)).await {
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
            let mut interval_playback =
                if let Some(crate::remote_stream::RemoteStreamSource::RetainedIntervals(source)) =
                    active.remote_stream.as_ref().map(|stream| &stream.source)
                {
                    let deadline = match interval_startup {
                        Some((item, deadline)) if item == active.playout_item_id => deadline,
                        _ => {
                            let deadline = startup_started_at + Duration::from_secs(20);
                            interval_startup = Some((active.playout_item_id, deadline));
                            deadline
                        }
                    };
                    match Box::pin(self.prepare_interval_playback(
                        &pool,
                        source,
                        start_position,
                        playback_frame_size(&playback_profile).height,
                        deadline,
                        active_transcode_slot_id,
                        idle_timeout,
                        watch_idle_timeout,
                    ))
                    .await
                    {
                        Ok(Some(prepared)) => Some(prepared),
                        Ok(None) => {
                            tracing::warn!(
                                channel_id = self.channel_id,
                                "retained HLS plan is exhausted at the requested programme position"
                            );
                            break;
                        }
                        Err(HlsProcessOutcome::Reclaimed) => {
                            self.set_admission_status(HlsAdmissionStatus::Waiting);
                            continue;
                        }
                        Err(_) => break,
                    }
                } else {
                    None
                };
            let previous_audio_streams = source_audio_streams;
            let source_audio_streams = interval_playback.as_ref().map_or_else(
                || previous_audio_streams.clone(),
                |prepared| prepared.source_audio.clone(),
            );
            let source_stream = interval_playback
                .as_ref()
                .map_or(source_stream, |prepared| {
                    Some(prepared.source_video.clone())
                });
            let supports_quality_fallback = is_live_remote
                && active
                    .remote_stream
                    .as_ref()
                    .and_then(remote_stream_video_height_ceiling)
                    .is_some();
            let configured_software_video_scope = match hls_delivery_software_scope(
                self.channel_id,
                &playback_profile,
            ) {
                Ok(scope) => Some(scope),
                Err(error) => {
                    tracing::warn!(
                        %error,
                        channel_id = self.channel_id,
                        ffmpeg_profile_id = playback_profile.profile.id,
                        "unable to fingerprint the HLS playback profile; disabling persisted software fallback"
                    );
                    None
                }
            };
            let available_software_video_scope = configured_software_video_scope
                .filter(|_| hls_software_video_fallback_available(&playback_profile));
            let (delivery_adaptation, delivery_fallback_verified) = if is_live_remote {
                let previous_adaptation = delivery_adaptations.get(&active.media_item_id).copied();
                let in_memory = previous_adaptation.and_then(|adaptation| {
                    hls_delivery_adaptation_for_scope(
                        adaptation,
                        available_software_video_scope,
                        supports_quality_fallback,
                    )
                });
                if let Some(previous_software_scope) =
                    previous_adaptation.and_then(|adaptation| adaptation.software_video_scope)
                {
                    let retained_software_scope =
                        in_memory.and_then(|adaptation| adaptation.software_video_scope);
                    if retained_software_scope != Some(previous_software_scope) {
                        let _ = clear_hls_delivery_fallback(
                            &pool,
                            active.media_item_id,
                            Some(previous_software_scope),
                        )
                        .await;
                    }
                }
                if let Some(adaptation) = in_memory {
                    delivery_adaptations.insert(active.media_item_id, adaptation);
                    (Some(adaptation), false)
                } else {
                    delivery_adaptations.remove(&active.media_item_id);
                    match load_hls_delivery_fallback(
                        &pool,
                        active.media_item_id,
                        configured_software_video_scope,
                        Utc::now(),
                    )
                    .await
                    {
                        Ok(Some(fallback)) => (
                            hls_delivery_adaptation_for_scope(
                                HlsDeliveryAdaptation {
                                    detected_at: fallback.detected_at,
                                    max_video_height: fallback.max_video_height,
                                    software_video_scope: fallback.software_video_scope,
                                },
                                available_software_video_scope,
                                supports_quality_fallback,
                            ),
                            true,
                        ),
                        Ok(None) => (None, false),
                        Err(error) => {
                            tracing::warn!(
                                %error,
                                channel_id = self.channel_id,
                                channel_number = %self.channel_number,
                                media_item_id = active.media_item_id,
                                "ignoring invalid live HLS delivery fallback state"
                            );
                            let _ = clear_hls_delivery_fallback(
                                &pool,
                                active.media_item_id,
                                configured_software_video_scope,
                            )
                            .await;
                            let _ = clear_hls_delivery_fallback(&pool, active.media_item_id, None)
                                .await;
                            (None, false)
                        }
                    }
                }
            } else {
                (None, false)
            };
            if delivery_adaptation
                .is_some_and(|adaptation| adaptation.software_video_scope.is_some())
            {
                apply_hls_software_video_fallback(&mut playback_profile);
            }
            let source_stream = if interval_playback.is_some() {
                source_stream
            } else if active.external.is_some() {
                tokio::select! {
                    biased;
                    () = self.cancel.notified() => break,
                    stream = crate::playback::enrich_transient_source_video_stream_for_playback(
                        &pool, source_stream, &active.media_path, active.content_duration.unwrap_or_default(),
                        playback_profile.profile.deinterlace_video == Some(true),
                        playback_profile.profile.hardware_acceleration, external::deadline(&self),
                    ) => stream,
                }
            } else {
                tokio::select! {
                    biased;
                    () = self.cancel.notified() => break,
                    enriched = enrich_source_video_stream_for_playback(
                        &pool,
                        active.media_version_id,
                        playback_profile.profile.deinterlace_video == Some(true),
                        source_stream,
                        playback_profile.profile.hardware_acceleration,
                        &active.media_path,
                    ) => enriched,
                }
            };
            let Ok(target_frame_rate) = self
                .startup_read(self.load_target_frame_rate(&pool, "hls worker"))
                .await
            else {
                if let Some(playback) = interval_playback.take() {
                    let _ = playback.prepared.cancel().await;
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            };
            let language_preferences = match self
                .startup_read(playback_language_preferences(&pool))
                .await
            {
                Ok(preferences) => preferences,
                Err(error) => {
                    tracing::error!(
                        %error,
                        channel_id = self.channel_id,
                        channel_number = %self.channel_number,
                        "failed to load HLS playback language preferences; delaying hls worker"
                    );
                    if let Some(playback) = interval_playback.take() {
                        let _ = playback.prepared.cancel().await;
                    }
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    continue;
                }
            };
            let subtitle_mode = active.subtitle_mode.unwrap_or(active.channel_subtitle_mode);
            let preferred_subtitle_languages = language_preferences.subtitle_languages(
                active.preferred_subtitle_language_code.as_deref(),
                active.channel_preferred_subtitle_language_code.as_deref(),
            );
            let preferred_audio_title = active
                .preferred_audio_title
                .as_deref()
                .or(active.channel_preferred_audio_title.as_deref());
            let preferred_audio_language = language_preferences.audio_language(
                active.preferred_audio_language_code.as_deref(),
                active.channel_preferred_audio_language_code.as_deref(),
                preferred_audio_title,
                StreamingMode::HttpLiveStreamingSegmenter,
            );
            let channel_for_streams =
                active.channel_for_runtime_streams(self.channel_id, self.channel_number.clone());
            let custom_selection = if active.has_pinned_stream_selection {
                None
            } else {
                match self
                    .startup_read(external::custom(&pool, &active, &channel_for_streams))
                    .await
                {
                    Ok(selection) => selection,
                    Err(error) => {
                        tracing::error!(
                            %error,
                            channel_id = self.channel_id,
                            channel_number = %self.channel_number,
                            media_item_id = active.media_item_id,
                            media_version_id = active.media_version_id,
                            "failed to execute HLS channel stream selector; delaying hls worker"
                        );
                        if let Some(playback) = interval_playback.take() {
                            let _ = playback.prepared.cancel().await;
                        }
                        tokio::time::sleep(Duration::from_secs(1)).await;
                        continue;
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
                    crate::audio_script::transient::select(
                        &channel_for_streams,
                        media.audio_metadata(),
                        crate::audio_script::AudioScriptPreferences {
                            language: preferred_audio_language.as_deref(),
                            title: preferred_audio_title,
                            streaming_mode: StreamingMode::HttpLiveStreamingSegmenter,
                        },
                        &media.tracks(),
                    )
                    .await
                } else {
                    crate::audio_script::select(
                        &pool,
                        &channel_for_streams,
                        active.media_item_id,
                        active.media_version_id,
                        crate::audio_script::AudioScriptPreferences {
                            language: preferred_audio_language.as_deref(),
                            title: preferred_audio_title,
                            streaming_mode: StreamingMode::HttpLiveStreamingSegmenter,
                        },
                        if interval_playback.is_some() {
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
            let mut audio_selection = if active.has_pinned_stream_selection {
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
                    .or_else(|| {
                        select_playback_audio_stream_index(
                            &source_audio_streams,
                            preferred_audio_language.as_deref(),
                            preferred_audio_title,
                        )
                    })
                    .map_or(
                        PlaybackAudioSelection::Automatic,
                        PlaybackAudioSelection::Exact,
                    )
            };
            if interval_playback.is_some()
                && (active.has_pinned_stream_selection
                    || script_audio.is_some()
                    || custom_selection
                        .as_ref()
                        .is_some_and(|selection| selection.audio_stream_index.is_some()))
            {
                audio_selection = if let Ok(selection) =
                    crate::playback::remap_pinned_audio_selection(
                        audio_selection,
                        &previous_audio_streams,
                        &source_audio_streams,
                    ) {
                    selection
                } else {
                    tracing::warn!(
                        channel_id = self.channel_id,
                        "retained HLS audio selection cannot be mapped to fresh transport"
                    );
                    if let Some(playback) = interval_playback.take() {
                        let _ = playback.prepared.cancel().await;
                    }
                    break;
                };
            }
            if interval_playback.is_some() {
                audio_selection =
                    retained_hls_audio_selection(audio_selection, &source_audio_streams);
            }
            // Persisted media/external probes and fresh retained transports have
            // complete metadata. An eager unprobed RemoteStream can have no
            // stored audio rows without proving that its actual input is silent.
            if active.remote_stream.is_none()
                || active.external.is_some()
                || interval_playback.is_some()
            {
                audio_selection = audio_selection.with_complete_source_audio(&source_audio_streams);
            }
            let mut source_caption = None;
            let subtitle = if active.has_pinned_stream_selection {
                if let Some(stream_index) = active.subtitle_stream_index {
                    match self
                        .startup_read(select_pinned_playback_subtitle(
                            &pool,
                            active.media_item_id,
                            active.media_version_id,
                            stream_index,
                            StreamingMode::HttpLiveStreamingSegmenter,
                        ))
                        .await
                    {
                        Ok(subtitle) => Some(subtitle),
                        Err(error) => {
                            tracing::error!(
                                %error,
                                channel_id = self.channel_id,
                                media_item_id = active.media_item_id,
                                media_version_id = active.media_version_id,
                                stream_index,
                                "pinned HLS subtitle stream is unavailable; delaying worker"
                            );
                            if let Some(playback) = interval_playback.take() {
                                let _ = playback.prepared.cancel().await;
                            }
                            tokio::time::sleep(Duration::from_secs(1)).await;
                            continue;
                        }
                    }
                } else {
                    None
                }
            } else if let Some(selection) = custom_selection {
                selection.subtitle
            } else {
                match self
                    .startup_read(Box::pin(self.automatic_source_caption(
                        &pool,
                        source_caption::AutomaticCaptionRequest {
                            active: &active,
                            languages: &preferred_subtitle_languages,
                            mode: subtitle_mode,
                            channel: &channel_for_streams,
                            resolution: &playback_profile.resolution,
                            start: start_seconds,
                            playback_interval: duration.and_then(|duration| {
                                source_caption::SourceTeletextPlaybackInterval::from_start_and_duration(
                                    start_position,
                                    duration,
                                )
                            }),
                            ordinary_only: source_caption_fallback.take().is_some_and(
                                |occurrence| {
                                    occurrence == source_caption::CaptionOccurrence::from(&active)
                                },
                            ),
                        },
                    )))
                    .await
                {
                    Ok(selection) => {
                        source_caption = selection.source;
                        selection.subtitle
                    }
                    Err(error) => {
                        tracing::error!(
                            %error,
                            channel_id = self.channel_id,
                            channel_number = %self.channel_number,
                            media_item_id = active.media_item_id,
                            "failed to select HLS subtitle; delaying hls worker"
                        );
                        if let Some(playback) = interval_playback.take() {
                            let _ = playback.prepared.cancel().await;
                        }
                        tokio::time::sleep(Duration::from_secs(1)).await;
                        continue;
                    }
                }
            };
            // Keep the owner until the encoder is supervised to completion.
            let (subtitle, _external_subtitle_owner) =
                if let (Some(media), Some(selected)) = (&active.external, subtitle.as_ref()) {
                    if let Ok((selected, owner)) = crate::external_json::native::prepare_subtitle(
                        &pool,
                        media,
                        &active.media_path,
                        selected.clone(),
                        StreamingMode::HttpLiveStreamingSegmenter,
                        external::deadline(&self),
                    )
                    .await
                    {
                        (Some(selected), owner)
                    } else {
                        tracing::warn!(
                            channel_id = self.channel_id,
                            "external HLS subtitle preparation failed"
                        );
                        break;
                    }
                } else {
                    (subtitle, None)
                };
            let mut retained_subtitle_input = None;
            let retained_subtitle_verified =
                if let (
                    Some(subtitle),
                    Some(crate::remote_stream::RemoteStreamSource::RetainedIntervals(source)),
                ) = (
                    subtitle.as_ref(),
                    active.remote_stream.as_ref().map(|stream| &stream.source),
                ) {
                    match
                    self.startup_read(
                        crate::addon_media_list_subtitle_playback::prepare_retained_subtitle_input(
                            &pool,
                            active.media_item_id,
                            subtitle,
                            source.plan(),
                            start_position,
                        )
                    )
                    .await {
                        Ok(input) => { retained_subtitle_input = input; true }
                        Err(_) => false,
                    }
                } else {
                    true
                };
            if !retained_subtitle_verified {
                tracing::warn!(
                    channel_id = self.channel_id,
                    "retained HLS subtitles require edited-timeline mapping"
                );
                if let Some(playback) = interval_playback.take() {
                    let _ = playback.prepared.cancel().await;
                }
                break;
            }
            let subtitle_input = source_caption
                .as_ref()
                .map(|source| source.input())
                .or_else(|| {
                    subtitle.as_ref().and_then(|subtitle| {
                        let input = if active.external.is_some() {
                            crate::external_json::native::subtitle_input
                        } else {
                            subtitle_input_for_stream
                        };
                        input(
                            subtitle,
                            &active.media_path,
                            self.output_format,
                            StreamingMode::HttpLiveStreamingSegmenter,
                        )
                    })
                });
            if active.subtitle_stream_index.is_some() && subtitle_input.is_none() {
                tracing::error!(
                    channel_id = self.channel_id,
                    media_item_id = active.media_item_id,
                    stream_index = active.subtitle_stream_index,
                    "pinned HLS subtitle cannot be mapped; delaying worker"
                );
                if let Some(playback) = interval_playback.take() {
                    let _ = playback.prepared.cancel().await;
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            }
            let metadata = playback_output_metadata(
                Some("ErsatzTV"),
                Some(&active.channel_name),
                preferred_audio_language.as_deref(),
                &source_audio_streams,
                subtitle.as_ref(),
                audio_selection,
            );
            let subtitle_input = if interval_playback.is_some() {
                retained_subtitle_input
            } else {
                rebase_burned_text_subtitle_input(subtitle_input, start_seconds).await
            };
            let watermark_channel = Channel {
                id: self.channel_id,
                watermark_id: active.channel_watermark_id,
                streaming_mode: StreamingMode::HttpLiveStreamingSegmenter,
                ..Channel::default()
            };
            let watermark_inputs = match self
                .startup_read(async {
                    if let Some(media) = &active.external {
                        crate::playback::select_fallback_watermarks(
                            &pool,
                            &watermark_channel,
                            &crate::external_json::native::decoration_item(media),
                            now,
                        )
                        .await
                    } else {
                        select_playback_watermarks(
                            &pool,
                            &watermark_channel,
                            active.playout_item_id,
                            active.disable_watermarks,
                            now,
                        )
                        .await
                    }
                })
                .await
            {
                Ok(watermarks) => {
                    if let Some((_, deadline)) =
                        interval_startup.filter(|_| interval_playback.is_some())
                    {
                        let (cancel, receiver) = tokio::sync::oneshot::channel();
                        let operation = crate::playback::watermark_inputs_for_stream_cancellable(
                            watermarks,
                            active.program_duration,
                            deadline,
                            receiver,
                        );
                        tokio::pin!(operation);
                        let result = tokio::select! {
                            biased;
                            () = self.cancel.notified() => {
                                let _ = cancel.send(());
                                let _ = operation.await;
                                Err(RepositoryError::Conflict("HLS watermark startup cancelled".into()))
                            },
                            result = &mut operation => result,
                        };
                        if let Ok(inputs) = result {
                            inputs
                        } else {
                            self.set_startup_readiness(HlsStartupReadiness::Failed);
                            tracing::warn!(
                                channel_id = self.channel_id,
                                "retained HLS watermark preparation failed; refusing encoder startup"
                            );
                            if let Some(playback) = interval_playback.take() {
                                let _ = playback.prepared.cancel().await;
                            }
                            break;
                        }
                    } else {
                        watermark_inputs_for_stream(watermarks, active.program_duration).await
                    }
                }
                Err(error) => {
                    tracing::warn!(%error, channel_number = %self.channel_number, "failed to select HLS watermarks");
                    Vec::new()
                }
            };
            let mut graphics_cleanup_dir = None;
            let mut dynamic_text_controllers = Vec::new();
            let graphics_overlays = match self
                .startup_read(async {
                    if let Some(media) = &active.external {
                        crate::playback::select_fallback_graphics(
                            &pool,
                            &crate::external_json::native::decoration_item(media),
                            now,
                        )
                        .await
                    } else {
                        select_playback_graphics(
                            &pool,
                            &watermark_channel,
                            active.playout_item_id,
                            now,
                        )
                        .await
                    }
                })
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
                    match self
                        .render_hls_graphics(
                            &pool,
                            &graphics,
                            &PlaybackGraphicsRenderContext {
                                channel_id: self.channel_id,
                                render_time: if decision.start_at_zero {
                                    active.playout_start
                                } else {
                                    decision.request_time
                                },
                                playout_item_id: Some(active.playout_item_id),
                                channel_number: self.channel_number.clone(),
                                media: active.external.as_ref().map_or_else(
                                    || {
                                        crate::graphics_template::GraphicsMedia::Persisted(
                                            active.media_item_id,
                                        )
                                    },
                                    |media| media.graphics_media(),
                                ),
                                media_path: active.media_path.clone(),
                                channel_start_time,
                                content_start_time: active.playout_start,
                                content_finish_time: active.playout_finish,
                                stream_seek_seconds: start_position.as_secs_f64(),
                                duration_seconds: active
                                    .program_duration
                                    .map_or(0.0, |duration| duration.as_secs_f64()),
                                frame_size,
                                square_pixel_frame_size,
                                frame_rate: target_frame_rate
                                    .or_else(|| {
                                        source_stream.as_ref().and_then(|stream| stream.frame_rate)
                                    })
                                    .map(Into::into),
                            },
                            output_dir,
                            interval_startup.map(|(_, deadline)| deadline),
                        )
                        .await
                    {
                        Ok(PlaybackGraphicsArtifacts {
                            overlays,
                            dynamic_text_controllers: controllers,
                        }) => {
                            dynamic_text_controllers = controllers;
                            overlays
                        }
                        Err(error) => {
                            if let Some(path) = &graphics_cleanup_dir {
                                let _ = tokio::fs::remove_dir_all(path).await;
                            }
                            graphics_cleanup_dir = None;
                            if interval_playback.is_some() {
                                tracing::warn!(
                                    channel_id = self.channel_id,
                                    "retained HLS graphics preparation failed; refusing encoder startup"
                                );
                                if let Some(playback) = interval_playback.take() {
                                    let _ = playback.prepared.cancel().await;
                                }
                                break;
                            }
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
            let output_ts_offset = self
                .next_output_ts_offset(is_first, next_mpegts_output_ts_offset)
                .await;
            let append_requires_discontinuity =
                last_playout_item_id != Some((active.playout_item_id, active.playout_start));
            let discontinuity = hls_output_discontinuity(
                self.output_format,
                is_first,
                output_ts_offset,
                append_requires_discontinuity,
            );
            let source_stream_for_denial = source_stream.clone();
            let opacity_clock = watermark_opacity_clock_at_spawn(
                decision.request_time,
                start_position,
                active.content_duration,
                channel_start_time,
            );
            let media_input = interval_playback.as_ref().map_or_else(
                || {
                    active.remote_stream.as_ref().map_or_else(
                        || Ok(InputFile::video(active.media_path.clone())),
                        remote_stream_input_file,
                    )
                },
                |prepared| Ok(prepared.video_input.clone()),
            );
            let mut media_input = match media_input {
                Ok(input) => input,
                Err(error) => {
                    tracing::warn!(channel_id = self.channel_id, %error, "HLS input is not ready");
                    shutdown_dynamic_text_controllers(dynamic_text_controllers).await;
                    if let Some(directory) = graphics_cleanup_dir {
                        let _ = tokio::fs::remove_dir_all(directory).await;
                    }
                    if let Some(playback) = interval_playback.take() {
                        let _ = playback.prepared.cancel().await;
                    }
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    continue;
                }
            };
            if is_continuous_remote && decision.working_ahead && interval_playback.is_none() {
                media_input = media_input.with_input_options([
                    "-readrate_initial_burst".to_owned(),
                    HLS_REMOTE_INITIAL_BURST_SECONDS.to_string(),
                ]);
            }
            let mut remote_stream = active.remote_stream.clone();
            let max_video_height = delivery_adaptation
                .map_or(REMOTE_STREAM_DEFAULT_MAX_VIDEO_HEIGHT, |adaptation| {
                    adaptation.max_video_height
                });
            if supports_quality_fallback {
                remote_stream = remote_stream.as_ref().map(|stream| {
                    with_remote_stream_video_height_ceiling(stream, max_video_height)
                });
            }
            let delivery_recovery = is_live_remote.then_some(HlsDeliveryRecovery {
                media_item_id: active.media_item_id,
                max_video_height,
                supports_quality_fallback,
                software_video_scope: delivery_adaptation
                    .and_then(|adaptation| adaptation.software_video_scope),
                available_software_video_scope,
                fallback_detected_at: delivery_adaptation.map(|adaptation| adaptation.detected_at),
                fallback_verified: delivery_fallback_verified,
            });
            let Ok(mut runtime_capabilities) = self
                .startup_read(async { Ok(self.runtime_capabilities().await) })
                .await
            else {
                shutdown_dynamic_text_controllers(dynamic_text_controllers).await;
                if let Some(directory) = graphics_cleanup_dir {
                    let _ = cleanup_working_dir(&directory).await;
                }
                if let Some(playback) = interval_playback.take() {
                    let _ = playback.prepared.cancel().await;
                }
                break;
            };
            if interval_playback.is_some()
                && interval_startup
                    .is_some_and(|(_, deadline)| tokio::time::Instant::now() >= deadline)
            {
                tracing::warn!(
                    channel_id = self.channel_id,
                    "retained HLS startup deadline expired before encoder creation"
                );
                shutdown_dynamic_text_controllers(dynamic_text_controllers).await;
                if let Some(directory) = graphics_cleanup_dir {
                    let _ = cleanup_working_dir(&directory).await;
                }
                if let Some(playback) = interval_playback.take() {
                    let _ = playback.prepared.cancel().await;
                }
                break;
            }
            let song = {
                let executable = runtime_capabilities
                    .runtime_identity
                    .as_ref()
                    .map_or_else(ffmpeg_path, |identity| identity.ffmpeg_path.clone());
                let (sender, receiver) = tokio::sync::oneshot::channel();
                let operation = crate::song_background::prepare_native_with_reservation(
                    &pool,
                    active.media_item_id,
                    self.channel_id,
                    &playback_profile.resolution,
                    active.playout_item_id,
                    Path::new(&executable),
                    interval_startup.map_or(
                        tokio::time::Instant::now() + Duration::from_secs(15),
                        |(_, deadline)| {
                            deadline.min(tokio::time::Instant::now() + Duration::from_secs(15))
                        },
                    ),
                    receiver,
                    Some(active_transcode_reservation.clone()),
                );
                tokio::pin!(operation);
                let result = tokio::select! {
                    biased;
                    () = self.cancel.notified() => {
                        let _ = sender.send(());
                        if let Ok(Some(song)) = operation.await { song.confirm_reaped(); }
                        if let Some(playback) = interval_playback.take() {
                            let _ = playback.prepared.cancel().await;
                        }
                        shutdown_dynamic_text_controllers(dynamic_text_controllers).await;
                        if let Some(path) = graphics_cleanup_dir { let _ = cleanup_working_dir(&path).await; }
                        break;
                    },
                    result = &mut operation => result,
                };
                match result {
                    Ok(song) => song,
                    Err(error) => {
                        tracing::warn!(channel_id = self.channel_id, %error, "song image preparation failed");
                        if let Some(playback) = interval_playback.take() {
                            let _ = playback.prepared.cancel().await;
                        }
                        shutdown_dynamic_text_controllers(dynamic_text_controllers).await;
                        if let Some(path) = graphics_cleanup_dir {
                            let _ = cleanup_working_dir(&path).await;
                        }
                        if error.termination_unconfirmed() {
                            // The owned renderer retained admission until restart.
                            break;
                        }
                        tokio::time::sleep(Duration::from_secs(1)).await;
                        continue;
                    }
                }
            };
            let (acceleration, mut request) = build_hls_request_with_input_explicit_discontinuity(
                media_input,
                self.output_format,
                is_first,
                false,
                hls_realtime,
                start_position,
                duration,
                output_ts_offset,
                &playback_profile,
                target_frame_rate,
                metadata,
                runtime_capabilities.clone(),
                source_stream,
                source_audio_streams,
                audio_selection,
                subtitle_input,
                watermark_inputs,
                graphics_overlays,
                HlsDiscontinuity::from(discontinuity),
                Some(opacity_clock),
                if interval_playback.is_some() {
                    HlsInputTimeline::RetainedIntervals
                } else {
                    HlsInputTimeline::Original
                },
                song.as_ref(),
            );
            crate::playback::apply_live_input_eof_policy(&mut request, is_live_remote);
            apply_hls_bootstrap_readburst(
                &mut request,
                initial_bootstrap,
                is_continuous_remote,
                self.startup_minimum_segments.load(Ordering::Acquire),
            );
            let bitmap_source = source_caption.as_ref().filter(|source| {
                source.delivery() == ersatzrs_domain::SourceTeletextCaptionDelivery::DvbSubtitles
            });
            let mut producer_failed = false;
            let producer_playlist = if bitmap_source.is_some() {
                let _ = self.trim_playlist_for_append(discontinuity).await;
                if let Ok(playlist) = producer_playlist::MpegTsProducerPlaylist::prepare(
                    &self.working_dir,
                    *self.playlist_start.lock().await,
                )
                .await
                {
                    if let Ok(options) = playlist.options() {
                        if let Some(hls) = request.state.hls.as_mut() {
                            hls.mpegts_segments = Some(options);
                            Some(playlist)
                        } else {
                            producer_failed = true;
                            None
                        }
                    } else {
                        producer_failed = true;
                        None
                    }
                } else {
                    producer_failed = true;
                    None
                }
            } else {
                None
            };
            let source_caption_request = if let Some(source) = bitmap_source {
                let request = crate::playback::source_teletext::runtime::SourceTeletextConsumerRequest::new_bitmap_carriage(
                    acceleration, request, source.input(),
                );
                if let Some(playlist) = &producer_playlist {
                    request.with_hls_producer_directory(playlist.directory_owner())
                } else {
                    request
                }
            } else {
                crate::playback::source_teletext::runtime::SourceTeletextConsumerRequest::new(
                    acceleration,
                    request,
                )
            };
            let command = source_caption_request.build();
            let live_completed_media_duration = std::sync::Mutex::new(None);
            let retained_failure = Arc::new(std::sync::Mutex::new(RetainedHlsFailure::default()));
            let (mut outcome, selected_graph_for_denial) = match command {
                _ if producer_failed => {
                    if let Some(song) = &song {
                        song.confirm_reaped();
                    }
                    (HlsProcessOutcome::SourceCaptionUnavailable, None)
                }
                Ok((command, source_caption_plan)) => {
                    // Attributed from the command that actually ran, so a
                    // runtime failure cannot be blamed on a graph this spawn
                    // did not use.
                    let selected_graph_for_denial =
                        ersatzrs_ffmpeg::selected_hardware_graph(&command)
                            .map(|operation| (operation, command.executable.clone()));
                    log_hls_av1_spawn_diagnostics(
                        &self.channel_number,
                        &command,
                        &runtime_capabilities,
                        source_stream_for_denial.as_ref(),
                        active.remote_stream.as_ref(),
                    );
                    if producer_playlist.is_none() {
                        let _ = self.trim_playlist_for_append(discontinuity).await;
                    }
                    let outcome = self
                        .run_hls_process_with_remote(
                            command,
                            &mut cleanup_interval,
                            idle_timeout,
                            watch_idle_timeout,
                            remote_stream,
                            interval_playback.take().map(|prepared| RetainedHlsProcess {
                                prepared: prepared.prepared,
                                failure: Arc::clone(&retained_failure),
                                graph: selected_graph_for_denial
                                    .as_ref()
                                    .map(|(operation, _)| *operation),
                                source: source_stream_for_denial.clone(),
                                acceleration: playback_profile.profile.hardware_acceleration,
                                remaining: duration.unwrap_or_default(),
                                deadline: interval_startup.map_or(
                                    startup_started_at + Duration::from_secs(20),
                                    |(_, deadline)| deadline,
                                ),
                            }),
                            HlsProcessCapacity {
                                completed_media_duration: is_live_remote
                                    .then_some(&live_completed_media_duration),
                                producer_playlist: producer_playlist.as_ref(),
                                song: song.as_ref(),
                                active_transcode_slot_id: Some(active_transcode_slot_id),
                                work_ahead_reservation: (is_continuous_remote
                                    && decision.working_ahead)
                                    .then_some(&mut work_ahead_reservation),
                                delivery_pool: is_live_remote.then_some(&pool),
                                delivery_recovery,
                                source_caption: source_caption.as_ref().map(|source| {
                                    (
                                        Arc::clone(source),
                                        active_transcode_reservation.clone(),
                                        source_caption_plan,
                                        source_caption_request.clone(),
                                    )
                                }),
                            },
                        )
                        .await;
                    (outcome, selected_graph_for_denial)
                }
                Err(error) => {
                    // ADR 0064: refuse rather than transcode a source whose
                    // colour ErsatzRS cannot render correctly. Route the
                    // refusal through the normal error-slate path below so the
                    // HLS cursor advances instead of selecting this item again.
                    tracing::error!(
                        channel_id = self.channel_id,
                        media_item_id = active.media_item_id,
                        reason = %error,
                        "refusing to transcode an unsupported source"
                    );
                    if let Some(song) = &song {
                        song.confirm_reaped();
                    }
                    (refused_hls_process_outcome(&error), None)
                }
            };
            // A completed Live producer may end before the scheduled item.
            // Advance by its own complete output, never by the unserved remainder.
            let duration = if is_live_remote && outcome == HlsProcessOutcome::Success {
                let measured = live_completed_media_duration
                    .lock()
                    .ok()
                    .and_then(|value| *value);
                if measured.is_none() {
                    outcome =
                        HlsProcessOutcome::Failed("live output duration was not retained".into());
                }
                measured
            } else {
                duration
            };
            if let Some(song) = &song {
                if outcome == HlsProcessOutcome::Success {
                    song.confirm_reaped();
                }
            }
            if let Some(playback) = interval_playback.take() {
                let _ = playback.prepared.cancel().await;
            }
            shutdown_dynamic_text_controllers(dynamic_text_controllers).await;
            if song
                .as_ref()
                .is_some_and(crate::song_background::NativeSongBackground::cleanup_unconfirmed)
            {
                active_transcode_reservation.retain_until_restart();
                break;
            }
            if let Some(path) = graphics_cleanup_dir {
                let _ = cleanup_working_dir(&path).await;
            }
            if outcome == HlsProcessOutcome::SourceCaptionUnavailable {
                // No consumer was spawned and no media was published. Retry
                // this exact occurrence/cursor once with ordinary captions;
                // the next successful chunk may qualify source captions anew.
                source_caption_fallback = Some(source_caption::CaptionOccurrence::from(&active));
                self.set_admission_status(HlsAdmissionStatus::Waiting);
                continue;
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
                    interval_startup = None;
                    startup_attempt_started_at = None;
                    self.set_admission_status(HlsAdmissionStatus::Waiting);
                    if let Some(duration) = duration {
                        transcoded_until = add_std_duration(decision.request_time, duration);
                        self.report_transcoded_until(transcoded_until);
                        self.set_session_state(next_hls_session_state(
                            self.session_state(),
                            item_complete(&active, transcoded_until),
                            has_slug,
                        ));
                        next_mpegts_output_ts_offset = advance_mpegts_output_ts_offset(
                            self.output_format,
                            next_mpegts_output_ts_offset,
                            duration,
                        );
                        last_playout_item_id = Some((active.playout_item_id, active.playout_start));
                    }
                }
                HlsProcessOutcome::SourceCaptionUnavailable => {
                    // Handled before checkpoint/cursor mutation above.
                    continue;
                }
                HlsProcessOutcome::Failed(error_message) => {
                    if retained_failure
                        .lock()
                        .is_ok_and(|failure| failure.media_published)
                    {
                        tracing::warn!(
                            channel_id = self.channel_id,
                            "retained HLS failed after media publication; refusing duplicate-prefix retry"
                        );
                        break;
                    }
                    // A graph that only fails under real media cannot be caught
                    // by startup probing, so the denial is learned from the
                    // failure and the retry goes through the next proven graph.
                    // Checked first because it is the more specific diagnosis.
                    if let Some((operation, ffmpeg_path_for_denial)) = selected_graph_for_denial {
                        if let Some(kind) = retained_failure
                            .lock()
                            .ok()
                            .and_then(|failure| failure.graph)
                            .or_else(|| {
                                ersatzrs_ffmpeg::GraphFailureKind::classify_runtime(
                                    operation,
                                    &error_message,
                                )
                            })
                        {
                            let device = runtime_capabilities.hardware_graph_device();
                            let denial =
                                ersatzrs_ffmpeg::HardwareGraphDenial::new(operation, &device, kind);
                            let evidence = denial.evidence();
                            let persist_result =
                                crate::hardware_graph::persist_hardware_graph_denial_into_runtime(
                                    &pool,
                                    &ffmpeg_path_for_denial,
                                    &mut runtime_capabilities,
                                    &self.ffmpeg_runtime,
                                    denial.clone(),
                                )
                                .await;
                            if let Err(error) = persist_result {
                                tracing::warn!(
                                    %error,
                                    channel_number = %self.channel_number,
                                    "failed to persist runtime-learned hardware graph denial"
                                );
                            }
                            if learned_graph_retry_used {
                                tracing::warn!(
                                    channel_number = %self.channel_number,
                                    evidence,
                                    "learned fallback graph denial after the hls retry was already used"
                                );
                            } else {
                                learned_graph_retry_used = true;
                                tracing::warn!(
                                    channel_number = %self.channel_number,
                                        evidence,
                                    "retrying hls segmenter once through the next proven graph after a runtime graph failure"
                                );
                                self.set_admission_status(HlsAdmissionStatus::Waiting);
                                continue;
                            }
                        }
                    }
                    let retained_decode_denial = retained_failure
                        .lock()
                        .ok()
                        .and_then(|failure| failure.decode.clone())
                        .filter(|_| !learned_decode_retry_used);
                    if let Some(denial) = retained_decode_denial.or_else(|| {
                        learned_hardware_decode_denial(
                            learned_decode_retry_used,
                            playback_profile.profile.hardware_acceleration,
                            source_stream_for_denial.as_ref(),
                            &error_message,
                        )
                    }) {
                        learned_decode_retry_used = true;
                        tracing::warn!(
                            channel_number = %self.channel_number,
                            acceleration = ?denial.acceleration,
                            codec = %denial.codec,
                            bit_depth = denial.bit_depth,
                            "retrying hls segmenter once with software input decode after runtime hardware decode rejection"
                        );
                        let persist_result =
                            crate::ffmpeg_capabilities::persist_hardware_decode_denial_into_runtime(
                                &pool,
                                &mut runtime_capabilities,
                                &self.ffmpeg_runtime,
                                denial,
                            )
                            .await;
                        if let Err(error) = persist_result {
                            tracing::warn!(
                                %error,
                                channel_number = %self.channel_number,
                                "failed to persist runtime-learned hardware decode denial"
                            );
                        }
                        self.set_admission_status(HlsAdmissionStatus::Waiting);
                        continue;
                    }
                    let Some(duration) = duration else {
                        break;
                    };
                    let fallback_runtime_capabilities = self.runtime_capabilities().await;
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
                        runtime_capabilities: fallback_runtime_capabilities,
                        error_message: &error_message,
                    })
                    .await
                    else {
                        break;
                    };
                    if self.trim_playlist_for_append(discontinuity).await.is_none() {
                        tracing::debug!(
                            channel_number = %self.channel_number,
                            "hls playlist not ready to trim before error fallback append"
                        );
                    }
                    if let Ok(mut output_error) = self.output_error.lock() {
                        *output_error = Some(
                            crate::remote_stream::sanitize_remote_stream_diagnostic(&error_message)
                                .chars()
                                .take(4096)
                                .collect(),
                        );
                    }
                    match self
                        .run_hls_process(
                            error_command,
                            &mut cleanup_interval,
                            idle_timeout,
                            watch_idle_timeout,
                            Some(active_transcode_slot_id),
                        )
                        .await
                    {
                        HlsProcessOutcome::Success => {
                            self.set_admission_status(HlsAdmissionStatus::Waiting);
                            let (next_cursor, next_state, next_output_offset) =
                                advance_after_hls_error_slate(
                                    decision.request_time,
                                    duration,
                                    self.session_state(),
                                    has_slug,
                                    self.output_format,
                                    next_mpegts_output_ts_offset,
                                );
                            transcoded_until = next_cursor;
                            self.report_transcoded_until(transcoded_until);
                            interval_startup = None;
                            startup_attempt_started_at = None;
                            self.set_session_state(next_state);
                            next_mpegts_output_ts_offset = next_output_offset;
                            last_playout_item_id = None;
                        }
                        HlsProcessOutcome::Reclaimed => {
                            self.set_admission_status(HlsAdmissionStatus::Waiting);
                            self.set_session_state(self.session_state().throttle());
                            continue;
                        }
                        HlsProcessOutcome::Failed(_)
                        | HlsProcessOutcome::SourceCaptionUnavailable
                        | HlsProcessOutcome::Cancelled
                        | HlsProcessOutcome::DeliverySoftwareFallback { .. }
                        | HlsProcessOutcome::DeliveryFallback { .. } => {
                            break;
                        }
                    }
                }
                HlsProcessOutcome::Reclaimed => {
                    self.set_admission_status(HlsAdmissionStatus::Waiting);
                    self.set_session_state(self.session_state().throttle());
                    continue;
                }
                HlsProcessOutcome::DeliverySoftwareFallback {
                    detected_at,
                    max_video_height,
                    software_video_scope,
                } => {
                    delivery_adaptations.insert(
                        active.media_item_id,
                        HlsDeliveryAdaptation {
                            detected_at,
                            max_video_height,
                            software_video_scope: Some(software_video_scope),
                        },
                    );
                    self.set_admission_status(HlsAdmissionStatus::Waiting);
                    tracing::warn!(
                        channel_number = %self.channel_number,
                        media_item_id = active.media_item_id,
                        ffmpeg_profile_id = software_video_scope.ffmpeg_profile_id,
                        "restarting under-delivering live HLS output with software video encoding"
                    );
                    continue;
                }
                HlsProcessOutcome::DeliveryFallback {
                    detected_at,
                    software_video_scope,
                } => {
                    delivery_adaptations.insert(
                        active.media_item_id,
                        HlsDeliveryAdaptation {
                            detected_at,
                            max_video_height: REMOTE_STREAM_FALLBACK_MAX_VIDEO_HEIGHT,
                            software_video_scope,
                        },
                    );
                    self.set_admission_status(HlsAdmissionStatus::Waiting);
                    tracing::warn!(
                        channel_number = %self.channel_number,
                        media_item_id = active.media_item_id,
                        from_height = REMOTE_STREAM_DEFAULT_MAX_VIDEO_HEIGHT,
                        to_height = REMOTE_STREAM_FALLBACK_MAX_VIDEO_HEIGHT,
                        "restarting under-delivering live HLS source with a lower video ceiling"
                    );
                    continue;
                }
                HlsProcessOutcome::Cancelled => break,
            }

            is_first = false;
        }

        self.set_admission_status(HlsAdmissionStatus::Waiting);
        let _ = cleanup_working_dir(&self.working_dir).await;
        registry
            .remove_if_current(&(self.clone() as Arc<dyn HlsSessionHandle>))
            .await;
    }

    async fn run_all_streams_in_use_slate(
        &self,
        pool: &DbPool,
        cleanup_interval: &mut tokio::time::Interval,
        idle_timeout: Option<Duration>,
        is_first_transcode: bool,
        next_mpegts_output_ts_offset: Option<Duration>,
    ) -> HlsProcessOutcome {
        let playback_profile = match self.load_playback_profile(pool).await {
            Ok(Some(profile)) => profile,
            Ok(None) => {
                return HlsProcessOutcome::Failed(
                    "channel has no playback ffmpeg profile for capacity slate".to_owned(),
                );
            }
            Err(error) => {
                return HlsProcessOutcome::Failed(format!(
                    "failed to load playback ffmpeg profile for capacity slate: {error}"
                ));
            }
        };
        let channel_name = match load_channel_name(pool, self.channel_id).await {
            Ok(name) => name.unwrap_or_else(|| self.channel_number.clone()),
            Err(error) => {
                return HlsProcessOutcome::Failed(format!(
                    "failed to load channel name for capacity slate: {error}"
                ));
            }
        };
        let target_frame_rate = match self.load_target_frame_rate(pool, "capacity slate").await {
            Ok(frame_rate) => frame_rate,
            Err(error) => {
                return HlsProcessOutcome::Failed(format!(
                    "failed to load channel target frame rate for capacity slate: {error}"
                ));
            }
        };
        let output_ts_offset = self
            .next_output_ts_offset(is_first_transcode, next_mpegts_output_ts_offset)
            .await;
        let discontinuity = hls_output_discontinuity(
            self.output_format,
            is_first_transcode,
            output_ts_offset,
            true,
        );
        let runtime_capabilities = self.runtime_capabilities().await;
        let Some(command) = build_hls_error_command(HlsErrorProcessRequest {
            working_dir: &self.working_dir,
            output_format: self.output_format,
            is_first_transcode,
            hls_realtime: true,
            duration: HLS_AT_CAPACITY_SLATE_DURATION,
            output_ts_offset,
            playback_profile: &playback_profile,
            target_frame_rate,
            metadata: OutputMetadata {
                service_provider: Some("ErsatzTV".to_owned()),
                service_name: Some(channel_name),
                ..OutputMetadata::default()
            },
            runtime_capabilities,
            error_message: ALL_STREAMS_IN_USE_SLATE_MESSAGE,
        })
        .await
        else {
            return HlsProcessOutcome::Failed(
                "failed to build all-streams-in-use HLS slate command".to_owned(),
            );
        };

        if self.trim_playlist_for_append(discontinuity).await.is_none() {
            tracing::debug!(
                channel_number = %self.channel_number,
                "hls playlist not ready to trim before capacity slate append"
            );
        }

        if let Ok(mut output_error) = self.output_error.lock() {
            *output_error = Some(ALL_STREAMS_IN_USE_SLATE_MESSAGE.to_owned());
        }
        self.run_hls_process(
            command,
            cleanup_interval,
            idle_timeout,
            transcode_watch_idle_timeout(idle_timeout),
            None,
        )
        .await
    }

    async fn run_hls_process(
        &self,
        command: FfmpegCommand,
        cleanup_interval: &mut tokio::time::Interval,
        idle_timeout: Option<Duration>,
        watch_idle_timeout: Option<Duration>,
        active_transcode_slot_id: Option<u64>,
    ) -> HlsProcessOutcome {
        self.run_hls_process_with_remote(
            command,
            cleanup_interval,
            idle_timeout,
            watch_idle_timeout,
            None,
            None,
            HlsProcessCapacity {
                completed_media_duration: None,
                producer_playlist: None,
                song: None,
                active_transcode_slot_id,
                work_ahead_reservation: None,
                delivery_pool: None,
                delivery_recovery: None,
                source_caption: None,
            },
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn render_hls_graphics(
        &self,
        pool: &DbPool,
        graphics: &[crate::playback::ResolvedPlaybackGraphicsElement],
        context: &PlaybackGraphicsRenderContext,
        output_dir: PathBuf,
        deadline: Option<tokio::time::Instant>,
    ) -> RepositoryResult<PlaybackGraphicsArtifacts> {
        let Some(deadline) = deadline else {
            return render_playback_graphics_overlays(pool, graphics, context, output_dir).await;
        };
        let (cancel, receiver) = tokio::sync::oneshot::channel();
        let operation = crate::playback::render_playback_graphics_overlays_cancellable(
            pool, graphics, context, output_dir, deadline, receiver,
        );
        tokio::pin!(operation);
        tokio::select! {
            biased;
            () = self.cancel.notified() => {},
            result = &mut operation => return result,
        }
        let _ = cancel.send(());
        let _ = operation.await;
        self.set_startup_readiness(HlsStartupReadiness::Failed);
        Err(RepositoryError::Conflict("HLS startup cancelled".into()))
    }

    #[allow(clippy::too_many_arguments)]
    async fn prepare_interval_playback(
        &self,
        pool: &DbPool,
        source: &crate::published_interval_source::PublishedIntervalSource,
        programme_position: Duration,
        max_video_height: u32,
        deadline: tokio::time::Instant,
        slot_id: u64,
        idle_timeout: Option<Duration>,
        watch_idle_timeout: Option<Duration>,
    ) -> Result<Option<crate::published_interval_source::RetainedIntervalPlayback>, HlsProcessOutcome>
    {
        let programme_milliseconds = u64::try_from(programme_position.as_millis())
            .map_err(|_| HlsProcessOutcome::Cancelled)?;
        let extractor = crate::ffprobe::FfprobeExtractor::new(ffprobe_path());
        let (cancel, receiver) = tokio::sync::oneshot::channel();
        let operation = source.prepare_playback_cancellable(
            crate::published_interval_source::IntervalPlaybackContext {
                pool,
                probe: &extractor,
                spawn_context: RemoteStreamSpawnContext::hls(&self.channel_number),
            },
            programme_milliseconds,
            Some(max_video_height),
            deadline,
            receiver,
        );
        tokio::pin!(operation);
        let interrupted = loop {
            tokio::select! {
                biased;
                () = self.cancel.notified() => break HlsProcessOutcome::Cancelled,
                () = self.wait_for_idle_timeout(idle_timeout) => break HlsProcessOutcome::Cancelled,
                () = self.wait_for_idle_timeout(watch_idle_timeout) => break HlsProcessOutcome::Reclaimed,
                () = self.reclaim_transcode.notified() => {
                    if self.take_reclaim_request(slot_id) { break HlsProcessOutcome::Reclaimed; }
                }
                result = &mut operation => return result.map_err(|error| {
                    tracing::warn!(channel_id = self.channel_id, %error, "retained HLS input preparation failed");
                    HlsProcessOutcome::Cancelled
                }),
            }
        };
        let _ = cancel.send(());
        match operation.await {
            Ok(Some(playback)) => {
                let _ = playback.prepared.cancel().await;
            }
            Err(error) => {
                tracing::debug!(channel_id = self.channel_id, %error, "retained HLS preparation cancellation completed");
            }
            Ok(None) => {}
        }
        Err(interrupted)
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "Preserve the existing process boundary while tracking per-segment output provenance"
    )]
    async fn run_hls_process_with_remote(
        &self,
        command: FfmpegCommand,
        cleanup_interval: &mut tokio::time::Interval,
        idle_timeout: Option<Duration>,
        watch_idle_timeout: Option<Duration>,
        remote_stream: Option<ResolvedRemoteStream>,
        retained: Option<RetainedHlsProcess>,
        capacity: HlsProcessCapacity<'_>,
    ) -> HlsProcessOutcome {
        let baseline = playlist_segment_uris(&self.working_dir).await.ok();
        let completed_media_duration = capacity.completed_media_duration;
        let error = self
            .output_error
            .lock()
            .ok()
            .and_then(|mut error| error.take());
        if let Ok(mut outputs) = self.output_status.lock() {
            outputs.begin(baseline.clone(), error);
        }
        let outcome = self
            .run_hls_process_with_remote_inner(
                command,
                cleanup_interval,
                idle_timeout,
                watch_idle_timeout,
                remote_stream,
                retained,
                capacity,
            )
            .await;
        let segments = playlist_segment_uris(&self.working_dir).await.ok();
        if let Ok(mut outputs) = self.output_status.lock() {
            outputs.finish(segments.as_ref());
        }
        if outcome == HlsProcessOutcome::Success {
            if let Some(completed) = completed_media_duration {
                // FFmpeg's list_size=0 final playlist retains every new segment.
                // Viewer trimming is a read-only projection; this is the producer's
                // own output, including the last short segment, not source metadata.
                let measured = match baseline {
                    Some(baseline) => {
                        playlist_duration_excluding_uris(&self.working_dir, &baseline).await
                    }
                    None => Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "live output baseline unavailable",
                    )),
                };
                match (measured, completed.lock()) {
                    (Ok(duration), Ok(mut value)) if !duration.is_zero() => *value = Some(duration),
                    _ => {
                        return HlsProcessOutcome::Failed(
                            "complete live output duration unavailable".into(),
                        )
                    }
                }
            }
        }
        outcome
    }

    #[allow(clippy::too_many_lines, clippy::too_many_arguments)] // Preserve the existing process state machine behind the provenance wrapper.
    async fn run_hls_process_with_remote_inner(
        &self,
        mut command: FfmpegCommand,
        cleanup_interval: &mut tokio::time::Interval,
        idle_timeout: Option<Duration>,
        watch_idle_timeout: Option<Duration>,
        remote_stream: Option<ResolvedRemoteStream>,
        retained: Option<RetainedHlsProcess>,
        mut capacity: HlsProcessCapacity<'_>,
    ) -> HlsProcessOutcome {
        let delivery_pool = capacity.delivery_pool;
        let delivery_recovery = capacity.delivery_recovery;
        let burst_baseline = if capacity.work_ahead_reservation.is_some() || retained.is_some() {
            playlist_segment_uris(&self.working_dir)
                .await
                .unwrap_or_default()
        } else {
            HashSet::new()
        };
        let delivery_started_at = tokio::time::Instant::now();
        let mut delivery_tracker = self
            .initial_hls_delivery_tracker(
                delivery_pool.is_some(),
                pending_hls_delivery_fallback_detected_at(delivery_recovery),
            )
            .await;
        let mut fallback_verified =
            delivery_recovery.is_some_and(|recovery| recovery.fallback_verified);
        let producer = remote_stream.as_ref().and_then(remote_stream_producer);
        if retained
            .as_ref()
            .is_some_and(|retained| tokio::time::Instant::now() >= retained.deadline)
        {
            tracing::warn!(
                channel_id = self.channel_id,
                "retained HLS startup deadline expired before spawn"
            );
            if let Some(retained) = retained {
                let _ = retained.prepared.cancel().await;
            }
            return HlsProcessOutcome::Cancelled;
        }
        let retained_remaining = retained.as_ref().map(|retained| retained.remaining);
        let retained_failure_state = retained
            .as_ref()
            .map(|retained| Arc::clone(&retained.failure));
        let mut reserve_interval = tokio::time::interval(Duration::from_millis(250));
        let mut initialized_bitmap = None;
        if let Some((source, _, plan, request)) = &capacity.source_caption {
            if producer.is_some() || retained.is_some() {
                if let Some(retained) = retained {
                    let _ = retained.prepared.cancel().await;
                }
                return HlsProcessOutcome::Failed(
                    "local source captions cannot use a remote HLS producer".into(),
                );
            }
            match Box::pin(source.initialize_consumer(
                &command,
                plan.as_ref(),
                request,
                self.source_caption_check_deadline(source.initial_deadline()),
            ))
            .await
            {
                Ok(initialized) => {
                    let (initialized_command, owner) = initialized.into_parts();
                    command = initialized_command;
                    initialized_bitmap = owner;
                }
                Err(_) => return HlsProcessOutcome::SourceCaptionUnavailable,
            }
        }
        if let Some(song) = capacity.song {
            song.mark_in_use();
        }
        let (spawn, retained_diagnostics, failed_spawn_cleanup_confirmed) =
            if let Some(retained) = retained {
                let child = crate::remote_stream::spawn_ffmpeg_with_prepared_remote_input(
                    &command,
                    Some(&self.working_dir),
                    retained.prepared,
                    RemoteStreamSpawnContext::hls(&self.channel_number),
                )
                .await;
                let cleanup_confirmed = child.as_ref().is_err_and(|error| {
                    matches!(error, crate::remote_stream::RemoteStreamError::Spawn(_))
                });
                (
                    child.map_err(std::io::Error::other),
                    Some((
                        retained.failure,
                        retained.graph,
                        retained.source,
                        retained.acceleration,
                    )),
                    cleanup_confirmed,
                )
            } else {
                (
                    self.spawn_hls_child_with_remote(&command, producer),
                    None,
                    producer.is_none(),
                )
            };
        let Ok(mut child) = spawn else {
            tracing::warn!(channel_number = %self.channel_number, "failed to spawn ffmpeg hls segmenter");
            if let Some(song) = capacity.song.filter(|_| failed_spawn_cleanup_confirmed) {
                song.confirm_reaped();
            }
            return HlsProcessOutcome::Failed("failed to spawn ffmpeg hls segmenter".to_owned());
        };
        // No await separates a local consumer spawn from transfer of its exact
        // receipt and actual transcode admission. The child adapter, including
        // its uncertainty Drop, owns them independently of the requesting UI.
        if let Some((source, reservation, _, _)) = &capacity.source_caption {
            let attached = if let Some(owner) = &initialized_bitmap {
                child.attach_initialized_teletext_bitmap(Arc::clone(owner), reservation.clone())
            } else {
                child.attach_local_teletext_source(source.receipt(), reservation.clone())
            };
            if let Err(error) = attached {
                if self
                    .terminate_hls_child(&mut child, None, "source caption attachment failed")
                    .await
                {
                    if let Some(song) = capacity.song {
                        song.confirm_reaped();
                    }
                }
                return HlsProcessOutcome::Failed(error.to_string());
            }
        }
        let stderr_remote_stream = remote_stream.clone();
        let mut stderr_task = child.stderr.take().map(|stderr| {
            tokio::spawn(async move {
                let stderr = read_ffmpeg_stderr_tail(stderr).await;
                if let Some((failure, graph, source, acceleration)) = retained_diagnostics {
                    if let Ok(mut failure) = failure.lock() {
                        *failure = classify_retained_hls_failure(
                            &stderr,
                            graph,
                            source.as_ref(),
                            acceleration,
                        );
                    }
                }
                redact_remote_stderr(stderr, stderr_remote_stream.as_ref())
            })
        });

        loop {
            tokio::select! {
                result = child.wait() => {
                    let cleanup_confirmed = result.is_ok() || matches!(tokio::time::timeout(Duration::from_secs(5), child.kill()).await, Ok(Ok(())));
                    if cleanup_confirmed { if let Some(song) = capacity.song { song.confirm_reaped(); } }
                    let outcome = self.hls_process_wait_outcome(result, stderr_task.take()).await;
                    if cleanup_confirmed && outcome == HlsProcessOutcome::Success {
                        if let Some(playlist) = capacity.producer_playlist {
                            if let Err(error) = playlist.publish(self, true).await {
                                tracing::warn!(channel_id = self.channel_id, %error, "private HLS segment publication failed after producer completion");
                                return HlsProcessOutcome::Failed("private HLS segment publication failed".into());
                            }
                        }
                    }
                    if let Some(failure) = &retained_failure_state {
                        let published = playlist_segment_uris(&self.working_dir).await.map_or(true, |uris| uris.iter().any(|uri| !burst_baseline.contains(uri)));
                        if let Ok(mut failure) = failure.lock() { failure.media_published = published; }
                    }
                    if outcome == HlsProcessOutcome::Success {
                        if let Some(remaining) = retained_remaining {
                            self.observe_startup_reserve(&burst_baseline, remaining, true).await;
                        }
                    }
                    return outcome;
                }
                _ = reserve_interval.tick(), if capacity.producer_playlist.is_some() || retained_remaining.is_some() && matches!(self.startup_readiness(), HlsStartupReadiness::Pending { .. }) => {
                    if let Some(playlist) = capacity.producer_playlist {
                        if let Err(error) = playlist.publish(self, false).await {
                            tracing::warn!(channel_id = self.channel_id, %error, "private HLS segment publication failed");
                            if self.terminate_hls_child(&mut child, stderr_task.take(), "private HLS segment publication failed").await {
                                if let Some(song) = capacity.song { song.confirm_reaped(); }
                            }
                            return HlsProcessOutcome::Failed("private HLS segment publication failed".into());
                        }
                    }
                    if let Some(remaining) = retained_remaining {
                        self.observe_startup_reserve(&burst_baseline, remaining, false).await;
                    }
                }
                () = self.cancel.notified() => {
                    if self.terminate_hls_child(&mut child, stderr_task.take(), "channel stream cancelled").await { if let Some(song) = capacity.song { song.confirm_reaped(); } }
                    return HlsProcessOutcome::Cancelled;
                }
                () = self.wait_for_idle_timeout(idle_timeout) => {
                    tracing::info!(
                        channel_number = %self.channel_number,
                        "stopping idle HLS session while ffmpeg is running"
                    );
                    if self.terminate_hls_child(&mut child, stderr_task.take(), "channel stream idle timeout").await { if let Some(song) = capacity.song { song.confirm_reaped(); } }
                    return HlsProcessOutcome::Cancelled;
                }
                () = self.wait_for_idle_timeout(watch_idle_timeout) => {
                    tracing::info!(
                        channel_number = %self.channel_number,
                        "reclaiming HLS transcode slot because the session is outside the watch window"
                    );
                    if self.terminate_hls_child(&mut child, stderr_task.take(), "channel stream outside watch window").await { if let Some(song) = capacity.song { song.confirm_reaped(); } }
                    if retained_remaining.is_some() && retained_media_was_published(&self.working_dir, &burst_baseline).await {
                        return HlsProcessOutcome::Cancelled;
                    }
                    return HlsProcessOutcome::Reclaimed;
                }
                () = self.reclaim_transcode.notified(), if capacity.active_transcode_slot_id.is_some() => {
                    if let Some(outcome) = self
                        .reclaim_hls_child_for_admission(
                            capacity.active_transcode_slot_id,
                            &mut child,
                            &mut stderr_task,
                            capacity.song,
                        )
                        .await
                    {
                        if retained_remaining.is_some() && retained_media_was_published(&self.working_dir, &burst_baseline).await {
                            return HlsProcessOutcome::Cancelled;
                        }
                        return outcome;
                    }
                }
                _ = cleanup_interval.tick() => {
                    if let Some(playlist) = capacity.producer_playlist {
                        if let Err(error) = playlist.publish(self, false).await {
                            tracing::warn!(channel_id = self.channel_id, %error, "private HLS segment publication failed");
                            if self.terminate_hls_child(&mut child, stderr_task.take(), "private HLS segment publication failed").await {
                                if let Some(song) = capacity.song { song.confirm_reaped(); }
                            }
                            return HlsProcessOutcome::Failed("private HLS segment publication failed".into());
                        }
                    }
                    if let (Some(pool), Some(tracker)) = (delivery_pool, delivery_tracker.as_mut()) {
                        if let Some(observation) = self
                            .sample_hls_delivery(pool, tracker, delivery_started_at)
                            .await
                        {
                            if let Some(recovery) = delivery_recovery {
                                if let Some(outcome) = self
                                    .apply_hls_delivery_recovery(
                                        pool,
                                        recovery,
                                        &observation,
                                        &mut fallback_verified,
                                        &mut child,
                                        &mut stderr_task,
                                        capacity.song,
                                    )
                                    .await
                                {
                                    return outcome;
                                }
                            }
                        }
                    }
                    if capacity.work_ahead_reservation
                        .as_ref()
                        .is_some_and(|reservation| reservation.is_some())
                        && playlist_duration_excluding_uris(&self.working_dir, &burst_baseline)
                            .await
                            .is_ok_and(|duration| {
                                duration >= Duration::from_secs(HLS_REMOTE_PROMOTION_BUFFER_SECONDS)
                            })
                        && capacity.active_transcode_slot_id
                            .is_some_and(promote_hls_transcode_to_realtime)
                    {
                        if let Some(reservation) = capacity.work_ahead_reservation.as_mut() {
                            drop((**reservation).take());
                        }
                        tracing::debug!(
                            channel_number = %self.channel_number,
                            buffered_seconds = HLS_REMOTE_PROMOTION_BUFFER_SECONDS,
                            "promoted continuous remote HLS producer to realtime"
                        );
                    }
                    let _ = self.trim_playlist_inner().await;
                }
            }
        }
    }

    async fn observe_startup_reserve(
        &self,
        baseline: &HashSet<String>,
        remaining: Duration,
        completed: bool,
    ) {
        if !matches!(
            self.startup_readiness(),
            HlsStartupReadiness::Pending { .. }
        ) {
            return;
        }
        if completed_segment_reserve(&self.working_dir, baseline)
            .await
            .is_ok_and(|(duration, count)| {
                retained_startup_reserve_ready(duration, count, remaining, completed)
                    && (completed && remaining <= Duration::from_secs(8)
                        || count >= self.startup_minimum_segments.load(Ordering::Relaxed))
            })
        {
            self.set_startup_readiness(HlsStartupReadiness::Ready);
        }
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "Delivery recovery carries the same bounded HLS inputs and owned song picture as initial playback"
    )]
    async fn apply_hls_delivery_recovery(
        &self,
        pool: &DbPool,
        recovery: HlsDeliveryRecovery,
        observation: &HlsDeliveryObservation,
        fallback_verified: &mut bool,
        child: &mut RemoteStreamProcess,
        stderr_task: &mut Option<tokio::task::JoinHandle<String>>,
        song: Option<&crate::song_background::NativeSongBackground>,
    ) -> Option<HlsProcessOutcome> {
        match hls_delivery_recovery_action(recovery, observation, *fallback_verified) {
            HlsDeliveryRecoveryAction::RestartWithSoftware(software_video_scope) => {
                if self
                    .terminate_hls_child(
                        child,
                        stderr_task.take(),
                        "live HLS software video fallback",
                    )
                    .await
                {
                    if let Some(song) = song {
                        song.confirm_reaped();
                    }
                }
                Some(HlsProcessOutcome::DeliverySoftwareFallback {
                    detected_at: observation.observed_at,
                    max_video_height: recovery.max_video_height,
                    software_video_scope,
                })
            }
            HlsDeliveryRecoveryAction::RestartAtFallback => {
                if self
                    .terminate_hls_child(child, stderr_task.take(), "live HLS delivery fallback")
                    .await
                {
                    if let Some(song) = song {
                        song.confirm_reaped();
                    }
                }
                Some(HlsProcessOutcome::DeliveryFallback {
                    detected_at: observation.observed_at,
                    software_video_scope: recovery.software_video_scope,
                })
            }
            HlsDeliveryRecoveryAction::ClearFallback => {
                let _ = clear_hls_delivery_fallback(
                    pool,
                    recovery.media_item_id,
                    recovery.software_video_scope,
                )
                .await;
                None
            }
            HlsDeliveryRecoveryAction::VerifyFallback => {
                let detected_at = recovery
                    .fallback_detected_at
                    .unwrap_or(observation.last_under_run_at);
                match persist_hls_delivery_fallback(
                    pool,
                    recovery.media_item_id,
                    detected_at,
                    observation.observed_at,
                    recovery.max_video_height,
                    recovery.software_video_scope,
                )
                .await
                {
                    Ok(()) => *fallback_verified = true,
                    Err(error) => tracing::warn!(
                        %error,
                        channel_number = %self.channel_number,
                        media_item_id = recovery.media_item_id,
                        "failed to persist verified live HLS delivery fallback"
                    ),
                }
                None
            }
            HlsDeliveryRecoveryAction::None => None,
        }
    }

    async fn initial_hls_delivery_tracker(
        &self,
        enabled: bool,
        active_under_run_at: Option<DateTime<Utc>>,
    ) -> Option<HlsDeliveryTracker> {
        if !enabled {
            return None;
        }
        let baseline = hls_delivery_segments(&self.working_dir)
            .await
            .ok()
            .and_then(|segments| segments.last().map(|segment| segment.sequence));
        Some(HlsDeliveryTracker::new(baseline, active_under_run_at))
    }

    async fn sample_hls_delivery(
        &self,
        pool: &DbPool,
        tracker: &mut HlsDeliveryTracker,
        started_at: tokio::time::Instant,
    ) -> Option<HlsDeliveryObservation> {
        match hls_delivery_segments(&self.working_dir).await {
            Ok(segments) => {
                if let Some(observation) =
                    tracker.observe(started_at.elapsed(), &segments, Utc::now())
                {
                    self.record_hls_delivery_transition(pool, &observation)
                        .await;
                    return Some(observation);
                }
            }
            Err(error) => tracing::debug!(
                %error,
                channel_number = %self.channel_number,
                "could not sample HLS delivery progress"
            ),
        }
        None
    }

    async fn record_hls_delivery_transition(
        &self,
        pool: &DbPool,
        observation: &HlsDeliveryObservation,
    ) {
        match (observation.state, observation.transitioned) {
            (HlsDeliveryState::UnderRun, true) => tracing::warn!(
                channel_number = %self.channel_number,
                wall_seconds = observation.wall_seconds,
                media_seconds = observation.media_seconds,
                delivery_ratio_percent = observation.delivery_ratio_percent,
                no_progress_seconds = observation.no_progress_seconds,
                "live HLS delivery is not keeping pace with wall time"
            ),
            (HlsDeliveryState::Recovered, true) => tracing::info!(
                channel_number = %self.channel_number,
                wall_seconds = observation.wall_seconds,
                media_seconds = observation.media_seconds,
                delivery_ratio_percent = observation.delivery_ratio_percent,
                "live HLS delivery recovered"
            ),
            (HlsDeliveryState::UnderRun | HlsDeliveryState::Recovered, false) => {}
        }

        let persisted = tokio::time::timeout(
            Duration::from_secs(2),
            persist_hls_delivery_observation(pool, self.channel_id, observation),
        )
        .await;
        match persisted {
            Ok(Ok(())) => {}
            Ok(Err(error)) => tracing::warn!(
                %error,
                channel_number = %self.channel_number,
                "failed to persist HLS delivery observation"
            ),
            Err(_) => tracing::warn!(
                channel_number = %self.channel_number,
                "timed out persisting HLS delivery observation"
            ),
        }
    }

    async fn hls_process_wait_outcome(
        &self,
        result: std::io::Result<std::process::ExitStatus>,
        stderr_task: Option<tokio::task::JoinHandle<String>>,
    ) -> HlsProcessOutcome {
        match result {
            Ok(status) if status.success() => {
                let _ = self.trim_playlist_inner().await;
                HlsProcessOutcome::Success
            }
            Ok(status) => {
                let stderr = await_stderr_tail(stderr_task).await;
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
                HlsProcessOutcome::Failed(error_message)
            }
            Err(error) => {
                let stderr = await_stderr_tail(stderr_task).await;
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
                HlsProcessOutcome::Failed(error_message)
            }
        }
    }

    fn spawn_hls_child_with_remote(
        &self,
        command: &FfmpegCommand,
        remote_producer: Option<&RemoteStreamCommand>,
    ) -> std::io::Result<RemoteStreamProcess> {
        spawn_ffmpeg_with_remote_input(
            command,
            Some(&self.working_dir),
            remote_producer,
            RemoteStreamSpawnContext::hls(&self.channel_number),
        )
        .map_err(std::io::Error::other)
    }

    async fn reclaim_hls_child_for_admission(
        &self,
        active_transcode_slot_id: Option<u64>,
        child: &mut RemoteStreamProcess,
        stderr_task: &mut Option<tokio::task::JoinHandle<String>>,
        song: Option<&crate::song_background::NativeSongBackground>,
    ) -> Option<HlsProcessOutcome> {
        let slot_id = active_transcode_slot_id?;
        if !self.take_reclaim_request(slot_id) {
            return None;
        }
        tracing::info!(
            channel_number = %self.channel_number,
            active_transcode_slot_id = slot_id,
            "reclaiming HLS transcode slot for admission control"
        );
        if self
            .terminate_hls_child(child, stderr_task.take(), "hls transcode capacity reclaim")
            .await
        {
            if let Some(song) = song {
                song.confirm_reaped();
            }
        }
        Some(HlsProcessOutcome::Reclaimed)
    }

    async fn wait_for_idle_timeout(&self, idle_timeout: Option<Duration>) {
        let Some(timeout) = idle_timeout else {
            std::future::pending::<()>().await;
            return;
        };

        loop {
            let idle_for = self.idle_for_duration();
            if idle_for >= timeout {
                return;
            }

            tokio::time::sleep(timeout.saturating_sub(idle_for)).await;
        }
    }

    fn idle_for_duration(&self) -> Duration {
        let last_access_ms = self.last_access_ms.load(Ordering::Relaxed);
        let idle_ms = Utc::now()
            .timestamp_millis()
            .saturating_sub(last_access_ms)
            .max(0);
        Duration::from_millis(u64::try_from(idle_ms).unwrap_or(u64::MAX))
    }

    fn was_recently_accessed(&self) -> bool {
        self.idle_for_duration() < HLS_ACTIVE_WATCH_WINDOW
    }

    async fn load_target_frame_rate(
        &self,
        pool: &DbPool,
        context: &str,
    ) -> RepositoryResult<Option<FrameRate>> {
        match load_channel_target_frame_rate(pool, self.channel_id).await {
            Ok(frame_rate) => Ok(frame_rate),
            Err(error) => {
                tracing::error!(
                    %error,
                    channel_id = self.channel_id,
                    channel_number = %self.channel_number,
                    context,
                    "failed to load channel target frame rate; refusing to build HLS FFmpeg command"
                );
                Err(error)
            }
        }
    }

    fn active_transcode_reacquire_delay(&self) -> Option<Duration> {
        active_transcode_reacquire_delay(
            &self.reclaim_reacquire_deferred_until_ms,
            Utc::now().timestamp_millis(),
        )
    }

    fn take_reclaim_request(&self, active_transcode_slot_id: u64) -> bool {
        take_reclaim_request(&self.reclaim_requested_slot_id, active_transcode_slot_id)
    }

    async fn terminate_hls_child(
        &self,
        child: &mut RemoteStreamProcess,
        stderr_task: Option<tokio::task::JoinHandle<String>>,
        reason: &str,
    ) -> bool {
        let killed = child.kill().await;
        if let Err(error) = &killed {
            tracing::warn!(
                %error,
                channel_number = %self.channel_number,
                reason,
                "failed to kill ffmpeg hls segmenter"
            );
        }

        let stderr = await_stderr_tail(stderr_task).await;
        if !stderr.is_empty() {
            tracing::debug!(
                stderr = %stderr,
                channel_number = %self.channel_number,
                reason,
                "ffmpeg hls segmenter stderr after termination"
            );
        }
        killed.is_ok()
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
            self.mpegts_output_ts_offset().await
        }
    }

    async fn next_output_ts_offset(
        &self,
        is_first: bool,
        next_mpegts_output_ts_offset: Option<Duration>,
    ) -> Option<Duration> {
        if is_first {
            return None;
        }

        if self.output_format == OutputFormat::Hls {
            return next_mpegts_output_ts_offset;
        }

        self.probe_output_ts_offset(false).await
    }

    async fn mpegts_output_ts_offset(&self) -> Option<Duration> {
        if self.output_format != OutputFormat::Hls {
            return None;
        }

        if let Some(offset) = self.ts_continuity.lock().await.pcr_output_ts_offset() {
            return Some(offset);
        }

        playlist_duration(&self.working_dir).await.ok().flatten()
    }

    async fn trim_playlist_inner(&self) -> Option<TrimmedPlaylist> {
        self.trim_playlist(false, Some(PLAYLIST_MAX_SEGMENTS), false)
            .await
    }

    async fn trim_playlist_for_append(
        &self,
        end_with_discontinuity: bool,
    ) -> Option<TrimmedPlaylist> {
        self.trim_playlist(end_with_discontinuity, None, true).await
    }

    async fn trim_playlist(
        &self,
        end_with_discontinuity: bool,
        maybe_max_segments: Option<usize>,
        write_playlist: bool,
    ) -> Option<TrimmedPlaylist> {
        let playlist_path = self.working_dir.join("live.m3u8");
        let playlist = tokio::fs::read_to_string(playlist_path).await.ok()?;
        let filter_before = Utc::now() - chrono::Duration::seconds(PLAYLIST_FILTER_BEFORE_SECONDS);
        let playlist_start = *self.playlist_start.lock().await;
        let trimmed = if self.output_format == OutputFormat::HlsMp4 {
            let mut init_cache = self.init_segment_cache.lock().await;
            let _ = refresh_init_segments(&self.working_dir, &mut init_cache).await;
            trim_playlist_contents(
                self.output_format,
                &playlist,
                playlist_start,
                filter_before,
                maybe_max_segments,
                end_with_discontinuity,
                Some(&init_cache),
            )?
        } else {
            trim_playlist_contents(
                self.output_format,
                &playlist,
                playlist_start,
                filter_before,
                maybe_max_segments,
                end_with_discontinuity,
                None,
            )?
        };

        if write_playlist {
            if let Err(error) =
                tokio::fs::write(self.working_dir.join("live.m3u8"), &trimmed.playlist).await
            {
                tracing::warn!(
                    %error,
                    channel_number = %self.channel_number,
                    "failed to write trimmed hls playlist before append"
                );
            } else {
                *self.playlist_start.lock().await = trimmed.playlist_start;
            }
        }

        if self.output_format == OutputFormat::Hls {
            let mut continuity = self.ts_continuity.lock().await;
            if let Err(error) =
                normalize_hls_ts_continuity(&self.working_dir, &trimmed, &mut continuity).await
            {
                tracing::warn!(
                    %error,
                    channel_number = %self.channel_number,
                    "failed to normalize hls mpeg-ts continuity counters and PCR"
                );
            }
        }

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

fn redact_remote_stderr(stderr: String, remote_stream: Option<&ResolvedRemoteStream>) -> String {
    match remote_stream {
        Some(stream) => redact_remote_stream_output(&stderr, stream),
        None => stderr,
    }
}

fn retained_hls_audio_selection(
    selection: PlaybackAudioSelection,
    audio: &[SourceAudioStream],
) -> PlaybackAudioSelection {
    match selection {
        PlaybackAudioSelection::Automatic | PlaybackAudioSelection::AutomaticSilence => audio
            .iter()
            .find(|audio| audio.default)
            .or_else(|| audio.first())
            .map_or(selection, |audio| {
                PlaybackAudioSelection::Exact(audio.index)
            }),
        selection => selection,
    }
}

fn classify_retained_hls_failure(
    stderr: &str,
    graph: Option<ersatzrs_ffmpeg::HardwareGraphOperation>,
    source: Option<&SourceVideoStream>,
    acceleration: HardwareAccelerationKind,
) -> RetainedHlsFailure {
    RetainedHlsFailure {
        media_published: false,
        graph: graph
            .and_then(|graph| ersatzrs_ffmpeg::GraphFailureKind::classify_runtime(graph, stderr)),
        decode: learned_hardware_decode_denial(false, acceleration, source, stderr),
    }
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
    remote_stream: Option<&ResolvedRemoteStream>,
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

    if let Some(remote_stream) = remote_stream {
        tracing::debug!(
            channel_number = %channel_number,
            executable = %command.executable,
            remote_source = %redacted_source_label(&remote_stream.source),
            nvidia_av1_decoders = ?av1_decoders,
            hardware_decode_denials = ?av1_denials,
            "spawning hls segmenter for remote av1 source with runtime capability rows"
        );
    } else {
        tracing::debug!(
            channel_number = %channel_number,
            executable = %command.executable,
            arguments = ?command.arguments,
            nvidia_av1_decoders = ?av1_decoders,
            hardware_decode_denials = ?av1_denials,
            "spawning hls segmenter for av1 source with runtime capability rows"
        );
    }
}

#[must_use]
pub fn apply_playback_profile_override(
    mut playback_profile: PlaybackProfile,
    override_kind: HlsPlaybackProfileOverride,
) -> PlaybackProfile {
    match override_kind {
        HlsPlaybackProfileOverride::BrowserCompatible => {
            playback_profile.profile.transcode_video = true;
            playback_profile.profile.transcode_audio = true;
            playback_profile.profile.video_format = FFmpegProfileVideoFormat::H264;
            playback_profile.profile.video_profile = Some("high".to_owned());
            playback_profile.profile.bit_depth = FFmpegProfileBitDepth::EightBit;
            playback_profile.profile.audio_format = FFmpegProfileAudioFormat::Aac;
        }
    }
    playback_profile
}

fn hls_software_video_fallback_available(playback_profile: &PlaybackProfile) -> bool {
    playback_profile.profile.transcode_video
        && playback_profile.profile.hardware_acceleration != HardwareAccelerationKind::None
        && playback_profile.profile.video_format == FFmpegProfileVideoFormat::H264
}

fn hls_delivery_software_scope(
    channel_id: i64,
    playback_profile: &PlaybackProfile,
) -> Result<HlsDeliverySoftwareScope, serde_json::Error> {
    let profile_json =
        serde_json::to_vec(&(&playback_profile.profile, &playback_profile.resolution))?;
    Ok(HlsDeliverySoftwareScope {
        channel_id,
        ffmpeg_profile_id: playback_profile.profile.id,
        profile_sha256: Sha256::digest(profile_json).into(),
    })
}

fn hls_delivery_adaptation_for_scope(
    mut adaptation: HlsDeliveryAdaptation,
    available_software_video_scope: Option<HlsDeliverySoftwareScope>,
    supports_quality_fallback: bool,
) -> Option<HlsDeliveryAdaptation> {
    if adaptation.software_video_scope.is_some()
        && adaptation.software_video_scope != available_software_video_scope
    {
        adaptation.software_video_scope = None;
    }
    if !supports_quality_fallback {
        adaptation.max_video_height = REMOTE_STREAM_DEFAULT_MAX_VIDEO_HEIGHT;
    }

    (adaptation.software_video_scope.is_some()
        || adaptation.max_video_height < REMOTE_STREAM_DEFAULT_MAX_VIDEO_HEIGHT)
        .then_some(adaptation)
}

fn apply_hls_software_video_fallback(playback_profile: &mut PlaybackProfile) {
    if playback_profile.profile.video_format != FFmpegProfileVideoFormat::H264 {
        return;
    }

    playback_profile.profile.hardware_acceleration = HardwareAccelerationKind::None;
    playback_profile.profile.video_preset = Some("veryfast".to_owned());
    playback_profile.profile.video_tune = None;
}

#[async_trait]
impl HlsSessionHandle for HlsSessionWorker {
    async fn segment_output(
        &self,
        segment: &str,
        started_at_millis: i64,
    ) -> Option<ersatzrs_core::hls::HlsSegmentOutput> {
        let playlist = tokio::fs::read_to_string(self.working_dir.join("live.m3u8"))
            .await
            .ok()?;
        if !playlist.contains("#EXT-X-PROGRAM-DATE-TIME:") {
            return None;
        }
        // Public playlists use the saved timeline anchor rather than FFmpeg's
        // spawn clock. Reuse that projection without trimming/writing media.
        let playlist_start = *self.playlist_start.lock().await;
        let segments = parsed_playlist_segments_from(&playlist, playlist_start);
        if !segments.iter().any(|entry| {
            entry.uri == segment && entry.start_time.timestamp_millis() == started_at_millis
        }) {
            return None;
        }
        let served = segments.into_iter().map(|entry| entry.uri).collect();
        self.output_status.lock().ok()?.get(segment, &served)
    }

    fn session_model(&self) -> Option<ersatzrs_core::hls::HlsSessionModel> {
        Some(ersatzrs_core::hls::HlsSessionModel {
            started_at: DateTime::from_timestamp_millis(
                self.started_at_ms.load(Ordering::Relaxed),
            )?,
            channel_number: self.channel_number.clone(),
            state: format!("{:?}", self.session_state()),
            transcoded_until: DateTime::from_timestamp_millis(
                self.transcoded_until_ms.load(Ordering::Relaxed),
            ),
            last_access: DateTime::from_timestamp_millis(
                self.last_access_ms.load(Ordering::Relaxed),
            )?,
        })
    }

    fn startup_readiness(&self) -> HlsStartupReadiness {
        self.startup
            .lock()
            .map_or(HlsStartupReadiness::Failed, |state| match *state {
                HlsStartupReadiness::Pending { deadline }
                    if std::time::Instant::now() >= deadline =>
                {
                    HlsStartupReadiness::Failed
                }
                state => state,
            })
    }
    // The port names this accessor `channel_number`, but explicit session
    // options may deliberately use a distinct registry identity.
    #[allow(clippy::misnamed_getters)]
    fn channel_number(&self) -> &str {
        &self.registry_key
    }

    fn touch(&self) {
        self.last_access_ms
            .store(Utc::now().timestamp_millis(), Ordering::Relaxed);
    }

    fn admission_status(&self) -> HlsAdmissionStatus {
        HlsAdmissionStatus::from_i32(self.admission_status.load(Ordering::Relaxed))
    }

    async fn trim_playlist(&self) -> Option<String> {
        if !self.working_dir_ready.load(Ordering::Acquire) {
            return None;
        }
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
/// # Errors
///
/// Returns [`ersatzrs_ffmpeg::UnsupportedSource`] when the selected source
/// carries a Dolby Vision configuration with no characterized conversion.
pub fn build_hls_command(
    media_path: &str,
    working_dir: &Path,
    output_format: OutputFormat,
    is_first_transcode: bool,
    troubleshooting: bool,
    hls_realtime: bool,
    start_position: Duration,
    duration: Option<Duration>,
    output_ts_offset: Option<Duration>,
    playback_profile: &PlaybackProfile,
    target_frame_rate: Option<FrameRate>,
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
    build_hls_command_with_opacity_clock(
        media_path,
        working_dir,
        output_format,
        is_first_transcode,
        troubleshooting,
        hls_realtime,
        start_position,
        duration,
        output_ts_offset,
        playback_profile,
        target_frame_rate,
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
/// # Errors
///
/// Returns [`ersatzrs_ffmpeg::UnsupportedSource`] when the selected source
/// carries a Dolby Vision configuration with no characterized conversion.
pub fn build_hls_command_with_opacity_clock(
    media_path: &str,
    working_dir: &Path,
    output_format: OutputFormat,
    is_first_transcode: bool,
    troubleshooting: bool,
    hls_realtime: bool,
    start_position: Duration,
    duration: Option<Duration>,
    output_ts_offset: Option<Duration>,
    playback_profile: &PlaybackProfile,
    target_frame_rate: Option<FrameRate>,
    metadata: OutputMetadata,
    runtime_capabilities: FfmpegCapabilityReport,
    source_stream: Option<SourceVideoStream>,
    source_audio_streams: Vec<ersatzrs_ffmpeg::SourceAudioStream>,
    audio_selection: impl Into<PlaybackAudioSelection>,
    subtitle_input: Option<InputFile>,
    watermark_inputs: Vec<(InputFile, ersatzrs_ffmpeg::WatermarkOverlay)>,
    graphics_overlays: Vec<GraphicsPipelineOverlay>,
    opacity_clock: Option<WatermarkOpacityClock>,
) -> Result<FfmpegCommand, ersatzrs_ffmpeg::UnsupportedSource> {
    build_hls_command_with_opacity_clock_and_song(
        media_path,
        working_dir,
        output_format,
        is_first_transcode,
        troubleshooting,
        hls_realtime,
        start_position,
        duration,
        output_ts_offset,
        playback_profile,
        target_frame_rate,
        metadata,
        runtime_capabilities,
        source_stream,
        source_audio_streams,
        audio_selection,
        subtitle_input,
        watermark_inputs,
        graphics_overlays,
        opacity_clock,
        None,
    )
}

/// Build native HLS with the shared occurrence-owned song picture.
/// # Errors
/// Returns an unsupported-source error for uncharacterized conversions.
#[allow(clippy::too_many_arguments)] // Preserve the existing HLS adapter arguments plus its owned song picture.
pub fn build_hls_command_with_opacity_clock_and_song(
    media_path: &str,
    working_dir: &Path,
    output_format: OutputFormat,
    is_first_transcode: bool,
    troubleshooting: bool,
    hls_realtime: bool,
    start_position: Duration,
    duration: Option<Duration>,
    output_ts_offset: Option<Duration>,
    playback_profile: &PlaybackProfile,
    target_frame_rate: Option<FrameRate>,
    metadata: OutputMetadata,
    runtime_capabilities: FfmpegCapabilityReport,
    source_stream: Option<SourceVideoStream>,
    source_audio_streams: Vec<ersatzrs_ffmpeg::SourceAudioStream>,
    audio_selection: impl Into<PlaybackAudioSelection>,
    subtitle_input: Option<InputFile>,
    watermark_inputs: Vec<(InputFile, ersatzrs_ffmpeg::WatermarkOverlay)>,
    graphics_overlays: Vec<GraphicsPipelineOverlay>,
    opacity_clock: Option<WatermarkOpacityClock>,
    song: Option<&crate::song_background::NativeSongBackground>,
) -> Result<FfmpegCommand, ersatzrs_ffmpeg::UnsupportedSource> {
    let audio_selection = audio_selection.into();
    build_hls_command_with_explicit_discontinuity(
        media_path,
        working_dir,
        output_format,
        is_first_transcode,
        troubleshooting,
        hls_realtime,
        start_position,
        duration,
        output_ts_offset,
        playback_profile,
        target_frame_rate,
        metadata,
        runtime_capabilities,
        source_stream,
        source_audio_streams,
        audio_selection,
        subtitle_input,
        watermark_inputs,
        graphics_overlays,
        HlsDiscontinuity::from(hls_output_discontinuity(
            output_format,
            is_first_transcode,
            output_ts_offset,
            false,
        )),
        opacity_clock,
        song,
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum HlsDiscontinuity {
    Present,
    Absent,
}

impl HlsDiscontinuity {
    const fn enabled(self) -> bool {
        matches!(self, Self::Present)
    }
}

impl From<bool> for HlsDiscontinuity {
    fn from(enabled: bool) -> Self {
        if enabled {
            Self::Present
        } else {
            Self::Absent
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn build_hls_command_with_explicit_discontinuity(
    media_path: &str,
    _working_dir: &Path,
    output_format: OutputFormat,
    is_first_transcode: bool,
    troubleshooting: bool,
    hls_realtime: bool,
    start_position: Duration,
    duration: Option<Duration>,
    output_ts_offset: Option<Duration>,
    playback_profile: &PlaybackProfile,
    target_frame_rate: Option<FrameRate>,
    metadata: OutputMetadata,
    runtime_capabilities: FfmpegCapabilityReport,
    source_stream: Option<SourceVideoStream>,
    source_audio_streams: Vec<ersatzrs_ffmpeg::SourceAudioStream>,
    audio_selection: PlaybackAudioSelection,
    subtitle_input: Option<InputFile>,
    watermark_inputs: Vec<(InputFile, ersatzrs_ffmpeg::WatermarkOverlay)>,
    graphics_overlays: Vec<GraphicsPipelineOverlay>,
    hls_discontinuity: HlsDiscontinuity,
    opacity_clock: Option<WatermarkOpacityClock>,
    song: Option<&crate::song_background::NativeSongBackground>,
) -> Result<FfmpegCommand, ersatzrs_ffmpeg::UnsupportedSource> {
    build_hls_command_with_input_explicit_discontinuity(
        InputFile::video(media_path.to_string()),
        output_format,
        is_first_transcode,
        troubleshooting,
        hls_realtime,
        start_position,
        duration,
        output_ts_offset,
        playback_profile,
        target_frame_rate,
        metadata,
        runtime_capabilities,
        source_stream,
        source_audio_streams,
        audio_selection,
        subtitle_input,
        watermark_inputs,
        graphics_overlays,
        hls_discontinuity,
        opacity_clock,
        HlsInputTimeline::Original,
        song,
    )
    .map(|(command, _)| command)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum HlsInputTimeline {
    Original,
    RetainedIntervals,
}

fn apply_hls_bootstrap_readburst(
    request: &mut PipelineRequest,
    initial_bootstrap: bool,
    continuous_remote: bool,
    minimum_segments: usize,
) {
    if !initial_bootstrap
        || request.state.retained_stream_timestamps
        || continuous_remote
        || request.state.realtime_initial_burst.is_some()
        || request.inputs.iter().any(|input| {
            input
                .input_options
                .iter()
                .any(|option| option == "-readrate_initial_burst")
        })
    {
        return;
    }
    // Keep pacing in typed generator state: supplied input options deliberately
    // make source-caption timing opaque. The four-second startup segments must
    // fit within one ordinary work window. Retained/remote policies stay separate.
    let burst_seconds = u64::try_from(minimum_segments)
        .unwrap_or(u64::MAX)
        .saturating_mul(4)
        .min(HLS_WORK_AHEAD_TRANSCODE_SECONDS);
    request.state.realtime_initial_burst = Some(Duration::from_secs(burst_seconds));
}

#[allow(clippy::too_many_arguments)]
fn build_hls_command_with_input_explicit_discontinuity(
    input: InputFile,
    output_format: OutputFormat,
    is_first_transcode: bool,
    troubleshooting: bool,
    hls_realtime: bool,
    start_position: Duration,
    duration: Option<Duration>,
    output_ts_offset: Option<Duration>,
    playback_profile: &PlaybackProfile,
    target_frame_rate: Option<FrameRate>,
    metadata: OutputMetadata,
    runtime_capabilities: FfmpegCapabilityReport,
    source_stream: Option<SourceVideoStream>,
    source_audio_streams: Vec<ersatzrs_ffmpeg::SourceAudioStream>,
    audio_selection: PlaybackAudioSelection,
    subtitle_input: Option<InputFile>,
    watermark_inputs: Vec<(InputFile, ersatzrs_ffmpeg::WatermarkOverlay)>,
    graphics_overlays: Vec<GraphicsPipelineOverlay>,
    hls_discontinuity: HlsDiscontinuity,
    opacity_clock: Option<WatermarkOpacityClock>,
    input_timeline: HlsInputTimeline,
    song: Option<&crate::song_background::NativeSongBackground>,
) -> Result<
    (
        FfmpegCommand,
        Option<ersatzrs_ffmpeg::SourceTeletextInputPlan>,
    ),
    ersatzrs_ffmpeg::UnsupportedSource,
> {
    let (acceleration, request) = build_hls_request_with_input_explicit_discontinuity(
        input,
        output_format,
        is_first_transcode,
        troubleshooting,
        hls_realtime,
        start_position,
        duration,
        output_ts_offset,
        playback_profile,
        target_frame_rate,
        metadata,
        runtime_capabilities,
        source_stream,
        source_audio_streams,
        audio_selection,
        subtitle_input,
        watermark_inputs,
        graphics_overlays,
        hls_discontinuity,
        opacity_clock,
        input_timeline,
        song,
    );
    ersatzrs_ffmpeg::build_pipeline_command_with_source_teletext_plan(acceleration, request)
}

#[allow(clippy::too_many_arguments)] // Keep the existing HLS builder's completed policy at one shared boundary.
fn build_hls_request_with_input_explicit_discontinuity(
    input: InputFile,
    output_format: OutputFormat,
    is_first_transcode: bool,
    troubleshooting: bool,
    hls_realtime: bool,
    start_position: Duration,
    duration: Option<Duration>,
    output_ts_offset: Option<Duration>,
    playback_profile: &PlaybackProfile,
    target_frame_rate: Option<FrameRate>,
    metadata: OutputMetadata,
    runtime_capabilities: FfmpegCapabilityReport,
    source_stream: Option<SourceVideoStream>,
    source_audio_streams: Vec<ersatzrs_ffmpeg::SourceAudioStream>,
    audio_selection: PlaybackAudioSelection,
    subtitle_input: Option<InputFile>,
    watermark_inputs: Vec<(InputFile, ersatzrs_ffmpeg::WatermarkOverlay)>,
    graphics_overlays: Vec<GraphicsPipelineOverlay>,
    hls_discontinuity: HlsDiscontinuity,
    opacity_clock: Option<WatermarkOpacityClock>,
    input_timeline: HlsInputTimeline,
    song: Option<&crate::song_background::NativeSongBackground>,
) -> (HardwareAccelerationKind, PipelineRequest) {
    let seek_after_input = input.seek_after_input;
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
    request = request.with_source_audio_streams(source_audio_streams);
    match audio_selection {
        PlaybackAudioSelection::Exact(stream_index) => {
            if let Some(audio_input) = request
                .inputs
                .iter_mut()
                .find(|input| input.kind == InputKind::Audio)
            {
                *audio_input = audio_input.clone().with_stream_index(stream_index);
            }
        }
        PlaybackAudioSelection::Disabled => request = request.without_audio(),
        PlaybackAudioSelection::Automatic | PlaybackAudioSelection::AutomaticSilence => {}
    }
    if let Some(source_stream) = source_stream {
        request = request.with_source_video_stream(source_stream);
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
    if input_timeline == HlsInputTimeline::RetainedIntervals {
        request = request.with_retained_stream_input();
    }
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
        mpegts_segments: None,
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
        discontinuity: hls_discontinuity.enabled(),
        troubleshooting,
    });

    apply_automatic_null_audio(&mut request, playback_profile, audio_selection);
    if let Some(song) = song {
        if input_timeline == HlsInputTimeline::RetainedIntervals {
            request.state.song_progress_start = Some(start_position);
        }
        song.apply(&mut request);
    }
    (playback_profile.profile.hardware_acceleration, request)
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
        ffmpeg_executable: runtime_capabilities
            .runtime_identity
            .as_ref()
            .map(|identity| identity.ffmpeg_path.clone()),
        runtime_generation: runtime_capabilities
            .runtime_identity
            .as_ref()
            .map(|identity| identity.generation),
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
        mpegts_segments: None,
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
        discontinuity: hls_output_discontinuity(
            output_format,
            is_first_transcode,
            output_ts_offset,
            true,
        ),
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

pub struct MpegTsSlateCommandRequest<'a> {
    pub working_dir: &'a Path,
    pub duration: Duration,
    pub playback_profile: &'a PlaybackProfile,
    pub target_frame_rate: Option<FrameRate>,
    pub metadata: OutputMetadata,
    pub runtime_capabilities: FfmpegCapabilityReport,
    pub message: &'a str,
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

pub async fn build_mpeg_ts_slate_command(
    request: MpegTsSlateCommandRequest<'_>,
) -> Option<FfmpegCommand> {
    if let Err(error) = tokio::fs::create_dir_all(request.working_dir).await {
        tracing::warn!(%error, path = %request.working_dir.display(), "failed to create mpeg-ts slate asset directory");
        return None;
    }

    let background_path = request.working_dir.join("background.png");
    if let Err(error) = tokio::fs::write(&background_path, ERROR_BACKGROUND_PNG).await {
        tracing::warn!(%error, path = %background_path.display(), "failed to write mpeg-ts slate background");
        return None;
    }

    let subtitle_path = request.working_dir.join("slate.ass");
    let subtitle = hls_error_ass(
        request.message,
        playback_frame_size(request.playback_profile),
    );
    if let Err(error) = tokio::fs::write(&subtitle_path, subtitle).await {
        tracing::warn!(%error, path = %subtitle_path.display(), "failed to write mpeg-ts slate subtitle");
        return None;
    }

    Some(build_mpeg_ts_slate_command_from_assets(
        MpegTsSlateCommandAssetRequest {
            background_path: &background_path,
            subtitle_path: &subtitle_path,
            duration: request.duration,
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

struct MpegTsSlateCommandAssetRequest<'a> {
    background_path: &'a Path,
    subtitle_path: &'a Path,
    duration: Duration,
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
        Some(&generated_error_video_stream(frame_rate)),
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
        source_video_stream: Some(generated_error_video_stream(frame_rate)),
        source_audio_streams: vec![generated_slug_audio_stream(audio_sample_rate)],
        ffmpeg_executable: request
            .runtime_capabilities
            .runtime_identity
            .as_ref()
            .map(|identity| identity.ffmpeg_path.clone()),
        runtime_generation: request
            .runtime_capabilities
            .runtime_identity
            .as_ref()
            .map(|identity| identity.generation),
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
        mpegts_segments: None,
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
        discontinuity: hls_output_discontinuity(
            request.output_format,
            request.is_first_transcode,
            request.output_ts_offset,
            true,
        ),
        troubleshooting: false,
    });

    build_pipeline_for_acceleration(
        request.playback_profile.profile.hardware_acceleration,
        pipeline_request.clone(),
    )
    .command(&pipeline_request.inputs)
}

#[must_use]
fn build_mpeg_ts_slate_command_from_assets(
    request: MpegTsSlateCommandAssetRequest<'_>,
) -> FfmpegCommand {
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
        Some(&generated_error_video_stream(frame_rate)),
        request.target_frame_rate,
        StreamingMode::TransportStream,
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
    desired.realtime_mode = RealtimeMode::On;
    desired.frame_rate = Some(frame_rate);
    desired.scaled_size = frame_size;
    desired.frame_size = frame_size;
    desired.cropped_size = None;

    let mut pipeline_request = PipelineRequest {
        inputs: vec![video_input, audio_input, subtitle_input],
        state: ersatzrs_ffmpeg::FfmpegState::default(),
        desired,
        source_video_stream: Some(generated_error_video_stream(frame_rate)),
        source_audio_streams: vec![generated_slug_audio_stream(audio_sample_rate)],
        ffmpeg_executable: request
            .runtime_capabilities
            .runtime_identity
            .as_ref()
            .map(|identity| identity.ffmpeg_path.clone()),
        runtime_generation: request
            .runtime_capabilities
            .runtime_identity
            .as_ref()
            .map(|identity| identity.generation),
        runtime_capabilities: Some(request.runtime_capabilities),
        watermark: None,
        watermarks: Vec::new(),
        watermark_opacity_clock: WatermarkOpacityClock::default(),
        drawvg_filters: Vec::new(),
        graphics_overlays: Vec::new(),
        subtitle_fonts_dir: Some(subtitle_fonts_dir()),
    };
    pipeline_request.state.output_format = OutputFormat::MpegTs;
    pipeline_request.state.duration = Some(request.duration);
    pipeline_request.state.metadata = request.metadata;
    pipeline_request.state.report_progress = false;
    if let Ok(thread_count) = u32::try_from(request.playback_profile.profile.thread_count) {
        if thread_count > 0 {
            pipeline_request.state.thread_count = Some(thread_count);
        }
    }

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

fn generated_error_video_stream(frame_rate: FrameRate) -> SourceVideoStream {
    let mut stream = generated_slug_video_stream(ERROR_BACKGROUND_FRAME_SIZE, frame_rate);
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

fn hls_output_discontinuity(
    output_format: OutputFormat,
    is_first_transcode: bool,
    output_ts_offset: Option<Duration>,
    append_requires_discontinuity: bool,
) -> bool {
    if is_first_transcode {
        return false;
    }

    if output_format == OutputFormat::Hls
        && !append_requires_discontinuity
        && output_ts_offset.is_some_and(|offset| !offset.is_zero())
    {
        return false;
    }

    append_requires_discontinuity
        || output_format != OutputFormat::Hls
        || output_ts_offset.is_none_or(|offset| offset.is_zero())
}

fn advance_mpegts_output_ts_offset(
    output_format: OutputFormat,
    current: Option<Duration>,
    appended_duration: Duration,
) -> Option<Duration> {
    if output_format != OutputFormat::Hls {
        return current;
    }

    Some(
        current
            .unwrap_or(Duration::ZERO)
            .saturating_add(appended_duration),
    )
}

fn refused_hls_process_outcome(error: &ersatzrs_ffmpeg::UnsupportedSource) -> HlsProcessOutcome {
    HlsProcessOutcome::Failed(error.to_string())
}

fn advance_after_hls_error_slate(
    request_time: DateTime<Utc>,
    duration: Duration,
    state: HlsSessionState,
    has_slug: bool,
    output_format: OutputFormat,
    current_output_offset: Option<Duration>,
) -> (DateTime<Utc>, HlsSessionState, Option<Duration>) {
    (
        add_std_duration(request_time, duration),
        next_hls_session_state(state, false, has_slug),
        advance_mpegts_output_ts_offset(output_format, current_output_offset, duration),
    )
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

fn live_source_admission_delay(
    is_live: bool,
    transcode_cursor: DateTime<Utc>,
    wall_clock_now: DateTime<Utc>,
) -> Option<Duration> {
    is_live
        .then(|| (transcode_cursor - wall_clock_now).to_std().ok())
        .flatten()
        .filter(|delay| !delay.is_zero())
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
    let request_time = hls_transcode_request_time(state, transcoded_until, wall_clock_now);
    let state = if realtime {
        state.throttle()
    } else {
        state.accelerate()
    };

    HlsTranscodeDecision {
        state,
        realtime,
        working_ahead: !realtime,
        request_time,
        start_at_zero: state.start_at_zero(),
    }
}

fn hls_transcode_request_time(
    state: HlsSessionState,
    transcoded_until: DateTime<Utc>,
    wall_clock_now: DateTime<Utc>,
) -> DateTime<Utc> {
    if matches!(
        state,
        HlsSessionState::SeekAndWorkAhead | HlsSessionState::PlayoutUpdated
    ) {
        wall_clock_now
    } else {
        transcoded_until
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

struct AdmissionStatusGuard<'a> {
    worker: &'a HlsSessionWorker,
}

impl<'a> AdmissionStatusGuard<'a> {
    fn new(worker: &'a HlsSessionWorker) -> Self {
        Self { worker }
    }
}

impl Drop for AdmissionStatusGuard<'_> {
    fn drop(&mut self) {
        self.worker
            .set_admission_status(HlsAdmissionStatus::Waiting);
    }
}

fn active_transcode_reacquire_delay(
    deferred_until_ms: &AtomicI64,
    now_ms: i64,
) -> Option<Duration> {
    let remaining_ms = deferred_until_ms
        .load(Ordering::Acquire)
        .saturating_sub(now_ms);
    (remaining_ms > 0).then(|| Duration::from_millis(u64::try_from(remaining_ms).unwrap_or(0)))
}

fn take_reclaim_request(
    reclaim_requested_slot_id: &AtomicU64,
    active_transcode_slot_id: u64,
) -> bool {
    match reclaim_requested_slot_id.compare_exchange(
        active_transcode_slot_id,
        0,
        Ordering::AcqRel,
        Ordering::Acquire,
    ) {
        Ok(_) => true,
        Err(stale_slot_id) => {
            if stale_slot_id != 0 {
                let _ = reclaim_requested_slot_id.compare_exchange(
                    stale_slot_id,
                    0,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                );
            }
            false
        }
    }
}

fn transcode_watch_idle_timeout(session_idle_timeout: Option<Duration>) -> Option<Duration> {
    session_idle_timeout
        .is_some_and(|timeout| timeout > HLS_ACTIVE_WATCH_WINDOW)
        .then_some(HLS_ACTIVE_WATCH_WINDOW)
}

pub(crate) async fn load_hls_work_ahead_limit(pool: &DbPool) -> RepositoryResult<Option<i32>> {
    let value = sqlx::query_scalar::<_, Option<String>>(
        r#"SELECT "Value" FROM "ConfigElement" WHERE "Key" = ?"#,
    )
    .bind(ersatzrs_domain::CONFIG_KEY_FFMPEG_WORK_AHEAD_SEGMENTERS)
    .fetch_optional(pool)
    .await
    .map_err(|error| map_data_access_error("HLS work-ahead limit", &error))?;
    let Some(value) = value else {
        return Ok(None);
    };
    let value = value.ok_or_else(|| RepositoryError::Decode {
        kind: "HLS work-ahead limit",
        value: "persisted value is null".to_owned(),
    })?;
    let value = value
        .trim()
        .parse::<i32>()
        .map_err(|error| RepositoryError::Decode {
            kind: "HLS work-ahead limit",
            value: error.to_string(),
        })?;
    if value < 0 {
        return Err(RepositoryError::Decode {
            kind: "HLS work-ahead limit",
            value: format!("expected a value greater than or equal to 0, found {value}"),
        });
    }
    Ok(Some(value))
}

async fn load_channel_slug_seconds(
    pool: &DbPool,
    channel_id: i64,
) -> RepositoryResult<Option<f64>> {
    let value = sqlx::query_scalar::<_, Option<f64>>(
        r#"SELECT "SlugSeconds" FROM "Channel" WHERE "Id" = ?"#,
    )
    .bind(channel_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| map_data_access_error("channel slug duration", &error))?;
    match value.flatten() {
        None => Ok(None),
        Some(seconds) if seconds.is_finite() && seconds >= 0.0 => Ok(Some(seconds)),
        Some(seconds) => Err(RepositoryError::Decode {
            kind: "channel slug duration",
            value: format!("expected a finite, non-negative value, found {seconds}"),
        }),
    }
}

async fn load_channel_name(pool: &DbPool, channel_id: i64) -> RepositoryResult<Option<String>> {
    let value =
        sqlx::query_scalar::<_, Option<String>>(r#"SELECT "Name" FROM "Channel" WHERE "Id" = ?"#)
            .bind(channel_id)
            .fetch_optional(pool)
            .await
            .map_err(|error| map_data_access_error("channel name", &error))?;
    Ok(value.flatten())
}

pub(crate) async fn time_shift_on_demand_playout(
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

    let mut tx = crate::db::begin_write(pool).await?;
    sqlx::query(r#"DELETE FROM "PlayoutItem" WHERE "PlayoutId" = ? AND "Finish" < ?"#)
        .bind(playout_id)
        .bind(encode_datetime(checkpoint))
        .execute(&mut *tx)
        .await?;
    shift_playout_items(&mut tx, playout_id, offset).await?;
    shift_playout_gaps(&mut tx, playout_id, checkpoint, offset).await?;
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

async fn shift_playout_gaps(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    playout_id: i64,
    checkpoint: DateTime<Utc>,
    offset: chrono::Duration,
) -> Result<(), sqlx::Error> {
    sqlx::query(r#"DELETE FROM "PlayoutGap" WHERE "PlayoutId" = ? AND "Finish" < ?"#)
        .bind(playout_id)
        .bind(encode_datetime(checkpoint))
        .execute(&mut **tx)
        .await?;
    let rows =
        sqlx::query(r#"SELECT "Id", "Start", "Finish" FROM "PlayoutGap" WHERE "PlayoutId" = ?"#)
            .bind(playout_id)
            .fetch_all(&mut **tx)
            .await?;
    for row in rows {
        sqlx::query(r#"UPDATE "PlayoutGap" SET "Start" = ?, "Finish" = ? WHERE "Id" = ?"#)
            .bind(shift_required_datetime(
                "playout gap Start",
                &row.try_get::<String, _>("Start")?,
                offset,
            )?)
            .bind(shift_required_datetime(
                "playout gap Finish",
                &row.try_get::<String, _>("Finish")?,
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
    if let Some(segment) = last_playlist_segment(working_dir).await? {
        return Ok(Some(segment));
    }

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

async fn last_playlist_segment(working_dir: &Path) -> io::Result<Option<PathBuf>> {
    let playlist = match tokio::fs::read_to_string(working_dir.join("live.m3u8")).await {
        Ok(playlist) => playlist,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };

    Ok(playlist
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .rfind(|line| {
            matches!(
                Path::new(line)
                    .extension()
                    .and_then(|extension| extension.to_str()),
                Some("ts" | "m4s")
            )
        })
        .map(|file_name| working_dir.join(file_name)))
}

async fn playlist_duration(working_dir: &Path) -> io::Result<Option<Duration>> {
    let playlist = match tokio::fs::read_to_string(working_dir.join("live.m3u8")).await {
        Ok(playlist) => playlist,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };

    let mut duration = chrono::Duration::zero();
    for line in playlist.lines().map(str::trim) {
        if line.starts_with("#EXTINF:") {
            if let Some(extinf_duration) = parse_extinf_duration(line) {
                duration += extinf_duration;
            }
        }
    }

    Ok(duration
        .to_std()
        .ok()
        .filter(|duration| !duration.is_zero()))
}

async fn playlist_segment_uris(working_dir: &Path) -> io::Result<HashSet<String>> {
    let playlist = match tokio::fs::read_to_string(working_dir.join("live.m3u8")).await {
        Ok(playlist) => playlist,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(HashSet::new()),
        Err(error) => return Err(error),
    };
    Ok(parsed_playlist_segments(&playlist)
        .into_iter()
        .map(|segment| segment.uri)
        .collect())
}

async fn retained_media_was_published(working_dir: &Path, baseline: &HashSet<String>) -> bool {
    playlist_segment_uris(working_dir)
        .await
        .map_or(true, |uris| uris.iter().any(|uri| !baseline.contains(uri)))
}

fn retained_startup_reserve_ready(
    duration: Duration,
    count: usize,
    remaining: Duration,
    completed: bool,
) -> bool {
    (duration >= Duration::from_secs(8) && count >= 3)
        || (completed
            && !remaining.is_zero()
            && remaining <= Duration::from_secs(8)
            && count > 0
            && duration >= remaining)
}

/// Only complete local segments, including their initialization file, count.
async fn completed_segment_reserve(
    working_dir: &Path,
    excluded: &HashSet<String>,
) -> io::Result<(Duration, usize)> {
    let playlist = tokio::fs::read_to_string(working_dir.join("live.m3u8")).await?;
    let mut duration = Duration::ZERO;
    let mut count = 0;
    for segment in parsed_playlist_segments(&playlist)
        .into_iter()
        .filter(|segment| !excluded.contains(&segment.uri))
    {
        let Some(segment_duration) = parse_extinf_duration(&segment.extinf_line)
            .and_then(|duration| duration.to_std().ok())
            .filter(|duration| !duration.is_zero())
        else {
            break;
        };
        let mut files = vec![segment.uri.as_str()];
        if let Some(init) = segment.map_uri.as_deref() {
            files.push(init);
        }
        for file in files {
            if !matches!(
                Path::new(file).components().collect::<Vec<_>>().as_slice(),
                [std::path::Component::Normal(_)]
            ) || !tokio::fs::metadata(working_dir.join(file))
                .await
                .is_ok_and(|metadata| metadata.is_file() && metadata.len() > 0)
            {
                return Ok((duration, count));
            }
        }
        duration = duration.saturating_add(segment_duration);
        count += 1;
    }
    Ok((duration, count))
}

async fn playlist_duration_excluding_uris(
    working_dir: &Path,
    excluded_uris: &HashSet<String>,
) -> io::Result<Duration> {
    let playlist = match tokio::fs::read_to_string(working_dir.join("live.m3u8")).await {
        Ok(playlist) => playlist,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Duration::ZERO),
        Err(error) => return Err(error),
    };
    let parsed = parse_playlist(&playlist, Utc::now(), false).ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidData, "invalid completed HLS playlist")
    })?;
    parsed
        .items
        .into_iter()
        .filter_map(|item| match item {
            PlaylistItem::Segment(segment) => Some(segment),
            PlaylistItem::Discontinuity => None,
        })
        .filter(|segment| !excluded_uris.contains(&segment.uri))
        .try_fold(Duration::ZERO, |total, segment| {
            let duration = parse_extinf_duration(&segment.extinf_line)
                .and_then(|duration| duration.to_std().ok())
                .filter(|duration| !duration.is_zero())
                .ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "invalid completed HLS duration")
                })?;
            total.checked_add(duration).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "completed HLS duration overflow",
                )
            })
        })
}

async fn hls_delivery_segments(working_dir: &Path) -> io::Result<Vec<HlsDeliverySegment>> {
    let playlist = match tokio::fs::read_to_string(working_dir.join("live.m3u8")).await {
        Ok(playlist) => playlist,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut segments = parsed_playlist_segments(&playlist)
        .into_iter()
        .filter_map(|segment| {
            let duration = parse_extinf_duration(&segment.extinf_line)?.to_std().ok()?;
            Some(HlsDeliverySegment {
                sequence: segment.sequence,
                duration,
            })
        })
        .collect::<Vec<_>>();
    segments.sort_unstable_by_key(|segment| segment.sequence);
    Ok(segments)
}

fn parsed_playlist_segments(playlist: &str) -> Vec<PlaylistSegment> {
    parsed_playlist_segments_from(playlist, Utc::now())
}

fn parsed_playlist_segments_from(
    playlist: &str,
    playlist_start: DateTime<Utc>,
) -> Vec<PlaylistSegment> {
    parse_playlist(playlist, playlist_start, false)
        .map(|parsed| {
            parsed
                .items
                .into_iter()
                .filter_map(|item| match item {
                    PlaylistItem::Segment(segment) => Some(segment),
                    PlaylistItem::Discontinuity => None,
                })
                .collect()
        })
        .unwrap_or_default()
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
        let earliest = self
            .earliest_segments_by_hash
            .entry(hash)
            .or_insert_with(|| file_name.clone());
        let candidate_generation = generated_at_from_init_file(&file_name);
        let earliest_generation = generated_at_from_init_file(earliest);
        let should_replace = match (candidate_generation, earliest_generation) {
            (Some(candidate), Some(current)) => candidate < current,
            _ => file_name < *earliest,
        };
        if should_replace {
            *earliest = file_name;
        }
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

async fn select_channel_custom_streams(
    pool: &DbPool,
    media_version_id: i64,
    media_item_id: i64,
    channel: &Channel,
    content_start_time: DateTime<Utc>,
    streaming_mode: StreamingMode,
) -> RepositoryResult<Option<CustomStreamSelection>> {
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

#[allow(clippy::too_many_lines)]
async fn load_active_hls_item(
    pool: &DbPool,
    channel_id: i64,
    path_replacements: &[PathReplacement],
    now: DateTime<Utc>,
) -> RepositoryResult<Option<ActiveHlsItem>> {
    load_active_hls_item_with_artifact_root(
        pool,
        channel_id,
        path_replacements,
        now,
        &crate::file_system_layout::addon_processed_media_folder(),
    )
    .await
}

#[allow(clippy::too_many_lines)]
async fn load_active_hls_item_with_artifact_root(
    pool: &DbPool,
    channel_id: i64,
    path_replacements: &[PathReplacement],
    now: DateTime<Utc>,
    artifact_root: &Path,
) -> RepositoryResult<Option<ActiveHlsItem>> {
    let Some(playout_context) = load_channel_playout_context(pool, channel_id).await? else {
        return Ok(None);
    };
    let now_text = encode_datetime(playout_context.source_time(now)?);
    let row = sqlx::query(
        r#"SELECT mf."Path" AS "MediaPath",
                  mf."Id" AS "MediaFileId",
                  pi."Id" AS "PlayoutItemId",
                  pi."MediaItemId" AS "MediaItemId",
                  mv."Id" AS "MediaVersionId",
                  mv."Duration" AS "ContentDuration",
                  pi."InPoint" AS "InPoint",
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
                  c."Name" AS "ChannelName",
                  c."WatermarkId" AS "ChannelWatermarkId",
                  c."StreamSelectorMode" AS "ChannelStreamSelectorMode",
                  c."StreamSelector" AS "ChannelStreamSelector",
                  c."PreferredAudioLanguageCode" AS "ChannelPreferredAudioLanguageCode",
                  c."PreferredAudioTitle" AS "ChannelPreferredAudioTitle",
                  c."PreferredSubtitleLanguageCode" AS "ChannelPreferredSubtitleLanguageCode",
                  c."SubtitleMode" AS "ChannelSubtitleMode",
                  c."MusicVideoCreditsMode" AS "ChannelMusicVideoCreditsMode",
                  c."MusicVideoCreditsTemplate" AS "ChannelMusicVideoCreditsTemplate",
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
           JOIN "Channel" c ON c."Id" = ?
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
    .bind(channel_id)
    .bind(playout_context.source_channel_id)
    .bind(&now_text)
    .bind(&now_text)
    .fetch_optional(pool)
    .await
    .repository()?;
    let Some(row) = row else {
        return Ok(None);
    };

    let media_path = decode_hls_column::<String>(&row, "MediaPath")?;
    let remote_url = decode_hls_column::<Option<String>>(&row, "RemoteUrl")?;
    let remote_addon = decode_hls_column::<Option<String>>(&row, "RemoteAddonId")?;
    let remote_legacy_script = decode_hls_column::<Option<String>>(&row, "RemoteLegacyScript")?;
    if remote_legacy_script.is_some() {
        return Err(RepositoryError::Decode {
            kind: "active HLS remote stream",
            value: "legacy script source requires migration to an add-on".to_owned(),
        });
    }
    let remote_stream = if remote_url.is_some() || remote_addon.is_some() {
        let legacy_is_live =
            decode_hls_column::<Option<i64>>(&row, "RemoteIsLive")?.is_some_and(|value| value != 0);
        let stored_liveness = decode_persisted_liveness(
            decode_hls_column::<Option<i64>>(&row, "RemoteLiveness")?.unwrap_or_default(),
            legacy_is_live,
        )
        .map_err(|error| RepositoryError::Decode {
            kind: "active HLS remote stream",
            value: error.to_string(),
        })?;
        let definition = RemoteStreamDefinition {
            url: remote_url,
            addon: remote_addon,
            duration: decode_hls_column(&row, "RemoteDuration")?,
            fallback_query: decode_hls_column(&row, "RemoteFallbackQuery")?,
            liveness: Some(stored_liveness),
            is_live: legacy_is_live,
            title: None,
            plot: None,
            year: None,
            content_rating: None,
            tags: Vec::new(),
            ..RemoteStreamDefinition::default()
        };
        let runtime = AddonRuntimeContext {
            ffmpeg_path: Some(ffmpeg_path()),
            ffprobe_path: Some(ffprobe_path()),
        };
        let published = crate::addon_media_list_playback::resolve_published_remote_stream(
            pool,
            decode_hls_column(&row, "RemoteStreamId")?,
            &runtime,
            artifact_root,
        )
        .await?;
        Some(match published {
            Some(stream) => stream,
            None => resolve_remote_stream_with_addons(pool, &definition, &runtime)
                .await
                .map_err(|error| RepositoryError::Decode {
                    kind: "active HLS remote stream",
                    value: error.to_string(),
                })?,
        })
    } else {
        None
    };
    let resolved = if remote_stream.is_some() || path_replacements.is_empty() {
        media_path.clone()
    } else {
        replace_path(path_replacements, &media_path, PathPlatform::Unix)
    };
    let playout_start_text = decode_hls_column::<String>(&row, "PlayoutStart")?;
    let source_playout_start =
        parse_sqlite_utc_timestamp(&playout_start_text).ok_or_else(|| RepositoryError::Decode {
            kind: "active HLS item",
            value: format!("PlayoutStart: invalid timestamp {playout_start_text:?}"),
        })?;
    let playout_finish_text = decode_hls_column::<String>(&row, "PlayoutFinish")?;
    let source_playout_finish =
        parse_sqlite_utc_timestamp(&playout_finish_text).ok_or_else(|| {
            RepositoryError::Decode {
                kind: "active HLS item",
                value: format!("PlayoutFinish: invalid timestamp {playout_finish_text:?}"),
            }
        })?;
    let playout_start = playout_context.target_time(source_playout_start)?;
    let playout_finish = playout_context.target_time(source_playout_finish)?;
    let program_duration =
        (playout_finish - playout_start)
            .to_std()
            .map_err(|error| RepositoryError::Decode {
                kind: "active HLS item",
                value: format!("PlayoutFinish precedes PlayoutStart: {error}"),
            })?;
    let content_duration_text = decode_hls_column::<String>(&row, "ContentDuration")?;
    let content_duration = try_decode_duration_text(&content_duration_text)?
        .to_std()
        .map_err(|error| RepositoryError::Decode {
            kind: "active HLS item",
            value: format!("ContentDuration: {error}"),
        })?;
    let in_point_text = decode_hls_column::<String>(&row, "InPoint")?;
    let in_point = try_decode_duration_text(&in_point_text)?
        .to_std()
        .map_err(|error| RepositoryError::Decode {
            kind: "active HLS item",
            value: format!("InPoint: {error}"),
        })?;
    let subtitle_mode_value = decode_hls_column::<Option<i64>>(&row, "SubtitleMode")?;
    let subtitle_mode = subtitle_mode_value
        .map(|value| {
            decode_subtitle_mode(value).ok_or_else(|| RepositoryError::Decode {
                kind: "active HLS item",
                value: format!("SubtitleMode: unknown value {value}"),
            })
        })
        .transpose()?;
    let stream_selector_mode_value = decode_hls_column::<i64>(&row, "ChannelStreamSelectorMode")?;
    let channel_stream_selector_mode = decode_stream_selector_mode(stream_selector_mode_value)
        .ok_or_else(|| RepositoryError::Decode {
            kind: "active HLS item",
            value: format!("ChannelStreamSelectorMode: unknown value {stream_selector_mode_value}"),
        })?;
    let channel_subtitle_mode_value = decode_hls_column::<i64>(&row, "ChannelSubtitleMode")?;
    let channel_subtitle_mode =
        decode_subtitle_mode(channel_subtitle_mode_value).ok_or_else(|| {
            RepositoryError::Decode {
                kind: "active HLS item",
                value: format!("ChannelSubtitleMode: unknown value {channel_subtitle_mode_value}"),
            }
        })?;
    let music_video_credits_mode_value =
        decode_hls_column::<i64>(&row, "ChannelMusicVideoCreditsMode")?;
    let channel_music_video_credits_mode =
        decode_music_video_credits_mode(music_video_credits_mode_value).ok_or_else(|| {
            RepositoryError::Decode {
                kind: "active HLS item",
                value: format!(
                    "ChannelMusicVideoCreditsMode: unknown value {music_video_credits_mode_value}"
                ),
            }
        })?;

    Ok(Some(ActiveHlsItem {
        original_media_path: media_path,
        media_path: resolved,
        playout_item_id: decode_hls_column(&row, "PlayoutItemId")?,
        media_item_id: decode_hls_column(&row, "MediaItemId")?,
        media_version_id: decode_hls_column(&row, "MediaVersionId")?,
        media_file_id: Some(decode_hls_column(&row, "MediaFileId")?),
        content_duration: Some(content_duration),
        in_point,
        program_duration: Some(program_duration),
        playout_start,
        playout_finish: Some(playout_finish),
        disable_watermarks: decode_hls_bool(&row, "DisableWatermarks")?,
        preferred_audio_language_code: decode_hls_column(&row, "PreferredAudioLanguageCode")?,
        preferred_audio_title: decode_hls_column(&row, "PreferredAudioTitle")?,
        preferred_subtitle_language_code: decode_hls_column(&row, "PreferredSubtitleLanguageCode")?,
        subtitle_mode,
        has_pinned_stream_selection: decode_hls_column::<Option<i64>>(
            &row,
            "PinnedMediaVersionId",
        )?
        .is_some(),
        video_stream_index: decode_hls_column(&row, "VideoStreamIndex")?,
        audio_stream_index: decode_hls_column(&row, "AudioStreamIndex")?,
        subtitle_stream_index: decode_hls_column(&row, "SubtitleStreamIndex")?,
        channel_name: decode_hls_column::<Option<String>>(&row, "ChannelName")?.unwrap_or_default(),
        channel_preferred_audio_language_code: decode_hls_column(
            &row,
            "ChannelPreferredAudioLanguageCode",
        )?,
        channel_preferred_audio_title: decode_hls_column(&row, "ChannelPreferredAudioTitle")?,
        channel_stream_selector_mode,
        channel_stream_selector: decode_hls_column(&row, "ChannelStreamSelector")?,
        channel_preferred_subtitle_language_code: decode_hls_column(
            &row,
            "ChannelPreferredSubtitleLanguageCode",
        )?,
        channel_subtitle_mode,
        channel_watermark_id: decode_hls_column(&row, "ChannelWatermarkId")?,
        channel_music_video_credits_mode,
        channel_music_video_credits_template: decode_hls_column(
            &row,
            "ChannelMusicVideoCreditsTemplate",
        )?,
        remote_stream,
        external: None,
    }))
}

async fn current_remote_stream_is_live(
    pool: &DbPool,
    channel_id: i64,
    transcode_at: DateTime<Utc>,
) -> RepositoryResult<bool> {
    let Some(playout_context) = load_channel_playout_context(pool, channel_id).await? else {
        return Ok(false);
    };
    let transcode_at_text = encode_datetime(playout_context.source_time(transcode_at)?);
    let is_live = sqlx::query_scalar::<_, i64>(
        r#"SELECT CASE
                       WHEN rs."Liveness" = 2 THEN 1
                       ELSE 0
                   END
           FROM "PlayoutItem" pi
           JOIN "Playout" p ON p."Id" = pi."PlayoutId"
           JOIN "RemoteStream" rs ON rs."Id" = pi."MediaItemId"
           WHERE p."ChannelId" = ?
             AND pi."Start" <= ?
             AND pi."Finish" > ?
           ORDER BY pi."Start" DESC, pi."Id" DESC
           LIMIT 1"#,
    )
    .bind(playout_context.source_channel_id)
    .bind(&transcode_at_text)
    .bind(&transcode_at_text)
    .fetch_optional(pool)
    .await
    .repository()?;
    Ok(is_live.is_some_and(|value| value != 0))
}

fn decode_hls_column<T>(row: &crate::db::DbRow, column: &'static str) -> RepositoryResult<T>
where
    for<'row> T: sqlx::Decode<'row, crate::db::Db> + sqlx::Type<crate::db::Db>,
{
    row.try_get(column)
        .map_err(|error| RepositoryError::Decode {
            kind: "active HLS item",
            value: format!("{column}: {error}"),
        })
}

fn decode_hls_bool(row: &crate::db::DbRow, column: &'static str) -> RepositoryResult<bool> {
    let value = decode_hls_column::<i64>(row, column)?;
    match value {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(RepositoryError::Decode {
            kind: "active HLS item",
            value: format!("{column}: expected 0 or 1, found {value}"),
        }),
    }
}

#[derive(Clone)]
struct ActiveHlsItem {
    original_media_path: String,
    media_path: String,
    playout_item_id: i64,
    media_item_id: i64,
    media_version_id: i64,
    media_file_id: Option<i64>,
    content_duration: Option<Duration>,
    in_point: Duration,
    program_duration: Option<Duration>,
    playout_start: DateTime<Utc>,
    playout_finish: Option<DateTime<Utc>>,
    disable_watermarks: bool,
    preferred_audio_language_code: Option<String>,
    preferred_audio_title: Option<String>,
    preferred_subtitle_language_code: Option<String>,
    subtitle_mode: Option<ChannelSubtitleMode>,
    has_pinned_stream_selection: bool,
    video_stream_index: Option<i32>,
    audio_stream_index: Option<i32>,
    subtitle_stream_index: Option<i32>,
    channel_name: String,
    channel_preferred_audio_language_code: Option<String>,
    channel_preferred_audio_title: Option<String>,
    channel_stream_selector_mode: ChannelStreamSelectorMode,
    channel_stream_selector: Option<String>,
    channel_preferred_subtitle_language_code: Option<String>,
    channel_subtitle_mode: ChannelSubtitleMode,
    channel_watermark_id: Option<i64>,
    channel_music_video_credits_mode: ChannelMusicVideoCreditsMode,
    channel_music_video_credits_template: Option<String>,
    remote_stream: Option<ResolvedRemoteStream>,
    external: Option<Arc<crate::external_json::playback::ResolvedExternalMedia>>,
}

impl ActiveHlsItem {
    fn channel_for_runtime_streams(&self, id: i64, number: String) -> Channel {
        Channel {
            id,
            number,
            name: self.channel_name.clone(),
            stream_selector_mode: self.channel_stream_selector_mode,
            stream_selector: self.channel_stream_selector.clone(),
            preferred_audio_language_code: self.channel_preferred_audio_language_code.clone(),
            preferred_audio_title: self.channel_preferred_audio_title.clone(),
            preferred_subtitle_language_code: self.channel_preferred_subtitle_language_code.clone(),
            subtitle_mode: self.channel_subtitle_mode,
            music_video_credits_mode: self.channel_music_video_credits_mode,
            music_video_credits_template: self.channel_music_video_credits_template.clone(),
            ..Channel::default()
        }
    }

    fn content_seek_position(&self, elapsed: Duration) -> Duration {
        self.in_point.saturating_add(elapsed)
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

const fn decode_stream_selector_mode(value: i64) -> Option<ChannelStreamSelectorMode> {
    match value {
        0 => Some(ChannelStreamSelectorMode::Default),
        1 => Some(ChannelStreamSelectorMode::Custom),
        100 => Some(ChannelStreamSelectorMode::Troubleshooting),
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

fn playout_start_duration(now: DateTime<Utc>, playout_start: DateTime<Utc>) -> Duration {
    (now - playout_start).to_std().unwrap_or(Duration::ZERO)
}

fn duration_as_i64_seconds(duration: Duration) -> i64 {
    i64::try_from(duration.as_secs()).unwrap_or(i64::MAX)
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
    start_time: DateTime<Utc>,
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
    playlist_start: DateTime<Utc>,
}

#[derive(Debug, Default)]
struct TsContinuityState {
    next_counters: HashMap<u16, u8>,
    pcr_offsets: HashMap<u16, i128>,
    first_pcr: HashMap<u16, i128>,
    last_pcr: HashMap<u16, i128>,
    normalized_sequences: HashSet<u64>,
}

impl TsContinuityState {
    fn pcr_output_ts_offset(&self) -> Option<Duration> {
        let ticks = self
            .last_pcr
            .iter()
            .filter_map(|(pid, last)| {
                let first = self.first_pcr.get(pid)?;
                Some(last.saturating_sub(*first))
            })
            .max()?;
        if ticks <= 0 {
            return None;
        }

        let nanos = ticks.saturating_mul(1_000_000_000) / PCR_TICKS_PER_SECOND;
        u64::try_from(nanos).ok().map(Duration::from_nanos)
    }
}

fn trim_playlist_contents(
    output_format: OutputFormat,
    playlist: &str,
    playlist_start: DateTime<Utc>,
    filter_before: DateTime<Utc>,
    maybe_max_segments: Option<usize>,
    end_with_discontinuity: bool,
    init_cache: Option<&HlsInitSegmentCache>,
) -> Option<TrimmedPlaylist> {
    let parsed = parse_playlist(playlist, playlist_start, end_with_discontinuity)?;
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

    let recent: Vec<PlaylistSegment> = all_segments
        .iter()
        .filter(|segment| segment_is_after_filter(segment, filter_before))
        .cloned()
        .collect();
    if recent.is_empty() {
        all_segments.to_vec()
    } else {
        recent
    }
}

fn segment_is_after_filter(segment: &PlaylistSegment, filter_before: DateTime<Utc>) -> bool {
    segment.start_time >= filter_before
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
    if output_format != OutputFormat::Hls && leading_discontinuities > 1 {
        let _ = writeln!(
            out,
            "#EXT-X-DISCONTINUITY-SEQUENCE:{leading_discontinuities}"
        );
    }
    out.push_str("#EXT-X-INDEPENDENT-SEGMENTS\n");
    if output_format != OutputFormat::Hls && leading_discontinuities == 1 {
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
                if output_format == OutputFormat::Hls {
                    continue;
                }
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
                let _ = writeln!(
                    out,
                    "#EXT-X-PROGRAM-DATE-TIME:{}",
                    format_program_date_time(segment.start_time)
                );
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
        playlist_start: first_segment.start_time,
    })
}

fn format_program_date_time(start_time: DateTime<Utc>) -> String {
    start_time.format("%Y-%m-%dT%H:%M:%S%.3f%z").to_string()
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

fn parse_playlist(
    playlist: &str,
    playlist_start: DateTime<Utc>,
    end_with_discontinuity: bool,
) -> Option<ParsedPlaylist> {
    let lines: Vec<&str> = playlist.lines().collect();
    let mut version = "7".to_string();
    let mut target_duration = None;
    let mut current_map_uri = None::<String>;
    let mut items = Vec::new();
    let mut index = 0;
    let mut current_time = playlist_start;

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
        let duration = parse_extinf_duration(line)?;
        index += 1;

        while index < lines.len() {
            let next_line = lines[index].trim();
            if next_line.is_empty() {
                index += 1;
                continue;
            }

            if let Some(value) = next_line.strip_prefix("#EXT-X-PROGRAM-DATE-TIME:") {
                let _ = value;
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
                start_time: current_time,
                uri,
                map_uri: current_map_uri.clone(),
                generated_at,
                sequence,
            }));
            current_time += duration;
            index += 1;
            break;
        }
    }

    if end_with_discontinuity
        && !items.is_empty()
        && !matches!(items.last(), Some(PlaylistItem::Discontinuity))
    {
        items.push(PlaylistItem::Discontinuity);
    }

    Some(ParsedPlaylist {
        version,
        target_duration: target_duration?,
        items,
    })
}

fn parse_extinf_duration(line: &str) -> Option<chrono::Duration> {
    let seconds = line
        .trim_end_matches(',')
        .split_once(':')?
        .1
        .parse::<f64>()
        .ok()?;
    Duration::try_from_secs_f64(seconds)
        .ok()
        .and_then(|duration| chrono::Duration::from_std(duration).ok())
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

async fn normalize_hls_ts_continuity(
    working_dir: &Path,
    trim_result: &TrimmedPlaylist,
    state: &mut TsContinuityState,
) -> io::Result<()> {
    let mut segments: Vec<(u64, String)> = trim_result
        .referenced_files
        .iter()
        .filter(|file| {
            Path::new(file)
                .extension()
                .and_then(|extension| extension.to_str())
                == Some("ts")
        })
        .filter_map(|file| parse_segment_sequence(file).map(|sequence| (sequence, file.clone())))
        .collect();
    segments.sort_by_key(|(sequence, _)| *sequence);

    for (sequence, file_name) in segments {
        if state.normalized_sequences.contains(&sequence) {
            continue;
        }

        let path = working_dir.join(&file_name);
        match tokio::fs::try_exists(&path).await {
            Ok(true) => {}
            Ok(false) => continue,
            Err(error) => return Err(error),
        }
        normalize_ts_file_continuity(&path, state).await?;
        state.normalized_sequences.insert(sequence);
    }

    Ok(())
}

async fn normalize_ts_file_continuity(
    path: &Path,
    state: &mut TsContinuityState,
) -> io::Result<()> {
    let mut body = tokio::fs::read(path).await?;
    if body.len() % TS_PACKET_SIZE != 0 {
        return Ok(());
    }

    let mut changed = false;
    for packet in body.chunks_exact_mut(TS_PACKET_SIZE) {
        if packet[0] != TS_SYNC_BYTE {
            return Ok(());
        }

        let pid = (u16::from(packet[1] & 0x1f) << 8) | u16::from(packet[2]);
        if pid == TS_NULL_PID {
            continue;
        }

        let adaptation_field_control = (packet[3] >> 4) & 0x03;
        let has_adaptation = matches!(adaptation_field_control, 0x02 | 0x03);
        let has_payload = matches!(adaptation_field_control, 0x01 | 0x03);
        let original_counter = packet[3] & 0x0f;
        let counter = match (state.next_counters.get(&pid).copied(), has_payload) {
            (Some(next_counter), true) => next_counter,
            (Some(next_counter), false) => next_counter.wrapping_sub(1) & 0x0f,
            (None, _) => original_counter,
        };
        if counter != original_counter {
            packet[3] = (packet[3] & 0xf0) | counter;
            changed = true;
        }

        state.next_counters.insert(pid, (counter + 1) & 0x0f);

        if let Some(pcr) = packet_pcr(packet, has_adaptation) {
            let normalized = normalize_pcr(pid, pcr, state);
            if normalized != pcr {
                write_packet_pcr(packet, normalized);
                changed = true;
            }
        }
    }

    if changed {
        tokio::fs::write(path, body).await?;
    }

    Ok(())
}

fn packet_pcr(packet: &[u8], has_adaptation: bool) -> Option<u64> {
    if !has_adaptation {
        return None;
    }

    let adaptation_len = usize::from(*packet.get(4)?);
    if adaptation_len < 7 || 5 + adaptation_len > packet.len() {
        return None;
    }
    if packet[5] & 0x10 == 0 {
        return None;
    }

    let base = (u64::from(packet[6]) << 25)
        | (u64::from(packet[7]) << 17)
        | (u64::from(packet[8]) << 9)
        | (u64::from(packet[9]) << 1)
        | (u64::from(packet[10]) >> 7);
    let extension = (u64::from(packet[10] & 0x01) << 8) | u64::from(packet[11]);
    Some(base * 300 + extension)
}

fn write_packet_pcr(packet: &mut [u8], pcr: u64) {
    let pcr = pcr % u64::try_from(PCR_MODULUS_TICKS).expect("PCR modulus fits u64");
    let base = pcr / 300;
    let extension = pcr % 300;

    packet[6] = low_byte(base >> 25);
    packet[7] = low_byte(base >> 17);
    packet[8] = low_byte(base >> 9);
    packet[9] = low_byte(base >> 1);
    packet[10] = (low_byte(base & 0x01) << 7) | 0x7e | low_byte(extension >> 8);
    packet[11] = low_byte(extension);
}

fn low_byte(value: u64) -> u8 {
    u8::try_from(value & 0xff).expect("masked value fits in u8")
}

fn normalize_pcr(pid: u16, pcr: u64, state: &mut TsContinuityState) -> u64 {
    let offset = state.pcr_offsets.entry(pid).or_default();
    let mut adjusted = i128::from(pcr) + *offset;
    if let Some(last) = state.last_pcr.get(&pid).copied() {
        if adjusted <= last {
            let delta = last + 1 - adjusted;
            *offset += delta;
            adjusted += delta;
        } else if adjusted - last > PCR_LEAP_TOLERANCE_TICKS {
            let delta = adjusted - (last + 1);
            *offset -= delta;
            adjusted -= delta;
        }
    }

    state.first_pcr.entry(pid).or_insert(adjusted);
    state.last_pcr.insert(pid, adjusted);
    u64::try_from(adjusted.rem_euclid(PCR_MODULUS_TICKS)).expect("PCR remainder fits in u64")
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
    use std::process::Stdio;

    use super::*;
    use crate::transcode_capacity::ActiveTranscodeReclaimReason;
    use ersatzrs_domain::{
        FFmpegProfile, FFmpegProfileAudioFormat, FFmpegProfileVideoFormat,
        HardwareAccelerationKind, Resolution, WatermarkLocation, WatermarkSize,
    };
    use ersatzrs_ffmpeg::{PixelFormat, WatermarkOverlay};
    use tokio::time::timeout;
    use uuid::Uuid;

    #[test]
    fn source_caption_plan_tracks_final_hls_input_and_fresh_retry() {
        let mut profile = playback_profile();
        profile.profile.hardware_acceleration = HardwareAccelerationKind::None;
        let build = |page: Option<u16>, start| {
            let (acceleration, mut request) = build_hls_request_with_input_explicit_discontinuity(
                InputFile::video("synthetic-source.ts"),
                OutputFormat::Hls,
                true,
                false,
                true,
                start,
                Some(Duration::from_secs(12)),
                None,
                &profile,
                None,
                OutputMetadata::default(),
                FfmpegCapabilityReport::default(),
                Some(SourceVideoStream::new(SourceVideoCodec::H264).with_index(7)),
                vec![
                    SourceAudioStream::new(SourceAudioCodec::Aac).with_index(1),
                    SourceAudioStream::new(SourceAudioCodec::Aac).with_index(5),
                ],
                PlaybackAudioSelection::Exact(5),
                page.map(|page| {
                    InputFile::teletext_subtitle(
                        "synthetic-source.ts",
                        9,
                        ersatzrs_domain::TeletextPageNumber::new(page).unwrap(),
                    )
                }),
                Vec::new(),
                Vec::new(),
                HlsDiscontinuity::Absent,
                None,
                HlsInputTimeline::Original,
                None,
            );
            apply_hls_bootstrap_readburst(&mut request, true, false, 3);
            crate::playback::source_teletext::runtime::SourceTeletextConsumerRequest::new(
                acceleration,
                request,
            )
            .build()
            .unwrap()
        };
        let (primary, plan) = build(Some(888), Duration::from_secs(40));
        let plan = plan.expect("actual HLS source caption has its generated plan");
        assert!(plan.matches_command(&primary));
        assert!(!plan.timing().has_opaque_input_options());
        assert_eq!(
            plan.timing().effective_input_seek_microseconds(),
            Some(40_000_000)
        );
        assert!(plan
            .physical_input_options()
            .windows(2)
            .any(|pair| pair == ["-readrate_initial_burst", "12"]));
        assert_eq!(plan.caption_physical_input_index(), 0);
        assert_eq!(plan.video_stream(), Some((0, 7)));
        assert_eq!(plan.audio_stream(), Some((0, 5)));
        assert_eq!(plan.enabled_caption_input_streams(), [7, 5, 9]);
        assert_eq!(plan.caption_input().path, "synthetic-source.ts");
        assert_eq!(
            plan.caption_input().streams[0].source_teletext_page,
            ersatzrs_domain::TeletextPageNumber::new(888).ok()
        );
        assert_eq!(
            primary.arguments.iter().filter(|arg| *arg == "-i").count(),
            1
        );
        assert!(plan
            .physical_input_options()
            .windows(2)
            .any(|pair| pair == ["-readrate", "1.05"]));

        // A rebuilt command must carry its new seal, even for the same page.
        let (retry, retry_plan) = build(Some(888), Duration::from_secs(41));
        let retry_plan = retry_plan.unwrap();
        assert!(retry_plan.matches_command(&retry));
        assert!(!retry_plan.timing().has_opaque_input_options());
        assert_eq!(
            retry_plan.timing().effective_input_seek_microseconds(),
            Some(41_000_000)
        );
        assert!(!plan.matches_command(&retry));
        assert!(!retry_plan.matches_command(&primary));
        let (ordinary, ordinary_plan) = build(None, Duration::from_secs(40));
        assert!(ordinary_plan.is_none());
        assert!(!plan.matches_command(&ordinary));
        assert!(!ordinary.arguments.iter().any(|arg| arg == "-txt_page:9"));
        assert!(ordinary
            .arguments
            .windows(2)
            .any(|pair| pair == ["-readrate", "1.05"]));
    }

    #[test]
    fn retained_hls_default_audio_language_matches_actual_selected_track() {
        let mut japanese = SourceAudioStream::new(SourceAudioCodec::Aac).with_index(0);
        japanese.language = Some("jpn".into());
        let mut english = SourceAudioStream::new(SourceAudioCodec::Aac).with_index(3);
        english.language = Some("eng".into());
        english.default = true;
        let streams = [japanese, english];
        let selection = retained_hls_audio_selection(PlaybackAudioSelection::Automatic, &streams);
        assert_eq!(selection, PlaybackAudioSelection::Exact(3));
        let metadata = playback_output_metadata(None, None, None, &streams, None, selection);
        assert_eq!(metadata.audio_language.as_deref(), Some("eng"));
        let disabled = retained_hls_audio_selection(PlaybackAudioSelection::Disabled, &streams);
        assert_eq!(disabled, PlaybackAudioSelection::Disabled);
        assert!(
            playback_output_metadata(None, None, None, &streams, None, disabled)
                .audio_language
                .is_none()
        );
        assert_eq!(
            retained_hls_audio_selection(PlaybackAudioSelection::Exact(0), &streams),
            PlaybackAudioSelection::Exact(0)
        );
    }

    #[test]
    fn retained_hls_automatic_null_audio_classifies_only_fresh_complete_metadata() {
        let absent = PlaybackAudioSelection::Automatic.with_complete_source_audio(&[]);
        assert_eq!(
            retained_hls_audio_selection(absent, &[]),
            PlaybackAudioSelection::AutomaticSilence
        );
        let mut dutch = SourceAudioStream::new(SourceAudioCodec::Ac3).with_index(3);
        dutch.default = true;
        dutch.language = Some("nld".into());
        let fresh = [dutch];
        let selection = absent.with_complete_source_audio(&fresh);
        assert_eq!(
            retained_hls_audio_selection(selection, &fresh),
            PlaybackAudioSelection::Exact(3)
        );
        assert_eq!(
            retained_hls_audio_selection(PlaybackAudioSelection::Automatic, &[]),
            PlaybackAudioSelection::Automatic,
            "empty unclassified metadata is still unknown"
        );
        assert!(
            playback_output_metadata(None, None, Some("nld"), &[], None, absent)
                .audio_language
                .is_none()
        );
    }

    #[test]
    fn automatic_null_audio_hls_maps_independent_stream_zero_and_preserves_seek() {
        let command = build_hls_command(
            "synthetic-video-with-subtitle.mkv",
            Path::new("synthetic-hls"),
            OutputFormat::Hls,
            true,
            false,
            false,
            Duration::from_secs(3),
            Some(Duration::from_secs(5)),
            None,
            &playback_profile(),
            None,
            OutputMetadata::default(),
            FfmpegCapabilityReport::default(),
            Some(SourceVideoStream::new(SourceVideoCodec::H264).with_index(0)),
            Vec::new(),
            PlaybackAudioSelection::Automatic.with_complete_source_audio(&[]),
            None,
            Vec::new(),
            Vec::new(),
        )
        .unwrap();
        let arguments = command.arguments.join(" ");
        assert!(
            arguments.contains("[1:0]aresample=async=1,apad[a]"),
            "{arguments}"
        );
        assert!(!arguments.contains("[0:1]aresample"), "{arguments}");
        assert_eq!(
            command
                .arguments
                .iter()
                .filter(|argument| *argument == "-ss")
                .count(),
            1
        );
        assert!(arguments.contains("-t 00:00:05"), "{arguments}");
        let input_contract = command
            .arguments
            .windows(2)
            .filter(|pair| matches!(pair[0].as_str(), "-ss" | "-f" | "-i"))
            .map(|pair| pair.join(" "))
            .collect::<Vec<_>>()
            .join("\n");
        insta::assert_snapshot!(input_contract, @"
        -ss 00:00:03.0000000
        -i synthetic-video-with-subtitle.mkv
        -f lavfi
        -i anullsrc=channel_layout=stereo:sample_rate=48000
        -f hls
        ");
        insta::assert_snapshot!(arguments);
    }

    #[test]
    fn automatic_null_audio_retained_hls_preserves_fresh_video_index_and_clock() {
        let mut profile = playback_profile();
        profile.profile.video_format = FFmpegProfileVideoFormat::Copy;
        profile.profile.audio_format = FFmpegProfileAudioFormat::Copy;
        let video = InputFile::video("pipe:0")
            .with_stream_index(7)
            .with_input_options(["-f", "nut"]);
        let (command, _) = build_hls_command_with_input_explicit_discontinuity(
            video,
            OutputFormat::Hls,
            true,
            false,
            true,
            Duration::from_secs(90),
            Some(Duration::from_secs(20)),
            Some(Duration::from_mins(5)),
            &profile,
            None,
            OutputMetadata::default(),
            FfmpegCapabilityReport::default(),
            Some(SourceVideoStream::new(SourceVideoCodec::RawVideo).with_index(7)),
            Vec::new(),
            PlaybackAudioSelection::AutomaticSilence,
            None,
            Vec::new(),
            Vec::new(),
            HlsDiscontinuity::Absent,
            None,
            HlsInputTimeline::RetainedIntervals,
            None,
        )
        .unwrap();
        let arguments = command.arguments.join(" ");
        assert!(arguments.contains("0:7"), "{arguments}");
        let source_input = command
            .arguments
            .iter()
            .position(|argument| argument == "pipe:0")
            .unwrap();
        let audio_input = command
            .arguments
            .iter()
            .position(|argument| argument.starts_with("anullsrc="))
            .unwrap();
        assert!(
            command.arguments[..source_input]
                .iter()
                .any(|argument| argument == "-readrate"),
            "retained source must retain its pacing: {arguments}"
        );
        assert!(
            !command.arguments[source_input + 1..audio_input]
                .iter()
                .any(|argument| argument.starts_with("-readrate")),
            "generated silence must not acquire independent readrate: {arguments}"
        );
        assert!(
            arguments.contains("[1:0]aresample=async=1,apad[a]"),
            "{arguments}"
        );
        assert!(
            command
                .arguments
                .iter()
                .any(|argument| argument == "-copyts"),
            "{arguments}"
        );
        assert!(
            !command.arguments.iter().any(|argument| argument == "-ss"),
            "{arguments}"
        );
        assert!(
            !arguments.contains("-codec:v copy") && !arguments.contains("-codec:a copy"),
            "{arguments}"
        );
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
        -f lavfi
        -i anullsrc=channel_layout=stereo:sample_rate=48000
        -f hls
        ");
        insta::assert_snapshot!(arguments);
    }

    #[test]
    fn automatic_null_audio_hls_does_not_repair_unknown_or_explicit_selection() {
        for selection in [
            PlaybackAudioSelection::Automatic,
            PlaybackAudioSelection::Disabled,
            PlaybackAudioSelection::Exact(99),
        ] {
            let command = build_hls_command(
                "pipe:0",
                Path::new("synthetic-hls"),
                OutputFormat::Hls,
                true,
                false,
                false,
                Duration::ZERO,
                Some(Duration::from_secs(5)),
                None,
                &playback_profile(),
                None,
                OutputMetadata::default(),
                FfmpegCapabilityReport::default(),
                None,
                Vec::new(),
                selection,
                None,
                Vec::new(),
                Vec::new(),
            )
            .unwrap();
            let arguments = command.arguments.join(" ");
            assert!(
                !arguments.contains("anullsrc"),
                "{selection:?}: {arguments}"
            );
            if selection.is_disabled() {
                assert!(
                    !arguments.contains("-codec:a") && !arguments.contains("-map [a]"),
                    "{arguments}"
                );
            } else if let Some(index) = selection.stream_index() {
                assert!(
                    arguments.contains(&format!("[0:{index}]aresample")),
                    "{arguments}"
                );
            }
        }
    }

    #[test]
    fn automatic_null_audio_hls_copy_video_uses_profile_audio_encoder() {
        for (format, encoder) in [
            (FFmpegProfileAudioFormat::Copy, "aac"),
            (FFmpegProfileAudioFormat::None, "aac"),
            (FFmpegProfileAudioFormat::Ac3, "ac3"),
            (FFmpegProfileAudioFormat::AacLatm, "aac"),
        ] {
            let mut profile = playback_profile();
            profile.profile.video_format = FFmpegProfileVideoFormat::Copy;
            profile.profile.audio_format = format;
            profile.profile.audio_channels = 6;
            profile.profile.audio_sample_rate = 44;
            let command = build_hls_command(
                "synthetic-copy-video.mkv",
                Path::new("synthetic-hls"),
                OutputFormat::Hls,
                true,
                false,
                false,
                Duration::ZERO,
                Some(Duration::from_secs(5)),
                None,
                &profile,
                None,
                OutputMetadata::default(),
                FfmpegCapabilityReport::default(),
                Some(SourceVideoStream::new(SourceVideoCodec::H264)),
                Vec::new(),
                PlaybackAudioSelection::AutomaticSilence,
                None,
                Vec::new(),
                Vec::new(),
            )
            .unwrap();
            let arguments = command.arguments.join(" ");
            assert!(arguments.contains("-codec:v copy"), "{arguments}");
            assert!(
                arguments.contains(&format!("-codec:a {encoder}")),
                "{arguments}"
            );
            assert!(
                arguments.contains("-ac 6") && arguments.contains("-ar 44k"),
                "{arguments}"
            );
            assert!(
                arguments.contains("anullsrc=channel_layout=stereo:sample_rate=44000"),
                "{arguments}"
            );
            assert!(!arguments.contains("-codec:a copy"), "{arguments}");
        }
    }

    #[test]
    fn retained_hls_failure_classification_keeps_only_typed_results() {
        let source = SourceVideoStream::new(SourceVideoCodec::H264);
        let failure = classify_retained_hls_failure(
            "x-ersatzrs-range-source: synthetic-private-capability\nscale_cuda failed to configure output pad\nerror submitting packet to decoder: function not implemented",
            Some(ersatzrs_ffmpeg::HardwareGraphOperation::NvencCudaScale),
            Some(&source),
            HardwareAccelerationKind::Nvenc,
        );
        assert_eq!(
            failure.graph,
            Some(ersatzrs_ffmpeg::GraphFailureKind::HardwareMap)
        );
        assert_eq!(failure.decode.unwrap().codec, "h264");
    }

    #[test]
    fn retained_hls_worker_cannot_restore_readiness_after_startup_deadline() {
        let worker = HlsSessionWorker::new_with_options(
            "42".into(),
            1,
            HlsSessionOptions::standard("42", std::env::temp_dir().join("synthetic-retained-hls")),
            OutputFormat::Hls,
            Vec::new(),
            test_ffmpeg_runtime(FfmpegCapabilityReport::default()),
        );
        worker.initialize_startup(std::time::Instant::now(), 3);
        worker.set_startup_readiness(HlsStartupReadiness::Ready);
        assert_eq!(worker.startup_readiness(), HlsStartupReadiness::Failed);
        worker.set_startup_readiness(HlsStartupReadiness::Legacy);
        assert_eq!(worker.startup_readiness(), HlsStartupReadiness::Failed);
    }

    #[test]
    fn retained_hls_reserve_requires_duration_floor_or_successful_short_remainder() {
        let seconds = Duration::from_secs;
        assert!(!retained_startup_reserve_ready(
            seconds(6),
            3,
            seconds(30),
            false
        ));
        assert!(!retained_startup_reserve_ready(
            seconds(8),
            2,
            seconds(30),
            false
        ));
        assert!(retained_startup_reserve_ready(
            seconds(12),
            3,
            seconds(30),
            false
        ));
        assert!(!retained_startup_reserve_ready(
            seconds(3),
            1,
            seconds(3),
            false
        ));
        assert!(retained_startup_reserve_ready(
            seconds(3),
            1,
            seconds(3),
            true
        ));
        assert!(!retained_startup_reserve_ready(
            seconds(8),
            2,
            seconds(9),
            true
        ));
        assert!(!retained_startup_reserve_ready(
            seconds(0),
            0,
            seconds(0),
            true
        ));
    }

    #[tokio::test]
    async fn retained_hls_reserve_excludes_old_and_missing_segment_or_init_files() {
        let directory = tempfile::tempdir().unwrap();
        tokio::fs::write(directory.path().join("live.m3u8"), "#EXTM3U\n#EXT-X-VERSION:7\n#EXT-X-TARGETDURATION:4\n#EXT-X-MEDIA-SEQUENCE:0\n#EXT-X-MAP:URI=\"init.mp4\"\n#EXTINF:4.0,\nlive000000.m4s\n#EXTINF:4.0,\nlive000001.m4s\n").await.unwrap();
        let baseline = HashSet::from(["live000000.m4s".to_owned()]);
        tokio::fs::write(
            directory.path().join("live000001.m4s"),
            b"synthetic segment",
        )
        .await
        .unwrap();
        assert_eq!(
            completed_segment_reserve(directory.path(), &baseline)
                .await
                .unwrap(),
            (Duration::ZERO, 0)
        );
        tokio::fs::write(
            directory.path().join("init.mp4"),
            b"synthetic initialization",
        )
        .await
        .unwrap();
        assert_eq!(
            completed_segment_reserve(directory.path(), &baseline)
                .await
                .unwrap(),
            (Duration::from_secs(4), 1)
        );
        tokio::fs::remove_file(directory.path().join("live000001.m4s"))
            .await
            .unwrap();
        assert_eq!(
            completed_segment_reserve(directory.path(), &baseline)
                .await
                .unwrap(),
            (Duration::ZERO, 0)
        );
    }

    #[test]
    fn retained_hls_commands_use_fresh_indexes_and_shared_clock() {
        for selection in [
            PlaybackAudioSelection::Exact(3),
            PlaybackAudioSelection::Disabled,
        ] {
            let video = InputFile::video("pipe:0")
                .with_stream_index(7)
                .with_input_options(["-f", "nut"]);
            let mut profile = playback_profile();
            profile.profile.video_format = FFmpegProfileVideoFormat::Copy;
            profile.profile.audio_format = FFmpegProfileAudioFormat::Copy;
            let (command, _) = build_hls_command_with_input_explicit_discontinuity(
                video,
                OutputFormat::Hls,
                true,
                false,
                true,
                Duration::from_secs(90),
                Some(Duration::from_secs(20)),
                Some(Duration::from_mins(5)),
                &profile,
                Some(FrameRate::DEFAULT),
                OutputMetadata::default(),
                FfmpegCapabilityReport::default(),
                Some(SourceVideoStream::new(SourceVideoCodec::RawVideo).with_index(7)),
                vec![SourceAudioStream::new(SourceAudioCodec::Unknown).with_index(3)],
                selection,
                None,
                Vec::new(),
                Vec::new(),
                HlsDiscontinuity::Absent,
                None,
                HlsInputTimeline::RetainedIntervals,
                None,
            )
            .unwrap();
            let args = command.arguments.join(" ");
            assert!(
                command.arguments.iter().any(|arg| arg == "-copyts"),
                "{args}"
            );
            assert!(
                !command
                    .arguments
                    .iter()
                    .any(|arg| arg == "-ss" || arg == "-r"),
                "{args}"
            );
            assert!(args.contains("-fps_mode:v vfr"), "{args}");
            assert!(args.contains("0:7"), "{args}");
            assert!(
                !args.contains("concatdec_select") && !args.contains("setpts="),
                "{args}"
            );
            if selection.is_disabled() {
                assert!(!args.contains("0:3"), "{args}");
            } else {
                assert!(args.contains("0:3"), "{args}");
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
                -f hls
                ");
            }
            assert!(
                !args.contains("-c:v copy") && !args.contains("-c:a copy"),
                "{args}"
            );
        }
    }

    fn hls_counter_test_guard() -> std::sync::MutexGuard<'static, ()> {
        crate::transcode_capacity::active_transcode_test_guard_for_tests()
    }

    fn reset_active_transcode_state() {
        crate::transcode_capacity::reset_active_transcode_state_for_tests();
    }

    fn delivery_segments(first: u64, last: u64) -> Vec<HlsDeliverySegment> {
        (first..=last)
            .map(|sequence| HlsDeliverySegment {
                sequence,
                duration: Duration::from_secs(4),
            })
            .collect()
    }

    const fn software_video_scope() -> HlsDeliverySoftwareScope {
        HlsDeliverySoftwareScope {
            channel_id: 5,
            ffmpeg_profile_id: 7,
            profile_sha256: [1; 32],
        }
    }

    #[test]
    fn hls_delivery_tracker_allows_one_segment_of_sampling_jitter() {
        let mut tracker = HlsDeliveryTracker::new(None, None);

        let observation = tracker.observe(
            Duration::from_mins(1),
            &delivery_segments(1, 14),
            Utc::now(),
        );

        assert!(observation.is_none());
    }

    #[test]
    fn hls_delivery_tracker_detects_sustained_under_run_and_recovery() {
        let now = Utc::now();
        let mut tracker = HlsDeliveryTracker::new(None, None);

        let under_run = tracker
            .observe(Duration::from_mins(1), &delivery_segments(1, 7), now)
            .expect("slow media delivery should transition to under-run");
        assert_eq!(under_run.state, HlsDeliveryState::UnderRun);
        assert!((under_run.media_seconds - 28.0).abs() < f64::EPSILON);
        assert!(under_run.delivery_ratio_percent < 50.0);

        let recovered = tracker
            .observe(
                Duration::from_mins(2),
                &delivery_segments(8, 22),
                now + chrono::Duration::seconds(60),
            )
            .expect("a healthy window should transition to recovered");
        assert_eq!(recovered.state, HlsDeliveryState::Recovered);
        assert_eq!(recovered.last_under_run_at, now);
        assert!((recovered.media_seconds - 60.0).abs() < f64::EPSILON);
    }

    #[test]
    fn hls_delivery_tracker_detects_fast_severe_under_run() {
        let mut tracker = HlsDeliveryTracker::new(None, None);

        let under_run = tracker
            .observe(
                HLS_DELIVERY_FAST_WINDOW,
                &delivery_segments(1, 2),
                Utc::now(),
            )
            .expect("severe under-delivery should trigger the fast window");

        assert_eq!(under_run.state, HlsDeliveryState::UnderRun);
        assert!(under_run.transitioned);
    }

    #[test]
    fn hls_delivery_tracker_does_not_fast_fail_before_twenty_seconds() {
        let mut tracker = HlsDeliveryTracker::new(None, None);

        let observation = tracker.observe(
            Duration::from_secs(19),
            &delivery_segments(1, 2),
            Utc::now(),
        );

        assert!(observation.is_none());
    }

    #[test]
    fn hls_delivery_tracker_recovers_seeded_fallback_after_healthy_window() {
        let detected_at = Utc::now();
        let mut tracker = HlsDeliveryTracker::new(None, Some(detected_at));

        let recovered = tracker
            .observe(
                HLS_DELIVERY_WINDOW,
                &delivery_segments(1, 15),
                detected_at + chrono::Duration::minutes(1),
            )
            .expect("a verified fallback should transition to recovered");

        assert_eq!(recovered.state, HlsDeliveryState::Recovered);
        assert!(recovered.transitioned);
        assert_eq!(recovered.last_under_run_at, detected_at);
    }

    #[test]
    fn hls_delivery_recovery_allows_one_downgrade_and_never_loops_at_720p() {
        let observed_at = Utc::now();
        let under_run = HlsDeliveryObservation {
            transitioned: true,
            observed_at,
            state: HlsDeliveryState::UnderRun,
            last_under_run_at: observed_at,
            wall_seconds: 20.0,
            media_seconds: 8.0,
            delivery_ratio_percent: 40.0,
            no_progress_seconds: 0.0,
        };
        let initial = HlsDeliveryRecovery {
            media_item_id: 42,
            max_video_height: REMOTE_STREAM_DEFAULT_MAX_VIDEO_HEIGHT,
            supports_quality_fallback: true,
            software_video_scope: None,
            available_software_video_scope: None,
            fallback_detected_at: None,
            fallback_verified: false,
        };
        let fallback = HlsDeliveryRecovery {
            max_video_height: REMOTE_STREAM_FALLBACK_MAX_VIDEO_HEIGHT,
            fallback_detected_at: Some(observed_at),
            ..initial
        };

        assert_eq!(
            hls_delivery_recovery_action(initial, &under_run, false),
            HlsDeliveryRecoveryAction::RestartAtFallback
        );
        assert_eq!(
            hls_delivery_recovery_action(fallback, &under_run, false),
            HlsDeliveryRecoveryAction::ClearFallback
        );
    }

    #[test]
    fn hls_delivery_recovery_prefers_software_video_before_quality_downgrade() {
        let observed_at = Utc::now();
        let under_run = HlsDeliveryObservation {
            transitioned: true,
            observed_at,
            state: HlsDeliveryState::UnderRun,
            last_under_run_at: observed_at,
            wall_seconds: 20.0,
            media_seconds: 4.0,
            delivery_ratio_percent: 26.7,
            no_progress_seconds: 5.0,
        };
        let hardware = HlsDeliveryRecovery {
            media_item_id: 42,
            max_video_height: REMOTE_STREAM_DEFAULT_MAX_VIDEO_HEIGHT,
            supports_quality_fallback: true,
            software_video_scope: None,
            available_software_video_scope: Some(software_video_scope()),
            fallback_detected_at: None,
            fallback_verified: false,
        };
        let software = HlsDeliveryRecovery {
            software_video_scope: Some(software_video_scope()),
            fallback_detected_at: Some(observed_at),
            ..hardware
        };

        assert_eq!(
            hls_delivery_recovery_action(hardware, &under_run, false),
            HlsDeliveryRecoveryAction::RestartWithSoftware(software_video_scope())
        );
        assert_eq!(
            hls_delivery_recovery_action(software, &under_run, false),
            HlsDeliveryRecoveryAction::RestartAtFallback
        );
    }

    #[test]
    fn verified_720p_fallback_can_escalate_to_software_encoding() {
        let detected_at = Utc::now() - chrono::Duration::minutes(2);
        let recovery = HlsDeliveryRecovery {
            media_item_id: 42,
            max_video_height: REMOTE_STREAM_FALLBACK_MAX_VIDEO_HEIGHT,
            supports_quality_fallback: true,
            software_video_scope: None,
            available_software_video_scope: Some(software_video_scope()),
            fallback_detected_at: Some(detected_at),
            fallback_verified: true,
        };
        let mut tracker = HlsDeliveryTracker::new(
            None,
            pending_hls_delivery_fallback_detected_at(Some(recovery)),
        );
        let observation = tracker
            .observe(
                HLS_DELIVERY_FAST_WINDOW,
                &delivery_segments(1, 2),
                Utc::now(),
            )
            .expect("a fresh under-run should transition after loading a verified fallback");

        assert!(observation.transitioned);
        assert_eq!(
            hls_delivery_recovery_action(recovery, &observation, true),
            HlsDeliveryRecoveryAction::RestartWithSoftware(software_video_scope())
        );
    }

    #[test]
    fn hls_delivery_in_memory_software_fallback_does_not_cross_channel_scope() {
        let adaptation = HlsDeliveryAdaptation {
            detected_at: Utc::now(),
            max_video_height: REMOTE_STREAM_DEFAULT_MAX_VIDEO_HEIGHT,
            software_video_scope: Some(software_video_scope()),
        };
        let other_channel = HlsDeliverySoftwareScope {
            channel_id: 6,
            ..software_video_scope()
        };

        assert!(hls_delivery_adaptation_for_scope(adaptation, Some(other_channel), true).is_none());

        let quality_and_software = HlsDeliveryAdaptation {
            max_video_height: REMOTE_STREAM_FALLBACK_MAX_VIDEO_HEIGHT,
            ..adaptation
        };
        let normalized =
            hls_delivery_adaptation_for_scope(quality_and_software, Some(other_channel), true)
                .expect("media-scoped quality fallback should remain active");
        assert!(normalized.software_video_scope.is_none());
        assert_eq!(
            normalized.max_video_height,
            REMOTE_STREAM_FALLBACK_MAX_VIDEO_HEIGHT
        );
    }

    #[test]
    fn hls_delivery_recovery_verifies_healthy_720p_once() {
        let detected_at = Utc::now();
        let recovered = HlsDeliveryObservation {
            transitioned: true,
            observed_at: detected_at + chrono::Duration::minutes(1),
            state: HlsDeliveryState::Recovered,
            last_under_run_at: detected_at,
            wall_seconds: 60.0,
            media_seconds: 60.0,
            delivery_ratio_percent: 100.0,
            no_progress_seconds: 0.0,
        };
        let fallback = HlsDeliveryRecovery {
            media_item_id: 42,
            max_video_height: REMOTE_STREAM_FALLBACK_MAX_VIDEO_HEIGHT,
            supports_quality_fallback: true,
            software_video_scope: None,
            available_software_video_scope: None,
            fallback_detected_at: Some(detected_at),
            fallback_verified: false,
        };

        assert_eq!(
            hls_delivery_recovery_action(fallback, &recovered, false),
            HlsDeliveryRecoveryAction::VerifyFallback
        );
        assert_eq!(
            hls_delivery_recovery_action(fallback, &recovered, true),
            HlsDeliveryRecoveryAction::None
        );
    }

    #[test]
    fn hls_delivery_recovery_verifies_healthy_software_fallback_once() {
        let detected_at = Utc::now();
        let recovered = HlsDeliveryObservation {
            transitioned: true,
            observed_at: detected_at + chrono::Duration::minutes(1),
            state: HlsDeliveryState::Recovered,
            last_under_run_at: detected_at,
            wall_seconds: 60.0,
            media_seconds: 60.0,
            delivery_ratio_percent: 100.0,
            no_progress_seconds: 0.0,
        };
        let fallback = HlsDeliveryRecovery {
            media_item_id: 42,
            max_video_height: REMOTE_STREAM_DEFAULT_MAX_VIDEO_HEIGHT,
            supports_quality_fallback: true,
            software_video_scope: Some(software_video_scope()),
            available_software_video_scope: Some(software_video_scope()),
            fallback_detected_at: Some(detected_at),
            fallback_verified: false,
        };

        assert_eq!(
            hls_delivery_recovery_action(fallback, &recovered, false),
            HlsDeliveryRecoveryAction::VerifyFallback
        );
        assert_eq!(
            hls_delivery_recovery_action(fallback, &recovered, true),
            HlsDeliveryRecoveryAction::None
        );
    }

    #[test]
    fn hls_delivery_tracker_detects_startup_and_midstream_stalls() {
        let now = Utc::now();
        let mut startup = HlsDeliveryTracker::new(None, None);
        let startup_under_run = startup
            .observe(Duration::from_secs(30), &[], now)
            .expect("missing startup progress should be an under-run");
        assert_eq!(startup_under_run.state, HlsDeliveryState::UnderRun);

        let mut midstream = HlsDeliveryTracker::new(None, None);
        assert!(midstream
            .observe(Duration::from_secs(5), &delivery_segments(1, 1), now,)
            .is_none());
        let stalled = midstream
            .observe(Duration::from_secs(17), &[], now)
            .expect("twelve seconds without a segment should be an under-run");
        assert_eq!(stalled.state, HlsDeliveryState::UnderRun);
        assert!((stalled.no_progress_seconds - 12.0).abs() < f64::EPSILON);
    }

    #[test]
    fn hls_delivery_tracker_refreshes_active_under_run_without_log_transition() {
        let now = Utc::now();
        let mut tracker = HlsDeliveryTracker::new(None, None);
        let initial = tracker
            .observe(Duration::from_mins(1), &delivery_segments(1, 7), now)
            .expect("initial under-run");
        assert!(initial.transitioned);

        let heartbeat_at = now + chrono::Duration::minutes(1);
        let heartbeat = tracker
            .observe(Duration::from_mins(2), &[], heartbeat_at)
            .expect("active under-run heartbeat");
        assert_eq!(heartbeat.state, HlsDeliveryState::UnderRun);
        assert!(!heartbeat.transitioned);
        assert_eq!(heartbeat.last_under_run_at, heartbeat_at);
    }

    fn test_ffmpeg_runtime(
        capabilities: FfmpegCapabilityReport,
    ) -> Arc<RwLock<FfmpegRuntimeReport>> {
        let mut runtime = FfmpegRuntimeReport::for_tests();
        runtime.capabilities = capabilities;
        Arc::new(RwLock::new(runtime))
    }

    #[test]
    fn browser_preview_override_changes_only_codec_compatibility_fields() {
        let mut configured = playback_profile();
        configured.profile.transcode_video = false;
        configured.profile.transcode_audio = false;
        configured.profile.video_format = FFmpegProfileVideoFormat::Hevc;
        configured.profile.video_profile = Some("main10".to_owned());
        configured.profile.bit_depth = FFmpegProfileBitDepth::TenBit;
        configured.profile.audio_format = FFmpegProfileAudioFormat::Ac3;
        configured.profile.video_bitrate = 12_345;
        configured.profile.normalize_colors = true;
        let resolution = configured.resolution.clone();

        let preview = apply_playback_profile_override(
            configured,
            HlsPlaybackProfileOverride::BrowserCompatible,
        );

        assert!(preview.profile.transcode_video);
        assert!(preview.profile.transcode_audio);
        assert_eq!(preview.profile.video_format, FFmpegProfileVideoFormat::H264);
        assert_eq!(preview.profile.video_profile.as_deref(), Some("high"));
        assert_eq!(preview.profile.bit_depth, FFmpegProfileBitDepth::EightBit);
        assert_eq!(preview.profile.audio_format, FFmpegProfileAudioFormat::Aac);
        assert_eq!(preview.profile.video_bitrate, 12_345);
        assert!(preview.profile.normalize_colors);
        assert_eq!(preview.resolution, resolution);
    }

    #[test]
    fn hls_software_video_fallback_preserves_profile_except_encoder_controls() {
        let mut profile = playback_profile();
        profile.profile.transcode_video = true;
        profile.profile.hardware_acceleration = HardwareAccelerationKind::Nvenc;
        profile.profile.video_format = FFmpegProfileVideoFormat::H264;
        profile.profile.video_preset = Some("p5".to_owned());
        profile.profile.video_tune = Some("hq".to_owned());
        let expected_resolution = profile.resolution.clone();
        let expected_bitrate = profile.profile.video_bitrate;

        assert!(hls_software_video_fallback_available(&profile));
        apply_hls_software_video_fallback(&mut profile);

        assert_eq!(
            profile.profile.hardware_acceleration,
            HardwareAccelerationKind::None
        );
        assert_eq!(profile.profile.video_preset.as_deref(), Some("veryfast"));
        assert!(profile.profile.video_tune.is_none());
        assert_eq!(profile.profile.video_bitrate, expected_bitrate);
        assert_eq!(profile.resolution, expected_resolution);
    }

    #[test]
    fn hls_software_video_fallback_command_uses_libx264() {
        // ADR 0084: an observed live-delivery under-run may replace hardware
        // H.264 encoding while preserving the remainder of the HLS contract.
        let mut profile = playback_profile();
        profile.profile.transcode_video = true;
        profile.profile.hardware_acceleration = HardwareAccelerationKind::Nvenc;
        profile.profile.video_preset = Some("p5".to_owned());
        profile.profile.video_tune = Some("hq".to_owned());
        apply_hls_software_video_fallback(&mut profile);

        let command = build_hls_command(
            "/media/test.mkv",
            Path::new("/tmp/hls/software-fallback"),
            OutputFormat::Hls,
            true,
            false,
            true,
            Duration::ZERO,
            None,
            None,
            &profile,
            None,
            OutputMetadata::default(),
            FfmpegCapabilityReport::default(),
            None,
            Vec::new(),
            PlaybackAudioSelection::Automatic,
            None,
            Vec::new(),
            Vec::new(),
        )
        .expect("software fallback should produce a playable HLS command");

        insta::assert_snapshot!(command.arguments.join(" "));
    }

    fn test_active_slot(
        channel_number: &str,
        kind: ActiveTranscodeKind,
        idle_for: Duration,
        allow_watched_preemption: bool,
    ) -> ActiveTranscodeSlot {
        let idle_ms = i64::try_from(idle_for.as_millis()).unwrap_or(i64::MAX);
        let last_access_ms = Utc::now().timestamp_millis().saturating_sub(idle_ms);
        ActiveTranscodeSlot::new_hls(
            channel_number,
            kind,
            Arc::new(AtomicI64::new(last_access_ms)),
            ActiveTranscodeReclaimHandle::new(
                Arc::new(Notify::new()),
                Arc::new(AtomicU64::new(0)),
                Arc::new(AtomicI64::new(0)),
                allow_watched_preemption,
            ),
        )
    }

    #[tokio::test]
    async fn segment_output_rejects_reused_names_and_missing_programme_date_time() {
        let root = tempfile::tempdir().unwrap();
        let worker = HlsSessionWorker::new(
            "native-test".into(),
            1,
            root.path().into(),
            OutputFormat::Hls,
            Vec::new(),
            test_ffmpeg_runtime(FfmpegCapabilityReport::default()),
        );
        worker
            .output_status
            .lock()
            .unwrap()
            .begin(Some(HashSet::new()), Some("drawvg failed".into()));
        let header = "#EXTM3U\n#EXT-X-TARGETDURATION:4\n#EXT-X-MEDIA-SEQUENCE:0\n#EXTINF:4.004,\n";
        let time = DateTime::from_timestamp_millis(1_634_389_200_142).unwrap();
        *worker.playlist_start.lock().await = time;
        let producer_time = time + chrono::Duration::hours(1);
        let original = format!(
            "{header}#EXT-X-PROGRAM-DATE-TIME:{}\nlive000000.ts\n#EXTINF:4.004,\n#EXT-X-PROGRAM-DATE-TIME:{}\nlive000001.ts\n",
            producer_time.to_rfc3339(),
            (producer_time + chrono::Duration::milliseconds(4_004)).to_rfc3339()
        );
        tokio::fs::write(root.path().join("live.m3u8"), &original)
            .await
            .unwrap();
        assert!(worker
            .segment_output("live000000.ts", time.timestamp_millis())
            .await
            .unwrap()
            .error
            .is_some());
        assert!(worker
            .segment_output("live000001.ts", time.timestamp_millis() + 4_004)
            .await
            .unwrap()
            .error
            .is_some());
        assert!(worker
            .segment_output("live000000.ts", time.timestamp_millis() - 1)
            .await
            .is_none());
        assert!(worker
            .segment_output("live000000.ts", producer_time.timestamp_millis())
            .await
            .is_none());
        assert_eq!(
            tokio::fs::read_to_string(root.path().join("live.m3u8"))
                .await
                .unwrap(),
            original
        );
        assert_eq!(*worker.playlist_start.lock().await, time);
        assert_eq!(worker.last_access_ms.load(Ordering::Relaxed), 0);
        tokio::fs::write(
            root.path().join("live.m3u8"),
            format!("{header}live000000.ts\n"),
        )
        .await
        .unwrap();
        assert!(worker
            .segment_output("live000000.ts", time.timestamp_millis())
            .await
            .is_none());
    }

    #[test]
    fn session_model_reports_native_state_cursor_and_last_access_without_touching() {
        let root = tempfile::tempdir().unwrap();
        let worker = HlsSessionWorker::new(
            "native-test".into(),
            1,
            root.path().into(),
            OutputFormat::Hls,
            Vec::new(),
            test_ffmpeg_runtime(FfmpegCapabilityReport::default()),
        );
        worker.last_access_ms.store(1_000, Ordering::Relaxed);
        worker.started_at_ms.store(500, Ordering::Relaxed);
        assert!(worker.session_model().unwrap().transcoded_until.is_none());
        let until = DateTime::from_timestamp_millis(9_000).unwrap();
        worker.report_transcoded_until(until);
        worker.set_session_state(HlsSessionState::ZeroAndWorkAhead);
        let report = worker.session_model().unwrap();
        assert_eq!(report.channel_number, "native-test");
        assert_eq!(report.state, "ZeroAndWorkAhead");
        assert_eq!(report.transcoded_until, Some(until));
        assert_eq!(report.last_access.timestamp_millis(), 1_000);
        assert_eq!(report.started_at.timestamp_millis(), 500);
        assert_eq!(
            serde_json::to_value(&report).unwrap()["startedAt"],
            serde_json::to_value(report.started_at).unwrap()
        );
        assert_eq!(worker.last_access_ms.load(Ordering::Relaxed), 1_000);
        worker.touch();
        assert!(worker.session_model().unwrap().last_access > report.last_access);
        worker.report_transcoded_until(until + chrono::TimeDelta::seconds(30));
        assert_eq!(
            worker.session_model().unwrap().started_at,
            report.started_at
        );
    }

    #[tokio::test]
    async fn cancel_signals_worker_run_loop() {
        let worker = HlsSessionWorker::new(
            "42".to_string(),
            1,
            std::env::temp_dir().join("ersatzrs-hls-test-cancel"),
            OutputFormat::Hls,
            Vec::new(),
            test_ffmpeg_runtime(FfmpegCapabilityReport::default()),
        );

        // `cancel` is requested while nothing is awaiting; `notify_one` must
        // store a permit so the worker's select loop observes it on its next
        // `cancel.notified()` poll.
        worker.cancel().await;
        timeout(Duration::from_millis(250), worker.cancel.notified())
            .await
            .expect("worker run loop observes the cancellation signal");
    }

    #[tokio::test]
    async fn cancellation_stops_startup_reads_before_a_process_exists() {
        let worker = HlsSessionWorker::new(
            "42".to_string(),
            1,
            std::env::temp_dir().join("ersatzrs-hls-test-cancel-read"),
            OutputFormat::Hls,
            Vec::new(),
            test_ffmpeg_runtime(FfmpegCapabilityReport::default()),
        );
        let read = worker.startup_read(std::future::pending::<RepositoryResult<()>>());
        tokio::pin!(read);
        assert!(timeout(Duration::from_millis(10), &mut read).await.is_err());
        worker.cancel().await;
        assert!(timeout(Duration::from_millis(250), read)
            .await
            .expect("cancel interrupts the pending source lookup")
            .is_err());
        assert!(matches!(
            worker.startup_readiness(),
            HlsStartupReadiness::Failed
        ));

        // A stored cancel permit must also beat an immediately failing source
        // read; otherwise the surrounding error/retry loop can continue forever.
        worker.cancel().await;
        let error = worker
            .startup_read(async {
                Err::<(), _>(RepositoryError::Conflict("source unavailable".into()))
            })
            .await
            .unwrap_err();
        assert!(error.to_string().contains("HLS startup cancelled"));
    }

    #[test]
    fn hybrid_bootstrap_cancellation_releases_owned_producer_without_opening_source() {
        let _guard = hls_counter_test_guard();
        reset_active_transcode_state();
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                let root = tempfile::tempdir().unwrap();
                let output = root.path().join("cancelled-bootstrap");
                let worker = HlsSessionWorker::new(
                    "bootstrap-cancel".into(),
                    1,
                    output.clone(),
                    OutputFormat::Hls,
                    Vec::new(),
                    test_ffmpeg_runtime(FfmpegCapabilityReport::default()),
                );
                let registry: Arc<dyn HlsSessionRegistry> =
                    Arc::new(crate::hls::InMemoryHlsSessionRegistry::new());
                assert!(registry.try_insert(worker.clone()).await);
                worker.initialize_startup(std::time::Instant::now() + Duration::from_secs(1), 3);
                let (wrapper, producer) = worker
                    .try_reserve_wrapper_capacity(2, Some(Duration::from_mins(1)))
                    .unwrap();
                assert_eq!(
                    crate::transcode_capacity::active_transcode_snapshot(2).active,
                    2
                );
                worker.cancel().await;
                let pool = sqlx::sqlite::SqlitePoolOptions::new()
                    .connect_lazy("sqlite::memory:")
                    .unwrap();
                worker.start_with_initial_reservation(
                    pool,
                    Arc::clone(&registry),
                    Some(Duration::from_mins(1)),
                    producer,
                );
                timeout(Duration::from_secs(1), async {
                    while registry.try_get("bootstrap-cancel").await.is_some()
                        || crate::transcode_capacity::active_transcode_snapshot(2).active != 1
                    {
                        tokio::task::yield_now().await;
                    }
                })
                .await
                .expect("cancelled bootstrap releases its producer and registry entry");
                assert!(
                    !output.exists(),
                    "cancelled startup must not prepare output or open media"
                );
                drop(wrapper);
                assert_eq!(
                    crate::transcode_capacity::active_transcode_snapshot(2).active,
                    0
                );
                assert_eq!(
                    crate::transcode_capacity::active_transcode_snapshot(2).peak,
                    2
                );
            });
    }

    #[tokio::test]
    async fn session_does_not_publish_stale_playlist_before_working_dir_is_prepared() {
        let working_dir =
            std::env::temp_dir().join(format!("ersatzrs-hls-test-stale-{}", Uuid::new_v4()));
        tokio::fs::create_dir_all(&working_dir)
            .await
            .expect("create stale hls working dir");
        tokio::fs::write(
            working_dir.join("live.m3u8"),
            "#EXTM3U\n#EXTINF:4.0,\nsegment00001.ts\n",
        )
        .await
        .expect("write stale hls playlist");
        let worker = HlsSessionWorker::new(
            "42".to_string(),
            1,
            working_dir.clone(),
            OutputFormat::Hls,
            Vec::new(),
            test_ffmpeg_runtime(FfmpegCapabilityReport::default()),
        );

        assert!(HlsSessionHandle::trim_playlist(&*worker).await.is_none());

        tokio::fs::remove_dir_all(working_dir)
            .await
            .expect("remove stale hls working dir");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn idle_timeout_cancels_running_hls_process() {
        let working_dir =
            std::env::temp_dir().join(format!("ersatzrs-hls-test-idle-{}", Uuid::new_v4()));
        tokio::fs::create_dir_all(&working_dir)
            .await
            .expect("create hls working dir");
        let worker = HlsSessionWorker::new(
            "42".to_string(),
            1,
            working_dir.clone(),
            OutputFormat::Hls,
            Vec::new(),
            test_ffmpeg_runtime(FfmpegCapabilityReport::default()),
        );
        worker.touch();
        let command = FfmpegCommand {
            mpegts_output: None,
            executable: "/bin/sleep".to_string(),
            arguments: vec!["30".to_string()],
            environment: Vec::new(),
        };
        let mut cleanup_interval = tokio::time::interval(Duration::from_secs(59));

        let outcome = timeout(
            Duration::from_secs(2),
            worker.run_hls_process(
                command,
                &mut cleanup_interval,
                Some(Duration::from_millis(50)),
                None,
                None,
            ),
        )
        .await
        .expect("idle timeout should cancel the running process");

        assert_eq!(outcome, HlsProcessOutcome::Cancelled);
        let _ = cleanup_working_dir(&working_dir).await;
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn watch_idle_timeout_reclaims_running_hls_process_without_cancelling_session() {
        let working_dir =
            std::env::temp_dir().join(format!("ersatzrs-hls-test-watch-idle-{}", Uuid::new_v4()));
        tokio::fs::create_dir_all(&working_dir)
            .await
            .expect("create hls working dir");
        let worker = HlsSessionWorker::new(
            "42".to_string(),
            1,
            working_dir.clone(),
            OutputFormat::Hls,
            Vec::new(),
            test_ffmpeg_runtime(FfmpegCapabilityReport::default()),
        );
        worker.touch();
        let command = FfmpegCommand {
            mpegts_output: None,
            executable: "/bin/sleep".to_string(),
            arguments: vec!["30".to_string()],
            environment: Vec::new(),
        };
        let mut cleanup_interval = tokio::time::interval(Duration::from_secs(59));

        let outcome = timeout(
            Duration::from_secs(2),
            worker.run_hls_process(
                command,
                &mut cleanup_interval,
                None,
                Some(Duration::from_millis(50)),
                None,
            ),
        )
        .await
        .expect("watch idle timeout should reclaim the running process");

        assert_eq!(outcome, HlsProcessOutcome::Reclaimed);
        let _ = cleanup_working_dir(&working_dir).await;
    }

    #[test]
    fn playout_updated_sets_worker_state_for_next_iteration() {
        let worker = HlsSessionWorker::new(
            "42".to_string(),
            1,
            std::env::temp_dir().join("ersatzrs-hls-test-playout-updated"),
            OutputFormat::Hls,
            Vec::new(),
            test_ffmpeg_runtime(FfmpegCapabilityReport::default()),
        );

        worker.set_session_state(HlsSessionState::ZeroAndRealtime);
        worker.playout_updated();

        assert_eq!(worker.session_state(), HlsSessionState::PlayoutUpdated);
    }

    #[tokio::test]
    async fn worker_capability_snapshots_follow_shared_runtime_updates() {
        let runtime = test_ffmpeg_runtime(FfmpegCapabilityReport::default());
        let worker = HlsSessionWorker::new(
            "42".to_owned(),
            1,
            std::env::temp_dir().join("ersatzrs-hls-test-runtime-refresh"),
            OutputFormat::Hls,
            Vec::new(),
            Arc::clone(&runtime),
        );

        assert!(!worker
            .runtime_capabilities()
            .await
            .filters
            .contains("runtime-refresh-marker"));

        runtime
            .write()
            .await
            .capabilities
            .filters
            .insert("runtime-refresh-marker".to_owned());

        assert!(worker
            .runtime_capabilities()
            .await
            .filters
            .contains("runtime-refresh-marker"));
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
        assert_eq!(
            hls_transcode_request_time(HlsSessionState::ZeroAndWorkAhead, transcoded_until, now),
            transcoded_until
        );
    }

    #[test]
    fn live_source_waits_for_its_wall_clock_boundary() {
        let now = Utc::now();
        assert_eq!(
            live_source_admission_delay(true, now + chrono::Duration::seconds(9), now),
            Some(Duration::from_secs(9))
        );
        assert_eq!(live_source_admission_delay(true, now, now), None);
        assert_eq!(
            live_source_admission_delay(false, now + chrono::Duration::seconds(9), now),
            None
        );
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
    fn active_transcode_reservation_respects_configured_limit() {
        let _guard = hls_counter_test_guard();
        reset_active_transcode_state();
        let first = try_reserve_active_transcode(
            2,
            test_active_slot("1", ActiveTranscodeKind::HlsRealtime, Duration::ZERO, true),
        );
        let second = try_reserve_active_transcode(
            2,
            test_active_slot("2", ActiveTranscodeKind::HlsRealtime, Duration::ZERO, true),
        );
        assert!(first.is_some());
        assert!(second.is_some());
        assert!(try_reserve_active_transcode(
            2,
            test_active_slot("3", ActiveTranscodeKind::HlsRealtime, Duration::ZERO, true,),
        )
        .is_none());
        assert_eq!(
            crate::transcode_capacity::active_transcode_snapshot(2).active,
            2
        );
        drop(second);
        drop(first);
        assert_eq!(
            crate::transcode_capacity::active_transcode_snapshot(2).active,
            0
        );
        assert!(try_reserve_active_transcode(
            0,
            test_active_slot("4", ActiveTranscodeKind::HlsRealtime, Duration::ZERO, true,),
        )
        .is_none());
    }

    #[test]
    fn watch_idle_timeout_decouples_transcode_slots_from_idle_retention() {
        assert_eq!(
            transcode_watch_idle_timeout(Some(Duration::from_mins(1))),
            Some(HLS_ACTIVE_WATCH_WINDOW)
        );
        assert_eq!(
            transcode_watch_idle_timeout(Some(Duration::from_secs(30))),
            None
        );
        assert_eq!(transcode_watch_idle_timeout(None), None);
    }

    #[test]
    fn active_transcode_reclaim_prefers_work_ahead_before_other_slots() {
        let _guard = hls_counter_test_guard();
        reset_active_transcode_state();
        let work_ahead = try_reserve_active_transcode(
            3,
            test_active_slot(
                "work-ahead",
                ActiveTranscodeKind::HlsWorkAhead,
                Duration::from_secs(1),
                true,
            ),
        );
        let cooling = try_reserve_active_transcode(
            3,
            test_active_slot(
                "cooling",
                ActiveTranscodeKind::HlsRealtime,
                HLS_ACTIVE_WATCH_WINDOW + Duration::from_secs(5),
                true,
            ),
        );
        let stale = try_reserve_active_transcode(
            3,
            test_active_slot(
                "stale",
                ActiveTranscodeKind::HlsRealtime,
                HLS_STALE_WATCHED_PREEMPTION_THRESHOLD,
                true,
            ),
        );

        let reclaim = request_active_transcode_reclaim(
            "requester",
            HLS_ACTIVE_WATCH_WINDOW,
            HLS_STALE_WATCHED_PREEMPTION_THRESHOLD,
            HLS_RECLAIMED_REACQUIRE_DELAY,
        )
        .expect("reclaim target");
        assert_eq!(reclaim.channel_number, "work-ahead");
        assert_eq!(reclaim.reason, ActiveTranscodeReclaimReason::WorkAhead);

        drop(stale);
        drop(cooling);
        drop(work_ahead);
    }

    #[test]
    fn active_transcode_reclaim_prefers_cooling_before_stale_watched() {
        let _guard = hls_counter_test_guard();
        reset_active_transcode_state();
        let cooling = try_reserve_active_transcode(
            2,
            test_active_slot(
                "cooling",
                ActiveTranscodeKind::HlsRealtime,
                HLS_ACTIVE_WATCH_WINDOW + Duration::from_secs(1),
                true,
            ),
        );
        let stale = try_reserve_active_transcode(
            2,
            test_active_slot(
                "stale",
                ActiveTranscodeKind::HlsRealtime,
                HLS_STALE_WATCHED_PREEMPTION_THRESHOLD,
                true,
            ),
        );

        let reclaim = request_active_transcode_reclaim(
            "requester",
            HLS_ACTIVE_WATCH_WINDOW,
            HLS_STALE_WATCHED_PREEMPTION_THRESHOLD,
            HLS_RECLAIMED_REACQUIRE_DELAY,
        )
        .expect("reclaim target");
        assert_eq!(reclaim.channel_number, "cooling");
        assert_eq!(reclaim.reason, ActiveTranscodeReclaimReason::Cooling);

        drop(stale);
        drop(cooling);
    }

    #[test]
    fn active_transcode_reclaim_preempts_only_stale_preemptible_watched_slots() {
        let _guard = hls_counter_test_guard();
        reset_active_transcode_state();
        let fresh = try_reserve_active_transcode(
            3,
            test_active_slot(
                "fresh",
                ActiveTranscodeKind::HlsRealtime,
                HLS_STALE_WATCHED_PREEMPTION_THRESHOLD
                    .checked_sub(Duration::from_secs(1))
                    .expect("threshold exceeds one second"),
                true,
            ),
        );
        let keep_running = try_reserve_active_transcode(
            3,
            test_active_slot(
                "keep-running",
                ActiveTranscodeKind::HlsRealtime,
                HLS_STALE_WATCHED_PREEMPTION_THRESHOLD,
                false,
            ),
        );
        let stale = try_reserve_active_transcode(
            3,
            test_active_slot(
                "stale",
                ActiveTranscodeKind::HlsRealtime,
                HLS_STALE_WATCHED_PREEMPTION_THRESHOLD,
                true,
            ),
        );

        let reclaim = request_active_transcode_reclaim(
            "requester",
            HLS_ACTIVE_WATCH_WINDOW,
            HLS_STALE_WATCHED_PREEMPTION_THRESHOLD,
            HLS_RECLAIMED_REACQUIRE_DELAY,
        )
        .expect("reclaim target");
        assert_eq!(reclaim.channel_number, "stale");
        assert_eq!(reclaim.reason, ActiveTranscodeReclaimReason::StaleWatched);

        drop(stale);
        drop(keep_running);
        drop(fresh);
    }

    #[test]
    fn active_transcode_reclaim_defers_target_reacquire_for_requester_retry_window() {
        let _guard = hls_counter_test_guard();
        reset_active_transcode_state();
        let worker = HlsSessionWorker::new(
            "target".to_string(),
            1,
            std::env::temp_dir().join("ersatzrs-hls-test-reclaim-deferral"),
            OutputFormat::Hls,
            Vec::new(),
            test_ffmpeg_runtime(FfmpegCapabilityReport::default()),
        );
        let reservation = try_reserve_active_transcode(
            1,
            ActiveTranscodeSlot::new_hls(
                "target",
                ActiveTranscodeKind::HlsWorkAhead,
                Arc::clone(&worker.last_access_ms),
                ActiveTranscodeReclaimHandle::new(
                    Arc::clone(&worker.reclaim_transcode),
                    Arc::clone(&worker.reclaim_requested_slot_id),
                    Arc::clone(&worker.reclaim_reacquire_deferred_until_ms),
                    true,
                ),
            ),
        );
        assert!(reservation.is_some());

        let reclaim = request_active_transcode_reclaim(
            "requester",
            HLS_ACTIVE_WATCH_WINDOW,
            HLS_STALE_WATCHED_PREEMPTION_THRESHOLD,
            HLS_RECLAIMED_REACQUIRE_DELAY,
        )
        .expect("reclaim target");
        assert_eq!(reclaim.channel_number, "target");
        assert_eq!(reclaim.reason, ActiveTranscodeReclaimReason::WorkAhead);

        let delay = worker
            .active_transcode_reacquire_delay()
            .expect("target reacquire is deferred");
        assert!(delay > Duration::ZERO);
        assert!(delay <= HLS_RECLAIMED_REACQUIRE_DELAY);

        drop(reservation);
    }

    #[test]
    fn stale_reclaim_generation_does_not_match_later_slot_generation() {
        let worker = HlsSessionWorker::new(
            "42".to_string(),
            1,
            std::env::temp_dir().join("ersatzrs-hls-test-stale-reclaim"),
            OutputFormat::Hls,
            Vec::new(),
            test_ffmpeg_runtime(FfmpegCapabilityReport::default()),
        );
        let first_slot = ActiveTranscodeSlot::new_hls(
            "42",
            ActiveTranscodeKind::HlsRealtime,
            Arc::clone(&worker.last_access_ms),
            ActiveTranscodeReclaimHandle::new(
                Arc::clone(&worker.reclaim_transcode),
                Arc::clone(&worker.reclaim_requested_slot_id),
                Arc::clone(&worker.reclaim_reacquire_deferred_until_ms),
                true,
            ),
        );
        let second_slot = ActiveTranscodeSlot::new_hls(
            "42",
            ActiveTranscodeKind::HlsRealtime,
            Arc::clone(&worker.last_access_ms),
            ActiveTranscodeReclaimHandle::new(
                Arc::clone(&worker.reclaim_transcode),
                Arc::clone(&worker.reclaim_requested_slot_id),
                Arc::clone(&worker.reclaim_reacquire_deferred_until_ms),
                true,
            ),
        );
        assert_ne!(first_slot.slot_id(), second_slot.slot_id());

        worker
            .reclaim_requested_slot_id
            .store(first_slot.slot_id(), Ordering::Release);
        assert!(!worker.take_reclaim_request(second_slot.slot_id()));
        assert_eq!(worker.reclaim_requested_slot_id.load(Ordering::Acquire), 0);

        worker
            .reclaim_requested_slot_id
            .store(second_slot.slot_id(), Ordering::Release);
        assert!(worker.take_reclaim_request(second_slot.slot_id()));
        assert_eq!(worker.reclaim_requested_slot_id.load(Ordering::Acquire), 0);
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
    fn hls_content_seek_and_remaining_duration_preserve_fractional_offsets() {
        let start = DateTime::parse_from_rfc3339("2026-06-08T12:00:00Z")
            .expect("timestamp")
            .with_timezone(&Utc);
        let request_time = start + chrono::Duration::milliseconds(517);
        let mut active =
            active_hls_item_for_window(start, start + chrono::Duration::milliseconds(44_000));
        active.in_point = Duration::from_millis(250);

        let elapsed = playout_start_duration(request_time, active.playout_start);

        assert_eq!(elapsed, Duration::from_millis(517));
        assert_eq!(
            active.content_seek_position(elapsed),
            Duration::from_millis(767)
        );
        assert_eq!(
            hls_playout_item_duration(&active, request_time, false, true),
            Some(Duration::from_millis(43_483))
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

    #[tokio::test]
    async fn hls_work_ahead_loader_distinguishes_absence_invalid_and_database_failure() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("open HLS config test database");
        sqlx::query(
            r#"CREATE TABLE "ConfigElement" (
                "Key" TEXT NULL,
                "Value" TEXT NULL
            )"#,
        )
        .execute(&pool)
        .await
        .expect("create ConfigElement");
        sqlx::query(
            r#"CREATE TABLE "Channel" (
                "Id" INTEGER PRIMARY KEY,
                "Name" TEXT NULL,
                "SlugSeconds" REAL NULL
            )"#,
        )
        .execute(&pool)
        .await
        .expect("create Channel");
        assert_eq!(
            load_hls_work_ahead_limit(&pool)
                .await
                .expect("load absent HLS work-ahead limit"),
            None
        );

        sqlx::query(r#"INSERT INTO "ConfigElement" ("Key", "Value") VALUES (?, 'invalid')"#)
            .bind(ersatzrs_domain::CONFIG_KEY_FFMPEG_WORK_AHEAD_SEGMENTERS)
            .execute(&pool)
            .await
            .expect("seed invalid HLS work-ahead limit");
        assert!(matches!(
            load_hls_work_ahead_limit(&pool).await,
            Err(RepositoryError::Decode { .. })
        ));

        sqlx::query(
            r#"INSERT INTO "Channel" ("Id", "Name", "SlugSeconds")
               VALUES (7, x'FF', 'invalid')"#,
        )
        .execute(&pool)
        .await
        .expect("seed malformed channel configuration");
        assert!(matches!(
            load_channel_slug_seconds(&pool, 7).await,
            Err(RepositoryError::Decode { .. })
        ));
        assert!(matches!(
            load_channel_name(&pool, 7).await,
            Err(RepositoryError::Decode { .. })
        ));

        pool.close().await;
        assert!(matches!(
            load_hls_work_ahead_limit(&pool).await,
            Err(RepositoryError::Unavailable { .. })
        ));
    }

    #[tokio::test]
    async fn live_remote_stream_lookup_uses_transcode_cursor_not_wall_clock() {
        let pool = active_hls_item_test_pool().await;
        let wall_clock_now = DateTime::parse_from_rfc3339("2026-06-08T12:00:00Z")
            .expect("wall clock")
            .with_timezone(&Utc);
        let transcode_cursor = DateTime::parse_from_rfc3339("2026-06-08T12:01:30Z")
            .expect("transcode cursor")
            .with_timezone(&Utc);
        sqlx::query(
            r#"INSERT INTO "RemoteStream" (
                   "Id", "Url", "Script", "Duration", "FallbackQuery", "Liveness", "IsLive"
               ) VALUES (30, 'https://example.test/live.m3u8', NULL, NULL, NULL, 2, 1)"#,
        )
        .execute(&pool)
        .await
        .expect("remote stream");
        sqlx::query(
            r#"INSERT INTO "PlayoutItem" (
                   "Id", "MediaItemId", "PlayoutId", "InPoint", "Start", "Finish",
                   "DisableWatermarks", "SubtitleMode"
               ) VALUES (
                   13, 30, 11, '00:00:00', '2026-06-08 12:01:00',
                   '2026-06-08 12:03:00', 0, 0
               )"#,
        )
        .execute(&pool)
        .await
        .expect("next live playout item");

        assert!(!current_remote_stream_is_live(&pool, 7, wall_clock_now)
            .await
            .expect("wall-clock liveness"));
        assert!(current_remote_stream_is_live(&pool, 7, transcode_cursor)
            .await
            .expect("cursor liveness"));
    }

    #[tokio::test]
    async fn active_hls_item_loader_preserves_absence_decode_and_database_failure() {
        let pool = active_hls_item_test_pool().await;
        let now = DateTime::parse_from_rfc3339("2026-06-08T12:00:00Z")
            .expect("timestamp")
            .with_timezone(&Utc);
        assert!(load_active_hls_item(&pool, 99, &[], now)
            .await
            .expect("load absent active item")
            .is_none());
        assert!(matches!(
            load_active_hls_item(&pool, 7, &[], now).await,
            Err(RepositoryError::Decode { .. })
        ));
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
        .expect("pin silent subtitle-off HLS item");
        let active = load_active_hls_item(&pool, 7, &[], now)
            .await
            .expect("load pinned HLS item")
            .expect("active HLS item");
        assert!(active.has_pinned_stream_selection);
        assert_eq!(active.media_file_id, Some(1));
        assert_eq!(active.original_media_path, "/media/test.mkv");
        assert_eq!(active.audio_stream_index, None);
        assert_eq!(active.subtitle_stream_index, None);

        sqlx::query(
            r#"INSERT INTO "RemoteStream" (
                   "Id", "Url", "Script", "Duration", "FallbackQuery", "Liveness", "IsLive"
               ) VALUES (
                   30, NULL, '"tools/live producer.sh"', NULL, NULL, 2, 1
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

        assert!(current_remote_stream_is_live(&pool, 7, now)
            .await
            .expect("load current live state"));
        assert!(matches!(
            load_active_hls_item(&pool, 7, &[], now).await,
            Err(RepositoryError::Decode { .. })
        ));

        pool.close().await;
        assert!(matches!(
            load_active_hls_item(&pool, 7, &[], now).await,
            Err(RepositoryError::Unavailable { .. })
        ));
    }

    #[tokio::test]
    async fn active_hls_item_preserves_unmanaged_remote_url() {
        let pool = active_hls_item_test_pool().await;
        sqlx::raw_sql(
            r#"
            INSERT INTO "RemoteStream" ("Id","Url","Duration","Liveness","IsLive")
                VALUES (30,'https://example.test/finite.mp4','00:02:00',1,0);
            UPDATE "MediaVersion" SET "MovieId"=NULL,"RemoteStreamId"=30 WHERE "Id"=21;
            UPDATE "PlayoutItem" SET "MediaItemId"=30,"SubtitleMode"=0 WHERE "Id"=12;
        "#,
        )
        .execute(&pool)
        .await
        .unwrap();
        let now = DateTime::parse_from_rfc3339("2026-06-08T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let active = load_active_hls_item(&pool, 7, &[], now)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            active.remote_stream.unwrap().source,
            crate::remote_stream::RemoteStreamSource::Url("https://example.test/finite.mp4".into())
        );
    }

    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn active_hls_item_uses_prepared_publication_and_rejects_missing_artifact() {
        use ersatzrs_core::repositories::{
            AddonMediaListLifecycleRepository as _, AddonMediaListRevisionArtifacts,
        };
        let temporary = tempfile::tempdir().unwrap();
        let pool = crate::db::init(&format!(
            "sqlite:{}",
            temporary.path().join("fixture.db").display()
        ))
        .await
        .unwrap();
        sqlx::raw_sql(r#"
            INSERT INTO "Resolution" ("Id","Name","Width","Height") VALUES (801,'fixture',320,180);
            INSERT INTO "FFmpegProfile" (
                "Id","Name","ThreadCount","TranscodeVideo","TranscodeAudio","ResolutionId",
                "ScalingBehavior","VideoFormat","AudioFormat","VideoBitrate","VideoBufferSize",
                "AudioBitrate","AudioBufferSize","NormalizeLoudnessMode","AudioChannels","AudioSampleRate",
                "HardwareAcceleration","QsvExtraHardwareFrames","VaapiDriver","TonemapAlgorithm",
                "AllowBFrames","BitDepth","NormalizeFramerate","NormalizeColors","DeinterlaceVideo","PadMode")
                VALUES (802,'fixture',0,1,1,801,0,1,1,4000,8000,192,384,0,2,48,0,64,0,0,0,0,1,1,1,0);
            INSERT INTO "AddonInstallation" ("AddonId","Version","Enabled","PackageSha256","ContentSha256","ManifestJson","InstallPath","InstalledAt","UpdatedAt")
                VALUES ('org.example.fixture','1.0.0',1,'fixture','fixture','{}','addons/fixture','2026-09-05T00:00:00Z','2026-09-05T00:00:00Z');
            INSERT INTO "MediaSource" ("Id") VALUES (1);
            INSERT INTO "Library" ("Id","MediaKind","MediaSourceId") VALUES (1,4,1);
            INSERT INTO "LibraryPath" ("Id","LibraryId","Path") VALUES (1,1,'streams');
            INSERT INTO "MediaItem" ("Id","LibraryPathId") VALUES (100,1);
            INSERT INTO "RemoteStream" ("Id","AddonId","ProviderId","Url","Liveness","Duration")
                VALUES (100,'org.example.fixture','item','https://example.test/item',1,'00:01:00');
            INSERT INTO "MediaVersion" ("Id","RemoteStreamId","Duration","DateAdded","DateUpdated","Height","Width","VideoScanKind")
                VALUES (101,100,'00:01:00','2026-09-05 12:00:00','2026-09-05 12:00:00',180,320,0);
            INSERT INTO "MediaFile" ("MediaVersionId","Path","PathHash") VALUES (101,'streams/item.yml','fixture');
            INSERT INTO "AddonMediaList" ("Id","AddonId","ProviderId","SourceUrl","Name","PublishedRevision")
                VALUES (1,'org.example.fixture','list','https://example.test/list','Fixture',1);
            INSERT INTO "AddonMediaListLibrary" ("AddonMediaListId","LibraryId","FolderName","ManifestPath")
                VALUES (1,1,'fixture','streams/list.yml');
            INSERT INTO "AddonMediaListItem" ("Id","AddonMediaListId","ProviderId","DisplayTitle","Kind","Rank","Title","SourceUrl","ProviderLiveness","EffectiveLiveness","Published","MediaItemId")
                VALUES (10,1,'item','Fixture',4,0,'Fixture','https://example.test/item',1,1,1,100);
            INSERT INTO "AddonMediaListArtifact" ("Id","AddonMediaListId","AddonMediaListItemId","CacheKey","Kind","State","RelativePath","ByteLength","DurationMilliseconds","CreatedAt","UpdatedAt")
                VALUES (1,1,10,'fixture',1,2,'objects/prepared.media',5,60000,'2026-09-05T00:00:00Z','2026-09-05T00:00:00Z');
            INSERT INTO "Channel" ("Id","UniqueId","Number","SortNumber","Name","FFmpegProfileId","StreamingMode","StreamSelectorMode","PlayoutSource","PlayoutMode","TranscodeMode","IdleBehavior","MusicVideoCreditsMode","SongVideoMode","SubtitleMode")
                SELECT 7,'70000000-0000-0000-0000-000000000007','7',7,'Seven',"Id",5,0,0,0,0,0,0,0,0 FROM "FFmpegProfile" ORDER BY "Id" LIMIT 1;
            INSERT INTO "Playout" ("Id","ChannelId","ScheduleKind","Seed") VALUES (11,7,20,1);
            INSERT INTO "PlayoutItem" ("Id","MediaItemId","PlayoutId","InPoint","OutPoint","Start","Finish","DisableWatermarks","GuideGroup","FillerKind","SubtitleMode")
                VALUES (12,100,11,'00:00:00','00:01:00','2026-09-05 11:59:30','2026-09-05 12:00:30',0,0,0,0);
        "#).execute(&pool).await.unwrap();
        let artifact = temporary.path().join("objects/prepared.media");
        tokio::fs::create_dir(artifact.parent().unwrap())
            .await
            .unwrap();
        tokio::fs::write(&artifact, b"media").await.unwrap();
        crate::repositories::SqliteAddonMediaListRepository(pool.clone())
            .retain_published_revision(
                1,
                1,
                0,
                Utc::now(),
                &[],
                &AddonMediaListRevisionArtifacts {
                    ids: vec![1],
                    prepared_media: [("item".into(), 1)].into(),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        let now = DateTime::parse_from_rfc3339("2026-09-05T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let active = load_active_hls_item_with_artifact_root(&pool, 7, &[], now, temporary.path())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(active.media_path, "streams/item.yml");
        assert_eq!(active.content_duration, Some(Duration::from_mins(1)));
        let remote = active.remote_stream.unwrap();
        assert_eq!(
            remote.source,
            crate::remote_stream::RemoteStreamSource::Prepared(artifact.clone())
        );
        assert!(!remote.uses_addon);
        tokio::fs::remove_file(&artifact).await.unwrap();
        assert!(
            load_active_hls_item_with_artifact_root(&pool, 7, &[], now, temporary.path())
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn active_hls_item_uses_source_programming_and_target_channel_settings() {
        let pool = active_hls_item_test_pool().await;
        sqlx::query(
            r#"INSERT INTO "Channel" (
                   "Id", "Name", "StreamSelectorMode", "SubtitleMode",
                   "MusicVideoCreditsMode", "PlayoutSource", "MirrorSourceChannelId",
                   "PlayoutOffset"
               ) VALUES (8, 'Delayed Eight', 0, 0, 0, 1, 7, '01:00:00')"#,
        )
        .execute(&pool)
        .await
        .expect("seed mirror target");
        sqlx::query(r#"UPDATE "PlayoutItem" SET "SubtitleMode" = 0 WHERE "Id" = 12"#)
            .execute(&pool)
            .await
            .expect("make HLS item valid");
        let mirror_now = DateTime::parse_from_rfc3339("2026-06-08T13:00:00Z")
            .expect("timestamp")
            .with_timezone(&Utc);

        let replacements = [PathReplacement {
            source_path: "/media/".to_owned(),
            replacement_path: "/replacement/".to_owned(),
            source_platform: PathPlatform::Unix,
        }];
        let active = load_active_hls_item(&pool, 8, &replacements, mirror_now)
            .await
            .expect("load mirror HLS item")
            .expect("active mirror HLS item");

        assert_eq!(active.playout_item_id, 12);
        assert_eq!(active.media_file_id, Some(1));
        assert_eq!(active.original_media_path, "/media/test.mkv");
        assert_eq!(active.media_path, "/replacement/test.mkv");
        assert_eq!(active.channel_name, "Delayed Eight");
        assert_eq!(
            active.playout_start,
            DateTime::parse_from_rfc3339("2026-06-08T12:59:00Z")
                .unwrap()
                .with_timezone(&Utc)
        );
        assert_eq!(
            active.playout_finish,
            Some(
                DateTime::parse_from_rfc3339("2026-06-08T13:01:00Z")
                    .unwrap()
                    .with_timezone(&Utc)
            )
        );
    }

    #[allow(clippy::too_many_lines)] // The inline schema mirrors the production query shape.
    async fn active_hls_item_test_pool() -> sqlx::SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("open active HLS item test database");
        for statement in [
            r#"CREATE TABLE "Channel" (
                "Id" INTEGER PRIMARY KEY,
                "Name" TEXT NULL,
                "WatermarkId" INTEGER NULL,
                "StreamSelectorMode" INTEGER NOT NULL,
                "StreamSelector" TEXT NULL,
                "PreferredAudioLanguageCode" TEXT NULL,
                "PreferredAudioTitle" TEXT NULL,
                "PreferredSubtitleLanguageCode" TEXT NULL,
                "SubtitleMode" INTEGER NOT NULL,
                "MusicVideoCreditsMode" INTEGER NOT NULL,
                "MusicVideoCreditsTemplate" TEXT NULL,
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
                "Id" INTEGER PRIMARY KEY,
                "MediaVersionId" INTEGER NOT NULL,
                "Path" TEXT NULL
            )"#,
            r#"CREATE TABLE "RemoteStream" (
                "Id" INTEGER PRIMARY KEY,
                "Url" TEXT NULL,
                "AddonId" TEXT NULL,
                "ProviderId" TEXT NULL,
                "Script" TEXT NULL,
                "Duration" TEXT NULL,
                "FallbackQuery" TEXT NULL,
                "Liveness" INTEGER NOT NULL,
                "IsLive" INTEGER NOT NULL
            )"#,
            r#"CREATE TABLE "AddonInstallation" (
                "AddonId" TEXT PRIMARY KEY,
                "Enabled" INTEGER NOT NULL,
                "ManifestJson" TEXT NOT NULL
            )"#,
            r#"CREATE TABLE "MediaItem" ("Id" INTEGER PRIMARY KEY, "LibraryPathId" INTEGER)"#,
            r#"CREATE TABLE "LibraryPath" ("Id" INTEGER PRIMARY KEY, "LibraryId" INTEGER)"#,
            r#"CREATE TABLE "AddonMediaListLibrary" ("AddonMediaListId" INTEGER, "LibraryId" INTEGER)"#,
            r#"CREATE TABLE "AddonMediaList" ("Id" INTEGER PRIMARY KEY, "PublishedRevision" INTEGER, "AddonId" TEXT)"#,
        ] {
            sqlx::query(statement)
                .execute(&pool)
                .await
                .expect("create active HLS item test table");
        }
        for statement in [
            r#"INSERT INTO "Channel" (
                "Id", "Name", "StreamSelectorMode", "SubtitleMode",
                "MusicVideoCreditsMode", "PlayoutSource"
            ) VALUES (7, 'Seven', 0, 0, 0, 0)"#,
            r#"INSERT INTO "Playout" ("Id", "ChannelId") VALUES (11, 7)"#,
            r#"INSERT INTO "PlayoutItem" (
                "Id", "MediaItemId", "PlayoutId", "InPoint", "Start", "Finish",
                "DisableWatermarks", "SubtitleMode"
            ) VALUES (
                12, 20, 11, '00:00:00', '2026-06-08 11:59:00',
                '2026-06-08 12:01:00', 0, 99
            )"#,
            r#"INSERT INTO "MediaVersion" ("Id", "Duration", "MovieId")
               VALUES (21, '00:02:00', 20)"#,
            r#"INSERT INTO "MediaFile" ("MediaVersionId", "Path")
               VALUES (21, '/media/test.mkv')"#,
        ] {
            sqlx::query(statement)
                .execute(&pool)
                .await
                .expect("seed active HLS item test row");
        }
        pool
    }

    #[tokio::test]
    async fn on_demand_startup_timeshifts_playout_and_updates_checkpoint() {
        let pool = on_demand_test_pool().await;

        let now = DateTime::parse_from_rfc3339("2026-06-08T12:00:00Z")
            .expect("timestamp")
            .with_timezone(&Utc);
        assert_eq!(
            load_channel_slug_seconds(&pool, 7)
                .await
                .expect("load channel slug seconds"),
            Some(2.5)
        );
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

    #[tokio::test]
    async fn on_demand_timeshift_trims_only_items_before_checkpoint() {
        let pool = on_demand_test_pool().await;
        sqlx::query(
            r#"UPDATE "Playout" SET "OnDemandCheckpoint" = '2026-06-08 10:00:24' WHERE "Id" = 11"#,
        )
        .execute(&pool)
        .await
        .expect("set checkpoint");
        for statement in [
            r#"INSERT INTO "PlayoutItem" ("Id", "PlayoutId", "Start", "Finish") VALUES (22, 11, '2026-06-08 09:00:00', '2026-06-08 10:00:23')"#,
            r#"INSERT INTO "PlayoutItem" ("Id", "PlayoutId", "Start", "Finish") VALUES (23, 11, '2026-06-08 10:00:24', '2026-06-08 10:01:24')"#,
            r#"INSERT INTO "Channel" ("Id", "Number", "Name", "PlayoutMode") VALUES (8, '8', 'Other', 1)"#,
            r#"INSERT INTO "Playout" ("Id", "ChannelId") VALUES (12, 8)"#,
            r#"INSERT INTO "PlayoutItem" ("Id", "PlayoutId", "Start", "Finish") VALUES (24, 12, '2026-06-08 09:00:00', '2026-06-08 10:00:23')"#,
        ] {
            sqlx::query(statement)
                .execute(&pool)
                .await
                .expect("seed item");
        }

        let now = DateTime::parse_from_rfc3339("2026-06-08T12:00:00Z")
            .expect("timestamp")
            .with_timezone(&Utc);
        time_shift_on_demand_playout(&pool, 7, now)
            .await
            .expect("shift playout");
        let rows = sqlx::query(r#"SELECT "Id", "Start" FROM "PlayoutItem" ORDER BY "Id""#)
            .fetch_all(&pool)
            .await
            .expect("load surviving rows");
        let actual = rows
            .iter()
            .map(|row| (row.get::<i64, _>("Id"), row.get::<String, _>("Start")))
            .collect::<Vec<_>>();
        assert_eq!(
            actual,
            vec![
                (21, "2026-06-08 11:59:36".to_owned()),
                (23, "2026-06-08 12:00:00".to_owned()),
                (24, "2026-06-08 09:00:00".to_owned()),
            ]
        );
    }

    #[tokio::test]
    async fn on_demand_gaps_shift_trim_and_roll_back_with_items() {
        let pool = on_demand_test_pool().await;
        for statement in [
            r#"UPDATE "Playout" SET "OnDemandCheckpoint"='2026-06-08 10:00:00' WHERE "Id"=11"#,
            r#"INSERT INTO "PlayoutGap" ("Id", "PlayoutId", "Start", "Finish") VALUES
                (1, 11, '2026-06-08 09:00:00', '2026-06-08 09:30:00'),
                (2, 11, '2026-06-08 09:30:00', '2026-06-08 10:00:00'),
                (3, 11, '2026-06-08 10:00:24', '2026-06-08 10:01:00'),
                (4, 12, '2026-06-08 09:00:00', '2026-06-08 09:30:00')"#,
        ] {
            sqlx::query(statement).execute(&pool).await.unwrap();
        }
        let now = "2026-06-08T12:00:00Z".parse().unwrap();
        time_shift_on_demand_playout(&pool, 7, now).await.unwrap();
        let rows: Vec<(i64, String, String)> =
            sqlx::query_as(r#"SELECT "Id", "Start", "Finish" FROM "PlayoutGap" ORDER BY "Id""#)
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(
            rows,
            vec![
                (
                    2,
                    "2026-06-08 11:30:00".into(),
                    "2026-06-08 12:00:00".into()
                ),
                (
                    3,
                    "2026-06-08 12:00:24".into(),
                    "2026-06-08 12:01:00".into()
                ),
                (
                    4,
                    "2026-06-08 09:00:00".into(),
                    "2026-06-08 09:30:00".into()
                ),
            ]
        );
        sqlx::query(
            r#"CREATE TRIGGER reject_gap_shift BEFORE UPDATE ON "PlayoutGap"
            BEGIN SELECT RAISE(ABORT, 'synthetic gap failure'); END"#,
        )
        .execute(&pool)
        .await
        .unwrap();
        assert!(
            time_shift_on_demand_playout(&pool, 7, now + chrono::Duration::hours(1))
                .await
                .is_err()
        );
        let item_start: String =
            sqlx::query_scalar(r#"SELECT "Start" FROM "PlayoutItem" WHERE "Id"=21"#)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(item_start, "2026-06-08 12:00:00");
        let unchanged: Vec<(i64, String, String)> =
            sqlx::query_as(r#"SELECT "Id", "Start", "Finish" FROM "PlayoutGap" ORDER BY "Id""#)
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(
            unchanged, rows,
            "gap pruning and shifting must roll back too"
        );
    }

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

    async fn create_on_demand_test_schema(pool: &sqlx::SqlitePool) {
        for statement in [
            r#"CREATE TABLE "PlayoutGap" ("Id" INTEGER PRIMARY KEY, "PlayoutId" INTEGER NOT NULL, "Start" TEXT NOT NULL, "Finish" TEXT NOT NULL)"#,
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
            Duration::ZERO,
            None,
            None,
            &playback_profile(),
            None,
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
    fn hls_output_discontinuity_is_skipped_for_normalized_mpegts_appends() {
        assert!(!hls_output_discontinuity(
            OutputFormat::Hls,
            false,
            Some(Duration::from_secs(12)),
            false
        ));
        assert!(hls_output_discontinuity(
            OutputFormat::Hls,
            false,
            None,
            false
        ));
        assert!(hls_output_discontinuity(
            OutputFormat::Hls,
            false,
            Some(Duration::ZERO),
            false
        ));
        assert!(hls_output_discontinuity(
            OutputFormat::Hls,
            false,
            Some(Duration::from_secs(12)),
            true
        ));
        assert!(hls_output_discontinuity(
            OutputFormat::HlsMp4,
            false,
            Some(Duration::from_secs(12)),
            false
        ));
    }

    #[tokio::test]
    async fn completed_live_output_duration_keeps_short_final_segment_and_rejects_invalid_time() {
        let root = tempfile::tempdir().unwrap();
        let playlist = root.path().join("live.m3u8");
        let baseline = HashSet::from(["live000000.ts".to_owned()]);
        tokio::fs::write(&playlist, "#EXTM3U\n#EXT-X-TARGETDURATION:4\n#EXTINF:4,\nlive000000.ts\n#EXTINF:2.44,\nlive000001.ts\n#EXT-X-DISCONTINUITY\n#EXTINF:0.04,\nlive000002.ts\n").await.unwrap();
        assert_eq!(
            playlist_duration_excluding_uris(root.path(), &baseline)
                .await
                .unwrap(),
            Duration::from_millis(2480)
        );
        for invalid in ["0", "-1", "NaN", "inf", "bad"] {
            tokio::fs::write(
                &playlist,
                format!("#EXTM3U\n#EXT-X-TARGETDURATION:4\n#EXTINF:{invalid},\nlive000001.ts\n"),
            )
            .await
            .unwrap();
            assert!(
                playlist_duration_excluding_uris(root.path(), &baseline)
                    .await
                    .is_err(),
                "{invalid}"
            );
        }
    }

    #[test]
    fn mpegts_output_ts_offset_advances_by_successful_append_duration() {
        assert_eq!(
            advance_mpegts_output_ts_offset(OutputFormat::Hls, None, Duration::from_secs(4)),
            Some(Duration::from_secs(4))
        );
        assert_eq!(
            advance_mpegts_output_ts_offset(
                OutputFormat::Hls,
                Some(Duration::from_secs(4)),
                Duration::from_secs(8),
            ),
            Some(Duration::from_secs(12))
        );
        assert_eq!(
            advance_mpegts_output_ts_offset(
                OutputFormat::HlsMp4,
                Some(Duration::from_secs(4)),
                Duration::from_secs(8),
            ),
            Some(Duration::from_secs(4))
        );
    }

    #[tokio::test]
    async fn mpegts_output_ts_offset_uses_last_normalized_pcr() {
        let worker = HlsSessionWorker::new(
            "7".to_owned(),
            7,
            std::env::temp_dir().join(format!("ersatzrs-hls-test-{}", Uuid::new_v4())),
            OutputFormat::Hls,
            Vec::new(),
            test_ffmpeg_runtime(FfmpegCapabilityReport::default()),
        );
        {
            let mut continuity = worker.ts_continuity.lock().await;
            continuity
                .first_pcr
                .insert(0x0100, PCR_TICKS_PER_SECOND * 7_200);
            continuity
                .last_pcr
                .insert(0x0100, PCR_TICKS_PER_SECOND * 7_212);
        }

        assert_eq!(
            worker.mpegts_output_ts_offset().await,
            Some(Duration::from_secs(12))
        );
    }

    #[tokio::test]
    async fn mpegts_output_ts_offset_falls_back_to_playlist_duration_before_pcr() {
        let working_dir =
            std::env::temp_dir().join(format!("ersatzrs-hls-test-{}", Uuid::new_v4()));
        tokio::fs::create_dir_all(&working_dir)
            .await
            .expect("create temp dir");
        tokio::fs::write(
            working_dir.join("live.m3u8"),
            "#EXTM3U\n#EXT-X-TARGETDURATION:5\n#EXTINF:4.800000,\nlive000000.ts\n#EXTINF:2.400000,\nlive000001.ts\n",
        )
        .await
        .expect("write playlist");
        let worker = HlsSessionWorker::new(
            "7".to_owned(),
            7,
            working_dir.clone(),
            OutputFormat::Hls,
            Vec::new(),
            test_ffmpeg_runtime(FfmpegCapabilityReport::default()),
        );

        assert_eq!(
            worker.mpegts_output_ts_offset().await,
            Some(Duration::from_millis(7_200))
        );

        let _ = tokio::fs::remove_dir_all(&working_dir).await;
    }

    #[tokio::test]
    async fn remote_burst_duration_excludes_segments_from_before_the_spawn() {
        let working_dir =
            std::env::temp_dir().join(format!("ersatzrs-hls-test-{}", Uuid::new_v4()));
        tokio::fs::create_dir_all(&working_dir)
            .await
            .expect("create temp dir");
        tokio::fs::write(
            working_dir.join("live.m3u8"),
            "#EXTM3U\n#EXT-X-TARGETDURATION:4\n\
#EXTINF:4.000000,\nlive000000.ts\n\
#EXTINF:4.000000,\nlive000001.ts\n\
#EXTINF:4.000000,\nlive000002.ts\n\
#EXTINF:4.000000,\nlive000003.ts\n\
#EXTINF:4.000000,\nlive000004.ts\n\
#EXTINF:4.000000,\nlive000005.ts\n\
#EXTINF:4.000000,\nlive000006.ts\n\
#EXTINF:4.000000,\nlive000007.ts\n\
#EXTINF:4.000000,\nlive000008.ts\n",
        )
        .await
        .expect("write playlist");
        let excluded = HashSet::from(["live000000.ts".to_owned()]);

        assert_eq!(
            playlist_duration_excluding_uris(&working_dir, &excluded)
                .await
                .expect("calculate burst duration"),
            Duration::from_secs(32)
        );

        let _ = tokio::fs::remove_dir_all(&working_dir).await;
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
            Duration::ZERO,
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
        )
        .expect("the fixture source is playable");
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
            Duration::ZERO,
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
        )
        .expect("the fixture source is playable");
        let arguments = command.arguments.join(" ");
        assert!(arguments.contains("-output_ts_offset 12.5s"));
        assert!(!arguments.contains("discont_start"), "{arguments}");
        assert!(
            !arguments.contains("mpegts_flags=+initial_discontinuity"),
            "{arguments}"
        );
    }

    #[test]
    fn build_hls_command_can_preserve_real_discontinuity_with_output_ts_offset() {
        let command = build_hls_command_with_explicit_discontinuity(
            "/media/test.mkv",
            Path::new("/tmp/hls/1"),
            OutputFormat::Hls,
            false,
            false,
            false,
            Duration::ZERO,
            None,
            Some(Duration::from_millis(12_500)),
            &playback_profile(),
            None,
            OutputMetadata::default(),
            FfmpegCapabilityReport::default(),
            None,
            Vec::new(),
            PlaybackAudioSelection::Automatic,
            None,
            Vec::new(),
            Vec::new(),
            HlsDiscontinuity::Present,
            None,
            None,
        )
        .expect("the fixture source is playable");
        let arguments = command.arguments.join(" ");

        assert!(arguments.contains("-output_ts_offset 12.5s"));
        assert!(
            arguments.contains(
                "-hls_flags program_date_time+omit_endlist+append_list+discont_start+independent_segments"
            ),
            "{arguments}"
        );
        assert!(
            !arguments.contains("mpegts_flags=+initial_discontinuity"),
            "{arguments}"
        );
    }

    #[test]
    fn build_hls_bootstrap_readburst_preserves_both_audio_selections() {
        let mut profile = playback_profile();
        profile.profile.video_format = FFmpegProfileVideoFormat::Copy;
        profile.profile.audio_format = FFmpegProfileAudioFormat::Copy;
        let build = |audio_index, initial_bootstrap| {
            let (acceleration, mut request) = build_hls_request_with_input_explicit_discontinuity(
                InputFile::video("synthetic-bootstrap.mkv"),
                OutputFormat::Hls,
                true,
                false,
                true,
                Duration::ZERO,
                Some(Duration::from_secs(44)),
                None,
                &profile,
                None,
                OutputMetadata::default(),
                FfmpegCapabilityReport::default(),
                Some(SourceVideoStream::new(SourceVideoCodec::H264).with_index(7)),
                vec![
                    SourceAudioStream::new(SourceAudioCodec::Aac).with_index(1),
                    SourceAudioStream::new(SourceAudioCodec::Aac).with_index(5),
                ],
                PlaybackAudioSelection::Exact(audio_index),
                None,
                Vec::new(),
                Vec::new(),
                HlsDiscontinuity::Absent,
                None,
                HlsInputTimeline::Original,
                None,
            );
            apply_hls_bootstrap_readburst(&mut request, initial_bootstrap, false, 3);
            ersatzrs_ffmpeg::build_pipeline_command_with_source_teletext_plan(acceleration, request)
                .unwrap()
                .0
        };
        let mut snapshot = String::new();
        for selected_audio in [1, 5] {
            let ordinary = build(selected_audio, false);
            let command = build(selected_audio, true);
            let burst_position = command
                .arguments
                .iter()
                .position(|argument| argument == "-readrate_initial_burst")
                .unwrap();
            assert_eq!(
                &command.arguments[burst_position..burst_position + 6],
                [
                    "-readrate_initial_burst",
                    "12",
                    "-readrate_initial_burst",
                    "12",
                    "-readrate",
                    "1.0",
                ]
            );
            let mut without_burst = command.arguments.clone();
            while let Some(position) = without_burst
                .iter()
                .position(|argument| argument == "-readrate_initial_burst")
            {
                assert_eq!(without_burst[position + 1], "12");
                without_burst.drain(position..position + 2);
            }
            assert_eq!(
                without_burst, ordinary.arguments,
                "burst must not change maps, codecs, captions or timeline options"
            );
            assert!(command
                .arguments
                .windows(2)
                .any(|pair| pair[0] == "-map" && pair[1] == format!("0:{selected_audio}")));
            writeln!(
                snapshot,
                "selected audio {selected_audio}\n{}",
                command.arguments.join(" ")
            )
            .unwrap();
        }
        insta::assert_snapshot!("hls_bootstrap_readburst_audio_selection", snapshot);
    }

    #[test]
    fn hls_bootstrap_readburst_is_bounded_and_preserves_other_input_policies() {
        let request = || {
            let profile = playback_profile();
            PipelineRequest::new(
                InputFile::video("synthetic-bootstrap.mkv"),
                DesiredVideoState::from_profile(
                    &profile.profile,
                    &profile.resolution,
                    None,
                    None,
                    StreamingMode::HttpLiveStreamingSegmenter,
                ),
            )
        };
        for (segments, seconds) in [(3, 12), (5, 20), (usize::MAX, 44)] {
            let mut request = request();
            apply_hls_bootstrap_readburst(&mut request, true, false, segments);
            assert_eq!(
                request.state.realtime_initial_burst,
                Some(Duration::from_secs(seconds))
            );
            assert!(request
                .inputs
                .iter()
                .all(|input| input.input_options.is_empty()));
        }
        for (initial, retained, continuous) in [
            (false, false, false),
            (true, true, false),
            (true, false, true),
        ] {
            let mut request = request();
            request.state.retained_stream_timestamps = retained;
            apply_hls_bootstrap_readburst(&mut request, initial, continuous, 3);
            assert!(request.state.realtime_initial_burst.is_none());
        }
        let mut raw = request();
        raw.inputs[0]
            .input_options
            .extend(["-readrate_initial_burst".into(), "44".into()]);
        apply_hls_bootstrap_readburst(&mut raw, true, false, 3);
        assert!(raw.state.realtime_initial_burst.is_none());
        assert_eq!(
            raw.inputs[0].input_options,
            ["-readrate_initial_burst", "44"]
        );
        let mut typed = request();
        typed.state.realtime_initial_burst = Some(Duration::from_secs(20));
        apply_hls_bootstrap_readburst(&mut typed, true, false, 3);
        assert_eq!(
            typed.state.realtime_initial_burst,
            Some(Duration::from_secs(20))
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
            Duration::ZERO,
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
        )
        .expect("the fixture source is playable");
        let arguments = command.arguments.join(" ");

        assert!(arguments.contains("-readrate 1.05 -i /media/test.mkv"));
    }

    #[test]
    fn build_hls_command_paces_continuous_remote_after_initial_burst() {
        let input = InputFile::video("pipe:0")
            .with_seek_after_input()
            .with_input_options([
                "-readrate_initial_burst".to_owned(),
                HLS_REMOTE_INITIAL_BURST_SECONDS.to_string(),
            ]);
        let (command, _) = build_hls_command_with_input_explicit_discontinuity(
            input,
            OutputFormat::Hls,
            true,
            false,
            true,
            Duration::ZERO,
            Some(Duration::from_mins(30)),
            None,
            &playback_profile(),
            None,
            OutputMetadata::default(),
            FfmpegCapabilityReport::default(),
            None,
            Vec::new(),
            PlaybackAudioSelection::Automatic,
            None,
            Vec::new(),
            Vec::new(),
            HlsDiscontinuity::Absent,
            None,
            HlsInputTimeline::Original,
            None,
        )
        .expect("the remote fixture source is playable");
        let arguments = command.arguments.join(" ");

        assert!(
            arguments.contains("-readrate_initial_burst 44 -readrate 1.05 -i pipe:0"),
            "{arguments}"
        );
        assert!(arguments.contains("-t 00:30:00.0000000"), "{arguments}");
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
            Duration::ZERO,
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
        )
        .expect("the fixture source is playable");
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
    fn build_hls_slug_command_keeps_real_discontinuity_with_pts_offset() {
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
        assert!(arguments.contains(
            "-hls_flags program_date_time+omit_endlist+append_list+discont_start+independent_segments"
        ));
        assert!(!arguments.contains("mpegts_flags=+initial_discontinuity"));
    }

    #[test]
    fn generated_error_video_stream_matches_embedded_background_dimensions() {
        // PNG's first chunk is IHDR; its first eight data bytes are the actual
        // image width and height. Keep source metadata tied to the shipped asset.
        assert_eq!(&ERROR_BACKGROUND_PNG[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(&ERROR_BACKGROUND_PNG[12..16], b"IHDR");
        let dimensions = FrameSize::new(
            u32::from_be_bytes(ERROR_BACKGROUND_PNG[16..20].try_into().unwrap()),
            u32::from_be_bytes(ERROR_BACKGROUND_PNG[20..24].try_into().unwrap()),
        );
        assert_eq!(dimensions, ERROR_BACKGROUND_FRAME_SIZE);
        let source = generated_error_video_stream(FrameRate::new(25, 1));
        assert_eq!(source.frame_size, Some(dimensions));
        assert!(source.is_still_image);
    }

    fn slate_snapshot_arguments(command: &FfmpegCommand) -> String {
        // Redact only the machine-local font cache; preserve the complete final
        // argv order, scaling graph and subtitle filter in both output paths.
        let original =
            ersatzrs_ffmpeg::subtitles_filter("synthetic.ass", Some(&subtitle_fonts_dir()));
        let replacement =
            ersatzrs_ffmpeg::subtitles_filter("synthetic.ass", Some("synthetic-fonts"));
        command.arguments.join(" ").replace(&original, &replacement)
    }

    #[test]
    fn build_hls_error_command_scales_embedded_background_to_profile_resolution() {
        let mut profile = playback_profile();
        profile.resolution.width = 492;
        profile.resolution.height = 250;
        let command = build_hls_error_command_from_assets(HlsErrorCommandRequest {
            background_path: Path::new("synthetic-background.png"),
            subtitle_path: Path::new("synthetic.ass"),
            output_format: OutputFormat::Hls,
            is_first_transcode: true,
            hls_realtime: true,
            duration: Duration::from_secs(4),
            output_ts_offset: None,
            playback_profile: &profile,
            target_frame_rate: Some(FrameRate::new(25, 1)),
            metadata: OutputMetadata::default(),
            runtime_capabilities: FfmpegCapabilityReport::default(),
        });
        let arguments = slate_snapshot_arguments(&command);
        assert!(arguments.contains("scale=492:250:"), "{arguments}");
        insta::assert_snapshot!(arguments);
    }

    #[test]
    fn build_mpeg_ts_slate_command_scales_embedded_background_to_profile_resolution() {
        let mut profile = playback_profile();
        profile.resolution.width = 492;
        profile.resolution.height = 250;
        let command = build_mpeg_ts_slate_command_from_assets(MpegTsSlateCommandAssetRequest {
            background_path: Path::new("synthetic-background.png"),
            subtitle_path: Path::new("synthetic.ass"),
            duration: Duration::from_secs(4),
            playback_profile: &profile,
            target_frame_rate: Some(FrameRate::new(25, 1)),
            metadata: OutputMetadata::default(),
            runtime_capabilities: FfmpegCapabilityReport::default(),
        });
        let arguments = slate_snapshot_arguments(&command);
        assert!(arguments.contains("scale=492:250:"), "{arguments}");
        insta::assert_snapshot!(arguments);
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

    #[test]
    fn build_mpeg_ts_slate_command_uses_background_subtitle_and_null_audio() {
        let command = build_mpeg_ts_slate_command_from_assets(MpegTsSlateCommandAssetRequest {
            background_path: Path::new("/tmp/mpegts/slate/background.png"),
            subtitle_path: Path::new("/tmp/mpegts/slate/slate.ass"),
            duration: Duration::from_secs(4),
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

        assert!(arguments.contains("-loop 1 -i /tmp/mpegts/slate/background.png"));
        assert!(
            arguments.contains("anullsrc=channel_layout=stereo:sample_rate=48000"),
            "{arguments}"
        );
        assert!(
            arguments.contains("subtitles=/tmp/mpegts/slate/slate.ass"),
            "{arguments}"
        );
        assert!(arguments.contains("-t 00:00:04.0000000"), "{arguments}");
        assert!(
            arguments.contains("-metadata service_provider=\"ErsatzTV\""),
            "{arguments}"
        );
        assert!(
            arguments.contains("-metadata service_name=\"Channel One\""),
            "{arguments}"
        );
        assert!(arguments.contains("-f mpegts"), "{arguments}");
        assert!(arguments.contains("pipe:1"), "{arguments}");
        assert!(arguments.contains("-readrate 1.05"), "{arguments}");
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

    #[tokio::test]
    async fn build_mpeg_ts_slate_command_writes_generated_assets() {
        let working_dir = tempfile::tempdir().expect("create mpeg-ts slate temp dir");
        let command = build_mpeg_ts_slate_command(MpegTsSlateCommandRequest {
            working_dir: working_dir.path(),
            duration: Duration::from_secs(2),
            playback_profile: &playback_profile(),
            target_frame_rate: None,
            metadata: OutputMetadata::default(),
            runtime_capabilities: FfmpegCapabilityReport::default(),
            message: "all streams\nin use",
        })
        .await
        .expect("mpeg-ts slate command");

        let subtitle = tokio::fs::read_to_string(working_dir.path().join("slate.ass"))
            .await
            .expect("read generated slate subtitle");

        assert!(working_dir.path().join("background.png").exists());
        assert!(subtitle.contains(r"all streams\Nin use"));
        let arguments = command.arguments.join(" ");
        assert!(arguments.contains("-f mpegts"), "{arguments}");
        assert!(arguments.contains("pipe:1"), "{arguments}");
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
            Duration::ZERO,
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
    fn pinned_silent_hls_command_has_no_audio_mapping_or_encoder() {
        let command = build_hls_command(
            "/media/test.mkv",
            Path::new("/tmp/hls/1"),
            OutputFormat::Hls,
            true,
            true,
            false,
            Duration::ZERO,
            None,
            None,
            &playback_profile(),
            None,
            OutputMetadata::default(),
            FfmpegCapabilityReport::default(),
            None,
            vec![SourceAudioStream::new(SourceAudioCodec::Aac).with_index(1)],
            PlaybackAudioSelection::Disabled,
            None,
            Vec::new(),
            Vec::new(),
        )
        .expect("the fixture source is playable");
        let arguments = command.arguments.join(" ");

        assert!(!arguments.contains("-map 0:1"), "{arguments}");
        assert!(!arguments.contains("-map 0:a"), "{arguments}");
        assert!(!arguments.contains("-codec:a"), "{arguments}");
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
            Duration::ZERO,
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
        )
        .expect("the fixture source is playable");
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
            Duration::ZERO,
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
        )
        .expect("the fixture source is playable");
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
            Duration::from_mins(2),
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
        )
        .expect("the fixture source is playable");
        let arguments = command.arguments.join(" ");

        assert!(
            arguments.contains("-ss 00:02:00.0000000 -c:a dts -core_only true -i /media/test.mkv"),
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
            Duration::ZERO,
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
        )
        .expect("the fixture source is playable");
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
            Duration::from_mins(15),
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
        )
        .expect("the fixture source is playable");

        assert!(command
            .arguments
            .join(" ")
            .contains("-ss 00:15:00.0000000 -i /media/test.mkv"));
    }

    #[test]
    fn build_hls_command_preserves_fractional_playout_seek() {
        let command = build_hls_command(
            "/media/test.mkv",
            Path::new("/tmp/hls/1"),
            OutputFormat::Hls,
            true,
            false,
            false,
            Duration::from_millis(517),
            Some(Duration::from_millis(43_483)),
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
        )
        .expect("the fixture source is playable");

        let arguments = command.arguments.join(" ");
        assert!(
            arguments.contains("-ss 00:00:00.5170000 -i /media/test.mkv"),
            "{arguments}"
        );
        assert!(arguments.contains("-t 00:00:43.4830000"), "{arguments}");
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
            Duration::ZERO,
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
        )
        .expect("the fixture source is playable");
        let arguments = command.arguments.join(" ");

        assert!(arguments.contains("-filter_complex"));
        assert!(
            arguments.contains("drawvg=file=/tmp/hls/1/graphics/item-1/title.vgs"),
            "{arguments}"
        );
    }

    #[test]
    fn playout_start_duration_matches_mid_item_offset() {
        let now = DateTime::parse_from_rfc3339("2026-05-31T21:45:00.517+00:00")
            .expect("valid timestamp")
            .with_timezone(&Utc);
        let start = DateTime::parse_from_rfc3339("2026-05-31T21:30:00.000+00:00")
            .expect("valid timestamp")
            .with_timezone(&Utc);

        assert_eq!(
            playout_start_duration(now, start),
            Duration::from_millis(900_517)
        );
        assert_eq!(playout_start_duration(start, now), Duration::ZERO);
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
            DateTime::parse_from_rfc3339("2026-04-20T20:00:00.000+00:00")
                .expect("valid timestamp")
                .with_timezone(&Utc),
            DateTime::parse_from_rfc3339("2026-04-20T20:00:03.000+00:00")
                .expect("valid timestamp")
                .with_timezone(&Utc),
            None,
            false,
            None,
        )
        .expect("trimmed playlist");

        assert!(!trimmed.playlist.contains("seg00001.ts"));
        assert!(!trimmed.playlist.contains("#EXT-X-DISCONTINUITY"));
        assert!(trimmed.playlist.contains("seg00002.ts"));
        assert!(trimmed.playlist.contains("seg00003.ts"));
        assert_eq!(trimmed.referenced_files.len(), 2);
    }

    #[test]
    fn trim_playlist_contents_keeps_stale_segments_when_none_are_recent() {
        let playlist = concat!(
            "#EXTM3U\n",
            "#EXT-X-VERSION:7\n",
            "#EXT-X-TARGETDURATION:4\n",
            "#EXT-X-MEDIA-SEQUENCE:1137\n",
            "#EXT-X-INDEPENDENT-SEGMENTS\n",
            "#EXTINF:4.000000,\n",
            "live001137.ts\n",
            "#EXTINF:4.000000,\n",
            "live001138.ts\n"
        );
        let start = DateTime::parse_from_rfc3339("2021-10-09T13:00:00Z")
            .expect("valid timestamp")
            .with_timezone(&Utc);

        let trimmed = trim_playlist_contents(
            OutputFormat::Hls,
            playlist,
            start,
            start + chrono::Duration::seconds(60),
            None,
            true,
            None,
        )
        .expect("stale segments still form a playable playlist");

        assert_eq!(trimmed.sequence, 1137);
        assert_eq!(trimmed.playlist_start, start);
        assert_eq!(trimmed.playlist.matches("#EXTINF:").count(), 2);
        assert!(trimmed.playlist.contains("#EXT-X-MEDIA-SEQUENCE:1137"));
        assert!(trimmed.referenced_files.contains("live001137.ts"));
        assert!(trimmed.referenced_files.contains("live001138.ts"));
    }

    #[tokio::test]
    async fn trim_playlist_contents_keeps_stale_fmp4_segments_and_files() {
        let working_dir = tempfile::tempdir().expect("create temp dir");
        let generation = 1_760_874_038_i64;
        let init_file = format!("{generation}_init.mp4");
        let stale_file = format!("live_{generation}_001136.m4s");
        let segments = (1137..=1139)
            .map(|sequence| format!("live_{generation}_{sequence:06}.m4s"))
            .collect::<Vec<_>>();
        for file in std::iter::once(&init_file)
            .chain(std::iter::once(&stale_file))
            .chain(segments.iter())
        {
            tokio::fs::write(working_dir.path().join(file), b"segment")
                .await
                .expect("write HLS artifact");
        }
        let playlist = format!(
            "#EXTM3U\n#EXT-X-VERSION:7\n#EXT-X-TARGETDURATION:4\n\
             #EXT-X-MEDIA-SEQUENCE:1137\n#EXT-X-INDEPENDENT-SEGMENTS\n\
             #EXT-X-DISCONTINUITY\n#EXT-X-MAP:URI=\"{init_file}\"\n\
             #EXTINF:4.000000,\n{}\n#EXTINF:4.000000,\n{}\n\
             #EXTINF:4.000000,\n{}\n",
            segments[0], segments[1], segments[2]
        );
        let start = DateTime::parse_from_rfc3339("2021-10-09T13:00:00Z")
            .expect("valid timestamp")
            .with_timezone(&Utc);

        let trimmed = trim_playlist_contents(
            OutputFormat::HlsMp4,
            &playlist,
            start,
            start + chrono::Duration::seconds(60),
            None,
            true,
            None,
        )
        .expect("stale fMP4 segments still form a playable playlist");

        assert_eq!(trimmed.sequence, 1137);
        assert_eq!(trimmed.playlist_start, start);
        assert_eq!(trimmed.generated_at, Some(generation));
        assert_eq!(trimmed.playlist.matches("#EXTINF:").count(), 3);
        assert!(trimmed
            .playlist
            .contains(&format!("#EXT-X-MAP:URI=\"{init_file}\"")));
        assert!(trimmed.referenced_files.contains(&init_file));
        for segment in &segments {
            assert!(trimmed.referenced_files.contains(segment));
        }

        delete_old_segments(
            working_dir.path(),
            &trimmed,
            &mut HlsInitSegmentCache::default(),
        )
        .await
        .expect("delete only unreferenced segments");
        assert!(!working_dir.path().join(stale_file).exists());
        assert!(working_dir.path().join(init_file).exists());
        for segment in &segments {
            assert!(working_dir.path().join(segment).exists());
        }
    }

    #[test]
    fn trim_playlist_contents_rewrites_program_date_time_across_discontinuity() {
        let playlist = concat!(
            "#EXTM3U\n",
            "#EXT-X-VERSION:6\n",
            "#EXT-X-TARGETDURATION:5\n",
            "#EXT-X-MEDIA-SEQUENCE:41\n",
            "#EXT-X-DISCONTINUITY-SEQUENCE:3\n",
            "#EXT-X-INDEPENDENT-SEGMENTS\n",
            "#EXTINF:4.800000,\n",
            "#EXT-X-PROGRAM-DATE-TIME:2026-06-13T10:34:12.387+0200\n",
            "live000041.ts\n",
            "#EXTINF:0.320000,\n",
            "#EXT-X-PROGRAM-DATE-TIME:2026-06-13T10:34:17.187+0200\n",
            "live000042.ts\n",
            "#EXT-X-DISCONTINUITY\n",
            "#EXTINF:4.800000,\n",
            "#EXT-X-PROGRAM-DATE-TIME:2026-06-13T10:34:11.869+0200\n",
            "live000043.ts\n"
        );
        let playlist_start = DateTime::parse_from_rfc3339("2026-06-13T08:34:12.387+00:00")
            .expect("valid timestamp")
            .with_timezone(&Utc);

        let trimmed = trim_playlist_contents(
            OutputFormat::Hls,
            playlist,
            playlist_start,
            playlist_start - chrono::Duration::seconds(1),
            Some(10),
            false,
            None,
        )
        .expect("trimmed playlist");

        assert!(trimmed
            .playlist
            .contains("#EXT-X-PROGRAM-DATE-TIME:2026-06-13T08:34:12.387+0000\nlive000041.ts"));
        assert!(trimmed
            .playlist
            .contains("#EXT-X-PROGRAM-DATE-TIME:2026-06-13T08:34:17.187+0000\nlive000042.ts"));
        assert!(trimmed
            .playlist
            .contains("#EXT-X-PROGRAM-DATE-TIME:2026-06-13T08:34:17.507+0000\nlive000043.ts"));
        assert!(!trimmed.playlist.contains("#EXT-X-DISCONTINUITY"));
        assert!(!trimmed
            .playlist
            .contains("#EXT-X-PROGRAM-DATE-TIME:2026-06-13T10:34:11.869+0200"));
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
            DateTime::parse_from_rfc3339("2026-04-20T20:00:00.000+00:00")
                .expect("valid timestamp")
                .with_timezone(&Utc),
            DateTime::parse_from_rfc3339("2026-04-20T20:13:00.000+00:00")
                .expect("valid timestamp")
                .with_timezone(&Utc),
            Some(10),
            false,
            None,
        )
        .expect("trimmed playlist");

        assert!(!trimmed.playlist.contains("seg00001.ts"));
        assert!(!trimmed.playlist.contains("seg00002.ts"));
        assert!(trimmed.playlist.contains("seg00003.ts"));
        assert!(trimmed.playlist.contains("seg00012.ts"));
        assert_eq!(trimmed.referenced_files.len(), 10);
    }

    #[tokio::test]
    async fn normalize_hls_ts_continuity_rewrites_restart_counters() {
        let working_dir =
            std::env::temp_dir().join(format!("ersatzrs-hls-test-{}", Uuid::new_v4()));
        tokio::fs::create_dir_all(&working_dir)
            .await
            .expect("create temp dir");

        tokio::fs::write(
            working_dir.join("live000010.ts"),
            [
                ts_packet(0, 4),
                ts_packet(0, 5),
                ts_packet(0x0100, 8),
                ts_packet(0x0100, 9),
            ]
            .concat(),
        )
        .await
        .expect("write first segment");
        tokio::fs::write(
            working_dir.join("live000011.ts"),
            [
                ts_packet(0, 0),
                ts_packet(0, 1),
                ts_packet(0x0100, 0),
                ts_packet(0x0100, 1),
            ]
            .concat(),
        )
        .await
        .expect("write second segment");

        let trim_result = test_trim_result(10, None, ["live000010.ts", "live000011.ts"]);
        let mut state = TsContinuityState::default();
        normalize_hls_ts_continuity(&working_dir, &trim_result, &mut state)
            .await
            .expect("normalize continuity");

        let second = tokio::fs::read(working_dir.join("live000011.ts"))
            .await
            .expect("read second segment");
        assert_eq!(packet_continuity_counters(&second, 0), vec![6, 7]);
        assert_eq!(packet_continuity_counters(&second, 0x0100), vec![10, 11]);

        let _ = tokio::fs::remove_dir_all(&working_dir).await;
    }

    #[tokio::test]
    async fn normalize_hls_ts_continuity_preserves_adaptation_only_counter_rule() {
        let working_dir =
            std::env::temp_dir().join(format!("ersatzrs-hls-test-{}", Uuid::new_v4()));
        tokio::fs::create_dir_all(&working_dir)
            .await
            .expect("create temp dir");

        tokio::fs::write(
            working_dir.join("live000010.ts"),
            [
                ts_packet(0x0100, 4),
                ts_adaptation_only_packet(0x0100, 9),
                ts_packet(0x0100, 0),
            ]
            .concat(),
        )
        .await
        .expect("write first segment");
        tokio::fs::write(
            working_dir.join("live000011.ts"),
            [ts_adaptation_only_packet(0x0100, 0), ts_packet(0x0100, 0)].concat(),
        )
        .await
        .expect("write second segment");

        let trim_result = test_trim_result(10, None, ["live000010.ts", "live000011.ts"]);
        let mut state = TsContinuityState::default();
        normalize_hls_ts_continuity(&working_dir, &trim_result, &mut state)
            .await
            .expect("normalize continuity");

        let first = tokio::fs::read(working_dir.join("live000010.ts"))
            .await
            .expect("read first segment");
        let second = tokio::fs::read(working_dir.join("live000011.ts"))
            .await
            .expect("read second segment");
        assert_eq!(packet_continuity_counters(&first, 0x0100), vec![4, 4, 5]);
        assert_eq!(packet_continuity_counters(&second, 0x0100), vec![5, 6]);

        let _ = tokio::fs::remove_dir_all(&working_dir).await;
    }

    #[tokio::test]
    async fn normalize_hls_ts_continuity_rewrites_restart_pcr() {
        let working_dir =
            std::env::temp_dir().join(format!("ersatzrs-hls-test-{}", Uuid::new_v4()));
        tokio::fs::create_dir_all(&working_dir)
            .await
            .expect("create temp dir");

        tokio::fs::write(
            working_dir.join("live000010.ts"),
            [
                ts_packet_with_pcr(0x0100, 8, 27_000_000),
                ts_packet_with_pcr(0x0100, 9, 27_090_000),
            ]
            .concat(),
        )
        .await
        .expect("write first segment");
        tokio::fs::write(
            working_dir.join("live000011.ts"),
            [
                ts_packet_with_pcr(0x0100, 0, 1_000),
                ts_packet_with_pcr(0x0100, 1, 91_000),
            ]
            .concat(),
        )
        .await
        .expect("write second segment");

        let trim_result = test_trim_result(10, None, ["live000010.ts", "live000011.ts"]);
        let mut state = TsContinuityState::default();
        normalize_hls_ts_continuity(&working_dir, &trim_result, &mut state)
            .await
            .expect("normalize continuity");

        let first = tokio::fs::read(working_dir.join("live000010.ts"))
            .await
            .expect("read first segment");
        let second = tokio::fs::read(working_dir.join("live000011.ts"))
            .await
            .expect("read second segment");
        assert_eq!(
            packet_pcr_values(&first, 0x0100),
            vec![27_000_000, 27_090_000]
        );
        assert_eq!(
            packet_pcr_values(&second, 0x0100),
            vec![27_090_001, 27_180_001]
        );
        assert_eq!(packet_continuity_counters(&second, 0x0100), vec![10, 11]);

        let _ = tokio::fs::remove_dir_all(&working_dir).await;
    }

    #[tokio::test]
    async fn normalize_hls_ts_continuity_skips_missing_referenced_segments() {
        let working_dir =
            std::env::temp_dir().join(format!("ersatzrs-hls-test-{}", Uuid::new_v4()));
        tokio::fs::create_dir_all(&working_dir)
            .await
            .expect("create temp dir");

        let trim_result = test_trim_result(12, None, ["live000012.ts"]);
        let mut state = TsContinuityState::default();
        normalize_hls_ts_continuity(&working_dir, &trim_result, &mut state)
            .await
            .expect("missing segment is a normal playlist race");

        assert!(!state.normalized_sequences.contains(&12));

        let _ = tokio::fs::remove_dir_all(&working_dir).await;
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
            Duration::ZERO,
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
        )
        .expect("the fixture source is playable");

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
            Duration::ZERO,
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
        )
        .expect("the fixture source is playable");

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

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ignore = "requires ffmpeg on PATH"]
    async fn hls_segmenter_live_proves_opacity_expression_clock_bindings() {
        let ffmpeg = ersatzrs_ffmpeg::ffmpeg_path();
        if !ffmpeg_is_available().await {
            eprintln!("skipping: ffmpeg not on PATH");
            return;
        }

        let temp = tempfile::tempdir().expect("tempdir");
        let input = temp.path().join("black.mp4");
        let logo = temp.path().join("logo.ppm");
        let working_dir = temp.path().join("hls");
        tokio::fs::create_dir_all(&working_dir)
            .await
            .expect("create hls dir");
        generate_hls_opacity_input(&ffmpeg, &input).await;
        std::fs::write(&logo, red_ppm(64, 32)).expect("write watermark fixture");

        let clock = WatermarkOpacityClock::new(86_399.5, 5.0, Some(12.0), 30.0);
        let expression = concat!(
            "if(content_seconds >= 5.5 && content_seconds < content_total_seconds && ",
            "channel_seconds >= 30.5 && channel_seconds < 31.5 && ",
            "time_of_day_seconds < 1, 1, 0)"
        );
        let command = build_hls_command_with_opacity_clock(
            input.to_string_lossy().as_ref(),
            &working_dir,
            OutputFormat::Hls,
            true,
            false,
            false,
            Duration::from_secs(5),
            Some(Duration::from_secs(3)),
            None,
            &playback_profile(),
            None,
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
        )
        .expect("the fixture source is playable");
        let arguments = command.arguments.join(" ");
        eprintln!("HLS opacity clock proof argv: {arguments}");
        assert!(arguments.contains("format=yuva420p|yuva444p|yuva422p,geq="));
        assert!(arguments.contains("(5+T)"), "{arguments}");
        assert!(arguments.contains("(30+T)"), "{arguments}");
        assert!(arguments.contains("mod((86399.5+T),86400)"), "{arguments}");
        assert!(arguments.contains("lt((5+T),12)"), "{arguments}");

        run_hls_command(command, &working_dir).await;
        let playlist = working_dir.join("live.m3u8");
        let segment = working_dir.join("live000000.ts");
        assert!(
            playlist.is_file(),
            "expected playlist at {}",
            playlist.display()
        );
        assert!(
            segment.is_file(),
            "expected first segment at {}",
            segment.display()
        );
        let samples = decode_opacity_samples(&ffmpeg, &segment).await;
        assert_black(samples[0], "before midnight wrap and threshold");
        assert_red(samples[1], "after content/channel/time thresholds");
        assert_black(samples[2], "after one-second clock window");
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
            Duration::ZERO,
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
        )
        .expect("the fixture source is playable");

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

    async fn generate_hls_opacity_input(ffmpeg: &str, input: &Path) {
        let mut command = Command::new(ffmpeg);
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

    async fn run_hls_command(command: FfmpegCommand, working_dir: &Path) {
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
        apply_ffmpeg_library_path(&mut process);
        let output = timeout(Duration::from_secs(30), process.output())
            .await
            .expect("timed out running hls opacity segmenter")
            .expect("run hls opacity segmenter");
        assert!(
            output.status.success(),
            "ffmpeg hls failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    async fn decode_opacity_samples(ffmpeg: &str, stream: &Path) -> Vec<[u8; 3]> {
        let mut command = Command::new(ffmpeg);
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

    fn red_ppm(width: usize, height: usize) -> Vec<u8> {
        let mut ppm = format!("P6\n{width} {height}\n255\n").into_bytes();
        ppm.extend(std::iter::repeat_n([255, 0, 0], width * height).flatten());
        ppm
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
            original_media_path: "/media/test.mkv".to_owned(),
            media_path: "/media/test.mkv".to_owned(),
            playout_item_id: 1,
            media_item_id: 1,
            media_version_id: 1,
            media_file_id: None,
            content_duration: (playout_finish - playout_start).to_std().ok(),
            in_point: Duration::ZERO,
            program_duration: (playout_finish - playout_start).to_std().ok(),
            playout_start,
            playout_finish: Some(playout_finish),
            disable_watermarks: false,
            preferred_audio_language_code: None,
            preferred_audio_title: None,
            preferred_subtitle_language_code: None,
            subtitle_mode: None,
            has_pinned_stream_selection: false,
            video_stream_index: None,
            audio_stream_index: None,
            subtitle_stream_index: None,
            channel_name: "Channel One".to_owned(),
            channel_preferred_audio_language_code: None,
            channel_preferred_audio_title: None,
            channel_stream_selector_mode: ChannelStreamSelectorMode::Default,
            channel_stream_selector: None,
            channel_preferred_subtitle_language_code: None,
            channel_subtitle_mode: ChannelSubtitleMode::None,
            channel_watermark_id: None,
            channel_music_video_credits_mode: ChannelMusicVideoCreditsMode::None,
            channel_music_video_credits_template: None,
            remote_stream: None,
            external: None,
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
        let workspace_root = test_manifest_dir!()
            .parent()
            .and_then(Path::parent)
            .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
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
            DateTime::parse_from_rfc3339("2026-04-20T20:01:20.000+00:00")
                .expect("valid timestamp")
                .with_timezone(&Utc),
            DateTime::parse_from_rfc3339("2026-04-20T20:00:30.000+00:00")
                .expect("valid timestamp")
                .with_timezone(&Utc),
            Some(10),
            false,
            None,
        )
        .expect("trimmed playlist");

        assert!(trimmed.playlist.contains("#EXT-X-MAP:URI=\"init.mp4\""));
        assert!(trimmed.referenced_files.contains("init.mp4"));
        assert!(trimmed.referenced_files.contains("seg00003.m4s"));
    }

    #[tokio::test]
    async fn init_segment_cache_tracks_earliest_generation_across_reverse_insertion() {
        let working_dir = tempfile::tempdir().expect("create temp dir");
        let earlier = working_dir.path().join("1760874038_init.mp4");
        let later = working_dir.path().join("1760874042_init.mp4");
        tokio::fs::write(&earlier, b"same-init")
            .await
            .expect("write earlier init");
        tokio::fs::write(&later, b"same-init")
            .await
            .expect("write later init");

        let mut init_cache = HlsInitSegmentCache::default();
        init_cache
            .add_segment(&later)
            .await
            .expect("cache later init first");
        init_cache
            .add_segment(&earlier)
            .await
            .expect("cache earlier init second");

        assert_eq!(
            init_cache
                .earliest_segment_by_generated_at(1_760_874_042)
                .as_deref(),
            Some("1760874038_init.mp4")
        );
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
            DateTime::parse_from_rfc3339("2026-04-20T20:01:00.000+00:00")
                .expect("valid timestamp")
                .with_timezone(&Utc),
            Some(10),
            false,
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
            DateTime::parse_from_rfc3339("2026-04-20T20:00:40.000+00:00")
                .expect("valid timestamp")
                .with_timezone(&Utc),
            DateTime::parse_from_rfc3339("2026-04-20T20:01:00.000+00:00")
                .expect("valid timestamp")
                .with_timezone(&Utc),
            Some(2),
            false,
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
    async fn last_hls_segment_prefers_completed_playlist_segment() {
        let working_dir =
            std::env::temp_dir().join(format!("ersatzrs-hls-test-{}", Uuid::new_v4()));
        tokio::fs::create_dir_all(&working_dir)
            .await
            .expect("create temp dir");
        tokio::fs::write(working_dir.join("live000010.ts"), b"complete")
            .await
            .expect("write completed segment");
        tokio::fs::write(working_dir.join("live000011.ts"), b"in-progress")
            .await
            .expect("write newer in-progress segment");
        tokio::fs::write(
            working_dir.join("live.m3u8"),
            concat!(
                "#EXTM3U\n",
                "#EXT-X-TARGETDURATION:4\n",
                "#EXTINF:4.000000,\n",
                "live000010.ts\n"
            ),
        )
        .await
        .expect("write playlist");

        let segment = last_hls_segment(&working_dir)
            .await
            .expect("find segment")
            .expect("segment");

        assert_eq!(segment, working_dir.join("live000010.ts"));

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
            playlist_start: DateTime::parse_from_rfc3339("2026-04-20T20:00:00.000+00:00")
                .expect("valid timestamp")
                .with_timezone(&Utc),
        }
    }

    fn ts_packet(pid: u16, continuity_counter: u8) -> Vec<u8> {
        let mut packet = vec![0xff; 188];
        let [pid_hi, pid_lo] = pid.to_be_bytes();
        packet[0] = 0x47;
        packet[1] = 0x40 | (pid_hi & 0x1f);
        packet[2] = pid_lo;
        packet[3] = 0x10 | (continuity_counter & 0x0f);
        packet
    }

    fn ts_packet_with_pcr(pid: u16, continuity_counter: u8, pcr: u64) -> Vec<u8> {
        let mut packet = ts_packet(pid, continuity_counter);
        packet[3] = 0x30 | (continuity_counter & 0x0f);
        packet[4] = 7;
        packet[5] = 0x10;
        write_packet_pcr(&mut packet, pcr);
        packet
    }

    fn ts_adaptation_only_packet(pid: u16, continuity_counter: u8) -> Vec<u8> {
        let mut packet = ts_packet(pid, continuity_counter);
        packet[3] = 0x20 | (continuity_counter & 0x0f);
        packet[4] = 183;
        packet[5] = 0;
        packet
    }

    fn packet_continuity_counters(body: &[u8], pid: u16) -> Vec<u8> {
        body.chunks_exact(188)
            .filter(|packet| {
                let packet_pid = (u16::from(packet[1] & 0x1f) << 8) | u16::from(packet[2]);
                packet_pid == pid
            })
            .map(|packet| packet[3] & 0x0f)
            .collect()
    }

    fn packet_pcr_values(body: &[u8], pid: u16) -> Vec<u64> {
        body.chunks_exact(188)
            .filter(|packet| {
                let packet_pid = (u16::from(packet[1] & 0x1f) << 8) | u16::from(packet[2]);
                packet_pid == pid
            })
            .filter_map(|packet| packet_pcr(packet, true))
            .collect()
    }

    fn dolby_vision_source_320p(profile: u8, compatibility: u8) -> SourceVideoStream {
        let mut source = source_320p();
        source.codec = ersatzrs_ffmpeg::SourceVideoCodec::Hevc;
        if profile == 7 {
            source.profile = Some("Main 10".to_owned());
            source.pixel_format = Some(ersatzrs_ffmpeg::PixelFormat::Yuv420P10Le);
        }
        source.dolby_vision = Some(ersatzrs_ffmpeg::DolbyVisionConfiguration {
            profile,
            bl_signal_compatibility_id: compatibility,
            rpu_present: true,
            el_present: profile == 7,
            bl_present: true,
        });
        source
    }

    fn hls_command_for(
        source: SourceVideoStream,
    ) -> Result<FfmpegCommand, ersatzrs_ffmpeg::UnsupportedSource> {
        hls_command_for_with_capabilities(source, FfmpegCapabilityReport::default())
    }

    fn hls_command_for_with_capabilities(
        source: SourceVideoStream,
        runtime_capabilities: FfmpegCapabilityReport,
    ) -> Result<FfmpegCommand, ersatzrs_ffmpeg::UnsupportedSource> {
        build_hls_command(
            "/media/dv.mkv",
            Path::new("/tmp/ersatzrs-hls-test"),
            OutputFormat::Hls,
            true,
            false,
            false,
            Duration::ZERO,
            Some(Duration::from_secs(10)),
            None,
            &playback_profile(),
            None,
            OutputMetadata::default(),
            runtime_capabilities,
            Some(source),
            Vec::new(),
            None,
            None,
            Vec::new(),
            Vec::new(),
        )
    }

    /// ADR 0064 at the HLS segmenter command boundary.
    ///
    /// Asserted here rather than at the worker loop because this is where the
    /// refusal is enforced: a refused source produces no command, so the worker
    /// has nothing to spawn.
    #[test]
    fn the_hls_builder_refuses_unplayable_dolby_vision_before_any_spawn() {
        for (profile, compatibility) in [(5_u8, 0_u8), (7, 0), (9, 1)] {
            let error = hls_command_for(dolby_vision_source_320p(profile, compatibility))
                .expect_err("an unplayable Dolby Vision profile must be refused");
            assert!(
                !error.reason.is_empty(),
                "profile {profile} must carry an operator-facing reason"
            );
        }
    }

    #[test]
    fn refused_hls_source_enters_error_slate_path_and_advances_the_cursor() {
        let error =
            hls_command_for(dolby_vision_source_320p(5, 0)).expect_err("profile 5 must be refused");
        assert!(matches!(
            refused_hls_process_outcome(&error),
            HlsProcessOutcome::Failed(_)
        ));

        let request_time = DateTime::parse_from_rfc3339("2026-08-08T12:00:00Z")
            .expect("timestamp")
            .with_timezone(&Utc);
        let duration = Duration::from_secs(10);
        let (cursor, state, output_offset) = advance_after_hls_error_slate(
            request_time,
            duration,
            HlsSessionState::ZeroAndWorkAhead,
            false,
            OutputFormat::Hls,
            Some(Duration::from_secs(20)),
        );

        assert_eq!(cursor, add_std_duration(request_time, duration));
        assert_eq!(state, HlsSessionState::SeekAndRealtime);
        assert_eq!(output_offset, Some(Duration::from_secs(30)));
    }

    #[test]
    fn the_hls_builder_allows_a_compatible_dolby_vision_profile() {
        // Profile 8.1: the base layer is a conforming HDR10 signal.
        assert!(hls_command_for(dolby_vision_source_320p(8, 1)).is_ok());
        // And a source with no Dolby Vision at all is untouched.
        assert!(hls_command_for(source_320p()).is_ok());
    }

    #[test]
    fn the_hls_builder_uses_direct_profile_7_base_layer_decode() {
        let command = hls_command_for_with_capabilities(
            dolby_vision_source_320p(7, 6),
            FfmpegCapabilityReport::default(),
        )
        .expect("profile 7 HLS command");

        assert!(
            !command
                .arguments
                .iter()
                .any(|argument| argument.contains("dovi_split")),
            "got: {}",
            command.arguments.join(" ")
        );
    }
}
