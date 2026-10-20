//! Explicit complete consumer intent joins the existing retained native transaction.
use super::{
    apply, check_deadline, fmt, page_selections, repeat, Action, ApplicationServices, Baselines,
    EntityId, Instant, NativeTeletextEditorialImportError, NativeTeletextExportPolicy, Request,
    Result, SavedEditorialTeletextDesign, TeletextDesignMutation, TeletextEditorialChanges,
    TeletextServiceBudget,
};
use ersatzrs_app::teletext_design_documents::TeletextSavedObjectPageSelectionChange;
use ersatzrs_domain::{
    TeletextLocalObjectGlyphProgramOverlay, TeletextMainResourceInvocationOverlay,
    TeletextResourceObjectProgramOverlay,
};

/// Separate typed authority for coordinated Main, Local and resource consumers.
pub struct NativeTeletextObjectConsumersSelectRequest {
    pub name: String,
    pub receipt_id: EntityId,
    pub receipt_digest: String,
    pub expected_design_head: i64,
    pub selections: Vec<TeletextSavedObjectPageSelectionChange>,
    pub programs: Vec<TeletextResourceObjectProgramOverlay>,
    pub local_programs: Vec<TeletextLocalObjectGlyphProgramOverlay>,
    pub main_resource_invocations: Vec<TeletextMainResourceInvocationOverlay>,
}
impl fmt::Debug for NativeTeletextObjectConsumersSelectRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NativeTeletextObjectConsumersSelectRequest")
            .field("receipt_id", &self.receipt_id)
            .field("expected_design_head", &self.expected_design_head)
            .field("selections", &self.selections.len())
            .field("programs", &self.programs.len())
            .field("local_programs", &self.local_programs.len())
            .field(
                "main_resource_invocations",
                &self.main_resource_invocations.len(),
            )
            .finish_non_exhaustive()
    }
}

/// Save coordinated consumers as one unpublished, authenticated concept revision.
/// # Errors
/// Rejects stale/foreign receipt authority, invalid original consumers, incomplete
/// dependencies, protected effects, exceeded bounds and exact-head conflicts atomically.
pub async fn select_teletext_editorial_object_consumers(
    application: &ApplicationServices,
    request: NativeTeletextObjectConsumersSelectRequest,
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
            action: Action::Consumers(Box::new(TeletextEditorialChanges {
                object_page_selections: request.selections,
                resource_programs: request.programs,
                object_programs: request.local_programs,
                main_resource_invocations: request.main_resource_invocations,
                ..Default::default()
            })),
        },
        policy,
        deadline,
    )
    .await
}

pub(super) fn preflight(
    changes: &TeletextEditorialChanges,
    budget: TeletextServiceBudget,
    deadline: Instant,
) -> Result<()> {
    check_deadline(deadline)?;
    if changes.is_empty() {
        return Err(NativeTeletextEditorialImportError::NoChanges);
    }
    repeat::check_operand_budget(changes, budget)?;
    let mut commands = budget.max_commands;
    let mut objects = budget.max_objects;
    for program in &changes.object_programs {
        objects = objects
            .checked_sub(1)
            .ok_or(NativeTeletextEditorialImportError::Budget)?;
        for body in [program.expected(), program.replacement()] {
            check_deadline(deadline)?;
            commands = commands
                .checked_sub(body.commands().len())
                .ok_or(NativeTeletextEditorialImportError::Budget)?;
        }
    }
    for program in &changes.resource_programs {
        objects = objects
            .checked_sub(1)
            .ok_or(NativeTeletextEditorialImportError::Budget)?;
        for body in [program.expected(), program.replacement()] {
            check_deadline(deadline)?;
            commands = commands
                .checked_sub(body.commands().len())
                .ok_or(NativeTeletextEditorialImportError::Budget)?;
        }
    }
    for _ in &changes.main_resource_invocations {
        check_deadline(deadline)?;
        commands = commands
            .checked_sub(2)
            .ok_or(NativeTeletextEditorialImportError::Budget)?;
    }
    check_deadline(deadline)
}

pub(super) fn encode(
    baselines: Baselines,
    incoming: &TeletextEditorialChanges,
    revision: i64,
    policy: &NativeTeletextExportPolicy,
    deadline: Instant,
) -> Result<(super::EncodedStoredDesign, String)> {
    preflight(incoming, policy.document.validation.service, deadline)?;
    page_selections::encode_changes(baselines, incoming, revision, policy, deadline)
}
