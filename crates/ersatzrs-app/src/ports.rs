//! Injected core ports used by application services.

use std::sync::Arc;

use ersatzrs_core::repositories::{LibraryRepository, PlayoutRepository};

/// Bounded repository port set for the next playout and scan vertical slices.
#[derive(Clone)]
pub struct ApplicationPorts {
    playouts: Arc<dyn PlayoutRepository>,
    libraries: Arc<dyn LibraryRepository>,
}

impl ApplicationPorts {
    /// Construct the port set from adapter implementations.
    #[must_use]
    pub fn new(
        playouts: Arc<dyn PlayoutRepository>,
        libraries: Arc<dyn LibraryRepository>,
    ) -> Self {
        Self {
            playouts,
            libraries,
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
}

impl std::fmt::Debug for ApplicationPorts {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ApplicationPorts")
            .field("playouts", &"dyn PlayoutRepository")
            .field("libraries", &"dyn LibraryRepository")
            .finish()
    }
}
