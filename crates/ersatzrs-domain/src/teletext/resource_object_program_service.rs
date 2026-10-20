//! Resource program replacement through the unchanged full semantic collector.
use super::{
    charge, TeletextMagazinePresentationDraft, TeletextObjectIdentity,
    TeletextResolvedServiceDraft, TeletextSemanticService, TeletextServiceBudget,
    TeletextServiceError,
};
use crate::teletext::{TeletextResourceObjectProgramFailure, TeletextResourceObjectProgramOverlay};
use std::collections::BTreeMap;

impl TeletextSemanticService {
    /// Replace exact existing Public/Global programs without changing declarations.
    ///
    /// Raw input, existing service and final authored command budgets precede
    /// catalogue cloning. Reconstruction checks all transitive calls and DRCS
    /// dependencies for every executing consumer; hosts must still authenticate
    /// original spans and protected effects before saving or publishing.
    /// # Errors
    /// Rejects empty, duplicate, missing or stale changes and service/collector limits.
    pub fn with_resource_object_program_overlays(
        self,
        overlays: &[TeletextResourceObjectProgramOverlay],
        budget: TeletextServiceBudget,
    ) -> Result<Self, TeletextServiceError> {
        self.check_resource_program_inputs(overlays, budget)?;
        if overlays.is_empty() {
            return Err(TeletextServiceError::EmptyResourceObjectProgramOverlays);
        }
        let indexed = self.index_resource_programs(overlays)?;
        self.check_resource_program_result(&indexed, budget)?;
        let mut objects = self.objects.definitions().to_vec();
        for object in &mut objects {
            if let Some(overlay) = indexed.get(&object.identity) {
                object.program = overlay.replacement().clone();
            }
        }
        let draft = TeletextResolvedServiceDraft {
            base: self.base,
            pages: self.pages.into_values().collect(),
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
            objects,
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

    pub(in crate::teletext) fn check_resource_program_inputs(
        &self,
        overlays: &[TeletextResourceObjectProgramOverlay],
        budget: TeletextServiceBudget,
    ) -> Result<(), TeletextServiceError> {
        let mut objects = budget.max_objects;
        charge(&mut objects, overlays.len(), "object", budget.max_objects)?;
        let mut commands = budget.max_commands;
        let mut items = budget.max_structure_items;
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
                &mut items,
                overlay.retained_commands().len(),
                "structure item",
                budget.max_structure_items,
            )?;
        }
        self.check_existing_service_budgets(budget)
    }

    pub(in crate::teletext) fn index_resource_programs<'a>(
        &self,
        overlays: &'a [TeletextResourceObjectProgramOverlay],
    ) -> Result<
        BTreeMap<TeletextObjectIdentity, &'a TeletextResourceObjectProgramOverlay>,
        TeletextServiceError,
    > {
        let mut indexed = BTreeMap::new();
        for overlay in overlays {
            let failure = |reason| TeletextServiceError::ResourceObjectProgramOverlay {
                identity: overlay.identity(),
                reason,
            };
            if indexed.insert(overlay.identity(), overlay).is_some() {
                return Err(failure(TeletextResourceObjectProgramFailure::Duplicate));
            }
            let object = self
                .objects
                .definition(overlay.identity())
                .ok_or_else(|| failure(TeletextResourceObjectProgramFailure::MissingObject))?;
            if &object.program != overlay.expected() {
                return Err(failure(
                    TeletextResourceObjectProgramFailure::ProgramMismatch,
                ));
            }
        }
        Ok(indexed)
    }

    pub(in crate::teletext) fn check_resource_program_result(
        &self,
        overlays: &BTreeMap<TeletextObjectIdentity, &TeletextResourceObjectProgramOverlay>,
        budget: TeletextServiceBudget,
    ) -> Result<(), TeletextServiceError> {
        let mut commands = budget.max_commands;
        charge(
            &mut commands,
            self.base_text_spellings.len(),
            "authored command",
            budget.max_commands,
        )?;
        for page in self.pages.values() {
            charge(
                &mut commands,
                page.local_program
                    .as_ref()
                    .map_or(0, |program| program.commands().len()),
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
            let program = overlays
                .get(&object.identity)
                .map_or(&object.program, |overlay| overlay.replacement());
            charge(
                &mut commands,
                program.commands().len(),
                "authored command",
                budget.max_commands,
            )?;
        }
        Ok(())
    }
}
