//! Durable selected generations with separate native and transactional fences.

use std::{fmt, time::Instant};

use async_trait::async_trait;
use ersatzrs_domain::{
    EntityId, TeletextAcceptedGeneration, TeletextChannelSelectionSnapshot,
    TeletextEffectiveChannelSelectionSnapshot, TeletextPublicationRequest,
};

use super::{RepositoryError, Result, TeletextPublicationRejection};

/// Latest native compiled-artifact fingerprint framing, independent of documents.
pub const TELETEXT_GENERATION_ARTIFACT_VERSION: u16 = 1;
/// Scoped artifact framing includes an explicit authored-service/absence role.
pub const TELETEXT_EFFECTIVE_GENERATION_ARTIFACT_VERSION: u16 = 2;

/// Fresh promotion target and independent accepted-pointer CAS.
#[derive(Clone, PartialEq, Eq)]
pub struct TeletextGenerationSave {
    /// Exact current pending intent, including immutable target digest.
    pub request: TeletextPublicationRequest,
    /// Live design head observed by this explicit publication operation.
    pub expected_design_head: i64,
    /// Independent accepted-selection CAS; None requires no accepted generation.
    pub expected_generation_id: Option<EntityId>,
    /// Whole original effective channel selection, not one profile alone.
    pub selection: TeletextChannelSelectionSnapshot,
    /// Native compiled-service fingerprint framing version.
    pub artifact_version: u16,
    /// Complete native wire/service fingerprint, redacted from diagnostics.
    pub artifact_digest: String,
}

impl fmt::Debug for TeletextGenerationSave {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TeletextGenerationSave")
            .field("request", &self.request)
            .field("expected_design_head", &self.expected_design_head)
            .field("expected_generation_id", &self.expected_generation_id)
            .field("binding_count", &self.selection.bindings.len())
            .finish_non_exhaustive()
    }
}

/// Explicit scoped promotion, independent of the accepted base pointer.
/// Whole membership includes non-producing Off and captured transition expiry.
#[derive(Clone, PartialEq, Eq)]
pub struct TeletextEffectiveGenerationSave {
    pub request: TeletextPublicationRequest,
    pub expected_design_head: i64,
    pub expected_generation_id: Option<EntityId>,
    pub selection: TeletextEffectiveChannelSelectionSnapshot,
    pub artifact_version: u16,
    pub artifact_digest: String,
}

impl fmt::Debug for TeletextEffectiveGenerationSave {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TeletextEffectiveGenerationSave")
            .field("request", &self.request)
            .field("expected_design_head", &self.expected_design_head)
            .field("expected_generation_id", &self.expected_generation_id)
            .field("channel_id", &self.selection.channel_id)
            .field("binding_count", &self.selection.bindings.len())
            .field("valid_until", &self.selection.valid_until)
            .finish_non_exhaustive()
    }
}

/// Explicit no-write outcomes; rejected promotion preserves old selected output.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TeletextGenerationRejection {
    /// Existing exact design, pending-request or channel fence failed.
    Publication(TeletextPublicationRejection),
    /// Accepted-pointer CAS differs from the submitted independent token.
    GenerationConflict,
    /// This immutable pending request already has an accepted generation.
    AlreadyAccepted,
    /// Missing current-role/freshness proof; historical source data is insufficient.
    GuideFreshnessRequired,
}

/// Accepted selection is not an aggregate sink/receiver acknowledgement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TeletextGenerationMutation<T> {
    /// Durable output selection; actual stream admission remains separate.
    Accepted(T),
    /// No accepted history or pointer changed.
    Rejected(TeletextGenerationRejection),
}

/// Atomic durable publication and exact read-only runtime revalidation.
#[async_trait]
pub trait TeletextGenerationRepository: Send + Sync {
    /// Accept scoped originals after native proof under independent scoped CAS.
    /// The write transaction repeats exact current scope/expiry, version-2
    /// revision, request and artifact fences. Admission uses the deadline;
    /// admitted commit settlement remains truthful without altering base output.
    /// # Errors
    /// Returns unsupported/storage failures without base promotion fallback.
    async fn accept_effective(
        &self,
        _save: &TeletextEffectiveGenerationSave,
        _max_document_bytes: usize,
        _deadline: Instant,
    ) -> Result<TeletextGenerationMutation<TeletextAcceptedGeneration>> {
        Err(RepositoryError::unavailable(
            "effective teletext generation acceptance",
            "effective teletext generation selection is unsupported",
        ))
    }

    /// Retained scoped accepted metadata only; expiry/current scope is separate.
    /// # Errors
    /// Returns unsupported/storage failures; pending/base records never substitute.
    async fn current_effective(
        &self,
        _channel_id: EntityId,
    ) -> Result<Option<TeletextAcceptedGeneration>> {
        Err(RepositoryError::unavailable(
            "effective teletext generation read",
            "effective teletext generation selection is unsupported",
        ))
    }

    /// Recheck exact scoped pointer, revision/digest and current effective scope
    /// including interval expiry in one read transaction. Native artifact proof
    /// remains the caller's duty; no expired generation becomes a base fallback.
    /// # Errors
    /// Returns unsupported/storage failures or deadline admission failure.
    async fn confirm_current_effective(
        &self,
        _generation: &TeletextAcceptedGeneration,
        _selection: &TeletextEffectiveChannelSelectionSnapshot,
        _max_document_bytes: usize,
        _deadline: Instant,
    ) -> Result<TeletextGenerationMutation<()>> {
        Err(RepositoryError::unavailable(
            "effective teletext generation confirmation",
            "effective teletext generation selection is unsupported",
        ))
    }

    /// Accept after trusted native confirmation; recheck current pending request,
    /// design head/revision/digest, complete selection and accepted-pointer CAS
    /// in one immediate transaction. Dynamic guide content needs fresh role proof.
    async fn accept(
        &self,
        save: &TeletextGenerationSave,
        max_document_bytes: usize,
    ) -> Result<TeletextGenerationMutation<TeletextAcceptedGeneration>>;

    /// Retained selected metadata only, never sufficient runtime authority.
    /// Corruption is an error, not an absent generation or pending fallback.
    async fn current(&self, channel_id: EntityId) -> Result<Option<TeletextAcceptedGeneration>>;

    /// One read snapshot rechecks the exact accepted pointer, revision/digest and
    /// full effective channel selection. New drafts/pending intents do not alter
    /// accepted output. Missing native artifact proof remains the caller's duty.
    async fn confirm_current(
        &self,
        generation: &TeletextAcceptedGeneration,
        selection: &TeletextChannelSelectionSnapshot,
        max_document_bytes: usize,
    ) -> Result<TeletextGenerationMutation<()>>;
}
