use ersatzrs_domain::EntityId;

/// Storage-neutral failure vocabulary shared by repository ports.
///
/// Concrete adapters translate database, provider, and row-decoding errors
/// into these variants before returning across the core boundary.
#[derive(Debug, thiserror::Error)]
pub enum RepositoryError {
    /// The requested aggregate does not exist.
    #[error("{kind} with id {id} not found")]
    NotFound { kind: &'static str, id: EntityId },
    /// The requested mutation conflicts with current persisted state.
    #[error("conflict: {0}")]
    Conflict(String),
    /// Persisted or provider data could not be decoded into the port type.
    #[error("failed to decode {kind}: {value}")]
    Decode { kind: &'static str, value: String },
    /// The backing storage or provider could not complete an operation.
    #[error("{operation} is unavailable: {message}")]
    Unavailable {
        operation: &'static str,
        message: String,
    },
}

impl RepositoryError {
    /// Construct an unavailable-storage failure without exposing an adapter
    /// error type through the repository contract.
    #[must_use]
    pub fn unavailable(operation: &'static str, message: impl Into<String>) -> Self {
        Self::Unavailable {
            operation,
            message: message.into(),
        }
    }
}

/// Result returned by storage-neutral repository ports.
pub type Result<T> = std::result::Result<T, RepositoryError>;

#[cfg(test)]
mod tests {
    use super::RepositoryError;

    #[test]
    fn unavailable_error_keeps_operation_context_without_adapter_types() {
        let error = RepositoryError::unavailable("load channels", "connection closed");

        assert_eq!(
            error.to_string(),
            "load channels is unavailable: connection closed"
        );
        assert!(matches!(
            error,
            RepositoryError::Unavailable {
                operation: "load channels",
                ..
            }
        ));
    }
}
