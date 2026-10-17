//! Exact-original Local glyph program replacement through complete service gates.

use std::collections::BTreeMap;

use super::{
    charge, TeletextMagazinePresentationDraft, TeletextObjectIdentity,
    TeletextResolvedServiceDraft, TeletextSemanticService, TeletextServiceBudget,
    TeletextServiceError,
};
use crate::teletext::{
    TeletextLocalObjectGlyphProgramFailure, TeletextLocalObjectGlyphProgramOverlay,
};

impl TeletextSemanticService {
    /// Apply bounded glyph-only structure changes to exact existing Local programs.
    ///
    /// Every original must match exactly. Object identities/types, declarations,
    /// all other programs, resources, defaults and physical overlays are retained.
    /// Raw inputs, existing service and final authored command counts are checked
    /// before cloning; reconstruction reruns transitive execution/dependency gates.
    /// Hosts must authenticate compiler correspondence and qualify all shared
    /// consumer effects. This grants neither edit permission nor publication.
    /// # Errors
    /// Rejects empty, repeated, absent or stale targets and budget/dependency failures.
    pub fn with_local_object_glyph_program_overlays(
        self,
        overlays: &[TeletextLocalObjectGlyphProgramOverlay],
        budget: TeletextServiceBudget,
    ) -> Result<Self, TeletextServiceError> {
        self.check_glyph_program_overlay_budgets(overlays, budget)?;
        if overlays.is_empty() {
            return Err(TeletextServiceError::EmptyLocalObjectGlyphProgramOverlays);
        }
        let indexed = self.validate_local_object_glyph_program_overlays(overlays)?;
        self.check_glyph_program_result_budget(&indexed, budget)?;
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
        .with_physical_base_overlays(self.physical_base_overlays.into_values().collect(), budget)
    }

    fn check_glyph_program_overlay_budgets(
        &self,
        overlays: &[TeletextLocalObjectGlyphProgramOverlay],
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

    fn validate_local_object_glyph_program_overlays<'a>(
        &self,
        overlays: &'a [TeletextLocalObjectGlyphProgramOverlay],
    ) -> Result<
        BTreeMap<TeletextObjectIdentity, &'a TeletextLocalObjectGlyphProgramOverlay>,
        TeletextServiceError,
    > {
        let mut indexed = BTreeMap::new();
        for overlay in overlays {
            let failure = |reason| TeletextServiceError::LocalObjectGlyphProgramOverlay {
                identity: overlay.identity(),
                reason,
            };
            if indexed.insert(overlay.identity(), overlay).is_some() {
                return Err(failure(TeletextLocalObjectGlyphProgramFailure::Duplicate));
            }
            let object = self
                .objects
                .definition(overlay.identity())
                .ok_or_else(|| failure(TeletextLocalObjectGlyphProgramFailure::MissingObject))?;
            if &object.program != overlay.expected() {
                return Err(failure(
                    TeletextLocalObjectGlyphProgramFailure::ProgramMismatch,
                ));
            }
        }
        Ok(indexed)
    }

    fn check_glyph_program_result_budget(
        &self,
        overlays: &BTreeMap<TeletextObjectIdentity, &TeletextLocalObjectGlyphProgramOverlay>,
        budget: TeletextServiceBudget,
    ) -> Result<(), TeletextServiceError> {
        let mut commands = budget.max_commands;
        for page in self.pages.values() {
            let local = page
                .local_program
                .as_ref()
                .map_or(0, |value| value.commands().len());
            charge(
                &mut commands,
                local,
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
