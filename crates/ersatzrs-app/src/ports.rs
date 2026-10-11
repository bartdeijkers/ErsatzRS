//! Injected core ports used by application services.

use std::sync::Arc;

use async_trait::async_trait;
use ersatzrs_core::playout_builder::PlayoutBuilder;
use ersatzrs_core::repositories::{
    AddonMediaListLifecycleRepository, AddonMediaListRepository,
    AddonMediaListSourceProbeRepository, AdminMediaRepository, AdminSearchRepository,
    AdminSettingsRepository, BlockRepository, ChannelRepository, CollectionRepository,
    DecoRepository, FfmpegProfileRepository, FillerRepository, GraphicsRepository,
    LibraryRepository, ManagedLibraryOperationRepository, MediaItemRepository,
    MediaSourceRepository, PlayoutRepository, ProgramScheduleRepository, SavedSearchRepository,
    SettingsMutation, TeletextDesignRepository, TeletextProfileRepository, TelevisionRepository,
    TemplateRepository, TroubleshootingRepository, WatermarkRepository,
};
use ersatzrs_core::scheduling::{
    BlockPreviewRepository, PlayoutBuildRequest, SchedulingError, SchedulingRequest,
    SchedulingResult,
};
use ersatzrs_domain::{EntityId, MediaSourceKind, ScanPlayoutRefresh, ScanProgressSink};
use serde::{Deserialize, Serialize};

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

    /// Snapshot normal playlist workers without extending their lifetime.
    async fn session_models(
        &self,
    ) -> Result<Vec<ersatzrs_core::hls::HlsSessionModel>, ChannelRuntimeError>;

    /// Read output provenance from the isolated browser-preview worker.
    async fn preview_segment_output(
        &self,
        _number: &str,
        _segment: &str,
        _started_at_millis: i64,
    ) -> Result<Option<ersatzrs_core::hls::HlsSegmentOutput>, ChannelRuntimeError> {
        Ok(None)
    }

    /// Stop one channel streaming session, returning whether it existed.
    async fn stop_channel_session(&self, number: &str) -> Result<bool, ChannelRuntimeError>;

    /// Admit background subtitle extraction for saved channel playouts.
    fn request_subtitle_extraction(&self, playout_ids: Vec<EntityId>);

    /// Republish active consumers of a changed profile without rebuilding schedules.
    async fn refresh_profile_playouts(
        &self,
        profile_id: EntityId,
    ) -> Result<(), ChannelRuntimeError>;
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

/// Storage-neutral result of one completed native library scan.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScanRuntimeReport {
    pub playout_refreshes: Vec<ScanPlayoutRefresh>,
}

/// Whether the adapter's independently owned scan workers confirmed cleanup.
///
/// Uncertainty is sticky for the enclosing operation and must not become an
/// ordinary scan failure that releases its durable lease.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScanCleanupConfirmation {
    Confirmed,
    Unconfirmed,
}

/// Native scanner execution supplied by the composition root.
#[async_trait]
pub trait ScanRuntime: Send + Sync {
    /// Await current scan-owned workers without closing the operation scope.
    ///
    /// Adapters with independent process workers must retain uncertainty even
    /// when their scanner absorbs a worker error. Other adapters have no
    /// independent cleanup to confirm.
    async fn confirm_cleanup(&self) -> ScanCleanupConfirmation {
        ScanCleanupConfirmation::Confirmed
    }

    /// Read-only, bounded folder discovery before a local television library is
    /// saved. The native adapter must retain capacity for timed-out workers.
    async fn preview_television_folders(
        &self,
        _paths: Vec<String>,
    ) -> Result<Vec<ersatzrs_core::television_discovery::TelevisionFolderPreview>, ScanRuntimeError>
    {
        Err(ScanRuntimeError::new("Folder preview is unavailable"))
    }

    /// Return Local libraries whose interrupted scan markers need replay.
    async fn interrupted_scan_library_ids(&self) -> Result<Vec<EntityId>, ScanRuntimeError> {
        Ok(Vec::new())
    }

    /// Recover membership-triggered refreshes that survived process shutdown.
    async fn recover_playout_refreshes(&self) -> Result<ScanRuntimeReport, ScanRuntimeError> {
        Ok(ScanRuntimeReport::default())
    }

    /// Load pending refresh intents for the normal retry sweep. Unlike startup
    /// recovery, this must not replay builds that are already in progress.
    async fn pending_playout_refreshes(&self) -> Result<ScanRuntimeReport, ScanRuntimeError> {
        Ok(ScanRuntimeReport::default())
    }

    /// Execute one full library scan.
    async fn scan_library(
        &self,
        provider: MediaSourceKind,
        library_id: EntityId,
        deep_scan: bool,
        progress: &dyn ScanProgressSink,
    ) -> Result<ScanRuntimeReport, ScanRuntimeError>;

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
    /// Read completed cached checks, or request a fresh run when `refresh` is true.
    /// Concurrent requests may join one run; dropping a reader does not cancel it.
    async fn run_health_checks(
        &self,
        refresh: bool,
    ) -> Result<AdminHealthRead, AdminHealthRuntimeError>;
    async fn ffmpeg_runtime(&self) -> AdminFfmpegRuntimeRead;
}

#[async_trait]
pub trait AdminTroubleshootingRuntime: Send + Sync {
    async fn list_stream_selectors(&self) -> Result<Vec<String>, AdminTroubleshootingRuntimeError>;
    /// List bundled and operator music-video credits templates for previews.
    async fn list_music_video_credits_templates(
        &self,
    ) -> Result<Vec<String>, AdminTroubleshootingRuntimeError>;
    async fn start_playback_preview(
        &self,
        request: PlaybackPreviewRequest,
    ) -> Result<(), AdminTroubleshootingRuntimeError>;
    /// Admit one ephemeral preview and return its owned ID without waiting for `FFmpeg`.
    async fn queue_playback_preview(
        &self,
        request: PlaybackPreviewRequest,
    ) -> Result<PlaybackPreviewStatus, AdminTroubleshootingRuntimeError>;
    /// Read one owned preview; old IDs never observe a replacement's output.
    async fn playback_preview_status(
        &self,
        id: String,
    ) -> Result<Option<PlaybackPreviewStatus>, AdminTroubleshootingRuntimeError>;
    /// Request cancellation of this ID only. Terminal states imply completed cleanup.
    async fn stop_playback_preview(
        &self,
        id: String,
    ) -> Result<Option<PlaybackPreviewStatus>, AdminTroubleshootingRuntimeError>;
    /// Read a bounded, safe HLS member from the current owned preview.
    async fn playback_preview_file(
        &self,
        id: String,
        filename: String,
    ) -> Result<Option<Vec<u8>>, AdminTroubleshootingRuntimeError>;
    async fn build_troubleshooting_archive(
        &self,
    ) -> Result<Option<Vec<u8>>, AdminTroubleshootingRuntimeError>;
    async fn playback_preview_log(
        &self,
    ) -> Result<Option<PlaybackPreviewLog>, AdminTroubleshootingRuntimeError>;
    async fn build_media_sample_archive(
        &self,
        media_item_id: EntityId,
    ) -> Result<Option<Vec<u8>>, AdminTroubleshootingRuntimeError>;
}

/// Observable diagnostic lifecycle, independent of live channel producers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaybackPreviewState {
    Queued,
    Running,
    Completed,
    Failed,
    Cancelling,
    Cancelled,
}

impl PlaybackPreviewState {
    /// Terminal states follow cleanup attempts; check `cleanup_confirmed` before reusing capacity.
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
}

/// Bounded ephemeral preview state returned to authenticated administration.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlaybackPreviewStatus {
    pub id: String,
    pub state: PlaybackPreviewState,
    pub ready: bool,
    /// False after unconfirmed cleanup: new work remains fenced until restart.
    pub cleanup_confirmed: bool,
    pub playlist_url: String,
    /// UTC RFC 3339 instant, suitable for display in the operator's timezone.
    pub selected_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    /// Stable safe startup/lifecycle code; never raw provider/process errors.
    pub error_code: Option<String>,
    pub return_url: String,
    pub log: Option<PlaybackPreviewLog>,
}

/// Completed preview diagnostics, read under the preview's output ownership.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlaybackPreviewLog {
    pub text: String,
    /// Last reported `FFmpeg` processing speed relative to real time.
    pub speed: Option<f64>,
}

/// One materialized entry shown by Remote Stream playlist administration.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteStreamPlaylistEntryRead {
    pub id: String,
    pub title: String,
    pub duration_seconds: Option<u64>,
    pub state: String,
}

/// Effective manifest and synchronization state shown by administration.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteStreamPlaylistRead {
    pub library_id: i64,
    pub root_index: usize,
    pub relative_path: String,
    pub name: String,
    pub url: String,
    pub addon: String,
    pub folder: String,
    pub interval: String,
    pub max_items: usize,
    pub order: String,
    pub on_removed: String,
    pub create_playlist: bool,
    pub tags: Vec<String>,
    pub state: String,
    pub last_sync: Option<String>,
    pub item_count: usize,
    pub truncated: bool,
    pub managed_by_ersatzrs: bool,
    pub entries: Vec<RemoteStreamPlaylistEntryRead>,
}

/// Editable values for an ErsatzRS-owned Remote Stream playlist manifest.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteStreamPlaylistEdit {
    pub library_id: i64,
    pub root_index: usize,
    pub relative_path: Option<String>,
    pub name: String,
    pub url: String,
    pub addon: String,
    pub folder: String,
    pub interval: String,
    pub max_items: usize,
    pub order: String,
    pub on_removed: String,
    pub create_playlist: bool,
    pub tags: String,
}

/// Bounded result of one explicit playlist enumeration probe.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteStreamPlaylistTroubleshootingRead {
    pub entry_count: usize,
    pub diagnostic_tail: String,
}

/// Failure from the native playlist administration adapter.
#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct RemoteStreamPlaylistRuntimeError {
    message: String,
}

impl RemoteStreamPlaylistRuntimeError {
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

/// Native filesystem, storage, and add-on operations for playlist administration.
#[async_trait]
pub trait RemoteStreamPlaylistRuntime: Send + Sync {
    async fn list(
        &self,
        library_id: i64,
    ) -> Result<Vec<RemoteStreamPlaylistRead>, RemoteStreamPlaylistRuntimeError>;

    async fn addon_ids(&self) -> Result<Vec<String>, RemoteStreamPlaylistRuntimeError>;

    async fn tags(&self) -> Result<Vec<String>, RemoteStreamPlaylistRuntimeError>;

    async fn save(
        &self,
        editor: RemoteStreamPlaylistEdit,
    ) -> Result<String, RemoteStreamPlaylistRuntimeError>;

    async fn delete(
        &self,
        library_id: i64,
        root_index: usize,
        relative_path: String,
    ) -> Result<(), RemoteStreamPlaylistRuntimeError>;

    async fn exclude_entry(
        &self,
        library_id: i64,
        root_index: usize,
        relative_path: String,
        entry_id: String,
    ) -> Result<(), RemoteStreamPlaylistRuntimeError>;

    async fn troubleshoot(
        &self,
        library_id: i64,
        root_index: usize,
        relative_path: String,
    ) -> Result<RemoteStreamPlaylistTroubleshootingRead, RemoteStreamPlaylistRuntimeError>;
}

/// Exact repository and offer count committed by a bundle import.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AddonRepositoryImportRead {
    pub id: String,
    pub name: String,
    pub available_count: usize,
}

/// One local checkout or offline bundle shown on the add-on settings page.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddonRepositoryRead {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub root_path: String,
    pub last_scan: String,
    pub last_error: Option<String>,
}

/// One validated local offer paired with its source repository.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AvailableAddonRead {
    pub repository_id: String,
    pub addon: ersatzrs_addon_contract::CatalogAddon,
    pub host_compatible: bool,
    pub package_available: bool,
    pub icon_url: Option<String>,
}

/// Persisted, unresolved setting value or secret reference.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddonSettingRead {
    pub key: String,
    pub value: Option<String>,
    pub reference_kind: Option<String>,
    pub reference: Option<String>,
}

/// Latest maintenance evidence safe for host-rendered administration.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddonToolMaintenanceRead {
    /// Declared executable setting, never its configured path.
    pub tool_key: String,
    /// Current durable queue state.
    pub state: ersatzrs_domain::AddonToolMaintenanceState,
    /// Latest terminal outcome, if execution has finished.
    pub outcome: Option<ersatzrs_domain::AddonToolMaintenanceOutcome>,
    /// Bounded machine diagnostic; provider prose is deliberately excluded.
    pub diagnostic_code: Option<String>,
    /// Observed versions, without executable identities or filesystem paths.
    pub before_version: Option<String>,
    /// Version observed after the native update, if readable.
    pub after_version: Option<String>,
    /// Time of the latest persisted status change.
    pub updated_at: String,
}

/// One installed package and its host-rendered configuration state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledAddonRead {
    pub addon_id: String,
    pub repository_id: Option<String>,
    pub version: String,
    pub enabled: bool,
    pub trust_tier: String,
    pub manifest: ersatzrs_addon_contract::AddonManifest,
    pub settings: Vec<AddonSettingRead>,
    pub last_check_code: Option<String>,
    pub last_check_message: Option<String>,
    pub last_check_tool_update: Option<ersatzrs_addon_contract::AddonToolUpdateWarning>,
    pub last_check_at: Option<String>,
    pub installed_at: String,
    pub updated_at: String,
    pub icon_url: Option<String>,
    /// Only the latest attempt for each tool currently declared by the package.
    pub tool_maintenance: Vec<AddonToolMaintenanceRead>,
}

/// Complete read model for the add-on settings page.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddonOverviewRead {
    pub unverified_sources_enabled: bool,
    pub checkout_path: String,
    pub repositories: Vec<AddonRepositoryRead>,
    pub available: Vec<AvailableAddonRead>,
    pub installed: Vec<InstalledAddonRead>,
}

/// Bounded package artwork served through the host-owned admin API.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AddonIconRead {
    pub bytes: Vec<u8>,
    pub content_type: String,
}

/// Editable setting value. Secret kinds carry a reference, never secret bytes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddonSettingEdit {
    pub addon_id: String,
    pub key: String,
    pub value: Option<String>,
    pub reference_kind: Option<String>,
    pub reference: Option<String>,
}

/// Durable access-check state without provider diagnostics.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AddonItemAccessState {
    Queued,
    Running,
    Waiting,
    Complete,
    Failed,
    Cancelled,
    Stale,
}

/// Redacted access evidence for one saved item context.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddonItemAccessRead {
    pub job_id: i64,
    pub state: AddonItemAccessState,
    pub outcome: Option<ersatzrs_addon_contract::MediaListAccessCheckOutcome>,
    pub next_eligible_at: Option<chrono::DateTime<chrono::Utc>>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Failure from the native add-on lifecycle adapter.
#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct AddonRuntimeError {
    message: String,
}

impl AddonRuntimeError {
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

/// Which add-on the host selected for one operator source URL.
///
/// `Ambiguous` is deliberately distinct from `None`: several packages claim the
/// host equally well, so the operator must choose rather than the host guessing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceAddonRouting {
    /// A package declares this host as one it handles.
    Declared(String),
    /// No package claims the host; the configured default accepted it.
    Default(String),
    /// Nothing claims the host and no default applies.
    None,
    /// Several packages claim the host equally well.
    Ambiguous(Vec<String>),
}

impl SourceAddonRouting {
    /// The selected add-on, if exactly one applies.
    #[must_use]
    pub fn addon_id(&self) -> Option<&str> {
        match self {
            Self::Declared(addon_id) | Self::Default(addon_id) => Some(addon_id.as_str()),
            Self::None | Self::Ambiguous(_) => None,
        }
    }
}

/// Local source, package, configuration, and process lifecycle operations.
#[async_trait]
pub trait AddonRuntime: Send + Sync {
    /// Invoke one exact profile selection without publication authority.
    /// Native adapters retain process cleanup after caller abandonment.
    async fn invoke_teletext_profile(
        &self,
        _selection: ersatzrs_domain::TeletextSelectionSnapshot,
        _input: Vec<u8>,
    ) -> Result<
        crate::teletext_profiles::TeletextProfileRuntimeReceipt,
        crate::teletext_profiles::TeletextProfileRuntimeError,
    > {
        Err(crate::teletext_profiles::TeletextProfileRuntimeError::Unsupported)
    }

    /// Invoke under the channel preparation's original absolute deadline.
    /// Adapters must enforce the remaining budget, rather than restart it.
    async fn invoke_teletext_profile_before(
        &self,
        _selection: ersatzrs_domain::TeletextSelectionSnapshot,
        _input: Vec<u8>,
        _deadline: std::time::Instant,
    ) -> Result<
        crate::teletext_profiles::TeletextProfileRuntimeReceipt,
        crate::teletext_profiles::TeletextProfileRuntimeError,
    > {
        Err(crate::teletext_profiles::TeletextProfileRuntimeError::Unsupported)
    }

    /// Whether the enabled package provides access checking and runtime identity.
    async fn supports_media_list_access_check(
        &self,
        _addon_id: &str,
    ) -> Result<bool, AddonRuntimeError> {
        Ok(false)
    }

    /// Queue a durable access check using the exact saved draft revision.
    async fn queue_media_list_access_check(
        &self,
        _list_id: i64,
        _item_id: i64,
        _expected_revision: i32,
    ) -> Result<i64, AddonRuntimeError> {
        Err(AddonRuntimeError::new("item access checks are unavailable"))
    }

    /// Read current typed access evidence; unsupported adapters have no evidence.
    async fn media_list_access_check(
        &self,
        _list_id: i64,
        _item_id: i64,
    ) -> Result<Option<AddonItemAccessRead>, AddonRuntimeError> {
        Ok(None)
    }

    /// Cancel only the explicitly owned item job.
    async fn cancel_media_list_access_check(
        &self,
        _list_id: i64,
        _item_id: i64,
        _job_id: i64,
    ) -> Result<(), AddonRuntimeError> {
        Err(AddonRuntimeError::new("item access checks are unavailable"))
    }

    /// Read the installed package's validated option declaration without activating it.
    async fn media_list_option_schema(
        &self,
        _addon_id: &str,
    ) -> Result<Option<ersatzrs_core::repositories::AddonMediaListOptionSchema>, AddonRuntimeError>
    {
        Ok(None)
    }

    async fn overview(&self) -> Result<AddonOverviewRead, AddonRuntimeError>;
    async fn installed_icon(
        &self,
        addon_id: String,
    ) -> Result<Option<AddonIconRead>, AddonRuntimeError>;
    async fn source_icon(
        &self,
        source_id: String,
        addon_id: String,
    ) -> Result<Option<AddonIconRead>, AddonRuntimeError>;
    async fn scan_repositories(&self) -> Result<(), AddonRuntimeError>;
    async fn remove_repository(&self, repository_id: String) -> Result<(), AddonRuntimeError>;
    async fn install(
        &self,
        repository_id: String,
        addon_id: String,
        adopt_legacy: bool,
    ) -> Result<(), AddonRuntimeError>;
    async fn set_unverified_sources(&self, enabled: bool) -> Result<(), AddonRuntimeError>;
    /// Select the add-on that should process one operator-supplied source URL.
    async fn resolve_source_addon(
        &self,
        url: String,
        required_capabilities: Vec<String>,
    ) -> Result<SourceAddonRouting, AddonRuntimeError>;
    async fn default_remote_stream_addon(&self) -> Result<Option<String>, AddonRuntimeError>;
    async fn set_default_remote_stream_addon(
        &self,
        addon_id: Option<String>,
    ) -> Result<(), AddonRuntimeError>;
    async fn install_from_package(&self, archive: Vec<u8>) -> Result<(), AddonRuntimeError>;
    async fn install_repository_bundle(
        &self,
        archive: Vec<u8>,
    ) -> Result<AddonRepositoryImportRead, AddonRuntimeError>;
    async fn save_setting(&self, setting: AddonSettingEdit) -> Result<(), AddonRuntimeError>;
    async fn check(&self, addon_id: String) -> Result<String, AddonRuntimeError>;
    async fn enable(&self, addon_id: String) -> Result<String, AddonRuntimeError>;
    async fn disable(&self, addon_id: String) -> Result<(), AddonRuntimeError>;
    async fn uninstall(&self, addon_id: String, force: bool) -> Result<(), AddonRuntimeError>;
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PlaybackPreviewRequest {
    pub streaming_engine: ersatzrs_domain::StreamingEngine,
    pub media_item_id: Option<EntityId>,
    pub channel_id: Option<EntityId>,
    pub ffmpeg_profile_id: Option<EntityId>,
    pub streaming_mode: Option<String>,
    pub watermark_ids: Vec<EntityId>,
    pub graphics_element_ids: Vec<EntityId>,
    pub stream_selector: Option<String>,
    pub preferred_audio_title: Option<String>,
    pub subtitle_id: Option<EntityId>,
    pub music_video_credits_template: Option<String>,
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
    async fn refresh_plex_libraries(&self, source_id: i64) -> Result<(), MediaSourceRuntimeError>;
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

    async fn start_plex_pin(
        &self,
        force_new_credentials: bool,
    ) -> Result<PlexPinFlow, MediaSourceRuntimeError>;

    async fn complete_plex_pin(&self, pin_id: &str) -> Result<bool, MediaSourceRuntimeError>;
}

/// Bounded repository port set for the next playout and scan vertical slices.
#[derive(Clone)]
pub struct ApplicationPorts {
    channels: Arc<dyn ChannelRepository>,
    channel_runtime: Arc<dyn ChannelRuntime>,
    addon_media_lists: Arc<dyn AddonMediaListRepository>,
    addon_media_list_lifecycle: Arc<dyn AddonMediaListLifecycleRepository>,
    addon_media_list_source_probes: Arc<dyn AddonMediaListSourceProbeRepository>,
    admin_settings: Arc<dyn AdminSettingsRepository>,
    settings_runtime: Arc<dyn SettingsRuntime>,
    collections: Arc<dyn CollectionRepository>,
    saved_searches: Arc<dyn SavedSearchRepository>,
    ffmpeg_profiles: Arc<dyn FfmpegProfileRepository>,
    fillers: Arc<dyn FillerRepository>,
    graphics: Arc<dyn GraphicsRepository>,
    watermarks: Arc<dyn WatermarkRepository>,
    templates: Arc<dyn TemplateRepository>,
    blocks: Arc<dyn BlockRepository>,
    decos: Arc<dyn DecoRepository>,
    program_schedules: Arc<dyn ProgramScheduleRepository>,
    playouts: Arc<dyn PlayoutRepository>,
    libraries: Arc<dyn LibraryRepository>,
    managed_library_operations: Arc<dyn ManagedLibraryOperationRepository>,
    media_sources: Arc<dyn MediaSourceRepository>,
    media_items: Arc<dyn MediaItemRepository>,
    admin_media: Arc<dyn AdminMediaRepository>,
    television: Arc<dyn TelevisionRepository>,
    admin_health_runtime: Arc<dyn AdminHealthRuntime>,
    troubleshooting: Arc<dyn TroubleshootingRepository>,
    troubleshooting_runtime: Arc<dyn AdminTroubleshootingRuntime>,
    remote_stream_playlists: Arc<dyn RemoteStreamPlaylistRuntime>,
    addons: Arc<dyn AddonRuntime>,
    teletext_profiles: Arc<dyn TeletextProfileRepository>,
    teletext_designs: Option<Arc<dyn TeletextDesignRepository>>,
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
        addon_media_lists: Arc<dyn AddonMediaListRepository>,
        addon_media_list_lifecycle: Arc<dyn AddonMediaListLifecycleRepository>,
        addon_media_list_source_probes: Arc<dyn AddonMediaListSourceProbeRepository>,
        admin_settings: Arc<dyn AdminSettingsRepository>,
        settings_runtime: Arc<dyn SettingsRuntime>,
        collections: Arc<dyn CollectionRepository>,
        saved_searches: Arc<dyn SavedSearchRepository>,
        ffmpeg_profiles: Arc<dyn FfmpegProfileRepository>,
        fillers: Arc<dyn FillerRepository>,
        graphics: Arc<dyn GraphicsRepository>,
        watermarks: Arc<dyn WatermarkRepository>,
        templates: Arc<dyn TemplateRepository>,
        blocks: Arc<dyn BlockRepository>,
        decos: Arc<dyn DecoRepository>,
        program_schedules: Arc<dyn ProgramScheduleRepository>,
        playouts: Arc<dyn PlayoutRepository>,
        libraries: Arc<dyn LibraryRepository>,
        managed_library_operations: Arc<dyn ManagedLibraryOperationRepository>,
        media_sources: Arc<dyn MediaSourceRepository>,
        media_items: Arc<dyn MediaItemRepository>,
        admin_media: Arc<dyn AdminMediaRepository>,
        television: Arc<dyn TelevisionRepository>,
        admin_health_runtime: Arc<dyn AdminHealthRuntime>,
        troubleshooting: Arc<dyn TroubleshootingRepository>,
        troubleshooting_runtime: Arc<dyn AdminTroubleshootingRuntime>,
        remote_stream_playlists: Arc<dyn RemoteStreamPlaylistRuntime>,
        addons: Arc<dyn AddonRuntime>,
        teletext_profiles: Arc<dyn TeletextProfileRepository>,
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
            addon_media_lists,
            addon_media_list_lifecycle,
            addon_media_list_source_probes,
            admin_settings,
            settings_runtime,
            collections,
            saved_searches,
            ffmpeg_profiles,
            fillers,
            graphics,
            watermarks,
            templates,
            blocks,
            decos,
            program_schedules,
            playouts,
            libraries,
            managed_library_operations,
            media_sources,
            media_items,
            admin_media,
            television,
            admin_health_runtime,
            troubleshooting,
            troubleshooting_runtime,
            remote_stream_playlists,
            addons,
            teletext_profiles,
            teletext_designs: None,
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

    /// Add-on media list persistence and synchronizer adapter.
    #[must_use]
    pub fn addon_media_lists(&self) -> &(dyn AddonMediaListRepository + 'static) {
        self.addon_media_lists.as_ref()
    }

    /// Add-on media-list revision, impact, and lifecycle persistence.
    #[must_use]
    pub fn addon_media_list_lifecycle(&self) -> &(dyn AddonMediaListLifecycleRepository + 'static) {
        self.addon_media_list_lifecycle.as_ref()
    }

    /// Durable, non-mutating add-on media-list source probes.
    #[must_use]
    pub fn addon_media_list_source_probes(
        &self,
    ) -> &(dyn AddonMediaListSourceProbeRepository + 'static) {
        self.addon_media_list_source_probes.as_ref()
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

    /// Reusable advanced-search persistence.
    #[must_use]
    pub fn saved_searches(&self) -> &(dyn SavedSearchRepository + 'static) {
        self.saved_searches.as_ref()
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

    /// Graphics-element persistence used by the text graphics admin slice.
    #[must_use]
    pub fn graphics(&self) -> &(dyn GraphicsRepository + 'static) {
        self.graphics.as_ref()
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

    /// Durable admission, lease, and publication-recovery persistence.
    #[must_use]
    pub fn managed_library_operations(&self) -> &(dyn ManagedLibraryOperationRepository + 'static) {
        self.managed_library_operations.as_ref()
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

    /// Native Remote Stream playlist administration adapter.
    #[must_use]
    pub fn remote_stream_playlists(&self) -> &(dyn RemoteStreamPlaylistRuntime + 'static) {
        self.remote_stream_playlists.as_ref()
    }

    /// Signed add-on repository and package lifecycle adapter.
    #[must_use]
    pub fn addons(&self) -> &(dyn AddonRuntime + 'static) {
        self.addons.as_ref()
    }

    /// Named teletext profiles and channel-scoped selection controls.
    #[must_use]
    pub fn teletext_profiles(&self) -> &(dyn TeletextProfileRepository + 'static) {
        self.teletext_profiles.as_ref()
    }

    /// Attach the native host's durable draft repository.
    /// An absent port reports unavailable; it never stores drafts elsewhere.
    #[must_use]
    pub fn with_teletext_designs(mut self, repository: Arc<dyn TeletextDesignRepository>) -> Self {
        self.teletext_designs = Some(repository);
        self
    }

    /// Durable original drafts; stored data does not establish output readiness.
    #[must_use]
    pub fn teletext_designs(&self) -> Option<&(dyn TeletextDesignRepository + 'static)> {
        self.teletext_designs.as_deref()
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
            .field("addon_media_lists", &"dyn AddonMediaListRepository")
            .field("admin_settings", &"dyn AdminSettingsRepository")
            .field("settings_runtime", &"dyn SettingsRuntime")
            .field("collections", &"dyn CollectionRepository")
            .field("saved_searches", &"dyn SavedSearchRepository")
            .field("ffmpeg_profiles", &"dyn FfmpegProfileRepository")
            .field("fillers", &"dyn FillerRepository")
            .field("watermarks", &"dyn WatermarkRepository")
            .field("templates", &"dyn TemplateRepository")
            .field("blocks", &"dyn BlockRepository")
            .field("decos", &"dyn DecoRepository")
            .field("program_schedules", &"dyn ProgramScheduleRepository")
            .field("playouts", &"dyn PlayoutRepository")
            .field("libraries", &"dyn LibraryRepository")
            .field(
                "managed_library_operations",
                &"dyn ManagedLibraryOperationRepository",
            )
            .field("media_sources", &"dyn MediaSourceRepository")
            .field("media_items", &"dyn MediaItemRepository")
            .field("admin_media", &"dyn AdminMediaRepository")
            .field(
                "remote_stream_playlists",
                &"dyn RemoteStreamPlaylistRuntime",
            )
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
