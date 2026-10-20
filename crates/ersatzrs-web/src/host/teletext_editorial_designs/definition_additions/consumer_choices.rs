//! Small Main/Local choices resolve only from qualified current native state.
use super::super::{consumer_selections, TeletextEditorialChanges};
use super::{
    apply, check_deadline, fmt, invalid, preflight, resolve_object_calls, resolve_pages, Action,
    ApplicationServices, BTreeSet, EntityId, Instant, NativeTeletextEditorialImportError,
    NativeTeletextExportPolicy, QualifiedBaseline, Request, Result, SavedEditorialTeletextDesign,
    TeletextDesignMutation, TeletextEnhancementCommand, TeletextMainResourceInvocationOverlay,
    TeletextObjectCallChoice, TeletextObjectIdentity, TeletextObjectPageChoice,
    TeletextPageAddress, TeletextServiceBudget,
};

/// An explicit current Main command and chosen resource identity, never supplied old state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextMainCallChoice {
    pub owner: TeletextPageAddress,
    pub command_index: usize,
    pub target: TeletextObjectIdentity,
}

/// Additive choice request retains existing resource-only request compatibility.
pub struct NativeTeletextObjectConsumersChooseRequest {
    pub name: String,
    pub receipt_id: EntityId,
    pub receipt_digest: String,
    pub expected_design_head: i64,
    pub selections: Vec<TeletextObjectPageChoice>,
    pub calls: Vec<TeletextObjectCallChoice>,
    pub main_calls: Vec<TeletextMainCallChoice>,
}
impl fmt::Debug for NativeTeletextObjectConsumersChooseRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NativeTeletextObjectConsumersChooseRequest")
            .field("receipt_id", &self.receipt_id)
            .field("expected_design_head", &self.expected_design_head)
            .field("selections", &self.selections.len())
            .field("calls", &self.calls.len())
            .field("main_calls", &self.main_calls.len())
            .finish_non_exhaustive()
    }
}

/// Resolve all explicit consumers in the existing held worker and save one concept.
/// # Errors
/// Rejects invalid/stale authority, repeated or mistyped calls, missing original
/// consumers, incomplete final graphs, protected effects and trusted bounds atomically.
pub async fn choose_teletext_editorial_object_consumers(
    application: &ApplicationServices,
    request: NativeTeletextObjectConsumersChooseRequest,
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
            action: Action::ConsumerChoices {
                selections: request.selections,
                calls: request.calls,
                main_calls: request.main_calls,
            },
        },
        policy,
        deadline,
    )
    .await
}

pub(in super::super) fn preflight_consumers(
    selections: &[TeletextObjectPageChoice],
    calls: &[TeletextObjectCallChoice],
    main_calls: &[TeletextMainCallChoice],
    budget: TeletextServiceBudget,
    deadline: Instant,
) -> Result<()> {
    check_deadline(deadline)?;
    if selections
        .len()
        .checked_add(calls.len())
        .and_then(|count| count.checked_add(main_calls.len()))
        .is_none_or(|count| count > budget.max_structure_items)
        || main_calls.len() > budget.max_records.min(budget.max_commands / 2)
    {
        return Err(NativeTeletextEditorialImportError::Budget);
    }
    if !selections.is_empty() || !calls.is_empty() {
        preflight(selections, calls, budget, deadline)?;
    } else if main_calls.is_empty() {
        return Err(NativeTeletextEditorialImportError::NoChanges);
    }
    let mut seen = BTreeSet::new();
    for choice in main_calls {
        check_deadline(deadline)?;
        if choice.target.resource().is_none() || !seen.insert((choice.owner, choice.command_index))
        {
            return Err(invalid());
        }
    }
    Ok(())
}

pub(in super::super) fn resolve_consumers(
    latest: &QualifiedBaseline,
    choices: &[TeletextObjectPageChoice],
    calls: &[TeletextObjectCallChoice],
    main_calls: &[TeletextMainCallChoice],
    budget: TeletextServiceBudget,
    deadline: Instant,
) -> Result<TeletextEditorialChanges> {
    preflight_consumers(choices, calls, main_calls, budget, deadline)?;
    let service = latest
        .original
        .view()
        .encoded()
        .ok_or_else(invalid)?
        .declared();
    let mut incoming = TeletextEditorialChanges {
        object_page_selections: resolve_pages(latest, choices, deadline)?,
        ..Default::default()
    };
    for choice in main_calls {
        check_deadline(deadline)?;
        let expected = *service
            .semantic()
            .page_enhancements()
            .get(&choice.owner)
            .and_then(|page| page.local_program.as_ref())
            .and_then(|program| program.commands().get(choice.command_index))
            .ok_or_else(invalid)?;
        let TeletextEnhancementCommand::InvokeObject {
            target,
            object_type,
            offset,
        } = expected
        else {
            return Err(invalid());
        };
        if target.resource().is_none()
            || service
                .semantic()
                .objects()
                .definition(choice.target)
                .is_none_or(|definition| definition.object_type != object_type)
        {
            return Err(invalid());
        }
        if target == choice.target {
            continue;
        }
        incoming.main_resource_invocations.push(
            TeletextMainResourceInvocationOverlay::new(
                choice.owner,
                choice.command_index,
                expected,
                TeletextEnhancementCommand::InvokeObject {
                    target: choice.target,
                    object_type,
                    offset,
                },
            )
            .map_err(|_| invalid())?,
        );
    }
    let mut object_budget = budget;
    object_budget.max_commands = budget
        .max_commands
        .checked_sub(
            incoming
                .main_resource_invocations
                .len()
                .checked_mul(2)
                .ok_or(NativeTeletextEditorialImportError::Budget)?,
        )
        .ok_or(NativeTeletextEditorialImportError::Budget)?;
    object_budget.max_structure_items = budget
        .max_structure_items
        .checked_sub(incoming.main_resource_invocations.len())
        .and_then(|count| count.checked_sub(incoming.object_page_selections.len()))
        .ok_or(NativeTeletextEditorialImportError::Budget)?;
    (incoming.object_programs, incoming.resource_programs) =
        resolve_object_calls(service.semantic().objects(), calls, object_budget, deadline)?;
    consumer_selections::preflight(&incoming, budget, deadline)?;
    Ok(incoming)
}
