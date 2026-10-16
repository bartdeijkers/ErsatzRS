//! Reuse a truly free logical subpage only behind an explicit compatible link.

use super::{unused_scope, Target};
use ersatzrs_domain::{
    TeletextDeclaredService, TeletextDrcsTableScope, TeletextPageAddress, TeletextResourceIdentity,
    TeletextResourceKind, TeletextResourceSubpageIndex, TeletextX27LinkRole, TeletextX27LinkTarget,
    TeletextX27LinkValidity,
};

pub(super) fn target(
    declared: &TeletextDeclaredService,
    owner: TeletextPageAddress,
) -> Option<Target> {
    let service = declared.semantic();
    if let Some(scope) = unused_scope(service, owner) {
        return Some((scope, None));
    }
    let metadata = service.page_enhancements().get(&owner)?;
    let links = metadata.compositional_links?;
    for (scope, role, kind) in [
        (
            TeletextDrcsTableScope::Normal,
            TeletextX27LinkRole::Drcs,
            TeletextResourceKind::Drcs,
        ),
        (
            TeletextDrcsTableScope::Global,
            TeletextX27LinkRole::Gdrcs,
            TeletextResourceKind::Gdrcs,
        ),
    ] {
        let Some(page) = metadata.resources.drcs.page(scope) else {
            continue;
        };
        let link = links.link(role);
        if link.target() != (TeletextX27LinkTarget::Page { page })
            || link.validity() == TeletextX27LinkValidity::Level35Only
            || declared
                .resource_inventory()
                .iter()
                .any(|resource| resource.page() == page && resource.kind() != kind)
            || service.base().pages.iter().any(|display| {
                display.address.page.magazine() == page.magazine()
                    && display.address.page.wire_page() == page.page()
            })
        {
            continue;
        }
        for subpage in 0..16 {
            let subpage = TeletextResourceSubpageIndex::new(subpage).ok()?;
            if !declared
                .resource_inventory()
                .iter()
                .any(|resource| resource.page() == page && resource.logical_subpage() == subpage)
            {
                let resource = TeletextResourceIdentity::new(kind, page, subpage).ok()?;
                return Some((scope, Some(resource)));
            }
        }
    }
    None
}
