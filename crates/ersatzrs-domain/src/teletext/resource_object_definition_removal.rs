//! Explicit unused-definition removal, without authoring or publication authority.
use super::{TeletextObjectIdentity, TeletextServiceError};

/// A complete removal batch rejected without returning a partially edited service.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TeletextResourceObjectDefinitionRemovalError {
    #[error("teletext definition removals must not be empty")]
    Empty,
    #[error("teletext definition removal {identity:?} is not Public or Global")]
    Scope { identity: TeletextObjectIdentity },
    #[error("teletext definition removal {identity:?} is repeated")]
    Duplicate { identity: TeletextObjectIdentity },
    #[error("teletext definition removal {identity:?} does not exist")]
    Missing { identity: TeletextObjectIdentity },
    #[error("teletext definition removal {identity:?} still has a caller or default")]
    Referenced { identity: TeletextObjectIdentity },
    #[error(transparent)]
    Service(#[from] TeletextServiceError),
}
