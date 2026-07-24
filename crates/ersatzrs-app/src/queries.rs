//! Application query requests and read models.

use crate::commands::{ScanOperationId, ScanRequest};
use crate::operations::OperationKey;
use ersatzrs_domain::EntityId;

/// Request the current state of one long-running operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GetOperationStatus {
    /// Stable operation identity to inspect.
    pub key: OperationKey,
}

impl GetOperationStatus {
    /// Create an operation-status query.
    #[must_use]
    pub const fn new(key: OperationKey) -> Self {
        Self { key }
    }
}

/// Application-level state returned for an operation query.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OperationStatus {
    /// No worker currently owns the operation lock.
    Idle,
    /// A caller or worker currently owns the operation lock.
    Running,
}

/// Lifecycle state for one accepted scan operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScanOperationState {
    /// Accepted work is waiting in the FIFO queue.
    Queued,
    /// The sole scanner worker is executing the request.
    Running,
    /// The request completed successfully and the library is idle.
    Completed,
    /// The request failed; the diagnostic remains observable after cleanup.
    Failed {
        /// Storage-neutral worker or adapter diagnostic.
        message: String,
    },
}

/// Application read model for one scan operation.
#[derive(Clone, Debug, PartialEq)]
pub struct ScanOperationSnapshot {
    /// Stable application operation identity.
    pub operation_id: ScanOperationId,
    /// Validated request accepted for execution.
    pub request: ScanRequest,
    /// Current or terminal lifecycle state.
    pub state: ScanOperationState,
    /// Active fractional progress in `[0.0, 1.0]`.
    ///
    /// Terminal operations never retain progress.
    pub progress: Option<f32>,
}

/// Aggregate library status used by polling adapters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LibraryScanStatus {
    /// No operation has been observed for this library.
    Idle,
    /// At least one accepted request is waiting.
    Queued,
    /// A request currently owns the shared library exclusion boundary.
    Running,
    /// The latest request completed and the library is idle.
    Completed,
    /// The latest request failed and the library is idle.
    Failed {
        /// Storage-neutral worker or adapter diagnostic.
        message: String,
    },
}

/// Query one library's aggregate scan state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GetLibraryScanStatus {
    /// Persisted library identity.
    pub library_id: EntityId,
}

impl GetLibraryScanStatus {
    /// Create a library scan status query.
    #[must_use]
    pub const fn new(library_id: EntityId) -> Self {
        Self { library_id }
    }
}
