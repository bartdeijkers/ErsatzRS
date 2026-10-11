//! Durable pending intents, distinct from accepted service generations (ADR 0123).

use std::fmt;

use crate::EntityId;

/// An explicitly requested immutable design revision awaiting output promotion.
/// Persistence alone does not activate transmission. Runtime recovery must
/// revalidate selection, compile the exact revision and reconcile receiver
/// lifecycle before accepting a generation. This record is never an active flag.
#[derive(Clone, PartialEq, Eq)]
pub struct TeletextPublicationRequest {
    /// Monotonic repository identity, also the channel request CAS token.
    pub id: EntityId,
    /// Target channel; independent channels have independent current pointers.
    pub channel_id: EntityId,
    /// Durable original design identity.
    pub design_id: EntityId,
    /// Explicitly selected immutable revision; need not be the latest revision.
    pub revision: i64,
    /// Exact immutable document digest, not a native admission receipt.
    pub digest: String,
}

impl fmt::Debug for TeletextPublicationRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TeletextPublicationRequest")
            .field("id", &self.id)
            .field("channel_id", &self.channel_id)
            .field("design_id", &self.design_id)
            .field("revision", &self.revision)
            .finish_non_exhaustive()
    }
}
