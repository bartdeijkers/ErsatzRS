//! Only authenticated declarations may materialize absent X27 MOT selections.

use super::{
    bounded, Result, TeletextDrcsExtensionBudget, TeletextDrcsExtensionError,
    TeletextDrcsPageExtension, TeletextDrcsSubtable, TeletextResolvedObjectPageLinks,
    TeletextResolvedPageEnhancementDraft, TeletextResolvedPageResourceSelection,
    TeletextSemanticService, TeletextSubpageRequirementMask, TeletextX27Level25Links,
    TeletextX27LinkDeclaration, TeletextX27LinkRole, TeletextX27LinkTarget,
    TeletextX27LinkValidity, TeletextX27Magazine,
};
use crate::teletext::{
    TeletextDeclaredDisplayResolution, TeletextDeclaredService, TeletextDrcsPageLinks,
    TeletextMotResolutionRequirements, TeletextMotRoleResolution,
};

impl TeletextDeclaredService {
    /// Derive physical DRCS supplements using the authenticated MOT/X27 owners.
    /// When an affected page has no explicit X27, its checked selected MOT may
    /// supply all four original roles before the existing additive derivation.
    /// Targets, contiguous MOT extents, defaults and conditional presentation
    /// remain unchanged; the original declared service is never modified.
    /// This grants neither authored edit permission nor publication authority.
    /// # Errors
    /// Rejects complete declared-namespace collisions and count overflow before
    /// cloning, and every existing semantic DRCS-subpage derivation failure.
    pub fn derive_with_drcs_subpages(
        &self,
        resources: &[TeletextDrcsSubtable],
        pages: &[TeletextDrcsPageExtension],
        budget: TeletextDrcsExtensionBudget,
    ) -> Result<TeletextSemanticService> {
        let original = self.semantic();
        original.bound_drcs_extension(resources, pages, budget)?;
        let count = original
            .base()
            .pages
            .len()
            .checked_add(self.resource_inventory().len())
            .and_then(|count| count.checked_add(resources.len()))
            .ok_or(TeletextDrcsExtensionError::Size)?;
        bounded("records", count, budget.max_records)?;
        for (index, table) in resources.iter().enumerate() {
            let resource = table.identity();
            if resources[..index].iter().any(|table| {
                table.identity().page() == resource.page()
                    && table.identity().logical_subpage() == resource.logical_subpage()
            }) || self.resource_inventory().iter().any(|identity| {
                identity.page() == resource.page()
                    && identity.kind() == resource.kind()
                    && identity.logical_subpage() == resource.logical_subpage()
            }) {
                return Err(TeletextDrcsExtensionError::DuplicateResource { resource });
            }
            if original.base().pages.iter().any(|page| {
                page.address.page.magazine() == resource.page().magazine()
                    && page.address.page.wire_page() == resource.page().page()
            }) || self.resource_inventory().iter().any(|identity| {
                identity.page() == resource.page() && identity.kind() != resource.kind()
            }) {
                return Err(TeletextDrcsExtensionError::OccupiedResourcePage { resource });
            }
        }
        let mut links = Vec::with_capacity(pages.len());
        for extension in pages {
            let display = extension.display;
            let resolution = self
                .resolutions()
                .iter()
                .find(|resolution| resolution.display() == display)
                .ok_or(TeletextDrcsExtensionError::MissingPage { display })?;
            if original
                .page_enhancements()
                .get(&display)
                .is_some_and(|page| page.compositional_links.is_some())
                || resolution.mot().is_none()
            {
                continue;
            }
            links.push((resolution, materialize(resolution)?));
        }
        if links.is_empty() {
            return original.derive_with_drcs_subpages(resources, pages, budget);
        }
        let mut effective = original.clone();
        for (resolution, links) in links {
            let display = resolution.display();
            if !effective.pages.contains_key(&display) {
                effective.original_structure_items = effective
                    .original_structure_items
                    .checked_add(1)
                    .ok_or(TeletextDrcsExtensionError::Size)?;
            }
            let page = effective.pages.entry(display).or_insert_with(|| {
                TeletextResolvedPageEnhancementDraft {
                    address: display,
                    presentation: None,
                    compositional_links: None,
                    local_program: None,
                    resources: TeletextResolvedPageResourceSelection {
                        objects: TeletextResolvedObjectPageLinks {
                            public: None,
                            global: None,
                        },
                        drcs: TeletextDrcsPageLinks::unlinked(),
                        defaults: *resolution.defaults(),
                    },
                }
            });
            page.compositional_links = Some(links);
        }
        effective.derive_with_drcs_subpages(resources, pages, budget)
    }
}

fn materialize(resolution: &TeletextDeclaredDisplayResolution) -> Result<TeletextX27Level25Links> {
    let display = resolution.display();
    let unused = TeletextX27LinkDeclaration::new(
        TeletextX27LinkTarget::Unspecified {
            magazine: TeletextX27Magazine::new(display.page.magazine())?,
        },
        TeletextX27LinkValidity::Level25Only,
        TeletextSubpageRequirementMask::new(u16::MAX),
    )?;
    let mut links = [unused; 4];
    for role in TeletextX27LinkRole::ALL {
        links[usize::from(role.wire_code())] = match *resolution.role(role) {
            TeletextMotRoleResolution::Unselected => unused,
            TeletextMotRoleResolution::Selected {
                target,
                requirements: TeletextMotResolutionRequirements::MotExtent(extent),
                ..
            } => TeletextX27LinkDeclaration::new(
                TeletextX27LinkTarget::Page { page: target },
                TeletextX27LinkValidity::Level25Only,
                extent.required_mask(),
            )?,
            _ => return Err(TeletextDrcsExtensionError::UnprovenOtherRoles { display }),
        };
    }
    Ok(TeletextX27Level25Links::new(
        links[0], links[1], links[2], links[3],
    )?)
}

#[cfg(test)]
#[path = "declared_tests.rs"]
mod tests;
