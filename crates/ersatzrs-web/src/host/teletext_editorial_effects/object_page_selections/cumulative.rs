//! Root-relative selected roles remain exact on ordinary repeated imports.
use super::{
    check_deadline, failure, materialized, replace, resolved_role, resource_exists, BTreeMap,
    BTreeSet, Instant, Result, TeletextDeclaredService, TeletextEditorialEdits,
    TeletextX27LinkDeclaration, TeletextX27LinkRole, TeletextX27LinkTarget,
};

pub(super) fn check(
    after: &TeletextDeclaredService,
    edits: TeletextEditorialEdits<'_>,
    deadline: Instant,
) -> Result<()> {
    let mut seen = BTreeSet::new();
    let mut roots = BTreeMap::new();
    let mut expected = BTreeMap::new();
    for saved in edits.object_page_selections {
        check_deadline(deadline)?;
        let change = saved.change();
        let owner = change.display();
        if !seen.insert((owner, change.role().wire_code()))
            || roots
                .insert(owner, change.expected_links())
                .is_some_and(|previous| previous != change.expected_links())
            || resolved_role(after, owner, change.role(), change.expected_links())?
                != change.expected_role()
        {
            return Err(failure(owner));
        }
        resource_exists(after, owner, change.role(), change.replacement())?;
        let initial = materialized(after, owner, change.expected_links())?;
        let links = expected.get(&owner).copied().unwrap_or(initial);
        expected.insert(owner, replace(links, change.role(), change.replacement())?);
    }
    // Historical same-role A announcements are checked against the retained
    // exact A declaration. Current selected-page announcements may instead be
    // folded into B's complete replacement mask by the host.
    for saved in edits.object_subpage_announcements {
        check_deadline(deadline)?;
        let intent = saved.announcement();
        let owner = intent.display();
        let role = super::super::subpage_announcements::role(intent.resource())
            .ok_or_else(|| failure(owner))?;
        let Some(links) = expected.get_mut(&owner) else {
            continue;
        };
        let link = links.link(role);
        if link.target()
            == (TeletextX27LinkTarget::Page {
                page: intent.resource().page(),
            })
        {
            let bit = 1_u16 << intent.resource().logical_subpage().get();
            let changed = TeletextX27LinkDeclaration::new(
                link.target(),
                link.validity(),
                ersatzrs_domain::TeletextSubpageRequirementMask::new(
                    link.required_subpages().get() | bit,
                ),
            )
            .map_err(|_| failure(owner))?;
            *links = replace(*links, role, changed)?;
        }
    }
    for (owner, links) in expected {
        let page = after
            .semantic()
            .page_enhancements()
            .get(&owner)
            .ok_or_else(|| failure(owner))?;
        if page.compositional_links != Some(links) {
            return Err(failure(owner));
        }
        for saved in edits
            .object_page_selections
            .iter()
            .filter(|saved| saved.change().display() == owner)
        {
            let change = saved.change();
            let TeletextX27LinkTarget::Page { page: target } = change.replacement().target() else {
                return Err(failure(owner));
            };
            let actual = match change.role() {
                TeletextX27LinkRole::Pop => page.resources.objects.public,
                TeletextX27LinkRole::Gpop => page.resources.objects.global,
                TeletextX27LinkRole::Drcs => page
                    .resources
                    .drcs
                    .page(ersatzrs_domain::TeletextDrcsTableScope::Normal),
                TeletextX27LinkRole::Gdrcs => page
                    .resources
                    .drcs
                    .page(ersatzrs_domain::TeletextDrcsTableScope::Global),
            };
            if actual != Some(target) {
                return Err(failure(owner));
            }
        }
    }
    Ok(())
}
