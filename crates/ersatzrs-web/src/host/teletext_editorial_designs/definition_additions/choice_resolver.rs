//! Resolve small explicit choices only from the held worker's qualified original.
use super::{
    apply, check_deadline, fmt, page_selections, Action, ApplicationServices, EntityId, Instant,
    NativeTeletextEditorialImportError, NativeTeletextExportPolicy, QualifiedBaseline, Request,
    Result, SavedEditorialTeletextDesign, TeletextDesignMutation, TeletextServiceBudget,
};
use ersatzrs_app::teletext_design_documents::TeletextSavedObjectPageSelectionChange;
use ersatzrs_domain::{
    TeletextEnhancementCommand, TeletextEnhancementObjectCatalog, TeletextEnhancementProgram,
    TeletextLocalObjectGlyphProgramOverlay, TeletextMainResourceInvocationOverlay,
    TeletextObjectIdentity, TeletextObjectPageSelectionChange, TeletextPageAddress,
    TeletextRawDataPageNumber, TeletextResourceObjectProgramOverlay,
    TeletextSubpageRequirementMask, TeletextX27LinkDeclaration, TeletextX27LinkRole,
    TeletextX27LinkTarget,
};
use std::collections::BTreeSet;

/// Chosen page and exact S1 requirements; original links and provenance are server-owned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextObjectPageChoice {
    pub display: TeletextPageAddress,
    pub role: TeletextX27LinkRole,
    pub target: TeletextRawDataPageNumber,
    pub required_subpages: TeletextSubpageRequirementMask,
}

/// One existing resource call's explicitly chosen target, preserving its type and offset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextObjectCallChoice {
    pub caller: TeletextObjectIdentity,
    pub command_index: usize,
    pub target: TeletextObjectIdentity,
}

/// Current-receipt choices contain no supplied program, old state or edit permissions.
pub struct NativeTeletextObjectPagesChooseRequest {
    pub name: String,
    pub receipt_id: EntityId,
    pub receipt_digest: String,
    pub expected_design_head: i64,
    pub selections: Vec<TeletextObjectPageChoice>,
    pub calls: Vec<TeletextObjectCallChoice>,
}
impl fmt::Debug for NativeTeletextObjectPagesChooseRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NativeTeletextObjectPagesChooseRequest")
            .field("receipt_id", &self.receipt_id)
            .field("expected_design_head", &self.expected_design_head)
            .field("selections", &self.selections.len())
            .field("calls", &self.calls.len())
            .finish_non_exhaustive()
    }
}

/// Resolve explicit choices and save one unpublished concept through the existing worker/CAS.
/// # Errors
/// Rejects stale or foreign authority, duplicate/invalid choices, unavailable or mistyped
/// calls, exceeded bounds, protected effects and incomplete final dependencies atomically.
pub async fn choose_teletext_editorial_object_pages(
    application: &ApplicationServices,
    request: NativeTeletextObjectPagesChooseRequest,
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
            action: Action::Choices {
                selections: request.selections,
                calls: request.calls,
            },
        },
        policy,
        deadline,
    )
    .await
}

fn invalid() -> NativeTeletextEditorialImportError {
    NativeTeletextEditorialImportError::Association
}

pub(super) fn preflight(
    selections: &[TeletextObjectPageChoice],
    calls: &[TeletextObjectCallChoice],
    budget: TeletextServiceBudget,
    deadline: Instant,
) -> Result<()> {
    check_deadline(deadline)?;
    if selections.is_empty() && calls.is_empty() {
        return Err(NativeTeletextEditorialImportError::NoChanges);
    }
    if selections.len() > budget.max_records
        || calls.len() > budget.max_commands
        || selections
            .len()
            .checked_add(calls.len())
            .is_none_or(|count| count > budget.max_structure_items)
    {
        return Err(NativeTeletextEditorialImportError::Budget);
    }
    // Bound the raw input before creating any index, even for unchanged rows.
    let mut roles = BTreeSet::new();
    for choice in selections {
        check_deadline(deadline)?;
        if !roles.insert((choice.display, choice.role.wire_code())) {
            return Err(invalid());
        }
    }
    let mut commands = BTreeSet::new();
    let mut callers = BTreeSet::new();
    for choice in calls {
        check_deadline(deadline)?;
        if choice.target.resource().is_none()
            || !commands.insert((choice.caller, choice.command_index))
        {
            return Err(invalid());
        }
        callers.insert(choice.caller);
        if callers.len() > budget.max_objects {
            return Err(NativeTeletextEditorialImportError::Budget);
        }
    }
    Ok(())
}

pub(super) fn resolve(
    latest: &QualifiedBaseline,
    choices: &[TeletextObjectPageChoice],
    calls: &[TeletextObjectCallChoice],
    budget: TeletextServiceBudget,
    deadline: Instant,
) -> Result<(
    Vec<TeletextSavedObjectPageSelectionChange>,
    Vec<TeletextResourceObjectProgramOverlay>,
)> {
    preflight(choices, calls, budget, deadline)?;
    let selections = resolve_pages(latest, choices, deadline)?;
    let service = latest
        .original
        .view()
        .encoded()
        .ok_or_else(invalid)?
        .declared();
    let programs = resolve_calls(service.semantic().objects(), calls, budget, deadline)?;
    page_selections::preflight(&selections, &programs, budget, deadline)?;
    Ok((selections, programs))
}

fn resolve_pages(
    latest: &QualifiedBaseline,
    choices: &[TeletextObjectPageChoice],
    deadline: Instant,
) -> Result<Vec<TeletextSavedObjectPageSelectionChange>> {
    let service = latest
        .original
        .view()
        .encoded()
        .ok_or_else(invalid)?
        .declared();
    let mut selections = Vec::new();
    for choice in choices {
        check_deadline(deadline)?;
        let metadata = service
            .semantic()
            .page_enhancements()
            .get(&choice.display)
            .ok_or_else(invalid)?;
        let resolution = service
            .resolutions()
            .iter()
            .find(|resolution| resolution.display() == choice.display)
            .ok_or_else(invalid)?;
        let links = metadata.compositional_links.map_or_else(
            || {
                resolution
                    .materialized_level25_links()
                    .map_err(|_| invalid())
            },
            Ok,
        )?;
        let original = links.link(choice.role);
        let replacement = TeletextX27LinkDeclaration::new(
            TeletextX27LinkTarget::Page {
                page: choice.target,
            },
            original.validity(),
            choice.required_subpages,
        )
        .map_err(|_| invalid())?;
        if replacement == original {
            if !original.validity().applies_at_level25()
                || !matches!(resolution.role(choice.role),
                    ersatzrs_domain::TeletextMotRoleResolution::Selected { target, .. }
                    if *target == choice.target)
            {
                return Err(invalid());
            }
            continue;
        }
        let change = TeletextObjectPageSelectionChange::new(
            choice.display,
            choice.role,
            metadata.compositional_links,
            *resolution.role(choice.role),
            replacement,
        )
        .map_err(|_| invalid())?;
        let mapping = latest
            .body
            .project
            .pages
            .iter()
            .find(|mapping| {
                mapping.page.page == choice.display.page.get()
                    && mapping.page.subcode == choice.display.subcode.get()
            })
            .ok_or_else(invalid)?;
        selections.push(TeletextSavedObjectPageSelectionChange::new(
            mapping.producer_ordinal,
            change,
        )?);
    }
    Ok(selections)
}

fn resolve_calls(
    catalog: &TeletextEnhancementObjectCatalog,
    calls: &[TeletextObjectCallChoice],
    budget: TeletextServiceBudget,
    deadline: Instant,
) -> Result<Vec<TeletextResourceObjectProgramOverlay>> {
    let (local, resources) = resolve_object_calls(catalog, calls, budget, deadline)?;
    if !local.is_empty() {
        return Err(invalid());
    }
    Ok(resources)
}

fn resolve_object_calls(
    catalog: &TeletextEnhancementObjectCatalog,
    calls: &[TeletextObjectCallChoice],
    budget: TeletextServiceBudget,
    deadline: Instant,
) -> Result<(
    Vec<TeletextLocalObjectGlyphProgramOverlay>,
    Vec<TeletextResourceObjectProgramOverlay>,
)> {
    let mut changed = BTreeSet::new();
    // Authenticate and charge every complete body and correspondence before cloning any.
    let mut commands = budget.max_commands;
    let mut items = budget.max_structure_items;
    for choice in calls {
        check_deadline(deadline)?;
        let definition = catalog.definition(choice.caller).ok_or_else(invalid)?;
        let Some(TeletextEnhancementCommand::InvokeObject {
            target,
            object_type,
            ..
        }) = definition.program.commands().get(choice.command_index)
        else {
            return Err(invalid());
        };
        if catalog
            .definition(choice.target)
            .is_none_or(|target| target.object_type != *object_type)
        {
            return Err(invalid());
        }
        if *target != choice.target && changed.insert(choice.caller) {
            let count = definition.program.commands().len();
            commands = count
                .checked_mul(2)
                .and_then(|count| commands.checked_sub(count))
                .ok_or(NativeTeletextEditorialImportError::Budget)?;
            items = items
                .checked_sub(count)
                .ok_or(NativeTeletextEditorialImportError::Budget)?;
        }
    }
    let mut programs = Vec::with_capacity(changed.len());
    let mut local = Vec::new();
    for caller in changed {
        check_deadline(deadline)?;
        let original = &catalog.definition(caller).ok_or_else(invalid)?.program;
        let mut replacement = original.commands().to_vec();
        for choice in calls.iter().filter(|choice| choice.caller == caller) {
            check_deadline(deadline)?;
            let Some(TeletextEnhancementCommand::InvokeObject { target, .. }) =
                replacement.get_mut(choice.command_index)
            else {
                return Err(invalid());
            };
            *target = choice.target;
        }
        let replacement = TeletextEnhancementProgram::new(replacement).map_err(|_| invalid())?;
        let correspondence = (0..original.commands().len()).map(Some).collect();
        if caller.resource().is_some() {
            programs.push(
                TeletextResourceObjectProgramOverlay::new_with_invocation_changes(
                    caller,
                    original.clone(),
                    replacement,
                    correspondence,
                    budget,
                )
                .map_err(|_| invalid())?,
            );
        } else {
            local.push(
                TeletextLocalObjectGlyphProgramOverlay::new_with_invocation_changes(
                    caller,
                    original.clone(),
                    replacement,
                    correspondence,
                )
                .map_err(|_| invalid())?,
            );
        }
    }
    check_deadline(deadline)?;
    Ok((local, programs))
}

#[path = "consumer_choices.rs"]
mod consumer_choices;
pub use consumer_choices::{
    choose_teletext_editorial_object_consumers, NativeTeletextObjectConsumersChooseRequest,
    TeletextMainCallChoice,
};
pub(super) use consumer_choices::{preflight_consumers, resolve_consumers};

#[cfg(test)]
#[path = "choice_resolver_tests.rs"]
mod tests;
