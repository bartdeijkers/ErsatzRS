//! Explicit announcement authority reuses the retained-receipt/native worker.
use super::{
    apply, check_deadline, fmt, repeat, Action, ApplicationServices, EntityId, Instant,
    NativeTeletextEditorialImportError, NativeTeletextExportPolicy, Request, Result,
    SavedEditorialTeletextDesign, TeletextDesignMutation, TeletextEditorialChanges,
    TeletextServiceBudget,
};
use ersatzrs_app::teletext_design_documents::TeletextSavedObjectSubpageAnnouncement;

/// Add missing S1 requirements on already selected POP/GPOP pages.
/// TTI and sidecar uploads cannot grant this distinct authoring action.
pub struct NativeTeletextObjectSubpagesAnnounceRequest {
    pub name: String,
    pub receipt_id: EntityId,
    pub receipt_digest: String,
    pub expected_design_head: i64,
    pub announcements: Vec<TeletextSavedObjectSubpageAnnouncement>,
}
impl fmt::Debug for NativeTeletextObjectSubpagesAnnounceRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NativeTeletextObjectSubpagesAnnounceRequest")
            .field("receipt_id", &self.receipt_id)
            .field("expected_design_head", &self.expected_design_head)
            .field("announcements", &self.announcements.len())
            .finish_non_exhaustive()
    }
}

pub(super) fn append(
    cumulative: &mut TeletextEditorialChanges,
    incoming: &TeletextEditorialChanges,
    budget: TeletextServiceBudget,
    deadline: Instant,
) -> Result<()> {
    check_deadline(deadline)?;
    let count = cumulative
        .object_subpage_announcements
        .len()
        .checked_add(incoming.object_subpage_announcements.len())
        .ok_or(NativeTeletextEditorialImportError::Budget)?;
    if count > budget.max_records.min(budget.max_structure_items) {
        return Err(NativeTeletextEditorialImportError::Budget);
    }
    let mut seen = std::collections::BTreeSet::new();
    for value in cumulative
        .object_subpage_announcements
        .iter()
        .chain(&incoming.object_subpage_announcements)
    {
        check_deadline(deadline)?;
        let announcement = value.announcement();
        if !seen.insert((announcement.display(), announcement.resource())) {
            return Err(NativeTeletextEditorialImportError::Association);
        }
    }
    for saved in &incoming.object_subpage_announcements {
        check_deadline(deadline)?;
        if !super::page_selections::fold_announcement(cumulative, *saved)? {
            cumulative.object_subpage_announcements.push(*saved);
        }
    }
    repeat::check_operand_budget(cumulative, budget)?;
    check_deadline(deadline)
}

/// Save an explicitly announced retained resource in an unpublished concept.
/// Original sources, MOT/default provenance, other roles and current output stay
/// protected; native/effect validation precedes the existing exact-head CAS.
/// # Errors
/// Rejects stale receipts, absent/mismatched resources, duplicate/invalid intent,
/// wrong source ownership, exceeded bounds and deadlines without publishing.
pub async fn announce_teletext_editorial_object_subpages(
    application: &ApplicationServices,
    request: NativeTeletextObjectSubpagesAnnounceRequest,
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
            action: Action::Announcements(request.announcements),
        },
        policy,
        deadline,
    )
    .await
}
