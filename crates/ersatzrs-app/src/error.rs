//! Typed failures returned by application use cases.

use ersatzrs_core::repositories::RepositoryError;
use ersatzrs_domain::EntityId;

/// Storage- and transport-neutral application failure.
#[derive(Debug, thiserror::Error)]
pub enum ApplicationError {
    /// A request field failed use-case validation.
    #[error("invalid {field}: {message}")]
    Validation {
        field: &'static str,
        message: String,
    },
    /// A requested aggregate does not exist.
    #[error("{kind} with id {id} not found")]
    NotFound { kind: &'static str, id: EntityId },
    /// Current persisted state conflicts with the command.
    #[error("conflict: {0}")]
    Conflict(String),
    /// Persisted or provider data is invalid for the requested use case.
    #[error("invalid {kind}: {value}")]
    InvalidData { kind: &'static str, value: String },
    /// A required repository or provider is operationally unavailable.
    #[error("{operation} is unavailable: {message}")]
    Unavailable {
        operation: &'static str,
        message: String,
    },
}

impl From<RepositoryError> for ApplicationError {
    fn from(error: RepositoryError) -> Self {
        match error {
            RepositoryError::NotFound { kind, id } => Self::NotFound { kind, id },
            RepositoryError::Conflict(message) => Self::Conflict(message),
            RepositoryError::Decode { kind, value } => Self::InvalidData { kind, value },
            RepositoryError::Unavailable { operation, message } => {
                Self::Unavailable { operation, message }
            }
        }
    }
}

/// Result returned by application commands and queries.
pub type Result<T> = std::result::Result<T, ApplicationError>;

#[cfg(test)]
mod tests {
    use super::ApplicationError;
    use ersatzrs_core::repositories::RepositoryError;

    #[test]
    fn repository_failures_keep_their_application_category() {
        let cases = [
            RepositoryError::NotFound {
                kind: "playout",
                id: 7,
            },
            RepositoryError::Conflict("already queued".to_owned()),
            RepositoryError::Decode {
                kind: "playout schedule",
                value: "99".to_owned(),
            },
            RepositoryError::unavailable("load playout", "connection closed"),
        ];

        assert!(matches!(
            ApplicationError::from(cases.into_iter().next().expect("not found case")),
            ApplicationError::NotFound { id: 7, .. }
        ));
        assert!(matches!(
            ApplicationError::from(RepositoryError::Conflict("busy".to_owned())),
            ApplicationError::Conflict(_)
        ));
        assert!(matches!(
            ApplicationError::from(RepositoryError::Decode {
                kind: "channel",
                value: "invalid".to_owned(),
            }),
            ApplicationError::InvalidData { .. }
        ));
        assert!(matches!(
            ApplicationError::from(RepositoryError::unavailable("query", "offline")),
            ApplicationError::Unavailable { .. }
        ));
    }
}
