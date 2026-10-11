//! Atomic pending publication intents; output acceptance is separate (ADR 0123).

use std::fmt;

use async_trait::async_trait;
use ersatzrs_domain::{EntityId, TeletextChannelSelectionSnapshot, TeletextPublicationRequest};

use super::{Result, TeletextProfileRejection};

/// Exact target and complete current-selection fence for a pending request.
/// Names, effective settings and digests are excluded from diagnostic formatting.
#[derive(Clone, PartialEq, Eq)]
pub struct TeletextPublicationRequestSave {
    /// Target immutable design identity.
    pub design_id: EntityId,
    /// Target immutable revision, potentially older than the live design head.
    pub revision: i64,
    /// Expected digest of the exact target revision.
    pub digest: String,
    /// Expected live head, independently of the deliberately selected revision.
    pub expected_design_head: i64,
    /// Expected current channel request identity; None requires no prior request.
    pub expected_request_id: Option<EntityId>,
    /// Complete original channel selection, compared to current state in one tx.
    pub selection: TeletextChannelSelectionSnapshot,
}

impl fmt::Debug for TeletextPublicationRequestSave {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TeletextPublicationRequestSave")
            .field("design_id", &self.design_id)
            .field("revision", &self.revision)
            .field("expected_design_head", &self.expected_design_head)
            .field("expected_request_id", &self.expected_request_id)
            .field("channel_id", &self.selection.channel_id)
            .field("binding_count", &self.selection.bindings.len())
            .finish_non_exhaustive()
    }
}

/// A rejected request preserves all current pointers and immutable rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TeletextPublicationRejection {
    /// Invalid identity, digest or empty/invalid target membership.
    InvalidValues,
    /// No such durable design head.
    MissingDesign,
    /// The design is tombstoned.
    DeletedDesign,
    /// The expected live design head is stale.
    DesignConflict,
    /// No such immutable target revision.
    MissingRevision,
    /// The exact target document digest differs.
    DigestConflict,
    /// Complete membership, installation, profile or effective settings changed.
    SelectionChanged,
    /// The channel current-request CAS token differs.
    RequestConflict,
    /// Current configuration cannot supply a complete valid selection.
    SelectionUnavailable(TeletextProfileRejection),
}

/// A persisted pending request does not represent accepted transmission.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TeletextPublicationMutation<T> {
    /// Durable pending intent only; runtime promotion remains separate.
    Requested(T),
    /// Explicit no-write rejection.
    Rejected(TeletextPublicationRejection),
}

/// Pending intents with atomic design, whole-channel and current-request fences.
#[async_trait]
pub trait TeletextPublicationRepository: Send + Sync {
    /// Persist an intent after complete native validation by the application.
    /// One immediate transaction verifies the live design head, bounded exact
    /// immutable revision/digest, canonical full channel selection and request
    /// CAS, then writes the immutable request and pointer. No output is changed.
    async fn request(
        &self,
        save: &TeletextPublicationRequestSave,
        max_document_bytes: usize,
    ) -> Result<TeletextPublicationMutation<TeletextPublicationRequest>>;

    /// Read a retained current pending request, without activating or invoking it.
    /// Corrupt metadata must report an error, never masquerade as absent state.
    async fn current(&self, channel_id: EntityId) -> Result<Option<TeletextPublicationRequest>>;
}
