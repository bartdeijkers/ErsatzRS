//! Injected core ports used by application services.

use std::sync::Arc;

use async_trait::async_trait;
use ersatzrs_core::playout_builder::PlayoutBuilder;
use ersatzrs_core::repositories::{
    AdminMediaRepository, AdminSearchRepository, AdminSettingsRepository, BlockRepository,
    ChannelRepository, CollectionRepository, DecoRepository, FfmpegProfileRepository,
    FillerRepository, LibraryRepository, MediaItemRepository, MediaSourceRepository,
    PlayoutRepository, ProgramScheduleRepository, SettingsMutation, TelevisionRepository,
    TemplateRepository, TraktRepository, TroubleshootingRepository, WatermarkRepository,
};
use ersatzrs_core::scheduling::{
    BlockPreviewRepository, PlayoutBuildRequest, SchedulingError, SchedulingRequest,
    SchedulingResult,
};
use ersatzrs_domain::{EntityId, MediaSourceKind, ScanProgressSink};

use crate::commands::ScanWorkRequest;
use crate::operations::OperationGuard;

/// Runtime failure returned by channel-specific process adapters.
#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct ChannelRuntimeError {
    message: String,
}

impl ChannelRuntimeError {
    /// Describe an unavailable channel runtime operation.
    #[must_use]
    pub fn unavailable(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

/// Runtime-neutral channel session and subtitle side effects.
#[async_trait]
pub trait ChannelRuntime: Send + Sync {
    /// Return channel numbers with an active streaming session.
    async fn active_channel_numbers(&self) -> Result<Vec<String>, ChannelRuntimeError>;

    /// Stop one channel streaming session, returning whether it existed.
    async fn stop_channel_session(&self, number: &str) -> Result<bool, ChannelRuntimeError>;

    /// Admit background subtitle extraction for saved channel playouts.
    fn request_subtitle_extraction(&self, playout_ids: Vec<EntityId>);
}

/// Failure returned by process-local settings effects.
#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct SettingsRuntimeError {
    message: String,
}

impl SettingsRuntimeError {
    /// Describe an unavailable settings runtime operation.
    #[must_use]
    pub fn unavailable(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

/// Effective validated `FFmpeg` tool paths.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettingsFfmpegPaths {
    pub ffmpeg: String,
    pub ffprobe: String,
}

/// Effective per-user Windows startup registration state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowsStartupStatus {
    /// The host platform is not Windows.
    Unsupported,
    /// No ErsatzRS startup registration exists for the current user.
    Disabled,
    /// The current user's registration points at the packaged tray launcher.
    Enabled,
    /// A registration exists, but its command does not match this release's launcher.
    NeedsRepair,
    /// The host could not inspect the registration or locate the packaged launcher.
    Unavailable,
}

/// Runtime effects triggered by persisted admin settings.
#[async_trait]
pub trait SettingsRuntime: Send + Sync {
    /// Inspect the current user's Windows startup registration.
    fn windows_startup_status(&self) -> WindowsStartupStatus;

    /// Enable or disable the current user's Windows startup registration.
    fn set_windows_startup(&self, enabled: bool) -> Result<(), SettingsRuntimeError>;

    /// Reload the process logging filter from persisted settings.
    async fn reload_logging(&self) -> Result<(), SettingsRuntimeError>;

    /// Resolve and execute the configured tools' version probes.
    async fn validate_ffmpeg_tools(
        &self,
        ffmpeg_path: &str,
        ffprobe_path: &str,
    ) -> Result<SettingsFfmpegPaths, SettingsRuntimeError>;

    /// Atomically persist `FFmpeg` settings and publish the fully-probed runtime.
    async fn commit_ffmpeg_settings(
        &self,
        ffmpeg_path: &str,
        ffprobe_path: &str,
        mutations: Vec<SettingsMutation>,
    ) -> Result<(), SettingsRuntimeError>;

    /// Admit background embedded-subtitle extraction.
    fn request_embedded_subtitle_extraction(&self, playout_ids: Vec<EntityId>);

    /// Ensure the `FFmpeg` report directory exists when report saving is enabled.
    fn ensure_ffmpeg_report_directory(&self);

    /// Apply the configured tuner count to process-local HDHR state.
    fn set_tuner_count(&self, tuner_count: i32);
}

/// Failure returned when the scheduler work queue cannot accept a request.
#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct PlayoutBuildQueueError {
    message: String,
}

impl PlayoutBuildQueueError {
    /// Describe an unavailable scheduler work queue.
    #[must_use]
    pub fn unavailable(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

/// Outbound application port for scheduler-owned playout build work.
pub trait PlayoutBuildQueue: Send + Sync {
    /// Enqueue one already-admitted request for FIFO worker consumption.
    ///
    /// # Errors
    ///
    /// Returns an error when the worker queue is closed or otherwise
    /// unavailable.
    fn enqueue(&self, request: PlayoutBuildRequest) -> Result<(), PlayoutBuildQueueError>;
}

/// Outbound application port for the real block scheduling builder.
pub trait BlockPreviewBuilder: Send + Sync {
    /// Build the temporary playout and return the engine's actual result.
    ///
    /// # Errors
    ///
    /// Returns the real scheduling error when the temporary graph cannot be
    /// evaluated.
    fn build(&self, request: SchedulingRequest) -> Result<SchedulingResult, SchedulingError>;
}

impl BlockPreviewBuilder for PlayoutBuilder {
    fn build(&self, request: SchedulingRequest) -> Result<SchedulingResult, SchedulingError> {
        PlayoutBuilder::build(self, request)
    }
}

/// Failure returned when the native scanner queue cannot accept work.
#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct ScanWorkQueueError {
    message: String,
}

impl ScanWorkQueueError {
    /// Describe an unavailable scanner worker queue.
    #[must_use]
    pub fn unavailable(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

/// Outbound application port for FIFO scanner work.
pub trait ScanWorkQueue: Send + Sync {
    /// Enqueue one already-validated scan request for the sole native worker.
    ///
    /// # Errors
    ///
    /// Returns an error when the worker queue is closed or unavailable.
    fn enqueue(&self, request: ScanWorkRequest) -> Result<(), ScanWorkQueueError>;
}

/// Storage- and runtime-neutral scanner adapter failure.
#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct ScanRuntimeError {
    message: String,
}

impl ScanRuntimeError {
    /// Create an adapter failure suitable for the shared operation registry.
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

/// Native scanner execution supplied by the composition root.
#[async_trait]
pub trait ScanRuntime: Send + Sync {
    /// Execute one full library scan.
    async fn scan_library(
        &self,
        provider: MediaSourceKind,
        library_id: EntityId,
        deep_scan: bool,
        progress: &dyn ScanProgressSink,
    ) -> Result<(), ScanRuntimeError>;

    /// Execute one provider-specific targeted show scan.
    async fn scan_show(
        &self,
        provider: MediaSourceKind,
        library_id: EntityId,
        show_id: EntityId,
        deep_scan: bool,
        progress: &dyn ScanProgressSink,
    ) -> Result<(), ScanRuntimeError>;

    /// Extract embedded subtitles after a successful eligible deep show scan.
    async fn extract_embedded_show_subtitles(
        &self,
        show_id: EntityId,
    ) -> Result<(), ScanRuntimeError>;
}

/// Provider metadata returned after validating a remote media source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteServerInfo {
    pub server_name: String,
    pub operating_system: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdminFfmpegRuntimeRead {
    pub source: String,
    pub ffmpeg_version: String,
    pub ffprobe_version: String,
    pub installed_tag: Option<String>,
    pub latest_tag: Option<String>,
    pub previous_tag: Option<String>,
    pub asset_name: Option<String>,
    pub downloaded_update: bool,
    pub update_available: bool,
    pub update_check_error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdminHealthRead {
    pub summary: ersatzrs_core::health::HealthCheckSummary,
    pub results: Vec<ersatzrs_core::health::HealthCheckResult>,
    pub ffmpeg_runtime: AdminFfmpegRuntimeRead,
}

#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct AdminHealthRuntimeError {
    message: String,
}

impl AdminHealthRuntimeError {
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

#[async_trait]
pub trait AdminHealthRuntime: Send + Sync {
    async fn issue_counts(&self) -> Result<(u32, u32), AdminHealthRuntimeError>;
    async fn run_health_checks(&self) -> Result<AdminHealthRead, AdminHealthRuntimeError>;
    async fn ffmpeg_runtime(&self) -> AdminFfmpegRuntimeRead;
}

#[async_trait]
pub trait AdminTroubleshootingRuntime: Send + Sync {
    async fn list_stream_selectors(&self) -> Result<Vec<String>, AdminTroubleshootingRuntimeError>;
    async fn start_playback_preview(
        &self,
        request: PlaybackPreviewRequest,
    ) -> Result<(), AdminTroubleshootingRuntimeError>;
    async fn build_troubleshooting_archive(
        &self,
    ) -> Result<Option<Vec<u8>>, AdminTroubleshootingRuntimeError>;
    async fn build_media_sample_archive(
        &self,
        media_item_id: EntityId,
    ) -> Result<Option<Vec<u8>>, AdminTroubleshootingRuntimeError>;
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PlaybackPreviewRequest {
    pub media_item_id: Option<EntityId>,
    pub channel_id: Option<EntityId>,
    pub ffmpeg_profile_id: Option<EntityId>,
    pub streaming_mode: Option<String>,
    pub watermark_ids: Vec<EntityId>,
    pub graphics_element_ids: Vec<EntityId>,
    pub stream_selector: Option<String>,
    pub preferred_audio_title: Option<String>,
    pub subtitle_id: Option<EntityId>,
    pub seek_seconds: Option<i64>,
    pub start: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum AdminTroubleshootingRuntimeError {
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Unavailable(String),
}

impl AdminTroubleshootingRuntimeError {
    #[must_use]
    pub fn unavailable(message: impl Into<String>) -> Self {
        Self::Unavailable(message.into())
    }
}

/// Plex PIN-flow values required by the application use case.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlexPinFlow {
    pub id: String,
    pub code: String,
    pub auth_url: String,
}

/// Failure returned by remote media-source provider adapters.
#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct MediaSourceRuntimeError {
    message: String,
}

impl MediaSourceRuntimeError {
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

/// Remote provider calls and background synchronization effects.
#[async_trait]
pub trait MediaSourceRuntime: Send + Sync {
    async fn validate_jellyfin(
        &self,
        address: &str,
        api_key: &str,
    ) -> Result<RemoteServerInfo, MediaSourceRuntimeError>;

    async fn refresh_jellyfin_libraries(
        &self,
        source_id: EntityId,
        address: &str,
        api_key: &str,
    ) -> Result<(), MediaSourceRuntimeError>;

    fn request_library_sync(
        &self,
        kind: ersatzrs_core::repositories::RemoteMediaSourceKind,
        library_ids: Vec<EntityId>,
    );

    fn request_external_collections_sync(
        &self,
        kind: ersatzrs_core::repositories::RemoteMediaSourceKind,
        media_source_id: EntityId,
        deep_scan: bool,
        guard: OperationGuard,
    );

    async fn start_plex_pin(&self) -> Result<PlexPinFlow, MediaSourceRuntimeError>;

    async fn complete_plex_pin(
        &self,
        pin_id: &str,
        pin_code: &str,
    ) -> Result<Option<String>, MediaSourceRuntimeError>;
}

/// Bounded repository port set for the next playout and scan vertical slices.
#[derive(Clone)]
pub struct ApplicationPorts {
    channels: Arc<dyn ChannelRepository>,
    channel_runtime: Arc<dyn ChannelRuntime>,
    trakt: Arc<dyn TraktRepository>,
    admin_settings: Arc<dyn AdminSettingsRepository>,
    settings_runtime: Arc<dyn SettingsRuntime>,
    collections: Arc<dyn CollectionRepository>,
    ffmpeg_profiles: Arc<dyn FfmpegProfileRepository>,
    fillers: Arc<dyn FillerRepository>,
    watermarks: Arc<dyn WatermarkRepository>,
    templates: Arc<dyn TemplateRepository>,
    blocks: Arc<dyn BlockRepository>,
    decos: Arc<dyn DecoRepository>,
    program_schedules: Arc<dyn ProgramScheduleRepository>,
    playouts: Arc<dyn PlayoutRepository>,
    libraries: Arc<dyn LibraryRepository>,
    media_sources: Arc<dyn MediaSourceRepository>,
    media_items: Arc<dyn MediaItemRepository>,
    admin_media: Arc<dyn AdminMediaRepository>,
    television: Arc<dyn TelevisionRepository>,
    admin_health_runtime: Arc<dyn AdminHealthRuntime>,
    troubleshooting: Arc<dyn TroubleshootingRepository>,
    troubleshooting_runtime: Arc<dyn AdminTroubleshootingRuntime>,
    media_source_runtime: Arc<dyn MediaSourceRuntime>,
    admin_search: Arc<dyn AdminSearchRepository>,
    block_previews: Arc<dyn BlockPreviewRepository>,
    block_preview_builder: Arc<dyn BlockPreviewBuilder>,
    playout_builds: Arc<dyn PlayoutBuildQueue>,
    scan_work: Arc<dyn ScanWorkQueue>,
    scan_runtime: Arc<dyn ScanRuntime>,
}

impl ApplicationPorts {
    /// Construct the port set from adapter implementations.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        channels: Arc<dyn ChannelRepository>,
        channel_runtime: Arc<dyn ChannelRuntime>,
        trakt: Arc<dyn TraktRepository>,
        admin_settings: Arc<dyn AdminSettingsRepository>,
        settings_runtime: Arc<dyn SettingsRuntime>,
        collections: Arc<dyn CollectionRepository>,
        ffmpeg_profiles: Arc<dyn FfmpegProfileRepository>,
        fillers: Arc<dyn FillerRepository>,
        watermarks: Arc<dyn WatermarkRepository>,
        templates: Arc<dyn TemplateRepository>,
        blocks: Arc<dyn BlockRepository>,
        decos: Arc<dyn DecoRepository>,
        program_schedules: Arc<dyn ProgramScheduleRepository>,
        playouts: Arc<dyn PlayoutRepository>,
        libraries: Arc<dyn LibraryRepository>,
        media_sources: Arc<dyn MediaSourceRepository>,
        media_items: Arc<dyn MediaItemRepository>,
        admin_media: Arc<dyn AdminMediaRepository>,
        television: Arc<dyn TelevisionRepository>,
        admin_health_runtime: Arc<dyn AdminHealthRuntime>,
        troubleshooting: Arc<dyn TroubleshootingRepository>,
        troubleshooting_runtime: Arc<dyn AdminTroubleshootingRuntime>,
        media_source_runtime: Arc<dyn MediaSourceRuntime>,
        admin_search: Arc<dyn AdminSearchRepository>,
        block_previews: Arc<dyn BlockPreviewRepository>,
        block_preview_builder: Arc<dyn BlockPreviewBuilder>,
        playout_builds: Arc<dyn PlayoutBuildQueue>,
        scan_work: Arc<dyn ScanWorkQueue>,
        scan_runtime: Arc<dyn ScanRuntime>,
    ) -> Self {
        Self {
            channels,
            channel_runtime,
            trakt,
            admin_settings,
            settings_runtime,
            collections,
            ffmpeg_profiles,
            fillers,
            watermarks,
            templates,
            blocks,
            decos,
            program_schedules,
            playouts,
            libraries,
            media_sources,
            media_items,
            admin_media,
            television,
            admin_health_runtime,
            troubleshooting,
            troubleshooting_runtime,
            media_source_runtime,
            admin_search,
            block_previews,
            block_preview_builder,
            playout_builds,
            scan_work,
            scan_runtime,
        }
    }

    /// Channel persistence used by admin command and query slices.
    #[must_use]
    pub fn channels(&self) -> &(dyn ChannelRepository + 'static) {
        self.channels.as_ref()
    }

    /// Process-local channel session and subtitle runtime.
    #[must_use]
    pub fn channel_runtime(&self) -> &(dyn ChannelRuntime + 'static) {
        self.channel_runtime.as_ref()
    }

    /// Trakt list persistence and synchronizer adapter.
    #[must_use]
    pub fn trakt(&self) -> &(dyn TraktRepository + 'static) {
        self.trakt.as_ref()
    }

    /// Settings persistence used by typed settings commands and queries.
    #[must_use]
    pub fn admin_settings(&self) -> &(dyn AdminSettingsRepository + 'static) {
        self.admin_settings.as_ref()
    }

    /// Process-local settings effects.
    #[must_use]
    pub fn settings_runtime(&self) -> &(dyn SettingsRuntime + 'static) {
        self.settings_runtime.as_ref()
    }

    /// Collection persistence used by admin command and query slices.
    #[must_use]
    pub fn collections(&self) -> &(dyn CollectionRepository + 'static) {
        self.collections.as_ref()
    }

    /// `FFmpeg` profile persistence used by admin command and query slices.
    #[must_use]
    pub fn ffmpeg_profiles(&self) -> &(dyn FfmpegProfileRepository + 'static) {
        self.ffmpeg_profiles.as_ref()
    }

    /// Filler-preset persistence used by admin command and query slices.
    #[must_use]
    pub fn fillers(&self) -> &(dyn FillerRepository + 'static) {
        self.fillers.as_ref()
    }

    /// Watermark persistence used by admin command and query slices.
    #[must_use]
    pub fn watermarks(&self) -> &(dyn WatermarkRepository + 'static) {
        self.watermarks.as_ref()
    }

    /// Template persistence used by admin command and query slices.
    #[must_use]
    pub fn templates(&self) -> &(dyn TemplateRepository + 'static) {
        self.templates.as_ref()
    }

    /// Block persistence used by admin command and query slices.
    #[must_use]
    pub fn blocks(&self) -> &(dyn BlockRepository + 'static) {
        self.blocks.as_ref()
    }

    /// Deco persistence used by admin command and query slices.
    #[must_use]
    pub fn decos(&self) -> &(dyn DecoRepository + 'static) {
        self.decos.as_ref()
    }

    /// Program schedule persistence used by admin command and query slices.
    #[must_use]
    pub fn program_schedules(&self) -> &(dyn ProgramScheduleRepository + 'static) {
        self.program_schedules.as_ref()
    }

    /// Playout persistence used by the p7b-02 command slice.
    #[must_use]
    pub fn playouts(&self) -> &(dyn PlayoutRepository + 'static) {
        self.playouts.as_ref()
    }

    /// Library persistence used by the p7b-03 scan slice.
    #[must_use]
    pub fn libraries(&self) -> &(dyn LibraryRepository + 'static) {
        self.libraries.as_ref()
    }

    /// Media-source persistence used by admin source and artwork queries.
    #[must_use]
    pub fn media_sources(&self) -> &(dyn MediaSourceRepository + 'static) {
        self.media_sources.as_ref()
    }

    /// Media-item persistence used by admin media commands.
    #[must_use]
    pub fn media_items(&self) -> &(dyn MediaItemRepository + 'static) {
        self.media_items.as_ref()
    }

    /// Rich admin media detail projections.
    #[must_use]
    pub fn admin_media(&self) -> &(dyn AdminMediaRepository + 'static) {
        self.admin_media.as_ref()
    }

    /// Television persistence used by season-detail and media-info queries.
    #[must_use]
    pub fn television(&self) -> &(dyn TelevisionRepository + 'static) {
        self.television.as_ref()
    }

    #[must_use]
    pub fn admin_health_runtime(&self) -> &(dyn AdminHealthRuntime + 'static) {
        self.admin_health_runtime.as_ref()
    }

    #[must_use]
    pub fn troubleshooting(&self) -> &(dyn TroubleshootingRepository + 'static) {
        self.troubleshooting.as_ref()
    }

    #[must_use]
    pub fn troubleshooting_runtime(&self) -> &(dyn AdminTroubleshootingRuntime + 'static) {
        self.troubleshooting_runtime.as_ref()
    }

    /// Remote media-source validation and synchronization effects.
    #[must_use]
    pub fn media_source_runtime(&self) -> &(dyn MediaSourceRuntime + 'static) {
        self.media_source_runtime.as_ref()
    }

    /// Search-index and card metadata used by the global admin search.
    #[must_use]
    pub fn admin_search(&self) -> &(dyn AdminSearchRepository + 'static) {
        self.admin_search.as_ref()
    }

    /// Real media/reference-data loader used by unsaved block preview.
    #[must_use]
    pub fn block_previews(&self) -> &(dyn BlockPreviewRepository + 'static) {
        self.block_previews.as_ref()
    }

    /// Real scheduling builder used by unsaved block preview.
    #[must_use]
    pub fn block_preview_builder(&self) -> &(dyn BlockPreviewBuilder + 'static) {
        self.block_preview_builder.as_ref()
    }

    /// Scheduler work queue used by the playout-build command.
    #[must_use]
    pub fn playout_builds(&self) -> &(dyn PlayoutBuildQueue + 'static) {
        self.playout_builds.as_ref()
    }

    /// FIFO scanner work queue used by library and show commands.
    #[must_use]
    pub fn scan_work(&self) -> &(dyn ScanWorkQueue + 'static) {
        self.scan_work.as_ref()
    }

    /// Native scanner runtime used only by the composition-root worker.
    #[must_use]
    pub fn scan_runtime(&self) -> &(dyn ScanRuntime + 'static) {
        self.scan_runtime.as_ref()
    }
}

impl std::fmt::Debug for ApplicationPorts {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ApplicationPorts")
            .field("channels", &"dyn ChannelRepository")
            .field("channel_runtime", &"dyn ChannelRuntime")
            .field("trakt", &"dyn TraktRepository")
            .field("admin_settings", &"dyn AdminSettingsRepository")
            .field("settings_runtime", &"dyn SettingsRuntime")
            .field("collections", &"dyn CollectionRepository")
            .field("ffmpeg_profiles", &"dyn FfmpegProfileRepository")
            .field("fillers", &"dyn FillerRepository")
            .field("watermarks", &"dyn WatermarkRepository")
            .field("templates", &"dyn TemplateRepository")
            .field("blocks", &"dyn BlockRepository")
            .field("decos", &"dyn DecoRepository")
            .field("program_schedules", &"dyn ProgramScheduleRepository")
            .field("playouts", &"dyn PlayoutRepository")
            .field("libraries", &"dyn LibraryRepository")
            .field("media_sources", &"dyn MediaSourceRepository")
            .field("media_items", &"dyn MediaItemRepository")
            .field("admin_media", &"dyn AdminMediaRepository")
            .field("media_source_runtime", &"dyn MediaSourceRuntime")
            .field("admin_search", &"dyn AdminSearchRepository")
            .field("block_previews", &"dyn BlockPreviewRepository")
            .field("block_preview_builder", &"dyn BlockPreviewBuilder")
            .field("playout_builds", &"dyn PlayoutBuildQueue")
            .field("scan_work", &"dyn ScanWorkQueue")
            .field("scan_runtime", &"dyn ScanRuntime")
            .finish()
    }
}
