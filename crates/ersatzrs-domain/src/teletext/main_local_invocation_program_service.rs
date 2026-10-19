//! Main replacements reuse complete semantic service reconstruction.
use super::{
    charge, TeletextEnhancementCommand as Command, TeletextMagazinePresentationDraft,
    TeletextResolvedServiceDraft, TeletextSemanticService, TeletextServiceBudget,
    TeletextServiceError,
};
use crate::teletext::{
    TeletextMainLocalInvocationProgramFailure as Failure, TeletextMainLocalInvocationProgramOverlay,
};
use std::collections::BTreeMap;
impl TeletextSemanticService {
    /// Apply exact explicit Main replacements, preserving catalogues and resources.
    /// Deleting every call retains Some(empty), never enabling default invocations.
    /// # Errors
    /// Rejects empty/duplicate/stale owners, absent explicit Main programs, unknown
    /// Local targets/types and raw/final budgets. Full reconstruction rechecks all
    /// transitive execution/dependency invariants; hosts still qualify permissions.
    pub fn with_main_local_invocation_program_overlays(
        self,
        overlays: &[TeletextMainLocalInvocationProgramOverlay],
        budget: TeletextServiceBudget,
    ) -> Result<Self, TeletextServiceError> {
        if overlays.is_empty() {
            return Err(TeletextServiceError::EmptyMainLocalInvocationProgramOverlays);
        }
        let mut records = budget.max_records;
        charge(&mut records, overlays.len(), "record", budget.max_records)?;
        let mut commands = budget.max_commands;
        let mut structure = budget.max_structure_items;
        for overlay in overlays {
            for program in [overlay.expected(), overlay.replacement()] {
                charge(
                    &mut commands,
                    program.commands().len(),
                    "authored command",
                    budget.max_commands,
                )?;
            }
            charge(
                &mut structure,
                overlay.retained_commands().len(),
                "structure item",
                budget.max_structure_items,
            )?;
        }
        self.check_existing_service_budgets(budget)?;
        let indexed = self.index_main_invocation_overlays(overlays)?;
        self.check_main_invocation_result_budget(&indexed, budget)?;
        let mut pages: Vec<_> = self.pages.into_values().collect();
        for page in &mut pages {
            if let Some(overlay) = indexed.get(&page.address) {
                page.local_program = Some(overlay.replacement().clone());
            }
        }
        let draft = TeletextResolvedServiceDraft {
            base: self.base,
            pages,
            magazines: self
                .magazines
                .into_iter()
                .map(
                    |(magazine, presentation)| TeletextMagazinePresentationDraft {
                        magazine,
                        presentation,
                    },
                )
                .collect(),
            objects: self.objects.definitions().to_vec(),
            drcs: self.drcs.subtables().to_vec(),
        };
        Self::new_with_resolved_mot_fallbacks(
            draft,
            self.object_companions,
            self.mot_fallbacks,
            budget,
        )?
        .with_physical_base_overlays(self.physical_base_overlays.into_values().collect(), budget)
    }
    fn index_main_invocation_overlays<'a>(
        &self,
        overlays: &'a [TeletextMainLocalInvocationProgramOverlay],
    ) -> Result<
        BTreeMap<
            crate::teletext::TeletextPageAddress,
            &'a TeletextMainLocalInvocationProgramOverlay,
        >,
        TeletextServiceError,
    > {
        let mut indexed = BTreeMap::new();
        for overlay in overlays {
            let failure = |reason| TeletextServiceError::MainLocalInvocationProgramOverlay {
                owner: overlay.owner(),
                reason,
            };
            if indexed.insert(overlay.owner(), overlay).is_some() {
                return Err(failure(Failure::Duplicate));
            }
            let original = self
                .pages
                .get(&overlay.owner())
                .and_then(|page| page.local_program.as_ref())
                .ok_or_else(|| failure(Failure::MissingProgram))?;
            if original != overlay.expected() {
                return Err(failure(Failure::ProgramMismatch));
            }
            for command in overlay
                .expected()
                .commands()
                .iter()
                .chain(overlay.replacement().commands())
            {
                if let Command::InvokeObject {
                    target,
                    object_type,
                    ..
                } = command
                {
                    if target.owner() == Some(overlay.owner())
                        && self
                            .objects
                            .definition(*target)
                            .is_none_or(|definition| definition.object_type != *object_type)
                    {
                        return Err(failure(Failure::Target));
                    }
                }
            }
        }
        Ok(indexed)
    }
    fn check_main_invocation_result_budget(
        &self,
        overlays: &BTreeMap<
            crate::teletext::TeletextPageAddress,
            &TeletextMainLocalInvocationProgramOverlay,
        >,
        budget: TeletextServiceBudget,
    ) -> Result<(), TeletextServiceError> {
        let mut commands = budget.max_commands;
        for page in self.pages.values() {
            let count = overlays.get(&page.address).map_or_else(
                || {
                    page.local_program
                        .as_ref()
                        .map_or(0, |program| program.commands().len())
                },
                |overlay| overlay.replacement().commands().len(),
            );
            charge(
                &mut commands,
                count,
                "authored command",
                budget.max_commands,
            )?;
            charge(
                &mut commands,
                page.resources.defaults.iter().flatten().count(),
                "authored command",
                budget.max_commands,
            )?;
        }
        for object in self.objects.definitions() {
            charge(
                &mut commands,
                object.program.commands().len(),
                "authored command",
                budget.max_commands,
            )?;
        }
        Ok(())
    }
}
