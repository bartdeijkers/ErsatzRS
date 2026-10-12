//! Native preparation of one selection-bound document, without publication.

use std::{fmt, time::Instant};

use ersatzrs_app::{
    teletext_guide::SuppliedTeletextGuideContext,
    teletext_profile_documents::{
        BoundTeletextDocumentError, BoundTeletextDocumentInput, TeletextProfileDocument,
        TeletextProfileDocumentBudget,
    },
    ApplicationServices,
};
use ersatzrs_domain::{
    EntityId, TeletextDeclaredService, TeletextDeclaredServiceBudget, TeletextDeclaredServiceError,
    TeletextDisplayMotSelection, TeletextPageSetDraft, TeletextProducerPagePresentation,
    TeletextProducerPresentation, TeletextProducerPresentationBudget,
    TeletextProducerPresentationError, TeletextSelectionSnapshot, TeletextServiceBudget,
    TeletextServiceError,
};
use ersatzrs_ffmpeg::teletext::{
    encode_teletext_authored_service_before, lift_teletext_v1_page_set,
    EncodedTeletextAuthoredService, TeletextAuthoredServiceEncodingBudget,
    TeletextAuthoredServiceEncodingError, TeletextAuthoredServiceEncodingFailure,
    TeletextEncodingRegion, TeletextServiceDescriptor, TeletextServiceResourceTransmission,
};

/// Trusted validation, compilation and explicit resource lifecycle inputs.
/// Lifecycle values are never inferred from producer content or prior output.
pub struct TeletextDocumentEncodingPolicy {
    pub validation: TeletextProfileDocumentBudget,
    pub v1_region: TeletextEncodingRegion,
    pub encoding: TeletextAuthoredServiceEncodingBudget,
    pub transmissions: Vec<TeletextServiceResourceTransmission>,
}

/// Associated observed selection and complete native candidate, not admission.
/// The preview source is retained inside the encoder's immutable declared service.
pub struct EncodedBoundTeletextDocumentCandidate {
    selection: TeletextSelectionSnapshot,
    generation: String,
    deadline: Instant,
    encoded: EncodedTeletextAuthoredService,
    descriptor: TeletextServiceDescriptor,
    presentations: Vec<TeletextProducerPagePresentation>,
}

impl EncodedBoundTeletextDocumentCandidate {
    #[must_use]
    pub const fn selection(&self) -> &TeletextSelectionSnapshot {
        &self.selection
    }
    /// Opaque request correlation, without generation ownership.
    #[must_use]
    pub fn generation(&self) -> &str {
        &self.generation
    }
    #[must_use]
    pub const fn deadline(&self) -> Instant {
        self.deadline
    }
    #[must_use]
    pub const fn encoded(&self) -> &EncodedTeletextAuthoredService {
        &self.encoded
    }
    #[must_use]
    pub const fn descriptor(&self) -> &TeletextServiceDescriptor {
        &self.descriptor
    }
    /// Original producer-local defaults and uncertainty, captured before encoding.
    /// These snapshots are not permission to emit derived presentation packets.
    #[must_use]
    pub fn producer_presentations(&self) -> &[TeletextProducerPagePresentation] {
        &self.presentations
    }
}

impl fmt::Debug for EncodedBoundTeletextDocumentCandidate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EncodedBoundTeletextDocumentCandidate")
            .field("selection", &self.selection)
            .field("pages", &self.encoded.pages().len())
            .field("units", &self.encoded.unit_count())
            .finish_non_exhaustive()
    }
}

/// Complete failures retain numeric provenance, without raw document text.
#[derive(Debug, thiserror::Error)]
pub enum TeletextDocumentEncodingError {
    #[error(transparent)]
    Document(#[from] BoundTeletextDocumentError),
    #[error("v1 semantic lift failed: {0}")]
    Lift(Box<TeletextServiceError>),
    #[error("v1 declaration authentication failed: {0}")]
    Declaration(Box<TeletextDeclaredServiceError>),
    #[error(transparent)]
    Presentation(#[from] TeletextProducerPresentationError),
    #[error("v1 display declarations {count} exceed trusted budget {maximum}")]
    DeclarationBudget { count: usize, maximum: usize },
    #[error("native document encoding failed: {0}")]
    Encoding(Box<TeletextAuthoredServiceEncodingError>),
    #[error("teletext document preparation exceeded its shared deadline")]
    TimedOut,
    #[error("teletext document validation/encoding worker failed")]
    WorkerFailed,
}

/// Invoke, validate and compile one exact bound document under one deadline.
///
/// Validation, lossless v1 lifting and complete native encoding share one joined
/// blocking task. V2 declarations move directly into the encoder. The selection
/// is read again after CPU work; publication still requires its own fresh fence.
/// Caller abandonment does not interrupt synchronous CPU work. There is no
/// contract retry, partial result, configuration write or lifecycle inference.
/// This is single-producer preparation, not mixed channel composition.
/// # Errors
/// Returns strict document, lift, declaration, compiler, worker or deadline errors.
pub async fn prepare_bound_teletext_document_candidate(
    application: &ApplicationServices,
    channel_id: EntityId,
    addon_id: &str,
    expected_binding_revision: i64,
    context: &SuppliedTeletextGuideContext<'_>,
    policy: TeletextDocumentEncodingPolicy,
) -> Result<EncodedBoundTeletextDocumentCandidate, TeletextDocumentEncodingError> {
    let input = application
        .invoke_bound_teletext_document(channel_id, addon_id, expected_binding_revision, context)
        .await?;
    let candidate = tokio::task::spawn_blocking(move || encode_document(input, &policy))
        .await
        .map_err(|_| TeletextDocumentEncodingError::WorkerFailed)??;
    application
        .recheck_teletext_document_selection(candidate.selection(), candidate.deadline())
        .await?;
    Ok(candidate)
}

fn encode_document(
    input: BoundTeletextDocumentInput,
    policy: &TeletextDocumentEncodingPolicy,
) -> Result<EncodedBoundTeletextDocumentCandidate, TeletextDocumentEncodingError> {
    let (selection, generation, document, deadline) =
        input.validate(policy.validation)?.into_parts();
    let declared = match document {
        TeletextProfileDocument::Base(base) => lift_base(
            base,
            policy.v1_region,
            policy.validation.service,
            policy.validation.declarations,
        )?,
        TeletextProfileDocument::Authored(declared) => *declared,
    };
    check_deadline(deadline)?;
    let (declared, presentations) = TeletextProducerPresentation::new(
        declared,
        TeletextProducerPresentationBudget {
            max_pages: policy.validation.declarations.max_displays,
        },
    )?
    .into_parts();
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
            TeletextDocumentEncodingError::TimedOut
        } else {
            TeletextDocumentEncodingError::Encoding(Box::new(error))
        }
    })?;
    let descriptor = TeletextServiceDescriptor::from_authored_service(&encoded);
    check_deadline(deadline)?;
    Ok(EncodedBoundTeletextDocumentCandidate {
        selection,
        generation,
        deadline,
        encoded,
        descriptor,
        presentations,
    })
}

pub(super) fn lift_base(
    base: TeletextPageSetDraft,
    region: TeletextEncodingRegion,
    service_budget: TeletextServiceBudget,
    declaration_budget: TeletextDeclaredServiceBudget,
) -> Result<TeletextDeclaredService, TeletextDocumentEncodingError> {
    let semantic = lift_teletext_v1_page_set(base, region, service_budget)
        .map_err(|error| TeletextDocumentEncodingError::Lift(Box::new(error)))?;
    let count = semantic.base().pages.len();
    if count > declaration_budget.max_displays {
        return Err(TeletextDocumentEncodingError::DeclarationBudget {
            count,
            maximum: declaration_budget.max_displays,
        });
    }
    let selections = semantic
        .base()
        .pages
        .iter()
        .map(|page| TeletextDisplayMotSelection {
            display: page.address,
            mot: None,
        })
        .collect();
    TeletextDeclaredService::new(
        semantic,
        Vec::new(),
        selections,
        Vec::new(),
        declaration_budget,
    )
    .map_err(|error| TeletextDocumentEncodingError::Declaration(Box::new(error)))
}

fn check_deadline(deadline: Instant) -> Result<(), TeletextDocumentEncodingError> {
    if Instant::now() >= deadline {
        Err(TeletextDocumentEncodingError::TimedOut)
    } else {
        Ok(())
    }
}
