//! Injected core ports used by application services.

use std::sync::Arc;

use async_trait::async_trait;
use ersatzrs_core::playout_builder::PlayoutBuilder;
use ersatzrs_core::repositories::{LibraryRepository, PlayoutRepository};
use ersatzrs_core::scheduling::{
    BlockPreviewRepository, PlayoutBuildRequest, SchedulingError, SchedulingRequest,
    SchedulingResult,
};
use ersatzrs_domain::{EntityId, MediaSourceKind, ScanProgressSink};

use crate::commands::ScanWorkRequest;

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

/// Bounded repository port set for the next playout and scan vertical slices.
#[derive(Clone)]
pub struct ApplicationPorts {
    playouts: Arc<dyn PlayoutRepository>,
    libraries: Arc<dyn LibraryRepository>,
    block_previews: Arc<dyn BlockPreviewRepository>,
    block_preview_builder: Arc<dyn BlockPreviewBuilder>,
    playout_builds: Arc<dyn PlayoutBuildQueue>,
    scan_work: Arc<dyn ScanWorkQueue>,
    scan_runtime: Arc<dyn ScanRuntime>,
}

impl ApplicationPorts {
    /// Construct the port set from adapter implementations.
    #[must_use]
    pub fn new(
        playouts: Arc<dyn PlayoutRepository>,
        libraries: Arc<dyn LibraryRepository>,
        block_previews: Arc<dyn BlockPreviewRepository>,
        block_preview_builder: Arc<dyn BlockPreviewBuilder>,
        playout_builds: Arc<dyn PlayoutBuildQueue>,
        scan_work: Arc<dyn ScanWorkQueue>,
        scan_runtime: Arc<dyn ScanRuntime>,
    ) -> Self {
        Self {
            playouts,
            libraries,
            block_previews,
            block_preview_builder,
            playout_builds,
            scan_work,
            scan_runtime,
        }
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
            .field("playouts", &"dyn PlayoutRepository")
            .field("libraries", &"dyn LibraryRepository")
            .field("block_previews", &"dyn BlockPreviewRepository")
            .field("block_preview_builder", &"dyn BlockPreviewBuilder")
            .field("playout_builds", &"dyn PlayoutBuildQueue")
            .field("scan_work", &"dyn ScanWorkQueue")
            .field("scan_runtime", &"dyn ScanRuntime")
            .finish()
    }
}
