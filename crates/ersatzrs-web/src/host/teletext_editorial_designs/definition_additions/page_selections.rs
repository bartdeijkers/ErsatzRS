//! Explicit role changes reuse retained receipts, root-relative folding and CAS.
use super::{
    apply, check_deadline, finish_encode, fmt, repeat, Action, ApplicationServices, Baselines,
    EntityId, Instant, NativeTeletextEditorialImportError, NativeTeletextExportPolicy,
    QualifiedBaseline, Request, Result, SavedEditorialTeletextDesign, TeletextDesignMutation,
    TeletextEditorialChanges, TeletextServiceBudget,
};
use ersatzrs_app::teletext_design_documents::TeletextSavedObjectPageSelectionChange;
use ersatzrs_domain::{
    TeletextDeclaredService, TeletextObjectPageSelectionChange,
    TeletextResourceObjectProgramOverlay, TeletextX27LinkTarget,
};

/// Explicit typed authoring action; TTI files cannot request bank-role changes.
pub struct NativeTeletextObjectPagesSelectRequest {
    pub name: String,
    pub receipt_id: EntityId,
    pub receipt_digest: String,
    pub expected_design_head: i64,
    pub selections: Vec<TeletextSavedObjectPageSelectionChange>,
    pub programs: Vec<TeletextResourceObjectProgramOverlay>,
}
impl fmt::Debug for NativeTeletextObjectPagesSelectRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NativeTeletextObjectPagesSelectRequest")
            .field("receipt_id", &self.receipt_id)
            .field("expected_design_head", &self.expected_design_head)
            .field("selections", &self.selections.len())
            .field("programs", &self.programs.len())
            .finish_non_exhaustive()
    }
}
/// Save coordinated bank-role and checked resource-call changes as an unpublished concept.
/// # Errors
/// Rejects missing/stale authority, wrong original/source ownership, unsupported
/// changes, complete native/effect failures, deadlines and exact-head conflicts.
pub async fn select_teletext_editorial_object_pages(
    application: &ApplicationServices,
    request: NativeTeletextObjectPagesSelectRequest,
    policy: NativeTeletextExportPolicy,
    deadline: Instant,
) -> Result<TeletextDesignMutation<SavedEditorialTeletextDesign>> {
    apply(
        application,
        Request {
            name: request.name,
            receipt_id: request.receipt_id,
            receipt_digest: request.receipt_digest,
            expected_design_head: request.expected_design_head,
            action: Action::Selections {
                selections: request.selections,
                programs: request.programs,
            },
        },
        policy,
        deadline,
    )
    .await
}
fn fail() -> NativeTeletextEditorialImportError {
    NativeTeletextEditorialImportError::Association
}
pub(super) fn preflight(
    selections: &[TeletextSavedObjectPageSelectionChange],
    programs: &[TeletextResourceObjectProgramOverlay],
    budget: TeletextServiceBudget,
    deadline: Instant,
) -> Result<()> {
    check_deadline(deadline)?;
    if selections.is_empty() && programs.is_empty() {
        return Err(NativeTeletextEditorialImportError::NoChanges);
    }
    if selections.len() > budget.max_records.min(budget.max_structure_items)
        || programs.len() > budget.max_objects
    {
        return Err(NativeTeletextEditorialImportError::Budget);
    }
    let mut commands = budget.max_commands;
    let mut items = budget.max_structure_items - selections.len();
    for program in programs {
        check_deadline(deadline)?;
        for body in [program.expected(), program.replacement()] {
            commands = commands
                .checked_sub(body.commands().len())
                .ok_or(NativeTeletextEditorialImportError::Budget)?;
        }
        items = items
            .checked_sub(program.retained_commands().len())
            .ok_or(NativeTeletextEditorialImportError::Budget)?;
    }
    Ok(())
}
fn declared(baseline: &QualifiedBaseline) -> Result<&TeletextDeclaredService> {
    Ok(baseline
        .original
        .view()
        .encoded()
        .ok_or_else(fail)?
        .declared())
}
fn preselection(
    root: &QualifiedBaseline,
    changes: &TeletextEditorialChanges,
    policy: &NativeTeletextExportPolicy,
    deadline: Instant,
) -> Result<TeletextDeclaredService> {
    check_deadline(deadline)?;
    let mut service = declared(root)?.clone();
    let banks = changes
        .object_resource_additions
        .iter()
        .map(|bank| bank.addition().clone())
        .collect::<Vec<_>>();
    if !banks.is_empty() || !changes.resource_additions.is_empty() {
        service = service
            .with_object_resource_and_definition_additions(
                &banks,
                &changes.resource_additions,
                policy.document.validation.service,
                policy.document.validation.declarations,
            )
            .map_err(|_| fail())?;
    }
    if !changes.object_subpage_announcements.is_empty() {
        let announcements = changes
            .object_subpage_announcements
            .iter()
            .map(ersatzrs_app::teletext_design_documents::TeletextSavedObjectSubpageAnnouncement::announcement)
            .collect::<Vec<_>>();
        service = service
            .with_object_subpage_announcements(
                &announcements,
                policy.document.validation.service,
                policy.document.validation.declarations,
            )
            .map_err(|_| fail())?;
    }
    check_deadline(deadline)?;
    Ok(service)
}
fn authenticate(
    latest: &QualifiedBaseline,
    incoming: &[TeletextSavedObjectPageSelectionChange],
    deadline: Instant,
) -> Result<()> {
    let service = declared(latest)?;
    let mut owners = std::collections::BTreeSet::new();
    for saved in incoming {
        check_deadline(deadline)?;
        let change = saved.change();
        let mapping = latest
            .body
            .project
            .pages
            .iter()
            .find(|mapping| {
                mapping.page.page == change.display().page.get()
                    && mapping.page.subcode == change.display().subcode.get()
            })
            .ok_or_else(fail)?;
        let meta = service
            .semantic()
            .page_enhancements()
            .get(&change.display())
            .ok_or_else(fail)?;
        let resolution = service
            .resolutions()
            .iter()
            .find(|resolution| resolution.display() == change.display())
            .ok_or_else(fail)?;
        if !owners.insert((change.display(), change.role().wire_code()))
            || mapping.producer_ordinal != saved.document_index()
            || meta.compositional_links != change.expected_links()
            || *resolution.role(change.role()) != change.expected_role()
        {
            return Err(fail());
        }
    }
    Ok(())
}
fn fold(
    original: &TeletextDeclaredService,
    cumulative: &mut TeletextEditorialChanges,
    incoming: &[TeletextSavedObjectPageSelectionChange],
    deadline: Instant,
) -> Result<()> {
    for saved in incoming {
        check_deadline(deadline)?;
        let change = saved.change();
        let meta = original
            .semantic()
            .page_enhancements()
            .get(&change.display())
            .ok_or_else(fail)?;
        let resolution = original
            .resolutions()
            .iter()
            .find(|resolution| resolution.display() == change.display())
            .ok_or_else(fail)?;
        let links = meta.compositional_links.map_or_else(
            || {
                (if *resolution.role(change.role())
                    == ersatzrs_domain::TeletextMotRoleResolution::Unselected
                {
                    resolution.materialized_level25_attachment_links()
                } else {
                    resolution.materialized_level25_links()
                })
                .map_err(|_| fail())
            },
            Ok,
        )?;
        if change.is_unselected_restoration() {
            let prior = cumulative
                .object_page_selections
                .iter()
                .find(|prior| {
                    prior.change().display() == change.display()
                        && prior.change().role() == change.role()
                })
                .ok_or_else(fail)?;
            if change.restored_unselected_links() != Some(meta.compositional_links)
                || *resolution.role(change.role())
                    != ersatzrs_domain::TeletextMotRoleResolution::Unselected
                || change.replacement() != links.link(change.role())
                || prior.document_index() != saved.document_index()
                || prior.change().expected_links() != meta.compositional_links
                || prior.change().expected_role()
                    != ersatzrs_domain::TeletextMotRoleResolution::Unselected
            {
                return Err(fail());
            }
            cumulative.object_page_selections.retain(|prior| {
                prior.change().display() != change.display()
                    || prior.change().role() != change.role()
            });
            continue;
        }
        cumulative.object_page_selections.retain(|prior| {
            prior.change().display() != change.display() || prior.change().role() != change.role()
        });
        let expected = *resolution.role(change.role());
        if matches!((expected,change.replacement().target()),(ersatzrs_domain::TeletextMotRoleResolution::Selected {target,..},TeletextX27LinkTarget::Page {page}) if target==page)
        {
            // Exact root undo removes the cumulative role; mask-only edits retain
            // their separate explicit announcement contract, never a fake no-op.
            if links.link(change.role()) != change.replacement() {
                return Err(fail());
            }
        } else {
            let rooted = rooted_selection(
                change.display(),
                change.role(),
                meta.compositional_links,
                expected,
                change.replacement(),
            )
            .map_err(|_| fail())?;
            cumulative
                .object_page_selections
                .push(TeletextSavedObjectPageSelectionChange::new(
                    saved.document_index(),
                    rooted,
                )?);
        }
    }
    Ok(())
}
pub(super) fn encode(
    baselines: Baselines,
    selections: Vec<TeletextSavedObjectPageSelectionChange>,
    programs: Vec<TeletextResourceObjectProgramOverlay>,
    revision: i64,
    policy: &NativeTeletextExportPolicy,
    deadline: Instant,
) -> Result<(super::EncodedStoredDesign, String)> {
    encode_changes(
        baselines,
        &TeletextEditorialChanges {
            object_page_selections: selections,
            resource_programs: programs,
            ..Default::default()
        },
        revision,
        policy,
        deadline,
    )
}

pub(super) fn encode_changes(
    mut baselines: Baselines,
    incoming: &TeletextEditorialChanges,
    revision: i64,
    policy: &NativeTeletextExportPolicy,
    deadline: Instant,
) -> Result<(super::EncodedStoredDesign, String)> {
    let latest = baselines.latest.as_ref().unwrap_or(&baselines.root);
    authenticate(latest, &incoming.object_page_selections, deadline)?;
    let mut cumulative = repeat::merge_explicit_resource_changes(
        &baselines.root,
        &baselines.cumulative,
        incoming,
        policy,
        deadline,
    )?;
    let original = preselection(&baselines.root, &cumulative, policy, deadline)?;
    fold(
        &original,
        &mut cumulative,
        &incoming.object_page_selections,
        deadline,
    )?;
    repeat::check_operand_budget(&cumulative, policy.document.validation.service)?;
    baselines.cumulative = cumulative;
    finish_encode(&baselines, incoming, revision, policy, deadline)
}

/// A later announcement on the newly selected page belongs to that final role's
/// mask, not to the immutable preselection announcement history.
pub(super) fn fold_announcement(
    cumulative: &mut TeletextEditorialChanges,
    saved: ersatzrs_app::teletext_design_documents::TeletextSavedObjectSubpageAnnouncement,
) -> Result<bool> {
    let announcement = saved.announcement();
    let resource = announcement.resource();
    let role = match resource.kind() {
        ersatzrs_domain::TeletextResourceKind::Pop => ersatzrs_domain::TeletextX27LinkRole::Pop,
        ersatzrs_domain::TeletextResourceKind::Gpop => ersatzrs_domain::TeletextX27LinkRole::Gpop,
        _ => return Err(fail()),
    };
    let Some(prior) = cumulative.object_page_selections.iter_mut().find(|prior| {
        prior.change().display() == announcement.display() && prior.change().role() == role
    }) else {
        return Ok(false);
    };
    let change = prior.change();
    if change.replacement().target()
        != (TeletextX27LinkTarget::Page {
            page: resource.page(),
        })
    {
        return Ok(false);
    }
    let bit = 1_u16 << resource.logical_subpage().get();
    if prior.document_index() != saved.document_index()
        || change.replacement().required_subpages().get() & bit != 0
    {
        return Err(fail());
    }
    let replacement = ersatzrs_domain::TeletextX27LinkDeclaration::new(
        change.replacement().target(),
        change.replacement().validity(),
        ersatzrs_domain::TeletextSubpageRequirementMask::new(
            change.replacement().required_subpages().get() | bit,
        ),
    )
    .map_err(|_| fail())?;
    let updated = rooted_selection(
        change.display(),
        role,
        change.expected_links(),
        change.expected_role(),
        replacement,
    )
    .map_err(|_| fail())?;
    *prior = TeletextSavedObjectPageSelectionChange::new(prior.document_index(), updated)?;
    Ok(true)
}

fn rooted_selection(
    display: ersatzrs_domain::TeletextPageAddress,
    role: ersatzrs_domain::TeletextX27LinkRole,
    links: Option<ersatzrs_domain::TeletextX27Level25Links>,
    expected: ersatzrs_domain::TeletextMotRoleResolution,
    replacement: ersatzrs_domain::TeletextX27LinkDeclaration,
) -> std::result::Result<
    TeletextObjectPageSelectionChange,
    ersatzrs_domain::TeletextObjectPageSelectionError,
> {
    if expected == ersatzrs_domain::TeletextMotRoleResolution::Unselected {
        TeletextObjectPageSelectionChange::new_attachment(display, role, links, replacement)
    } else {
        TeletextObjectPageSelectionChange::new(display, role, links, expected, replacement)
    }
}

/// Exact receipt-owned attachment roles to restore to their immutable unselected roots.
pub struct NativeTeletextPageAttachmentsRestoreRequest {
    pub name: String,
    pub receipt_id: EntityId,
    pub receipt_digest: String,
    pub expected_design_head: i64,
    pub roles: Vec<(
        ersatzrs_domain::TeletextPageAddress,
        ersatzrs_domain::TeletextX27LinkRole,
    )>,
}
impl fmt::Debug for NativeTeletextPageAttachmentsRestoreRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NativeTeletextPageAttachmentsRestoreRequest")
            .field("receipt_id", &self.receipt_id)
            .field("expected_design_head", &self.expected_design_head)
            .field("roles", &self.roles.len())
            .finish_non_exhaustive()
    }
}
/// Restore selected attachment roles as one unpublished concept; publication is separate.
/// # Errors
/// Rejects stale/foreign authority, nonattachment roles, incomplete final dependencies,
/// exceeded bounds and protected changes without saving or altering live output.
pub async fn restore_teletext_editorial_page_attachments(
    application: &ApplicationServices,
    request: NativeTeletextPageAttachmentsRestoreRequest,
    policy: NativeTeletextExportPolicy,
    deadline: Instant,
) -> Result<TeletextDesignMutation<SavedEditorialTeletextDesign>> {
    apply(
        application,
        Request {
            name: request.name,
            receipt_id: request.receipt_id,
            receipt_digest: request.receipt_digest,
            expected_design_head: request.expected_design_head,
            action: Action::Restorations(request.roles),
        },
        policy,
        deadline,
    )
    .await
}
pub(super) fn preflight_restorations(
    roles: &[(
        ersatzrs_domain::TeletextPageAddress,
        ersatzrs_domain::TeletextX27LinkRole,
    )],
    budget: TeletextServiceBudget,
    deadline: Instant,
) -> Result<()> {
    check_deadline(deadline)?;
    if roles.is_empty() {
        return Err(NativeTeletextEditorialImportError::NoChanges);
    }
    if roles.len() > budget.max_records.min(budget.max_structure_items) {
        return Err(NativeTeletextEditorialImportError::Budget);
    }
    let mut seen = std::collections::BTreeSet::new();
    for &(display, role) in roles {
        check_deadline(deadline)?;
        if !seen.insert((display, role.wire_code())) {
            return Err(fail());
        }
    }
    Ok(())
}
pub(super) fn restore(
    baselines: Baselines,
    roles: Vec<(
        ersatzrs_domain::TeletextPageAddress,
        ersatzrs_domain::TeletextX27LinkRole,
    )>,
    revision: i64,
    policy: &NativeTeletextExportPolicy,
    deadline: Instant,
) -> Result<(super::EncodedStoredDesign, String)> {
    preflight_restorations(&roles, policy.document.validation.service, deadline)?;
    let latest = declared(baselines.latest.as_ref().ok_or_else(fail)?)?;
    let mut incoming = Vec::new();
    for (display, role) in roles {
        check_deadline(deadline)?;
        let saved = baselines
            .cumulative
            .object_page_selections
            .iter()
            .find(|saved| saved.change().display() == display && saved.change().role() == role)
            .ok_or_else(fail)?;
        let metadata = latest
            .semantic()
            .page_enhancements()
            .get(&display)
            .ok_or_else(fail)?;
        let resolution = latest
            .resolutions()
            .iter()
            .find(|r| r.display() == display)
            .ok_or_else(fail)?;
        let intent = TeletextObjectPageSelectionChange::new_unselected_restoration(
            saved.change(),
            metadata.compositional_links,
            *resolution.role(role),
        )
        .map_err(|_| fail())?;
        incoming.push(
            TeletextSavedObjectPageSelectionChange::for_unselected_restoration(
                saved.document_index(),
                intent,
            )?,
        );
    }
    encode(baselines, incoming, vec![], revision, policy, deadline)
}
