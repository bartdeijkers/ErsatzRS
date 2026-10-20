//! Source-owned selection intent joins the existing atomic domain transaction.
use super::{
    editorial_semantic_error, ChannelTeletextDocumentEncodingError, EditorialResourceDocuments,
    TeletextChannelDocumentEncodingPolicy, TeletextProducerCompositionBudget,
    TeletextProfileDocument,
};
use ersatzrs_domain::{
    TeletextDeclaredService, TeletextResourceKind, TeletextX27LinkRole, TeletextX27LinkTarget,
};

pub(super) fn check_source(
    documents: &[TeletextProfileDocument],
    editorial: &EditorialResourceDocuments<'_>,
    policy: &TeletextChannelDocumentEncodingPolicy,
) -> Result<(), ChannelTeletextDocumentEncodingError> {
    let invalid = || ChannelTeletextDocumentEncodingError::Association;
    let maximum = policy
        .composition
        .service
        .max_records
        .min(policy.composition.service.max_structure_items);
    if editorial
        .object_page_selections
        .len()
        .checked_add(editorial.object_subpage_announcements.len())
        .is_none_or(|count| count > maximum)
    {
        return Err(invalid());
    }
    let mut owners = std::collections::BTreeSet::new();
    for saved in editorial.object_page_selections {
        let change = saved.change();
        let Some(TeletextProfileDocument::Authored(source)) = documents.get(saved.document_index())
        else {
            return Err(invalid());
        };
        if !owners.insert((change.display(), change.role().wire_code()))
            || !source
                .semantic()
                .base()
                .pages
                .iter()
                .any(|page| page.address == change.display())
        {
            return Err(invalid());
        }
        let kind = match change.role() {
            TeletextX27LinkRole::Pop => TeletextResourceKind::Pop,
            TeletextX27LinkRole::Gpop => TeletextResourceKind::Gpop,
            _ => return Err(invalid()),
        };
        let TeletextX27LinkTarget::Page { page } = change.replacement().target() else {
            return Err(invalid());
        };
        let resources = || {
            source
                .resource_inventory()
                .iter()
                .copied()
                .chain(
                    editorial
                        .object_resource_additions
                        .iter()
                        .filter(|bank| bank.document_index() == saved.document_index())
                        .map(|bank| bank.addition().resource()),
                )
                .filter(|resource| resource.kind() == kind && resource.page() == page)
        };
        if resources().next().is_none()
            || (0..16_u8).any(|s1| {
                change.replacement().required_subpages().get() & (1_u16 << s1) != 0
                    && !resources().any(|resource| resource.logical_subpage().get() == s1)
            })
        {
            return Err(invalid());
        }
    }
    Ok(())
}

pub(super) fn apply(
    declared: TeletextDeclaredService,
    editorial: &EditorialResourceDocuments<'_>,
    composition: TeletextProducerCompositionBudget,
) -> Result<TeletextDeclaredService, ChannelTeletextDocumentEncodingError> {
    let changes = editorial
        .object_page_selections
        .iter()
        .map(
            ersatzrs_app::teletext_design_documents::TeletextSavedObjectPageSelectionChange::change,
        )
        .collect::<Vec<_>>();
    declared
        .with_object_page_selections_and_program_overlays(
            &changes,
            editorial.resource_programs,
            composition.service,
            composition.declarations,
        )
        .map_err(|error| match error {
            ersatzrs_domain::TeletextObjectPageSelectionError::Service(reason) => {
                editorial_semantic_error(reason)
            }
            error => ChannelTeletextDocumentEncodingError::ObjectPageSelection(Box::new(error)),
        })
}
