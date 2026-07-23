//! Injected core ports used by application services.

use std::sync::Arc;

use ersatzrs_core::repositories::{LibraryRepository, PlayoutRepository};
use ersatzrs_core::scheduling::PlayoutBuildRequest;

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

/// Bounded repository port set for the next playout and scan vertical slices.
#[derive(Clone)]
pub struct ApplicationPorts {
    playouts: Arc<dyn PlayoutRepository>,
    libraries: Arc<dyn LibraryRepository>,
    playout_builds: Arc<dyn PlayoutBuildQueue>,
}

impl ApplicationPorts {
    /// Construct the port set from adapter implementations.
    #[must_use]
    pub fn new(
        playouts: Arc<dyn PlayoutRepository>,
        libraries: Arc<dyn LibraryRepository>,
        playout_builds: Arc<dyn PlayoutBuildQueue>,
    ) -> Self {
        Self {
            playouts,
            libraries,
            playout_builds,
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

    /// Scheduler work queue used by the playout-build command.
    #[must_use]
    pub fn playout_builds(&self) -> &(dyn PlayoutBuildQueue + 'static) {
        self.playout_builds.as_ref()
    }
}

impl std::fmt::Debug for ApplicationPorts {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ApplicationPorts")
            .field("playouts", &"dyn PlayoutRepository")
            .field("libraries", &"dyn LibraryRepository")
            .field("playout_builds", &"dyn PlayoutBuildQueue")
            .finish()
    }
}
