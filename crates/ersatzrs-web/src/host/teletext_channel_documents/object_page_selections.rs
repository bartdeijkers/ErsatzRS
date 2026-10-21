//! Source-owned selection intent joins the existing atomic domain transaction.
use super::{
    editorial_semantic_error, ChannelTeletextDocumentEncodingError, EditorialResourceDocuments,
    TeletextChannelDocumentEncodingPolicy, TeletextProducerCompositionBudget,
    TeletextProfileDocument,
};
use ersatzrs_domain::{TeletextDeclaredService, TeletextX27LinkTarget};

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
        let kind = change.role().kind();
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
    check_main_source(documents, editorial, maximum)?;
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
    let main = remap_main_invocations(editorial)?;
    declared
        .with_object_page_selections_and_consumers(
            &changes,
            ersatzrs_domain::TeletextObjectPageSelectionConsumers {
                main: &main,
                local: editorial.programs.4,
                resources: editorial.resource_programs,
            },
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

/// Stored Main intent is root-relative; structural Main keeps original call anchors.
fn remap_main_invocations(
    editorial: &EditorialResourceDocuments<'_>,
) -> Result<
    Vec<ersatzrs_domain::TeletextMainResourceInvocationOverlay>,
    ChannelTeletextDocumentEncodingError,
> {
    let invalid = || ChannelTeletextDocumentEncodingError::Association;
    editorial
        .main_resource_invocations
        .iter()
        .map(|intent| {
            let structural = editorial
                .main_programs
                .iter()
                .find(|program| program.owner() == intent.owner());
            let command = structural
                .map_or(Some(intent.command()), |program| {
                    program.replacement_command(intent.command())
                })
                .ok_or_else(invalid)?;
            ersatzrs_domain::TeletextMainResourceInvocationOverlay::new(
                intent.owner(),
                command,
                intent.expected(),
                intent.replacement(),
            )
            .map_err(|_| invalid())
        })
        .collect()
}

fn check_main_source(
    documents: &[TeletextProfileDocument],
    editorial: &EditorialResourceDocuments<'_>,
    maximum: usize,
) -> Result<(), ChannelTeletextDocumentEncodingError> {
    let invalid = || ChannelTeletextDocumentEncodingError::Association;
    if editorial.main_resource_invocations.len() > maximum {
        return Err(invalid());
    }
    let mut seen = std::collections::BTreeSet::new();
    for intent in editorial.main_resource_invocations {
        if !seen.insert((intent.owner(), intent.command())) {
            return Err(invalid());
        }
        let mut owners = documents
            .iter()
            .enumerate()
            .filter_map(|(index, document)| {
                let TeletextProfileDocument::Authored(source) = document else {
                    return None;
                };
                source
                    .semantic()
                    .base()
                    .pages
                    .iter()
                    .any(|page| page.address == intent.owner())
                    .then_some((index, source))
            });
        let (Some((index, source)), None) = (owners.next(), owners.next()) else {
            return Err(invalid());
        };
        let original = source
            .semantic()
            .page_enhancements()
            .get(&intent.owner())
            .and_then(|page| page.local_program.as_ref())
            .and_then(|program| program.commands().get(intent.command()));
        let ersatzrs_domain::TeletextEnhancementCommand::InvokeObject { target, .. } =
            intent.replacement()
        else {
            return Err(invalid());
        };
        let resource = target.resource().ok_or_else(invalid)?;
        if original != Some(&intent.expected())
            || (!source.resource_inventory().contains(&resource)
                && !editorial.object_resource_additions.iter().any(|bank| {
                    bank.document_index() == index && bank.addition().resource() == resource
                }))
        {
            return Err(invalid());
        }
    }
    Ok(())
}
