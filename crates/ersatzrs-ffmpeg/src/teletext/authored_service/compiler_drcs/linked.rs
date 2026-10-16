//! Reuse a free logical subpage behind an explicit link or an authenticated MOT.

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
    let links = metadata.compositional_links;
    // Without X27, only this declaration's exact checked MOT resolution may
    // authorize the already selected roles. The later declaration-aware
    // derivation materializes every inherited role before adding the new S1.
    if links.is_none()
        && !declared
            .resolutions()
            .iter()
            .any(|resolved| resolved.display() == owner && resolved.mot().is_some())
    {
        return None;
    }
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
        if links.is_some_and(|links| {
            let link = links.link(role);
            link.target() != (TeletextX27LinkTarget::Page { page })
                || link.validity() == TeletextX27LinkValidity::Level35Only
        }) || declared
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
