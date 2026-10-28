//! Author-bound context changes preserve the original producer and full effects.
use super::{
    check_deadline, Instant, NativeTeletextEditorialImportError, Result, TeletextEditorialEdits,
};
use crate::host::{
    teletext_editorial_effects::{
        validate_teletext_editorial_transition_with_contexts,
        validate_teletext_editorial_transition_with_operands, SecondaryDefaultContextChange,
    },
    teletext_stored_designs::OriginalDesignView,
};
use ersatzrs_domain::{
    TeletextLevel25PresentationDefaults, TeletextProducerPresentation,
    TeletextProducerPresentationSource,
};
use ersatzrs_ffmpeg::teletext::TeletextAuthoredServiceEncodingBudget;

/// A historical TTI receipt can still replace ordinary body edits, as before,
/// but cannot silently replace a newer native-only context decision. The final
/// existing revision CAS also fences a concurrent decision after this read.
pub(super) async fn check_historical_choice(
    application: &ersatzrs_app::ApplicationServices,
    baseline: &super::EditorialBaseline,
    head: &ersatzrs_domain::TeletextDesignHead,
    policy: &super::NativeTeletextExportPolicy,
    admission: (Instant, ersatzrs_core::locking::TeletextWorkHold),
) -> Result<()> {
    let (deadline, hold) = admission;
    if baseline.original.revision == head.revision {
        return Ok(());
    }
    check_deadline(deadline)?;
    let current = application
        .teletext_design_revision(head.id, head.revision, policy.document.max_serialized_bytes)
        .await?
        .ok_or(NativeTeletextEditorialImportError::Association)?;
    let original = baseline.original.clone();
    let budget = policy.document;
    crate::host::teletext_work::spawn_blocking(hold, move || {
        check_deadline(deadline)?;
        let current = super::StoredDesignParts::decode(&current, budget)?;
        let original = super::StoredDesignParts::decode(&original, budget)?;
        if current.secondary_default_agreements() != original.secondary_default_agreements() {
            return Err(NativeTeletextEditorialImportError::Association);
        }
        check_deadline(deadline)
    })
    .await
    .map_err(|_| NativeTeletextEditorialImportError::WorkerFailed)?
}

fn expected(
    producer: &TeletextProducerPresentation,
    page: &ersatzrs_domain::TeletextProducerPagePresentation,
) -> Option<TeletextLevel25PresentationDefaults> {
    producer.secondary_default_agreement().map(|agreement| {
        let mut defaults = *page.effective_defaults();
        defaults.secondary = agreement.secondary();
        defaults
    })
}

fn contexts(
    before: OriginalDesignView<'_>,
    after: OriginalDesignView<'_>,
    budget: &TeletextAuthoredServiceEncodingBudget,
    deadline: Instant,
) -> Result<Vec<SecondaryDefaultContextChange>> {
    let fail = || NativeTeletextEditorialImportError::Association;
    if before.channel_id() != after.channel_id()
        || before.producers().len() != after.producers().len()
    {
        return Err(fail());
    }
    let old_pages = before
        .encoded()
        .ok_or_else(fail)?
        .declared()
        .semantic()
        .page_enhancements();
    let new_pages = after
        .encoded()
        .ok_or_else(fail)?
        .declared()
        .semantic()
        .page_enhancements();
    let mut changes = Vec::new();
    for (old, new) in before.producers().iter().zip(after.producers()) {
        check_deadline(deadline)?;
        if old.original() != new.original() || old.pages() != new.pages() {
            return Err(fail());
        }
        if old.secondary_default_agreement() == new.secondary_default_agreement() {
            continue;
        }
        for page in old.pages() {
            check_deadline(deadline)?;
            // Authored Page/Magazine values never derive permission from opt-in.
            if !matches!(
                page.source(),
                TeletextProducerPresentationSource::Header { .. }
                    | TeletextProducerPresentationSource::MotFallback { .. }
            ) {
                continue;
            }
            let before = old_pages
                .get(&page.address())
                .ok_or_else(fail)?
                .presentation;
            let after = new_pages
                .get(&page.address())
                .ok_or_else(fail)?
                .presentation;
            if before != expected(old, page) || after != expected(new, page) {
                return Err(fail());
            }
            if before != after {
                if changes.len() >= budget.max_pages {
                    return Err(NativeTeletextEditorialImportError::Budget);
                }
                changes.push(SecondaryDefaultContextChange {
                    owner: page.address(),
                    before,
                    after,
                });
            }
        }
    }
    Ok(changes)
}

/// Called only after the original/root receipt and complete model were qualified.
/// Exact full producer-local defaults are checked before allowing X28/0 changes;
/// all authored bytes, shared resources and protected raster/intent checks remain.
pub(super) fn validate_transition(
    before: OriginalDesignView<'_>,
    after: OriginalDesignView<'_>,
    project: &ersatzrs_app::teletext_export_project::TeletextExportProject,
    edits: &(TeletextEditorialEdits<'_>, TeletextEditorialEdits<'_>),
    budget: &TeletextAuthoredServiceEncodingBudget,
    deadline: Instant,
) -> Result<()> {
    let contexts = contexts(before, after, budget, deadline)?;
    if contexts.is_empty() {
        return validate_teletext_editorial_transition_with_operands(
            before
                .encoded()
                .ok_or(NativeTeletextEditorialImportError::Association)?,
            after
                .encoded()
                .ok_or(NativeTeletextEditorialImportError::Association)?,
            project,
            edits.0,
            edits.1,
            budget,
            deadline,
        )
        .map_err(|error| NativeTeletextEditorialImportError::Effects(Box::new(error)));
    }
    validate_teletext_editorial_transition_with_contexts(
        before
            .encoded()
            .ok_or(NativeTeletextEditorialImportError::Association)?,
        after
            .encoded()
            .ok_or(NativeTeletextEditorialImportError::Association)?,
        project,
        &(edits.0, edits.1, &contexts),
        budget,
        deadline,
    )
    .map_err(|error| NativeTeletextEditorialImportError::Effects(Box::new(error)))
}
