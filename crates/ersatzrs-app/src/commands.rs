//! Application command requests.

use crate::operations::OperationKey;

/// Request to begin one long-running operation under the shared coordinator.
///
/// The concrete playout-build and scan admission commands land in milestones
/// p7b-02 and p7b-03. This command supplies their common exclusion boundary
/// without moving either workflow prematurely.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BeginOperation {
    /// Stable operation identity to acquire.
    pub key: OperationKey,
}

impl BeginOperation {
    /// Create an operation-admission command.
    #[must_use]
    pub const fn new(key: OperationKey) -> Self {
        Self { key }
    }
}
