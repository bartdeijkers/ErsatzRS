//! Durable effective originals and historical previews, without promotion authority.

use std::{fmt, fmt::Write as _, sync::Arc, time::Instant};

use async_trait::async_trait;
use ersatzrs_app::{
    teletext_design_documents::{
        decode_teletext_effective_design_documents,
        encode_teletext_effective_design_documents_with_requests,
        EncodedEffectiveTeletextDesignDocuments, TeletextDesignDocumentBudget,
        TeletextEffectiveDesignDocumentInput, TeletextRestoredEffectiveDesignDocumentParts,
        TeletextSavedDocumentSource, TeletextSavedEditPolicy, TeletextSavedGuideInputs,
        TeletextSavedResourceTransmission, TeletextSavedResourceUpdate, TeletextSavedV1Region,
    },
    teletext_designs::{
        TeletextEffectiveOriginalDesignValidator, TeletextOriginalDesignValidationError,
    },
    ApplicationServices,
};
use ersatzrs_core::{
    locking::TeletextWorkHold,
    repositories::{TeletextDesignMutation, TeletextGuideSourceFence},
};
use ersatzrs_domain::{
    EntityId, TeletextDesignHead, TeletextDesignRevision,
    TeletextEffectiveChannelSelectionSnapshot, TeletextProducerCompositionMetadata,
    TeletextProducerPresentation,
};
use ersatzrs_ffmpeg::teletext::{
    EncodedTeletextAuthoredService, TeletextEncodingRegion, TeletextResourceFullUpdate,
    TeletextServiceResourceTransmission,
};
use sha2::{Digest, Sha256};

use super::{
    teletext_channel_documents::{
        encode_original_producer_documents_with_operands, maximum_producers,
        EncodedChannelTeletextOriginals, TeletextChannelDocumentEncodingPolicy,
    },
    teletext_designs::NativeTeletextDesignError,
    teletext_effective_channel_documents::EncodedEffectiveChannelTeletextDocumentCandidate,
};

/// Immutable saved content and its privately constructed complete native candidate.
/// Saving neither requests publication nor changes accepted or running output.
pub struct SavedEffectiveOriginalDesign {
    head: TeletextDesignHead,
    candidate: EncodedEffectiveChannelTeletextDocumentCandidate,
    digest: String,
}
impl SavedEffectiveOriginalDesign {
    #[must_use]
    pub const fn head(&self) -> &TeletextDesignHead {
        &self.head
    }
    #[must_use]
    pub const fn candidate(&self) -> &EncodedEffectiveChannelTeletextDocumentCandidate {
        &self.candidate
    }
    #[must_use]
    pub fn digest(&self) -> &str {
        &self.digest
    }
}
impl fmt::Debug for SavedEffectiveOriginalDesign {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SavedEffectiveOriginalDesign")
            .field("head", &self.head)
            .finish_non_exhaustive()
    }
}

/// Historical content, independently compiled without invoking profiles or Lua.
/// Its retained expired scope is descriptive and cannot authorize live publication.
pub struct RestoredEffectiveOriginalDesign {
    design_id: EntityId,
    revision: i64,
    selection: Arc<TeletextEffectiveChannelSelectionSnapshot>,
    producer_bindings: Vec<usize>,
    generation: String,
    entry_addon_id: Option<String>,
    sources: Vec<TeletextSavedDocumentSource>,
    guide_inputs: Option<TeletextSavedGuideInputs>,
    guide_source: Option<TeletextGuideSourceFence>,
    digest: String,
    originals: EncodedChannelTeletextOriginals,
    edit_policy: TeletextSavedEditPolicy,
}
impl RestoredEffectiveOriginalDesign {
    #[must_use]
    pub(super) fn into_encoded_service(self) -> Option<EncodedTeletextAuthoredService> {
        self.originals.encoded
    }
    #[must_use]
    pub const fn design_id(&self) -> EntityId {
        self.design_id
    }
    #[must_use]
    pub const fn revision(&self) -> i64 {
        self.revision
    }
    #[must_use]
    pub fn selection(&self) -> &TeletextEffectiveChannelSelectionSnapshot {
        &self.selection
    }
    #[must_use]
    pub fn producer_bindings(&self) -> &[usize] {
        &self.producer_bindings
    }
    #[must_use]
    pub fn generation(&self) -> &str {
        &self.generation
    }
    #[must_use]
    pub fn entry_addon_id(&self) -> Option<&str> {
        self.entry_addon_id.as_deref()
    }
    #[must_use]
    pub fn sources(&self) -> &[TeletextSavedDocumentSource] {
        &self.sources
    }
    #[must_use]
    pub const fn guide_inputs(&self) -> Option<&TeletextSavedGuideInputs> {
        self.guide_inputs.as_ref()
    }
    #[must_use]
    pub const fn guide_source(&self) -> Option<&TeletextGuideSourceFence> {
        self.guide_source.as_ref()
    }
    #[must_use]
    pub fn digest(&self) -> &str {
        &self.digest
    }
    #[must_use]
    pub const fn encoded(&self) -> Option<&EncodedTeletextAuthoredService> {
        self.originals.encoded.as_ref()
    }
    #[must_use]
    pub fn producers(&self) -> &[TeletextProducerPresentation] {
        &self.originals.producers
    }
    #[must_use]
    pub const fn metadata(&self) -> Option<&TeletextProducerCompositionMetadata> {
        self.originals.metadata.as_ref()
    }
    #[must_use]
    pub const fn v1_region(&self) -> Option<TeletextEncodingRegion> {
        self.originals.v1_region
    }
    #[must_use]
    pub fn transmissions(&self) -> &[TeletextServiceResourceTransmission] {
        &self.originals.transmissions
    }
    #[must_use]
    pub const fn edit_policy(&self) -> TeletextSavedEditPolicy {
        self.edit_policy
    }
}
impl fmt::Debug for RestoredEffectiveOriginalDesign {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RestoredEffectiveOriginalDesign")
            .field("documents", &self.sources.len())
            .finish_non_exhaustive()
    }
}

fn check_deadline(deadline: Instant) -> Result<(), NativeTeletextDesignError> {
    if Instant::now() >= deadline {
        Err(NativeTeletextDesignError::TimedOut)
    } else {
        Ok(())
    }
}
fn digest(bytes: &[u8]) -> String {
    let mut result = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        let _ = write!(result, "{byte:02x}");
    }
    result
}

// Only the complete privately constructed native candidate creates this witness.
struct NativeEffectiveConfirmation {
    digest: String,
    deadline: Instant,
    hold: TeletextWorkHold,
}
pub(super) fn native_effective_confirmation(
    digest: String,
    deadline: Instant,
    hold: TeletextWorkHold,
) -> impl TeletextEffectiveOriginalDesignValidator {
    NativeEffectiveConfirmation {
        digest,
        deadline,
        hold,
    }
}
#[async_trait]
impl TeletextEffectiveOriginalDesignValidator for NativeEffectiveConfirmation {
    async fn confirm_original(
        &self,
        document: EncodedEffectiveTeletextDesignDocuments,
    ) -> Result<EncodedEffectiveTeletextDesignDocuments, TeletextOriginalDesignValidationError>
    {
        let expected = self.digest.clone();
        let deadline = self.deadline;
        super::teletext_work::spawn_blocking(self.hold.clone(), move || {
            if Instant::now() >= deadline {
                return Err(TeletextOriginalDesignValidationError::TimedOut);
            }
            if digest(document.bytes()) != expected {
                return Err(TeletextOriginalDesignValidationError::Association);
            }
            if Instant::now() >= deadline {
                return Err(TeletextOriginalDesignValidationError::TimedOut);
            }
            Ok(document)
        })
        .await
        .map_err(|_| TeletextOriginalDesignValidationError::WorkerFailed)?
    }
}

/// Save complete native-qualified effective originals without renewing preparation.
/// Repository settlement retains its atomic transaction contract; a committed save
/// is returned even if settlement outlasts the original deadline.
/// # Errors
/// Rejects expired candidates, bounds, serialization or native/storage failures.
pub async fn save_effective_original_teletext_design_candidate(
    application: &ApplicationServices,
    name: String,
    existing: Option<(EntityId, i64)>,
    candidate: EncodedEffectiveChannelTeletextDocumentCandidate,
    budget: TeletextDesignDocumentBudget,
    deadline: Instant,
) -> Result<TeletextDesignMutation<SavedEffectiveOriginalDesign>, NativeTeletextDesignError> {
    let deadline = deadline.min(candidate.deadline());
    check_deadline(deadline)?;
    let guard = super::teletext_work::begin(application, Some(deadline)).await?;
    save_effective_original_teletext_design_candidate_with_hold(
        application,
        name,
        existing,
        candidate,
        budget,
        deadline,
        guard.hold_work(),
    )
    .await
}

pub(super) async fn save_effective_original_teletext_design_candidate_with_hold(
    application: &ApplicationServices,
    name: String,
    existing: Option<(EntityId, i64)>,
    candidate: EncodedEffectiveChannelTeletextDocumentCandidate,
    budget: TeletextDesignDocumentBudget,
    deadline: Instant,
    hold: TeletextWorkHold,
) -> Result<TeletextDesignMutation<SavedEffectiveOriginalDesign>, NativeTeletextDesignError> {
    let deadline = deadline.min(candidate.deadline());
    check_deadline(deadline)?;
    let (candidate, document, hash) =
        super::teletext_work::spawn_blocking(hold.clone(), move || {
            check_deadline(deadline)?;
            if candidate.transmissions().len() > budget.max_resource_transmissions {
                return Err(NativeTeletextDesignError::LifecycleBudget);
            }
            let transmissions = candidate
                .transmissions()
                .iter()
                .map(|r| TeletextSavedResourceTransmission {
                    resource: r.resource,
                    counter: r.counter,
                    last_subpage: r.last_subpage,
                    update: match r.update {
                        TeletextResourceFullUpdate::Changed => TeletextSavedResourceUpdate::Changed,
                        TeletextResourceFullUpdate::Unchanged => {
                            TeletextSavedResourceUpdate::Unchanged
                        }
                    },
                })
                .collect::<Vec<_>>();
            let input = TeletextEffectiveDesignDocumentInput {
                selection: candidate.selection(),
                producer_bindings: candidate.producer_bindings(),
                generation: candidate.generation(),
                sources: candidate.sources(),
                entry_addon_id: candidate.entry_addon_id(),
                entry: candidate.metadata().map(|m| m.entry),
                v1_region: candidate.v1_region().map(|r| match r {
                    TeletextEncodingRegion::WesternLatin => TeletextSavedV1Region::WesternLatin,
                }),
                resource_transmissions: &transmissions,
            };
            let document = encode_teletext_effective_design_documents_with_requests(
                input,
                candidate.requests(),
                budget,
            )?;
            let hash = digest(document.bytes());
            check_deadline(deadline)?;
            Ok::<_, NativeTeletextDesignError>((candidate, document, hash))
        })
        .await
        .map_err(|_| NativeTeletextDesignError::WorkerFailed)??;
    let confirmation = NativeEffectiveConfirmation {
        digest: hash.clone(),
        deadline,
        hold,
    };
    Ok(
        match application
            .save_effective_original_teletext_design(
                name,
                existing,
                document,
                &confirmation,
                budget.max_serialized_bytes,
            )
            .await?
        {
            TeletextDesignMutation::Saved(head) => {
                TeletextDesignMutation::Saved(SavedEffectiveOriginalDesign {
                    head,
                    candidate,
                    digest: hash,
                })
            }
            TeletextDesignMutation::Rejected(reason) => TeletextDesignMutation::Rejected(reason),
        },
    )
}

/// Reload immutable effective originals with a new bounded historical operation.
/// Current profile health and saved interval expiry do not prevent historical
/// preview. Reload does not invoke packages, replay Lua or confer live authority.
/// # Errors
/// Missing revision returns None; malformed, bounded or compiler failures are errors.
pub async fn reload_effective_original_teletext_design_revision(
    application: &ApplicationServices,
    id: EntityId,
    revision: i64,
    budget: TeletextDesignDocumentBudget,
    policy: TeletextChannelDocumentEncodingPolicy,
    deadline: Instant,
) -> Result<Option<RestoredEffectiveOriginalDesign>, NativeTeletextDesignError> {
    check_deadline(deadline)?;
    let guard = super::teletext_work::begin(application, Some(deadline)).await?;
    reload_effective_original_teletext_design_revision_with_hold(
        application,
        id,
        revision,
        budget,
        policy,
        deadline,
        guard.hold_work(),
    )
    .await
}

pub(super) async fn reload_effective_original_teletext_design_revision_with_hold(
    application: &ApplicationServices,
    id: EntityId,
    revision: i64,
    budget: TeletextDesignDocumentBudget,
    policy: TeletextChannelDocumentEncodingPolicy,
    deadline: Instant,
    hold: TeletextWorkHold,
) -> Result<Option<RestoredEffectiveOriginalDesign>, NativeTeletextDesignError> {
    let Some((stored, parts)) = read_original_parts(
        application,
        id,
        revision,
        budget,
        &policy,
        (deadline, hold.clone()),
    )
    .await?
    else {
        return Ok(None);
    };
    let restored = Box::pin(restore_parts(
        application,
        stored,
        parts,
        budget,
        policy,
        deadline,
        hold,
    ))
    .await?;
    Ok(Some(restored))
}

/// Live scoped qualification uses the original saved transition boundary.
/// Historical reload above remains independent of current profile/clock state.
pub(super) async fn reload_effective_original_for_publication_with_hold(
    application: &ApplicationServices,
    id: EntityId,
    revision: i64,
    budget: TeletextDesignDocumentBudget,
    policy: TeletextChannelDocumentEncodingPolicy,
    deadline: Instant,
    hold: TeletextWorkHold,
) -> Result<Option<(RestoredEffectiveOriginalDesign, Instant)>, NativeTeletextDesignError> {
    let Some((stored, parts)) = read_original_parts(
        application,
        id,
        revision,
        budget,
        &policy,
        (deadline, hold.clone()),
    )
    .await?
    else {
        return Ok(None);
    };
    if !parts.selection.bindings.iter().any(|b| {
        matches!(
            b,
            ersatzrs_domain::TeletextEffectiveBindingSnapshot::Scripted(_)
        )
    }) {
        return Err(context_error());
    }
    let boundary = parts.selection.valid_until.ok_or_else(context_error)?;
    let now = Instant::now();
    let remaining = (boundary - chrono::Utc::now())
        .to_std()
        .map_err(|_| NativeTeletextDesignError::TimedOut)?;
    let deadline = deadline
        .min(now + remaining.min(ersatzrs_app::teletext_channel_documents::PREPARATION_TIMEOUT));
    check_deadline(deadline)?;
    application.recheck_effective_channel_teletext_document_selection(&parts.selection,deadline).await
        .map_err(|error|NativeTeletextDesignError::Encoding(Box::new(super::teletext_channel_documents::ChannelTeletextDocumentEncodingError::Preparation(error))))?;
    let restored = Box::pin(restore_parts(
        application,
        stored,
        parts,
        budget,
        policy,
        deadline,
        hold,
    ))
    .await?;
    check_deadline(deadline)?;
    Ok(Some((restored, deadline)))
}

fn context_error() -> NativeTeletextDesignError {
    NativeTeletextDesignError::Document(
        ersatzrs_app::teletext_design_documents::TeletextDesignDocumentError {
            binding: None,
            reason: ersatzrs_app::teletext_design_documents::TeletextDesignDocumentFailure::Context,
        },
    )
}

async fn read_original_parts(
    application: &ApplicationServices,
    id: EntityId,
    revision: i64,
    mut budget: TeletextDesignDocumentBudget,
    policy: &TeletextChannelDocumentEncodingPolicy,
    admission: (Instant, TeletextWorkHold),
) -> Result<
    Option<(
        TeletextDesignRevision,
        TeletextRestoredEffectiveDesignDocumentParts,
    )>,
    NativeTeletextDesignError,
> {
    let (deadline, hold) = admission;
    check_deadline(deadline)?;
    budget.max_documents = budget.max_documents.min(maximum_producers(policy));
    let Some(stored) = application
        .teletext_design_revision(id, revision, budget.max_serialized_bytes)
        .await?
    else {
        return Ok(None);
    };
    if stored.document_version != ersatzrs_domain::TELETEXT_EFFECTIVE_DESIGN_DOCUMENT_VERSION {
        return Err(NativeTeletextDesignError::Document(
            ersatzrs_app::teletext_design_documents::TeletextDesignDocumentError {
                binding: None,
                reason:
                    ersatzrs_app::teletext_design_documents::TeletextDesignDocumentFailure::Version,
            },
        ));
    }
    let parts = super::teletext_work::spawn_blocking(hold, move || {
        check_deadline(deadline)?;
        let parts =
            decode_teletext_effective_design_documents(&stored.document, budget)?.into_parts();
        check_deadline(deadline)?;
        Ok::<_, NativeTeletextDesignError>((stored, parts))
    })
    .await
    .map_err(|_| NativeTeletextDesignError::WorkerFailed)??;
    Ok(Some(parts))
}

async fn restore_parts(
    application: &ApplicationServices,
    stored: TeletextDesignRevision,
    parts: TeletextRestoredEffectiveDesignDocumentParts,
    budget: TeletextDesignDocumentBudget,
    policy: TeletextChannelDocumentEncodingPolicy,
    deadline: Instant,
    hold: TeletextWorkHold,
) -> Result<RestoredEffectiveOriginalDesign, NativeTeletextDesignError> {
    let baseline = if let Some(overlay) = &parts.editorial_overlay {
        Some(super::teletext_editorial_designs::read_editorial_baseline(
            application, overlay.provenance(), stored.design_id, Some(stored.revision), budget, &policy, deadline,
        ).await.map_err(|error| match error {
            super::teletext_editorial_designs::NativeTeletextEditorialImportError::MissingReceipt => NativeTeletextDesignError::EditorialValidationUnavailable,
            error => NativeTeletextDesignError::Editorial(Box::new(error)),
        })?)
    } else {
        None
    };
    super::teletext_work::spawn_blocking(hold, move || match baseline {
        Some(baseline) => super::teletext_editorial_designs::restore_editorial_effective_design(
            stored, parts, baseline, budget, policy, deadline,
        )
        .map_err(|error| NativeTeletextDesignError::Editorial(Box::new(error))),
        None => compile_effective_restored_parts(stored, parts, policy, deadline, Vec::new()),
    })
    .await
    .map_err(|_| NativeTeletextDesignError::WorkerFailed)?
}

/// Physical overlays must already have passed retained-receipt and effect checks.
pub(super) fn compile_effective_restored_parts(
    stored: TeletextDesignRevision,
    parts: TeletextRestoredEffectiveDesignDocumentParts,
    mut policy: TeletextChannelDocumentEncodingPolicy,
    deadline: Instant,
    overlays: Vec<ersatzrs_domain::TeletextPhysicalBaseOverlay>,
) -> Result<RestoredEffectiveOriginalDesign, NativeTeletextDesignError> {
    check_deadline(deadline)?;
    if parts
        .editorial_overlay
        .as_ref()
        .is_some_and(|overlay| !overlay.pages().is_empty() && overlays.is_empty())
    {
        return Err(NativeTeletextDesignError::EditorialValidationUnavailable);
    }
    policy.entry = parts.entry;
    policy.transmissions = parts
        .resource_transmissions
        .iter()
        .map(|r| TeletextServiceResourceTransmission {
            resource: r.resource,
            counter: r.counter,
            last_subpage: r.last_subpage,
            update: match r.update {
                TeletextSavedResourceUpdate::Changed => TeletextResourceFullUpdate::Changed,
                TeletextSavedResourceUpdate::Unchanged => TeletextResourceFullUpdate::Unchanged,
            },
        })
        .collect();
    let region = parts.v1_region.map(|r| match r {
        TeletextSavedV1Region::WesternLatin => TeletextEncodingRegion::WesternLatin,
    });
    let entry = parts.entry_addon_id.as_deref().and_then(|addon| {
        parts
            .producer_bindings
            .iter()
            .position(|index| parts.selection.bindings[*index].addon_id() == addon)
    });
    let operands = parts
        .editorial_overlay
        .as_ref()
        .map_or(&[][..], |overlay| overlay.local_program_operands());
    let originals = encode_original_producer_documents_with_operands(
        entry,
        parts.documents,
        overlays,
        deadline,
        region,
        policy,
        operands,
    )
    .map_err(|e| NativeTeletextDesignError::Encoding(Box::new(e)))?;
    check_deadline(deadline)?;
    Ok(RestoredEffectiveOriginalDesign {
        design_id: stored.design_id,
        revision: stored.revision,
        selection: Arc::new(parts.selection),
        producer_bindings: parts.producer_bindings,
        generation: parts.generation,
        entry_addon_id: parts.entry_addon_id,
        sources: parts.sources,
        guide_inputs: parts.guide_inputs,
        guide_source: parts.guide_source,
        digest: stored.digest,
        originals,
        edit_policy: parts.edit_policy,
    })
}
