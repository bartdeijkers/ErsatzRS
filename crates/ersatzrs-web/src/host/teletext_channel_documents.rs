//! Native cold channel preparation, without publication or warm-cache authority.

use std::{fmt, mem::size_of, time::Instant};

use ersatzrs_app::{
    teletext_channel_documents::{
        BoundChannelTeletextDocumentInput, ChannelTeletextDocumentError,
        PreparedChannelTeletextDocumentParts, TeletextChannelDocumentInputBudget,
        TeletextChannelRequestTemplates,
    },
    teletext_guide::SuppliedTeletextGuideContext,
    teletext_profile_documents::{
        TeletextProfileDocument, TeletextProfileDocumentBudget, TeletextValidatedDocumentSource,
    },
    ApplicationServices,
};
use ersatzrs_core::repositories::TeletextGuideSourceFence;
use ersatzrs_domain::{
    EntityId, TeletextChannelSelectionSnapshot, TeletextColdProducerComposition,
    TeletextComposedProducerPage, TeletextPageAddress, TeletextPhysicalBaseOverlay,
    TeletextProducerCompositionBudget, TeletextProducerCompositionError,
    TeletextProducerCompositionFailure, TeletextProducerCompositionMetadata,
    TeletextProducerPresentation, TeletextProducerPresentationBudget,
    TeletextProducerPresentationError,
};
use ersatzrs_ffmpeg::teletext::{
    encode_teletext_authored_service_before, EncodedTeletextAuthoredService,
    TeletextAuthoredServiceEncodingBudget, TeletextAuthoredServiceEncodingError,
    TeletextAuthoredServiceEncodingFailure, TeletextEncodingRegion, TeletextServiceDescriptor,
    TeletextServiceResourceTransmission,
};

use super::teletext_documents::{lift_base, TeletextDocumentEncodingError};

#[cfg(test)]
mod physical_overlay_tests;

/// Trusted input/model/encoding limits and explicit resource lifecycle records.
/// Entry language always comes from the selected original producer, never policy.
#[derive(Clone)]
pub struct TeletextChannelDocumentEncodingPolicy {
    pub input: TeletextChannelDocumentInputBudget,
    pub validation: TeletextProfileDocumentBudget,
    pub v1_region: TeletextEncodingRegion,
    pub composition: TeletextProducerCompositionBudget,
    pub encoding: TeletextAuthoredServiceEncodingBudget,
    pub transmissions: Vec<TeletextServiceResourceTransmission>,
    /// Exact entry owner within the selected producer's original initial page.
    /// Absence is valid only when that page has one exact original subcode.
    pub entry: Option<TeletextPageAddress>,
}

/// Complete cold candidate or explicit Off, retaining original binding order.
/// Private fields cannot be independently paired with another encoded service.
/// Opaque correlation and observed selection do not grant generation ownership.
pub struct EncodedChannelTeletextDocumentCandidate {
    selection: TeletextChannelSelectionSnapshot,
    generation: String,
    deadline: Instant,
    entry_addon_id: Option<String>,
    sources: Vec<TeletextValidatedDocumentSource>,
    requests: TeletextChannelRequestTemplates,
    guide_source: Option<TeletextGuideSourceFence>,
    v1_region: Option<TeletextEncodingRegion>,
    producers: Vec<TeletextProducerPresentation>,
    provenance: Vec<TeletextComposedProducerPage>,
    metadata: Option<TeletextProducerCompositionMetadata>,
    encoded: Option<EncodedTeletextAuthoredService>,
    descriptor: Option<TeletextServiceDescriptor>,
    transmissions: Vec<TeletextServiceResourceTransmission>,
}
impl EncodedChannelTeletextDocumentCandidate {
    #[must_use]
    pub const fn selection(&self) -> &TeletextChannelSelectionSnapshot {
        &self.selection
    }
    #[must_use]
    pub fn generation(&self) -> &str {
        &self.generation
    }
    #[must_use]
    pub const fn deadline(&self) -> Instant {
        self.deadline
    }
    /// Host composition may only shorten the preparation lifetime, never renew it.
    pub(super) fn restrict_deadline(mut self, deadline: Instant) -> Self {
        self.deadline = self.deadline.min(deadline);
        self
    }
    #[must_use]
    pub fn entry_addon_id(&self) -> Option<&str> {
        self.entry_addon_id.as_deref()
    }
    /// `producers()[i]` belongs to exactly `selection().bindings[i]`.
    #[must_use]
    pub fn producers(&self) -> &[TeletextProducerPresentation] {
        &self.producers
    }
    /// Exact sources before composition: `sources()[i]` belongs to
    /// `producers()[i]` and `selection().bindings[i]`.
    #[must_use]
    pub fn sources(&self) -> &[TeletextValidatedDocumentSource] {
        &self.sources
    }
    /// Exact bounded guide requests used by installed producers, keyed by contract.
    /// These are input data, not durable source provenance or generation authority.
    #[must_use]
    pub const fn requests(&self) -> &TeletextChannelRequestTemplates {
        &self.requests
    }
    /// Historical semantic source association attached only by native SQL capture.
    /// It does not grant accepted-generation or runtime publication authority.
    #[must_use]
    pub const fn guide_source(&self) -> Option<&TeletextGuideSourceFence> {
        self.guide_source.as_ref()
    }
    pub(super) fn with_guide_source(mut self, source: TeletextGuideSourceFence) -> Self {
        self.guide_source = Some(source);
        self
    }
    /// Explicit host region actually used for V1 lift; absent for V2-only/Off.
    #[must_use]
    pub const fn v1_region(&self) -> Option<TeletextEncodingRegion> {
        self.v1_region
    }
    #[must_use]
    pub fn provenance(&self) -> &[TeletextComposedProducerPage] {
        &self.provenance
    }
    #[must_use]
    pub const fn metadata(&self) -> Option<&TeletextProducerCompositionMetadata> {
        self.metadata.as_ref()
    }
    #[must_use]
    pub const fn encoded(&self) -> Option<&EncodedTeletextAuthoredService> {
        self.encoded.as_ref()
    }
    /// Exact trusted lifecycle supplied to the complete compiler, never inferred.
    #[must_use]
    pub fn transmissions(&self) -> &[TeletextServiceResourceTransmission] {
        &self.transmissions
    }
    #[must_use]
    pub const fn descriptor(&self) -> Option<&TeletextServiceDescriptor> {
        self.descriptor.as_ref()
    }
}
impl fmt::Debug for EncodedChannelTeletextDocumentCandidate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EncodedChannelTeletextDocumentCandidate")
            .field("selection", &self.selection)
            .field("producers", &self.producers.len())
            .field("pages", &self.provenance.len())
            .field("enabled", &self.encoded.is_some())
            .finish_non_exhaustive()
    }
}

/// Numeric binding/engine failures, without original producer or guide text.
#[derive(Debug, thiserror::Error)]
pub enum ChannelTeletextDocumentEncodingError {
    #[error(transparent)]
    Work(#[from] super::teletext_work::TeletextWorkError),
    #[error(transparent)]
    Preparation(#[from] ChannelTeletextDocumentError),
    #[error("teletext channel documents do not match original binding association")]
    Association,
    #[error("teletext channel has {count} producers, exceeding trusted maximum {maximum}")]
    ProducerBudget { count: usize, maximum: usize },
    #[error("teletext binding {binding} v1 lift: {error}")]
    Lift {
        binding: usize,
        error: Box<TeletextDocumentEncodingError>,
    },
    #[error("teletext binding {binding} presentation: {error}")]
    Presentation {
        binding: usize,
        error: TeletextProducerPresentationError,
    },
    #[error("teletext binding {binding} initial page requires an exact entry subcode")]
    EntryRequired { binding: usize },
    #[error("teletext binding {binding} does not own requested initial entry {entry:?}")]
    EntryAddress {
        binding: usize,
        entry: TeletextPageAddress,
    },
    #[error("cold teletext channel composition: {0}")]
    Composition(Box<TeletextProducerCompositionError>),
    #[error("native teletext channel encoding: {0}")]
    Encoding(Box<TeletextAuthoredServiceEncodingError>),
    #[error("teletext channel preparation exceeded its original deadline")]
    TimedOut,
    #[error("teletext channel validation/encoding worker failed")]
    WorkerFailed,
}

/// Invoke all enabled contracts once and join one validation/native task.
///
/// Validation, lossless v1 lift, producer snapshots, checked cold composition and
/// complete encoding run off async workers under the original app deadline. The
/// entire channel selection is checked again after CPU work. Off performs no
/// lift/composition/encoding and ignores unused entry/lifecycle policy.
/// Caller abandonment does not preempt already running bounded synchronous work.
/// This neither writes configuration nor owns a generation, publication or a
/// warm receiver transition; subsequent promotion needs its own fresh fence.
/// # Errors
/// Rejects any original receipt, binding association, entry, model, encoding,
/// worker, deadline or changed complete channel selection without partial output.
pub async fn prepare_channel_teletext_document_candidate(
    application: &ApplicationServices,
    channel_id: EntityId,
    entry_addon_id: Option<&str>,
    context: &SuppliedTeletextGuideContext<'_>,
    policy: TeletextChannelDocumentEncodingPolicy,
) -> Result<EncodedChannelTeletextDocumentCandidate, ChannelTeletextDocumentEncodingError> {
    let deadline = Instant::now()
        .checked_add(ersatzrs_app::teletext_channel_documents::PREPARATION_TIMEOUT)
        .ok_or(ChannelTeletextDocumentEncodingError::TimedOut)?;
    let guard = super::teletext_work::begin(application, Some(deadline)).await?;
    prepare_channel_teletext_document_candidate_with_hold(
        application,
        channel_id,
        entry_addon_id,
        TeletextChannelDocumentPreparation {
            context,
            expected_selection: None,
            deadline,
        },
        policy,
        guard.hold_work(),
    )
    .await
}

/// Borrowed original operation state; raw settings never enter an admin receipt.
pub(super) struct TeletextChannelDocumentPreparation<'context, 'guide> {
    pub(super) context: &'context SuppliedTeletextGuideContext<'guide>,
    pub(super) expected_selection: Option<&'context TeletextChannelSelectionSnapshot>,
    pub(super) deadline: Instant,
}

/// Share one already admitted operation across preparation and later draft work.
/// The supplied deadline can only shorten the existing preparation allowance.
/// Actual producer/native workers retain their own clone if the caller drops.
pub(super) async fn prepare_channel_teletext_document_candidate_with_hold(
    application: &ApplicationServices,
    channel_id: EntityId,
    entry_addon_id: Option<&str>,
    preparation: TeletextChannelDocumentPreparation<'_, '_>,
    policy: TeletextChannelDocumentEncodingPolicy,
    hold: ersatzrs_core::locking::TeletextWorkHold,
) -> Result<EncodedChannelTeletextDocumentCandidate, ChannelTeletextDocumentEncodingError> {
    let deadline = Instant::now()
        .checked_add(ersatzrs_app::teletext_channel_documents::PREPARATION_TIMEOUT)
        .ok_or(ChannelTeletextDocumentEncodingError::TimedOut)?
        .min(preparation.deadline);
    check_deadline(deadline)?;
    let mut input_budget = policy.input;
    input_budget.max_producers = maximum_producers(&policy);
    let input = match preparation.expected_selection {
        Some(expected_selection) => {
            application
                .invoke_channel_teletext_documents_with_selection_and_hold(
                    channel_id,
                    entry_addon_id,
                    preparation.context,
                    input_budget,
                    expected_selection,
                    (deadline, hold.clone()),
                )
                .await?
        }
        None => {
            application
                .invoke_channel_teletext_documents_with_hold(
                    channel_id,
                    entry_addon_id,
                    preparation.context,
                    input_budget,
                    deadline,
                    hold.clone(),
                )
                .await?
        }
    };
    let candidate =
        super::teletext_work::spawn_blocking(hold.clone(), move || encode_channel(input, policy))
            .await
            .map_err(|_| ChannelTeletextDocumentEncodingError::WorkerFailed)??;
    let candidate = candidate.restrict_deadline(deadline);
    application
        .recheck_channel_teletext_document_selection(candidate.selection(), candidate.deadline())
        .await?;
    check_deadline(candidate.deadline())?;
    Ok(candidate)
}

/// Checked original content shared by fresh preparation and historical reload.
/// It carries no current selection, invocation receipt or publication authority.
pub(super) struct EncodedChannelTeletextOriginals {
    pub(super) v1_region: Option<TeletextEncodingRegion>,
    pub(super) producers: Vec<TeletextProducerPresentation>,
    pub(super) provenance: Vec<TeletextComposedProducerPage>,
    pub(super) metadata: Option<TeletextProducerCompositionMetadata>,
    pub(super) encoded: Option<EncodedTeletextAuthoredService>,
    pub(super) descriptor: Option<TeletextServiceDescriptor>,
    pub(super) transmissions: Vec<TeletextServiceResourceTransmission>,
}

fn encode_channel(
    input: BoundChannelTeletextDocumentInput,
    policy: TeletextChannelDocumentEncodingPolicy,
) -> Result<EncodedChannelTeletextDocumentCandidate, ChannelTeletextDocumentEncodingError> {
    check_deadline(input.deadline())?;
    check_producer_budget(input.producer_count(), &policy)?;
    let PreparedChannelTeletextDocumentParts {
        selection,
        generation,
        documents,
        sources,
        requests,
        entry_addon_id,
        deadline,
    } = input
        .validate(policy.validation)?
        .into_parts_with_requests();
    check_deadline(deadline)?;
    if documents.len() != selection.bindings.len() || sources.len() != documents.len() {
        return Err(ChannelTeletextDocumentEncodingError::Association);
    }
    let region = documents
        .iter()
        .any(|document| matches!(document, TeletextProfileDocument::Base(_)))
        .then_some(policy.v1_region);
    let originals = encode_original_channel_documents(
        &selection,
        entry_addon_id.as_deref(),
        documents,
        deadline,
        region,
        policy,
    )?;
    Ok(EncodedChannelTeletextDocumentCandidate {
        selection,
        generation,
        deadline,
        entry_addon_id,
        sources,
        requests,
        v1_region: originals.v1_region,
        guide_source: None,
        producers: originals.producers,
        provenance: originals.provenance,
        metadata: originals.metadata,
        encoded: originals.encoded,
        descriptor: originals.descriptor,
        transmissions: originals.transmissions,
    })
}

/// Compile validated originals without invoking packages or fabricating receipts.
/// Saved V1 region/lifecycle must be supplied explicitly by the historical owner.
pub(super) fn encode_original_channel_documents(
    selection: &TeletextChannelSelectionSnapshot,
    entry_addon_id: Option<&str>,
    documents: Vec<TeletextProfileDocument>,
    deadline: Instant,
    region: Option<TeletextEncodingRegion>,
    policy: TeletextChannelDocumentEncodingPolicy,
) -> Result<EncodedChannelTeletextOriginals, ChannelTeletextDocumentEncodingError> {
    encode_original_channel_documents_with_physical_overlays(
        selection,
        entry_addon_id,
        documents,
        Vec::new(),
        deadline,
        region,
        policy,
    )
}

/// Compile trusted physical-row inputs through the same cold service and encoder.
/// This internal seam neither authenticates imported editing permissions nor
/// constructs an invocation, saved concept or publication authority. Its caller
/// must validate the authoritative receipt and complete protected effects first.
pub(super) fn encode_original_channel_documents_with_physical_overlays(
    selection: &TeletextChannelSelectionSnapshot,
    entry_addon_id: Option<&str>,
    documents: Vec<TeletextProfileDocument>,
    overlays: Vec<TeletextPhysicalBaseOverlay>,
    deadline: Instant,
    region: Option<TeletextEncodingRegion>,
    policy: TeletextChannelDocumentEncodingPolicy,
) -> Result<EncodedChannelTeletextOriginals, ChannelTeletextDocumentEncodingError> {
    check_deadline(deadline)?;
    if documents.len() != selection.bindings.len() {
        return Err(ChannelTeletextDocumentEncodingError::Association);
    }
    let entry_producer = entry_addon_id.and_then(|addon| {
        selection
            .bindings
            .iter()
            .position(|binding| binding.addon_id == addon)
    });
    encode_original_producer_documents(
        entry_producer,
        documents,
        overlays,
        deadline,
        region,
        policy,
    )
}

/// Shared content compiler. Origins and producer-to-membership associations stay
/// with the caller; an entry ordinal conveys no selection or publication authority.
pub(super) fn encode_original_producer_documents(
    entry_producer: Option<usize>,
    documents: Vec<TeletextProfileDocument>,
    overlays: Vec<TeletextPhysicalBaseOverlay>,
    deadline: Instant,
    region: Option<TeletextEncodingRegion>,
    policy: TeletextChannelDocumentEncodingPolicy,
) -> Result<EncodedChannelTeletextOriginals, ChannelTeletextDocumentEncodingError> {
    check_deadline(deadline)?;
    check_producer_budget(documents.len(), &policy)?;
    if documents.is_empty() {
        if !overlays.is_empty() {
            return Err(ChannelTeletextDocumentEncodingError::Association);
        }
        return Ok(EncodedChannelTeletextOriginals {
            v1_region: None,
            producers: Vec::new(),
            provenance: Vec::new(),
            metadata: None,
            encoded: None,
            descriptor: None,
            transmissions: Vec::new(),
        });
    }
    let has_v1 = documents
        .iter()
        .any(|document| matches!(document, TeletextProfileDocument::Base(_)));
    if has_v1 != region.is_some() {
        return Err(ChannelTeletextDocumentEncodingError::Association);
    }
    let producers = prepare_producers(documents, deadline, region, &policy)?;
    let metadata = entry_metadata(entry_producer, &producers, policy.entry)?;
    check_deadline(deadline)?;
    let mut composition =
        TeletextColdProducerComposition::new(producers, metadata, policy.composition)
            .map_err(|error| ChannelTeletextDocumentEncodingError::Composition(Box::new(error)))?;
    if !overlays.is_empty() {
        composition = composition
            .with_physical_base_overlays(overlays, policy.composition)
            .map_err(|error| ChannelTeletextDocumentEncodingError::Composition(Box::new(error)))?;
    }
    let (producers, provenance, metadata, declared) = composition.into_parts();
    check_deadline(deadline)?;
    let encoded = encode_teletext_authored_service_before(
        declared,
        &policy.transmissions,
        &policy.encoding,
        deadline,
    )
    .map_err(|error| {
        if matches!(
            &error.reason,
            TeletextAuthoredServiceEncodingFailure::TimedOut
        ) {
            ChannelTeletextDocumentEncodingError::TimedOut
        } else {
            ChannelTeletextDocumentEncodingError::Encoding(Box::new(error))
        }
    })?;
    check_deadline(deadline)?;
    let descriptor = TeletextServiceDescriptor::from_authored_service(&encoded);
    check_deadline(deadline)?;
    Ok(EncodedChannelTeletextOriginals {
        v1_region: region,
        producers,
        provenance,
        metadata: Some(metadata),
        encoded: Some(encoded),
        descriptor: Some(descriptor),
        transmissions: policy.transmissions,
    })
}

pub(super) fn maximum_producers(policy: &TeletextChannelDocumentEncodingPolicy) -> usize {
    policy
        .input
        .max_producers
        .min(policy.composition.max_producers)
}

fn check_producer_budget(
    count: usize,
    policy: &TeletextChannelDocumentEncodingPolicy,
) -> Result<(), ChannelTeletextDocumentEncodingError> {
    let maximum = maximum_producers(policy);
    if count > maximum {
        return Err(ChannelTeletextDocumentEncodingError::ProducerBudget { count, maximum });
    }
    Ok(())
}

fn prepare_producers(
    documents: Vec<TeletextProfileDocument>,
    deadline: Instant,
    region: Option<TeletextEncodingRegion>,
    policy: &TeletextChannelDocumentEncodingPolicy,
) -> Result<Vec<TeletextProducerPresentation>, ChannelTeletextDocumentEncodingError> {
    check_producer_budget(documents.len(), policy)?;
    preflight_producer_storage(documents.len(), policy.composition.max_storage_bytes)?;
    let mut producers = Vec::with_capacity(documents.len());
    for (binding, document) in documents.into_iter().enumerate() {
        check_deadline(deadline)?;
        let declared = match document {
            TeletextProfileDocument::Base(base) => lift_base(
                base,
                region.ok_or(ChannelTeletextDocumentEncodingError::Association)?,
                policy.validation.service,
                policy.validation.declarations,
            )
            .map_err(|error| ChannelTeletextDocumentEncodingError::Lift {
                binding,
                error: Box::new(error),
            })?,
            TeletextProfileDocument::Authored(declared) => *declared,
        };
        check_deadline(deadline)?;
        let producer = TeletextProducerPresentation::new(
            declared,
            TeletextProducerPresentationBudget {
                max_pages: policy.validation.declarations.max_displays,
            },
        )
        .map_err(|error| ChannelTeletextDocumentEncodingError::Presentation { binding, error })?;
        check_deadline(deadline)?;
        producers.push(producer);
    }
    Ok(producers)
}

fn preflight_producer_storage(
    count: usize,
    maximum: usize,
) -> Result<(), ChannelTeletextDocumentEncodingError> {
    let bytes = count.checked_mul(size_of::<TeletextProducerPresentation>());
    let reason = match bytes {
        Some(count) if isize::try_from(count).is_ok() && count <= maximum => return Ok(()),
        Some(count) if isize::try_from(count).is_ok() => {
            TeletextProducerCompositionFailure::Budget {
                category: "producer storage",
                count,
                maximum,
            }
        }
        _ => TeletextProducerCompositionFailure::StorageOverflow,
    };
    Err(ChannelTeletextDocumentEncodingError::Composition(Box::new(
        TeletextProducerCompositionError {
            producer: None,
            page: None,
            display: None,
            resource: None,
            reason,
        },
    )))
}

fn entry_metadata(
    entry_producer: Option<usize>,
    producers: &[TeletextProducerPresentation],
    requested: Option<TeletextPageAddress>,
) -> Result<TeletextProducerCompositionMetadata, ChannelTeletextDocumentEncodingError> {
    let binding = entry_producer.ok_or(ChannelTeletextDocumentEncodingError::Association)?;
    let original = producers
        .get(binding)
        .ok_or(ChannelTeletextDocumentEncodingError::Association)?
        .original()
        .semantic()
        .base();
    let mut entries = original
        .pages
        .iter()
        .filter(|page| page.address.page == original.initial_page);
    let entry = match requested {
        Some(entry)
            if entry.page == original.initial_page && entries.any(|page| page.address == entry) =>
        {
            entry
        }
        Some(entry) => {
            return Err(ChannelTeletextDocumentEncodingError::EntryAddress { binding, entry })
        }
        None => {
            let first = entries
                .next()
                .ok_or(ChannelTeletextDocumentEncodingError::Association)?;
            if entries.next().is_some() {
                return Err(ChannelTeletextDocumentEncodingError::EntryRequired { binding });
            }
            first.address
        }
    };
    Ok(TeletextProducerCompositionMetadata {
        language: original.language.clone(),
        entry,
    })
}

fn check_deadline(deadline: Instant) -> Result<(), ChannelTeletextDocumentEncodingError> {
    if Instant::now() >= deadline {
        Err(ChannelTeletextDocumentEncodingError::TimedOut)
    } else {
        Ok(())
    }
}
