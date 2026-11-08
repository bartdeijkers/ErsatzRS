//! Unpublished native compilation for exact base/published Scripted origins.
//!
//! The whole selection retains Off members and transition expiry. Content uses
//! the shared cold compiler; no effective member becomes a fabricated base token.

use std::{fmt, sync::Arc, time::Instant};

use ersatzrs_app::{
    teletext_channel_documents::{TeletextChannelRequestTemplates, PREPARATION_TIMEOUT},
    teletext_effective_channel_documents::{
        PreparedEffectiveChannelTeletextDocumentParts, PreparedEffectiveChannelTeletextDocuments,
    },
    teletext_guide::SuppliedTeletextGuideContext,
    teletext_profile_documents::{TeletextProfileDocument, TeletextValidatedDocumentSource},
    ApplicationServices,
};
use ersatzrs_core::locking::TeletextWorkHold;
use ersatzrs_domain::{
    EntityId, TeletextComposedProducerPage, TeletextEffectiveChannelSelectionSnapshot,
    TeletextProducerCompositionMetadata, TeletextProducerPresentation,
};
use ersatzrs_ffmpeg::teletext::{
    EncodedTeletextAuthoredService, TeletextEncodingRegion, TeletextServiceDescriptor,
    TeletextServiceResourceTransmission,
};

use super::teletext_channel_documents::{
    encode_original_producer_documents, maximum_producers, ChannelTeletextDocumentEncodingError,
    EncodedChannelTeletextOriginals, TeletextChannelDocumentEncodingPolicy,
};

/// Complete checked content and exact effective origin observation, still
/// unpublished. `producer_bindings()[i]` associates every original/producer and
/// composition provenance ordinal with its whole-selection binding. Off members
/// have no producer ordinal but remain in every later whole-channel fence.
pub struct EncodedEffectiveChannelTeletextDocumentCandidate {
    selection: Arc<TeletextEffectiveChannelSelectionSnapshot>,
    producer_bindings: Vec<usize>,
    generation: String,
    deadline: Instant,
    entry_addon_id: Option<String>,
    sources: Vec<TeletextValidatedDocumentSource>,
    requests: TeletextChannelRequestTemplates,
    // Keep the optional historical source out of every ordinary/static async
    // frame; the bounded owned association moves only on the guide path.
    guide_source: Option<Box<ersatzrs_core::repositories::TeletextGuideSourceFence>>,
    originals: EncodedChannelTeletextOriginals,
}

impl EncodedEffectiveChannelTeletextDocumentCandidate {
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
    pub const fn deadline(&self) -> Instant {
        self.deadline
    }
    #[must_use]
    pub fn entry_addon_id(&self) -> Option<&str> {
        self.entry_addon_id.as_deref()
    }
    #[must_use]
    pub fn sources(&self) -> &[TeletextValidatedDocumentSource] {
        &self.sources
    }
    #[must_use]
    pub const fn requests(&self) -> &TeletextChannelRequestTemplates {
        &self.requests
    }
    /// Historical semantic source association, not current-role authority.
    #[must_use]
    pub fn guide_source(&self) -> Option<&ersatzrs_core::repositories::TeletextGuideSourceFence> {
        self.guide_source.as_deref()
    }
    pub(super) fn with_guide_source(
        mut self,
        source: ersatzrs_core::repositories::TeletextGuideSourceFence,
    ) -> Self {
        self.guide_source = Some(Box::new(source));
        self
    }
    #[must_use]
    pub const fn v1_region(&self) -> Option<TeletextEncodingRegion> {
        self.originals.v1_region
    }
    #[must_use]
    pub fn producers(&self) -> &[TeletextProducerPresentation] {
        &self.originals.producers
    }
    #[must_use]
    pub fn provenance(&self) -> &[TeletextComposedProducerPage] {
        &self.originals.provenance
    }
    #[must_use]
    pub const fn metadata(&self) -> Option<&TeletextProducerCompositionMetadata> {
        self.originals.metadata.as_ref()
    }
    #[must_use]
    pub const fn encoded(&self) -> Option<&EncodedTeletextAuthoredService> {
        self.originals.encoded.as_ref()
    }
    #[must_use]
    pub const fn descriptor(&self) -> Option<&TeletextServiceDescriptor> {
        self.originals.descriptor.as_ref()
    }
    #[must_use]
    pub fn transmissions(&self) -> &[TeletextServiceResourceTransmission] {
        &self.originals.transmissions
    }
}

impl fmt::Debug for EncodedEffectiveChannelTeletextDocumentCandidate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EncodedEffectiveChannelTeletextDocumentCandidate")
            .field("selection", &self.selection)
            .field("producer_count", &self.producer_bindings.len())
            .field("page_count", &self.originals.provenance.len())
            .field("encoded", &self.originals.encoded.is_some())
            .finish_non_exhaustive()
    }
}

/// Invoke and compile all effective producers under one admission and original
/// deadline. The actual CPU worker retains its work hold after caller abandonment.
/// Join native work, then recheck the whole origin/Off/expiry fence. This produces
/// no saved design, pending request, accepted generation or stream activation.
/// # Errors
/// Preserves preparation, association, compiler, deadline and whole-selection errors.
pub async fn prepare_effective_channel_teletext_document_candidate(
    application: &ApplicationServices,
    channel_id: EntityId,
    entry_addon_id: Option<&str>,
    context: &SuppliedTeletextGuideContext<'_>,
    policy: TeletextChannelDocumentEncodingPolicy,
) -> Result<EncodedEffectiveChannelTeletextDocumentCandidate, ChannelTeletextDocumentEncodingError>
{
    let deadline = Instant::now()
        .checked_add(PREPARATION_TIMEOUT)
        .ok_or(ChannelTeletextDocumentEncodingError::TimedOut)?;
    let guard = super::teletext_work::begin(application, Some(deadline)).await?;
    prepare_effective_channel_teletext_document_candidate_with_hold(
        application,
        channel_id,
        entry_addon_id,
        context,
        policy,
        (deadline, guard.hold_work()),
    )
    .await
}

/// Reuse an admitted operation without renewing its deadline or native worker slots.
pub(super) async fn prepare_effective_channel_teletext_document_candidate_with_hold(
    application: &ApplicationServices,
    channel_id: EntityId,
    entry_addon_id: Option<&str>,
    context: &SuppliedTeletextGuideContext<'_>,
    policy: TeletextChannelDocumentEncodingPolicy,
    admission: (Instant, TeletextWorkHold),
) -> Result<EncodedEffectiveChannelTeletextDocumentCandidate, ChannelTeletextDocumentEncodingError>
{
    prepare_effective_checked(
        application,
        channel_id,
        entry_addon_id,
        context,
        policy,
        None,
        admission,
    )
    .await
}

/// Bind rendered intent before producer invocation, keeping the same final
/// joined-worker scope fence and original deadline as ordinary preparation.
pub(super) async fn prepare_effective_channel_teletext_document_candidate_checked_with_hold(
    application: &ApplicationServices,
    channel_id: EntityId,
    entry_addon_id: Option<&str>,
    context: &SuppliedTeletextGuideContext<'_>,
    policy: TeletextChannelDocumentEncodingPolicy,
    expected: &TeletextEffectiveChannelSelectionSnapshot,
    admission: (Instant, TeletextWorkHold),
) -> Result<EncodedEffectiveChannelTeletextDocumentCandidate, ChannelTeletextDocumentEncodingError>
{
    prepare_effective_checked(
        application,
        channel_id,
        entry_addon_id,
        context,
        policy,
        Some(expected),
        admission,
    )
    .await
}

async fn prepare_effective_checked(
    application: &ApplicationServices,
    channel_id: EntityId,
    entry_addon_id: Option<&str>,
    context: &SuppliedTeletextGuideContext<'_>,
    policy: TeletextChannelDocumentEncodingPolicy,
    expected: Option<&TeletextEffectiveChannelSelectionSnapshot>,
    admission: (Instant, TeletextWorkHold),
) -> Result<EncodedEffectiveChannelTeletextDocumentCandidate, ChannelTeletextDocumentEncodingError>
{
    let (deadline, hold) = admission;
    check_deadline(deadline)?;
    let mut budget = policy.input;
    budget.max_producers = maximum_producers(&policy);
    let input = if let Some(expected) = expected {
        application
            .invoke_effective_channel_teletext_documents_with_expected_hold(
                channel_id,
                entry_addon_id,
                context,
                budget,
                (deadline, hold.clone(), expected),
            )
            .await?
    } else {
        application
            .invoke_effective_channel_teletext_documents_with_hold(
                channel_id,
                entry_addon_id,
                context,
                budget,
                (deadline, hold.clone()),
            )
            .await?
    };
    let candidate = super::teletext_work::spawn_blocking(hold, move || {
        check_deadline(input.deadline())?;
        let prepared = input.validate(policy.validation)?;
        encode_effective_channel(prepared, policy)
    })
    .await
    .map_err(|_| ChannelTeletextDocumentEncodingError::WorkerFailed)??;
    application
        .recheck_effective_channel_teletext_document_selection(
            candidate.selection(),
            candidate.deadline(),
        )
        .await?;
    check_deadline(candidate.deadline())?;
    Ok(candidate)
}

fn encode_effective_channel(
    prepared: PreparedEffectiveChannelTeletextDocuments,
    policy: TeletextChannelDocumentEncodingPolicy,
) -> Result<EncodedEffectiveChannelTeletextDocumentCandidate, ChannelTeletextDocumentEncodingError>
{
    let PreparedEffectiveChannelTeletextDocumentParts {
        selection,
        generation,
        producer_bindings,
        documents,
        sources,
        requests,
        entry_addon_id,
        deadline,
    } = prepared.into_parts();
    check_deadline(deadline)?;
    let expected_producers = selection
        .bindings
        .iter()
        .enumerate()
        .filter(|(_, binding)| binding.enabled())
        .map(|(index, _)| index);
    if producer_bindings.iter().copied().ne(expected_producers)
        || documents.len() != producer_bindings.len()
        || sources.len() != documents.len()
    {
        return Err(ChannelTeletextDocumentEncodingError::Association);
    }
    let entry_producer = entry_addon_id.as_deref().and_then(|addon| {
        producer_bindings
            .iter()
            .position(|index| selection.bindings[*index].addon_id() == addon)
    });
    let region = documents
        .iter()
        .any(TeletextProfileDocument::is_base)
        .then_some(policy.v1_region);
    let originals = encode_original_producer_documents(
        entry_producer,
        documents,
        Vec::new(),
        deadline,
        region,
        policy,
    )?;
    check_deadline(deadline)?;
    Ok(EncodedEffectiveChannelTeletextDocumentCandidate {
        selection,
        producer_bindings,
        generation,
        deadline,
        entry_addon_id,
        sources,
        requests,
        guide_source: None,
        originals,
    })
}

fn check_deadline(deadline: Instant) -> Result<(), ChannelTeletextDocumentEncodingError> {
    if Instant::now() >= deadline {
        Err(ChannelTeletextDocumentEncodingError::TimedOut)
    } else {
        Ok(())
    }
}
