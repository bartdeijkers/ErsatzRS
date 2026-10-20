//! Exact additive dependency deltas; no bank selection or edit authority.
use super::{
    check_deadline, error, BTreeMap, BTreeSet, Instant, Result, TeletextEditorialEdits,
    TeletextEditorialEffectsError, TeletextEditorialEffectsFailure, TeletextPageAddress,
};
use ersatzrs_domain::{
    TeletextDeclaredService, TeletextMotLevel25Resolution, TeletextMotLinkProvenance,
    TeletextMotResolutionRequirements, TeletextMotRoleResolution, TeletextResourceIdentity,
    TeletextResourceKind, TeletextSubpageRequirementMask, TeletextX27Level25Links,
    TeletextX27LinkDeclaration, TeletextX27LinkRole, TeletextX27LinkTarget,
};

pub(super) fn failure(owner: TeletextPageAddress) -> TeletextEditorialEffectsError {
    error(
        Some(owner),
        None,
        TeletextEditorialEffectsFailure::ProtectedStructure,
    )
}
pub(super) fn role(resource: TeletextResourceIdentity) -> Option<TeletextX27LinkRole> {
    match resource.kind() {
        TeletextResourceKind::Pop => Some(TeletextX27LinkRole::Pop),
        TeletextResourceKind::Gpop => Some(TeletextX27LinkRole::Gpop),
        _ => None,
    }
}

/// Only small copied link/resolution records are derived. Complete services,
/// catalogues and command bodies are never cloned to authenticate this delta.
pub(super) fn check_resolutions(
    before: &TeletextDeclaredService,
    after: &TeletextDeclaredService,
    incoming: TeletextEditorialEdits<'_>,
    cumulative: TeletextEditorialEdits<'_>,
    deadline: Instant,
) -> Result<BTreeMap<TeletextPageAddress, Option<TeletextX27Level25Links>>> {
    check_cumulative(after, cumulative, deadline)?;
    let expected = expected_links(before, after, incoming, cumulative, deadline)?;
    let expected = super::object_page_selections::apply(
        before, after, incoming, cumulative, expected, deadline,
    )?;
    compare_resolutions(before, after, &expected, deadline)?;
    Ok(expected)
}

fn check_cumulative(
    after: &TeletextDeclaredService,
    cumulative: TeletextEditorialEdits<'_>,
    deadline: Instant,
) -> Result<()> {
    let mut known = BTreeSet::new();
    for saved in cumulative.object_subpage_announcements {
        check_deadline(deadline)?;
        let intent = saved.announcement();
        let owner = intent.display();
        let resource = intent.resource();
        let role = role(resource).ok_or_else(|| failure(owner))?;
        if !known.insert((owner, resource)) || !after.resource_inventory().contains(&resource) {
            return Err(failure(owner));
        }
        let final_link = after
            .semantic()
            .page_enhancements()
            .get(&owner)
            .and_then(|page| page.compositional_links)
            .map(|links| links.link(role))
            .ok_or_else(|| failure(owner))?;
        let link = if final_link.target()
            == (TeletextX27LinkTarget::Page {
                page: resource.page(),
            }) {
            final_link
        } else {
            cumulative
                .object_page_selections
                .iter()
                .find_map(|saved| {
                    let change = saved.change();
                    (change.display() == owner && change.role() == role)
                        .then(|| change.expected_links().map(|links| links.link(role)))
                        .flatten()
                })
                .ok_or_else(|| failure(owner))?
        };
        if !link.validity().applies_at_level25()
            || link.target()
                != (TeletextX27LinkTarget::Page {
                    page: resource.page(),
                })
            || link.required_subpages().get() & (1_u16 << resource.logical_subpage().get()) == 0
        {
            return Err(failure(owner));
        }
    }
    Ok(())
}

fn expected_links(
    before: &TeletextDeclaredService,
    after: &TeletextDeclaredService,
    incoming: TeletextEditorialEdits<'_>,
    cumulative: TeletextEditorialEdits<'_>,
    deadline: Instant,
) -> Result<BTreeMap<TeletextPageAddress, TeletextX27Level25Links>> {
    let mut expected = BTreeMap::new();
    let mut seen = BTreeSet::new();
    for saved in incoming.object_subpage_announcements {
        check_deadline(deadline)?;
        let intent = saved.announcement();
        let owner = intent.display();
        let resource = intent.resource();
        let role = role(resource).ok_or_else(|| failure(owner))?;
        if !seen.insert((owner, resource))
            || !(cumulative.object_subpage_announcements.contains(saved)
                || cumulative.object_page_selections.iter().any(|selection| {
                    let change = selection.change();
                    let link = change.replacement();
                    selection.document_index() == saved.document_index()
                        && change.display() == owner && change.role() == role
                        && link.target() == (TeletextX27LinkTarget::Page { page: resource.page() })
                        && link.required_subpages().get() & (1_u16 << resource.logical_subpage().get()) != 0
                }))
            || !after.resource_inventory().contains(&resource)
            // New bank inventories are authenticated separately against the
            // exact incoming complete bank records, never an unknown resource.
            || (!before.resource_inventory().contains(&resource)
                && !incoming.object_resource_additions.iter()
                    .any(|bank| bank.addition().resource() == resource))
        {
            return Err(failure(owner));
        }
        let resolution = before
            .resolutions()
            .iter()
            .find(|value| value.display() == owner)
            .ok_or_else(|| failure(owner))?;
        if !matches!(resolution.role(role), TeletextMotRoleResolution::Selected {target,..} if *target == resource.page())
        {
            return Err(failure(owner));
        }
        let original = before
            .semantic()
            .page_enhancements()
            .get(&owner)
            .ok_or_else(|| failure(owner))?
            .compositional_links;
        let links = match expected.get(&owner).copied().or(original) {
            Some(links) => links,
            None if resolution.mot().is_some() => resolution
                .materialized_level25_links()
                .map_err(|_| failure(owner))?,
            None => return Err(failure(owner)),
        };
        let link = links.link(role);
        let bit = 1_u16 << resource.logical_subpage().get();
        if !link.validity().applies_at_level25()
            || link.target()
                != (TeletextX27LinkTarget::Page {
                    page: resource.page(),
                })
            || link.required_subpages().get() & bit != 0
        {
            return Err(failure(owner));
        }
        let changed = TeletextX27LinkDeclaration::new(
            link.target(),
            link.validity(),
            TeletextSubpageRequirementMask::new(link.required_subpages().get() | bit),
        )
        .map_err(|_| failure(owner))?;
        let mut roles = TeletextX27LinkRole::ALL.map(|role| links.link(role));
        roles[usize::from(role.wire_code())] = changed;
        expected.insert(
            owner,
            TeletextX27Level25Links::new(roles[0], roles[1], roles[2], roles[3])
                .map_err(|_| failure(owner))?,
        );
    }
    Ok(expected)
}

fn compare_resolutions(
    before: &TeletextDeclaredService,
    after: &TeletextDeclaredService,
    expected: &BTreeMap<TeletextPageAddress, Option<TeletextX27Level25Links>>,
    deadline: Instant,
) -> Result<()> {
    if before.resolutions().len() != after.resolutions().len() {
        return Err(error(
            None,
            None,
            TeletextEditorialEffectsFailure::ProtectedStructure,
        ));
    }
    for (old, new) in before.resolutions().iter().zip(after.resolutions()) {
        check_deadline(deadline)?;
        let owner = old.display();
        let Some(links) = expected.get(&owner).copied() else {
            if old != new {
                return Err(failure(owner));
            }
            continue;
        };
        if owner != new.display() || old.defaults() != new.defaults() {
            return Err(failure(owner));
        }
        if let Some(mot) = old.mot() {
            let declaration = before
                .mots()
                .iter()
                .find(|value| value.identity() == mot.selected_mot())
                .ok_or_else(|| failure(owner))?;
            let resolved = TeletextMotLevel25Resolution::resolve(declaration, owner, links)
                .map_err(|_| failure(owner))?;
            if new.mot() != Some(&resolved)
                || TeletextX27LinkRole::ALL
                    .into_iter()
                    .any(|role| new.role(role) != resolved.role(role))
            {
                return Err(failure(owner));
            }
        } else {
            if new.mot().is_some() {
                return Err(failure(owner));
            }
            for role in TeletextX27LinkRole::ALL {
                let selected =
                    links
                        .map(|links| links.link(role))
                        .and_then(|link| match link.target() {
                            TeletextX27LinkTarget::Page { page }
                                if link.validity().applies_at_level25() =>
                            {
                                Some((page, link.required_subpages()))
                            }
                            _ => None,
                        });
                let expected_role = match selected {
                    Some((page, mask)) => TeletextMotRoleResolution::Selected {
                        target: page,
                        requirements: TeletextMotResolutionRequirements::X27Mask(mask),
                        provenance: TeletextMotLinkProvenance::X27 {
                            display: owner,
                            role,
                        },
                        original_mot: None,
                    },
                    None => TeletextMotRoleResolution::Unselected,
                };
                if new.role(role) != &expected_role {
                    return Err(failure(owner));
                }
            }
        }
    }
    Ok(())
}
