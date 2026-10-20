//! Presence transitions revalidate complete execution, dependencies and defaults.
use super::{
    charge, TeletextMagazinePresentationDraft, TeletextResolvedServiceDraft,
    TeletextSemanticService, TeletextServiceBudget, TeletextServiceError,
};
use crate::teletext::{
    TeletextMainProgramPresenceFailure as Failure, TeletextMainProgramPresenceOverlay,
};
use std::collections::BTreeMap;

fn check_raw_presence(
    overlays: &[TeletextMainProgramPresenceOverlay],
    budget: TeletextServiceBudget,
) -> Result<(), TeletextServiceError> {
    if overlays.is_empty() {
        return Err(TeletextServiceError::EmptyMainProgramPresenceOverlays);
    }
    let mut records = budget.max_records;
    charge(&mut records, overlays.len(), "record", budget.max_records)?;
    let mut commands = budget.max_commands;
    let mut items = budget.max_structure_items;
    charge(
        &mut items,
        overlays.len(),
        "structure item",
        budget.max_structure_items,
    )?;
    for overlay in overlays {
        for program in [overlay.expected(), overlay.replacement()]
            .into_iter()
            .flatten()
        {
            charge(
                &mut commands,
                program.commands().len(),
                "authored command",
                budget.max_commands,
            )?;
            charge(
                &mut items,
                program.commands().len(),
                "structure item",
                budget.max_structure_items,
            )?;
        }
    }
    Ok(())
}

impl TeletextSemanticService {
    /// Change explicit Main presence without changing catalogues, resources or Base.
    /// Removing Some(empty) reactivates the already declared MOT/default fallback.
    /// # Errors
    /// Rejects empty/duplicate/stale owners, all raw/existing/final budgets, missing
    /// or wrong-owner targets and invalid complete transitive execution atomically.
    pub fn with_main_program_presence_overlays(
        self,
        overlays: &[TeletextMainProgramPresenceOverlay],
        budget: TeletextServiceBudget,
    ) -> Result<Self, TeletextServiceError> {
        check_raw_presence(overlays, budget)?;
        self.check_existing_service_budgets(budget)?;
        let indexed = self.index_main_presence(overlays)?;
        let mut final_commands = budget.max_commands;
        for page in self.pages.values() {
            let program = indexed
                .get(&page.address)
                .map_or(page.local_program.as_ref(), |overlay| overlay.replacement());
            charge(
                &mut final_commands,
                program.map_or(0, |program| program.commands().len()),
                "authored command",
                budget.max_commands,
            )?;
            charge(
                &mut final_commands,
                page.resources.defaults.iter().flatten().count(),
                "authored command",
                budget.max_commands,
            )?;
        }
        for object in self.objects.definitions() {
            charge(
                &mut final_commands,
                object.program.commands().len(),
                "authored command",
                budget.max_commands,
            )?;
        }
        let mut pages: Vec<_> = self.pages.into_values().collect();
        for page in &mut pages {
            if let Some(overlay) = indexed.get(&page.address) {
                page.local_program = overlay.replacement().cloned();
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
        .with_physical_base_overlays(self.physical_base_overlays.into_values().collect(), budget)?
        .with_base_text_spellings(self.base_text_spellings.into_values().collect(), budget)
    }
    fn index_main_presence<'a>(
        &self,
        overlays: &'a [TeletextMainProgramPresenceOverlay],
    ) -> Result<
        BTreeMap<crate::teletext::TeletextPageAddress, &'a TeletextMainProgramPresenceOverlay>,
        TeletextServiceError,
    > {
        let mut indexed = BTreeMap::new();
        for overlay in overlays {
            let failure = |reason| TeletextServiceError::MainProgramPresenceOverlay {
                owner: overlay.owner(),
                reason,
            };
            if indexed.insert(overlay.owner(), overlay).is_some() {
                return Err(failure(Failure::Duplicate));
            }
            let original = self
                .pages
                .get(&overlay.owner())
                .ok_or_else(|| failure(Failure::MissingOwner))?;
            if original.local_program.as_ref() != overlay.expected() {
                return Err(failure(Failure::OriginalMismatch));
            }
            for command in overlay
                .replacement()
                .into_iter()
                .flat_map(crate::teletext::TeletextEnhancementProgram::commands)
            {
                if let super::TeletextEnhancementCommand::InvokeObject {
                    target,
                    object_type,
                    ..
                } = command
                {
                    if target.owner().is_some_and(|owner| owner != overlay.owner())
                        || self
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
}
