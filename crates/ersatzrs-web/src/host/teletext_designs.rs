//! Durable, fully compiled originals without active-publication authority.

use std::{fmt, fmt::Write as _, sync::Arc, time::Instant};

use async_trait::async_trait;
use ersatzrs_app::{
    teletext_design_documents::{
        decode_teletext_design_documents, encode_teletext_design_documents_with_requests,
        encode_teletext_design_documents_with_source, EncodedTeletextDesignDocuments,
        TeletextDesignDocumentBudget, TeletextDesignDocumentError, TeletextDesignDocumentInput,
        TeletextRestoredDesignDocumentParts, TeletextSavedDocumentSource, TeletextSavedEditPolicy,
        TeletextSavedGuideInputs, TeletextSavedResourceTransmission, TeletextSavedResourceUpdate,
        TeletextSavedV1Region,
    },
    teletext_designs::{
        TeletextDesignStoreError, TeletextOriginalDesignValidationError,
        TeletextOriginalDesignValidator,
    },
    ApplicationServices,
};
use ersatzrs_core::repositories::{
    TeletextDesignMutation, TeletextGuideSourceCheckLimits, TeletextGuideSourceFence,
};
use ersatzrs_domain::{
    EntityId, TeletextChannelSelectionSnapshot, TeletextDesignHead, TeletextDesignRevision,
    TeletextPhysicalBaseOverlay, TeletextProducerCompositionMetadata, TeletextProducerPresentation,
};
use ersatzrs_ffmpeg::teletext::{
    EncodedTeletextAuthoredService, TeletextEncodingRegion, TeletextResourceFullUpdate,
    TeletextServiceResourceTransmission,
};
use sha2::{Digest, Sha256};

use super::teletext_channel_documents::{
    encode_original_channel_documents_with_editorial_programs,
    encode_original_channel_documents_with_editorial_resources, maximum_producers,
    ChannelTeletextDocumentEncodingError, EditorialResourceDocuments,
    EncodedChannelTeletextDocumentCandidate, EncodedChannelTeletextOriginals,
    TeletextChannelDocumentEncodingPolicy,
};

#[derive(Debug, thiserror::Error)]
pub enum NativeTeletextDesignError {
    #[error(transparent)]
    Work(#[from] super::teletext_work::TeletextWorkError),
    #[error(transparent)]
    Storage(#[from] TeletextDesignStoreError),
    #[error(transparent)]
    Document(#[from] TeletextDesignDocumentError),
    #[error("saved teletext native encoding: {0}")]
    Encoding(Box<ChannelTeletextDocumentEncodingError>),
    #[error("teletext design operation exceeded its deadline")]
    TimedOut,
    #[error("teletext design worker failed")]
    WorkerFailed,
    #[error("teletext resource lifecycle count exceeds storage policy")]
    LifecycleBudget,
    #[error("saved teletext editorial receipt and edit-effect validation is unavailable")]
    EditorialValidationUnavailable,
    #[error("saved teletext editorial validation: {0}")]
    Editorial(Box<super::teletext_editorial_designs::NativeTeletextEditorialImportError>),
}

/// Saved immutable original and its associated complete preview candidate.
/// Saving does not create a generation, enable teletext or change any output.
pub struct SavedOriginalTeletextDesign {
    head: TeletextDesignHead,
    candidate: EncodedChannelTeletextDocumentCandidate,
    digest: String,
}
impl SavedOriginalTeletextDesign {
    #[must_use]
    pub const fn head(&self) -> &TeletextDesignHead {
        &self.head
    }
    #[must_use]
    pub const fn candidate(&self) -> &EncodedChannelTeletextDocumentCandidate {
        &self.candidate
    }
    #[must_use]
    pub fn digest(&self) -> &str {
        &self.digest
    }
}
impl fmt::Debug for SavedOriginalTeletextDesign {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SavedOriginalTeletextDesign")
            .field("head", &self.head)
            .finish_non_exhaustive()
    }
}

/// Historical preview, independently recompiled without invoking any package.
/// Historical settings and lifecycle describe originals, not current authority.
pub struct RestoredOriginalTeletextDesign {
    design_id: EntityId,
    revision: i64,
    selection: TeletextChannelSelectionSnapshot,
    sources: Vec<TeletextSavedDocumentSource>,
    guide_inputs: Option<TeletextSavedGuideInputs>,
    guide_source: Option<TeletextGuideSourceFence>,
    digest: String,
    originals: EncodedChannelTeletextOriginals,
    edit_policy: TeletextSavedEditPolicy,
}
impl RestoredOriginalTeletextDesign {
    pub(super) fn with_resource_lifecycle_variants(
        mut self,
        deadline: Instant,
    ) -> Result<Self, ersatzrs_ffmpeg::teletext::TeletextResourceLifecycleError> {
        self.originals.encoded = self
            .originals
            .encoded
            .take()
            .map(|service| service.with_resource_lifecycle_variants(deadline))
            .transpose()?;
        Ok(self)
    }

    pub(crate) fn into_encoded_service(self) -> Option<EncodedTeletextAuthoredService> {
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
    pub const fn selection(&self) -> &TeletextChannelSelectionSnapshot {
        &self.selection
    }
    #[must_use]
    pub fn sources(&self) -> &[TeletextSavedDocumentSource] {
        &self.sources
    }
    /// Historical exact invocation inputs; absence remains legacy/static data.
    /// These bytes confer no current guide-source or publication authority.
    #[must_use]
    pub const fn guide_inputs(&self) -> Option<&TeletextSavedGuideInputs> {
        self.guide_inputs.as_ref()
    }
    /// Historical source data; freshness requires a separate transactional check.
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
impl fmt::Debug for RestoredOriginalTeletextDesign {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RestoredOriginalTeletextDesign")
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

fn sha256_hex(bytes: &[u8]) -> String {
    let mut digest = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        let _ = write!(digest, "{byte:02x}");
    }
    digest
}

// Constructed only alongside capture of a private, complete native candidate.
struct NativeOriginalDesignConfirmation {
    digest: String,
    deadline: Instant,
    hold: ersatzrs_core::locking::TeletextWorkHold,
}
#[async_trait]
impl TeletextOriginalDesignValidator for NativeOriginalDesignConfirmation {
    async fn confirm_original(
        &self,
        document: EncodedTeletextDesignDocuments,
    ) -> Result<EncodedTeletextDesignDocuments, TeletextOriginalDesignValidationError> {
        let expected = self.digest.clone();
        let deadline = self.deadline;
        super::teletext_work::spawn_blocking(self.hold.clone(), move || {
            if Instant::now() >= deadline {
                return Err(TeletextOriginalDesignValidationError::TimedOut);
            }
            let actual = sha256_hex(document.bytes());
            if actual != expected {
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

/// Capture exact original sources from a complete native candidate and save CAS.
/// The fresh CPU/admission deadline is distinct from its invocation receipt.
/// Once admitted, repository I/O uses the normal `SQLite` timeout/transaction
/// contract. The deadline does not cancel or undo a committed save.
/// No active configuration, output, package or playout notification is changed.
/// # Errors
/// Rejects codec, worker or admission-deadline failures before saving; storage
/// failures retain the repository's atomic no-partial-write contract.
pub async fn save_original_teletext_design_candidate(
    application: &ApplicationServices,
    name: String,
    existing: Option<(EntityId, i64)>,
    candidate: EncodedChannelTeletextDocumentCandidate,
    budget: TeletextDesignDocumentBudget,
    deadline: Instant,
) -> Result<TeletextDesignMutation<SavedOriginalTeletextDesign>, NativeTeletextDesignError> {
    check_deadline(deadline)?;
    let guard = super::teletext_work::begin(application, Some(deadline)).await?;
    save_original_teletext_design_candidate_with_hold(
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

/// Save under an existing operation's admission and exact supplied deadline.
/// No preparation allowance is renewed here. Standalone compatibility callers
/// retain their fresh save deadline; a chained owner passes its original one.
/// Repository settlement retains its existing atomic `SQLite` transaction contract.
pub(super) async fn save_original_teletext_design_candidate_with_hold(
    application: &ApplicationServices,
    name: String,
    existing: Option<(EntityId, i64)>,
    candidate: EncodedChannelTeletextDocumentCandidate,
    budget: TeletextDesignDocumentBudget,
    deadline: Instant,
    hold: ersatzrs_core::locking::TeletextWorkHold,
) -> Result<TeletextDesignMutation<SavedOriginalTeletextDesign>, NativeTeletextDesignError> {
    check_deadline(deadline)?;
    let (candidate, document, digest) =
        super::teletext_work::spawn_blocking(hold.clone(), move || {
            check_deadline(deadline)?;
            if candidate.transmissions().len() > budget.max_resource_transmissions {
                return Err(NativeTeletextDesignError::LifecycleBudget);
            }
            let transmissions: Vec<_> = candidate
                .transmissions()
                .iter()
                .map(|record| TeletextSavedResourceTransmission {
                    resource: record.resource,
                    counter: record.counter,
                    last_subpage: record.last_subpage,
                    update: match record.update {
                        TeletextResourceFullUpdate::Changed => TeletextSavedResourceUpdate::Changed,
                        TeletextResourceFullUpdate::Unchanged => {
                            TeletextSavedResourceUpdate::Unchanged
                        }
                    },
                })
                .collect();
            let input = TeletextDesignDocumentInput {
                selection: candidate.selection(),
                generation: candidate.generation(),
                sources: candidate.sources(),
                entry_addon_id: candidate.entry_addon_id(),
                entry: candidate.metadata().map(|metadata| metadata.entry),
                v1_region: candidate.v1_region().map(|region| match region {
                    TeletextEncodingRegion::WesternLatin => TeletextSavedV1Region::WesternLatin,
                }),
                resource_transmissions: &transmissions,
            };
            let document = match candidate.guide_source() {
                Some(source) => encode_teletext_design_documents_with_source(
                    input,
                    candidate.requests(),
                    source,
                    budget,
                ),
                None => encode_teletext_design_documents_with_requests(
                    input,
                    candidate.requests(),
                    budget,
                ),
            }?;
            let digest = sha256_hex(document.bytes());
            check_deadline(deadline)?;
            Ok::<_, NativeTeletextDesignError>((candidate, document, digest))
        })
        .await
        .map_err(|_| NativeTeletextDesignError::WorkerFailed)??;
    let confirmation = NativeOriginalDesignConfirmation {
        digest: digest.clone(),
        deadline,
        hold,
    };
    Ok(
        match application
            .save_original_teletext_design(
                name,
                existing,
                document,
                &confirmation,
                budget.max_serialized_bytes,
            )
            .await?
        {
            TeletextDesignMutation::Saved(head) => {
                TeletextDesignMutation::Saved(SavedOriginalTeletextDesign {
                    head,
                    candidate,
                    digest,
                })
            }
            TeletextDesignMutation::Rejected(reason) => TeletextDesignMutation::Rejected(reason),
        },
    )
}

/// Reload immutable originals and compile the same checked preview model.
/// No producer runs and no historical selection is promoted into a live fence.
/// Caller policy supplies bounds; saved exact entry/region/lifecycle supply content.
/// # Errors
/// Missing revision returns None; corrupt/unsupported/bounded/native failures are errors.
pub async fn reload_original_teletext_design_revision(
    application: &ApplicationServices,
    id: EntityId,
    revision: i64,
    budget: TeletextDesignDocumentBudget,
    policy: TeletextChannelDocumentEncodingPolicy,
    deadline: Instant,
) -> Result<Option<RestoredOriginalTeletextDesign>, NativeTeletextDesignError> {
    check_deadline(deadline)?;
    let guard = super::teletext_work::begin(application, Some(deadline)).await?;
    reload_original_teletext_design_revision_with_hold(
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

pub(super) async fn reload_original_teletext_design_revision_with_hold(
    application: &ApplicationServices,
    id: EntityId,
    revision: i64,
    mut budget: TeletextDesignDocumentBudget,
    policy: TeletextChannelDocumentEncodingPolicy,
    deadline: Instant,
    hold: ersatzrs_core::locking::TeletextWorkHold,
) -> Result<Option<RestoredOriginalTeletextDesign>, NativeTeletextDesignError> {
    check_deadline(deadline)?;
    // Restrict the decoder's existing streaming document-count admission before
    // it converts any original into a semantic service. Stored bytes retain the
    // independent max_serialized_bytes limit; entry/lifecycle content is unchanged.
    budget.max_documents = budget.max_documents.min(maximum_producers(&policy));
    let Some(stored) = application
        .teletext_design_revision(id, revision, budget.max_serialized_bytes)
        .await?
    else {
        return Ok(None);
    };
    if stored.document_version != ersatzrs_domain::TELETEXT_DESIGN_DOCUMENT_VERSION {
        return Err(NativeTeletextDesignError::Document(
            TeletextDesignDocumentError {
                binding: None,
                reason:
                    ersatzrs_app::teletext_design_documents::TeletextDesignDocumentFailure::Version,
            },
        ));
    }
    let (stored, parts) = super::teletext_work::spawn_blocking(hold.clone(), move || {
        check_deadline(deadline)?;
        let parts = decode_teletext_design_documents(&stored.document, budget)?.into_parts();
        Ok::<_, NativeTeletextDesignError>((stored, parts))
    })
    .await
    .map_err(|_| NativeTeletextDesignError::WorkerFailed)??;
    let baseline = if let Some(overlay) = parts.editorial_overlay.as_ref() {
        Some(super::teletext_editorial_designs::read_editorial_baseline(
            application, overlay.provenance(), stored.design_id, Some(stored.revision),
            budget, &policy, deadline,
        ).await.map_err(|error| {
            if matches!(error, super::teletext_editorial_designs::NativeTeletextEditorialImportError::MissingReceipt) {
                NativeTeletextDesignError::EditorialValidationUnavailable
            } else { NativeTeletextDesignError::Editorial(Box::new(error)) }
        })?)
    } else {
        None
    };
    let restored = super::teletext_work::spawn_blocking(hold, move || match baseline {
        Some(baseline) => super::teletext_editorial_designs::restore_editorial_design(
            stored, parts, baseline, budget, policy, deadline,
        )
        .map_err(|error| NativeTeletextDesignError::Editorial(Box::new(error))),
        None => compile_restored_parts(stored, parts, policy, deadline, Vec::new()),
    })
    .await
    .map_err(|_| NativeTeletextDesignError::WorkerFailed)??;
    Ok(Some(restored))
}

/// The caller has authenticated any physical overlay before this final shared
/// compilation. Historical entry, region, sources and lifecycle remain exact.
pub(super) fn compile_restored_parts(
    stored: TeletextDesignRevision,
    parts: TeletextRestoredDesignDocumentParts,
    policy: TeletextChannelDocumentEncodingPolicy,
    deadline: Instant,
    overlays: Vec<TeletextPhysicalBaseOverlay>,
) -> Result<RestoredOriginalTeletextDesign, NativeTeletextDesignError> {
    check_deadline(deadline)?;
    // The raw entry point never authorizes stored resource-object intent.
    if parts.editorial_overlay.as_ref().is_some_and(|overlay| {
        !overlay.resource_object_operands().is_empty()
            || !overlay.resource_object_program_overlays().is_empty()
            || !overlay.resource_object_definition_additions().is_empty()
    }) {
        return Err(NativeTeletextDesignError::EditorialValidationUnavailable);
    }
    compile_restored_parts_with_resource_operands(stored, parts, policy, deadline, overlays, &[])
}

/// Replay an explicitly authenticated exact resource slice alongside historical inputs.
/// The caller retains original-receipt and all shared-effect qualification duties.
pub(super) fn compile_restored_parts_with_resource_operands(
    stored: TeletextDesignRevision,
    parts: TeletextRestoredDesignDocumentParts,
    policy: TeletextChannelDocumentEncodingPolicy,
    deadline: Instant,
    overlays: Vec<TeletextPhysicalBaseOverlay>,
    resource_operands: &[ersatzrs_domain::TeletextResourceObjectOperandOverlay],
) -> Result<RestoredOriginalTeletextDesign, NativeTeletextDesignError> {
    compile_restored_parts_with_resource_edits(
        stored,
        parts,
        policy,
        deadline,
        overlays,
        resource_operands,
        &[],
    )
}

/// Replay only the exact resource intents authenticated by the original receipt.
pub(super) fn compile_restored_parts_with_resource_edits(
    stored: TeletextDesignRevision,
    parts: TeletextRestoredDesignDocumentParts,
    policy: TeletextChannelDocumentEncodingPolicy,
    deadline: Instant,
    overlays: Vec<TeletextPhysicalBaseOverlay>,
    resource_operands: &[ersatzrs_domain::TeletextResourceObjectOperandOverlay],
    resource_programs: &[ersatzrs_domain::TeletextResourceObjectProgramOverlay],
) -> Result<RestoredOriginalTeletextDesign, NativeTeletextDesignError> {
    check_deadline(deadline)?;
    super::teletext_stored_designs::check_authenticated_editorial_resource_replay(
        parts.editorial_overlay.as_ref(),
        !overlays.is_empty(),
        resource_operands,
        resource_programs,
    )?;
    compile_restored_parts_with_resource_intents(
        stored,
        parts,
        policy,
        deadline,
        overlays,
        super::teletext_stored_designs::EditorialResourceIntents {
            object_resource_additions: &[],
            object_subpage_announcements: &[],
            object_page_selections: &[],
            operands: resource_operands,
            programs: resource_programs,
            additions: &[],
        },
    )
}

/// Replay the complete stored intent after retained-receipt qualification.
pub(super) fn compile_restored_parts_with_resource_intents(
    stored: TeletextDesignRevision,
    parts: TeletextRestoredDesignDocumentParts,
    mut policy: TeletextChannelDocumentEncodingPolicy,
    deadline: Instant,
    overlays: Vec<TeletextPhysicalBaseOverlay>,
    resources: super::teletext_stored_designs::EditorialResourceIntents<'_>,
) -> Result<RestoredOriginalTeletextDesign, NativeTeletextDesignError> {
    let resource_operands = resources.operands;
    let resource_programs = resources.programs;
    let resource_additions = resources.additions;
    check_deadline(deadline)?;
    super::teletext_stored_designs::check_authenticated_editorial_resource_intents(
        parts.editorial_overlay.as_ref(),
        !overlays.is_empty(),
        resources,
    )?;
    policy.entry = parts.entry;
    policy.transmissions = super::teletext_stored_designs::resource_transmissions(
        &parts.resource_transmissions,
        resources.object_resource_additions,
    );
    let region = parts.v1_region.map(|region| match region {
        TeletextSavedV1Region::WesternLatin => TeletextEncodingRegion::WesternLatin,
    });
    let operands = parts
        .editorial_overlay
        .as_ref()
        .map_or(&[][..], |overlay| overlay.local_program_operands());
    let object_operands = parts
        .editorial_overlay
        .as_ref()
        .map_or(&[][..], |overlay| overlay.local_object_operands());
    let programs = parts.editorial_overlay.as_ref().map_or(&[][..], |overlay| {
        overlay.local_object_glyph_program_overlays()
    });
    let bitmaps = parts
        .editorial_overlay
        .as_ref()
        .map_or(&[][..], |overlay| overlay.drcs_bitmap_overlays());
    let editorial = (overlays, operands, object_operands, bitmaps, programs);
    let base_text_edits = parts
        .editorial_overlay
        .as_ref()
        .map_or(&[][..], |overlay| overlay.base_text_edits());
    let main_programs = parts.editorial_overlay.as_ref().map_or(&[][..], |overlay| {
        overlay.main_local_invocation_program_overlays()
    });
    let originals = if resource_operands.is_empty()
        && resource_programs.is_empty()
        && resource_additions.is_empty()
        && resources.object_resource_additions.is_empty()
        && resources.object_subpage_announcements.is_empty()
        && main_programs.is_empty()
        && base_text_edits.is_empty()
    {
        encode_original_channel_documents_with_editorial_programs(
            &parts.selection,
            parts.entry_addon_id.as_deref(),
            parts.documents,
            editorial,
            deadline,
            region,
            policy,
        )
    } else {
        encode_original_channel_documents_with_editorial_resources(
            &parts.selection,
            parts.entry_addon_id.as_deref(),
            parts.documents,
            EditorialResourceDocuments {
                object_resource_additions: resources.object_resource_additions,
                object_subpage_announcements: resources.object_subpage_announcements,
                object_page_selections: resources.object_page_selections,
                base_text_edits,
                main_programs,
                programs: editorial,
                resource_operands,
                resource_programs,
                resource_additions,
            },
            deadline,
            region,
            policy,
        )
    }
    .map_err(|error| NativeTeletextDesignError::Encoding(Box::new(error)))?;
    check_deadline(deadline)?;
    Ok(RestoredOriginalTeletextDesign {
        design_id: stored.design_id,
        revision: stored.revision,
        selection: parts.selection,
        sources: parts.sources,
        guide_inputs: parts.guide_inputs,
        guide_source: parts.guide_source,
        digest: stored.digest,
        originals,
        edit_policy: parts.edit_policy,
    })
}

/// Private witness constructed only alongside complete native qualification.
pub(super) fn native_design_confirmation(
    digest: String,
    deadline: Instant,
    hold: ersatzrs_core::locking::TeletextWorkHold,
) -> impl TeletextOriginalDesignValidator {
    NativeOriginalDesignConfirmation {
        digest,
        deadline,
        hold,
    }
}

/// Redacted errors before a durable pending request is admitted.
#[derive(Debug, thiserror::Error)]
pub enum NativeTeletextPublicationError {
    #[error(transparent)]
    Design(#[from] NativeTeletextDesignError),
    #[error(transparent)]
    Request(#[from] ersatzrs_app::teletext_publications::TeletextPublicationStoreError),
}

/// Pending intent and exact compiled originals; this is not accepted output.
#[derive(Debug)]
pub struct RequestedOriginalTeletextPublication {
    request: ersatzrs_domain::TeletextPublicationRequest,
    design: RestoredOriginalTeletextDesign,
}
impl RequestedOriginalTeletextPublication {
    #[must_use]
    pub const fn request(&self) -> &ersatzrs_domain::TeletextPublicationRequest {
        &self.request
    }
    #[must_use]
    pub const fn design(&self) -> &RestoredOriginalTeletextDesign {
        &self.design
    }

    /// Move compiled output together with its exact pending request/selection.
    /// Raw stored documents and preview-only projections are released, without
    /// cloning the full service. This neither rechecks current configuration nor
    /// admits a generation; the runtime owner must still perform those fences.
    /// Returns no candidate when the original carries no compiled output.
    #[must_use]
    pub fn into_output_candidate(self) -> Option<RequestedTeletextOutputCandidate> {
        let RestoredOriginalTeletextDesign {
            selection,
            originals,
            ..
        } = self.design;
        Some(RequestedTeletextOutputCandidate {
            request: self.request,
            selection,
            service: Arc::new(originals.encoded?),
        })
    }
}

/// Associated pending request and exact immutable output, without promotion.
pub struct RequestedTeletextOutputCandidate {
    request: ersatzrs_domain::TeletextPublicationRequest,
    selection: TeletextChannelSelectionSnapshot,
    service: Arc<EncodedTeletextAuthoredService>,
}
impl RequestedTeletextOutputCandidate {
    #[must_use]
    pub const fn request(&self) -> &ersatzrs_domain::TeletextPublicationRequest {
        &self.request
    }
    #[must_use]
    pub const fn selection(&self) -> &TeletextChannelSelectionSnapshot {
        &self.selection
    }
    #[must_use]
    pub const fn service(&self) -> &Arc<EncodedTeletextAuthoredService> {
        &self.service
    }
}
impl fmt::Debug for RequestedTeletextOutputCandidate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RequestedTeletextOutputCandidate")
            .field("request", &self.request)
            .field("binding_count", &self.selection.bindings.len())
            .finish_non_exhaustive()
    }
}

// The only production confirmation is borrowed from this operation's private
// fully recompiled originals. A historical preview receipt is never sufficient.
struct NativePublicationConfirmation<'a> {
    design: &'a RestoredOriginalTeletextDesign,
    deadline: Instant,
}
#[async_trait]
impl ersatzrs_app::teletext_publications::TeletextPublicationValidator
    for NativePublicationConfirmation<'_>
{
    async fn confirm_request(
        &self,
        request: &ersatzrs_core::repositories::TeletextPublicationRequestSave,
    ) -> Result<(), ersatzrs_app::teletext_publications::TeletextPublicationValidationError> {
        use ersatzrs_app::teletext_publications::TeletextPublicationValidationError as E;
        if Instant::now() >= self.deadline {
            return Err(E::TimedOut);
        }
        if self.design.encoded().is_none()
            || request.design_id != self.design.design_id
            || request.revision != self.design.revision
            || request.digest != self.design.digest
            || request.selection != self.design.selection
        {
            return Err(E::Association);
        }
        if Instant::now() >= self.deadline {
            return Err(E::TimedOut);
        }
        Ok(())
    }

    async fn confirm_request_with_source(
        &self,
        request: &ersatzrs_core::repositories::TeletextPublicationRequestSave,
        source: &TeletextGuideSourceFence,
    ) -> Result<(), ersatzrs_app::teletext_publications::TeletextPublicationValidationError> {
        use ersatzrs_app::teletext_publications::TeletextPublicationValidationError as E;
        self.confirm_request(request).await?;
        if self.design.guide_source() != Some(source)
            || !crate::admin::languages::LANGUAGES
                .iter()
                .any(|definition| definition.code == source.locale)
        {
            return Err(E::Association);
        }
        self.confirm_request(request).await
    }
}

/// Recompile an exact saved revision, then persist an explicitly pending intent.
///
/// No package is invoked. Historical selection is compared with fresh complete
/// configuration inside the repository's immediate transaction. Expected head
/// and target revision are independent, allowing a deliberate older revision.
/// The deadline cuts off native admission, not an admitted database transaction.
/// This does not promote output, recover receiver caches or alter Lua timing.
/// # Errors
/// Reports native/compiler/admission/storage failures; identity, digest and
/// atomic CAS/configuration rejections remain explicit no-write results.
#[expect(
    clippy::too_many_arguments,
    reason = "explicit target, two independent CAS fences and trusted native policies"
)]
pub async fn request_original_teletext_design_publication(
    application: &ApplicationServices,
    design_id: EntityId,
    revision: i64,
    expected_digest: String,
    expected_design_head: i64,
    expected_request_id: Option<EntityId>,
    budget: TeletextDesignDocumentBudget,
    policy: TeletextChannelDocumentEncodingPolicy,
    deadline: Instant,
) -> Result<
    ersatzrs_core::repositories::TeletextPublicationMutation<RequestedOriginalTeletextPublication>,
    NativeTeletextPublicationError,
> {
    request_original_publication(
        application,
        design_id,
        revision,
        expected_digest,
        expected_design_head,
        expected_request_id,
        budget,
        policy,
        None,
        deadline,
    )
    .await
}

/// Recompile a source-bound saved revision and atomically compare its historical
/// guide context before writing pending intent. Limits are fresh trusted policy;
/// the source receipt's old deadline is not renewed or reused as admission.
/// No accepted generation, live Now/Next freshness or output activation is claimed.
/// # Errors
/// Reports native/admission/storage failures; source/CAS rejection writes nothing.
#[expect(
    clippy::too_many_arguments,
    reason = "exact revision, independent CAS fences and explicit trusted native/source policies"
)]
pub async fn request_source_bound_original_teletext_design_publication(
    application: &ApplicationServices,
    design_id: EntityId,
    revision: i64,
    expected_digest: String,
    expected_design_head: i64,
    expected_request_id: Option<EntityId>,
    budget: TeletextDesignDocumentBudget,
    policy: TeletextChannelDocumentEncodingPolicy,
    limits: TeletextGuideSourceCheckLimits,
    deadline: Instant,
) -> Result<
    ersatzrs_core::repositories::TeletextPublicationMutation<RequestedOriginalTeletextPublication>,
    NativeTeletextPublicationError,
> {
    request_original_publication(
        application,
        design_id,
        revision,
        expected_digest,
        expected_design_head,
        expected_request_id,
        budget,
        policy,
        Some(limits),
        deadline,
    )
    .await
}

/// Exact rendered revision/digest and independent head/pending CAS fences.
/// An optional route owner is mandatory for channel-scoped native admin callers.
pub(super) struct TeletextOriginalPublicationTarget {
    pub(super) design_id: EntityId,
    pub(super) revision: i64,
    pub(super) expected_digest: String,
    pub(super) expected_design_head: i64,
    pub(super) expected_request_id: Option<EntityId>,
    pub(super) expected_channel_id: Option<EntityId>,
    pub(super) require_static_original: bool,
}
impl TeletextOriginalPublicationTarget {
    fn is_valid(&self) -> bool {
        self.design_id > 0
            && self.revision > 0
            && self.expected_design_head > 0
            && self.expected_request_id.is_none_or(|id| id > 0)
            && self.expected_channel_id.is_none_or(|id| id > 0)
            && self.expected_digest.len() == 64
            && self
                .expected_digest
                .bytes()
                .all(|value| value.is_ascii_digit() || (b'a'..=b'f').contains(&value))
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "shared native admission retains separate exact target and trusted policy fences"
)]
async fn request_original_publication(
    application: &ApplicationServices,
    design_id: EntityId,
    revision: i64,
    expected_digest: String,
    expected_design_head: i64,
    expected_request_id: Option<EntityId>,
    budget: TeletextDesignDocumentBudget,
    policy: TeletextChannelDocumentEncodingPolicy,
    source_limits: Option<TeletextGuideSourceCheckLimits>,
    deadline: Instant,
) -> Result<
    ersatzrs_core::repositories::TeletextPublicationMutation<RequestedOriginalTeletextPublication>,
    NativeTeletextPublicationError,
> {
    use ersatzrs_core::repositories::{
        TeletextPublicationMutation as M, TeletextPublicationRejection as R,
    };
    let target = TeletextOriginalPublicationTarget {
        design_id,
        revision,
        expected_digest,
        expected_design_head,
        expected_request_id,
        expected_channel_id: None,
        require_static_original: false,
    };
    if !target.is_valid() {
        return Ok(M::Rejected(R::InvalidValues));
    }
    check_deadline(deadline)?;
    let guard = super::teletext_work::begin(application, Some(deadline))
        .await
        .map_err(NativeTeletextDesignError::from)?;
    request_original_publication_with_hold(
        application,
        target,
        budget,
        policy,
        source_limits,
        deadline,
        guard.hold_work(),
    )
    .await
}

/// Request pending intent within the controller's existing bounded admission.
/// Original channel ownership is checked before any repository mutation.
/// # Errors
/// Preserves exact native/source/CAS errors without acquiring another work slot.
pub(super) async fn request_original_publication_with_hold(
    application: &ApplicationServices,
    target: TeletextOriginalPublicationTarget,
    budget: TeletextDesignDocumentBudget,
    policy: TeletextChannelDocumentEncodingPolicy,
    source_limits: Option<TeletextGuideSourceCheckLimits>,
    deadline: Instant,
    hold: ersatzrs_core::locking::TeletextWorkHold,
) -> Result<
    ersatzrs_core::repositories::TeletextPublicationMutation<RequestedOriginalTeletextPublication>,
    NativeTeletextPublicationError,
> {
    use ersatzrs_core::repositories::{
        TeletextPublicationMutation as M, TeletextPublicationRejection as R,
        TeletextPublicationRequestSave,
    };
    if !target.is_valid() {
        return Ok(M::Rejected(R::InvalidValues));
    }
    check_deadline(deadline)?;
    let TeletextOriginalPublicationTarget {
        design_id,
        revision,
        expected_digest,
        expected_design_head,
        expected_request_id,
        expected_channel_id,
        require_static_original,
    } = target;
    let Some(design) = reload_original_teletext_design_revision_with_hold(
        application,
        design_id,
        revision,
        budget,
        policy,
        deadline,
        hold.clone(),
    )
    .await?
    else {
        return Ok(M::Rejected(R::MissingRevision));
    };
    if expected_channel_id.is_some_and(|channel_id| design.selection.channel_id != channel_id) {
        return Ok(M::Rejected(R::InvalidValues));
    }
    if require_static_original && !super::teletext_generations::frozen_original(&design) {
        return Ok(M::Rejected(R::GuideFenceRequired));
    }
    if design.digest != expected_digest {
        return Ok(M::Rejected(R::DigestConflict));
    }
    if design.encoded().is_none() || design.selection.bindings.is_empty() {
        return Ok(M::Rejected(R::InvalidValues));
    }
    let request = TeletextPublicationRequestSave {
        design_id,
        revision,
        digest: expected_digest,
        expected_design_head,
        expected_request_id,
        selection: design.selection.clone(),
    };
    let validator = NativePublicationConfirmation {
        design: &design,
        deadline,
    };
    let mutation = match (design.guide_source(), source_limits) {
        (Some(source), Some(limits)) => {
            application
                .request_source_bound_teletext_design_publication(
                    request,
                    &validator,
                    source,
                    limits,
                    budget.max_serialized_bytes,
                    deadline,
                )
                .await?
        }
        (Some(_), None) => return Ok(M::Rejected(R::GuideFenceRequired)),
        (None, Some(_)) => return Ok(M::Rejected(R::GuideFenceMismatch)),
        (None, None) => {
            application
                .request_teletext_design_publication(
                    request,
                    &validator,
                    budget.max_serialized_bytes,
                )
                .await?
        }
    };
    drop(hold);
    Ok(match mutation {
        M::Requested(request) => {
            M::Requested(RequestedOriginalTeletextPublication { request, design })
        }
        M::Rejected(reason) => M::Rejected(reason),
    })
}
