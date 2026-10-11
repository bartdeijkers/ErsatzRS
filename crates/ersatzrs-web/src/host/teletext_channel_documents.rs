//! Native cold channel preparation, without publication or warm-cache authority.

use std::{fmt, mem::size_of, time::Instant};

use ersatzrs_app::{
    teletext_channel_documents::{
        BoundChannelTeletextDocumentInput, ChannelTeletextDocumentError,
        TeletextChannelDocumentInputBudget,
    },
    teletext_guide::SuppliedTeletextGuideContext,
    teletext_profile_documents::{TeletextProfileDocument, TeletextProfileDocumentBudget},
    ApplicationServices,
};
use ersatzrs_domain::{
    EntityId, TeletextChannelSelectionSnapshot, TeletextColdProducerComposition,
    TeletextComposedProducerPage, TeletextPageAddress, TeletextProducerCompositionBudget,
    TeletextProducerCompositionError, TeletextProducerCompositionFailure,
    TeletextProducerCompositionMetadata, TeletextProducerPresentation,
    TeletextProducerPresentationBudget, TeletextProducerPresentationError,
};
use ersatzrs_ffmpeg::teletext::{
    encode_teletext_authored_service, EncodedTeletextAuthoredService,
    TeletextAuthoredServiceEncodingBudget, TeletextAuthoredServiceEncodingError,
    TeletextEncodingRegion, TeletextServiceDescriptor, TeletextServiceResourceTransmission,
};

use super::teletext_documents::{lift_base, TeletextDocumentEncodingError};

/// Trusted input/model/encoding limits and explicit resource lifecycle records.
/// Entry language always comes from the selected original producer, never policy.
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
    producers: Vec<TeletextProducerPresentation>,
    provenance: Vec<TeletextComposedProducerPage>,
    metadata: Option<TeletextProducerCompositionMetadata>,
    encoded: Option<EncodedTeletextAuthoredService>,
    descriptor: Option<TeletextServiceDescriptor>,
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
    #[must_use]
    pub fn entry_addon_id(&self) -> Option<&str> {
        self.entry_addon_id.as_deref()
    }
    /// `producers()[i]` belongs to exactly `selection().bindings[i]`.
    #[must_use]
    pub fn producers(&self) -> &[TeletextProducerPresentation] {
        &self.producers
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
    let input = application
        .invoke_channel_teletext_documents(channel_id, entry_addon_id, context, policy.input)
        .await?;
    let candidate = tokio::task::spawn_blocking(move || encode_channel(input, &policy))
        .await
        .map_err(|_| ChannelTeletextDocumentEncodingError::WorkerFailed)??;
    application
        .recheck_channel_teletext_document_selection(candidate.selection(), candidate.deadline())
        .await?;
    check_deadline(candidate.deadline())?;
    Ok(candidate)
}

fn encode_channel(
    input: BoundChannelTeletextDocumentInput,
    policy: &TeletextChannelDocumentEncodingPolicy,
) -> Result<EncodedChannelTeletextDocumentCandidate, ChannelTeletextDocumentEncodingError> {
    check_deadline(input.deadline())?;
    let (selection, generation, documents, entry_addon_id, deadline) =
        input.validate(policy.validation)?.into_parts();
    check_deadline(deadline)?;
    if documents.len() != selection.bindings.len() {
        return Err(ChannelTeletextDocumentEncodingError::Association);
    }
    let mut candidate = EncodedChannelTeletextDocumentCandidate {
        selection,
        generation,
        deadline,
        entry_addon_id,
        producers: Vec::new(),
        provenance: Vec::new(),
        metadata: None,
        encoded: None,
        descriptor: None,
    };
    if documents.is_empty() {
        return Ok(candidate);
    }
    let producers = prepare_producers(documents, deadline, policy)?;
    let metadata = entry_metadata(&candidate, &producers, policy.entry)?;
    check_deadline(deadline)?;
    let (producers, provenance, metadata, declared) =
        TeletextColdProducerComposition::new(producers, metadata, policy.composition)
            .map_err(|error| ChannelTeletextDocumentEncodingError::Composition(Box::new(error)))?
            .into_parts();
    check_deadline(deadline)?;
    let encoded =
        encode_teletext_authored_service(declared, &policy.transmissions, &policy.encoding)
            .map_err(|error| ChannelTeletextDocumentEncodingError::Encoding(Box::new(error)))?;
    check_deadline(deadline)?;
    let descriptor = TeletextServiceDescriptor::from_authored_service(&encoded);
    check_deadline(deadline)?;
    candidate.producers = producers;
    candidate.provenance = provenance;
    candidate.metadata = Some(metadata);
    candidate.encoded = Some(encoded);
    candidate.descriptor = Some(descriptor);
    Ok(candidate)
}

fn prepare_producers(
    documents: Vec<TeletextProfileDocument>,
    deadline: Instant,
    policy: &TeletextChannelDocumentEncodingPolicy,
) -> Result<Vec<TeletextProducerPresentation>, ChannelTeletextDocumentEncodingError> {
    if documents.len() > policy.composition.max_producers {
        return Err(ChannelTeletextDocumentEncodingError::ProducerBudget {
            count: documents.len(),
            maximum: policy.composition.max_producers,
        });
    }
    preflight_producer_storage(documents.len(), policy.composition.max_storage_bytes)?;
    let mut producers = Vec::with_capacity(documents.len());
    for (binding, document) in documents.into_iter().enumerate() {
        check_deadline(deadline)?;
        let declared = match document {
            TeletextProfileDocument::Base(base) => lift_base(
                base,
                policy.v1_region,
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
    candidate: &EncodedChannelTeletextDocumentCandidate,
    producers: &[TeletextProducerPresentation],
    requested: Option<TeletextPageAddress>,
) -> Result<TeletextProducerCompositionMetadata, ChannelTeletextDocumentEncodingError> {
    let addon = candidate
        .entry_addon_id()
        .ok_or(ChannelTeletextDocumentEncodingError::Association)?;
    let binding = candidate
        .selection
        .bindings
        .iter()
        .position(|binding| binding.addon_id == addon)
        .ok_or(ChannelTeletextDocumentEncodingError::Association)?;
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
