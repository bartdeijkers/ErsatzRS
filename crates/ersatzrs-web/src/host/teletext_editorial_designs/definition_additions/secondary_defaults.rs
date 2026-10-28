//! Explicit context opt-in/withdrawal through retained receipt and concept CAS.
use super::{
    apply, check_deadline, finish_encode, fmt, Action, ApplicationServices, Baselines,
    EncodedStoredDesign, EntityId, Instant, NativeTeletextEditorialImportError,
    NativeTeletextExportPolicy, Request, Result, SavedEditorialTeletextDesign,
    TeletextDesignMutation, TeletextEditorialChanges,
};
use ersatzrs_app::teletext_design_documents::TeletextSavedSecondaryDefaultAgreement;
use ersatzrs_domain::{TeletextProducerPresentationSource, TeletextSecondaryDefaultAgreement};

/// None explicitly withdraws the agreement; it never means no second G0.
/// The producer ordinal belongs to the exact retained receipt, not a package ID.
pub struct NativeTeletextSecondaryDefaultRequest {
    pub name: String,
    pub receipt_id: EntityId,
    pub receipt_digest: String,
    pub expected_design_head: i64,
    pub producer: usize,
    pub agreement: Option<TeletextSecondaryDefaultAgreement>,
}
impl fmt::Debug for NativeTeletextSecondaryDefaultRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NativeTeletextSecondaryDefaultRequest")
            .field("receipt_id", &self.receipt_id)
            .field("expected_design_head", &self.expected_design_head)
            .field("producer", &self.producer)
            .finish_non_exhaustive()
    }
}

/// Save a receipt-bound author choice as a concept, never a live selection or
/// publication. Retain full originals, other edits, protected cells and root undo.
/// # Errors
/// Rejects stale authority, absent/protected consumers, unchanged choices,
/// native/effect failures, limits, cancellation and exact-head conflicts.
pub async fn select_teletext_editorial_secondary_default(
    application: &ApplicationServices,
    request: NativeTeletextSecondaryDefaultRequest,
    policy: NativeTeletextExportPolicy,
    deadline: Instant,
) -> Result<TeletextDesignMutation<SavedEditorialTeletextDesign>> {
    apply(
        application,
        Request {
            name: request.name,
            receipt_id: request.receipt_id,
            receipt_digest: request.receipt_digest,
            expected_design_head: request.expected_design_head,
            action: Action::SecondaryDefault {
                producer: request.producer,
                agreement: request.agreement,
            },
        },
        policy,
        deadline,
    )
    .await
}

pub(super) fn encode(
    mut baselines: Baselines,
    producer: usize,
    agreement: Option<TeletextSecondaryDefaultAgreement>,
    revision: i64,
    policy: &NativeTeletextExportPolicy,
    deadline: Instant,
) -> Result<(EncodedStoredDesign, String)> {
    check_deadline(deadline)?;
    let latest = baselines.latest.as_ref().unwrap_or(&baselines.root);
    let fail = || NativeTeletextEditorialImportError::Association;
    let source = latest
        .original
        .view()
        .producers()
        .get(producer)
        .ok_or_else(fail)?;
    if source.secondary_default_agreement() == agreement {
        return Err(NativeTeletextEditorialImportError::NoChanges);
    }
    let project = &latest.body.project;
    let owner = project
        .producers
        .iter()
        .find(|value| value.ordinal == producer)
        .ok_or_else(fail)?;
    let metadata = owner.authoring.as_ref().ok_or_else(fail)?;
    for page in source.pages().iter().filter(|page| {
        matches!(
            page.source(),
            TeletextProducerPresentationSource::Header { .. }
                | TeletextProducerPresentationSource::MotFallback { .. }
        )
    }) {
        check_deadline(deadline)?;
        if !project.pages.iter().any(|value| {
            value.producer_ordinal == producer
                && value.page.page == page.address().page.get()
                && value.page.subcode == page.address().subcode.get()
        }) || !metadata.pages.iter().any(|value| {
            value.page.page == page.address().page.get()
                && value.page.subcode == page.address().subcode.get()
                && value.regions.iter().any(|region| {
                    region.kind
                        != ersatzrs_addon_contract::TeletextAuthoringRegionKindWire::Generated
                })
        }) {
            return Err(fail());
        }
    }
    let (id, profile_revision) = (owner.profile_id, owner.profile_revision);
    baselines
        .secondary_default_agreements
        .retain(|value| value.producer() != producer);
    if let Some(agreement) = agreement {
        baselines
            .secondary_default_agreements
            .push(TeletextSavedSecondaryDefaultAgreement::new(
                producer,
                id,
                profile_revision,
                agreement,
            ));
        baselines
            .secondary_default_agreements
            .sort_by_key(|value| value.producer());
    }
    finish_encode(
        &baselines,
        &TeletextEditorialChanges::default(),
        revision,
        policy,
        deadline,
    )
}
