//! Checked sharing of an original linked bank; ownership remains outside domain.

use super::{
    role, Result, TeletextDrcsExtensionError, TeletextDrcsPageExtension, TeletextDrcsTableScope,
    TeletextResourceIdentity, TeletextResourceKind, TeletextSemanticService, TeletextX27LinkTarget,
};

pub(super) fn existing_page(
    service: &TeletextSemanticService,
    resource: TeletextResourceIdentity,
) -> bool {
    service
        .drcs
        .subtables()
        .iter()
        .any(|table| table.identity().page() == resource.page())
}

pub(super) fn validate_resource(
    service: &TeletextSemanticService,
    resource: TeletextResourceIdentity,
    pages: &[TeletextDrcsPageExtension],
) -> Result<()> {
    if service.drcs.subtables().iter().any(|table| {
        table.identity().page() == resource.page()
            && table.identity().logical_subpage() == resource.logical_subpage()
    }) {
        return Err(TeletextDrcsExtensionError::DuplicateResource { resource });
    }
    if !service.drcs.subtables().iter().any(|table| {
        table.identity().page() == resource.page() && table.identity().kind() == resource.kind()
    }) {
        return Err(TeletextDrcsExtensionError::OccupiedResourcePage { resource });
    }
    let mut linked = false;
    for extension in pages.iter().filter(|extension| {
        extension.resource.page() == resource.page() && extension.resource.kind() == resource.kind()
    }) {
        validate_page(service, extension)?;
        linked = true;
    }
    if !linked {
        return Err(TeletextDrcsExtensionError::OccupiedResourcePage { resource });
    }
    Ok(())
}

pub(super) fn validate_page(
    service: &TeletextSemanticService,
    extension: &TeletextDrcsPageExtension,
) -> Result<()> {
    let display = extension.display;
    let page = service
        .pages
        .get(&display)
        .ok_or(TeletextDrcsExtensionError::MissingPage { display })?;
    let kind = match extension.table {
        TeletextDrcsTableScope::Normal => TeletextResourceKind::Drcs,
        TeletextDrcsTableScope::Global => TeletextResourceKind::Gdrcs,
    };
    if extension.resource.kind() != kind {
        return Err(TeletextDrcsExtensionError::Kind {
            display,
            table: extension.table,
            resource: extension.resource,
        });
    }
    let rejected = || TeletextDrcsExtensionError::OccupiedRole {
        display,
        table: extension.table,
    };
    let links = page.compositional_links.ok_or_else(rejected)?;
    let link = links.link(role(extension.table));
    if page.resources.drcs.page(extension.table) != Some(extension.resource.page())
        || link.target()
            != (TeletextX27LinkTarget::Page {
                page: extension.resource.page(),
            })
        || !link.validity().applies_at_level25()
    {
        return Err(rejected());
    }
    Ok(())
}
