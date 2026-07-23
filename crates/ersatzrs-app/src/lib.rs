//! Storage- and transport-neutral application services.
//!
//! Commands and queries enter through this crate, which coordinates core
//! ports without depending on SQL, HTTP, scheduler implementations, `FFmpeg`,
//! or platform adapters. The native web host constructs [`ApplicationServices`]
//! from concrete infrastructure ports.

pub mod commands;
pub mod error;
pub mod operations;
pub mod ports;
pub mod queries;

pub use error::{ApplicationError, Result};
pub use operations::OperationCoordinator;
pub use ports::{ApplicationPorts, PlayoutBuildQueue, PlayoutBuildQueueError};

/// Native application services assembled by a composition root.
///
/// The port set is intentionally limited to the playout and library
/// repositories required by the immediately following Phase 7B vertical
/// slices. Additional ports belong beside the use case that first needs them.
#[derive(Clone)]
pub struct ApplicationServices {
    ports: ApplicationPorts,
    operations: OperationCoordinator,
}

impl ApplicationServices {
    /// Construct the application container from injected core ports and the
    /// process-wide operation coordinator.
    #[must_use]
    pub fn new(ports: ApplicationPorts, operations: OperationCoordinator) -> Self {
        Self { ports, operations }
    }

    /// Core repository ports used by application use cases.
    #[must_use]
    pub const fn ports(&self) -> &ApplicationPorts {
        &self.ports
    }

    /// Shared process-local coordination for long-running operations.
    #[must_use]
    pub const fn operations(&self) -> &OperationCoordinator {
        &self.operations
    }

    /// Admit one playout build through the single application-owned queue
    /// boundary.
    #[must_use]
    pub fn request_playout_build(
        &self,
        command: commands::RequestPlayoutBuild,
    ) -> commands::RequestPlayoutBuildOutcome {
        commands::request_playout_build(command, self.ports.playout_builds(), &self.operations)
    }
}

impl std::fmt::Debug for ApplicationServices {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ApplicationServices")
            .field("ports", &self.ports)
            .field("operations", &self.operations)
            .finish()
    }
}
