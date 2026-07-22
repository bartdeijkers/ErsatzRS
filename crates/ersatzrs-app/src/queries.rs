//! Application query requests and read models.

use crate::operations::OperationKey;

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
