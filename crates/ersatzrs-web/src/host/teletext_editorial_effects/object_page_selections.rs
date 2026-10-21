//! Exact selected-page deltas; a selector never authorizes a resource-body edit.
use super::subpage_announcements::failure;
use super::{check_deadline, Result, TeletextEditorialEdits};
use ersatzrs_domain::{
    TeletextDeclaredService, TeletextDrcsPageLinks, TeletextDrcsTableScope,
    TeletextMotLevel25Resolution, TeletextMotLinkProvenance, TeletextMotResolutionRequirements,
    TeletextMotRoleResolution, TeletextPageAddress, TeletextResolvedPageEnhancementDraft,
    TeletextX27Level25Links, TeletextX27LinkDeclaration, TeletextX27LinkRole,
    TeletextX27LinkTarget,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    time::Instant,
};
mod cumulative;

pub(super) fn apply(
    before: &TeletextDeclaredService,
    after: &TeletextDeclaredService,
    incoming: TeletextEditorialEdits<'_>,
    cumulative: TeletextEditorialEdits<'_>,
    mut links: BTreeMap<TeletextPageAddress, TeletextX27Level25Links>,
    deadline: Instant,
) -> Result<BTreeMap<TeletextPageAddress, Option<TeletextX27Level25Links>>> {
    cumulative::check(after, cumulative, deadline)?;
    // Every selector compares against the same pre-selection declarations, even
    // when two roles of one owner are changed in this atomic batch.
    let mut original = BTreeMap::new();
    let mut seen = BTreeSet::new();
    for saved in incoming.object_page_selections {
        check_deadline(deadline)?;
        let change = saved.change();
        let owner = change.display();
        let base = before
            .semantic()
            .page_enhancements()
            .get(&owner)
            .ok_or_else(|| failure(owner))?;
        let expected_links = links.get(&owner).copied().or(base.compositional_links);
        original.entry(owner).or_insert(expected_links);
    }
    for saved in incoming.object_page_selections {
        check_deadline(deadline)?;
        let change = saved.change();
        let owner = change.display();
        let role = change.role();
        let expected_links = original[&owner];
        if !seen.insert((owner, role.wire_code()))
            || change.expected_links() != expected_links
            || change.expected_role() != resolved_role(before, owner, role, expected_links)?
        {
            return Err(failure(owner));
        }
        resource_exists(after, owner, role, change.replacement())?;
        let initial = materialized(before, owner, expected_links)?;
        let current = links.get(&owner).copied().unwrap_or(initial);
        if current.link(role).validity() != change.replacement().validity() {
            return Err(failure(owner));
        }
        links.insert(owner, replace(current, role, change.replacement())?);
        // A folded root-relative intent need not equal this incoming B->C
        // record. Final cumulative checks below bind its complete target and
        // all unaffected roles. Root undo can legitimately remove the record.
        if let Some(retained) = cumulative
            .object_page_selections
            .iter()
            .find(|value| value.change().display() == owner && value.change().role() == role)
        {
            if retained.document_index() != saved.document_index()
                || retained.change().replacement() != change.replacement()
            {
                return Err(failure(owner));
            }
        }
    }
    restored_links(after, incoming, cumulative, links, deadline)
}

fn restored_links(
    after: &TeletextDeclaredService,
    incoming: TeletextEditorialEdits<'_>,
    cumulative: TeletextEditorialEdits<'_>,
    links: BTreeMap<TeletextPageAddress, TeletextX27Level25Links>,
    deadline: Instant,
) -> Result<BTreeMap<TeletextPageAddress, Option<TeletextX27Level25Links>>> {
    let mut expected = BTreeMap::new();
    for (owner, links) in links {
        check_deadline(deadline)?;
        let absent = after
            .semantic()
            .page_enhancements()
            .get(&owner)
            .is_some_and(|page| page.compositional_links.is_none());
        if absent {
            // An exact cumulative root undo may remove the materialized X27,
            // restoring original MOT provenance rather than an explicit alias.
            // The independent immutable-root qualification remains mandatory.
            if !incoming
                .object_page_selections
                .iter()
                .any(|value| value.change().display() == owner)
                || cumulative
                    .object_page_selections
                    .iter()
                    .any(|value| value.change().display() == owner)
                || cumulative
                    .object_subpage_announcements
                    .iter()
                    .any(|value| value.announcement().display() == owner)
                || links != materialized(after, owner, None)?
            {
                return Err(failure(owner));
            }
            expected.insert(owner, None);
        } else {
            expected.insert(owner, Some(links));
        }
    }
    Ok(expected)
}

pub(super) fn materialized(
    service: &TeletextDeclaredService,
    owner: TeletextPageAddress,
    links: Option<TeletextX27Level25Links>,
) -> Result<TeletextX27Level25Links> {
    if let Some(links) = links {
        return Ok(links);
    }
    // Repeated qualification sees the final X27-bearing resolution. Rebuild
    // original MOT-only roles from the unchanged declaration, never final links.
    let unused = TeletextX27LinkDeclaration::new(
        TeletextX27LinkTarget::Unspecified {
            magazine: ersatzrs_domain::TeletextX27Magazine::new(owner.page.magazine())
                .map_err(|_| failure(owner))?,
        },
        ersatzrs_domain::TeletextX27LinkValidity::Level25Only,
        ersatzrs_domain::TeletextSubpageRequirementMask::new(u16::MAX),
    )
    .map_err(|_| failure(owner))?;
    let mut roles = [unused; 4];
    for role in TeletextX27LinkRole::ALL {
        roles[usize::from(role.wire_code())] = match resolved_role(service, owner, role, None)? {
            TeletextMotRoleResolution::Unselected => unused,
            TeletextMotRoleResolution::Selected {
                target,
                requirements: TeletextMotResolutionRequirements::MotExtent(extent),
                ..
            } => TeletextX27LinkDeclaration::new(
                TeletextX27LinkTarget::Page { page: target },
                ersatzrs_domain::TeletextX27LinkValidity::Level25Only,
                extent.required_mask(),
            )
            .map_err(|_| failure(owner))?,
            _ => return Err(failure(owner)),
        };
    }
    TeletextX27Level25Links::new(roles[0], roles[1], roles[2], roles[3]).map_err(|_| failure(owner))
}
fn replace(
    links: TeletextX27Level25Links,
    role: TeletextX27LinkRole,
    declaration: TeletextX27LinkDeclaration,
) -> Result<TeletextX27Level25Links> {
    let mut roles = TeletextX27LinkRole::ALL.map(|role| links.link(role));
    roles[usize::from(role.wire_code())] = declaration;
    TeletextX27Level25Links::new(roles[0], roles[1], roles[2], roles[3]).map_err(|_| {
        super::error(
            None,
            None,
            super::TeletextEditorialEffectsFailure::ProtectedStructure,
        )
    })
}
fn resource_exists(
    service: &TeletextDeclaredService,
    owner: TeletextPageAddress,
    role: TeletextX27LinkRole,
    link: TeletextX27LinkDeclaration,
) -> Result<()> {
    let kind = role.kind();
    if !link.validity().applies_at_level25()
        || !matches!(link.target(),
        TeletextX27LinkTarget::Page { page } if service.resource_inventory().iter()
            .any(|resource| resource.kind() == kind && resource.page() == page))
    {
        return Err(failure(owner));
    }
    Ok(())
}
fn resolved_role(
    service: &TeletextDeclaredService,
    owner: TeletextPageAddress,
    role: TeletextX27LinkRole,
    links: Option<TeletextX27Level25Links>,
) -> Result<TeletextMotRoleResolution> {
    let original = service
        .resolutions()
        .iter()
        .find(|item| item.display() == owner)
        .ok_or_else(|| failure(owner))?;
    if let Some(selected) = original.mot() {
        let mot = service
            .mots()
            .iter()
            .find(|mot| mot.identity() == selected.selected_mot())
            .ok_or_else(|| failure(owner))?;
        return TeletextMotLevel25Resolution::resolve(mot, owner, links)
            .map(|resolution| *resolution.role(role))
            .map_err(|_| failure(owner));
    }
    let Some(link) = links.map(|links| links.link(role)) else {
        return Ok(*original.role(role));
    };
    Ok(match link.target() {
        TeletextX27LinkTarget::Page { page } if link.validity().applies_at_level25() => {
            TeletextMotRoleResolution::Selected {
                target: page,
                requirements: TeletextMotResolutionRequirements::X27Mask(link.required_subpages()),
                provenance: TeletextMotLinkProvenance::X27 {
                    display: owner,
                    role,
                },
                original_mot: None,
            }
        }
        _ => TeletextMotRoleResolution::Unselected,
    })
}

/// Clone only the tiny resource-link record, never catalogues or command bodies.
/// Only explicitly selected roles change; defaults and every other role stay exact.
pub(super) fn same_resources(
    before: &TeletextResolvedPageEnhancementDraft,
    after: &TeletextResolvedPageEnhancementDraft,
    incoming: TeletextEditorialEdits<'_>,
) -> bool {
    let mut expected = before.resources;
    let mut normal = expected.drcs.page(TeletextDrcsTableScope::Normal);
    let mut global = expected.drcs.page(TeletextDrcsTableScope::Global);
    for saved in incoming.object_page_selections {
        let change = saved.change();
        if change.display() != before.address {
            continue;
        }
        let TeletextX27LinkTarget::Page { page } = change.replacement().target() else {
            return false;
        };
        match change.role() {
            TeletextX27LinkRole::Pop => expected.objects.public = Some(page),
            TeletextX27LinkRole::Gpop => expected.objects.global = Some(page),
            TeletextX27LinkRole::Drcs => normal = Some(page),
            TeletextX27LinkRole::Gdrcs => global = Some(page),
        }
    }
    let Ok(drcs) = TeletextDrcsPageLinks::new(normal, global) else {
        return false;
    };
    expected.drcs = drcs;
    after.resources == expected
}
