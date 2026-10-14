//! Atomic pending publication intents; output acceptance is separate (ADR 0123).

use std::{fmt, time::Instant};

use async_trait::async_trait;
use ersatzrs_domain::{
    EntityId, TeletextChannelSelectionSnapshot, TeletextEffectiveChannelSelectionSnapshot,
    TeletextPublicationRequest,
};

use super::{
    RepositoryError, Result, TeletextGuideSourceCheckLimits, TeletextGuideSourceFence,
    TeletextProfileRejection,
};

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

/// Exact effective-original target and independent scoped pending-pointer CAS.
/// Its whole selection retains Scripted Off and interval ownership/expiry;
/// it cannot substitute for base request authority.
#[derive(Clone, PartialEq, Eq)]
pub struct TeletextEffectivePublicationRequestSave {
    pub design_id: EntityId,
    pub revision: i64,
    pub digest: String,
    pub expected_design_head: i64,
    pub expected_request_id: Option<EntityId>,
    pub selection: TeletextEffectiveChannelSelectionSnapshot,
}

impl fmt::Debug for TeletextEffectivePublicationRequestSave {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TeletextEffectivePublicationRequestSave")
            .field("design_id", &self.design_id)
            .field("revision", &self.revision)
            .field("expected_design_head", &self.expected_design_head)
            .field("expected_request_id", &self.expected_request_id)
            .field("channel_id", &self.selection.channel_id)
            .field("binding_count", &self.selection.bindings.len())
            .field("valid_until", &self.selection.valid_until)
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
    /// A saved guide-bound target cannot use the legacy static request path.
    GuideFenceRequired,
    /// Supplied source data differs from the exact stored target association.
    GuideFenceMismatch,
    /// Transactional recapture differs from the saved semantic context.
    GuideChanged,
    /// Source-aware admission is unsupported, unavailable or exceeds its bounds.
    GuideUnavailable,
    /// The fresh source-aware admission operation exceeded its deadline.
    GuideTimedOut,
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
    /// Request version-2 originals under exact current effective scope and an
    /// independent scoped pointer. Check scope/expiry, design and CAS in the
    /// same write transaction. The deadline bounds admission; an admitted commit
    /// retains normal database settlement. No base pointer or binding changes.
    /// # Errors
    /// Returns unsupported/storage failures without falling back to base requests.
    async fn request_effective(
        &self,
        _save: &TeletextEffectivePublicationRequestSave,
        _max_document_bytes: usize,
        _deadline: Instant,
    ) -> Result<TeletextPublicationMutation<TeletextPublicationRequest>> {
        Err(RepositoryError::unavailable(
            "effective teletext publication request",
            "effective teletext publication is unsupported",
        ))
    }

    /// Retain the guide association and same-transaction semantic recapture in
    /// addition to scoped request fences. This proves no current Now/Next role.
    /// # Errors
    /// Returns unsupported/storage failures, never discarding the source fence.
    async fn request_effective_with_source(
        &self,
        _save: &TeletextEffectivePublicationRequestSave,
        _fence: &TeletextGuideSourceFence,
        _limits: TeletextGuideSourceCheckLimits,
        _max_document_bytes: usize,
        _deadline: Instant,
    ) -> Result<TeletextPublicationMutation<TeletextPublicationRequest>> {
        Err(RepositoryError::unavailable(
            "source-bound effective teletext publication request",
            "effective teletext publication is unsupported",
        ))
    }

    /// Retained scoped pending metadata, not active output or an unexpired scope.
    /// # Errors
    /// Returns unsupported/storage failures; corruption is never absent state.
    async fn current_effective(
        &self,
        _channel_id: EntityId,
    ) -> Result<Option<TeletextPublicationRequest>> {
        Err(RepositoryError::unavailable(
            "effective teletext publication request read",
            "effective teletext publication is unsupported",
        ))
    }

    /// Persist an intent after complete native validation by the application.
    /// One immediate transaction verifies the live design head, bounded exact
    /// immutable revision/digest, canonical full channel selection and request
    /// CAS, then writes the immutable request and pointer. No output is changed.
    async fn request(
        &self,
        save: &TeletextPublicationRequestSave,
        max_document_bytes: usize,
    ) -> Result<TeletextPublicationMutation<TeletextPublicationRequest>>;

    /// Request a guide-bound target after native validation of its association.
    /// The implementation must compare the exact stored fence and recapture
    /// matching semantic context inside the same immediate transaction as the
    /// design, selection and request CAS. This does not establish raw-source or
    /// current Now/Next freshness. No output is activated.
    /// The new deadline bounds admission, not a previously saved receipt's age;
    /// an admitted transaction retains the normal database completion contract.
    /// Unsupported repositories fail explicitly, without falling back to request.
    async fn request_with_source(
        &self,
        _save: &TeletextPublicationRequestSave,
        _fence: &TeletextGuideSourceFence,
        _limits: TeletextGuideSourceCheckLimits,
        _max_document_bytes: usize,
        _deadline: Instant,
    ) -> Result<TeletextPublicationMutation<TeletextPublicationRequest>> {
        Ok(TeletextPublicationMutation::Rejected(
            TeletextPublicationRejection::GuideUnavailable,
        ))
    }

    /// Read a retained current pending request, without activating or invoking it.
    /// Corrupt metadata must report an error, never masquerade as absent state.
    async fn current(&self, channel_id: EntityId) -> Result<Option<TeletextPublicationRequest>>;
}
